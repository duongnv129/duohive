use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use duohive_core::session_catalog::CatalogError;
use duohive_core::worktree::TaskId;
use tempfile::TempDir;

use super::{
    list_opencode_sessions, parse_opencode_sessions, run_opencode_session_list,
    run_session_list_with,
};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/sessions/opencode/list.json"
);
const LONG: Duration = Duration::from_secs(20);

fn recorded() -> String {
    fs::read_to_string(FIXTURE).unwrap()
}

fn task(name: &str) -> TaskId {
    TaskId(name.into())
}

fn owners() -> Vec<(PathBuf, Option<TaskId>)> {
    vec![
        (PathBuf::from("/work/payments"), None),
        (PathBuf::from("/work/payments-wt/fix"), Some(task("fix"))),
    ]
}

/// An executable shell script in `dir`.
fn script(dir: &Path, body: &str) -> String {
    let path = dir.join("opencode");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path.to_str().unwrap().to_owned()
}

fn unreadable(e: CatalogError) -> String {
    let CatalogError::Unreadable(reason) = e;
    reason
}

#[test]
fn opencode_list_parses_recorded_output() {
    let found = parse_opencode_sessions(&recorded(), &owners()).unwrap();
    let ids: Vec<&str> = found.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "ses_aaaa0000000000000000000001",
            "ses_aaaa0000000000000000000002"
        ]
    );
    assert_eq!(found[0].title, "Fix the retry loop");
    assert_eq!(found[0].updated_ms, 1_793_459_244_608);
    assert_eq!(found[0].dir, PathBuf::from("/work/payments"));
    assert_eq!(found[0].worktree, None);
    assert_eq!(found[1].dir, PathBuf::from("/work/payments-wt/fix"));
    assert_eq!(found[1].worktree, Some(task("fix")));
}

#[test]
fn opencode_list_keeps_only_given_directories() {
    let only = vec![(PathBuf::from("/work/payments-sibling"), None)];
    let found = parse_opencode_sessions(&recorded(), &only).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].title, "Sibling checkout work");
    let none = vec![(PathBuf::from("/elsewhere"), None)];
    assert!(
        parse_opencode_sessions(&recorded(), &none)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn opencode_list_has_no_branch_or_size() {
    let found = parse_opencode_sessions(&recorded(), &owners()).unwrap();
    assert!(
        found
            .iter()
            .all(|s| s.branch.is_none() && s.size_bytes == 0)
    );
}

#[test]
fn opencode_list_bad_json_is_unreadable() {
    for bad in ["not json", "{\"id\":\"x\"}", "", "\"a\""] {
        let err = parse_opencode_sessions(bad, &owners()).unwrap_err();
        assert_eq!(
            unreadable(err),
            "unrecognised OpenCode session list",
            "{bad}"
        );
    }
}

#[test]
fn opencode_list_skips_malformed_entries_and_dash_ids() {
    let json = r#"[
      {"id": "-rf", "title": "t", "updated": 5, "directory": "/work/payments"},
      {"id": "ses_ok", "title": "good", "updated": 4, "directory": "/work/payments"},
      {"id": "ses_nodir", "title": "x", "updated": 3},
      "junk",
      {"id": "ses_noupdated", "title": "x", "directory": "/work/payments"}
    ]"#;
    let found = parse_opencode_sessions(json, &owners()).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "ses_ok");
}

#[test]
fn opencode_list_skips_ids_that_start_with_a_dash() {
    let json =
        r#"[{"id": "--session", "title": "t", "updated": 1, "directory": "/work/payments"}]"#;
    assert!(parse_opencode_sessions(json, &owners()).unwrap().is_empty());
}

#[test]
fn opencode_titles_are_sanitised() {
    let long = "x".repeat(300);
    let json = serde_json::json!([
        {"id": "ses_a", "title": "\u{1b}[31mred\u{1b}[0m\u{7} and\nbroken\ttitle", "updated": 3, "directory": "/work/payments"},
        {"id": "ses_b", "title": long, "updated": 2, "directory": "/work/payments"},
        {"id": "ses_c", "title": "", "updated": 1, "directory": "/work/payments"},
    ])
    .to_string();
    let found = parse_opencode_sessions(&json, &owners()).unwrap();
    assert_eq!(found[0].title, "red and broken title");
    assert_eq!(found[1].title.chars().count(), 201);
    assert!(found[1].title.ends_with('…'));
    assert_eq!(found[2].title, "");
}

