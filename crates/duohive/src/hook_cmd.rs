//! `duohive hook <agent> <event>`: forwards one CLI hook to the running app.
//!
//! Review Focus 1: this runs inside the agent's CLI, so it never prints, always
//! exits 0, and finishes within 1 s whatever the state of DuoHive. Budget:
//! stdin 300 ms, connect 200 ms + write 150 ms, database fallback
//! `busy_timeout` 200 ms, then one append to `logs/hook.log`. Std only.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::ExitCode;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use duohive_core::hook::HookMessage;
use duohive_core::time::Millis;
use duohive_store::Paths;
use duohive_store::db::{insert_pending_hook, open_db_for_hook};
use serde_json::{Map, Value, json};

use crate::MAX_HOOK_LINE;
use crate::args::{Cli, parse_args};
use crate::logging::ensure_logs_dir;

/// How long a CLI may leave stdin open and silent.
const STDIN_DEADLINE: Duration = Duration::from_millis(300);
/// Reading stops here; anything longer is cut (and then wrapped as raw text).
/// The socket line is capped lower, at [`MAX_HOOK_LINE`], by shrinking the payload.
const STDIN_CAP: usize = 4 * 1024 * 1024;
const CONNECT_DEADLINE: Duration = Duration::from_millis(200);
const WRITE_DEADLINE: Duration = Duration::from_millis(150);
const DB_BUSY_TIMEOUT: Duration = Duration::from_millis(200);
const LOG_FILE_MODE: u32 = 0o600;
/// At 1 MiB `hook.log` moves to `hook.log.1` (replacing it) and starts over: it only
/// takes hooks that reached neither the socket nor the database.
const HOOK_LOG_CAP: u64 = 1024 * 1024;
/// Payload fields a shrunk payload keeps: enrichment and the engine need them.
const KEEP_FIELDS: [&str; 3] = ["transcript_path", "session_id", "hook_event_name"];
const TRUNCATED: &str = "duohive_truncated";

/// Runs `duohive hook <agent> <event>`. Reads nothing from the process
/// environment: `env` holds the variables. Always exits 0 and prints nothing.
pub fn run_hook(
    args: &[String],
    stdin: impl Read + Send + 'static,
    stdin_is_terminal: bool,
    env: &HashMap<String, String>,
) -> ExitCode {
    let Ok(Cli::Hook { agent, event }) = parse_args(args) else {
        return ExitCode::SUCCESS;
    };
    let Some(token) = env.get("DUOHIVE_TOKEN").filter(|t| !t.is_empty()) else {
        return ExitCode::SUCCESS;
    };
    let Ok(paths) = Paths::from_vars(env) else {
        return ExitCode::SUCCESS;
    };
    let payload = if stdin_is_terminal {
        Value::Null
    } else {
        parse_payload(&read_stdin(stdin))
    };
    // Stamped before any delivery attempt: replay orders by it (Review Focus 3).
    let msg = HookMessage {
        sent_at_ms: now().0,
        token: token.clone(),
        agent,
        event,
        payload,
    };
    deliver(&paths, &msg);
    ExitCode::SUCCESS
}

/// Socket, else database, else one log line; every failure is swallowed.
fn deliver(paths: &Paths, msg: &HookMessage) {
    let Err(sock_err) = send_to_socket(&paths.run_sock(), msg) else {
        return;
    };
    let Err(db_err) = queue_in_db(paths, msg) else {
        return;
    };
    // Never the token: the log is for the user to read.
    let line = format!(
        "{} hook {} {} not delivered: socket: {sock_err}; database: {db_err}\n",
        now().0,
        msg.agent,
        msg.event,
    );
    let _ = append_log(&paths.logs_dir(), &line);
}

/// Reads stdin to EOF (capped) on a helper thread; after the deadline, takes
/// what has arrived and leaves the thread blocked (the process exits).
fn read_stdin(mut stdin: impl Read + Send + 'static) -> Vec<u8> {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let shared = Arc::clone(&buf);
    let (done_tx, done_rx) = mpsc::channel();
    let spawned = thread::Builder::new()
        .name("hook-stdin".to_owned())
        .spawn(move || {
            let mut chunk = [0_u8; 64 * 1024];
            loop {
                match stdin.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut b = shared.lock().unwrap_or_else(PoisonError::into_inner);
                        let room = STDIN_CAP.saturating_sub(b.len());
                        b.extend_from_slice(&chunk[..n.min(room)]);
                        if b.len() >= STDIN_CAP {
                            break;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            let _ = done_tx.send(());
        });
    if spawned.is_err() {
        return Vec::new();
    }
    let _ = done_rx.recv_timeout(STDIN_DEADLINE);
    buf.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// Empty → `null`; JSON as is; anything else → `{"raw": "<text>"}`.
fn parse_payload(bytes: &[u8]) -> Value {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Value::Null;
    }
    serde_json::from_slice(bytes)
        .unwrap_or_else(|_| json!({ "raw": String::from_utf8_lossy(bytes) }))
}

/// Connects with a deadline (std's connect has none, so a helper thread does it).
fn connect(sock: &Path) -> io::Result<UnixStream> {
    let (tx, rx) = mpsc::channel();
    let path = sock.to_path_buf();
    thread::Builder::new()
        .name("hook-connect".to_owned())
        .spawn(move || {
            let _ = tx.send(UnixStream::connect(path));
        })?;
    rx.recv_timeout(CONNECT_DEADLINE)
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "connect timed out"))?
}

