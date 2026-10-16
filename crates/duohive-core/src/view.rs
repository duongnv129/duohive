//! The live pane view: types for streaming one agent's pane and sending it input.

use serde::{Deserialize, Serialize};

use crate::names::AgentName;

/// A pane's size in character cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneSize {
    /// Columns.
    pub cols: u16,
    /// Rows.
    pub rows: u16,
}

/// Names one open pane stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StreamId(pub u64);

/// What the user typed into the pane.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneInput {
    /// Literal text.
    Text(String),
    /// A named key.
    Key(PaneKey),
    /// Text sent as a bracketed paste.
    Paste(String),
    /// One wheel tick for a CLI that turned on mouse reporting (P2-D25). `col` and
    /// `row` are 1-based within the pane; `sgr` picks SGR over X10 encoding.
    Mouse {
        /// Which way the wheel turned.
        wheel: Wheel,
        /// The pointer's column in the pane, from 1.
        col: u16,
        /// The pointer's row in the pane, from 1.
        row: u16,
        /// The CLI asked for SGR encoding (mode 1006).
        sgr: bool,
    },
}

/// A wheel tick's direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Wheel {
    /// Away from the user: older lines.
    Up,
    /// Towards the user: newer lines.
    Down,
}

/// A key the pane understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneKey {
    /// Enter.
    Enter,
    /// Tab.
    Tab,
    /// Shift-Tab.
    BackTab,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Escape.
    Escape,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Insert.
    Insert,
    /// A function key, `F(1)` to `F(12)`.
    F(u8),
    /// Control plus a character.
    Ctrl(char),
    /// Alt plus a character.
    Alt(char),
}

/// What a pane stream delivers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneChunk {
    /// The screen as it is now: repaint from these bytes, then put the cursor at `cursor` (column, row).
    Reset {
        /// Terminal bytes that redraw the screen.
        bytes: Vec<u8>,
        /// Cursor column and row.
        cursor: (u16, u16),
    },
    /// New output.
    Bytes(Vec<u8>),
    /// The stream ended.
    Closed {
        /// Why.
        reason: String,
    },
}

/// An open pane stream.
#[derive(Debug)]
pub struct PaneStream {
    /// Which stream.
    pub id: StreamId,
    /// Its chunks, oldest first.
    pub rx: tokio::sync::mpsc::Receiver<PaneChunk>,
}

/// A pane view failure.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ViewError {
    /// The agent has no running window.
    #[error("{0} is not running")]
    NotRunning(AgentName),
    /// The stream is closed or unknown.
    #[error("stream closed")]
    Closed,
    /// tmux failed.
    #[error("{0}")]
    Tmux(String),
}
