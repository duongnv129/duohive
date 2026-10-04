//! Domain core: pure types and ports with no I/O dependencies.

pub mod adapter;
pub mod cli_kind;
pub mod command;
pub mod engine;
pub mod event;
pub mod hook;
pub mod inherit;
pub mod launch;
pub mod names;
pub mod org;
pub mod ports;
pub mod settings;
pub mod status;
pub mod time;
pub mod validate;

#[cfg(any(test, feature = "testkit"))]
#[allow(clippy::unwrap_used, clippy::expect_used, reason = "test-only helpers")]
pub mod testkit;
