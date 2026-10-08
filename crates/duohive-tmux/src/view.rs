//! `TmuxPaneView`: duohive-core's `PaneView` port over a second control client that
//! streams only the viewed pane (spike S3.8, TD §4.3).
//!
//! The view runs on its own thread, with its own tokio runtime, so the port's methods
//! stay synchronous and never block: they queue requests for the worker, which owns the
//! viewer connection. A stream is bound to the pane open when the worker looks the agent
//! up; keys and pastes go to that pane id, and tmux never reuses a pane id, so they can't
//! reach a newer window of the same agent (P2-D1).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use duohive_core::names::AgentName;
use duohive_core::ports::PaneView;
use duohive_core::view::{PaneChunk, PaneInput, PaneSize, PaneStream, StreamId, ViewError};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

mod commands;

use self::commands::{
    Target, find_target, first_frame, input_line, key_name, open_line, panes_command, q,
    resize_command,
};
use crate::connection::{self, CmdError, Conn, Notice};
use crate::server::{self, TmuxConfig, TmuxError};

/// Chunks a stream holds before its bytes wait in the worker's backlog.
const STREAM_CAPACITY: usize = 256;

/// Bytes a stream may fall behind by before it is closed.
const MAX_BACKLOG: usize = 8 * 1024 * 1024;

/// How long the worker waits for the reply to a lookup or an open.
const CALL_TIMEOUT: Duration = Duration::from_secs(15);

/// The live pane view. Dropping it stops its thread and the viewer connection.
pub struct TmuxPaneView {
    requests: mpsc::UnboundedSender<Req>,
    shared: Arc<Shared>,
    next_id: AtomicU64,
}

/// What the port's methods and the worker share.
#[derive(Default)]
struct Shared {
    /// The newest open stream, until it closes: the only one that takes input.
    live: Mutex<Option<StreamId>>,
    /// `%output` lines from panes other than the viewed one.
    foreign: AtomicU64,
}

impl Shared {
    fn live(&self) -> std::sync::MutexGuard<'_, Option<StreamId>> {
        self.live.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn is_live(&self, id: StreamId) -> bool {
        *self.live() == Some(id)
    }

    /// Forgets `id` unless a newer stream has replaced it.
    fn clear(&self, id: StreamId) {
        let mut live = self.live();
        if *live == Some(id) {
            *live = None;
        }
    }
}

/// One request to the worker, served in order.
enum Req {
    Open {
        id: StreamId,
        agent: AgentName,
        size: PaneSize,
        tx: mpsc::Sender<PaneChunk>,
    },
    Resize(StreamId, PaneSize),
    Send(StreamId, PaneInput),
    Scrollback(StreamId, u16, oneshot::Sender<Vec<u8>>),
}

impl TmuxPaneView {
    /// Starts the view's thread and attaches its control client (silenced, like the
    /// main connection) to the session, which must exist.
    ///
    /// # Errors
    /// The thread or its runtime can't start, or the attach fails.
    pub fn new(cfg: &TmuxConfig) -> Result<Self, TmuxError> {
        let (requests, rx) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared::default());
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let cfg = cfg.clone();
        let worker_shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("duohive-view".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = ready_tx.send(Err(TmuxError::Io(e)));
                        return;
                    }
                };
                rt.block_on(async move {
                    let link = match Link::attach(&cfg).await {
                        Ok(link) => link,
                        Err(e) => {
                            let _ = ready_tx.send(Err(e));
                            return;
                        }
                    };
                    let _ = ready_tx.send(Ok(()));
                    Worker::new(cfg, link, worker_shared).run(rx).await;
                });
            })?;
        ready_rx
            .recv()
            .map_err(|_| TmuxError::Attach("the view thread stopped".into()))??;
        Ok(Self {
            requests,
            shared,
            next_id: AtomicU64::new(0),
        })
    }

    /// `%output` lines the viewer received from panes other than the viewed one
    /// (diagnostics: S3.3's "other panes stay silent").
    #[doc(hidden)]
    pub fn foreign_output_lines(&self) -> u64 {
        self.shared.foreign.load(Ordering::Relaxed)
    }

    fn check(&self, stream: StreamId) -> Result<(), ViewError> {
        if self.shared.is_live(stream) {
            Ok(())
        } else {
            Err(ViewError::Closed)
        }
    }

    fn request(&self, req: Req) -> Result<(), ViewError> {
        self.requests
            .send(req)
            .map_err(|_| ViewError::Tmux("the pane view stopped".into()))
    }
}

