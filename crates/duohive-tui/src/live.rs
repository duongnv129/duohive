//! The agent pane's live stream (P2-D1, P2-D4, P2-D5, P2-D12): which agent is streamed,
//! when its stream opens and closes, the terminal its chunks draw on, and the frozen
//! scrollback snapshot.
//!
//! - One stream at a time. It opens [`SETTLE`] after the selection settles on a running
//!   agent, and closes (is dropped) when the selection moves or the pane is hidden. A move
//!   within the settle cancels the pending open.
//! - A size change resizes the window and, after the settle, opens the stream again: the
//!   view sends no fresh `Reset` on resize, and a shrinking `TermView` loses modes. While
//!   typing the re-open waits until typing ends, so keys stay on the opened stream.
//! - A stream that closes before any `Reset` means "not running": the summary stays, with
//!   no note. One that closes after a `Reset` ends Typing mode with `<agent>'s session
//!   ended`. Neither is opened again until the agent's status changes. A stream we let go
//!   of (pane hidden, reconnecting, selection moved) closes quietly, and opens again when
//!   it is wanted again.
//! - Nothing opens while the view isn't ready (still attaching); it is asked again every
//!   [`READY_POLL`].

use std::sync::Arc;
use std::time::Duration;

use duohive_core::names::AgentName;
use duohive_core::ports::PaneView;
use duohive_core::status::Status;
use duohive_core::view::{PaneChunk, PaneInput, PaneSize, PaneStream, StreamId};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::oneshot;
use tokio::time::Instant;

use crate::focus::AgentMode;
use crate::model::UiModel;
use crate::render::{LiveScreen, live_size};
use crate::term_view::TermView;

/// How long the selection must rest before a stream opens (P2-D4).
pub(crate) const SETTLE: Duration = Duration::from_millis(100);
/// The most history `PgUp` asks for (P2-D12).
pub(crate) const SCROLLBACK_LINES: u16 = 2_000;
/// How often a view that is still attaching is asked whether it is ready.
const READY_POLL: Duration = Duration::from_millis(250);

/// An open stream.
pub(crate) struct Open {
    agent: AgentName,
    id: StreamId,
    rx: tokio::sync::mpsc::Receiver<PaneChunk>,
    size: PaneSize,
    /// A `Reset` has arrived for this agent's session (on this stream or the one it
    /// replaced after a resize).
    had_reset: bool,
}

/// An open waiting for the selection to settle.
struct Pending {
    agent: AgentName,
    size: PaneSize,
    at: Instant,
}

/// A frozen scrollback snapshot, drawn a window at a time.
struct Snapshot {
    term: TermView,
    top: u16,
}

/// The live stream and what it draws.
pub(crate) struct Live {
    view: Arc<dyn PaneView>,
    stream: Option<Open>,
    pending: Option<Pending>,
    /// The streamed agent's terminal, once a `Reset` has arrived.
    term: Option<TermView>,
    snapshot: Option<Snapshot>,
    scrollback: Option<oneshot::Receiver<Vec<u8>>>,
    /// A stream for this agent closed while it had this status: not opened again until
    /// the status changes.
    blocked: Option<(AgentName, Status)>,
    /// The view wasn't ready: when to ask again.
    recheck: Option<Instant>,
    /// The size changed while typing: open the stream again once typing ends.
    reopen_deferred: bool,
    no_color: bool,
}

/// Why a stream is dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Close {
    /// The stream closed itself: the window is gone, or was never there.
    Itself,
    /// We let go of it: the selection moved, the pane is hidden, the app is
    /// reconnecting, or the agent stopped.
    Quiet,
}

impl Live {
    pub(crate) fn new(view: Arc<dyn PaneView>, no_color: bool) -> Self {
        Self {
            view,
            stream: None,
            pending: None,
            term: None,
            snapshot: None,
            scrollback: None,
            blocked: None,
            recheck: None,
            reopen_deferred: false,
            no_color,
        }
    }

    /// The agent and size the pane wants streamed now, if any; `None` also while the
    /// view isn't ready, the pane is hidden or the app is reconnecting.
    fn wanted(&mut self, m: &UiModel) -> Option<(AgentName, PaneSize)> {
        let empty = m.org.teams.is_empty() && m.org.roles.is_empty();
        if m.reconnecting > 0 || empty || !self.view.ready() {
            return None;
        }
        let agent = m.selected_agent()?;
        let status = &m.view(agent).status;
        if let Some((a, s)) = &self.blocked {
            if a == agent && s == status {
                return None;
            }
            self.blocked = None;
        }
        if !status.state.is_running() {
            return None;
        }
        Some((agent.clone(), live_size(m)?))
    }

