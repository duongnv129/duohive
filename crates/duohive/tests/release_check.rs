//! `scripts/check-release-version.sh` against throwaway git repositories.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly on setup errors"
)]

use std::path::Path;
use std::process::{Command, Output};

const SCRIPT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/check-release-version.sh"
);

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A gate file in the shape `duohive bench --gate --json` writes (sorted keys).
fn gate_json(all_pass: bool, platform: &str, bytes: u64, sha: &str) -> String {
    gate_json_dirty(all_pass, platform, bytes, sha, false)
}

fn gate_json_dirty(all_pass: bool, platform: &str, bytes: u64, sha: &str, dirty: bool) -> String {
    format!(
        "{{\"all_pass\":{all_pass},\"binary_bytes\":{bytes},\"gating\":true,\
         \"git_dirty\":{dirty},\"git_sha\":\"{sha}\",\"platform\":\"{platform}\",\"profile\":\"release\",\
         \"tiers\":[{{\"agents\":10,\"all_pass\":{all_pass}}}],\"version\":\"1.0.0\"}}"
    )
}

/// A repo with version 1.0.0, one commit of code, and (unless `gate` is None)
/// a gate file for the first commit's sha, committed on top.
struct Repo {
    dir: tempfile::TempDir,
    gated_sha: String,
}

fn repo(gate: Option<(bool, &str, u64)>) -> Repo {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    git(p, &["init", "-q"]);
    std::fs::write(
        p.join("Cargo.toml"),
        "[workspace.package]\nversion = \"1.0.0\"\n\n[workspace.dependencies]\n",
    )
    .unwrap();
    std::fs::create_dir_all(p.join("crates/x")).unwrap();
    std::fs::write(p.join("crates/x/lib.rs"), "// v1\n").unwrap();
    git(p, &["add", "-A"]);
    git(p, &["commit", "-q", "-m", "code"]);
    let gated_sha = git(p, &["rev-parse", "HEAD"]);
    if let Some((pass, platform, bytes)) = gate {
        std::fs::create_dir_all(p.join("docs/release-gates")).unwrap();
        std::fs::write(
            p.join("docs/release-gates/1.0.0.json"),
            gate_json(pass, platform, bytes, &gated_sha),
        )
        .unwrap();
        git(p, &["add", "-A"]);
        git(p, &["commit", "-q", "-m", "docs: gate result"]);
    }
    Repo { dir, gated_sha }
}

fn check(repo: &Repo, tag: &str) -> Output {
    Command::new("sh")
        .arg(SCRIPT)
        .arg(tag)
        .env("RELEASE_CHECK_ROOT", repo.dir.path())
        .output()
        .unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn release_check_passes_a_good_gate_file() {
    let r = repo(Some((true, "macos-aarch64", 6_715_312)));
    let out = check(&r, "v1.0.0");
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}

#[test]
fn release_check_refuses_a_tag_that_differs_from_the_version() {
    let r = repo(Some((true, "macos-aarch64", 6_715_312)));
    let out = check(&r, "v1.0.1");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("does not match"), "{}", stderr(&out));
}

#[test]
fn release_check_refuses_a_missing_gate_file() {
    let r = repo(None);
    let out = check(&r, "v1.0.0");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("no gate result"), "{}", stderr(&out));
}

#[test]
fn release_check_refuses_a_failed_gate() {
    let r = repo(Some((false, "macos-aarch64", 6_715_312)));
    let out = check(&r, "v1.0.0");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("did not pass"), "{}", stderr(&out));
}

#[test]
fn release_check_refuses_a_non_arm64_gate() {
    let r = repo(Some((true, "linux-x86_64", 6_715_312)));
    let out = check(&r, "v1.0.0");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("macos-aarch64"), "{}", stderr(&out));
}

#[test]
fn release_check_refuses_an_oversize_binary() {
    let r = repo(Some((true, "macos-aarch64", 10_000_000)));
    let out = check(&r, "v1.0.0");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("not under"), "{}", stderr(&out));
}

#[test]
fn release_check_refuses_a_gate_sha_that_differs_from_the_crates_tree() {
    let r = repo(Some((true, "macos-aarch64", 6_715_312)));
    let p = r.dir.path();
    std::fs::write(p.join("crates/x/lib.rs"), "// v2\n").unwrap();
    git(p, &["commit", "-q", "-am", "feat: more code"]);
    let out = check(&r, "v1.0.0");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("code changed since"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn release_check_allows_later_commits_that_touch_only_docs() {
    let r = repo(Some((true, "macos-aarch64", 6_715_312)));
    let p = r.dir.path();
    std::fs::write(p.join("docs/note.md"), "notes\n").unwrap();
    git(p, &["add", "-A"]);
    git(p, &["commit", "-q", "-m", "docs: note"]);
    let out = check(&r, "v1.0.0");
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}

#[test]
fn release_check_refuses_a_gate_run_on_a_dirty_checkout() {
    let r = repo(Some((true, "macos-aarch64", 6_715_312)));
    let p = r.dir.path();
    std::fs::write(
        p.join("docs/release-gates/1.0.0.json"),
        gate_json_dirty(true, "macos-aarch64", 6_715_312, &r.gated_sha, true),
    )
    .unwrap();
    git(p, &["commit", "-q", "-am", "docs: dirty gate"]);
    let out = check(&r, "v1.0.0");
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("uncommitted"), "{}", stderr(&out));
}