impl PaneView for TmuxPaneView {
    /// Returns at once; the stream's first chunk is the `Reset`, or `Closed` when the
    /// agent has no window (`"<agent> is not running"`) or the open fails.
    fn open(&self, agent: &AgentName, size: PaneSize) -> Result<PaneStream, ViewError> {
        let id = StreamId(self.next_id.fetch_add(1, Ordering::Relaxed) + 1);
        let (tx, rx) = mpsc::channel(STREAM_CAPACITY);
        *self.shared.live() = Some(id);
        let opened = self.request(Req::Open {
            id,
            agent: agent.clone(),
            size,
            tx,
        });
        if opened.is_err() {
            self.shared.clear(id);
        }
        opened.map(|()| PaneStream { id, rx })
    }

    fn resize(&self, stream: StreamId, size: PaneSize) -> Result<(), ViewError> {
        self.check(stream)?;
        self.request(Req::Resize(stream, size))
    }

    fn send(&self, stream: StreamId, input: PaneInput) -> Result<(), ViewError> {
        self.check(stream)?;
        if let PaneInput::Key(key) = &input {
            key_name(*key)?;
        }
        self.request(Req::Send(stream, input))
    }

    fn scrollback(
        &self,
        stream: StreamId,
        lines: u16,
    ) -> Result<oneshot::Receiver<Vec<u8>>, ViewError> {
        self.check(stream)?;
        let (tx, rx) = oneshot::channel();
        self.request(Req::Scrollback(stream, lines, tx))?;
        Ok(rx)
    }
}

/// The viewer's control client.
struct Link {
    conn: Conn,
    notices: mpsc::UnboundedReceiver<Notice>,
}

impl Link {
    async fn attach(cfg: &TmuxConfig) -> Result<Self, TmuxError> {
        let (conn, notices, _task) = connection::spawn(cfg.clone()).await?;
        Ok(Self { conn, notices })
    }
}

/// The open stream.
struct Live {
    id: StreamId,
    target: Target,
    tx: mpsc::Sender<PaneChunk>,
    /// The first frame was sent: `%output` is forwarded from here on.
    ready: bool,
    /// Bytes waiting for room in the stream.
    backlog: Vec<u8>,
}

/// What the worker does with the reply to a tagged line it didn't wait for.
enum Pending {
    /// `list-panes` of a window added while viewing: turn its panes off.
    NewWindow,
    /// Input or a resize for this stream: a failure closes it.
    Input(StreamId),
    /// Scrollback for the caller.
    Scrollback(oneshot::Sender<Vec<u8>>),
    /// Nothing; failures are logged.
    Ignore,
}

/// What woke the worker.
enum Event {
    Request(Option<Req>),
    Notice(Option<Notice>),
    /// The stream's receiver was dropped.
    Dropped,
    /// The stream has room for its backlog.
    Writable,
}

struct Worker {
    cfg: TmuxConfig,
    /// The paste buffer, apart from the runtime's.
    buffer: String,
    link: Option<Link>,
    live: Option<Live>,
    shared: Arc<Shared>,
    next_tag: u64,
    pending: HashMap<u64, Pending>,
}

impl Worker {
    fn new(cfg: TmuxConfig, link: Link, shared: Arc<Shared>) -> Self {
        Self {
            buffer: format!("{}-view", cfg.buffer_name),
            cfg,
            link: Some(link),
            live: None,
            shared,
            next_tag: 0,
            pending: HashMap::new(),
        }
    }

    async fn run(mut self, mut requests: mpsc::UnboundedReceiver<Req>) {
        loop {
            match self.next_event(&mut requests).await {
                Event::Request(None) => {
                    self.close_live(Some("the pane view stopped"));
                    return;
                }
                Event::Request(Some(req)) => self.serve(req).await,
                Event::Notice(notice) => self.on_notice(notice),
                Event::Dropped => self.close_live(None),
                Event::Writable => self.flush(),
            }
        }
    }