    /// Brings the stream in line with the model after any change: applies chunks already
    /// queued, closes a stream no longer wanted, schedules opens, and follows the pane's
    /// size. `true` when the screen changed.
    pub(crate) fn sync(&mut self, m: &mut UiModel, now: Instant) -> bool {
        if !m.scrolled {
            self.snapshot = None;
        }
        let mut dirty = self.drain(m);
        // A view still attaching is asked again shortly; nothing else would wake us.
        self.recheck = (!self.view.ready()).then(|| now + READY_POLL);
        let want = self.wanted(m);
        if let Some(open) = &self.stream
            && want.as_ref().map(|w| &w.0) != Some(&open.agent)
        {
            // Moved away, hidden, reconnecting or no longer running: close quietly. Only
            // the stream closing itself says the session ended.
            self.end(m, Close::Quiet);
            dirty = true;
        }
        match want {
            Some((agent, size)) => self.follow(agent, size, m, now),
            None => self.pending = None,
        }
        m.live_agent = self.shown_agent().cloned();
        dirty
    }

    /// Schedules the open of `agent` at `size`, or follows a size change of its open
    /// stream: resized at once, and opened again after the settle for a fresh `Reset`;
    /// never while typing (P2-D1: keys stay on the stream that was opened), so the re-open
    /// waits until typing ends.
    fn follow(&mut self, agent: AgentName, size: PaneSize, m: &UiModel, now: Instant) {
        let typing = m.focus.agent_mode == AgentMode::Typing;
        let settle = |agent| Pending {
            agent,
            size,
            at: now + SETTLE,
        };
        let Some(open) = &mut self.stream else {
            match &mut self.pending {
                Some(p) if p.agent == agent => p.size = size,
                _ => self.pending = Some(settle(agent)),
            }
            return;
        };
        if open.size != size {
            let _ = self.view.resize(open.id, size);
            open.size = size;
            if let Some(term) = &mut self.term {
                term.resize(size);
            }
            self.reopen_deferred = true;
            self.pending = None;
        }
        if typing {
            // A re-open waits for typing to end.
            if self.pending.take().is_some() {
                self.reopen_deferred = true;
            }
        } else if self.reopen_deferred {
            self.reopen_deferred = false;
            self.pending = Some(settle(agent));
        } else if let Some(p) = &mut self.pending {
            p.size = size;
        }
    }

    /// When the loop must next call [`Self::open_due`]: the pending open, or the next
    /// readiness check.
    pub(crate) fn open_at(&self) -> Option<Instant> {
        let open = self.pending.as_ref().map(|p| p.at);
        [open, self.recheck].into_iter().flatten().min()
    }

    /// Opens the pending stream when it is due; a stream for the same agent is replaced,
    /// keeping its screen until the new `Reset`. Chunks the old stream already queued are
    /// applied first, so a `Closed` isn't lost (and a dead window isn't re-opened). `true`
    /// when the screen changed.
    pub(crate) fn open_due(&mut self, m: &mut UiModel, now: Instant) -> bool {
        self.recheck = None;
        if self.pending.as_ref().is_none_or(|p| p.at > now) {
            return false;
        }
        let had_stream = self.stream.is_some();
        let dirty = self.drain(m);
        let Some(p) = self.pending.take() else {
            return dirty;
        };
        if had_stream && self.stream.is_none() {
            return dirty;
        }
        let had_reset = match self.stream.take() {
            Some(old) => {
                // A snapshot or a history request belongs to the old stream.
                self.snapshot = None;
                self.scrollback = None;
                m.scrolled = false;
                old.agent == p.agent && old.had_reset
            }
            None => false,
        };
        match self.view.open(&p.agent, p.size) {
            Ok(PaneStream { id, rx }) => {
                let _ = self.view.resize(id, p.size);
                self.stream = Some(Open {
                    agent: p.agent,
                    id,
                    rx,
                    size: p.size,
                    had_reset,
                });
            }
            Err(_) => {
                self.term = None;
                let status = m.view(&p.agent).status.clone();
                self.blocked = Some((p.agent, status));
            }
        }
        m.live_agent = self.shown_agent().cloned();
        dirty
    }

    /// Applies every chunk already queued on the stream; `true` when the screen changed.
    fn drain(&mut self, m: &mut UiModel) -> bool {
        let mut dirty = false;
        while let Some(open) = &mut self.stream {
            let chunk = match open.rx.try_recv() {
                Ok(chunk) => Some(chunk),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => None,
            };
            dirty |= self.on_chunk(chunk, m);
        }
        dirty
    }

    /// Whether a stream is open (its chunks are awaited).
    pub(crate) fn streaming(&self) -> bool {
        self.stream.is_some()
    }

