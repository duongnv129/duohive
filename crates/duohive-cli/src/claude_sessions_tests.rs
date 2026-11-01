use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use duohive_core::session_catalog::CatalogError;
use duohive_core::worktree::TaskId;
use tempfile::TempDir;

use super::read_claude_sessions;

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/sessions/claude"
);
const TITLED: &str = "00000000-0000-4000-8000-000000000001";
const PLAIN: &str = "00000000-0000-4000-8000-000000000002";

/// A fake home plus a work area, both under one temp dir.
struct Env {
    tmp: TempDir,
}

impl Env {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("home")).unwrap();
        fs::create_dir_all(tmp.path().join("work")).unwrap();
        Self { tmp }
    }
    fn home(&self) -> PathBuf {
        self.tmp.path().join("home")
    }
    /// A real directory under the work area, resolved.
    fn dir(&self, name: &str) -> PathBuf {
        let d = self.tmp.path().join("work").join(name);
        fs::create_dir_all(&d).unwrap();
        fs::canonicalize(d).unwrap()
    }
    /// The project folder Claude Code would use for `dir` (R1.2), made if missing.
    fn folder(&self, dir: &Path) -> PathBuf {
        let name: String = dir
            .to_str()
            .unwrap()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let f = self.home().join(".claude/projects").join(name);
        fs::create_dir_all(&f).unwrap();
        f
    }
    fn put(&self, dir: &Path, id: &str, contents: &str, age_s: u64) -> PathBuf {
        let p = self.folder(dir).join(format!("{id}.jsonl"));
        fs::write(&p, contents).unwrap();
        age(&p, age_s);
        p
    }
    fn put_fixture(&self, dir: &Path, id: &str, age_s: u64) -> PathBuf {
        let text = fs::read_to_string(Path::new(FIXTURES).join(format!("{id}.jsonl"))).unwrap();
        self.put(dir, id, &text, age_s)
    }
}

/// Sets the mtime to `age_s` seconds before a fixed point.
fn age(p: &Path, age_s: u64) {
    let t = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000_000 - age_s);
    fs::OpenOptions::new()
        .write(true)
        .open(p)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

fn user(prompt: &str) -> String {
    format!(
        "{}\n",
        serde_json::json!({"type":"user","gitBranch":"main","message":{"role":"user","content":prompt}})
    )
}

fn list(
    env: &Env,
    dirs: &[(PathBuf, Option<TaskId>)],
) -> Vec<duohive_core::session_catalog::SessionInfo> {
    read_claude_sessions(&env.home(), dirs).unwrap()
}

#[test]
fn claude_catalog_lists_newest_first() {
    let env = Env::new();
    let d = env.dir("proj");
    env.put_fixture(&d, TITLED, 100);
    env.put_fixture(&d, PLAIN, 10);
    let got = list(&env, &[(d.clone(), None)]);
    let ids: Vec<_> = got.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, [PLAIN, TITLED]);
    assert_eq!(got[0].updated_ms, (2_000_000_000 - 10) * 1000);
    assert_eq!(got[0].dir, d);
    assert_eq!(got[0].worktree, None);
    assert_eq!(got[0].in_use_by, None);
    assert_eq!(got[0].branch, None, "HEAD means no branch (R1.2)");
    assert_eq!(got[1].branch.as_deref(), Some("feature-x"));
    assert_eq!(got[1].title, "Parser fix");
    assert!(got[1].size_bytes > 0);
}

#[test]
fn claude_catalog_filters_to_the_given_directories() {
    let env = Env::new();
    let (a, b) = (env.dir("a"), env.dir("b"));
    env.put_fixture(&a, TITLED, 5);
    env.put_fixture(&b, PLAIN, 5);
    let got = list(&env, &[(a, None)]);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].id, TITLED);
}

#[test]
fn claude_catalog_names_the_worktree() {
    let env = Env::new();
    let (main, wt) = (env.dir("main"), env.dir("wt"));
    env.put_fixture(&main, TITLED, 50);
    env.put_fixture(&wt, PLAIN, 5);
    let task = TaskId("fix-1".into());
    let got = list(&env, &[(main, None), (wt.clone(), Some(task.clone()))]);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].worktree, Some(task));
    assert_eq!(got[0].dir, wt);
    assert_eq!(got[1].worktree, None);
}

