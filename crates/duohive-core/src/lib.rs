//! Domain core: pure types and ports with no I/O dependencies.

pub mod adapter;
pub mod alert;
pub mod card;
pub mod chat;
pub mod cli_kind;
pub mod command;
pub mod duo;
pub mod engine;
pub mod event;
pub mod hook;
pub mod inbox;
pub mod inherit;
pub mod launch;
pub mod names;
pub mod org;
pub mod ports;
pub mod settings;
pub mod status;
pub mod templates;
pub mod time;
pub mod tool;
pub mod validate;
pub mod view;
pub mod worktree;

#[cfg(any(test, feature = "testkit"))]
#[expect(clippy::unwrap_used, reason = "test-only helpers")]
pub mod testkit;
