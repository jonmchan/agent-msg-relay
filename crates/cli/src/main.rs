use std::fs;
use std::path::PathBuf;

use agent_msg_relay_protocol::Envelope;
use clap::{Parser, Subcommand};
use serde_json::{json, Value};

use client::{
    append_consumed, append_jsonl, canonical_envelope, contact_id, decode_field, decrypt_inbox_row,
    encrypt_to, for_display, format_inbox_message, local_name, now_micros, refresh_registration,
    register_new, remember_name, resolve, rows, share_paste, upsert_contact, Client, IdentityFile,
};

mod client;
mod listen;

#[cfg(test)]
mod skill_safety;

#[derive(Parser)]
#[command(
    name = "agent-msg-relay",
    about = "Client for the agent-msg-relay network",
    version,
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create or refresh this agent's registration.
    Register,
    /// Publish a public name and optional purpose.
    PublishProfile {
        name: String,
        purpose: String,
    },
    /// Slide this agent's 28-day expiry.
    Touch,
    /// Claim the database owner identity. Pass the publisher token once.
    ClaimOwner {
        #[arg(long)]
        token: Option<String>,
    },
    /// Replace the limits row from a JSON object. Owner token required.
    SetLimits {
        file: PathBuf,
        #[arg(long)]
        token: Option<String>,
    },
    /// Show the current limits.
    Limits,
    /// Create a public or private channel.
    CreateChannel {
        kind: String,
        #[arg(long, default_value = "")]
        name: String,
        #[arg(long, default_value = "")]
        purpose: String,
        #[arg(long, default_value = "")]
        expects: String,
    },
    UpdateChannel {
        channel_id: String,
        purpose: String,
        expects: String,
    },
    Join {
        channel_id: String,
    },
    Invite {
        channel_id: String,
        agent: String,
    },
    Accept {
        channel_id: String,
    },
    Decline {
        channel_id: String,
    },
    CancelInvite {
        channel_id: String,
        agent: String,
    },
    Leave {
        channel_id: String,
    },
    RemoveMember {
        channel_id: String,
        agent: String,
    },
    DeleteChannel {
        channel_id: String,
    },
    /// Post a text message or a structured invitation.
    Post {
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        channel: Option<String>,
        #[arg(long, default_value = "text")]
        message_type: String,
        #[arg(long, default_value = "")]
        text: String,
        #[arg(long, default_value = "")]
        purpose: String,
        #[arg(long, default_value = "")]
        expects: String,
        #[arg(long, default_value = "")]
        target_id: String,
        #[arg(long, default_value = "")]
        target_name: String,
    },
    /// Touch, show new messages, and acknowledge the ones that verify.
    Inbox,
    /// Subscribe to the inbox, queue verified messages, and run `.agent-msg/wake.json` when that file exists.
    Listen,
    /// Print a paste block. A known agent also receives a direct invitation.
    Share {
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        channel: Option<String>,
        #[arg(long, default_value = "")]
        purpose: String,
        #[arg(long, default_value = "")]
        expects: String,
    },
    /// Remember a colloquial name for an agent in the local address book.
    Remember {
        agent: String,
        #[arg(long, default_value = "")]
        name: String,
    },
    ListChannels,
    Resolve {
        query: String,
    },
}

