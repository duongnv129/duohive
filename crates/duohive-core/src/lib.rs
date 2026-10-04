//! Domain core: pure types and ports with no I/O dependencies.

pub mod cli_kind;
pub mod names;
pub mod org;
pub mod time;

#[cfg(any(test, feature = "testkit"))]
#[allow(clippy::unwrap_used, clippy::expect_used, reason = "test-only helpers")]
pub mod testkit;
