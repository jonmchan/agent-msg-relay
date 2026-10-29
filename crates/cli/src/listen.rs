use std::fs;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, ClientRequestBuilder, Message};

use crate::client::{tighten_file, write_private, Client};

const INBOX_QUERY: &str = "SELECT * FROM my_inbox";
const HEARTBEAT_EVERY: Duration = Duration::from_secs(30);
const TOUCH_EVERY: Duration = Duration::from_secs(60 * 60);
const WAKE_BACKOFF_START: Duration = Duration::from_secs(1);
const WAKE_BACKOFF_MAX: Duration = Duration::from_secs(300);
const CONNECT_BACKOFF_MAX: Duration = Duration::from_secs(30);
const HEARTBEAT_MAX_AGE_SECS: u64 = 120;

const WAKE_FAILED: &str = "wake command failed";

pub fn run(client: &Client) -> Result<(), String> {
    let identity = client.load_identity()?;
    let argv = load_wake_argv(&client.state_dir())?;
    validate_wake_argv(&client.root, &argv)?;
    tighten_file(&client.state_dir().join("wake.json"))?;
    let _pid = claim_pid(&client.state_dir())?;
    eprintln!(
        "listener started; a heartbeat older than {HEARTBEAT_MAX_AGE_SECS} seconds is not fresh"
    );
    let mut mail = MailState::new();
    let mut connect_backoff = WAKE_BACKOFF_START;
    loop {
        match session(client, &identity.token, &argv, &mut mail) {
            Ok(()) => connect_backoff = WAKE_BACKOFF_START,
            Err(err) => {
                let _ = fs::remove_file(client.state_dir().join("listener-heartbeat"));
                eprintln!("listener disconnected: {}", redact(&err, &identity.token));
                sleep_while_polling(client, &argv, &mut mail, connect_backoff);
                connect_backoff = (connect_backoff * 2).min(CONNECT_BACKOFF_MAX);
            }
        }
    }
}

fn session(
    client: &Client,
    token: &str,
    argv: &[String],
    mail: &mut MailState,
) -> Result<(), String> {
    let url = websocket_url(&client.host, &client.database)?;
    let uri = url
        .parse()
        .map_err(|_| "listener websocket url is invalid".to_string())?;
    let request = ClientRequestBuilder::new(uri)
        .with_header("Authorization", format!("Bearer {token}"))
        .with_sub_protocol("v1.json.spacetimedb");
    let (mut socket, _response) =
        connect(request).map_err(|err| redact(&err.to_string(), token))?;
    set_read_timeout(socket.get_mut(), HEARTBEAT_EVERY)?;
    let mut subscribed = false;
    let mut last_heartbeat = Instant::now();
    let mut last_touch = Instant::now() - TOUCH_EVERY;
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => match classify_message(text.as_ref())? {
                SocketEvent::Identity if !subscribed => {
                    socket
                        .send(Message::Text(subscribe_message().into()))
                        .map_err(|err| redact(&err.to_string(), token))?;
                }
                SocketEvent::Snapshot(count) => {
                    mail.on_snapshot(count);
                    subscribed = true;
                    write_heartbeat(&client.state_dir())?;
                    last_heartbeat = Instant::now();
                    if last_touch.elapsed() >= TOUCH_EVERY {
                        touch(client, token);
                        last_touch = Instant::now();
                    }
                }
                SocketEvent::Delta { inserts, deletes } => {
                    mail.on_delta(inserts, deletes);
                }
                SocketEvent::SubscribeFailed(reason) => {
                    return Err(format!("subscription failed: {reason}"));
                }
                SocketEvent::Identity | SocketEvent::Ignore => {}
            },
            Ok(Message::Ping(payload)) => {
                socket
                    .send(Message::Pong(payload))
                    .map_err(|err| redact(&err.to_string(), token))?;
            }
            Ok(Message::Close(_)) => return Err("connection closed".into()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(err))
                if err.kind() == ErrorKind::TimedOut || err.kind() == ErrorKind::WouldBlock => {}
            Err(err) => return Err(redact(&err.to_string(), token)),
        }
        if subscribed {
            if last_heartbeat.elapsed() >= HEARTBEAT_EVERY {
                write_heartbeat(&client.state_dir())?;
                last_heartbeat = Instant::now();
            }
            if last_touch.elapsed() >= TOUCH_EVERY {
                touch(client, token);
                last_touch = Instant::now();
            }
            poll_wake(client, argv, mail);
        }
    }
}

fn sleep_while_polling(client: &Client, argv: &[String], mail: &mut MailState, total: Duration) {
    let end = Instant::now() + total;
    while Instant::now() < end {
        poll_wake(client, argv, mail);
        let slice = Duration::from_millis(200).min(end.saturating_duration_since(Instant::now()));
        if slice.is_zero() {
            break;
        }
        thread::sleep(slice);
    }
}

