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
        // The fixtures were recorded elsewhere: point their cwd at `dir`.
        let here = format!("\"cwd\":{}", serde_json::json!(dir));
        let text = text
            .replace("\"cwd\":\"/work/demo\"", &here)
            .replace("\"cwd\":\"/work/plain\"", &here);
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
fn claude_catalog_title_falls_back_to_the_last_prompt() {
    let env = Env::new();
    let d = env.dir("p");
    env.put_fixture(&d, PLAIN, 5);
    let long = "x".repeat(300);
    env.put(&d, "long", &user(&long), 9);
    let got = list(&env, &[(d, None)]);
    // The fixture holds a `last-prompt` record after its real prompt.
    assert_eq!(got[0].title, "Something later");
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

/// Writes `bytes` at `offset`, growing the file (sparse) when needed.
fn write_at(path: &Path, offset: u64, bytes: &[u8]) {
    use std::io::{Seek, SeekFrom, Write};
    let mut f = fs::OpenOptions::new().write(true).open(path).unwrap();
    if f.metadata().unwrap().len() < offset + bytes.len() as u64 {
        f.set_len(offset + bytes.len() as u64).unwrap();
    }
    f.seek(SeekFrom::Start(offset)).unwrap();
    f.write_all(bytes).unwrap();
}

fn line(v: &serde_json::Value) -> String {
    format!("{v}\n")
}

const WINDOW: u64 = 256 * 1024;

#[test]
fn claude_catalog_reads_metadata_only() {
    let env = Env::new();
    let d = env.dir("p");
    let big_len: u64 = 100 * 1024 * 1024;
    let p = env.put(
        &d,
        "big",
        &line(&serde_json::json!({"type":"user","gitBranch":"head-branch","message":{"content":"head prompt"}})),
        5,
    );
    // Valid records far from both windows: a whole-file reader would take them.
    let middle = line(
        &serde_json::json!({"type":"custom-title","customTitle":"MIDDLE","gitBranch":"middle-branch"}),
    );
    write_at(&p, 50 * 1024 * 1024, format!("\n{middle}").as_bytes());
    let tail = String::from("\n")
        + &line(&serde_json::json!({"type":"custom-title","customTitle":"Tail title"}))
        + &line(&serde_json::json!({"type":"assistant","gitBranch":"tail-branch"}));
    write_at(&p, big_len - tail.len() as u64, tail.as_bytes());
    age(&p, 5);
    let got = list(&env, &[(d.clone(), None)]);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].size_bytes, big_len);
    assert_eq!(got[0].title, "Tail title");
    assert_eq!(got[0].branch.as_deref(), Some("tail-branch"));

    // Without head/tail titles, the middle title is still not seen.
    let q = env.put(
        &d,
        "big2",
        &line(&serde_json::json!({"type":"user","gitBranch":"head-branch","message":{"content":"head prompt"}})),
        6,
    );
    write_at(&q, 50 * 1024 * 1024, format!("\n{middle}").as_bytes());
    write_at(&q, big_len - 1, b"\n");
    age(&q, 6);
    let got = list(&env, &[(d, None)]);
    let s = got.iter().find(|s| s.id == "big2").unwrap();
    assert_eq!(s.title, "head prompt");
    assert_eq!(s.branch.as_deref(), Some("head-branch"));
}

#[test]
fn records_straddling_a_window_edge_are_discarded_without_error() {
    let env = Env::new();
    let d = env.dir("p");
    let first = line(
        &serde_json::json!({"type":"user","gitBranch":"b","message":{"content":"head prompt"}}),
    );
    let straddle = String::from("\n")
        + &line(
            &serde_json::json!({"type":"custom-title","customTitle":"STRADDLE","pad":"x".repeat(40)}),
        );
    let after = String::from("\n")
        + &line(&serde_json::json!({"type":"assistant","gitBranch":"after-edge"}));
    let len = 2 * 1024 * 1024_u64;

    // Cut by the end of the head window.
    let h = env.put(&d, "head-edge", &first, 5);
    write_at(&h, WINDOW - 20, straddle.as_bytes());
    write_at(&h, len - 1, b"\n");
    age(&h, 5);

    // Cut by the start of the tail window.
    let t = env.put(&d, "tail-edge", &first, 6);
    write_at(&t, len - WINDOW - 20, straddle.as_bytes());
    write_at(&t, len - after.len() as u64, after.as_bytes());
    age(&t, 6);

    let got = list(&env, &[(d, None)]);
    assert_eq!(got.len(), 2);
    for s in &got {
        assert_eq!(s.title, "head prompt", "{}", s.id);
    }
    assert_eq!(got[1].branch.as_deref(), Some("after-edge"));
}

