use spacetimedb::{AnonymousViewContext, ViewContext};

use crate::schema::{
    agent_lookup__view, channel__view, channel_invite__view, channel_member__view, delivery__view,
    limits__view, message__view, public_agent_name__view, AgentLookup, ChannelRow, InboxRow,
    InviteRow, Limits, MemberRow, PublicChannelRow, LIMITS_ID,
};

#[spacetimedb::view(name = public_channels, public)]
fn public_channels(ctx: &AnonymousViewContext) -> Vec<PublicChannelRow> {
    ctx.db
        .channel()
        .kind()
        .filter("public")
        .map(|channel| {
            let owner_name = ctx
                .db
                .public_agent_name()
                .agent()
                .find(channel.owner)
                .map(|row| row.name)
                .unwrap_or_default();
            PublicChannelRow {
                channel_id: channel.channel_id,
                name: channel.name,
                purpose: channel.purpose,
                expects: channel.expects,
                owner: channel.owner,
                owner_name,
                subscriber_count: channel.member_count,
            }
        })
        .collect()
}

#[spacetimedb::view(name = current_limits, public)]
fn current_limits(ctx: &AnonymousViewContext) -> Option<Limits> {
    ctx.db.limits().id().find(LIMITS_ID)
}

#[spacetimedb::view(name = my_inbox, public)]
fn my_inbox(ctx: &ViewContext) -> Vec<InboxRow> {
    ctx.db
        .delivery()
        .agent()
        .filter(ctx.sender)
        .filter(|delivery| !delivery.acked)
        .filter_map(|delivery| ctx.db.message().message_id().find(&delivery.message_id))
        .map(|message| InboxRow {
            message_id: message.message_id,
            sender: message.sender,
            recipient_kind: message.recipient_kind,
            recipient_id: message.recipient_id,
            body: message.body,
            encrypted: message.encrypted,
            signature_hex: message.signature_hex,
            created_micros: message.created_micros,
            wraps_json: message.wraps_json,
        })
        .collect()
}

#[spacetimedb::view(name = my_agent_lookup, public)]
fn my_agent_lookup(ctx: &ViewContext) -> Option<AgentLookup> {
    ctx.db.agent_lookup().caller().find(ctx.sender)
}

#[spacetimedb::view(name = my_channels, public)]
fn my_channels(ctx: &ViewContext) -> Vec<ChannelRow> {
    let mine: Vec<String> = ctx
        .db
        .channel_member()
        .agent()
        .filter(ctx.sender)
        .map(|row| row.channel_id)
        .collect();
    mine.into_iter()
        .filter_map(|channel_id| ctx.db.channel().channel_id().find(&channel_id))
        .map(channel_row)
        .collect()
}

#[spacetimedb::view(name = my_members, public)]
fn my_members(ctx: &ViewContext) -> Vec<MemberRow> {
    my_channel_ids(ctx)
        .into_iter()
        .flat_map(|channel_id| {
            ctx.db
                .channel_member()
                .channel_id()
                .filter(channel_id.as_str())
                .map(|member| MemberRow {
                    channel_id: member.channel_id,
                    agent: member.agent,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[spacetimedb::view(name = my_invites, public)]
fn my_invites(ctx: &ViewContext) -> Vec<InviteRow> {
    ctx.db
        .channel_invite()
        .agent()
        .filter(ctx.sender)
        .map(|invite| InviteRow {
            channel_id: invite.channel_id,
            agent: invite.agent,
            expires_at_micros: invite.expires_at_micros,
        })
        .collect()
}

fn my_channel_ids(ctx: &ViewContext) -> Vec<String> {
    ctx.db
        .channel_member()
        .agent()
        .filter(ctx.sender)
        .map(|row| row.channel_id)
        .collect()
}

fn channel_row(channel: crate::schema::Channel) -> ChannelRow {
    ChannelRow {
        channel_id: channel.channel_id,
        kind: channel.kind,
        name: channel.name,
        purpose: channel.purpose,
        expects: channel.expects,
        owner: channel.owner,
        member_count: channel.member_count,
        invite_count: channel.invite_count,
    }
}