fn poll_wake(client: &Client, argv: &[String], mail: &mut MailState) {
    match mail.poll_child() {
        Some(true) => clear_unhealthy(&client.state_dir()),
        Some(false) => {
            let _ = mark_unhealthy(&client.state_dir());
        }
        None => {}
    }
    if mail.wants_wake(Instant::now()) {
        match spawn_wake(&client.root, argv) {
            Ok(child) => {
                mail.child = Some(child);
                mail.retry_at = None;
            }
            Err(err) => {
                eprintln!("listener wake failed to start: {err}");
                let _ = mark_unhealthy(&client.state_dir());
                mail.note_failure(Instant::now());
            }
        }
    }
}

fn touch(client: &Client, token: &str) {
    if let Err(err) = client.call(Some(token), "touch", serde_json::json!([])) {
        eprintln!("listener touch failed: {}", redact(&err, token));
    }
}

fn spawn_wake(root: &Path, argv: &[String]) -> Result<Child, String> {
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .current_dir(root)
        .stdin(Stdio::null());
    command.spawn().map_err(|err| err.to_string())
}

fn websocket_url(host: &str, database: &str) -> Result<String, String> {
    crate::client::validate_endpoint(host, database)?;
    let base = host.trim().trim_end_matches('/');
    let ws = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        return Err("host must be http or https".into());
    };
    Ok(format!("{ws}/v1/database/{database}/subscribe"))
}

fn subscribe_message() -> String {
    serde_json::json!({
        "Subscribe": {
            "query_strings": [INBOX_QUERY],
            "request_id": 1
        }
    })
    .to_string()
}

fn set_read_timeout(
    stream: &mut MaybeTlsStream<TcpStream>,
    timeout: Duration,
) -> Result<(), String> {
    let tcp = match stream {
        MaybeTlsStream::Plain(tcp) => tcp,
        MaybeTlsStream::Rustls(tls) => tls.get_mut(),
        _ => return Ok(()),
    };
    tcp.set_read_timeout(Some(timeout))
        .map_err(|err| err.to_string())
}

fn redact(text: &str, token: &str) -> String {
    if token.is_empty() {
        text.to_string()
    } else {
        text.replace(token, "[token]")
    }
}

#[derive(Debug)]
struct MailState {
    outstanding: u64,
    child: Option<Child>,
    backoff: Duration,
    retry_at: Option<Instant>,
    unhealthy: bool,
}

impl MailState {
    fn new() -> Self {
        Self {
            outstanding: 0,
            child: None,
            backoff: WAKE_BACKOFF_START,
            retry_at: None,
            unhealthy: false,
        }
    }

    fn on_snapshot(&mut self, count: u64) {
        self.outstanding = count;
    }

    fn on_delta(&mut self, inserts: u64, deletes: u64) {
        self.outstanding = self
            .outstanding
            .saturating_add(inserts)
            .saturating_sub(deletes);
    }

    fn poll_child(&mut self) -> Option<bool> {
        let mut child = self.child.take()?;
        match child.try_wait() {
            Ok(Some(status)) => {
                let success = status.success();
                if success {
                    self.note_success();
                } else {
                    eprintln!("listener wake exited {}", status.code().unwrap_or(1));
                    self.note_failure(Instant::now());
                }
                Some(success)
            }
            Ok(None) => {
                self.child = Some(child);
                None
            }
            Err(err) => {
                eprintln!("listener wake failed: {err}");
                self.note_failure(Instant::now());
                Some(false)
            }
        }
    }

    fn note_success(&mut self) {
        self.backoff = WAKE_BACKOFF_START;
        self.retry_at = None;
        self.unhealthy = false;
    }

    fn note_failure(&mut self, now: Instant) {
        self.unhealthy = true;
        self.retry_at = Some(now + self.backoff);
        self.backoff = (self.backoff * 2).min(WAKE_BACKOFF_MAX);
    }

