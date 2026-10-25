use agent_msg_relay_protocol::valid_uuid;
use spacetimedb::{ReducerContext, Table};

use crate::agents::{reload_slide, require_agent, slide};
use crate::schema::{
    agent, channel, channel_invite, channel_member, message, public_channel_name, Channel,
    ChannelInvite, ChannelMember, PublicChannelName,
};
use crate::store::remove_channel;
use crate::util::{
    check_profile_len, days_micros, limits, member_key, now_micros, parse_identity, require_name,
    Out,
};

fn require_channel(ctx: &ReducerContext, channel_id: &str) -> Result<Channel, String> {
    ctx.db
        .channel()
        .channel_id()
        .find(channel_id.to_string())
        .ok_or_else(|| "channel not found".into())
}

fn is_member(ctx: &ReducerContext, channel_id: &str, agent: spacetimedb::Identity) -> bool {
    ctx.db
        .channel_member()
        .key()
        .find(member_key(channel_id, agent))
        .is_some()
}

#[spacetimedb::reducer]
pub fn create_channel(
    ctx: &ReducerContext,
    channel_id: String,
    kind: String,
    name: String,
    purpose: String,
    expects: String,
) -> Out {
    let mut agent = require_agent(ctx)?;
    if !valid_uuid(&channel_id) {
        return Err("channel id must be a UUID".into());
    }
    if ctx.db.channel().channel_id().find(&channel_id).is_some() {
        return Err("channel id is already used".into());
    }
    if kind != "public" && kind != "private" {
        return Err("channel kind must be public or private".into());
    }
    let limits = limits(ctx);
    check_profile_len(&limits, &purpose)?;
    check_profile_len(&limits, &expects)?;
    if agent.channels_owned >= limits.channels_owned_per_agent {
        return Err(format!(
            "limit channels_owned_per_agent max {}",
            limits.channels_owned_per_agent
        ));
    }
    if kind == "public" {
        require_name(&name)?;
        if purpose.is_empty() || expects.is_empty() {
            return Err("a public channel requires purpose and expects".into());
        }
        if ctx.db.public_channel_name().name().find(&name).is_some() {
            return Err("public channel name is already taken".into());
        }
    } else if !name.is_empty() {
        require_name(&name)?;
    }

    ctx.db
        .channel()
        .try_insert(Channel {
            channel_id: channel_id.clone(),
            kind: kind.clone(),
            name: name.clone(),
            purpose,
            expects,
            owner: ctx.sender,
            member_count: 1,
            invite_count: 0,
            live_messages: 0,
        })
        .map_err(|err| err.to_string())?;
    ctx.db
        .channel_member()
        .try_insert(ChannelMember {
            key: member_key(&channel_id, ctx.sender),
            channel_id: channel_id.clone(),
            agent: ctx.sender,
        })
        .map_err(|err| err.to_string())?;
    if kind == "public" {
        ctx.db
            .public_channel_name()
            .try_insert(PublicChannelName { name, channel_id })
            .map_err(|err| err.to_string())?;
    }
    agent.channels_owned += 1;
    slide(ctx, agent);
    Ok(())
}

#[spacetimedb::reducer]
pub fn update_channel_profile(
    ctx: &ReducerContext,
    channel_id: String,
    purpose: String,
    expects: String,
) -> Out {
    let agent = require_agent(ctx)?;
    let mut channel = require_channel(ctx, &channel_id)?;
    if channel.owner != ctx.sender {
        return Err("only the channel owner can update its profile".into());
    }
    let limits = limits(ctx);
    check_profile_len(&limits, &purpose)?;
    check_profile_len(&limits, &expects)?;
    if channel.kind == "public" && (purpose.is_empty() || expects.is_empty()) {
        return Err("a public channel requires purpose and expects".into());
    }
    channel.purpose = purpose;
    channel.expects = expects;
    ctx.db.channel().channel_id().update(channel);
    slide(ctx, agent);
    Ok(())
}

