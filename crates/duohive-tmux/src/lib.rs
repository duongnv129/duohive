//! duohive-tmux: tmux control-mode adapter.

pub mod control;
pub mod quote;
pub mod version;

pub use control::{ControlLine, ControlParser};
pub use quote::{QuoteError, tmux_quote};
pub use version::{TmuxVersion, parse_version};
