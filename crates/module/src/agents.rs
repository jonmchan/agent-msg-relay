use agent_msg_relay_protocol::{decode_hex, register_bytes, verify_prehash};
use spacetimedb::{ReducerContext, Table};

use crate::schema::{
    agent, agent_lookup, limits, public_agent_name, stats, Agent, AgentLookup, Limits,
    PublicAgentName,
};
use crate::util::{
    bump_stats, check_profile_len, days_micros, decode_pubkey, identity_string, limits, now_micros,
    parse_identity, require_name, require_owner, Out,
};

pub fn require_agent(ctx: &ReducerContext) -> Result<Agent, String> {
    let agent = ctx
        .db
        .agent()
        .identity()
        .find(ctx.sender())
        .ok_or("register an agent first")?;
    if agent.expires_at_micros <= now_micros(ctx) {
        return Err("agent registration has expired; call register again".into());
    }
    Ok(agent)
}

pub fn slide(ctx: &ReducerContext, mut agent: Agent) {
    let limits = limits(ctx);
    agent.expires_at_micros = now_micros(ctx) + days_micros(limits.agent_ttl_days);
    ctx.db.agent().identity().update(agent);
}

pub fn reload_slide(ctx: &ReducerContext) {
    if let Some(agent) = ctx.db.agent().identity().find(ctx.sender()) {
        slide(ctx, agent);
    }
}

#[spacetimedb::reducer]
pub fn register(ctx: &ReducerContext, public_key_hex: String, possession_sig_hex: String) -> Out {
    let pubkey = decode_pubkey(&public_key_hex)?;
    let signature = decode_hex(&possession_sig_hex)?;
    let identity = identity_string(ctx.sender());
    verify_prehash(&pubkey, &register_bytes(&identity), &signature)?;

    if let Some(existing) = ctx.db.agent().identity().find(ctx.sender()) {
        if existing.public_key_hex != public_key_hex {
            return Err("this identity is already registered with another public key".into());
        }
        slide(ctx, existing);
        return Ok(());
    }

    let limits = limits(ctx);
    let stats = ctx
        .db
        .stats()
        .id()
        .find(crate::schema::STATS_ID)
        .ok_or("stats row is missing")?;
    if stats.registered_agents >= limits.max_registered_agents {
        return Err(format!(
            "limit max_registered_agents max {}",
            limits.max_registered_agents
        ));
    }

    let agent = Agent {
        identity: ctx.sender(),
        public_key_hex,
        purpose: String::new(),
        expires_at_micros: now_micros(ctx) + days_micros(limits.agent_ttl_days),
        channels_owned: 0,
        live_dm_inbox: 0,
        sends_today: 0,
        send_day: 0,
        sends_this_minute: 0,
        send_minute: 0,
    };
    ctx.db
        .agent()
        .try_insert(agent)
        .map_err(|err| err.to_string())?;
    bump_stats(ctx, 1, 0)
}

#[spacetimedb::reducer]
pub fn publish_profile(ctx: &ReducerContext, name: String, purpose: String) -> Out {
    let agent = require_agent(ctx)?;
    let limits = limits(ctx);
    require_name(&name)?;
    check_profile_len(&limits, &purpose)?;
    if let Some(existing) = ctx
        .db
        .public_agent_name()
        .iter()
        .find(|row| row.agent == ctx.sender())
    {
        if existing.name != name {
            return Err("public name is reserved until this agent expires".into());
        }
    } else if ctx.db.public_agent_name().name().find(&name).is_some() {
        return Err("public name is already taken".into());
    } else {
        ctx.db
            .public_agent_name()
            .try_insert(PublicAgentName {
                name: name.clone(),
                agent: ctx.sender(),
            })
            .map_err(|err| err.to_string())?;
    }
    let mut agent = agent;
    agent.purpose = purpose;
    slide(ctx, agent);
    Ok(())
}

#[spacetimedb::reducer]
pub fn touch(ctx: &ReducerContext) -> Out {
    let agent = require_agent(ctx)?;
    slide(ctx, agent);
    Ok(())
}

#[spacetimedb::reducer]
pub fn resolve_agent(ctx: &ReducerContext, query: String) -> Out {
    require_agent(ctx)?;
    let found = if query.trim().starts_with("0x") || query.trim().len() == 64 {
        parse_identity(query.trim())
            .ok()
            .and_then(|identity| ctx.db.agent().identity().find(identity))
    } else {
        ctx.db
            .public_agent_name()
            .name()
            .find(&query)
            .and_then(|row| ctx.db.agent().identity().find(row.agent))
    };
    let lookup = match found {
        Some(agent) => {
            let name = ctx
                .db
                .public_agent_name()
                .iter()
                .find(|row| row.agent == agent.identity)
                .map(|row| row.name)
                .unwrap_or_default();
            let published = !name.is_empty();
            AgentLookup {
                caller: ctx.sender(),
                found: true,
                agent: agent.identity,
                public_key_hex: agent.public_key_hex,
                name,
                purpose: if published {
                    agent.purpose
                } else {
                    String::new()
                },
            }
        }
        None => AgentLookup {
            caller: ctx.sender(),
            found: false,
            agent: ctx.sender(),
            public_key_hex: String::new(),
            name: String::new(),
            purpose: String::new(),
        },
    };
    if ctx.db.agent_lookup().caller().find(ctx.sender()).is_some() {
        ctx.db.agent_lookup().caller().update(lookup);
    } else {
        ctx.db
            .agent_lookup()
            .try_insert(lookup)
            .map_err(|err| err.to_string())?;
    }
    let agent = require_agent(ctx)?;
    slide(ctx, agent);
    Ok(())
}

#[spacetimedb::reducer]
pub fn claim_owner(ctx: &ReducerContext) -> Out {
    let mut limits = limits(ctx);
    if let Some(owner) = limits.owner {
        if owner != ctx.sender() {
            return Err("database owner is already claimed".into());
        }
        return Ok(());
    }
    limits.owner = Some(ctx.sender());
    ctx.db.limits().id().update(limits);
    Ok(())
}

#[spacetimedb::reducer]
pub fn set_limits(ctx: &ReducerContext, next: Limits) -> Out {
    require_owner(ctx)?;
    if next.id != crate::schema::LIMITS_ID {
        return Err("limits id is fixed".into());
    }
    let mut next = next;
    next.owner = limits(ctx).owner;
    ctx.db.limits().id().update(next);
    Ok(())
}

pub fn charge_send(ctx: &ReducerContext, agent: &mut Agent, limits: &Limits) -> Out {
    let now = now_micros(ctx);
    let day = crate::util::utc_day(now);
    let minute = crate::util::utc_minute(now);
    if agent.send_day != day {
        agent.send_day = day;
        agent.sends_today = 0;
    }
    if agent.send_minute != minute {
        agent.send_minute = minute;
        agent.sends_this_minute = 0;
    }
    if agent.sends_today >= limits.sends_per_agent_per_utc_day {
        return Err(format!(
            "limit sends_per_agent_per_utc_day max {}",
            limits.sends_per_agent_per_utc_day
        ));
    }
    if agent.sends_this_minute >= limits.sends_per_agent_per_minute {
        return Err(format!(
            "limit sends_per_agent_per_minute max {}",
            limits.sends_per_agent_per_minute
        ));
    }
    agent.sends_today += 1;
    agent.sends_this_minute += 1;
    Ok(())
}
