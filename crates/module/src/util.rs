use agent_msg_relay_protocol::{decode_hex, normalize_identity, valid_public_name};
use spacetimedb::{Identity, ReducerContext, Timestamp};

use crate::schema::{limits, stats, Limits, LIMITS_ID, STATS_ID};

pub type Out = Result<(), String>;

pub fn identity_string(identity: Identity) -> String {
    let text = identity.to_string();
    normalize_identity(&text).unwrap_or(text)
}

pub fn parse_identity(value: &str) -> Result<Identity, String> {
    let normalized = normalize_identity(value)?;
    Identity::from_hex(normalized).map_err(|err| err.to_string())
}

pub fn now_micros(ctx: &ReducerContext) -> i64 {
    ctx.timestamp.to_micros_since_unix_epoch()
}

pub fn days_micros(days: u32) -> i64 {
    i64::from(days) * 24 * 60 * 60 * 1_000_000
}

pub fn utc_day(micros: i64) -> i64 {
    micros.div_euclid(24 * 60 * 60 * 1_000_000)
}

pub fn utc_minute(micros: i64) -> i64 {
    micros.div_euclid(60 * 1_000_000)
}

pub fn limits(ctx: &ReducerContext) -> Limits {
    ctx.db
        .limits()
        .id()
        .find(LIMITS_ID)
        .unwrap_or_else(Limits::defaults)
}

pub fn require_owner(ctx: &ReducerContext) -> Out {
    let limits = limits(ctx);
    let owner = limits
        .owner
        .ok_or("database owner has not claimed the limits row")?;
    if ctx.sender() != owner {
        return Err("set_limits is reserved for the database owner".into());
    }
    Ok(())
}

pub fn bump_stats(ctx: &ReducerContext, agents_delta: i64, bytes_delta: i64) -> Out {
    let mut stats = ctx
        .db
        .stats()
        .id()
        .find(STATS_ID)
        .ok_or("stats row is missing")?;
    stats.registered_agents = add_u64(stats.registered_agents, agents_delta)?;
    stats.live_message_bytes = add_u64(stats.live_message_bytes, bytes_delta)?;
    ctx.db.stats().id().update(stats);
    Ok(())
}

fn add_u64(value: u64, delta: i64) -> Result<u64, String> {
    if delta >= 0 {
        value
            .checked_add(delta as u64)
            .ok_or_else(|| "counter overflow".into())
    } else {
        value
            .checked_sub(delta.unsigned_abs())
            .ok_or_else(|| "counter underflow".into())
    }
}

pub fn check_profile_len(limits: &Limits, value: &str) -> Out {
    if value.len() as u64 > limits.profile_text_max_bytes {
        Err(format!(
            "limit profile_text_max_bytes max {}",
            limits.profile_text_max_bytes
        ))
    } else {
        Ok(())
    }
}

pub fn require_name(name: &str) -> Out {
    if valid_public_name(name) {
        Ok(())
    } else {
        Err("name must match [a-z0-9-]{3,32}".into())
    }
}

pub fn member_key(channel_id: &str, agent: Identity) -> String {
    format!("{channel_id}|{}", identity_string(agent))
}

pub fn dm_key(a: Identity, b: Identity) -> String {
    let left = identity_string(a);
    let right = identity_string(b);
    if left <= right {
        format!("{left}|{right}")
    } else {
        format!("{right}|{left}")
    }
}

pub fn decode_pubkey(hex_key: &str) -> Result<Vec<u8>, String> {
    let bytes = decode_hex(hex_key)?;
    if bytes.len() != 33 {
        return Err("public key must be a compressed secp256k1 key".into());
    }
    Ok(bytes)
}

pub fn within_skew(server: Timestamp, client_micros: i64) -> bool {
    let server_micros = server.to_micros_since_unix_epoch();
    (server_micros - client_micros).abs() <= agent_msg_relay_protocol::SKEW_MICROS
}

#[cfg(test)]
mod tests {
    use super::{
        check_profile_len, days_micros, decode_pubkey, dm_key, member_key, require_name, utc_day,
        utc_minute,
    };
    use crate::schema::Limits;
    use spacetimedb::Identity;

    fn identity(byte: u8) -> Identity {
        let mut bytes = [0u8; 32];
        bytes[31] = byte;
        Identity::from_byte_array(bytes)
    }

    #[test]
    fn day_and_minute_windows_use_utc() {
        assert_eq!(days_micros(1), 86_400_000_000);
        assert_eq!(utc_day(0), 0);
        assert_eq!(utc_day(86_400_000_000 - 1), 0);
        assert_eq!(utc_day(86_400_000_000), 1);
        assert_eq!(utc_day(-1), -1);
        assert_eq!(utc_minute(60_000_000), 1);
    }

    #[test]
    fn profile_cap_counts_bytes_and_names_stay_narrow() {
        let mut limits = Limits::defaults();
        limits.profile_text_max_bytes = 1;
        assert!(check_profile_len(&limits, "a").is_ok());
        let error = check_profile_len(&limits, "é").unwrap_err();
        assert!(error.contains("profile_text_max_bytes"), "{error}");
        assert!(require_name("lobby").is_ok());
        assert!(require_name("Lobby").is_err());
        assert!(decode_pubkey("aa").is_err());
    }

    #[test]
    fn dm_pair_key_is_independent_of_argument_order() {
        let left = identity(1);
        let right = identity(2);
        assert_eq!(dm_key(left, right), dm_key(right, left));
        assert!(member_key("channel", left).contains('|'));
    }
}