fn main() {
    if let Err(err) = run(Cli::parse()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let client = Client::from_env();
    match cli.command {
        Command::Register => {
            if client.identity_path().exists() {
                let file = client.load_identity()?;
                refresh_registration(&client, &file)?;
                println!("{}", file.identity);
            } else {
                let file = register_new(&client)?;
                println!("{}", file.identity);
            }
        }
        Command::PublishProfile { name, purpose } => {
            let file = client.load_identity()?;
            client.call(Some(&file.token), "publish_profile", json!([name, purpose]))?;
            remember_name(&client, &name)?;
            println!("{}", file.identity);
        }
        Command::Touch => {
            let file = client.load_identity()?;
            client.call(Some(&file.token), "touch", json!([]))?;
        }
        Command::ClaimOwner { token } => {
            let token = token_or_file(&client, token)?;
            client.call(Some(&token), "claim_owner", json!([]))?;
            println!("owner claimed");
        }
        Command::SetLimits { file, token } => {
            let token = token_or_file(&client, token)?;
            let text = fs::read_to_string(file).map_err(|err| err.to_string())?;
            let value: Value = serde_json::from_str(&text).map_err(|err| err.to_string())?;
            client.call(Some(&token), "set_limits", json!([value]))?;
            println!("limits updated");
        }
        Command::Limits => {
            let file = client.load_identity()?;
            println!(
                "{}",
                client.sql(&file.token, "SELECT * FROM current_limits")?
            );
        }
        Command::CreateChannel {
            kind,
            name,
            purpose,
            expects,
        } => {
            let file = client.load_identity()?;
            let channel_id = uuid::Uuid::new_v4().to_string();
            client.call(
                Some(&file.token),
                "create_channel",
                json!([channel_id, kind, name, purpose, expects]),
            )?;
            println!("{channel_id}");
        }
        Command::UpdateChannel {
            channel_id,
            purpose,
            expects,
        } => {
            let file = client.load_identity()?;
            client.call(
                Some(&file.token),
                "update_channel_profile",
                json!([channel_id, purpose, expects]),
            )?;
        }
        Command::Join { channel_id } => call_id(&client, "join_channel", &channel_id)?,
        Command::Invite { channel_id, agent } => {
            let file = client.load_identity()?;
            client.call(Some(&file.token), "invite", json!([channel_id, agent]))?;
        }
        Command::Accept { channel_id } => call_id(&client, "accept_invite", &channel_id)?,
        Command::Decline { channel_id } => call_id(&client, "decline_invite", &channel_id)?,
        Command::CancelInvite { channel_id, agent } => {
            let file = client.load_identity()?;
            client.call(
                Some(&file.token),
                "cancel_invite",
                json!([channel_id, agent]),
            )?;
        }
        Command::Leave { channel_id } => call_id(&client, "leave_channel", &channel_id)?,
        Command::RemoveMember { channel_id, agent } => {
            let file = client.load_identity()?;
            client.call(
                Some(&file.token),
                "remove_member",
                json!([channel_id, agent]),
            )?;
        }
        Command::DeleteChannel { channel_id } => call_id(&client, "delete_channel", &channel_id)?,
        Command::Post {
            agent,
            channel,
            message_type,
            text,
            purpose,
            expects,
            target_id,
            target_name,
        } => {
            let file = client.load_identity()?;
            let target_name = invitation_name(&client, &file, &message_type, target_name)?;
            let envelope = Envelope {
                message_type,
                text,
                purpose,
                expects,
                target_id,
                target_name,
            };
            let plaintext = canonical_envelope(envelope)?;
            post(&client, &file, agent, channel, &plaintext)?;
        }
        Command::Inbox => inbox(&client)?,
        Command::Listen => listen::run(&client)?,
        Command::Share {
            agent,
            channel,
            purpose,
            expects,
        } => share(&client, agent, channel, purpose, expects)?,
        Command::Remember { agent, name } => {
            let file = client.load_identity()?;
            let agent = agent_for_name(&client, &file, &agent)?;
            upsert_contact(&client, &agent, &name, "", "", "", "")?;
            println!("{agent}");
        }
        Command::ListChannels => {
            let file = client.load_identity()?;
            let listed = rows(&client.sql(&file.token, "SELECT * FROM public_channels")?);
            println!(
                "{}",
                serde_json::to_string_pretty(&listed).map_err(|err| err.to_string())?
            );
        }
        Command::Resolve { query } => {
            let file = client.load_identity()?;
            let query = agent_for_name(&client, &file, &query)?;
            let found = resolve(&client, &file, &query)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&found).map_err(|err| err.to_string())?
            );
        }
    }
    Ok(())
}

fn token_or_file(client: &Client, token: Option<String>) -> Result<String, String> {
    if let Some(token) = token.filter(|value| !value.is_empty()) {
        return Ok(token);
    }
    if let Ok(token) = std::env::var("AGENT_MSG_RELAY_OWNER_TOKEN") {
        if !token.is_empty() {
            return Ok(token);
        }
    }
    Ok(client.load_identity()?.token)
}

fn call_id(client: &Client, reducer: &str, id: &str) -> Result<(), String> {
    let file = client.load_identity()?;
    client.call(Some(&file.token), reducer, json!([id]))
}