    async fn next_event(&mut self, requests: &mut mpsc::UnboundedReceiver<Req>) -> Event {
        let tx = self.live.as_ref().map(|l| l.tx.clone());
        let backlogged = self.live.as_ref().is_some_and(|l| !l.backlog.is_empty());
        let notices = self.link.as_mut().map(|l| &mut l.notices);
        tokio::select! {
            req = requests.recv() => Event::Request(req),
            notice = recv(notices) => Event::Notice(notice),
            () = dropped(tx.as_ref()) => Event::Dropped,
            () = room(tx.as_ref()), if backlogged => Event::Writable,
        }
    }

    async fn serve(&mut self, req: Req) {
        match req {
            Req::Open {
                id,
                agent,
                size,
                tx,
            } => self.open(id, &agent, size, tx).await,
            Req::Resize(id, size) => {
                if let Some(live) = self.live_for(id) {
                    match resize_command(&live.target.window, size) {
                        Ok(line) => self.send_tagged(line, 1, Pending::Input(id)),
                        Err(e) => tracing::warn!(e, "resize: bad window id"),
                    }
                }
            }
            Req::Send(id, input) => self.send_input(id, &input).await,
            Req::Scrollback(id, lines, tx) => {
                if let Some(live) = self.live_for(id) {
                    match q(&live.target.pane) {
                        Ok(pane) => self.send_tagged(
                            format!("capture-pane -e -p -S -{lines} -t {pane}"),
                            1,
                            Pending::Scrollback(tx),
                        ),
                        Err(e) => tracing::warn!(e, "scrollback: bad pane id"),
                    }
                }
            }
        }
    }

    fn live_for(&self, id: StreamId) -> Option<&Live> {
        self.live.as_ref().filter(|l| l.id == id && l.ready)
    }

    async fn open(
        &mut self,
        id: StreamId,
        agent: &AgentName,
        size: PaneSize,
        tx: mpsc::Sender<PaneChunk>,
    ) {
        self.close_live(Some("another view opened"));
        if !self.shared.is_live(id) {
            self.refuse(id, &tx, "another view opened");
            return;
        }
        if let Err(reason) = self.ensure_link().await {
            self.refuse(id, &tx, &reason);
            return;
        }
        let rows = match panes_command(&self.cfg.session) {
            Ok(line) => self.call(line, 1).await,
            Err(e) => Err(CmdError::Tmux(e)),
        };
        let (target, others) = match rows {
            Ok(bodies) => find_target(bodies.first().map_or(&[][..], Vec::as_slice), agent),
            Err(e) => {
                self.refuse(id, &tx, &format!("couldn't list the panes: {e}"));
                return;
            }
        };
        let Some(target) = target else {
            self.refuse(id, &tx, &ViewError::NotRunning(agent.clone()).to_string());
            return;
        };
        let line = match open_line(&target, &others, size) {
            Ok(line) => line,
            Err(e) => {
                self.refuse(id, &tx, &e);
                return;
            }
        };
        self.live = Some(Live {
            id,
            target,
            tx,
            ready: false,
            backlog: Vec::new(),
        });
        let reply = self.call(line.0, line.1).await;
        // A notice read while waiting may have closed it already.
        let Some(live) = self.live.as_mut().filter(|l| l.id == id) else {
            return;
        };
        match reply {
            Ok(bodies) if bodies.len() >= 2 => {
                let n = bodies.len();
                let reset = first_frame(&bodies[n - 2], &bodies[n - 1]);
                // The stream is new and empty: there is room.
                let _ = live.tx.try_send(reset);
                live.ready = true;
            }
            Ok(_) => self.close_live(Some("couldn't open the view: short reply")),
            Err(e) => self.close_live(Some(&format!("couldn't open the view: {e}"))),
        }
    }

