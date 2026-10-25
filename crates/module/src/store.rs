use spacetimedb::{ReducerContext, Table};

use crate::schema::{
    agent, agent_lookup, channel, channel_invite, channel_member, delivery, dm_pair, message,
    public_agent_name, public_channel_name, DmPair, Message,
};
use crate::util::{bump_stats, dm_key, identity_string, Out};

pub fn delete_message(ctx: &ReducerContext, message_id: &str) -> Out {
    let Some(message) = ctx.db.message().message_id().find(message_id.to_string()) else {
        return Ok(());
    };
    if message.recipient_kind == "channel" {
        if let Some(mut channel) = ctx.db.channel().channel_id().find(&message.recipient_id) {
            channel.live_messages = channel.live_messages.saturating_sub(1);
            ctx.db.channel().channel_id().update(channel);
        }
    } else if message.recipient_kind == "agent" {
        if let Ok(recipient) = crate::util::parse_identity(&message.recipient_id) {
            let key = dm_key(message.sender, recipient);
            if let Some(mut pair) = ctx.db.dm_pair().key().find(&key) {
                pair.live = pair.live.saturating_sub(1);
                if pair.live == 0 {
                    ctx.db.dm_pair().key().delete(&key);
                } else {
                    ctx.db.dm_pair().key().update(pair);
                }
            }
            if let Some(mut agent) = ctx.db.agent().identity().find(recipient) {
                agent.live_dm_inbox = agent.live_dm_inbox.saturating_sub(1);
                ctx.db.agent().identity().update(agent);
            }
        }
    }
    let delivery_keys: Vec<String> = ctx
        .db
        .delivery()
        .message_id()
        .filter(message_id)
        .map(|row| row.key)
        .collect();
    for key in delivery_keys {
        ctx.db.delivery().key().delete(&key);
    }
    ctx.db.message().message_id().delete(message_id.to_string());
    bump_stats(ctx, 0, -(message.byte_len as i64))
}

pub fn remove_channel(ctx: &ReducerContext, channel_id: &str) -> Out {
    let Some(channel) = ctx.db.channel().channel_id().find(channel_id.to_string()) else {
        return Ok(());
    };
    let message_ids: Vec<String> = ctx
        .db
        .message()
        .iter()
        .filter(|message| message.recipient_kind == "channel" && message.recipient_id == channel_id)
        .map(|message| message.message_id)
        .collect();
    for message_id in message_ids {
        delete_message(ctx, &message_id)?;
    }
    let member_keys: Vec<String> = ctx
        .db
        .channel_member()
        .channel_id()
        .filter(channel_id)
        .map(|row| row.key)
        .collect();
    for key in member_keys {
        ctx.db.channel_member().key().delete(&key);
    }
    let invite_keys: Vec<String> = ctx
        .db
        .channel_invite()
        .channel_id()
        .filter(channel_id)
        .map(|row| row.key)
        .collect();
    for key in invite_keys {
        ctx.db.channel_invite().key().delete(&key);
    }
    if channel.kind == "public" && !channel.name.is_empty() {
        ctx.db.public_channel_name().name().delete(&channel.name);
    }
    if let Some(mut owner) = ctx.db.agent().identity().find(channel.owner) {
        owner.channels_owned = owner.channels_owned.saturating_sub(1);
        ctx.db.agent().identity().update(owner);
    }
    ctx.db.channel().channel_id().delete(channel_id.to_string());
    Ok(())
}

pub fn drop_recipient(
    ctx: &ReducerContext,
    message: &Message,
    agent: spacetimedb::Identity,
) -> Out {
    let key = format!("{}|{}", message.message_id, identity_string(agent));
    let Some(delivery) = ctx.db.delivery().key().find(&key) else {
        return Ok(());
    };
    ctx.db.delivery().key().delete(&key);
    if delivery.acked {
        return Ok(());
    }
    let Some(mut stored) = ctx.db.message().message_id().find(&message.message_id) else {
        return Ok(());
    };
    stored.pending_acks = stored.pending_acks.saturating_sub(1);
    if stored.pending_acks == 0 {
        delete_message(ctx, &stored.message_id)
    } else {
        ctx.db.message().message_id().update(stored);
        Ok(())
    }
}

pub fn delete_agent(ctx: &ReducerContext, agent: spacetimedb::Identity) -> Out {
    let owned: Vec<String> = ctx
        .db
        .channel()
        .iter()
        .filter(|channel| channel.owner == agent)
        .map(|channel| channel.channel_id)
        .collect();
    for channel_id in owned {
        remove_channel(ctx, &channel_id)?;
    }

    let memberships: Vec<String> = ctx
        .db
        .channel_member()
        .agent()
        .filter(agent)
        .map(|row| row.channel_id)
        .collect();
    for channel_id in memberships {
        if let Some(mut channel) = ctx.db.channel().channel_id().find(&channel_id) {
            channel.member_count = channel.member_count.saturating_sub(1);
            ctx.db.channel().channel_id().update(channel);
        }
        ctx.db
            .channel_member()
            .key()
            .delete(crate::util::member_key(&channel_id, agent));
    }

    let invites: Vec<(String, String)> = ctx
        .db
        .channel_invite()
        .iter()
        .filter(|invite| invite.agent == agent)
        .map(|invite| (invite.channel_id.clone(), invite.key.clone()))
        .collect();
    for (channel_id, key) in invites {
        if let Some(mut channel) = ctx.db.channel().channel_id().find(&channel_id) {
            channel.invite_count = channel.invite_count.saturating_sub(1);
            ctx.db.channel().channel_id().update(channel);
        }
        ctx.db.channel_invite().key().delete(&key);
    }

    let incoming: Vec<String> = ctx
        .db
        .delivery()
        .agent()
        .filter(agent)
        .map(|row| row.message_id)
        .collect();
    for message_id in incoming {
        if let Some(message) = ctx.db.message().message_id().find(&message_id) {
            drop_recipient(ctx, &message, agent)?;
        }
    }

    let names: Vec<String> = ctx
        .db
        .public_agent_name()
        .iter()
        .filter(|row| row.agent == agent)
        .map(|row| row.name)
        .collect();
    for name in names {
        ctx.db.public_agent_name().name().delete(&name);
    }
    ctx.db.agent_lookup().caller().delete(agent);
    ctx.db.agent().identity().delete(agent);
    bump_stats(ctx, -1, 0)
}

pub fn add_dm_live(ctx: &ReducerContext, key: String) -> Result<u32, String> {
    if let Some(mut pair) = ctx.db.dm_pair().key().find(&key) {
        pair.live = pair.live.saturating_add(1);
        let live = pair.live;
        ctx.db.dm_pair().key().update(pair);
        Ok(live)
    } else {
        ctx.db.dm_pair().insert(DmPair { key, live: 1 });
        Ok(1)
    }
}