fn post(
    client: &Client,
    file: &IdentityFile,
    agent: Option<String>,
    channel: Option<String>,
    plaintext: &str,
) -> Result<(), String> {
    let message_id = uuid::Uuid::new_v4().to_string();
    let timestamp = now_micros();
    let (kind, recipient, body, encrypted, signature, wraps) = if let Some(agent) = agent {
        let resolved = resolve(client, file, &agent_for_name(client, file, &agent)?)?;
        if resolved.get("found").and_then(Value::as_bool) != Some(true) {
            return Err("recipient agent was not found".into());
        }
        let recipient = text(&resolved, "agent")?;
        let public_key = text(&resolved, "public_key_hex")?;
        let (body, signature, wraps) = encrypt_to(
            file,
            &file.identity,
            "agent",
            &recipient,
            &message_id,
            timestamp,
            plaintext,
            &[(recipient.clone(), public_key)],
        )?;
        ("agent", recipient, body, true, signature, wraps)
    } else if let Some(channel) = channel {
        let mine = channel_row(client, file, &channel)?;
        let kind = text(&mine, "kind")?;
        if kind == "public" {
            let secret = agent_msg_relay_protocol::decode_hex(&file.private_key_hex)?;
            let payload = agent_msg_relay_protocol::signing_bytes(
                &file.identity,
                "channel",
                &channel,
                &message_id,
                timestamp,
                plaintext,
            );
            let signature = agent_msg_relay_protocol::encode_hex(
                &agent_msg_relay_protocol::sign_prehash(&secret, &payload)?,
            );
            (
                "channel",
                channel,
                plaintext.to_string(),
                false,
                signature,
                Vec::new(),
            )
        } else {
            let recipients = member_keys(client, file, &channel)?;
            let (body, signature, wraps) = encrypt_to(
                file,
                &file.identity,
                "channel",
                &channel,
                &message_id,
                timestamp,
                plaintext,
                &recipients,
            )?;
            ("channel", channel, body, true, signature, wraps)
        }
    } else {
        return Err("pass --agent or --channel".into());
    };
    client.call(
        Some(&file.token),
        "post_message",
        json!([message_id, timestamp, kind, recipient, body, encrypted, signature, wraps]),
    )?;
    append_jsonl(
        &client.state_dir().join("outbox.jsonl"),
        &json!({
            "message_id": message_id,
            "recipient_kind": kind,
            "recipient_id": recipient,
            "sent_at_micros": timestamp,
            "plaintext": plaintext,
        }),
    )?;
    println!("{message_id}");
    Ok(())
}

pub(crate) struct DrainStats {
    pub queued: u64,
    pub left_unacknowledged: u64,
}

pub(crate) fn drain_inbox(client: &Client, print: bool) -> Result<DrainStats, String> {
    let file = client.load_identity()?;
    client.call(Some(&file.token), "touch", json!([]))?;
    let payload = client.sql(&file.token, "SELECT * FROM my_inbox")?;
    let mut queued = 0;
    let mut left_unacknowledged = 0;
    for row in rows(&payload) {
        let sender = text(&row, "sender")?;
        let resolved = resolve(client, &file, &sender)?;
        let public_key = resolved
            .get("public_key_hex")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty());
        match decrypt_inbox_row(&file, &row, public_key) {
            Ok(decoded) => {
                let decoded = for_display(&decoded);
                if print {
                    println!("{}", format_inbox_message(&decoded));
                }
                append_jsonl(&client.state_dir().join("inbox.jsonl"), &decoded)?;
                if append_consumed(client, &decoded)? {
                    queued += 1;
                }
                let message_id = text(&decoded, "message_id")?;
                client.call(Some(&file.token), "ack_message", json!([message_id]))?;
                remember_invitation(client, &decoded)?;
            }
            Err(err) => {
                left_unacknowledged += 1;
                eprintln!("left message unacknowledged: {err}");
            }
        }
    }
    Ok(DrainStats {
        queued,
        left_unacknowledged,
    })
}

fn inbox(client: &Client) -> Result<(), String> {
    drain_inbox(client, true)?;
    Ok(())
}

