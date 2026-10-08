//! Helpers shared by the integration tests: throwaway git repos with a bare
//! `origin`, run with an isolated git environment, so the user's global git
//! config is never read or written.
#![allow(dead_code, reason = "each test crate uses its own subset")]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The environment every git child gets: `HOME` and `GIT_CONFIG_GLOBAL` in
/// `home` (created, with an empty config), and no system config.
pub fn isolated_git_env(home: &Path) -> Vec<(OsString, OsString)> {
    fs::create_dir_all(home).unwrap();
    let global = home.join("gitconfig");
    fs::write(&global, "").unwrap();
    vec![
        (OsString::from("HOME"), home.as_os_str().to_owned()),
        (OsString::from("GIT_CONFIG_GLOBAL"), global.into_os_string()),
        (OsString::from("GIT_CONFIG_NOSYSTEM"), OsString::from("1")),
    ]
}

fn git_command(env: &[(OsString, OsString)], dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(args)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .env("GIT_TERMINAL_PROMPT", "0");
    cmd
}

/// `git -C dir args`, which must succeed; its trimmed stdout.
pub fn git_in(env: &[(OsString, OsString)], dir: &Path, args: &[&str]) -> String {
    let out = git_command(env, dir, args).output().unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

/// Whether `git -C dir args` succeeds.
pub fn git_ok(env: &[(OsString, OsString)], dir: &Path, args: &[&str]) -> bool {
    git_command(env, dir, args)
        .output()
        .unwrap()
        .status
        .success()
}

/// Writes `name` under `dir`, commits everything, and returns the new commit.
pub fn commit_file(
    env: &[(OsString, OsString)],
    dir: &Path,
    name: &str,
    body: &str,
    msg: &str,
) -> String {
    let file = dir.join(name);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&file, body).unwrap();
    git_in(env, dir, &["add", "-A"]);
    git_in(env, dir, &["commit", "--quiet", "-m", msg]);
    git_in(env, dir, &["rev-parse", "HEAD"])
}

/// `<parent>/origin.git` (bare, its HEAD naming `origin_head`) and `<parent>/repo`
/// on `main`, with one commit pushed and `origin/HEAD` set; returns `(repo, origin)`.
pub fn repo_with_origin(
    env: &[(OsString, OsString)],
    parent: &Path,
    origin_head: &str,
) -> (PathBuf, PathBuf) {
    let origin = parent.join("origin.git");
    let repo = parent.join("repo");
    git_in(
        env,
        parent,
        &["init", "--quiet", "--bare", "-b", origin_head, "origin.git"],
    );
    git_in(env, parent, &["init", "--quiet", "-b", "main", "repo"]);
    git_in(env, &repo, &["config", "user.name", "Test"]);
    git_in(env, &repo, &["config", "user.email", "test@example.com"]);
    commit_file(env, &repo, "README", "hello\n", "initial");
    git_in(
        env,
        &repo,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    git_in(env, &repo, &["push", "--quiet", "origin", "main"]);
    if origin_head != "main" {
        git_in(
            env,
            &repo,
            &["push", "--quiet", "origin", &format!("main:{origin_head}")],
        );
    }
    git_in(env, &repo, &["fetch", "--quiet", "origin"]);
    git_in(env, &repo, &["remote", "set-head", "origin", "main"]);
    (repo, origin)
}
