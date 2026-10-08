//! duohive-tmux: tmux control-mode adapter.

mod activity;
mod connection;
pub mod control;
pub mod quote;
pub mod runtime;
mod server;
pub mod version;
pub mod view;

pub use control::{ControlLine, ControlParser};
pub use quote::{QuoteError, tmux_quote};
pub use runtime::{TmuxConfig, TmuxError, TmuxRuntime, tmux_version};
pub use version::{TmuxVersion, parse_version};
pub use view::TmuxPaneView;
