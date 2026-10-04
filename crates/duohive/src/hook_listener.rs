//! The app's end of `run.sock`: one JSON `HookMessage` per line from `duohive hook`.
//!
//! Review Focus 5: a second instance refuses to bind while the socket answers,
//! and a stale socket file from a crash is replaced. Tokens are forwarded to the
//! engine (which checks them) and never logged.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener as StdUnixListener, UnixStream as StdUnixStream};
use std::path::Path;
use std::sync::{Arc, PoisonError, RwLock};

use duohive_core::adapter::Adapters;
use duohive_core::cli_kind::CliKind;
use duohive_core::engine::Input;
use duohive_core::hook::HookMessage;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

/// The longest line accepted; longer ones are dropped with a warning.
const MAX_LINE: usize = 1024 * 1024;
const SOCK_MODE: u32 = 0o600;

/// Each agent's CLI, refreshed by the binary on `OrgChanged`.
pub type AgentClis = Arc<RwLock<HashMap<String, CliKind>>>;

/// The socket could not be taken.
#[derive(Debug, thiserror::Error)]
pub enum ListenerError {
    /// Another DuoHive answers on the socket.
    #[error("another DuoHive is already running (run.sock answers)")]
    AlreadyRunning,
    /// Removing, binding or restricting the socket failed.
    #[error("cannot listen on run.sock: {0}")]
    Io(#[from] io::Error),
}

/// A bound, owner-only `run.sock`.
#[derive(Debug)]
pub struct HookListener {
    listener: StdUnixListener,
}

impl HookListener {
    /// Binds `sock`: refuses when another instance answers on it, replaces a stale one.
    pub fn bind(sock: &Path) -> Result<HookListener, ListenerError> {
        if fs::symlink_metadata(sock).is_ok() {
            if StdUnixStream::connect(sock).is_ok() {
                return Err(ListenerError::AlreadyRunning);
            }
            fs::remove_file(sock)?;
        }
        let listener = StdUnixListener::bind(sock)?;
        fs::set_permissions(sock, fs::Permissions::from_mode(SOCK_MODE))?;
        listener.set_nonblocking(true)?;
        Ok(HookListener { listener })
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
                    Err(e) => tracing::warn!(error = %e, "hook listener: accept failed"),
                },
            }
        }
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

/// Reads up to the next `\n` (dropped); a line over `MAX_LINE` is skipped to its end.
async fn read_line(reader: &mut (impl AsyncBufRead + Unpin)) -> io::Result<Option<Line>> {
    let mut line = Vec::new();
    let mut too_long = false;
    loop {
        let buf = reader.fill_buf().await?;
        if buf.is_empty() {
            return Ok(match (too_long, line.is_empty()) {
                (true, _) => Some(Line::TooLong),
                (false, true) => None,
                (false, false) => Some(Line::Text(line)),
            });
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
            vec![
                Some(b"first line".to_vec()),
                Some(b"second".to_vec()),
                Some(b"last".to_vec())
            ]
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