    /// Ends a stream that never opened.
    fn refuse(&self, id: StreamId, tx: &mpsc::Sender<PaneChunk>, reason: &str) {
        self.shared.clear(id);
        let _ = tx.try_send(PaneChunk::Closed {
            reason: reason.to_owned(),
        });
    }

    async fn send_input(&mut self, id: StreamId, input: &PaneInput) {
        let Some(pane) = self.live_for(id).map(|l| l.target.pane.clone()) else {
            return;
        };
        if let PaneInput::Paste(text) = input
            && let Err(e) = server::load_buffer(&self.cfg_for_buffer(), text).await
        {
            tracing::warn!(e, "paste: load-buffer failed");
            return;
        }
        match input_line(&pane, input, &self.buffer) {
            Ok((line, 0)) if line.is_empty() => {}
            Ok((line, blocks)) => self.send_tagged(line, blocks, Pending::Input(id)),
            Err(e) => tracing::warn!(e, "input can't be sent"),
        }
    }

    /// The config with the view's own paste buffer.
    fn cfg_for_buffer(&self) -> TmuxConfig {
        TmuxConfig {
            buffer_name: self.buffer.clone(),
            ..self.cfg.clone()
        }
    }

    /// Attaches again when the viewer connection was lost.
    async fn ensure_link(&mut self) -> Result<(), String> {
        if self.link.as_ref().is_some_and(|l| !l.conn.is_closed()) {
            return Ok(());
        }
        self.link = None;
        self.pending.clear();
        let link = Link::attach(&self.cfg)
            .await
            .map_err(|e| format!("couldn't attach the view: {e}"))?;
        self.link = Some(link);
        Ok(())
    }

    fn tag(&mut self) -> u64 {
        self.next_tag += 1;
        self.next_tag
    }

    /// Sends a tagged line; its reply is handled by `on_notice`.
    fn send_tagged(&mut self, line: String, blocks: usize, then: Pending) {
        let tag = self.tag();
        let Some(link) = &self.link else { return };
        if link.conn.send_tagged(line, blocks, tag).is_ok() {
            self.pending.insert(tag, then);
        }
    }

    /// Sends a tagged line and waits for its reply, handling every notice before it.
    async fn call(&mut self, line: String, blocks: usize) -> Result<Vec<Vec<String>>, CmdError> {
        let tag = self.tag();
        self.link
            .as_ref()
            .ok_or(CmdError::Closed)?
            .conn
            .send_tagged(line, blocks, tag)?;
        let deadline = Instant::now() + CALL_TIMEOUT;
        loop {
            let link = self.link.as_mut().ok_or(CmdError::Closed)?;
            let notice = tokio::time::timeout_at(deadline, link.notices.recv())
                .await
                .map_err(|_| CmdError::Timeout)?;
            match notice {
                Some(Notice::Reply { tag: t, reply }) if t == tag => return reply,
                None => {
                    self.on_notice(None);
                    return Err(CmdError::Closed);
                }
                other => self.on_notice(other),
            }
        }
    }

    fn on_notice(&mut self, notice: Option<Notice>) {
        match notice {
            None => {
                self.link = None;
                self.pending.clear();
                self.close_live(Some("the tmux connection was lost"));
            }
            Some(Notice::Output { pane, data }) => self.on_output(&pane, data),
            Some(Notice::WindowClose(window)) => {
                if self
                    .live
                    .as_ref()
                    .is_some_and(|l| l.target.window == window)
                {
                    self.close_live(Some("the window closed"));
                }
            }
            Some(Notice::WindowAdd(window)) => {
                // It streams until it is turned off (S3.3).
                if self.live.is_some() {
                    match q(&window) {
                        Ok(w) => self.send_tagged(
                            format!("list-panes -t {w} -F '#{{pane_id}}'"),
                            1,
                            Pending::NewWindow,
                        ),
                        Err(e) => tracing::warn!(e, "window-add: bad window id"),
                    }
                }
            }
            Some(Notice::Reattached) => self.close_live(Some("the tmux connection was reset")),
            Some(Notice::Lost(reason)) => {
                self.close_live(Some(&format!("the tmux connection was lost: {reason}")));
            }
            Some(Notice::Reply { tag, reply }) => self.on_reply(tag, reply),
        }
    }

