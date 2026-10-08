//! `GitCli` against real git: throwaway repos in `tempfile`, each with a bare
//! `origin`. Every git child, the fixture's and `GitCli`'s, gets `HOME` and
//! `GIT_CONFIG_GLOBAL` in the temp dir, so the user's global config is never
//! read or written.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly on setup errors"
)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use duohive::git::GitCli;
use duohive_core::engine::Input;
use duohive_core::ports::{CheckWorktree, CreateWorktree, Git, GitError, GitEvent, RemoveWorktree};
use duohive_core::testkit::n;
use duohive_core::worktree::{BaseRef, MergeKind, MergeState, TaskId};
use tempfile::TempDir;
use tokio::sync::mpsc;

const ANSWER_WAIT: Duration = Duration::from_secs(60);

/// A repo cloned-in-spirit from a bare `origin`, with `main` pushed and
/// `origin/HEAD` set.
struct Fixture {
    tmp: TempDir,
    repo: PathBuf,
    origin: PathBuf,
    worktrees: PathBuf,
    env: Vec<(OsString, OsString)>,
}

impl Fixture {
    fn new() -> Self {
        Self::with_origin_head("main")
    }

    /// `origin`'s own HEAD names `origin_head`; the repo has `main`.
    fn with_origin_head(origin_head: &str) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let global = home.join("gitconfig");
        fs::write(&global, "").unwrap();
        let env = vec![
            (OsString::from("HOME"), home.into_os_string()),
            (OsString::from("GIT_CONFIG_GLOBAL"), global.into_os_string()),
            (OsString::from("GIT_CONFIG_NOSYSTEM"), OsString::from("1")),
        ];
        let origin = tmp.path().join("origin.git");
        let repo = tmp.path().join("repo");
        let worktrees = tmp.path().join("data").join("worktrees");
        fs::create_dir_all(&worktrees).unwrap();
        let f = Self {
            tmp,
            repo,
            origin,
            worktrees,
            env,
        };
        f.git_in(
            f.tmp.path(),
            &["init", "--quiet", "--bare", "-b", origin_head, "origin.git"],
        );
        f.git_in(f.tmp.path(), &["init", "--quiet", "-b", "main", "repo"]);
        f.git(&["config", "user.name", "Test"]);
        f.git(&["config", "user.email", "test@example.com"]);
        f.commit_file(&f.repo, "README", "hello\n", "initial");
        f.git(&["remote", "add", "origin", f.origin.to_str().unwrap()]);
        f.git(&["push", "--quiet", "origin", "main"]);
        if origin_head != "main" {
            f.git(&["push", "--quiet", "origin", &format!("main:{origin_head}")]);
        }
        f.git(&["fetch", "--quiet", "origin"]);
        f.git(&["remote", "set-head", "origin", "main"]);
        f
    }

    fn git_in(&self, dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    fn git(&self, args: &[&str]) -> String {
        self.git_in(&self.repo, args)
    }

    fn git_ok(&self, args: &[&str]) -> bool {
        Command::new("git")
            .arg("-C")
            .arg(&self.repo)
            .args(args)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .output()
            .unwrap()
            .status
            .success()
    }

    fn commit_file(&self, dir: &Path, name: &str, body: &str, msg: &str) -> String {
        let file = dir.join(name);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&file, body).unwrap();
        self.git_in(dir, &["add", "-A"]);
        self.git_in(dir, &["commit", "--quiet", "-m", msg]);
        self.git_in(dir, &["rev-parse", "HEAD"])
    }

    fn cli(&self) -> (GitCli, mpsc::Receiver<Input>) {
        let (tx, rx) = mpsc::channel(64);
        (
            GitCli::new(self.worktrees.clone(), tx, self.env.clone()),
            rx,
        )
    }

    fn path(&self, task: &str) -> PathBuf {
        self.worktrees.join("a").join(task)
    }

    fn create_req(&self, task: &str) -> CreateWorktree {
        CreateWorktree {
            agent: n("a"),
            task: TaskId(task.to_owned()),
            repo: self.repo.clone(),
            path: self.path(task),
            branch: branch(task),
        }
    }

    fn check_req(&self, task: &str, created_from: &str, generation: u64) -> CheckWorktree {
        CheckWorktree {
            agent: n("a"),
            task: TaskId(task.to_owned()),
            repo: self.repo.clone(),
            path: self.path(task),
            branch: branch(task),
            created_from: created_from.to_owned(),
            generation,
        }
    }

    fn remove_req(&self, task: &str, delete_branch: bool, force: bool) -> RemoveWorktree {
        RemoveWorktree {
            agent: n("a"),
            task: TaskId(task.to_owned()),
            repo: self.repo.clone(),
            path: self.path(task),
            branch: branch(task),
            delete_branch,
            force,
        }
    }

    fn branch_exists(&self, task: &str) -> bool {
        self.git_ok(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{}", branch(task)),
        ])
    }

    /// Advances `main` (and `origin/main`) with an unrelated commit, so later
    /// rebases and squashes produce new commit ids.
    fn advance_main(&self, name: &str) {
        self.commit_file(&self.repo, name, "unrelated\n", "unrelated");
        self.push_main();
    }

    fn push_main(&self) {
        self.git(&["push", "--quiet", "origin", "main"]);
    }
}