#[test]
fn colliding_dirs_attribute_each_session_once() {
    let env = Env::new();
    let (under, dash) = (env.dir("a_b"), env.dir("a-b"));
    let at =
        |cwd: &Path| line(&serde_json::json!({"type":"user","cwd":cwd,"message":{"content":"hi"}}));
    let folder = env.folder(&under);
    assert_eq!(folder, env.folder(&dash), "both dirs share one folder");
    env.put(&under, "s-under", &at(&under), 5);
    env.put(&dash, "s-dash", &at(&dash), 6);
    env.put(&under, "s-nocwd", &user("no cwd here"), 7);
    let (t1, t2) = (TaskId("t1".into()), TaskId("t2".into()));
    let got = list(
        &env,
        &[
            (under.clone(), Some(t1.clone())),
            (dash.clone(), Some(t2.clone())),
        ],
    );
    assert_eq!(got.len(), 3, "{got:?}");
    let by = |id: &str| got.iter().find(|s| s.id == id).unwrap();
    assert_eq!(
        (by("s-under").dir.clone(), by("s-under").worktree.clone()),
        (under.clone(), Some(t1.clone()))
    );
    assert_eq!(
        (by("s-dash").dir.clone(), by("s-dash").worktree.clone()),
        (dash, Some(t2))
    );
    // No cwd: the first requested dir for that folder.
    assert_eq!(
        (by("s-nocwd").dir.clone(), by("s-nocwd").worktree.clone()),
        (under, Some(t1))
    );
}

fn cwd_line(cwd: &Path) -> String {
    line(&serde_json::json!({"type":"user","cwd":cwd,"message":{"content":"hi"}}))
}

#[test]
fn session_of_an_unrequested_colliding_directory_is_not_listed() {
    let env = Env::new();
    let (under, dash) = (env.dir("a_b"), env.dir("a-b"));
    assert_eq!(env.folder(&under), env.folder(&dash));
    env.put(&under, "s-under", &cwd_line(&under), 5);
    env.put(&dash, "s-dash", &cwd_line(&dash), 6);
    let got = list(&env, &[(under.clone(), None)]);
    let ids: Vec<&str> = got.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["s-under"], "{got:?}");
    assert_eq!(got[0].dir, under);
}

#[test]
fn session_without_cwd_still_goes_to_the_first_owner() {
    let env = Env::new();
    let (under, dash) = (env.dir("a_b"), env.dir("a-b"));
    env.put(&under, "s-nocwd", &user("no cwd here"), 5);
    let got = list(&env, &[(under.clone(), None), (dash, None)]);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].dir, under);
}

#[cfg(unix)]
#[test]
fn symlink_spelled_cwd_still_matches_its_directory() {
    let env = Env::new();
    let real = env.dir("real");
    let link = env.tmp.path().join("work").join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    env.put(&real, "s-link", &cwd_line(&link), 5);
    let got = list(&env, &[(real.clone(), None)]);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].dir, real);
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
fn zero_length_jsonl_is_skipped_not_an_error() {
    let env = Env::new();
    let d = env.dir("p");
    env.put(&d, "empty", "", 5);
    assert_eq!(
        read_claude_sessions(&env.home(), &[(d, None)]),
        Ok(Vec::new())
    );
}

#[test]
fn empty_file_next_to_a_good_one_lists_the_good_one() {
    let env = Env::new();
    let d = env.dir("p");
    env.put(&d, "empty", "", 5);
    env.put(
        &d,
        TITLED,
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
        6,
    );
    let found = list(&env, &[(d, None)]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].id, TITLED);
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

const AI_TITLED: &str = "00000000-0000-4000-8000-000000000003";

fn ai_title(text: &str) -> String {
    line(&serde_json::json!({"type":"ai-title","aiTitle":text}))
}

