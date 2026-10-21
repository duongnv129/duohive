//! The terminal UI: renders the engine's events and sends its commands.

pub mod app;
pub mod chat;
pub mod filter;
pub mod first_run;
pub mod focus;
pub mod format;
pub mod forms;
pub mod input;
pub mod keymap;
pub mod keys_out;
mod live;
pub mod model;
pub mod render;
pub mod suggest;
pub mod term_view;
pub mod theme;
pub mod tree;

#[cfg(test)]
mod fixtures;

pub use app::{
    AppConfig, InputEvent, QuitSignals, TerminalHost, TuiError, editor_command, quits_blocking,
    restore_terminal, run_app, run_app_on, show_blocking,
};
pub use chat::{ChatLines, Route, route};
pub use focus::{AgentMode, Focus, Layout, Pane, layout};
pub use format::{format_age, state_cell, truncate_middle};
pub use forms::{Chooser, Field, FieldId, FieldKind, Form, FormOutcome, LineEdit};
pub use input::{Action, UiEffect, map_key, paste, update};
pub use keys_out::key_to_input;
pub use model::{Confirm, Dialog, SharedDirs, UiModel};
pub use render::{
    BlockingScreen, LiveScreen, RenderCtx, live_size, render, render_blocking, render_live,
};
pub use term_view::TermView;
pub use theme::{Palette, palette_for};
pub use tree::{TreeRow, TreeRowKind, tree_rows};