fn branch(task: &str) -> String {
    format!("duohive/a/{task}")
}

async fn answer(rx: &mut mpsc::Receiver<Input>) -> GitEvent {
    match tokio::time::timeout(ANSWER_WAIT, rx.recv()).await {
        Ok(Some(Input::Git(event))) => event,
        other => panic!("expected a git answer, got {other:?}"),
    }
}

/// Creates `task`'s worktree and returns its `created_from`.
async fn create(
    f: &Fixture,
    git: &mut GitCli,
    rx: &mut mpsc::Receiver<Input>,
    task: &str,
) -> String {
    git.create(f.create_req(task));
    match answer(rx).await {
        GitEvent::Created {
            result: Ok((_, from)),
            ..
        } => from,
        other => panic!("create failed: {other:?}"),
    }
}

/// Checks `task`'s worktree: its merge state, dirty files and base.
async fn check(
    f: &Fixture,
    git: &mut GitCli,
    rx: &mut mpsc::Receiver<Input>,
    task: &str,
    from: &str,
) -> (MergeState, u32, Option<BaseRef>) {
    git.check(f.check_req(task, from, 1));
    match answer(rx).await {
        GitEvent::Checked {
            merge,
            dirty_files,
            base,
            ..
        } => (merge, dirty_files, base),
        other => panic!("expected Checked, got {other:?}"),
    }
}

async fn remove(
    git: &mut GitCli,
    rx: &mut mpsc::Receiver<Input>,
    req: RemoveWorktree,
) -> Result<(), String> {
    git.remove(req);
    match answer(rx).await {
        GitEvent::Removed { result, .. } => result,
        other => panic!("expected Removed, got {other:?}"),
    }
}

// ---- Merge detection ----

#[tokio::test]
async fn detects_normal_merge() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();

    // A merge commit.
    let from = create(&f, &mut git, &mut rx, "merged").await;
    f.commit_file(&f.path("merged"), "a.txt", "a\n", "work");
    f.advance_main("other.txt");
    f.git(&[
        "merge",
        "--quiet",
        "--no-ff",
        "-m",
        "merge",
        &branch("merged"),
    ]);
    f.push_main();
    let (merge, dirty, base) = check(&f, &mut git, &mut rx, "merged", &from).await;
    assert_eq!(merge, MergeState::Merged(MergeKind::Normal));
    assert_eq!(dirty, 0);
    assert_eq!(base, Some(BaseRef("origin/main".into())));

    // A fast-forward.
    let from = create(&f, &mut git, &mut rx, "ff").await;
    f.commit_file(&f.path("ff"), "b.txt", "b\n", "work");
    f.git(&["merge", "--quiet", "--ff-only", &branch("ff")]);
    f.push_main();
    let (merge, _, _) = check(&f, &mut git, &mut rx, "ff", &from).await;
    assert_eq!(merge, MergeState::Merged(MergeKind::Normal));
}

