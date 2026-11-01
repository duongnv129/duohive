//! Lists OpenCode's past sessions through `opencode session list` (spike R2).
//!
//! Read only: OpenCode's config and data are never modified, and no
//! `OPENCODE_CONFIG` is set. The listing is per project (root commit), so a
//! worktree and its sibling checkouts share one list; the `directory` field
//! of each entry decides which sessions belong to the requested directories.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use duohive_core::session_catalog::{CatalogError, SessionInfo};
use duohive_core::worktree::TaskId;
use serde_json::Value;

use crate::claude_sessions::{clean, is_valid_session_id};
use crate::detect::{kill_group, spawn_waiting_out_busy};

/// The most sessions asked for; OpenCode's default of 100 would hide older ones (R2.1).
const MAX_COUNT: &str = "1000";
/// How long the listing may take before the child is killed.
const LIST_TIMEOUT: Duration = Duration::from_secs(10);
/// At most this much of stdout is read.
const MAX_STDOUT_BYTES: u64 = 8 * 1024 * 1024;
/// At most this much of stderr is read.
const MAX_STDERR_BYTES: u64 = 64 * 1024;
/// How long to wait for stderr once the child has ended.
const STDERR_GRACE: Duration = Duration::from_millis(500);
const POLL: Duration = Duration::from_millis(10);

/// A requested directory (resolved) and the worktree task it belongs to.
type Owner = (PathBuf, Option<TaskId>);

fn resolve(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Reads the output of `opencode session list --format json` and keeps the
/// sessions that ran in one of `dirs`, newest first.
///
/// OpenCode records neither a git branch nor a size (R2.1), so `branch` is
/// `None` and `size_bytes` is 0 for every session; the picker omits both for
/// OpenCode. A session that matches two requested directories is attributed to
/// the first in `dirs`.
///
/// # Errors
///
/// `CatalogError::Unreadable` when `json` is not a JSON array. Individual
/// malformed entries, and ids that break the engine's id rule, are skipped.
pub fn parse_opencode_sessions(
    json: &str,
    dirs: &[(PathBuf, Option<TaskId>)],
) -> Result<Vec<SessionInfo>, CatalogError> {
    let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(json) else {
        return Err(CatalogError::Unreadable(
            "unrecognised OpenCode session list".into(),
        ));
    };
    let owners: Vec<Owner> = dirs.iter().map(|(d, t)| (resolve(d), t.clone())).collect();
    let mut sessions: Vec<SessionInfo> = entries
        .iter()
        .filter_map(|entry| session_from(entry, &owners))
        .collect();
    sessions.sort_by(|a, b| {
        b.updated_ms
            .cmp(&a.updated_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(sessions)
}

fn session_from(entry: &Value, owners: &[Owner]) -> Option<SessionInfo> {
    let id = entry
        .get("id")?
        .as_str()
        .filter(|i| is_valid_session_id(i))?;
    let directory = resolve(Path::new(entry.get("directory")?.as_str()?));
    let updated_ms = entry.get("updated")?.as_u64()?;
    let (dir, worktree) = owners.iter().find(|(d, _)| *d == directory)?;
    Some(SessionInfo {
        id: id.to_owned(),
        title: entry
            .get("title")
            .and_then(Value::as_str)
            .map(clean)
            .unwrap_or_default(),
        updated_ms,
        branch: None,
        size_bytes: 0,
        dir: dir.clone(),
        worktree: worktree.clone(),
        in_use_by: None,
    })
}

/// Runs `<binary> session list --format json --max-count 1000` in `dir` and
/// returns its stdout. The only function here that starts a process.
///
/// # Errors
///
/// `CatalogError::Unreadable` when the process can't start, takes over 10 s
/// (it is killed), or exits non-zero (the first line of its stderr, or the
/// status, is the reason).
pub fn run_opencode_session_list(binary: &str, dir: &Path) -> Result<String, CatalogError> {
    run_session_list_with(binary, dir, LIST_TIMEOUT)
}

fn unreadable(reason: impl Into<String>) -> CatalogError {
    CatalogError::Unreadable(reason.into())
}

/// Reads at most `limit` bytes of `stream` on a helper thread.
fn read_bounded<R: Read + Send + 'static>(
    stream: Option<R>,
    limit: u64,
) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    if let Some(stream) = stream {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            // A read error leaves what was read so far.
            let _ = stream.take(limit).read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
    }
    rx
}

pub(crate) fn run_session_list_with(
    binary: &str,
    dir: &Path,
    timeout: Duration,
) -> Result<String, CatalogError> {
    let mut child = spawn_waiting_out_busy(
        Command::new(binary)
            .args([
                "session",
                "list",
                "--format",
                "json",
                "--max-count",
                MAX_COUNT,
            ])
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Its own process group, so a timeout stops everything it started.
            .process_group(0),
    )
    .map_err(|e| unreadable(format!("can't run {binary}: {e}")))?;
    let stdout = read_bounded(child.stdout.take(), MAX_STDOUT_BYTES);
    let stderr = read_bounded(child.stderr.take(), MAX_STDERR_BYTES);
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
            Ok(None) => {
                kill_group(child.id());
                let _ = child.kill();
                let _ = child.wait();
                return Err(unreadable("opencode session list timed out"));
            }
            Err(e) => return Err(unreadable(format!("can't wait for {binary}: {e}"))),
        }
    };
    let out = stdout.recv_timeout(STDERR_GRACE).unwrap_or_default();
    if status.success() {
        return Ok(String::from_utf8_lossy(&out).into_owned());
    }
    let err = stderr.recv_timeout(STDERR_GRACE).unwrap_or_default();
    let reason = String::from_utf8_lossy(&err)
        .lines()
        .map(clean)
        .find(|l| !l.is_empty())
        .unwrap_or_else(|| match status.code() {
            Some(code) => format!("exited with status {code}"),
            None => "exited with a signal".to_owned(),
        });
    Err(unreadable(reason))
}

/// Runs the listing once per distinct requested directory (a project's
/// worktrees share one list, so results overlap) and merges them by session id,
/// first wins. All runs failing returns the first error.
pub(crate) fn list_opencode_sessions(
    binary: &str,
    timeout: Duration,
    dirs: &[(PathBuf, Option<TaskId>)],
) -> Result<Vec<SessionInfo>, CatalogError> {
    let mut seen_dirs = HashSet::new();
    let mut seen_ids = HashSet::new();
    let mut merged = Vec::new();
    let mut first_error = None;
    let mut any_ok = false;
    for (dir, _) in dirs {
        if !seen_dirs.insert(resolve(dir)) {
            continue;
        }
        let parsed = run_session_list_with(binary, dir, timeout)
            .and_then(|json| parse_opencode_sessions(&json, dirs));
        match parsed {
            Ok(found) => {
                any_ok = true;
                merged.extend(found.into_iter().filter(|s| seen_ids.insert(s.id.clone())));
            }
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    match (any_ok, first_error) {
        (false, Some(e)) => Err(e),
        _ => {
            merged.sort_by(|a, b| {
                b.updated_ms
                    .cmp(&a.updated_ms)
                    .then_with(|| a.id.cmp(&b.id))
            });
            Ok(merged)
        }
    }
}

/// The adapter's entry point: the real `opencode` binary, 10 s limit.
pub(crate) fn list_with_default_binary(
    binary: &str,
    dirs: &[(PathBuf, Option<TaskId>)],
) -> Result<Vec<SessionInfo>, CatalogError> {
    list_opencode_sessions(binary, LIST_TIMEOUT, dirs)
}

#[cfg(test)]
#[path = "opencode_sessions_tests.rs"]
mod tests;
