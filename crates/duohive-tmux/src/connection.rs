//! The control-mode connection: one `tmux -C attach-session` child, replies matched
//! FIFO to `%begin`/`%end` blocks, reattach when the client drops but the server lives.

use std::collections::VecDeque;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::control::{ControlLine, ControlParser};
use crate::server::{self, TmuxConfig, TmuxError};

/// The S0.5 silencing (D25), sent on attach and on every reattach.
const SILENCE: &str = "refresh-client -f no-output,ignore-size";

/// How long the attach's own reply block may take.
const HANDSHAKE: Duration = Duration::from_secs(5);

/// Why a command got no reply.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CmdError {
    /// tmux answered `%error` with this text.
    #[error("{0}")]
    Tmux(String),
    /// The control client went away before replying.
    #[error("tmux connection closed")]
    Closed,
}

/// What the connection tells the runtime besides replies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Notice {
    /// `%unlinked-window-close` or `%window-close`.
    WindowClose(String),
    /// A new control client replaced a lost one; the server lived on.
    Reattached,
    /// The server is gone or the reattach failed.
    Lost(String),
}

type Reply = Result<Vec<String>, CmdError>;

struct Request {
    line: String,
    reply: oneshot::Sender<Reply>,
}

/// A handle that sends commands to the control client.
#[derive(Clone)]
pub(crate) struct Conn {
    tx: mpsc::UnboundedSender<Request>,
}

impl Conn {
    /// Sends one command line and waits for its reply block's body.
    pub async fn cmd(&self, line: String) -> Result<Vec<String>, CmdError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Request { line, reply })
            .map_err(|_| CmdError::Closed)?;
        rx.await.unwrap_or(Err(CmdError::Closed))
    }

    /// True once the connection task has ended for good.
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

/// One attached control client.
struct Client {
    _child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    parser: ControlParser,
    /// Replies owed, in the order the commands were written; `None` for internal ones.
    owed: VecDeque<Option<oneshot::Sender<Reply>>>,
}

impl Client {
    /// Attaches, consumes the attach's own reply block, then silences output.
    async fn open(cfg: &TmuxConfig) -> Result<Self, TmuxError> {
        let mut child = server::tmux(cfg)
            .args(["-C", "attach-session", "-t", &server::exact(cfg)])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(TmuxError::Attach("no pipes".into()));
        };
        let mut client = Self {
            _child: child,
            stdin,
            lines: BufReader::new(stdout).lines(),
            parser: ControlParser::default(),
            owed: VecDeque::new(),
        };
        tokio::time::timeout(HANDSHAKE, client.handshake())
            .await
            .map_err(|_| TmuxError::Attach("no reply to attach".into()))??;
        client.write(SILENCE, None).await?;
        Ok(client)
    }

    async fn handshake(&mut self) -> Result<(), TmuxError> {
        loop {
            let Some(line) = self.lines.next_line().await? else {
                return Err(TmuxError::Attach("control client exited".into()));
            };
            match self.parser.feed(&line) {
                ControlLine::End { .. } => return Ok(()),
                ControlLine::Error { .. } => return Err(TmuxError::Attach(line)),
                _ => {}
            }
        }
    }

    async fn write(
        &mut self,
        line: &str,
        reply: Option<oneshot::Sender<Reply>>,
    ) -> std::io::Result<()> {
        self.stdin.write_all(format!("{line}\n").as_bytes()).await?;
        self.stdin.flush().await?;
        self.owed.push_back(reply);
        Ok(())
    }

    fn settle(&mut self, reply: Reply) {
        if let Some(Some(tx)) = self.owed.pop_front() {
            let _ = tx.send(reply);
        }
    }

    /// Serves requests and reads lines until the client ends; returns why.
    async fn serve(
        &mut self,
        requests: &mut mpsc::UnboundedReceiver<Request>,
        notices: &mpsc::UnboundedSender<Notice>,
    ) -> Option<String> {
        let mut body = Vec::new();
        let mut exit_reason = None;
        loop {
            tokio::select! {
                req = requests.recv() => {
                    let Request { line, reply } = req?;
                    if let Err(e) = self.write(&line, Some(reply)).await {
                        return Some(format!("write failed: {e}"));
                    }
                }
                line = self.lines.next_line() => {
                    let line = match line {
                        Ok(Some(line)) => line,
                        Ok(None) => return Some(exit_reason.unwrap_or_else(|| "control client exited".into())),
                        Err(e) => return Some(format!("read failed: {e}")),
                    };
                    match self.parser.feed(&line) {
                        ControlLine::Begin { .. } => body.clear(),
                        ControlLine::Body(text) => body.push(text),
                        ControlLine::End { .. } => self.settle(Ok(std::mem::take(&mut body))),
                        ControlLine::Error { .. } => {
                            self.settle(Err(CmdError::Tmux(std::mem::take(&mut body).join("\n"))));
                        }
                        ControlLine::WindowClose { window } => {
                            let _ = notices.send(Notice::WindowClose(window));
                        }
                        ControlLine::Exit { reason } => {
                            exit_reason = Some(reason.unwrap_or_else(|| "tmux exited".into()));
                        }
                        ControlLine::Other(_) => {}
                    }
                }
            }
        }
    }
}

/// Attaches and starts the connection task.
pub(crate) async fn spawn(
    cfg: TmuxConfig,
) -> Result<(Conn, mpsc::UnboundedReceiver<Notice>, JoinHandle<()>), TmuxError> {
    let client = Client::open(&cfg).await?;
    let (tx, requests) = mpsc::unbounded_channel();
    let (notice_tx, notices) = mpsc::unbounded_channel();
    let task = tokio::spawn(supervise(cfg, client, requests, notice_tx));
    Ok((Conn { tx }, notices, task))
}

/// Serves the client; when it ends, reattaches if the session lives, else reports `Lost`.
async fn supervise(
    cfg: TmuxConfig,
    mut client: Client,
    mut requests: mpsc::UnboundedReceiver<Request>,
    notices: mpsc::UnboundedSender<Notice>,
) {
    loop {
        let Some(reason) = client.serve(&mut requests, &notices).await else {
            return; // every handle dropped
        };
        drop(client); // owed replies fail as Closed
        tracing::info!(reason, "tmux control client ended");
        let reattached = if server::has_session(&cfg).await {
            Client::open(&cfg).await.map_err(|e| e.to_string())
        } else {
            Err(reason)
        };
        match reattached {
            Ok(c) => {
                client = c;
                let _ = notices.send(Notice::Reattached);
            }
            Err(reason) => {
                let _ = notices.send(Notice::Lost(reason));
                return;
            }
        }
    }
}