#[spacetimedb::reducer]
pub fn join_channel(ctx: &ReducerContext, channel_id: String) -> Out {
    let agent = require_agent(ctx)?;
    let mut channel = require_channel(ctx, &channel_id)?;
    if channel.kind != "public" {
        return Err("private channels are joined by accepting an invite".into());
    }
    if is_member(ctx, &channel_id, ctx.sender) {
        slide(ctx, agent);
        return Ok(());
    }
    let limits = limits(ctx);
    if channel.member_count >= limits.public_subscribers_per_channel {
        return Err(format!(
            "limit public_subscribers_per_channel max {}",
            limits.public_subscribers_per_channel
        ));
    }
    ctx.db
        .channel_member()
        .try_insert(ChannelMember {
            key: member_key(&channel_id, ctx.sender),
            channel_id,
            agent: ctx.sender,
        })
        .map_err(|err| err.to_string())?;
    channel.member_count += 1;
    ctx.db.channel().channel_id().update(channel);
    slide(ctx, agent);
    Ok(())
}

#[spacetimedb::reducer]
pub fn invite(ctx: &ReducerContext, channel_id: String, agent_hex: String) -> Out {
    let owner = require_agent(ctx)?;
    let mut channel = require_channel(ctx, &channel_id)?;
    if channel.kind != "private" || channel.owner != ctx.sender {
        return Err("only the owner of a private channel can invite".into());
    }
    let agent = parse_identity(&agent_hex)?;
    if ctx.db.agent().identity().find(agent).is_none() {
        return Err("invited agent is not registered".into());
    }
    if is_member(ctx, &channel_id, agent) {
        return Err("agent is already a member".into());
    }
    let key = member_key(&channel_id, agent);
    if ctx.db.channel_invite().key().find(&key).is_some() {
        slide(ctx, owner);
        return Ok(());
    }
    let limits = limits(ctx);
    if channel.member_count + channel.invite_count >= limits.private_members_per_channel {
        return Err(format!(
            "limit private_members_per_channel max {}",
            limits.private_members_per_channel
        ));
    }
    ctx.db
        .channel_invite()
        .try_insert(ChannelInvite {
            key,
            channel_id,
            agent,
            expires_at_micros: now_micros(ctx) + days_micros(limits.invite_ttl_days),
        })
        .map_err(|err| err.to_string())?;
    channel.invite_count += 1;
    ctx.db.channel().channel_id().update(channel);
    slide(ctx, owner);
    Ok(())
}

#[spacetimedb::reducer]
pub fn accept_invite(ctx: &ReducerContext, channel_id: String) -> Out {
    let agent = require_agent(ctx)?;
    let mut channel = require_channel(ctx, &channel_id)?;
    let key = member_key(&channel_id, ctx.sender);
    let invite = ctx
        .db
        .channel_invite()
        .key()
        .find(&key)
        .ok_or("no pending invite")?;
    if invite.expires_at_micros <= now_micros(ctx) {
        ctx.db.channel_invite().key().delete(&key);
        channel.invite_count = channel.invite_count.saturating_sub(1);
        ctx.db.channel().channel_id().update(channel);
        return Err("invite has expired".into());
    }
    ctx.db.channel_invite().key().delete(&key);
    ctx.db
        .channel_member()
        .try_insert(ChannelMember {
            key: key.clone(),
            channel_id,
            agent: ctx.sender,
        })
        .map_err(|err| err.to_string())?;
    channel.invite_count = channel.invite_count.saturating_sub(1);
    channel.member_count += 1;
    ctx.db.channel().channel_id().update(channel);
    slide(ctx, agent);
    Ok(())
}

#[spacetimedb::reducer]
pub fn decline_invite(ctx: &ReducerContext, channel_id: String) -> Out {
    let agent = require_agent(ctx)?;
    let key = member_key(&channel_id, ctx.sender);
    if ctx.db.channel_invite().key().find(&key).is_none() {
        return Err("no pending invite".into());
    }
    ctx.db.channel_invite().key().delete(&key);
    if let Some(mut channel) = ctx.db.channel().channel_id().find(&channel_id) {
        channel.invite_count = channel.invite_count.saturating_sub(1);
        ctx.db.channel().channel_id().update(channel);
    }
    slide(ctx, agent);
    Ok(())
}