#[test]
fn opencode_list_symlinked_directory_matches() {
    let tmp = TempDir::new().unwrap();
    let real = fs::canonicalize(tmp.path()).unwrap().join("real");
    fs::create_dir(&real).unwrap();
    let link = real.parent().unwrap().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let json = serde_json::json!([
        {"id": "ses_a", "title": "t", "updated": 1, "directory": format!("{}/", real.display())},
    ])
    .to_string();
    let found = parse_opencode_sessions(&json, &[(link, None)]).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].dir, real);
}

#[test]
fn opencode_list_command_failure_is_unreadable_with_the_reason() {
    let tmp = TempDir::new().unwrap();
    let bin = script(
        tmp.path(),
        "echo >&2\necho 'database is locked' >&2\nexit 3",
    );
    let err = run_session_list_with(&bin, tmp.path(), LONG).unwrap_err();
    assert_eq!(unreadable(err), "database is locked");
    let quiet = script(tmp.path(), "exit 4");
    let err = run_session_list_with(&quiet, tmp.path(), LONG).unwrap_err();
    assert_eq!(unreadable(err), "exited with status 4");
    let err = run_opencode_session_list("/no/such/opencode", tmp.path()).unwrap_err();
    assert!(matches!(err, CatalogError::Unreadable(_)));
}

#[test]
fn opencode_list_command_times_out() {
    let tmp = TempDir::new().unwrap();
    let bin = script(tmp.path(), "sleep 30");
    let start = Instant::now();
    let err = run_session_list_with(&bin, tmp.path(), Duration::from_millis(200)).unwrap_err();
    assert_eq!(unreadable(err), "opencode session list timed out");
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[test]
fn opencode_list_passes_max_count() {
    let tmp = TempDir::new().unwrap();
    let work = fs::canonicalize(tmp.path()).unwrap();
    let bin = script(tmp.path(), "echo \"$@\"\npwd");
    let out = run_session_list_with(&bin, &work, LONG).unwrap();
    let mut lines = out.lines();
    assert_eq!(
        lines.next(),
        Some("session list --format json --max-count 1000")
    );
    assert_eq!(lines.next(), Some(work.to_str().unwrap()));
}

/// Two real directories under `tmp`, resolved, as requested owners.
fn real_owners(tmp: &Path) -> Vec<(PathBuf, Option<TaskId>)> {
    ["one", "two"]
        .iter()
        .map(|n| {
            let d = tmp.join(n);
            fs::create_dir(&d).unwrap();
            (fs::canonicalize(d).unwrap(), None)
        })
        .collect()
}

#[test]
fn opencode_list_merges_overlapping_runs_by_id() {
    let tmp = TempDir::new().unwrap();
    let dirs = real_owners(tmp.path());
    // Both runs print the same project list: one session per directory.
    let json = serde_json::json!([
        {"id": "ses_a", "title": "t", "updated": 2, "directory": dirs[0].0.to_str().unwrap()},
        {"id": "ses_b", "title": "t", "updated": 1, "directory": dirs[1].0.to_str().unwrap()},
    ])
    .to_string();
    let bin = script(tmp.path(), &format!("echo '{json}'"));
    let found = list_opencode_sessions(&bin, LONG, &dirs).unwrap();
    let ids: Vec<&str> = found.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["ses_a", "ses_b"]);
}

#[test]
fn opencode_list_all_runs_failing_returns_the_first_error() {
    let tmp = TempDir::new().unwrap();
    let dirs = real_owners(tmp.path());
    let bin = script(tmp.path(), "echo nope >&2\nexit 1");
    let err = list_opencode_sessions(&bin, LONG, &dirs).unwrap_err();
    assert_eq!(unreadable(err), "nope");
}

#[test]
fn opencode_list_keeps_what_the_successful_runs_found() {
    let tmp = TempDir::new().unwrap();
    // Fails in the first directory, works in the second.
    let first = tmp.path().join("one");
    let second = tmp.path().join("two");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    let second = fs::canonicalize(second).unwrap();
    let json = serde_json::json!([
        {"id": "ses_a", "title": "t", "updated": 1, "directory": second.to_str().unwrap()},
    ])
    .to_string();
    let body = format!("case \"$PWD\" in */one) exit 1;; esac\necho '{json}'");
    let bin = script(tmp.path(), &body);
    let dirs = vec![(first, None), (second, None)];
    let found = list_opencode_sessions(&bin, LONG, &dirs).unwrap();
    assert_eq!(found.len(), 1);
}
