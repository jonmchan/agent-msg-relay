use agent_msg_relay_protocol::{
    decode_hex, signing_bytes, valid_uuid, verify_prehash, Envelope, WRAP_LEN,
};
use spacetimedb::{ReducerContext, Table};

use crate::agents::{charge_send, require_agent, slide};
use crate::schema::{
    agent, channel, channel_member, delivery, dm_pair, message, stats, Delivery, KeyWrapArg,
    Message,
};
use crate::store::{add_dm_live, delete_message};
use crate::util::{
    bump_stats, dm_key, identity_string, limits, member_key, parse_identity, within_skew, Out,
};

#[spacetimedb::reducer]
#[allow(clippy::too_many_arguments)] // Reducer arguments are the public call shape.
pub fn post_message(
    ctx: &ReducerContext,
    message_id: String,
    timestamp_micros: i64,
    recipient_kind: String,
    recipient_id: String,
    body: String,
    encrypted: bool,
    signature_hex: String,
    wraps: Vec<KeyWrapArg>,
) -> Out {
    let mut agent = require_agent(ctx)?;
    if !valid_uuid(&message_id) {
        return Err("message id must be a UUID".into());
    }
    if ctx.db.message().message_id().find(&message_id).is_some() {
        return Err("message id is already used".into());
    }
    let recipient_id = if recipient_kind == "agent" {
        identity_string(parse_identity(&recipient_id)?)
    } else {
        recipient_id
    };
    if !within_skew(ctx.timestamp, timestamp_micros) {
        return Err("message timestamp is outside the 5 minute window".into());
    }
    let signature = decode_hex(&signature_hex)?;
    if signature.len() != 64 {
        return Err("signature must be 64-byte compact ECDSA".into());
    }
    let limits = limits(ctx);
    let stats = ctx
        .db
        .stats()
        .id()
        .find(crate::schema::STATS_ID)
        .ok_or("stats row is missing")?;

    let (recipients, byte_len) = if recipient_kind == "agent" {
        prepare_dm(ctx, &agent, &recipient_id, &body, encrypted, &wraps)?
    } else if recipient_kind == "channel" {
        prepare_channel(
            ctx,
            &agent,
            &recipient_id,
            &body,
            encrypted,
            &signature,
            &message_id,
            timestamp_micros,
            &wraps,
        )?
    } else {
        return Err("recipient kind must be agent or channel".into());
    };

    if body.len() as u64 > limits.body_max_bytes {
        return Err(format!(
            "limit body_max_bytes max {}",
            limits.body_max_bytes
        ));
    }
    if stats.live_message_bytes.saturating_add(byte_len) > limits.max_live_message_bytes {
        return Err(format!(
            "limit max_live_message_bytes max {}",
            limits.max_live_message_bytes
        ));
    }
    charge_send(ctx, &mut agent, &limits)?;

    let wraps_json = wraps_to_json(&wraps);
    ctx.db
        .message()
        .try_insert(Message {
            message_id: message_id.clone(),
            sender: ctx.sender(),
            recipient_kind,
            recipient_id,
            body,
            encrypted,
            signature_hex,
            created_micros: timestamp_micros,
            wraps_json,
            byte_len,
            pending_acks: recipients.len() as u32,
        })
        .map_err(|err| err.to_string())?;
    for recipient in &recipients {
        ctx.db
            .delivery()
            .try_insert(Delivery {
                key: format!("{message_id}|{}", identity_string(*recipient)),
                message_id: message_id.clone(),
                agent: *recipient,
                acked: false,
            })
            .map_err(|err| err.to_string())?;
    }
    bump_stats(ctx, 0, byte_len as i64)?;
    slide(ctx, agent);
    Ok(())
}

fn prepare_dm(
    ctx: &ReducerContext,
    _sender: &crate::schema::Agent,
    recipient_id: &str,
    body: &str,
    encrypted: bool,
    wraps: &[KeyWrapArg],
) -> Result<(Vec<spacetimedb::Identity>, u64), String> {
    if !encrypted {
        return Err("direct messages are encrypted".into());
    }
    let recipient = parse_identity(recipient_id)?;
    let recipient_agent = ctx
        .db
        .agent()
        .identity()
        .find(recipient)
        .ok_or("recipient agent is not registered")?;
    if wraps.len() != 1 {
        return Err("a direct message carries one key wrap".into());
    }
    let wrap_recipient = parse_identity(&wraps[0].recipient_hex)?;
    if wrap_recipient != recipient {
        return Err("direct message wrap is for a different recipient".into());
    }
    check_wrap(&wraps[0], &recipient_agent.public_key_hex)?;
    let limits = limits(ctx);
    let key = dm_key(ctx.sender(), recipient);
    let live = ctx
        .db
        .dm_pair()
        .key()
        .find(&key)
        .map(|pair| pair.live)
        .unwrap_or(0);
    if live >= limits.live_messages_per_dm_pair {
        return Err(format!(
            "limit live_messages_per_dm_pair max {}",
            limits.live_messages_per_dm_pair
        ));
    }
    if recipient_agent.live_dm_inbox >= limits.live_dm_messages_per_recipient {
        return Err(format!(
            "limit live_dm_messages_per_recipient max {}",
            limits.live_dm_messages_per_recipient
        ));
    }
    add_dm_live(ctx, key)?;
    let mut recipient_agent = recipient_agent;
    recipient_agent.live_dm_inbox += 1;
    ctx.db.agent().identity().update(recipient_agent);
    let cipher_len = decode_hex(body)?.len() as u64;
    let wrap_len: u64 = wraps
        .iter()
        .map(|wrap| decode_hex(&wrap.blob_hex).map(|b| b.len()).unwrap_or(0) as u64)
        .sum();
    Ok((vec![recipient], cipher_len + wrap_len))
}