    fn wants_wake(&self, now: Instant) -> bool {
        if self.child.is_some() || self.outstanding == 0 {
            return false;
        }
        self.retry_at.map(|at| now >= at).unwrap_or(true)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SocketEvent {
    Identity,
    Snapshot(u64),
    Delta { inserts: u64, deletes: u64 },
    SubscribeFailed(String),
    Ignore,
}

fn classify_message(text: &str) -> Result<SocketEvent, String> {
    let value: Value = serde_json::from_str(text).map_err(|err| err.to_string())?;
    if value.get("IdentityToken").is_some() {
        return Ok(SocketEvent::Identity);
    }
    if let Some(subscription) = value.get("InitialSubscription") {
        let tables = subscription.pointer("/database_update/tables");
        return Ok(SocketEvent::Snapshot(inbox_inserts(tables)));
    }
    if let Some(update) = value.get("TransactionUpdateLight") {
        let (inserts, deletes) = inbox_delta(update.pointer("/update/tables"));
        return Ok(SocketEvent::Delta { inserts, deletes });
    }
    if let Some(update) = value.get("TransactionUpdate") {
        let (inserts, deletes) = inbox_delta(update.pointer("/status/Committed/tables"));
        if inserts == 0 && deletes == 0 && update.pointer("/status/Committed").is_none() {
            return Ok(SocketEvent::Ignore);
        }
        return Ok(SocketEvent::Delta { inserts, deletes });
    }
    if let Some(error) = value.get("SubscriptionError") {
        let reason = error
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("subscription failed");
        return Ok(SocketEvent::SubscribeFailed(reason.to_string()));
    }
    Ok(SocketEvent::Ignore)
}

fn inbox_inserts(tables: Option<&Value>) -> u64 {
    inbox_delta(tables).0
}

fn inbox_delta(tables: Option<&Value>) -> (u64, u64) {
    let Some(tables) = tables.and_then(Value::as_array) else {
        return (0, 0);
    };
    let mut inserts = 0u64;
    let mut deletes = 0u64;
    for table in tables {
        if table.get("table_name").and_then(Value::as_str) != Some("my_inbox") {
            continue;
        }
        let Some(updates) = table.get("updates").and_then(Value::as_array) else {
            continue;
        };
        for update in updates {
            inserts += count_rows(update.get("inserts"));
            deletes += count_rows(update.get("deletes"));
        }
    }
    (inserts, deletes)
}

fn count_rows(value: Option<&Value>) -> u64 {
    value
        .and_then(Value::as_array)
        .map(|rows| rows.len() as u64)
        .unwrap_or(0)
}

fn load_wake_argv(state_dir: &Path) -> Result<Vec<String>, String> {
    let path = state_dir.join("wake.json");
    let text = fs::read_to_string(&path).map_err(|_| {
        "no .agent-msg/wake.json; the skill writes that file before listen".to_string()
    })?;
    let value: Value = serde_json::from_str(&text).map_err(|err| err.to_string())?;
    let argv = value
        .get("argv")
        .and_then(Value::as_array)
        .ok_or("wake.json argv must be an array of strings")?;
    argv.iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| "wake.json argv must be an array of strings".to_string())
        })
        .collect()
}

pub(crate) fn validate_wake_argv(root: &Path, argv: &[String]) -> Result<(), String> {
    if argv.is_empty() {
        return Err("wake command is empty".into());
    }
    for arg in argv {
        if arg.is_empty() || arg.contains('\0') {
            return Err("wake command has an empty argument".into());
        }
    }
    let program = &argv[0];
    if !program.contains('/') && !program.contains('\\') {
        if program == "." || program == ".." {
            return Err("wake program must be a command name or a file under .agent-msg".into());
        }
        return Ok(());
    }
    let path = if Path::new(program).is_absolute() {
        PathBuf::from(program)
    } else {
        root.join(program)
    };
    let canonical = fs::canonicalize(&path)
        .map_err(|_| "wake program path must exist under .agent-msg".to_string())?;
    let state = fs::canonicalize(root.join(".agent-msg"))
        .map_err(|_| "no .agent-msg directory".to_string())?;
    if !canonical.starts_with(&state) {
        return Err("wake program must be a command name or a file under .agent-msg".into());
    }
    Ok(())
}

struct PidGuard {
    path: PathBuf,
    pid: u32,
}