    fn on_output(&mut self, pane: &str, data: Vec<u8>) {
        let Some(live) = self.live.as_mut().filter(|l| l.target.pane == pane) else {
            self.shared.foreign.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if !live.ready {
            // Before the capture's reply: already in the first frame.
            return;
        }
        if !live.backlog.is_empty() {
            live.backlog.extend_from_slice(&data);
            if live.backlog.len() > MAX_BACKLOG {
                self.close_live(Some("the view fell behind"));
            }
            return;
        }
        match live.tx.try_send(PaneChunk::Bytes(data)) {
            Ok(()) => {}
            Err(TrySendError::Full(PaneChunk::Bytes(data))) => live.backlog = data,
            Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Closed(_)) => self.close_live(None),
        }
    }

    fn on_reply(&mut self, tag: u64, reply: Result<Vec<Vec<String>>, CmdError>) {
        let Some(then) = self.pending.remove(&tag) else {
            return; // a call that timed out
        };
        match (then, reply) {
            (Pending::NewWindow, Ok(bodies)) => {
                let viewed = self.live.as_ref().map(|l| l.target.pane.clone());
                for pane in bodies.into_iter().flatten() {
                    let pane = pane.trim().to_owned();
                    if viewed.as_deref() == Some(pane.as_str()) {
                        continue;
                    }
                    match q(&format!("{pane}:off")) {
                        Ok(arg) => {
                            self.send_tagged(
                                format!("refresh-client -A {arg}"),
                                1,
                                Pending::Ignore,
                            );
                        }
                        Err(e) => tracing::warn!(e, "window-add: bad pane id"),
                    }
                }
            }
            (Pending::Scrollback(tx), Ok(bodies)) => {
                let lines = bodies.into_iter().next().unwrap_or_default();
                let _ = tx.send(lines.join("\r\n").into_bytes());
            }
            (Pending::Input(id), Err(CmdError::Tmux(e))) if e.starts_with("can't find") => {
                if self.live.as_ref().is_some_and(|l| l.id == id) {
                    self.close_live(Some("the window closed"));
                }
            }
            (Pending::Input(_) | Pending::Ignore, Ok(_)) => {}
            (_, Err(e)) => tracing::debug!(%e, "a view command failed"),
        }
    }

    /// Moves the backlog into the stream.
    fn flush(&mut self) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let data = std::mem::take(&mut live.backlog);
        match live.tx.try_send(PaneChunk::Bytes(data)) {
            Ok(()) => {}
            Err(TrySendError::Full(PaneChunk::Bytes(data))) => live.backlog = data,
            Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Closed(_)) => self.close_live(None),
        }
    }

    /// Ends the open stream (with `Closed { reason }` as its last chunk, after any
    /// backlog) and silences the viewer again.
    fn close_live(&mut self, reason: Option<&str>) {
        let Some(live) = self.live.take() else { return };
        self.shared.clear(live.id);
        if let Some(reason) = reason {
            let reason = reason.to_owned();
            let (tx, backlog) = (live.tx, live.backlog);
            // Waits for room without holding the worker; ends if the receiver goes.
            tokio::spawn(async move {
                if !backlog.is_empty() && tx.send(PaneChunk::Bytes(backlog)).await.is_err() {
                    return;
                }
                let _ = tx.send(PaneChunk::Closed { reason }).await;
            });
        }
        self.send_tagged("refresh-client -f no-output".into(), 1, Pending::Ignore);
    }
}

async fn recv(notices: Option<&mut mpsc::UnboundedReceiver<Notice>>) -> Option<Notice> {
    match notices {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

async fn dropped(tx: Option<&mpsc::Sender<PaneChunk>>) {
    match tx {
        Some(tx) => tx.closed().await,
        None => std::future::pending().await,
    }
}

async fn room(tx: Option<&mpsc::Sender<PaneChunk>>) {
    match tx {
        Some(tx) => {
            if tx.reserve().await.is_err() {
                // Closed: `dropped` reports it.
                std::future::pending::<()>().await;
            }
        }
        None => std::future::pending().await,
    }
}