    /// Whether a scrollback answer is awaited.
    pub(crate) fn scrolling_back(&self) -> bool {
        self.scrollback.is_some()
    }

    /// Applies a chunk (`None`: the stream's sender went away); `true` when the screen
    /// changed.
    pub(crate) fn on_chunk(&mut self, chunk: Option<PaneChunk>, m: &mut UiModel) -> bool {
        let Some(open) = &mut self.stream else {
            return false;
        };
        let changed = match chunk {
            Some(chunk @ PaneChunk::Reset { .. }) => {
                open.had_reset = true;
                let size = open.size;
                self.term
                    .get_or_insert_with(|| TermView::new(size))
                    .apply(&chunk);
                true
            }
            Some(chunk @ PaneChunk::Bytes(_)) => match &mut self.term {
                Some(term) if open.had_reset => {
                    term.apply(&chunk);
                    true
                }
                _ => false,
            },
            Some(PaneChunk::Closed { .. }) | None => {
                self.end(m, Close::Itself);
                true
            }
        };
        m.live_agent = self.shown_agent().cloned();
        changed
    }

    /// Drops the stream and everything drawn from it; Typing mode ends. When the stream
    /// closed itself, a session that had shown its screen gets the footer note, and the
    /// agent isn't streamed again until its status changes.
    fn end(&mut self, m: &mut UiModel, how: Close) {
        let Some(open) = self.stream.take() else {
            return;
        };
        self.term = None;
        self.snapshot = None;
        self.scrollback = None;
        self.reopen_deferred = false;
        m.scrolled = false;
        m.live_agent = None;
        if m.focus.agent_mode == AgentMode::Typing {
            m.focus.agent_mode = AgentMode::Selected;
        }
        if how == Close::Quiet {
            return;
        }
        if open.had_reset {
            m.footer_note = Some(format!("{}'s session ended", open.agent));
        }
        if m.selected_agent() == Some(&open.agent) {
            let status = m.view(&open.agent).status.clone();
            self.blocked = Some((open.agent, status));
        }
    }

    /// The agent whose live terminal shows: its stream has delivered a `Reset`.
    fn shown_agent(&self) -> Option<&AgentName> {
        let open = self.stream.as_ref()?;
        (open.had_reset && self.term.is_some()).then_some(&open.agent)
    }

    /// Sends typed input to the open stream. A failure means the window is gone; its
    /// `Closed` follows.
    pub(crate) fn send(&self, input: PaneInput) {
        if let Some(open) = &self.stream {
            let _ = self.view.send(open.id, input);
        }
    }

    /// `PgUp` / `PgDn` (P2-D12). The first `PgUp` asks for the history; once it arrives
    /// the pane shows it a page up from the bottom.
    pub(crate) fn scroll(&mut self, up: bool, m: &mut UiModel) {
        let page = self.page();
        match (&mut self.snapshot, up) {
            (Some(s), true) => s.top = s.top.saturating_sub(page),
            (Some(s), false) => {
                let bottom = s.term.rows().saturating_sub(page);
                if s.top.saturating_add(page) >= bottom {
                    self.snapshot = None;
                    m.scrolled = false;
                } else {
                    s.top += page;
                }
            }
            (None, true) => {
                let Some(open) = &self.stream else {
                    return;
                };
                if open.had_reset && self.scrollback.is_none() {
                    self.scrollback = self.view.scrollback(open.id, SCROLLBACK_LINES).ok();
                }
            }
            (None, false) => {}
        }
    }

    /// The history arrived: freeze it (unless typing started meanwhile).
    pub(crate) fn on_scrollback(&mut self, bytes: Option<Vec<u8>>, m: &mut UiModel) -> bool {
        self.scrollback = None;
        let (Some(bytes), Some(open)) = (bytes, &self.stream) else {
            return false;
        };
        if m.focus.agent_mode == AgentMode::Typing {
            return false;
        }
        let term = snapshot_term(&bytes, open.size.cols);
        let page = self.page();
        let bottom = term.rows().saturating_sub(page);
        self.snapshot = Some(Snapshot {
            term,
            top: bottom.saturating_sub(page),
        });
        m.scrolled = true;
        true
    }

    /// Rows per page: the pane's.
    fn page(&self) -> u16 {
        self.stream.as_ref().map_or(1, |o| o.size.rows.max(1))
    }

    /// The stream and the scrollback request, borrowed apart so the loop can await
    /// both at once.
    pub(crate) fn waits(&mut self) -> (&mut Option<Open>, &mut Option<oneshot::Receiver<Vec<u8>>>) {
        (&mut self.stream, &mut self.scrollback)
    }