impl Drop for PidGuard {
    fn drop(&mut self) {
        if read_pid(&self.path) == Some(self.pid) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn claim_pid(state_dir: &Path) -> Result<PidGuard, String> {
    let path = state_dir.join("listener.pid");
    if let Some(pid) = read_pid(&path) {
        if process_alive(pid) {
            return Err("listener already running".into());
        }
    }
    let pid = std::process::id();
    write_private(&path, format!("{pid}\n").as_bytes())?;
    Ok(PidGuard { path, pid })
}

fn read_pid(path: &Path) -> Option<u32> {
    let text = fs::read_to_string(path).ok()?;
    text.trim().parse().ok()
}

fn process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .stdin(Stdio::null())
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

fn write_heartbeat(state_dir: &Path) -> Result<(), String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    write_private(
        &state_dir.join("listener-heartbeat"),
        format!("{seconds}\n").as_bytes(),
    )
}

fn mark_unhealthy(state_dir: &Path) -> Result<(), String> {
    write_private(
        &state_dir.join("listener-unhealthy"),
        format!("{WAKE_FAILED}\n").as_bytes(),
    )
}

fn clear_unhealthy(state_dir: &Path) {
    let _ = fs::remove_file(state_dir.join("listener-unhealthy"));
}

#[cfg(test)]
fn heartbeat_is_fresh(seconds: u64, now: SystemTime) -> bool {
    let now_seconds = now
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    now_seconds.saturating_sub(seconds) < HEARTBEAT_MAX_AGE_SECS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_snapshot_and_delta_without_reading_row_text() {
        let snapshot = r#"{"InitialSubscription":{"database_update":{"tables":[{"table_name":"my_inbox","updates":[{"deletes":[],"inserts":["secret-row"]}]}]}}}"#;
        assert_eq!(
            classify_message(snapshot).unwrap(),
            SocketEvent::Snapshot(1)
        );
        let insert = r#"{"TransactionUpdateLight":{"update":{"tables":[{"table_name":"my_inbox","updates":[{"deletes":[],"inserts":["secret-row"]}]}]}}}"#;
        assert_eq!(
            classify_message(insert).unwrap(),
            SocketEvent::Delta {
                inserts: 1,
                deletes: 0
            }
        );
        let delete = r#"{"TransactionUpdateLight":{"update":{"tables":[{"table_name":"other","updates":[{"deletes":["x"],"inserts":[]}]},{"table_name":"my_inbox","updates":[{"deletes":["secret-row"],"inserts":[]}]}]}}}"#;
        assert_eq!(
            classify_message(delete).unwrap(),
            SocketEvent::Delta {
                inserts: 0,
                deletes: 1
            }
        );
    }

    #[test]
    fn identity_token_is_not_a_row_event() {
        let text = r#"{"IdentityToken":{"token":"secret"}}"#;
        assert_eq!(classify_message(text).unwrap(), SocketEvent::Identity);
    }

    #[test]
    fn outstanding_follows_snapshot_then_ack() {
        let mut mail = MailState::new();
        mail.on_snapshot(2);
        assert!(mail.wants_wake(Instant::now()));
        mail.on_delta(1, 0);
        mail.on_delta(0, 3);
        assert_eq!(mail.outstanding, 0);
        assert!(!mail.wants_wake(Instant::now()));
    }

    #[test]
    fn failed_wake_waits_and_success_clears_the_wait() {
        let mut mail = MailState::new();
        mail.on_snapshot(1);
        let now = Instant::now();
        mail.note_failure(now);
        assert!(!mail.wants_wake(now));
        assert!(mail.unhealthy);
        assert!(mail.wants_wake(now + WAKE_BACKOFF_START));
        mail.note_success();
        assert!(!mail.unhealthy);
        assert!(mail.wants_wake(now));
    }

    #[test]
    fn a_running_child_blocks_another_wake() {
        let mut mail = MailState::new();
        mail.on_snapshot(1);
        mail.child = Some(Command::new("sleep").arg("30").spawn().unwrap());
        assert!(!mail.wants_wake(Instant::now()));
        let _ = mail.child.take().unwrap().kill();
    }

    #[test]
    fn wake_program_stays_under_the_state_dir() {
        let root = std::env::temp_dir().join(format!("amr-wake-{}", std::process::id()));
        let state = root.join(".agent-msg");
        fs::create_dir_all(&state).unwrap();
        let outside = std::env::temp_dir().join(format!("amr-outside-{}", std::process::id()));
        fs::write(&outside, b"echo\n").unwrap();
        assert!(validate_wake_argv(&root, &[outside.display().to_string()]).is_err());
        assert!(validate_wake_argv(&root, &["agent".into(), "-p".into(), "check".into()]).is_ok());
        let script = state.join("wake.sh");
        fs::write(&script, b"echo\n").unwrap();
        assert!(validate_wake_argv(&root, &[script.display().to_string()]).is_ok());
        assert!(validate_wake_argv(&root, &["../wake.sh".into()]).is_err());
        let _ = fs::remove_file(&outside);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn heartbeat_is_fresh_inside_two_minutes() {
        let now = SystemTime::now();
        let seconds = now.duration_since(UNIX_EPOCH).unwrap().as_secs();
        assert!(heartbeat_is_fresh(seconds, now));
        assert!(!heartbeat_is_fresh(seconds.saturating_sub(120), now));
    }

    #[test]
    fn websocket_url_uses_the_validated_host() {
        assert_eq!(
            websocket_url("https://maincloud.spacetimedb.com", "agent-msg-relay").unwrap(),
            "wss://maincloud.spacetimedb.com/v1/database/agent-msg-relay/subscribe"
        );
        assert!(websocket_url("https://user:pass@example.com", "db").is_err());
        assert!(websocket_url("http://example.com", "db").is_err());
    }
}
