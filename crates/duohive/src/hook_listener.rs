//! The app's end of `run.sock`: one JSON `HookMessage` per line from `duohive hook`.
//!
//! Review Focus 5: liveness is an exclusive lock on `run.lock`, held for the
//! listener's lifetime. A second instance finds it held and refuses; a socket
//! file with no lock holder is stale (a crash) and is replaced. Tokens are
//! forwarded to the engine (which checks them) and never logged.

use std::collections::HashMap;
use std::fs::{self, DirBuilder, File, OpenOptions, TryLockError};
use std::io;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixListener as StdUnixListener;
use std::path::Path;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use duohive_core::adapter::Adapters;
use duohive_core::cli_kind::CliKind;
use duohive_core::engine::Input;
use duohive_core::hook::HookMessage;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

use crate::MAX_HOOK_LINE as MAX_LINE;

const DIR_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;
const LOCK_FILE: &str = "run.lock";
/// A pause after a failed accept (e.g. out of file descriptors), so the loop never spins.
const ACCEPT_RETRY: Duration = Duration::from_millis(100);

/// Each agent's CLI, refreshed by the binary on `OrgChanged`.
pub type AgentClis = Arc<RwLock<HashMap<String, CliKind>>>;

/// The socket could not be taken.
#[derive(Debug, thiserror::Error)]
pub enum ListenerError {
    /// Another DuoHive holds the instance lock.
    #[error("another DuoHive is already running (run.lock is held)")]
    AlreadyRunning,
    /// Creating the directory, locking, removing, binding or restricting failed.
    #[error("cannot listen on run.sock: {0}")]
    Io(#[from] io::Error),
}

/// A bound, owner-only `run.sock`, and the instance lock that makes it ours.
#[derive(Debug)]
pub struct HookListener {
    listener: StdUnixListener,
    /// Held (locked) until the listener is dropped.
    lock: File,
}

impl HookListener {
    /// Takes the instance lock (`run.lock` beside `sock`), then binds `sock`,
    /// replacing a stale socket file; refuses when another instance holds the lock.
    ///
    /// Call this before starting anything that spawns processes (tmux, CLI
    /// `--version` detection): on macOS std marks a socket close-on-exec only
    /// after creating it, so a process spawned meanwhile would inherit the
    /// listener. (The lock file is opened close-on-exec atomically.)
    pub fn bind(sock: &Path) -> Result<HookListener, ListenerError> {
        let dir = sock.parent().unwrap_or(Path::new("."));
        ensure_private_dir(dir)?;
        let lock = take_lock(&dir.join(LOCK_FILE))?;
        // The lock is ours, so whatever socket sits at the path is stale.
        match fs::symlink_metadata(sock) {
            Ok(meta) if meta.file_type().is_socket() => fs::remove_file(sock)?,
            Ok(_) => {
                return Err(ListenerError::Io(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{} exists and is not a socket", sock.display()),
                )));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let listener = StdUnixListener::bind(sock)?;
        fs::set_permissions(sock, fs::Permissions::from_mode(FILE_MODE))?;
        listener.set_nonblocking(true)?;
        Ok(HookListener { listener, lock })
    }

    /// Accepts connections until the core's channel closes. Each line is parsed,
    /// enriched by the agent's adapter off the executor, and sent to the core
    /// (and to `tap` when set).
    pub async fn serve(
        self,
        agents: AgentClis,
        adapters: Arc<Adapters>,
        to_core: mpsc::Sender<Input>,
        tap: Option<mpsc::UnboundedSender<HookMessage>>,
    ) {
        let _lock = self.lock; // held while serving
        let listener = match UnixListener::from_std(self.listener) {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(error = %e, "hook listener cannot start");
                return;
            }
        };
        let route = Route {
            agents,
            adapters,
            to_core,
            tap,
        };
        loop {
            tokio::select! {
                () = route.to_core.closed() => return,
                accepted = listener.accept() => match accepted {
                    Ok((stream, _)) => {
                        tokio::spawn(route.clone().connection(stream));
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "hook listener: accept failed");
                        tokio::time::sleep(ACCEPT_RETRY).await;
                    }
                },
            }
        }
    }
}

/// Creates `dir` (0700) or tightens it to 0700.
fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent)?;
    }
    match DirBuilder::new().mode(DIR_MODE).create(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            fs::set_permissions(dir, fs::Permissions::from_mode(DIR_MODE))
        }
        Err(e) => Err(e),
    }
}

/// Opens `run.lock` (0600) and locks it exclusively without waiting.
fn take_lock(path: &Path) -> Result<File, ListenerError> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(FILE_MODE)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(FILE_MODE))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(ListenerError::AlreadyRunning),
        Err(TryLockError::Error(e)) => Err(e.into()),
    }
}