#[test]
fn ai_title_is_used_when_no_user_title() {
    let env = Env::new();
    let d = env.dir("p");
    env.put_fixture(&d, AI_TITLED, 5);
    let got = list(&env, &[(d, None)]);
    assert_eq!(got[0].title, "Review the cache layer");
}

#[test]
fn user_set_title_beats_ai_title() {
    let env = Env::new();
    let d = env.dir("p");
    let custom = user("a prompt")
        + &ai_title("Auto")
        + &line(&serde_json::json!({"type":"custom-title","customTitle":"Mine"}))
        + &ai_title("Auto later");
    env.put(&d, "custom", &custom, 5);
    let agent = user("a prompt")
        + &line(&serde_json::json!({"type":"agent-name","agentName":"Named"}))
        + &ai_title("Auto later");
    env.put(&d, "agent", &agent, 6);
    let got = list(&env, &[(d, None)]);
    assert_eq!(got.iter().find(|s| s.id == "custom").unwrap().title, "Mine");
    assert_eq!(got.iter().find(|s| s.id == "agent").unwrap().title, "Named");
}

#[test]
fn last_ai_title_wins() {
    let env = Env::new();
    let d = env.dir("p");
    env.put(
        &d,
        "small",
        &(user("p") + &ai_title("First") + &ai_title("Second")),
        5,
    );
    // Large sparse file: the earlier title in the head window, the later one in the tail.
    let big_len: u64 = 100 * 1024 * 1024;
    let p = env.put(&d, "big", &(user("p") + &ai_title("Head title")), 6);
    let tail = String::from("\n") + &ai_title("Tail title");
    write_at(&p, big_len - tail.len() as u64, tail.as_bytes());
    age(&p, 6);
    let got = list(&env, &[(d, None)]);
    assert_eq!(
        got.iter().find(|s| s.id == "small").unwrap().title,
        "Second"
    );
    assert_eq!(
        got.iter().find(|s| s.id == "big").unwrap().title,
        "Tail title"
    );
}

#[test]
fn ai_title_is_sanitised() {
    let env = Env::new();
    let d = env.dir("p");
    env.put(
        &d,
        "ansi",
        &ai_title("\u{1b}[1mBold\u{1b}[0m\u{8} name\n"),
        5,
    );
    env.put(&d, "long", &ai_title(&"y".repeat(300)), 6);
    let got = list(&env, &[(d, None)]);
    assert_eq!(
        got.iter().find(|s| s.id == "ansi").unwrap().title,
        "Bold name"
    );
    let long = &got.iter().find(|s| s.id == "long").unwrap().title;
    assert_eq!(long.chars().count(), 201);
    assert!(long.ends_with('…'));
}

#[test]
fn empty_or_non_string_ai_title_falls_back_to_the_first_prompt() {
    let env = Env::new();
    let d = env.dir("p");
    let bad = [
        ai_title(""),
        ai_title("\u{7}\u{1b}[0m"),
        line(&serde_json::json!({"type":"ai-title","aiTitle":42})),
        line(&serde_json::json!({"type":"ai-title","aiTitle":null})),
        line(&serde_json::json!({"type":"ai-title"})),
    ];
    for (i, rec) in bad.iter().enumerate() {
        env.put(
            &d,
            &format!("bad{i}"),
            &(user("the prompt") + rec),
            5 + i as u64,
        );
    }
    let got = list(&env, &[(d, None)]);
    assert_eq!(got.len(), 5);
    assert!(got.iter().all(|s| s.title == "the prompt"));
}

fn last_prompt(text: &str) -> String {
    line(
        &serde_json::json!({"type":"last-prompt","lastPrompt":text,"leafUuid":"u","sessionId":"s"}),
    )
}

fn title_of(contents: &str) -> String {
    let env = Env::new();
    let d = env.dir("p");
    env.put(&d, "one", contents, 5);
    list(&env, &[(d, None)]).remove(0).title
}

#[test]
fn last_prompt_record_is_the_title_when_there_is_no_ai_title() {
    let got = title_of(&(user("first") + &last_prompt("the last one")));
    assert_eq!(got, "the last one");
}