#[tokio::test]
async fn detects_rebase_merge() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    let from = create(&f, &mut git, &mut rx, "t").await;
    let c1 = f.commit_file(&f.path("t"), "a.txt", "a\n", "one");
    let c2 = f.commit_file(&f.path("t"), "b.txt", "b\n", "two");
    f.advance_main("other.txt");
    f.git(&["cherry-pick", &c1, &c2]);
    f.push_main();
    let (merge, _, _) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(merge, MergeState::Merged(MergeKind::Rebase));
}

#[tokio::test]
async fn detects_squash_merge() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    let from = create(&f, &mut git, &mut rx, "t").await;
    f.commit_file(&f.path("t"), "a.txt", "a\n", "one");
    f.commit_file(&f.path("t"), "b.txt", "b\n", "two");
    f.advance_main("other.txt");
    f.git(&["merge", "--quiet", "--squash", &branch("t")]);
    f.git(&["commit", "--quiet", "-m", "squashed"]);
    f.push_main();
    let (merge, _, _) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(merge, MergeState::Merged(MergeKind::Squash));
}

#[tokio::test]
async fn squash_check_works_without_git_identity() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    let from = create(&f, &mut git, &mut rx, "t").await;
    f.commit_file(&f.path("t"), "a.txt", "a\n", "one");
    f.commit_file(&f.path("t"), "b.txt", "b\n", "two");
    f.advance_main("other.txt");
    f.git(&["merge", "--quiet", "--squash", &branch("t")]);
    f.git(&["commit", "--quiet", "-m", "squashed"]);
    f.push_main();
    // No identity anywhere: HOME and GIT_CONFIG_GLOBAL are an empty temp dir,
    // and git may not guess one from the host.
    f.git(&["config", "--unset", "user.name"]);
    f.git(&["config", "--unset", "user.email"]);
    f.git(&["config", "user.useConfigOnly", "true"]);
    let (merge, _, _) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(merge, MergeState::Merged(MergeKind::Squash));
}

#[tokio::test]
async fn fresh_branch_is_not_merged() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    let from = create(&f, &mut git, &mut rx, "t").await;
    // `--is-ancestor` holds: the branch tip is `created_from`, on main.
    assert!(f.git_ok(&["merge-base", "--is-ancestor", &branch("t"), "origin/main"]));
    let (merge, dirty, _) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(merge, MergeState::NotMerged { files_changed: 0 });
    assert_eq!(dirty, 0);
}

#[tokio::test]
async fn unmerged_counts_files() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    let from = create(&f, &mut git, &mut rx, "t").await;
    f.commit_file(&f.path("t"), "a.txt", "a\n", "one");
    f.commit_file(&f.path("t"), "sub/b.txt", "b\n", "two");
    f.commit_file(&f.path("t"), "README", "changed\n", "three");
    f.advance_main("other.txt");
    let (merge, dirty, _) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(merge, MergeState::NotMerged { files_changed: 3 });
    assert_eq!(dirty, 0);
}

#[tokio::test]
async fn dirty_worktree_counted() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    let from = create(&f, &mut git, &mut rx, "t").await;
    fs::write(f.path("t").join("README"), "edited\n").unwrap();
    fs::write(f.path("t").join("new.txt"), "untracked\n").unwrap();
    let (_, dirty, _) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(dirty, 2);
}

#[tokio::test]
async fn check_echoes_generation() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    let from = create(&f, &mut git, &mut rx, "t").await;
    for generation in [7, 42] {
        git.check(f.check_req("t", &from, generation));
        match answer(&mut rx).await {
            GitEvent::Checked {
                generation: got, ..
            } => assert_eq!(got, generation),
            other => panic!("expected Checked, got {other:?}"),
        }
    }
}

