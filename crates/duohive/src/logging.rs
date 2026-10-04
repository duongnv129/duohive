//! Logging to `<data_dir>/logs/`, never the terminal (the TUI owns it).
//!
//! The app writes one file a day, `duohive.<date>.log`, and keeps the last
//! [`KEEP_DAYS`]. `duohive hook` writes nothing here but `hook.log`, which it
//! caps itself (Review Focus 1).

use std::fs::{self, DirBuilder, Permissions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::Path;

use duohive_store::Paths;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{InitError, RollingFileAppender, Rotation};

/// `logs/` is the user's alone.
const LOGS_DIR_MODE: u32 = 0o700;
/// Daily files kept.
const KEEP_DAYS: usize = 7;

/// Logging could not start.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    /// `logs/` cannot be created or made owner-only.
    #[error("cannot create the logs directory: {0}")]
    Dir(#[from] io::Error),
    /// The daily file cannot be opened.
    #[error("cannot open the log file: {0}")]
    File(#[from] InitError),
    /// A global subscriber is already set.
    #[error("logging is already set up")]
    AlreadySet,
}

/// Creates `dir` (and its parents) owner-only, and makes it owner-only if it exists.
///
/// # Errors
/// The directory cannot be created or its mode set.
pub fn ensure_logs_dir(dir: &Path) -> io::Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(LOGS_DIR_MODE)
        .create(dir)?;
    fs::set_permissions(dir, Permissions::from_mode(LOGS_DIR_MODE))
}

/// Sends `tracing` events at info and above to a daily file in `logs/`, through a
/// background writer. Keep the guard until exit: dropping it flushes the file.
///
/// # Errors
/// See [`LogError`].
pub fn init_logging(paths: &Paths) -> Result<WorkerGuard, LogError> {
    let dir = paths.logs_dir();
    ensure_logs_dir(&dir)?;
    let file = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("duohive")
        .filename_suffix("log")
        .max_log_files(KEEP_DAYS)
        .build(&dir)?;
    let (writer, guard) = tracing_appender::non_blocking(file);
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).map_err(|_| LogError::AlreadySet)?;
    Ok(guard)
}