#[test]
fn claude_catalog_title_falls_back_to_first_prompt() {
    let env = Env::new();
    let d = env.dir("p");
    env.put_fixture(&d, PLAIN, 5);
    let long = "x".repeat(300);
    env.put(&d, "long", &user(&long), 9);
    let got = list(&env, &[(d, None)]);
    assert_eq!(got[0].title, "Explain the build setup");
    assert_eq!(got[1].title.chars().count(), 201);
    assert!(got[1].title.ends_with('…'));
    let empty = Env::new();
    let e = empty.dir("e");
    empty.put(&e, "none", "{\"type\":\"mode\"}\n", 1);
    assert_eq!(list(&empty, &[(e, None)])[0].title, "");
}

#[test]
fn claude_catalog_skips_unreadable_files() {
    let env = Env::new();
    let d = env.dir("p");
    env.put_fixture(&d, TITLED, 5);
    env.put(&d, "truncated", "{\"type\":\"user\",\"message\":{\"con", 6);
    env.put(&d, "empty", "", 7);
    env.put(&d, "notjson", "hello world\n\u{0}\u{1}\n", 8);
    let got = list(&env, &[(d, None)]);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].id, TITLED);
}

#[test]
fn title_strips_control_characters() {
    let env = Env::new();
    let d = env.dir("p");
    let rec = serde_json::json!({"type":"user","message":{"content":"\u{1b}[31mred\u{1b}[0m\u{7}bell\u{1b}]0;x\u{7} a\nb\tc\u{0}"}});
    env.put(&d, "ansi", &format!("{rec}\n"), 5);
    let t = serde_json::json!({"type":"custom-title","customTitle":"\u{1b}[1mBold\u{1b}[0m\u{8}"});
    env.put(&d, "named", &format!("{t}\n"), 6);
    let got = list(&env, &[(d, None)]);
    assert_eq!(got[0].title, "redbell a b c");
    assert_eq!(got[1].title, "Bold");
}

#[test]
fn directory_match_resolves_symlinks_and_slashes() {
    let env = Env::new();
    let real = env.dir("real");
    env.put_fixture(&real, TITLED, 5);
    let link = env.tmp.path().join("work/link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let mut slashed = real.clone().into_os_string();
    slashed.push("/");
    for d in [link, PathBuf::from(slashed)] {
        let got = list(&env, &[(d, None)]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].dir, real);
    }
}

#[test]
fn claude_catalog_reads_metadata_only() {
    let env = Env::new();
    let d = env.dir("p");
    let head = user("head prompt");
    let tail = format!(
        "{}{}",
        serde_json::json!({"type":"custom-title","customTitle":"Big one"}),
        "\n"
    );
    let p = env.put(&d, "big", &head, 5);
    {
        use std::io::{Seek, SeekFrom, Write};
        let mut f = fs::OpenOptions::new().write(true).open(&p).unwrap();
        f.set_len(100 * 1024 * 1024).unwrap();
        let back = i64::try_from(tail.len()).unwrap() + 1;
        f.seek(SeekFrom::End(-back)).unwrap();
        f.write_all(b"\n").unwrap();
        f.write_all(tail.as_bytes()).unwrap();
    }
    age(&p, 5);
    let got = list(&env, &[(d, None)]);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].size_bytes, 100 * 1024 * 1024);
    // The middle is zero bytes (not even one JSON line): only head and tail are read.
    assert_eq!(got[0].title, "Big one");
    assert_eq!(got[0].branch.as_deref(), Some("main"));
}

#[test]
fn untested_format_is_unreadable_not_a_crash() {
    let env = Env::new();
    let d = env.dir("p");
    env.put(&d, "weird", "{\"hello\":1}\n[1,2]\n\"x\"\n", 5);
    env.put(&d, "other", "binary\u{0}\u{1}", 6);
    let err = read_claude_sessions(&env.home(), &[(d, None)]).unwrap_err();
    assert_eq!(
        err,
        CatalogError::Unreadable("unrecognised Claude Code session format".into())
    );
}

#[test]
fn missing_project_folder_is_empty_not_an_error() {
    let env = Env::new();
    let d = env.dir("nothing");
    assert!(list(&env, &[(d, None)]).is_empty());
    let gone = env.tmp.path().join("work/does-not-exist");
    assert!(list(&env, &[(gone, None)]).is_empty());
}

#[test]
fn ids_that_start_with_a_dash_are_skipped() {
    let env = Env::new();
    let d = env.dir("p");
    env.put(&d, "-rf", &user("hi"), 5);
    env.put(&d, "bad.name", &user("hi"), 6);
    env.put(&d, "good_1", &user("hi"), 7);
    let got = list(&env, &[(d, None)]);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].id, "good_1");
}