/// Where a connection's hooks go.
#[derive(Clone)]
struct Route {
    agents: AgentClis,
    adapters: Arc<Adapters>,
    to_core: mpsc::Sender<Input>,
    tap: Option<mpsc::UnboundedSender<HookMessage>>,
}

/// One line, or a line that was too long and was skipped.
enum Line {
    Text(Vec<u8>),
    TooLong,
}

impl Route {
    async fn connection(self, stream: UnixStream) {
        let mut reader = BufReader::new(stream);
        loop {
            match read_line(&mut reader).await {
                Ok(Some(Line::Text(bytes))) => {
                    if !self.handle(&bytes).await {
                        return;
                    }
                }
                Ok(Some(Line::TooLong)) => {
                    tracing::warn!(max = MAX_LINE, "hook line over the limit dropped");
                }
                Ok(None) => return,
                Err(e) => {
                    tracing::warn!(error = %e, "hook connection read failed");
                    return;
                }
            }
        }
    }

    /// Forwards one line; `false` once the core has gone.
    async fn handle(&self, bytes: &[u8]) -> bool {
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return true;
        }
        let msg: HookMessage = match serde_json::from_slice(bytes) {
            Ok(m) => m,
            Err(e) => {
                // The line may hold a token: log only the parse error.
                tracing::warn!(error = %e, "hook line is not a hook message");
                return true;
            }
        };
        let msg = self.enrich(msg).await;
        if let Some(tap) = &self.tap {
            let _ = tap.send(msg.clone());
        }
        self.to_core.send(Input::Hook(msg)).await.is_ok()
    }

    /// Runs the agent's adapter `enrich_hook` on the blocking pool (it may read a transcript).
    async fn enrich(&self, msg: HookMessage) -> HookMessage {
        let kind = self
            .agents
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&msg.agent)
            .copied();
        let Some(kind) = kind else {
            return msg;
        };
        let adapters = Arc::clone(&self.adapters);
        let fallback = msg.clone();
        let enriched = tokio::task::spawn_blocking(move || {
            let mut msg = msg;
            if let Some(adapter) = adapters.get(kind) {
                adapter.enrich_hook(&msg.event, &mut msg.payload);
            }
            msg
        })
        .await;
        enriched.unwrap_or_else(|e| {
            tracing::warn!(error = %e, agent = %fallback.agent, "hook enrichment failed");
            fallback
        })
    }
}

/// Reads up to the next `\n` (dropped); a line over `MAX_LINE` is skipped to its
/// end; an unterminated last line is dropped.
async fn read_line(reader: &mut (impl AsyncBufRead + Unpin)) -> io::Result<Option<Line>> {
    let mut line = Vec::new();
    let mut too_long = false;
    loop {
        let buf = reader.fill_buf().await?;
        if buf.is_empty() {
            // An unterminated last line was not fully delivered (the sender
            // gave up and queued the hook instead): drop it.
            if too_long || !line.is_empty() {
                tracing::warn!("unterminated hook line dropped");
            }
            return Ok(None);
        }
        let (chunk, found) = match buf.iter().position(|&b| b == b'\n') {
            Some(i) => (&buf[..i], Some(i + 1)),
            None => (buf, None),
        };
        if !too_long {
            if line.len() + chunk.len() > MAX_LINE {
                too_long = true;
                line = Vec::new();
            } else {
                line.extend_from_slice(chunk);
            }
        }
        let used = found.unwrap_or(buf.len());
        reader.consume(used);
        if found.is_some() {
            return Ok(Some(if too_long {
                Line::TooLong
            } else {
                Line::Text(line)
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn lines(input: &[u8]) -> Vec<Option<Vec<u8>>> {
        let mut r = BufReader::with_capacity(8, input);
        let mut out = Vec::new();
        while let Some(l) = read_line(&mut r).await.unwrap() {
            out.push(match l {
                Line::Text(t) => Some(t),
                Line::TooLong => None,
            });
        }
        out
    }

    #[tokio::test]
    async fn lines_split_across_reads() {
        assert_eq!(
            lines(b"first line\nsecond\nlast").await,
            // An unterminated last line was never fully delivered: dropped.
            vec![Some(b"first line".to_vec()), Some(b"second".to_vec())]
        );
    }

    #[tokio::test]
    async fn long_line_skipped_to_its_end() {
        let mut input = vec![b'a'; MAX_LINE + 1];
        input.extend_from_slice(b"\nok\n");
        assert_eq!(lines(&input).await, vec![None, Some(b"ok".to_vec())]);
        let mut exact = vec![b'b'; MAX_LINE];
        exact.push(b'\n');
        assert_eq!(lines(&exact).await, vec![Some(vec![b'b'; MAX_LINE])]);
    }
}
