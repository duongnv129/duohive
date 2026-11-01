//! Reads Claude Code's past sessions from `~/.claude/projects` (spike R1.2).
//!
//! Read only: nothing under `~/.claude` is written or deleted. Each file is read
//! as at most a head and a tail chunk, so a huge transcript is never read whole.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use duohive_core::session_catalog::{CatalogError, SessionInfo};
use duohive_core::worktree::TaskId;
use serde_json::Value;

/// How much of a file's start, and of its end, is read.
const CHUNK_BYTES: u64 = 256 * 1024;
/// A title or first prompt is cut after this many characters.
const TITLE_MAX_CHARS: usize = 200;
/// What `gitBranch` holds outside a git repository (R1.2).
const NO_BRANCH: &str = "HEAD";
/// The longest session id the engine accepts.
const ID_MAX_LEN: usize = 128;

/// Lists the Claude Code sessions that ran in any of `dirs`, newest first.
///
/// # Errors
///
/// `CatalogError::Unreadable` when a project folder can't be read, or when
/// session files exist but none has a recognisable record.
pub fn read_claude_sessions(
    home: &Path,
    dirs: &[(PathBuf, Option<TaskId>)],
) -> Result<Vec<SessionInfo>, CatalogError> {
    let mut sessions = Vec::new();
    let mut files_seen = 0_usize;
    let mut seen_dirs: Vec<PathBuf> = Vec::new();
    for (dir, task) in dirs {
        let resolved = fs::canonicalize(dir).unwrap_or_else(|_| dir.clone());
        if seen_dirs.contains(&resolved) {
            continue;
        }
        let folder = home
            .join(".claude")
            .join("projects")
            .join(folder_name(&resolved));
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(CatalogError::Unreadable(format!(
                    "can't read {}: {e}",
                    folder.display()
                )));
            }
        };
        for entry in entries {
            let entry = entry.map_err(|e| {
                CatalogError::Unreadable(format!("can't read {}: {e}", folder.display()))
            })?;
            let Some(id) = session_id(&entry.path()) else {
                continue;
            };
            files_seen += 1;
            if let Some(found) = read_session(&entry.path(), &id) {
                sessions.push(SessionInfo {
                    dir: resolved.clone(),
                    worktree: task.clone(),
                    ..found
                });
            }
        }
        seen_dirs.push(resolved);
    }
    if files_seen > 0 && sessions.is_empty() {
        return Err(CatalogError::Unreadable(
            "unrecognised Claude Code session format".into(),
        ));
    }
    sessions.sort_by(|a, b| {
        b.updated_ms
            .cmp(&a.updated_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(sessions)
}

/// The folder name Claude Code gives a working directory: every character that is
/// not `[A-Za-z0-9]` becomes `-` (R1.2).
fn folder_name(dir: &Path) -> String {
    dir.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The id of a `*.jsonl` file, when its stem follows the engine's id rule
/// `[A-Za-z0-9_][A-Za-z0-9_-]{0,127}`.
fn session_id(path: &Path) -> Option<String> {
    if path.extension()? != "jsonl" {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let mut chars = stem.chars();
    let first = chars.next()?;
    let valid = (first.is_ascii_alphanumeric() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && stem.len() <= ID_MAX_LEN;
    valid.then(|| stem.to_owned())
}

/// What the records of one file say.
#[derive(Default)]
struct Facts {
    recognised: bool,
    branch: Option<String>,
    title: Option<String>,
    first_prompt: Option<String>,
}

fn read_session(path: &Path, id: &str) -> Option<SessionInfo> {
    let meta = fs::metadata(path).ok().filter(fs::Metadata::is_file)?;
    let records = read_records(path, meta.len()).ok()?;
    let mut facts = Facts::default();
    for record in &records {
        absorb(&mut facts, record);
    }
    if !facts.recognised {
        return None;
    }
    let title = facts
        .title
        .filter(|t| !t.is_empty())
        .or(facts.first_prompt)
        .unwrap_or_default();
    let updated_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    Some(SessionInfo {
        id: id.to_owned(),
        title,
        updated_ms,
        branch: facts.branch,
        size_bytes: meta.len(),
        dir: PathBuf::new(),
        worktree: None,
        in_use_by: None,
    })
}

/// The JSON objects in the first and last `CHUNK_BYTES` of the file, in order.
/// A line cut by a chunk edge, and any line that isn't JSON, is dropped.
fn read_records(path: &Path, len: u64) -> io::Result<Vec<Value>> {
    let mut file = File::open(path)?;
    let mut records = Vec::new();
    if len <= 2 * CHUNK_BYTES {
        parse_lines(&read_range(&mut file, 0, len)?, false, false, &mut records);
        return Ok(records);
    }
    parse_lines(
        &read_range(&mut file, 0, CHUNK_BYTES)?,
        false,
        true,
        &mut records,
    );
    let tail = read_range(&mut file, len - CHUNK_BYTES, CHUNK_BYTES)?;
    parse_lines(&tail, true, false, &mut records);
    Ok(records)
}

fn read_range(file: &mut File, start: u64, count: u64) -> io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.take(count).read_to_end(&mut buf)?;
    Ok(buf)
}

fn parse_lines(bytes: &[u8], drop_first: bool, drop_last: bool, out: &mut Vec<Value>) {
    let mut lines: Vec<&[u8]> = bytes.split(|b| *b == b'\n').collect();
    if drop_last {
        lines.pop();
    }
    if drop_first && !lines.is_empty() {
        lines.remove(0);
    }
    out.extend(
        lines
            .into_iter()
            .filter_map(|l| serde_json::from_slice::<Value>(l).ok())
            .filter(Value::is_object),
    );
}

fn absorb(facts: &mut Facts, record: &Value) {
    let Some(kind) = record.get("type").and_then(Value::as_str) else {
        return;
    };
    facts.recognised = true;
    if let Some(branch) = record.get("gitBranch").and_then(Value::as_str) {
        facts.branch = (!branch.is_empty() && branch != NO_BRANCH).then(|| branch.to_owned());
    }
    match kind {
        "custom-title" => set_title(facts, record.get("customTitle")),
        "agent-name" => set_title(facts, record.get("agentName")),
        "user" if facts.first_prompt.is_none() => {
            facts.first_prompt = prompt_text(record)
                .map(|t| clean(&t))
                .filter(|t| !t.is_empty());
        }
        _ => {}
    }
}

fn set_title(facts: &mut Facts, value: Option<&Value>) {
    if let Some(text) = value.and_then(Value::as_str) {
        facts.title = Some(clean(text));
    }
}

/// The text of a user record, unless it is a meta prompt.
fn prompt_text(record: &Value) -> Option<String> {
    let content = record.get("message")?.get("content")?;
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .find_map(|b| b.get("text").and_then(Value::as_str))?
            .to_owned(),
        _ => return None,
    };
    let t = text.trim_start();
    (!t.starts_with("<system-reminder>") && !t.starts_with("<command-")).then_some(text)
}

/// Removes escape sequences and control characters (line breaks and tabs become
/// spaces), trims, and cuts at `TITLE_MAX_CHARS` with `…`.
fn clean(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => skip_escape(&mut chars),
            '\n' | '\r' | '\t' => out.push(' '),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    let out = out.trim();
    if out.chars().count() > TITLE_MAX_CHARS {
        let mut cut: String = out.chars().take(TITLE_MAX_CHARS).collect();
        cut.push('…');
        return cut;
    }
    out.to_owned()
}

/// Skips the rest of an escape sequence after `ESC`: CSI (`[` … final byte
/// `@`-`~`), OSC (`]` … BEL or `ESC \`), or a single following character.
fn skip_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match chars.next() {
        Some('[') => {
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        }
        Some(']') => {
            while let Some(c) = chars.next() {
                if c == '\u{7}' {
                    break;
                }
                if c == '\u{1b}' {
                    chars.next();
                    break;
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "claude_sessions_tests.rs"]
mod tests;