// ---- Base and offline ----

#[tokio::test]
async fn base_prefers_origin_head_then_ls_remote_then_default_main_master() {
    // origin's own HEAD is `trunk`; the local origin/HEAD says `main`.
    let f = Fixture::with_origin_head("trunk");
    let (mut git, mut rx) = f.cli();
    let from = create(&f, &mut git, &mut rx, "t").await;

    // 1. refs/remotes/origin/HEAD wins.
    let (_, _, base) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(base, Some(BaseRef("origin/main".into())));

    // 2. Unset: ask origin with ls-remote.
    f.git(&["remote", "set-head", "origin", "--delete"]);
    let (_, _, base) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(base, Some(BaseRef("origin/trunk".into())));

    // 3. No remote: init.defaultBranch, then main, then master.
    f.git(&["remote", "remove", "origin"]);
    f.git(&["branch", "trunk", "main"]);
    f.git(&["config", "init.defaultBranch", "trunk"]);
    let (_, _, base) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(base, Some(BaseRef("trunk".into())));

    f.git(&["config", "--unset", "init.defaultBranch"]);
    let (_, _, base) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(base, Some(BaseRef("main".into())));

    f.git(&["branch", "-m", "main", "master"]);
    let (_, _, base) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(base, Some(BaseRef("master".into())));

    // 4. None of them: unknown, never merged.
    f.git(&["checkout", "--quiet", "--detach"]);
    f.git(&["branch", "-D", "master", "trunk"]);
    let (merge, _, base) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(merge, MergeState::Unknown("no default branch".into()));
    assert_eq!(base, None);
}

#[tokio::test]
async fn offline_fetch_uses_local_refs() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    let from = create(&f, &mut git, &mut rx, "t").await;
    f.commit_file(&f.path("t"), "a.txt", "a\n", "work");
    f.git(&["merge", "--quiet", "--ff-only", &branch("t")]);
    f.push_main();
    // origin is gone: the fetch fails, and the local refs answer.
    fs::remove_dir_all(&f.origin).unwrap();
    let (merge, _, base) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(merge, MergeState::Merged(MergeKind::Normal));
    assert_eq!(base, Some(BaseRef("origin/main".into())));

    // Neither origin/HEAD nor ls-remote: the local default.
    f.git(&["remote", "set-head", "origin", "--delete"]);
    let (merge, _, base) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(merge, MergeState::Merged(MergeKind::Normal));
    assert_eq!(base, Some(BaseRef("main".into())));
}

// ---- Branches and removal ----

#[tokio::test]
async fn create_reports_base_and_created_from() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    git.create(f.create_req("t"));
    let GitEvent::Created {
        agent,
        task,
        result,
    } = answer(&mut rx).await
    else {
        panic!("expected Created");
    };
    assert_eq!((agent, task), (n("a"), TaskId("t".into())));
    let (base, from) = result.unwrap();
    assert_eq!(base, BaseRef("origin/main".into()));
    assert_eq!(from, f.git(&["rev-parse", "origin/main"]));
    assert_eq!(f.git(&["rev-parse", &branch("t")]), from);
    assert!(f.path("t").join("README").exists());
}

#[tokio::test]
async fn existing_branch_is_an_error_not_reused() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    f.git(&["branch", &branch("t")]);
    let tip = f.git(&["rev-parse", &branch("t")]);
    f.advance_main("other.txt");
    git.create(f.create_req("t"));
    match answer(&mut rx).await {
        GitEvent::Created { result, .. } => assert_eq!(result, Err(GitError::BranchExists)),
        other => panic!("expected Created, got {other:?}"),
    }
    assert_eq!(f.git(&["rev-parse", &branch("t")]), tip);
    assert!(!f.path("t").exists());
}

