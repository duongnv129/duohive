//! The control-mode connection: one `tmux -C attach-session` child, replies matched
//! FIFO to the `%begin`/`%end` blocks of commands it sent (flags 1), reattach when the
//! client drops (or a reply never comes) but the server lives.

use std::collections::VecDeque;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, Lines};
use tokio::process::Child;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::control::{ControlLine, ControlParser};
use crate::server::{self, TmuxConfig, TmuxError};

/// The S0.5 silencing (D25), sent on attach and on every reattach.
const SILENCE: &str = "refresh-client -f no-output,ignore-size";

/// The control client's size, sent right after the silencing: tmux 3.2a sizes windows
/// from the attaching control client (80x24) before `ignore-size` lands.
const SIZE: &str = "refresh-client -C 160x48";

/// How long the attach's own reply block may take.
const HANDSHAKE: Duration = Duration::from_secs(5);

/// How long tmux may take to answer one written command.
pub(crate) const CMD_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a caller waits in all: queued during a reattach, then answered.
const CALLER_TIMEOUT: Duration = Duration::from_secs(15);

/// Why a command got no reply.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CmdError {
    /// tmux answered `%error` with this text.
    #[error("{0}")]
    Tmux(String),
    /// The control client went away before replying.
    #[error("tmux connection closed")]
    Closed,
    /// No reply in time.
    #[error("no reply from tmux")]
    Timeout,
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
type Reader = Box<dyn AsyncRead + Unpin + Send>;
type Writer = Box<dyn AsyncWrite + Unpin + Send>;

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
        match tokio::time::timeout(CALLER_TIMEOUT, rx).await {
            Ok(reply) => reply.unwrap_or(Err(CmdError::Closed)),
            Err(_) => Err(CmdError::Timeout),
        }
    }

    /// True once the connection task has ended for good.
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

/// A reply owed for a written command; `reply` is `None` for internal commands.
struct Owed {
    reply: Option<oneshot::Sender<Reply>>,
    due: Instant,
}

/// One attached control client.
struct Client {
    _child: Option<Child>,
    writer: Writer,
    lines: Lines<BufReader<Reader>>,
    parser: ControlParser,
    owed: VecDeque<Owed>,
    /// The open block's body.
    body: Vec<String>,
    /// A command this client sent was answered.
    answered: bool,
}

impl Client {
    fn from_io(reader: Reader, writer: Writer, child: Option<Child>) -> Self {
        Self {
            _child: child,
            writer,
            lines: BufReader::new(reader).lines(),
            parser: ControlParser::default(),
            owed: VecDeque::new(),
            body: Vec::new(),
            answered: false,
        }
    }