    /// What the agent pane draws: the snapshot when scrolled, else the live terminal
    /// once a `Reset` has arrived.
    pub(crate) fn screen(&self) -> Option<LiveScreen<'_>> {
        let open = self.stream.as_ref()?;
        let (term, first_row) = match &self.snapshot {
            Some(s) => (&s.term, s.top),
            None => (self.term.as_ref()?, 0),
        };
        Some(LiveScreen {
            agent: &open.agent,
            term,
            first_row,
            no_color: self.no_color,
        })
    }
}

/// The open stream's next chunk; never, without one.
pub(crate) async fn next_chunk(stream: &mut Option<Open>) -> Option<PaneChunk> {
    match stream {
        Some(open) => open.rx.recv().await,
        None => std::future::pending().await,
    }
}

/// The scrollback answer; `None` when the view dropped the request.
pub(crate) async fn next_scrollback(
    rx: &mut Option<oneshot::Receiver<Vec<u8>>>,
) -> Option<Vec<u8>> {
    match rx {
        Some(rx) => rx.await.ok(),
        None => std::future::pending().await,
    }
}

/// `capture-pane -p` output as a terminal as tall as its lines (`\n` ends each line).
fn snapshot_term(bytes: &[u8], cols: u16) -> TermView {
    let body = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let lines = body.iter().filter(|b| **b == b'\n').count() + 1;
    let rows = u16::try_from(lines).unwrap_or(u16::MAX);
    let mut fed = Vec::with_capacity(body.len() + lines);
    for &b in body {
        if b == b'\n' {
            fed.push(b'\r');
        }
        fed.push(b);
    }
    let mut term = TermView::new(PaneSize { cols, rows });
    term.apply(&PaneChunk::Bytes(fed));
    term
}

#[cfg(test)]
mod tests {
    use super::*;
    use duohive_core::org::NodeKey;
    use duohive_core::testkit::{FakePaneView, PaneCall, n};
    use ratatui::layout::Rect;

    use crate::fixtures::spec_model;

    /// auth-service (working) selected on a 120×36 screen, its stream open with a `Reset`.
    fn opened() -> (Live, FakePaneView, UiModel, Instant) {
        let view = FakePaneView::default();
        let mut live = Live::new(Arc::new(view.clone()), false);
        let mut m = spec_model();
        m.selected = NodeKey::Agent(n("auth-service"));
        m.screen = Rect::new(0, 0, 120, 36);
        let t = Instant::now();
        live.sync(&mut m, t);
        live.open_due(&mut m, t + SETTLE);
        view.push(
            StreamId(1),
            PaneChunk::Reset {
                bytes: b"$ ".to_vec(),
                cursor: (2, 0),
            },
        );
        live.sync(&mut m, t + SETTLE);
        assert_eq!(m.live_agent, Some(n("auth-service")));
        (live, view, m, t + SETTLE)
    }

    #[test]
    fn a_closed_queued_before_a_reopen_is_not_lost() {
        let (mut live, view, mut m, t) = opened();
        m.screen = Rect::new(0, 0, 160, 48);
        live.sync(&mut m, t);
        // The window dies before the settle ends; its `Closed` is still queued.
        view.close(StreamId(1), "the window closed");
        live.open_due(&mut m, t + SETTLE);
        let opens = view
            .calls()
            .into_iter()
            .filter(|c| matches!(c, PaneCall::Open(..)))
            .count();
        assert_eq!(opens, 1, "no re-open of a dead window");
        assert_eq!(
            m.footer_note.as_deref(),
            Some("auth-service's session ended")
        );
        assert!(!live.streaming());
    }

    #[test]
    fn a_replaced_stream_drops_its_scrollback() {
        let (mut live, _view, mut m, t) = opened();
        live.scroll(true, &mut m);
        assert!(live.scrolling_back());
        m.screen = Rect::new(0, 0, 160, 48);
        live.sync(&mut m, t);
        live.open_due(&mut m, t + SETTLE);
        assert!(!live.scrolling_back());
        assert!(!m.scrolled);
        assert!(live.snapshot.is_none());
    }

    #[test]
    fn snapshot_keeps_every_line() {
        let term = snapshot_term(b"one\ntwo\nthree\n", 10);
        assert_eq!(term.rows(), 3);
        let area = ratatui::layout::Rect::new(0, 0, 10, 2);
        let mut buf = ratatui::buffer::Buffer::empty(area);
        term.render_from(area, &mut buf, 1, false, true);
        let row = |y: u16| -> String { (0..10).map(|x| buf[(x, y)].symbol()).collect() };
        assert_eq!(row(0).trim_end(), "two");
        assert_eq!(row(1).trim_end(), "three");
    }
}
