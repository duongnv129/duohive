//! The terminal UI: renders the engine's events and sends its commands.

pub mod format;
pub mod model;
pub mod render;
pub mod theme;
pub mod tree;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test-only fixtures"
)]
mod fixtures;

pub use format::{format_age, state_cell, truncate_middle};
pub use model::{Confirm, Dialog, UiModel};
pub use render::{BlockingScreen, RenderCtx, render, render_blocking};
pub use theme::{Palette, palette_for};
pub use tree::{TreeRow, TreeRowKind, tree_rows};