    /// Attaches, consumes the attach's own reply block, then silences and sizes the client.
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
        let mut client = Self::from_io(Box::new(stdout), Box::new(stdin), Some(child));
        tokio::time::timeout(HANDSHAKE, client.handshake())
            .await
            .map_err(|_| TmuxError::Attach("no reply to attach".into()))??;
        client.write(SILENCE, None).await?;
        client.write(SIZE, None).await?;
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
        self.writer
            .write_all(format!("{line}\n").as_bytes())
            .await?;
        self.writer.flush().await?;
        self.owed.push_back(Owed {
            reply,
            due: Instant::now() + CMD_TIMEOUT,
        });
        Ok(())
    }

    /// Ends the open block: a client command's block settles the oldest owed reply;
    /// a server block (flags 0: the attach, hook commands) is dropped.
    fn settle(&mut self, flags: u32, reply: impl FnOnce(Vec<String>) -> Reply) {
        let body = std::mem::take(&mut self.body);
        if flags & 1 == 0 {
            tracing::debug!(?body, "dropped a server block (flags 0)");
            return;
        }
        self.answered = true;
        if let Some(Owed {
            reply: Some(tx), ..
        }) = self.owed.pop_front()
        {
            let _ = tx.send(reply(body));
        }
    }

    /// Handles one line; `Some(reason)` once `%exit` is read.
    fn on_line(&mut self, line: &str, notices: &mpsc::UnboundedSender<Notice>) -> Option<String> {
        match self.parser.feed(line) {
            ControlLine::Begin { .. } => self.body.clear(),
            ControlLine::Body(text) => self.body.push(text),
            ControlLine::End { flags, .. } => self.settle(flags, Ok),
            ControlLine::Error { flags, .. } => {
                self.settle(flags, |body| Err(CmdError::Tmux(body.join("\n"))));
            }
            ControlLine::WindowClose { window } => {
                let _ = notices.send(Notice::WindowClose(window));
            }
            ControlLine::Exit { reason } => {
                return Some(reason.unwrap_or_else(|| "tmux exited".into()));
            }
            ControlLine::Other(_) => {}
        }
        None
    }

    /// Serves requests and reads lines until the client ends, a reply is overdue, or
    /// every handle is gone.
    async fn serve(
        &mut self,
        requests: &mut mpsc::UnboundedReceiver<Request>,
        notices: &mpsc::UnboundedSender<Notice>,
    ) -> End {
        let mut exit_reason = None;
        loop {
            let due = self.owed.front().map(|o| o.due);
            tokio::select! {
                req = requests.recv() => {
                    let Some(Request { line, reply }) = req else { return End::Shutdown };
                    if reply.is_closed() {
                        // Its caller gave up (timed out): running it now would act unseen,
                        // e.g. a new-window for a start already reported as failed.
                        tracing::debug!(line, "dropped a request its caller abandoned");
                        continue;
                    }
                    if let Err(e) = self.write(&line, Some(reply)).await {
                        return End::Exit(format!("write failed: {e}"));
                    }
                }
                line = self.lines.next_line() => match line {
                    Ok(Some(line)) => {
                        if let Some(reason) = self.on_line(&line, notices) {
                            exit_reason = Some(reason);
                        }
                    }
                    Ok(None) => return End::Exit(exit_reason.unwrap_or_else(|| "control client exited".into())),
                    Err(e) => return End::Exit(format!("read failed: {e}")),
                },
                () = tokio::time::sleep_until(due.unwrap_or_else(Instant::now)), if due.is_some() => {
                    // Replies can no longer be matched safely: give up on this client.
                    if let Some(Owed { reply: Some(tx), .. }) = self.owed.pop_front() {
                        let _ = tx.send(Err(CmdError::Timeout));
                    }
                    return End::Timeout { answered: self.answered };
                }
            }
        }
    }
}

/// Why a client stopped being served.
#[derive(Debug, Clone, PartialEq, Eq)]
enum End {
    /// Every handle is gone.
    Shutdown,
    /// The client exited or its pipes failed.
    Exit(String),
    /// A reply was overdue; `answered` tells whether any command was answered before.
    Timeout { answered: bool },
}

/// Consecutive timeout reattaches, with no command answered, before giving up.
const MAX_SILENT_REATTACHES: u32 = 3;

/// Attaches and starts the connection task.
pub(crate) async fn spawn(
    cfg: TmuxConfig,
) -> Result<(Conn, mpsc::UnboundedReceiver<Notice>, JoinHandle<()>), TmuxError> {
    let client = Client::open(&cfg).await?;
    let (tx, requests) = mpsc::unbounded_channel();
    let (notice_tx, notices) = mpsc::unbounded_channel();
    let reopen = move || {
        let cfg = cfg.clone();
        async move {
            if server::has_session(&cfg).await {
                Client::open(&cfg).await.map_err(|e| e.to_string())
            } else {
                Err("tmux server gone".to_owned())
            }
        }
    };
    let task = tokio::spawn(supervise(client, requests, notice_tx, reopen));
    Ok((Conn { tx }, notices, task))
}

