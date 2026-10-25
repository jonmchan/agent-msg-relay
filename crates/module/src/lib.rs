mod agents;
mod channels;
mod messages;
mod schema;
mod store;
mod util;
mod views;

use schema::{agent, channel, channel_invite, delivery, limits, message, stats};
use schema::{Limits, Stats, LIMITS_ID, STATS_ID};
use spacetimedb::{ReducerContext, ScheduleAt, Table, TimeDuration};
use store::{drop_recipient, remove_channel};
use util::{days_micros, limits, now_micros, Out};

#[spacetimedb::reducer(init)]
fn init(ctx: &ReducerContext) -> Out {
    if ctx.db.limits().id().find(LIMITS_ID).is_none() {
        ctx.db
            .limits()
            .try_insert(Limits::defaults())
            .map_err(|err| err.to_string())?;
    }
    if ctx.db.stats().id().find(STATS_ID).is_none() {
        ctx.db
            .stats()
            .try_insert(Stats {
                id: STATS_ID,
                registered_agents: 0,
                live_message_bytes: 0,
            })
            .map_err(|err| err.to_string())?;
    }
    if ctx.db.expire_schedule().iter().next().is_none() {
        ctx.db
            .expire_schedule()
            .try_insert(ExpireSchedule {
                scheduled_id: 0,
                scheduled_at: TimeDuration::from_micros(3_600_000_000).into(),
            })
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[spacetimedb::table(accessor = expire_schedule, scheduled(expire_due))]
pub struct ExpireSchedule {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
}

#[spacetimedb::reducer]
fn expire_due(ctx: &ReducerContext, _schedule: ExpireSchedule) -> Out {
    if ctx.sender() != ctx.database_identity() {
        return Err("expire_due is scheduler-only".into());
    }
    let now = now_micros(ctx);
    let limits = limits(ctx);
    let message_cutoff = now - days_micros(limits.message_ttl_days);

    let expired_agents: Vec<_> = ctx
        .db
        .agent()
        .iter()
        .filter(|agent| agent.expires_at_micros <= now)
        .map(|agent| agent.identity)
        .collect();
    for identity in expired_agents {
        store::delete_agent(ctx, identity)?;
    }

    let expired_invites: Vec<_> = ctx
        .db
        .channel_invite()
        .iter()
        .filter(|invite| invite.expires_at_micros <= now)
        .map(|invite| (invite.channel_id, invite.key))
        .collect();
    for (channel_id, key) in expired_invites {
        ctx.db.channel_invite().key().delete(&key);
        if let Some(mut channel) = ctx.db.channel().channel_id().find(&channel_id) {
            channel.invite_count = channel.invite_count.saturating_sub(1);
            ctx.db.channel().channel_id().update(channel);
        }
    }

    let expired_messages: Vec<_> = ctx
        .db
        .message()
        .iter()
        .filter(|message| message.created_micros <= message_cutoff)
        .map(|message| message.message_id)
        .collect();
    for message_id in expired_messages {
        store::delete_message(ctx, &message_id)?;
    }

    // Drop deliveries whose agent row is already gone, then delete empty messages.
    let orphan_deliveries: Vec<_> = ctx
        .db
        .delivery()
        .iter()
        .filter(|delivery| ctx.db.agent().identity().find(delivery.agent).is_none())
        .map(|delivery| (delivery.message_id, delivery.agent))
        .collect();
    for (message_id, agent) in orphan_deliveries {
        if let Some(message) = ctx.db.message().message_id().find(&message_id) {
            drop_recipient(ctx, &message, agent)?;
        }
    }

    let orphan_channels: Vec<_> = ctx
        .db
        .channel()
        .iter()
        .filter(|channel| ctx.db.agent().identity().find(channel.owner).is_none())
        .map(|channel| channel.channel_id)
        .collect();
    for channel_id in orphan_channels {
        remove_channel(ctx, &channel_id)?;
    }
    Ok(())
}