#[allow(clippy::too_many_arguments)] // Channel post checks share the reducer arguments.
fn prepare_channel(
    ctx: &ReducerContext,
    sender: &crate::schema::Agent,
    channel_id: &str,
    body: &str,
    encrypted: bool,
    signature: &[u8],
    message_id: &str,
    timestamp_micros: i64,
    wraps: &[KeyWrapArg],
) -> Result<(Vec<spacetimedb::Identity>, u64), String> {
    let mut channel = ctx
        .db
        .channel()
        .channel_id()
        .find(channel_id.to_string())
        .ok_or("channel not found")?;
    if ctx
        .db
        .channel_member()
        .key()
        .find(member_key(channel_id, ctx.sender()))
        .is_none()
    {
        return Err("only a member can post to this channel".into());
    }
    let limits = limits(ctx);
    if channel.live_messages >= limits.live_messages_per_channel {
        return Err(format!(
            "limit live_messages_per_channel max {}",
            limits.live_messages_per_channel
        ));
    }
    let members: Vec<spacetimedb::Identity> = ctx
        .db
        .channel_member()
        .channel_id()
        .filter(channel_id)
        .map(|row| row.agent)
        .collect();
    let byte_len = if channel.kind == "public" {
        if encrypted || !wraps.is_empty() {
            return Err("public channel messages are plaintext".into());
        }
        serde_json::from_str::<Envelope>(body).map_err(|err| err.to_string())?;
        let payload = signing_bytes(
            &identity_string(ctx.sender()),
            "channel",
            channel_id,
            message_id,
            timestamp_micros,
            body,
        );
        let pubkey = decode_hex(&sender.public_key_hex)?;
        verify_prehash(&pubkey, &payload, signature)?;
        body.len() as u64
    } else {
        if !encrypted {
            return Err("private channel messages are encrypted".into());
        }
        if wraps.len() != members.len() {
            return Err("private channel messages wrap the key once per accepted member".into());
        }
        for member in &members {
            let member_agent = ctx
                .db
                .agent()
                .identity()
                .find(member)
                .ok_or("channel member is missing")?;
            let matched = wraps.iter().any(|wrap| {
                parse_identity(&wrap.recipient_hex)
                    .ok()
                    .is_some_and(|identity| identity == *member)
                    && check_wrap(wrap, &member_agent.public_key_hex).is_ok()
            });
            if !matched {
                return Err("each accepted member needs a key wrap".into());
            }
        }
        let cipher_len = decode_hex(body)?.len() as u64;
        let wrap_len: u64 = wraps
            .iter()
            .map(|wrap| {
                decode_hex(&wrap.blob_hex)
                    .map(|bytes| bytes.len())
                    .unwrap_or(0) as u64
            })
            .sum();
        cipher_len + wrap_len
    };
    channel.live_messages += 1;
    ctx.db.channel().channel_id().update(channel);
    Ok((members, byte_len))
}

fn wraps_to_json(wraps: &[KeyWrapArg]) -> String {
    let parts: Vec<String> = wraps
        .iter()
        .map(|wrap| {
            format!(
                "{{\"recipient_hex\":{},\"blob_hex\":{}}}",
                serde_json::to_string(&wrap.recipient_hex).unwrap_or_else(|_| "\"\"".into()),
                serde_json::to_string(&wrap.blob_hex).unwrap_or_else(|_| "\"\"".into())
            )
        })
        .collect();
    format!("[{}]", parts.join(","))
}

fn check_wrap(wrap: &KeyWrapArg, public_key_hex: &str) -> Result<(), String> {
    let recipient = parse_identity(&wrap.recipient_hex)?;
    let _ = recipient;
    let blob = decode_hex(&wrap.blob_hex)?;
    if blob.len() != WRAP_LEN {
        return Err(format!("key wrap must be {WRAP_LEN} bytes"));
    }
    if wrap.recipient_hex.is_empty() || public_key_hex.is_empty() {
        return Err("key wrap is missing a recipient".into());
    }
    Ok(())
}

#[spacetimedb::reducer]
pub fn ack_message(ctx: &ReducerContext, message_id: String) -> Out {
    let agent = require_agent(ctx)?;
    let key = format!("{message_id}|{}", identity_string(ctx.sender()));
    let Some(mut delivery) = ctx.db.delivery().key().find(&key) else {
        return Err("message is not waiting for this agent".into());
    };
    if delivery.acked {
        slide(ctx, agent);
        return Ok(());
    }
    delivery.acked = true;
    ctx.db.delivery().key().update(delivery);
    let Some(mut message) = ctx.db.message().message_id().find(&message_id) else {
        return Err("message is already gone".into());
    };
    message.pending_acks = message.pending_acks.saturating_sub(1);
    if message.pending_acks == 0 {
        delete_message(ctx, &message_id)?;
    } else {
        ctx.db.message().message_id().update(message);
    }
    slide(ctx, agent);
    Ok(())
}