/// Serves the client; when it ends, reattaches through `reopen` (which fails when the
/// session is gone), else reports `Lost`. A server that keeps attaching but never
/// answers is lost after [`MAX_SILENT_REATTACHES`] timeouts in a row.
async fn supervise<F, Fut>(
    mut client: Client,
    mut requests: mpsc::UnboundedReceiver<Request>,
    notices: mpsc::UnboundedSender<Notice>,
    mut reopen: F,
) where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Client, String>>,
{
    let mut silent = 0;
    loop {
        let end = client.serve(&mut requests, &notices).await;
        drop(client); // owed replies fail as Closed
        tracing::info!(?end, "tmux control client ended");
        match end {
            End::Shutdown => return,
            End::Timeout { answered: false } => silent += 1,
            End::Timeout { answered: true } | End::Exit(_) => silent = 0,
        }
        if silent >= MAX_SILENT_REATTACHES {
            let _ = notices.send(Notice::Lost("tmux stopped answering".into()));
            return;
        }
        match reopen().await {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    /// A client wired to an in-memory fake tmux; returns the handle, the fake's side and the serve task.
    fn fake() -> (
        Conn,
        tokio::io::DuplexStream,
        JoinHandle<End>,
        mpsc::UnboundedReceiver<Notice>,
    ) {
        let (ours, theirs) = tokio::io::duplex(4096);
        let (r, w) = tokio::io::split(ours);
        let mut client = Client::from_io(Box::new(r), Box::new(w), None);
        let (tx, mut requests) = mpsc::unbounded_channel();
        let (notice_tx, notices) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move { client.serve(&mut requests, &notice_tx).await });
        (Conn { tx }, theirs, task, notices)
    }

    #[tokio::test]
    async fn server_blocks_with_flags_0_do_not_take_a_reply() {
        let (conn, theirs, _task, mut notices) = fake();
        let (r, mut w) = tokio::io::split(theirs);
        let first = tokio::spawn({
            let conn = conn.clone();
            async move { conn.cmd("one".into()).await }
        });
        let mut lines = BufReader::new(r).lines();
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("one"));
        let second = tokio::spawn({
            let conn = conn.clone();
            async move { conn.cmd("two".into()).await }
        });
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("two"));
        w.write_all(
            b"%begin 1 10 0\n%end 1 10 0\n%begin 1 11 1\nfirst\n%end 1 11 1\n\
              %begin 1 12 0\n%message hi\n%end 1 12 0\n%unlinked-window-close @4\n\
              %begin 1 13 1\nsecond\n%end 1 13 1\n",
        )
        .await
        .unwrap();
        assert_eq!(first.await.unwrap(), Ok(vec!["first".to_owned()]));
        assert_eq!(second.await.unwrap(), Ok(vec!["second".to_owned()]));
        assert_eq!(notices.recv().await, Some(Notice::WindowClose("@4".into())));
    }

    #[tokio::test(start_paused = true)]
    async fn a_lost_reply_times_out_and_ends_the_client() {
        let (conn, theirs, task, _notices) = fake();
        let (r, _w) = tokio::io::split(theirs);
        let pending = tokio::spawn(async move { conn.cmd("lost".into()).await });
        let mut lines = BufReader::new(r).lines();
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("lost"));
        assert_eq!(pending.await.unwrap(), Err(CmdError::Timeout));
        assert_eq!(task.await.unwrap(), End::Timeout { answered: false });
    }

    #[tokio::test]
    async fn requests_whose_caller_gave_up_are_never_written() {
        let (conn, theirs, _task, _notices) = fake();
        let (abandoned, rx) = oneshot::channel();
        drop(rx); // the caller timed out
        conn.tx
            .send(Request {
                line: "new-window stale".into(),
                reply: abandoned,
            })
            .unwrap();
        let (r, mut w) = tokio::io::split(theirs);
        let fresh = tokio::spawn(async move { conn.cmd("fresh".into()).await });
        let mut lines = BufReader::new(r).lines();
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("fresh"));
        w.write_all(b"%begin 1 1 1\nok\n%end 1 1 1\n")
            .await
            .unwrap();
        assert_eq!(fresh.await.unwrap(), Ok(vec!["ok".to_owned()]));
    }

    /// A fake client whose tmux reads everything and never answers; one internal command owed.
    async fn silent_client() -> Client {
        let (ours, theirs) = tokio::io::duplex(4096);
        let (r, w) = tokio::io::split(ours);
        tokio::spawn(async move {
            let mut lines = BufReader::new(theirs).lines();
            while let Ok(Some(_)) = lines.next_line().await {}
        });
        let mut client = Client::from_io(Box::new(r), Box::new(w), None);
        client.write(SIZE, None).await.unwrap();
        client
    }

    #[tokio::test(start_paused = true)]
    async fn a_server_that_never_answers_is_lost_after_three_reattaches() {
        let (_tx, requests) = mpsc::unbounded_channel();
        let (notice_tx, mut notices) = mpsc::unbounded_channel();
        let first = silent_client().await;
        let reopen = || async { Ok(silent_client().await) };
        supervise(first, requests, notice_tx, reopen).await;
        let mut got = Vec::new();
        while let Some(n) = notices.recv().await {
            got.push(n);
        }
        assert_eq!(
            got,
            vec![
                Notice::Reattached,
                Notice::Reattached,
                Notice::Lost("tmux stopped answering".into()),
            ]
        );
    }
}
