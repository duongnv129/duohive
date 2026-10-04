//! The terminal UI: renders the engine's events and sends its commands.

pub mod app;
pub mod format;
pub mod forms;
pub mod input;
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

pub use app::{
    AppConfig, InputEvent, TerminalHost, TuiError, editor_command, quits_blocking,
    restore_terminal, run_app, run_app_on, show_blocking,
};
pub use format::{format_age, state_cell, truncate_middle};
pub use forms::{Chooser, Field, FieldId, FieldKind, Form, FormOutcome, LineEdit};
pub use input::{Action, UiEffect, map_key, paste, update};
pub use model::{Confirm, Dialog, SharedDirs, UiModel};
pub use render::{BlockingScreen, RenderCtx, render, render_blocking};
pub use theme::{Palette, palette_for};
pub use tree::{TreeRow, TreeRowKind, tree_rows};