#[tokio::test]
async fn create_outside_a_repo_is_not_a_repo() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    let plain = f.tmp.path().join("plain");
    fs::create_dir_all(&plain).unwrap();
    let mut req = f.create_req("t");
    req.repo = plain;
    git.create(req);
    match answer(&mut rx).await {
        GitEvent::Created { result, .. } => assert_eq!(result, Err(GitError::NotARepo)),
        other => panic!("expected Created, got {other:?}"),
    }
}

#[tokio::test]
async fn remove_refuses_paths_outside_worktrees_dir() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    let elsewhere = f.tmp.path().join("elsewhere");
    let dotdot = f.worktrees.join("..").join("x");
    // Both are real worktrees of the repo: only the path check keeps them.
    f.git(&[
        "worktree",
        "add",
        "--quiet",
        "-b",
        "e1",
        elsewhere.to_str().unwrap(),
    ]);
    f.git(&[
        "worktree",
        "add",
        "--quiet",
        "-b",
        "e2",
        dotdot.to_str().unwrap(),
    ]);
    for (path, b) in [(&elsewhere, "e1"), (&dotdot, "e2")] {
        let mut req = f.remove_req("t", true, true);
        req.path = path.clone();
        req.branch = b.to_owned();
        let result = remove(&mut git, &mut rx, req).await;
        assert!(result.is_err(), "{path:?} was removed");
        assert!(path.join("README").exists(), "{path:?} is gone");
        assert!(f.git_ok(&["rev-parse", "--verify", "--quiet", b]));
    }
}

#[tokio::test]
async fn remove_without_force_keeps_dirty_worktree() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    create(&f, &mut git, &mut rx, "t").await;
    let file = f.path("t").join("notes.txt");
    fs::write(&file, "keep me\n").unwrap();
    let result = remove(&mut git, &mut rx, f.remove_req("t", false, false)).await;
    assert!(result.is_err());
    assert!(file.exists());
    assert!(f.branch_exists("t"));
}

#[tokio::test]
async fn create_then_remove_round_trip() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    create(&f, &mut git, &mut rx, "t").await;
    f.commit_file(&f.path("t"), "a.txt", "a\n", "work");
    f.git(&["merge", "--quiet", "--ff-only", &branch("t")]);
    f.push_main();
    let result = remove(&mut git, &mut rx, f.remove_req("t", true, false)).await;
    assert_eq!(result, Ok(()));
    assert!(!f.path("t").exists());
    assert!(!f.branch_exists("t"));

    // Without delete_branch the branch stays.
    create(&f, &mut git, &mut rx, "u").await;
    let result = remove(&mut git, &mut rx, f.remove_req("u", false, false)).await;
    assert_eq!(result, Ok(()));
    assert!(!f.path("u").exists());
    assert!(f.branch_exists("u"));
}

#[tokio::test]
async fn delete_branch_keeps_a_branch_that_is_no_longer_merged() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    let from = create(&f, &mut git, &mut rx, "t").await;
    f.commit_file(&f.path("t"), "a.txt", "a\n", "work");
    f.git(&["merge", "--quiet", "--ff-only", &branch("t")]);
    f.push_main();
    let (merge, _, _) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(merge, MergeState::Merged(MergeKind::Normal));
    // The stored `Merged` goes stale: the branch gains a commit.
    let tip = f.commit_file(&f.path("t"), "b.txt", "b\n", "more work");
    let result = remove(&mut git, &mut rx, f.remove_req("t", true, false)).await;
    assert_eq!(result, Err("branch is no longer merged".into()));
    assert_eq!(f.git(&["rev-parse", &branch("t")]), tip);
    assert!(f.path("t").join("b.txt").exists());
}