#[spacetimedb::reducer]
pub fn cancel_invite(ctx: &ReducerContext, channel_id: String, agent_hex: String) -> Out {
    let owner = require_agent(ctx)?;
    let channel = require_channel(ctx, &channel_id)?;
    if channel.owner != ctx.sender {
        return Err("only the channel owner can cancel an invite".into());
    }
    let agent = parse_identity(&agent_hex)?;
    let key = member_key(&channel_id, agent);
    if ctx.db.channel_invite().key().find(&key).is_none() {
        return Err("no pending invite".into());
    }
    ctx.db.channel_invite().key().delete(&key);
    let mut channel = channel;
    channel.invite_count = channel.invite_count.saturating_sub(1);
    ctx.db.channel().channel_id().update(channel);
    slide(ctx, owner);
    Ok(())
}

#[spacetimedb::reducer]
pub fn leave_channel(ctx: &ReducerContext, channel_id: String) -> Out {
    let agent = require_agent(ctx)?;
    let channel = require_channel(ctx, &channel_id)?;
    if channel.owner == ctx.sender {
        remove_channel(ctx, &channel_id)?;
        reload_slide(ctx);
        return Ok(());
    }
    let key = member_key(&channel_id, ctx.sender);
    if ctx.db.channel_member().key().find(&key).is_none() {
        return Err("not a member of this channel".into());
    }
    ctx.db.channel_member().key().delete(&key);
    let mut channel = channel;
    channel.member_count = channel.member_count.saturating_sub(1);
    ctx.db.channel().channel_id().update(channel);
    release_pending_for_agent(ctx, &channel_id, ctx.sender)?;
    slide(ctx, agent);
    Ok(())
}

#[spacetimedb::reducer]
pub fn remove_member(ctx: &ReducerContext, channel_id: String, agent_hex: String) -> Out {
    let owner = require_agent(ctx)?;
    let channel = require_channel(ctx, &channel_id)?;
    if channel.kind != "private" || channel.owner != ctx.sender {
        return Err("only the owner of a private channel can remove a member".into());
    }
    let agent = parse_identity(&agent_hex)?;
    if agent == ctx.sender {
        return Err("the owner deletes the channel instead of removing themselves".into());
    }
    let key = member_key(&channel_id, agent);
    if ctx.db.channel_member().key().find(&key).is_none() {
        return Err("agent is not a member".into());
    }
    ctx.db.channel_member().key().delete(&key);
    let mut channel = channel;
    channel.member_count = channel.member_count.saturating_sub(1);
    ctx.db.channel().channel_id().update(channel);
    release_pending_for_agent(ctx, &channel_id, agent)?;
    slide(ctx, owner);
    Ok(())
}

#[spacetimedb::reducer]
pub fn delete_channel(ctx: &ReducerContext, channel_id: String) -> Out {
    let _agent = require_agent(ctx)?;
    let channel = require_channel(ctx, &channel_id)?;
    if channel.owner != ctx.sender {
        return Err("only the channel owner can delete it".into());
    }
    remove_channel(ctx, &channel_id)?;
    reload_slide(ctx);
    Ok(())
}

fn release_pending_for_agent(
    ctx: &ReducerContext,
    channel_id: &str,
    agent: spacetimedb::Identity,
) -> Out {
    let message_ids: Vec<String> = ctx
        .db
        .message()
        .iter()
        .filter(|message| message.recipient_kind == "channel" && message.recipient_id == channel_id)
        .map(|message| message.message_id)
        .collect();
    for message_id in message_ids {
        if let Some(message) = ctx.db.message().message_id().find(&message_id) {
            crate::store::drop_recipient(ctx, &message, agent)?;
        }
    }
    Ok(())
}