#[test]
fn ai_title_beats_last_prompt() {
    let got = title_of(&(user("first") + &ai_title("Auto") + &last_prompt("last")));
    assert_eq!(got, "Auto");
}

#[test]
fn user_set_title_beats_last_prompt() {
    let named = line(&serde_json::json!({"type":"custom-title","customTitle":"Mine"}));
    let got = title_of(&(user("first") + &named + &last_prompt("last")));
    assert_eq!(got, "Mine");
}

#[test]
fn last_prompt_wins_over_the_first_prompt() {
    let got = title_of(&(user("first") + &last_prompt("last")));
    assert_eq!(got, "last");
    // With no last-prompt record, the last real user prompt is the title.
    let got = title_of(&(user("first") + &user("second") + &user("third")));
    assert_eq!(got, "third");
}

#[test]
fn injected_last_prompt_is_skipped() {
    for injected in [
        "Base directory for this skill: /x",
        "<local-command-caveat>Caveat: x</local-command-caveat>",
        "Caveat: the messages below were generated by the user",
        "<system-reminder>x</system-reminder>",
        "<local-command-stdout>x</local-command-stdout>",
    ] {
        let got = title_of(&(user("first") + &user("real prompt") + &last_prompt(injected)));
        assert_eq!(got, "real prompt", "{injected}");
    }
    let got = title_of(
        &(user("first") + &user("real prompt") + &user("Base directory for this skill: /x")),
    );
    assert_eq!(got, "real prompt");
}

#[test]
fn meta_user_records_are_never_titles() {
    let meta =
        line(&serde_json::json!({"type":"user","isMeta":true,"message":{"content":"meta text"}}));
    assert_eq!(title_of(&(user("real") + &meta)), "real");
    assert_eq!(title_of(&(meta.clone() + &last_prompt("   "))), "");
}

#[test]
fn command_only_session_shows_the_command_name() {
    let caveat = line(
        &serde_json::json!({"type":"user","isMeta":true,"message":{"content":
        "<local-command-caveat>Caveat: ignore</local-command-caveat>"}}),
    );
    let cmd = |name: &str, msg: &str| {
        line(&serde_json::json!({"type":"user","message":{"content":
            format!("<command-name>/{name}</command-name>\n<command-message>{msg}</command-message>\n<command-args></command-args>")}}))
    };
    let stdout = line(&serde_json::json!({"type":"user","message":{"content":
        "<local-command-stdout></local-command-stdout>"}}));
    assert_eq!(
        title_of(&(caveat.clone() + &cmd("clear", "clear") + &stdout)),
        "/clear"
    );
    assert_eq!(
        title_of(&(caveat.clone() + &cmd("effort", "effort") + &stdout)),
        "/effort"
    );
    let doctor = line(&serde_json::json!({"type":"user","message":{"content":
        "<command-message>doctor</command-message>\n<command-name>/doctor</command-name>"}}));
    assert_eq!(title_of(&(caveat + &doctor + &last_prompt(""))), "/doctor");
    // A real prompt beats a command.
    assert_eq!(title_of(&(cmd("clear", "clear") + &user("hello"))), "hello");
}

#[test]
fn last_prompt_in_the_tail_window_of_a_large_file_wins() {
    let env = Env::new();
    let d = env.dir("p");
    let big_len: u64 = 100 * 1024 * 1024;
    let p = env.put(
        &d,
        "big",
        &(user("head prompt") + &last_prompt("head last")),
        5,
    );
    let middle = String::from("\n") + &last_prompt("MIDDLE");
    write_at(&p, 50 * 1024 * 1024, middle.as_bytes());
    let tail = String::from("\n") + &last_prompt("tail last");
    write_at(&p, big_len - tail.len() as u64, tail.as_bytes());
    age(&p, 5);
    let got = list(&env, &[(d, None)]);
    assert_eq!(got[0].title, "tail last");
}

#[test]
fn last_prompt_is_sanitised() {
    assert_eq!(
        title_of(&last_prompt("\u{1b}[1mBold\u{1b}[0m\u{8} name\n")),
        "Bold name"
    );
    let long = title_of(&last_prompt(&"z".repeat(300)));
    assert_eq!(long.chars().count(), 201);
    assert!(long.ends_with('…'));
}
