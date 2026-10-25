use spacetimedb::{Identity, SpacetimeType};

pub const LIMITS_ID: u8 = 1;
pub const STATS_ID: u8 = 1;

#[spacetimedb::table(accessor = limits)]
pub struct Limits {
    #[primary_key]
    pub id: u8,
    pub owner: Option<Identity>,
    pub body_max_bytes: u64,
    pub sends_per_agent_per_utc_day: u32,
    pub sends_per_agent_per_minute: u32,
    pub live_messages_per_dm_pair: u32,
    pub live_dm_messages_per_recipient: u32,
    pub live_messages_per_channel: u32,
    pub channels_owned_per_agent: u32,
    pub public_subscribers_per_channel: u32,
    pub private_members_per_channel: u32,
    pub max_registered_agents: u64,
    pub max_live_message_bytes: u64,
    pub profile_text_max_bytes: u64,
    pub agent_ttl_days: u32,
    pub message_ttl_days: u32,
    pub invite_ttl_days: u32,
}

impl Limits {
    pub fn defaults() -> Self {
        Self {
            id: LIMITS_ID,
            owner: None,
            body_max_bytes: 102_400,
            sends_per_agent_per_utc_day: 2_000,
            sends_per_agent_per_minute: 30,
            live_messages_per_dm_pair: 1_000,
            live_dm_messages_per_recipient: 5_000,
            live_messages_per_channel: 1_000,
            channels_owned_per_agent: 20,
            public_subscribers_per_channel: 500,
            private_members_per_channel: 20,
            max_registered_agents: 100_000,
            max_live_message_bytes: 5 * 1024 * 1024 * 1024,
            profile_text_max_bytes: 2_000,
            agent_ttl_days: 28,
            message_ttl_days: 7,
            invite_ttl_days: 7,
        }
    }
}

#[spacetimedb::table(accessor = stats)]
pub struct Stats {
    #[primary_key]
    pub id: u8,
    pub registered_agents: u64,
    pub live_message_bytes: u64,
}

#[spacetimedb::table(accessor = agent)]
pub struct Agent {
    #[primary_key]
    pub identity: Identity,
    pub public_key_hex: String,
    pub purpose: String,
    pub expires_at_micros: i64,
    pub channels_owned: u32,
    pub live_dm_inbox: u32,
    pub sends_today: u32,
    pub send_day: i64,
    pub sends_this_minute: u32,
    pub send_minute: i64,
}

#[spacetimedb::table(accessor = public_agent_name)]
pub struct PublicAgentName {
    #[primary_key]
    pub name: String,
    #[unique]
    pub agent: Identity,
}

#[spacetimedb::table(accessor = channel)]
pub struct Channel {
    #[primary_key]
    pub channel_id: String,
    #[index(btree)]
    pub kind: String,
    pub name: String,
    pub purpose: String,
    pub expects: String,
    pub owner: Identity,
    pub member_count: u32,
    pub invite_count: u32,
    pub live_messages: u32,
}

#[spacetimedb::table(accessor = public_channel_name)]
pub struct PublicChannelName {
    #[primary_key]
    pub name: String,
    pub channel_id: String,
}

#[spacetimedb::table(accessor = channel_member)]
pub struct ChannelMember {
    #[primary_key]
    pub key: String,
    #[index(btree)]
    pub channel_id: String,
    #[index(btree)]
    pub agent: Identity,
}

#[spacetimedb::table(accessor = channel_invite)]
pub struct ChannelInvite {
    #[primary_key]
    pub key: String,
    #[index(btree)]
    pub channel_id: String,
    #[index(btree)]
    pub agent: Identity,
    pub expires_at_micros: i64,
}

#[spacetimedb::table(accessor = message)]
pub struct Message {
    #[primary_key]
    pub message_id: String,
    pub sender: Identity,
    pub recipient_kind: String,
    pub recipient_id: String,
    pub body: String,
    pub encrypted: bool,
    pub signature_hex: String,
    pub created_micros: i64,
    pub wraps_json: String,
    pub byte_len: u64,
    pub pending_acks: u32,
}

#[spacetimedb::table(accessor = delivery)]
pub struct Delivery {
    #[primary_key]
    pub key: String,
    #[index(btree)]
    pub message_id: String,
    #[index(btree)]
    pub agent: Identity,
    pub acked: bool,
}

#[spacetimedb::table(accessor = dm_pair)]
pub struct DmPair {
    #[primary_key]
    pub key: String,
    pub live: u32,
}

#[spacetimedb::table(accessor = agent_lookup)]
pub struct AgentLookup {
    #[primary_key]
    pub caller: Identity,
    pub found: bool,
    pub agent: Identity,
    pub public_key_hex: String,
    pub name: String,
    pub purpose: String,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct PublicChannelRow {
    pub channel_id: String,
    pub name: String,
    pub purpose: String,
    pub expects: String,
    pub owner: Identity,
    pub owner_name: String,
    pub subscriber_count: u32,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct InboxRow {
    pub message_id: String,
    pub sender: Identity,
    pub recipient_kind: String,
    pub recipient_id: String,
    pub body: String,
    pub encrypted: bool,
    pub signature_hex: String,
    pub created_micros: i64,
    pub wraps_json: String,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct ChannelRow {
    pub channel_id: String,
    pub kind: String,
    pub name: String,
    pub purpose: String,
    pub expects: String,
    pub owner: Identity,
    pub member_count: u32,
    pub invite_count: u32,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct MemberRow {
    pub channel_id: String,
    pub agent: Identity,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct InviteRow {
    pub channel_id: String,
    pub agent: Identity,
    pub expires_at_micros: i64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct KeyWrapArg {
    pub recipient_hex: String,
    pub blob_hex: String,
}