/// Sends create, check, remove, create for task `t` back to back, the middle
/// two through `other_repo`, and returns what each answer said. Each depends
/// on the one before having finished.
async fn run_back_to_back(f: &Fixture, other_repo: &Path) -> Vec<String> {
    let (mut git, mut rx) = f.cli();
    let from = f.git(&["rev-parse", "origin/main"]);
    git.create(f.create_req("t"));
    let mut check = f.check_req("t", &from, 1);
    check.repo = other_repo.to_path_buf();
    git.check(check);
    let mut remove = f.remove_req("t", false, false);
    remove.repo = other_repo.to_path_buf();
    git.remove(remove);
    git.create(f.create_req("t"));
    let mut kinds = Vec::new();
    for _ in 0..4 {
        kinds.push(match answer(&mut rx).await {
            GitEvent::Created { result, .. } => format!("created {}", result.is_ok()),
            GitEvent::Checked { merge, .. } => format!("checked {merge:?}"),
            GitEvent::Removed { result, .. } => format!("removed {}", result.is_ok()),
        });
    }
    kinds
}

const IN_ORDER: [&str; 4] = [
    "created true",
    "checked NotMerged { files_changed: 0 }",
    "removed true",
    // The branch was kept, so the name is taken.
    "created false",
];

#[tokio::test]
async fn same_repo_requests_run_in_order() {
    let f = Fixture::new();
    assert_eq!(run_back_to_back(&f, &f.repo).await, IN_ORDER);
}

#[tokio::test]
async fn repo_root_and_subdirectory_share_one_worker() {
    let f = Fixture::new();
    f.commit_file(&f.repo, "sub/deep/file.txt", "x\n", "sub");
    f.push_main();
    let sub = f.repo.join("sub").join("deep");
    assert_eq!(run_back_to_back(&f, &sub).await, IN_ORDER);
}

#[tokio::test]
async fn rebase_check_does_not_trust_merge_commits() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    let from = create(&f, &mut git, &mut rx, "t").await;
    let wt = f.path("t");
    let c1 = f.commit_file(&wt, "a.txt", "a\n", "one");
    // A side branch merged into the task branch; the merge commit itself
    // carries extra content no ordinary commit has.
    f.git_in(&wt, &["checkout", "--quiet", "-b", "side"]);
    let c2 = f.commit_file(&wt, "b.txt", "b\n", "two");
    f.git_in(&wt, &["checkout", "--quiet", &branch("t")]);
    f.git_in(&wt, &["merge", "--quiet", "--no-ff", "--no-commit", "side"]);
    fs::write(wt.join("only-in-merge.txt"), "precious\n").unwrap();
    f.git_in(&wt, &["add", "-A"]);
    f.git_in(&wt, &["commit", "--quiet", "-m", "merge side"]);
    // The base takes the ordinary commits only.
    f.advance_main("other.txt");
    f.git(&["cherry-pick", &c1, &c2]);
    f.push_main();
    let (merge, _, _) = check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(merge, MergeState::NotMerged { files_changed: 3 });
}

#[tokio::test]
async fn repeated_squash_checks_leave_one_object() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    let from = create(&f, &mut git, &mut rx, "t").await;
    f.commit_file(&f.path("t"), "a.txt", "a\n", "one");
    f.advance_main("other.txt");
    let loose = || {
        let stats = f.git(&["count-objects", "-v"]);
        stats
            .lines()
            .find_map(|l| l.strip_prefix("count: "))
            .unwrap()
            .to_owned()
    };
    check(&f, &mut git, &mut rx, "t", &from).await;
    let after_one = loose();
    // A later second: an unpinned date would give a new commit object.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    check(&f, &mut git, &mut rx, "t", &from).await;
    assert_eq!(loose(), after_one);
}

#[tokio::test]
async fn delete_branch_without_force_keeps_untracked_work() {
    let f = Fixture::new();
    let (mut git, mut rx) = f.cli();
    create(&f, &mut git, &mut rx, "t").await;
    f.commit_file(&f.path("t"), "a.txt", "a\n", "work");
    f.git(&["merge", "--quiet", "--ff-only", &branch("t")]);
    f.push_main();
    let file = f.path("t").join("notes.txt");
    fs::write(&file, "keep me\n").unwrap();
    let result = remove(&mut git, &mut rx, f.remove_req("t", true, false)).await;
    assert!(result.is_err());
    assert!(file.exists());
    assert!(f.branch_exists("t"));
}
