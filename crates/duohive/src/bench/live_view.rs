//! `duohive bench --view`: one live pane stream, held open from the moment
//! every agent has started to the end of the idle window (P2-D4), fed into a
//! terminal emulator as the app's agent pane would be.

use duohive_core::names::AgentName;
use duohive_core::ports::PaneView;
use duohive_core::view::{PaneChunk, PaneSize, PaneStream};
use duohive_tmux::{TmuxConfig, TmuxPaneView};
use duohive_tui::TermView;

use super::BenchError;

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
    /// Streams opened over the run: one.
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
    /// Whether the view's tmux client is inside DuoHive's CPU and memory.
    pub client_counted: bool,
}

/// The view, its one stream and the terminal it draws into.
pub struct LiveView {
    agent: AgentName,
    view: TmuxPaneView,
    stream: PaneStream,
    term: TermView,
    chunks: u64,
    bytes: u64,
    closed: Option<String>,
}

impl LiveView {
    /// Attaches the view's client (off the async loop: it blocks) and opens
    /// `agent`'s stream.
    ///
    /// # Errors
    /// The attach or the open fails.
    pub async fn open(cfg: TmuxConfig, agent: AgentName) -> Result<Self, BenchError> {
        let view = tokio::task::spawn_blocking(move || TmuxPaneView::new(&cfg))
            .await
            .map_err(|e| BenchError::View(e.to_string()))?
            .map_err(|e| BenchError::View(e.to_string()))?;
        let stream = view
            .open(&agent, SIZE)
            .map_err(|e| BenchError::View(e.to_string()))?;
        Ok(Self {
            agent,
            view,
            stream,
            term: TermView::new(SIZE),
            chunks: 0,
            bytes: 0,
            closed: None,
        })
    }

    /// Whether the stream is still open.
    pub fn is_open(&self) -> bool {
        self.closed.is_none()
    }

    /// The next chunk; never resolves once the stream has closed.
    pub async fn next(&mut self) -> Option<PaneChunk> {
        if self.closed.is_some() {
            return std::future::pending().await;
        }
        self.stream.rx.recv().await
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

    /// What it did, with whether its client was counted.
    pub fn usage(&self, client_counted: bool) -> LiveViewUse {
        LiveViewUse {
            agent: self.agent.as_str().to_owned(),
            streams_opened: 1,
            open_at_end: self.is_open(),
            closed: self.closed.clone(),
            chunks: self.chunks,
            bytes: self.bytes,
            foreign_output_lines: self.view.foreign_output_lines(),
            client_counted,
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