fn send_to_socket(sock: &Path, msg: &HookMessage) -> io::Result<()> {
    let mut line = fit_line(msg)?;
    line.push(b'\n');
    let mut stream = connect(sock)?;
    write_with_deadline(&mut stream, &line, Instant::now() + WRITE_DEADLINE)
}

/// The message as one JSON line of at most [`MAX_HOOK_LINE`] bytes. Too long:
/// the largest payload fields go (never [`KEEP_FIELDS`]) and `duohive_truncated`
/// is set, so the event itself always arrives.
fn fit_line(msg: &HookMessage) -> io::Result<Vec<u8>> {
    let line = serde_json::to_vec(msg)?;
    if line.len() <= MAX_HOOK_LINE {
        return Ok(line);
    }
    let fields = match &msg.payload {
        Value::Object(m) => m.clone(),
        _ => Map::new(),
    };
    let mut sized: Vec<(String, usize)> = fields
        .iter()
        .filter(|(k, _)| !KEEP_FIELDS.contains(&k.as_str()))
        // Key, quotes, colon and comma, then the value.
        .map(|(k, v)| {
            (
                k.clone(),
                k.len() + 4 + serde_json::to_vec(v).map_or(0, |b| b.len()),
            )
        })
        .collect();
    sized.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
    // Room for the `"duohive_truncated":true` marker too.
    let mut over = (line.len() + TRUNCATED.len() + 8).saturating_sub(MAX_HOOK_LINE);
    let drop: Vec<String> = sized
        .into_iter()
        .take_while(|(_, size)| {
            let needed = over > 0;
            over = over.saturating_sub(*size);
            needed
        })
        .map(|(k, _)| k)
        .collect();
    let mut kept: Map<String, Value> = fields
        .into_iter()
        .filter(|(k, _)| !drop.contains(k))
        .collect();
    kept.insert(TRUNCATED.to_owned(), Value::Bool(true));
    for payload in [Value::Object(kept), json!({ TRUNCATED: true })] {
        let shrunk = HookMessage {
            payload,
            ..msg.clone()
        };
        let line = serde_json::to_vec(&shrunk)?;
        if line.len() <= MAX_HOOK_LINE {
            return Ok(line);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "hook too large to send",
    ))
}

/// `write_all` bounded by one overall deadline, not one per `write`.
fn write_with_deadline(
    stream: &mut UnixStream,
    mut data: &[u8],
    deadline: Instant,
) -> io::Result<()> {
    while !data.is_empty() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "write timed out"));
        }
        stream.set_write_timeout(Some(left))?;
        match stream.write(data) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) => data = &data[n..],
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn queue_in_db(paths: &Paths, msg: &HookMessage) -> Result<(), String> {
    let conn = open_db_for_hook(&paths.state_db(), DB_BUSY_TIMEOUT).map_err(|e| e.to_string())?;
    insert_pending_hook(&conn, msg, now()).map_err(|e| e.to_string())
}

fn append_log(dir: &Path, line: &str) -> io::Result<()> {
    ensure_logs_dir(dir)?;
    let path = dir.join("hook.log");
    if fs::metadata(&path).is_ok_and(|m| m.len() >= HOOK_LOG_CAP) {
        fs::rename(&path, dir.join("hook.log.1"))?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(LOG_FILE_MODE)
        .open(path)?
        .write_all(line.as_bytes())
}

fn now() -> Millis {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    Millis(ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payloads_parse_or_wrap() {
        assert_eq!(parse_payload(b""), Value::Null);
        assert_eq!(parse_payload(b" \n"), Value::Null);
        assert_eq!(parse_payload(b"{\"a\":1}"), json!({"a": 1}));
        assert_eq!(parse_payload(b"oops"), json!({"raw": "oops"}));
    }

    #[test]
    fn stdin_is_capped() {
        let big = vec![b'x'; STDIN_CAP + 10];
        assert_eq!(read_stdin(io::Cursor::new(big)).len(), STDIN_CAP);
    }

    #[test]
    fn undelivered_hook_is_logged_without_its_token() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config_dir: tmp.path().join("c"),
            data_dir: tmp.path().to_path_buf(),
            tmux_socket: "duohive-test-x".to_owned(),
        };
        // No listener, and a directory where state.db should be: both routes fail.
        fs::create_dir(paths.state_db()).unwrap();
        let msg = HookMessage {
            sent_at_ms: 0,
            token: "secret-token".to_owned(),
            agent: "pay-qa".to_owned(),
            event: "busy".to_owned(),
            payload: Value::Null,
        };
        deliver(&paths, &msg);
        let log = fs::read_to_string(paths.logs_dir().join("hook.log")).unwrap();
        assert_eq!(log.lines().count(), 1);
        assert!(log.contains("hook pay-qa busy not delivered"), "{log}");
        assert!(!log.contains("secret-token"), "{log}");
    }

    #[test]
    fn hook_log_is_owner_only_and_rotates() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("logs");
        append_log(&dir, "first\n").unwrap();
        let mode = fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        let full = "x".repeat(usize::try_from(HOOK_LOG_CAP).unwrap());
        fs::write(dir.join("hook.log"), &full).unwrap();
        append_log(&dir, "next\n").unwrap();
        // A full hook.log moves to hook.log.1 (replacing the older one) and starts over.
        assert_eq!(fs::read_to_string(dir.join("hook.log.1")).unwrap(), full);
        assert_eq!(fs::read_to_string(dir.join("hook.log")).unwrap(), "next\n");
    }
}