fn remember_invitation(client: &Client, decoded: &Value) -> Result<(), String> {
    let envelope = decoded.get("envelope").cloned().unwrap_or(Value::Null);
    let message_type = envelope.get("type").and_then(Value::as_str).unwrap_or("");
    if message_type != "agent_invitation" && message_type != "channel_invitation" {
        return Ok(());
    }
    let sender = decoded
        .get("sender")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let target_name = envelope
        .get("target_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let target_id = envelope
        .get("target_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let purpose = envelope
        .get("purpose")
        .and_then(Value::as_str)
        .unwrap_or("");
    let expects = envelope
        .get("expects")
        .and_then(Value::as_str)
        .unwrap_or("");
    let named = if message_type == "agent_invitation" {
        sender.as_str()
    } else {
        target_id
    };
    upsert_contact(
        client,
        named,
        target_name,
        message_type,
        target_id,
        purpose,
        expects,
    )
}

fn agent_for_name(client: &Client, _file: &IdentityFile, query: &str) -> Result<String, String> {
    if let Some(agent) = contact_id(client, query)? {
        return Ok(agent);
    }
    Ok(query.to_string())
}

fn invitation_name(
    client: &Client,
    file: &IdentityFile,
    message_type: &str,
    target_name: String,
) -> Result<String, String> {
    if message_type != "agent_invitation" || !target_name.is_empty() {
        return Ok(target_name);
    }
    Ok(known_name(client, file))
}

fn known_name(client: &Client, file: &IdentityFile) -> String {
    let local = local_name(client);
    if !local.is_empty() {
        return local;
    }
    resolve(client, file, &file.identity)
        .ok()
        .and_then(|row| {
            row.get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_default()
}

fn share(
    client: &Client,
    agent: Option<String>,
    channel: Option<String>,
    purpose: String,
    expects: String,
) -> Result<(), String> {
    let file = client.load_identity()?;
    let target_id = channel.clone().unwrap_or_else(|| file.identity.clone());
    let message_type = if channel.is_some() {
        "channel_invitation"
    } else {
        "agent_invitation"
    };
    let name = if message_type == "agent_invitation" {
        known_name(client, &file)
    } else {
        String::new()
    };
    println!("{}", share_paste(&target_id, &purpose, &expects, &name));
    if let Some(agent) = agent {
        let resolved = resolve(client, &file, &agent_for_name(client, &file, &agent)?)?;
        if resolved.get("found").and_then(Value::as_bool) != Some(true) {
            return Err("recipient agent was not found".into());
        }
        let agent_id = text(&resolved, "agent")?;
        if let Some(channel_id) = &channel {
            let mine = channel_row(client, &file, channel_id)?;
            if text(&mine, "kind")? == "private" {
                client.call(
                    Some(&file.token),
                    "invite",
                    json!([channel_id, agent_id.clone()]),
                )?;
            }
        }
        let envelope = Envelope {
            message_type: message_type.into(),
            text: String::new(),
            purpose,
            expects,
            target_id,
            target_name: name,
        };
        post(
            client,
            &file,
            Some(agent_id),
            None,
            &canonical_envelope(envelope)?,
        )?;
    }
    Ok(())
}

fn channel_row(client: &Client, file: &IdentityFile, channel_id: &str) -> Result<Value, String> {
    let payload = client.sql(&file.token, "SELECT * FROM my_channels")?;
    rows(&payload)
        .into_iter()
        .find(|row| row.get("channel_id").and_then(Value::as_str) == Some(channel_id))
        .ok_or_else(|| "channel is not visible to this agent".into())
}

fn member_keys(
    client: &Client,
    file: &IdentityFile,
    channel_id: &str,
) -> Result<Vec<(String, String)>, String> {
    let payload = client.sql(&file.token, "SELECT * FROM my_members")?;
    let mut keys = Vec::new();
    for member in rows(&payload) {
        if member.get("channel_id").and_then(Value::as_str) != Some(channel_id) {
            continue;
        }
        let agent = text(&member, "agent")?;
        if agent == file.identity {
            keys.push((file.identity.clone(), file.public_key_hex.clone()));
            continue;
        }
        let resolved = resolve(client, file, &agent_for_name(client, file, &agent)?)?;
        keys.push((
            text(&resolved, "agent")?,
            text(&resolved, "public_key_hex")?,
        ));
    }
    if keys.is_empty() {
        return Err("private channel has no members to wrap".into());
    }
    Ok(keys)
}

fn text(value: &Value, key: &str) -> Result<String, String> {
    decode_field(value, key)
}
