//! Domain core: pure types and ports with no I/O dependencies.

#[cfg(any(test, feature = "testkit"))]
#[allow(clippy::unwrap_used, clippy::expect_used, reason = "test-only helpers")]
pub mod testkit;
