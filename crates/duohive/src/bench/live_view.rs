//! `duohive bench --view`: one live pane stream, held open from the moment
//! every agent has started to the end of the idle window (P2-D4), fed into a
//! terminal emulator as the app's agent pane would be.

use duohive_core::names::AgentName;
use duohive_core::ports::PaneView;
use duohive_core::view::{PaneChunk, PaneSize, PaneStream};
use duohive_tmux::{TmuxConfig, TmuxPaneView};
use duohive_tui::TermView;

use super::BenchError;
use super::measure::child_pids_matching;

/// The size the bench views the pane at: a typical agent pane.
const SIZE: PaneSize = PaneSize {
    cols: 110,
    rows: 40,
};

/// What the live view did over the run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveViewUse {
    /// The agent viewed.
    pub agent: String,
    /// Streams opened over the run (each `open` call counts).
    pub streams_opened: u32,
    /// Whether the stream was still open when the idle window ended.
    pub open_at_end: bool,
    /// Why it closed, when it did.
    pub closed: Option<String>,
    /// Chunks received.
    pub chunks: u64,
    /// Bytes received.
    pub bytes: u64,
    /// `%output` lines the view's client got from other panes (should stay 0).
    pub foreign_output_lines: u64,
    /// The view's own tmux client, when it was found.
    pub client_pid: Option<u32>,
    /// Whether that client is inside DuoHive's CPU and memory.
    pub client_counted: bool,
}

/// A pane view the bench can hold open: the port plus the view's diagnostics.
pub trait Viewer: PaneView {
    /// `%output` lines received from panes other than the viewed one.
    fn foreign_output_lines(&self) -> u64 {
        0
    }
}

impl Viewer for TmuxPaneView {
    fn foreign_output_lines(&self) -> u64 {
        TmuxPaneView::foreign_output_lines(self)
    }
}

/// The view, its stream and the terminal it draws into.
pub struct LiveView {
    agent: AgentName,
    view: Box<dyn Viewer>,
    client_pid: Option<u32>,
    stream: Option<PaneStream>,
    opens: u32,
    term: TermView,
    chunks: u64,
    bytes: u64,
    closed: Option<String>,
}

impl LiveView {
    /// Attaches the view's client (off the async loop: it blocks), noting its
    /// pid as the one new child on `socket`, and opens `agent`'s stream.
    ///
    /// # Errors
    /// The attach or the open fails.
    pub async fn attach(
        cfg: TmuxConfig,
        socket: &str,
        agent: AgentName,
    ) -> Result<Self, BenchError> {
        let before = child_pids_matching(socket);
        let view = tokio::task::spawn_blocking(move || TmuxPaneView::new(&cfg))
            .await
            .map_err(|e| BenchError::View(e.to_string()))?
            .map_err(|e| BenchError::View(e.to_string()))?;
        let new: Vec<u32> = child_pids_matching(socket)
            .into_iter()
            .filter(|pid| !before.contains(pid))
            .collect();
        // Exactly one new client, or none counted: never a guess.
        let client_pid = match new.as_slice() {
            [pid] => Some(*pid),
            _ => None,
        };
        let mut live = Self::new(Box::new(view), agent, client_pid);
        live.open_stream()?;
        Ok(live)
    }

    /// A view on `agent` with no stream open yet.
    pub fn new(view: Box<dyn Viewer>, agent: AgentName, client_pid: Option<u32>) -> Self {
        Self {
            agent,
            view,
            client_pid,
            stream: None,
            opens: 0,
            term: TermView::new(SIZE),
            chunks: 0,
            bytes: 0,
            closed: None,
        }
    }

    /// Opens (or reopens) the agent's stream; every call counts.
    ///
    /// # Errors
    /// The view refuses the open.
    pub fn open_stream(&mut self) -> Result<(), BenchError> {
        let stream = self
            .view
            .open(&self.agent, SIZE)
            .map_err(|e| BenchError::View(e.to_string()))?;
        self.opens += 1;
        self.stream = Some(stream);
        self.closed = None;
        Ok(())
    }

    /// The view's own tmux client, when it was found.
    pub fn client_pid(&self) -> Option<u32> {
        self.client_pid
    }

    /// Whether the stream is open.
    pub fn is_open(&self) -> bool {
        self.stream.is_some() && self.closed.is_none()
    }

    /// The next chunk; never resolves while no stream is open.
    pub async fn next(&mut self) -> Option<PaneChunk> {
        match (&mut self.stream, &self.closed) {
            (Some(stream), None) => stream.rx.recv().await,
            _ => std::future::pending().await,
        }
    }

    /// Applies one chunk; `None` (the sender gone) closes the stream.
    pub fn take(&mut self, chunk: Option<PaneChunk>) {
        let Some(chunk) = chunk else {
            self.closed = Some("the stream ended".to_owned());
            return;
        };
        self.chunks += 1;
        match &chunk {
            PaneChunk::Reset { bytes, .. } | PaneChunk::Bytes(bytes) => {
                self.bytes += bytes.len() as u64;
            }
            PaneChunk::Closed { reason } => self.closed = Some(reason.clone()),
        }
        self.term.apply(&chunk);
    }

    /// What it did; `client_counted` is left for the caller, who samples the clients.
    pub fn usage(&self) -> LiveViewUse {
        LiveViewUse {
            agent: self.agent.as_str().to_owned(),
            streams_opened: self.opens,
            open_at_end: self.is_open(),
            closed: self.closed.clone(),
            chunks: self.chunks,
            bytes: self.bytes,
            foreign_output_lines: self.view.foreign_output_lines(),
            client_pid: self.client_pid,
            client_counted: false,
        }
    }
}

/// The next chunk of `live`, or never when there is none.
pub async fn next_chunk(live: Option<&mut LiveView>) -> Option<PaneChunk> {
    match live {
        Some(live) => live.next().await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use duohive_core::testkit::{FakePaneView, n};

    use super::*;

    impl Viewer for FakePaneView {}

    #[tokio::test]
    async fn usage_counts_every_open_and_names_the_client() {
        let mut live = LiveView::new(Box::new(FakePaneView::default()), n("bench-1"), Some(42));
        live.open_stream().unwrap();
        assert_eq!(live.usage().streams_opened, 1);
        live.open_stream().unwrap();
        let usage = live.usage();
        assert_eq!(usage.streams_opened, 2);
        assert_eq!(usage.client_pid, Some(42));
    }
}
