//! Logging to `<data_dir>/logs/`, never the terminal (the TUI owns it).
//!
//! The app writes one owner-only file a day (UTC), `duohive.<date>.log`, and keeps
//! the last [`KEEP_DAYS`]. `duohive hook` writes nothing here but `hook.log`, which
//! it rotates itself (Review Focus 1).

use std::fs::{self, DirBuilder, File, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use duohive_store::Paths;
use tracing_appender::non_blocking::WorkerGuard;

/// `logs/` is the user's alone.
const LOGS_DIR_MODE: u32 = 0o700;
/// And so is every log file.
const LOG_FILE_MODE: u32 = 0o600;
/// Daily files kept.
const KEEP_DAYS: usize = 7;
const SECS_PER_DAY: u64 = 86_400;
const PREFIX: &str = "duohive.";
const SUFFIX: &str = ".log";

/// Logging could not start.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    /// `logs/` cannot be created or made owner-only.
    #[error("cannot create the logs directory: {0}")]
    Dir(#[source] io::Error),
    /// The daily file cannot be opened.
    #[error("cannot open the log file: {0}")]
    File(#[source] io::Error),
    /// A global subscriber is already set.
    #[error("logging is already set up")]
    AlreadySet,
}

/// Creates `dir` (and its parents) owner-only; an existing `dir` is made owner-only
/// only when group or others can reach it.
///
/// # Errors
/// The directory cannot be created, read or have its mode set.
pub fn ensure_logs_dir(dir: &Path) -> io::Result<()> {
    match fs::metadata(dir) {
        Ok(m) if m.permissions().mode() & 0o077 != 0 => {
            fs::set_permissions(dir, Permissions::from_mode(LOGS_DIR_MODE))
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => DirBuilder::new()
            .recursive(true)
            .mode(LOGS_DIR_MODE)
            .create(dir),
        Err(e) => Err(e),
    }
}

/// Sends `tracing` events at info and above to the day's file in `logs/`, through a
/// background writer. Keep the guard until exit: dropping it flushes the file.
///
/// # Errors
/// See [`LogError`].
pub fn init_logging(paths: &Paths) -> Result<WorkerGuard, LogError> {
    let dir = paths.logs_dir();
    ensure_logs_dir(&dir).map_err(LogError::Dir)?;
    let file = DailyFile::open(dir, today).map_err(LogError::File)?;
    let (writer, guard) = tracing_appender::non_blocking(file);
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).map_err(|_| LogError::AlreadySet)?;
    Ok(guard)
}

/// The day's log file (mode 0600), reopened when the UTC date changes, pruning
/// all but the last [`KEEP_DAYS`] files on each open.
pub(crate) struct DailyFile {
    dir: PathBuf,
    /// Days since 1970-01-01 now (injected for tests).
    today: fn() -> u64,
    day: u64,
    file: File,
}

impl DailyFile {
    /// Opens today's file in `dir`.
    pub(crate) fn open(dir: PathBuf, today: fn() -> u64) -> io::Result<Self> {
        let day = today();
        let file = open_day(&dir, day)?;
        prune(&dir);
        Ok(Self {
            dir,
            today,
            day,
            file,
        })
    }
}

impl Write for DailyFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let now = (self.today)();
        if now != self.day {
            // A new day: keep writing to the old file if the new one won't open.
            if let Ok(file) = open_day(&self.dir, now) {
                self.file = file;
                self.day = now;
                prune(&self.dir);
            }
        }
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn open_day(dir: &Path, day: u64) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(LOG_FILE_MODE)
        .open(dir.join(log_name(day)))
}

/// Removes all but the newest [`KEEP_DAYS`] daily files (names sort by date).
fn prune(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut logs: Vec<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with(PREFIX) && n.ends_with(SUFFIX))
        .collect();
    logs.sort();
    let old = logs.len().saturating_sub(KEEP_DAYS);
    for name in &logs[..old] {
        let _ = fs::remove_file(dir.join(name));
    }
}

/// Days since 1970-01-01, UTC.
fn today() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / SECS_PER_DAY)
}

/// `duohive.<yyyy-mm-dd>.log` for `day` days since 1970-01-01.
pub(crate) fn log_name(day: u64) -> String {
    let (y, m, d) = civil_date(day);
    format!("{PREFIX}{y:04}-{m:02}-{d:02}{SUFFIX}")
}

/// The proleptic Gregorian date `days` after 1970-01-01 (H. Hinnant's `civil_from_days`).
pub(crate) fn civil_date(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + u64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn mode(p: &Path) -> u32 {
        fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn dates_are_civil_utc() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(19_723), (2024, 1, 1));
        assert_eq!(civil_date(19_782), (2024, 2, 29));
        assert_eq!(log_name(20_365), "duohive.2025-10-04.log");
    }

    #[test]
    fn daily_files_are_owner_only_and_old_ones_pruned() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for day in 0..10 {
            fs::write(dir.join(log_name(20_000 + day)), "old").unwrap();
        }
        fs::write(dir.join("hook.log"), "kept").unwrap();
        let mut file = DailyFile::open(dir.to_path_buf(), || 20_365).unwrap();
        file.write_all(b"hello\n").unwrap();
        file.flush().unwrap();
        let today = dir.join(log_name(20_365));
        assert_eq!(fs::read_to_string(&today).unwrap(), "hello\n");
        assert_eq!(mode(&today), 0o600);
        let mut logs: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.starts_with("duohive."))
            .collect();
        logs.sort();
        assert_eq!(logs.len(), KEEP_DAYS, "{logs:?}");
        assert_eq!(logs.last(), Some(&log_name(20_365)));
        assert!(dir.join("hook.log").exists());
    }

    #[test]
    fn the_logs_dir_is_made_owner_only_only_when_needed() {
        let tmp = tempfile::tempdir().unwrap();
        let fresh = tmp.path().join("a/logs");
        ensure_logs_dir(&fresh).unwrap();
        assert_eq!(mode(&fresh), 0o700);
        let open = tmp.path().join("open");
        fs::create_dir(&open).unwrap();
        fs::set_permissions(&open, Permissions::from_mode(0o755)).unwrap();
        ensure_logs_dir(&open).unwrap();
        assert_eq!(mode(&open), 0o700);
        // Already owner-only (here stricter): left alone.
        let strict = tmp.path().join("strict");
        fs::create_dir(&strict).unwrap();
        fs::set_permissions(&strict, Permissions::from_mode(0o500)).unwrap();
        ensure_logs_dir(&strict).unwrap();
        assert_eq!(mode(&strict), 0o500);
    }
}
