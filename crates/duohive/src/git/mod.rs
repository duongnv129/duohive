//! Git worktrees over the `git` binary (TD §5.3).
//!
//! Each request ends with one [`Input::Git`]. Requests for one repo run in
//! order on that repo's worker thread; different repos run in parallel. Every
//! child is `git -C <dir>` with argv only, `GIT_TERMINAL_PROMPT=0`, `LC_ALL=C`
//! and a 30 s timeout. Workers are plain threads rather than tokio's process
//! support: the core's runtime has no I/O driver.
//!
//! Work is never lost: a path outside the worktrees directory is never
//! removed, a dirty worktree is removed only when the request forces it, and a
//! branch is deleted only after a fresh merge check still finds it merged.

mod merge;
mod run;

use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::Duration;

use duohive_core::engine::Input;
use duohive_core::ports::{CheckWorktree, CreateWorktree, Git, GitError, GitEvent, RemoveWorktree};
use duohive_core::worktree::{BaseRef, MergeState};
use tokio::sync::mpsc;

use merge::{head_ref, merge_state, ref_exists, resolve_base};
use run::Runner;

/// How long one git command may run.
pub const GIT_TIMEOUT: Duration = Duration::from_secs(30);
/// The answer when no default branch can be found (P2-D17).
const NO_DEFAULT_BRANCH: &str = "no default branch";
/// The answer when a branch asked to be deleted no longer checks as merged.
const NO_LONGER_MERGED: &str = "branch is no longer merged";

/// One request for a repo's worker.
#[derive(Debug)]
enum Job {
    Create(CreateWorktree),
    Check(CheckWorktree),
    Remove(RemoveWorktree),
}

impl Job {
    fn repo(&self) -> &Path {
        match self {
            Self::Create(r) => &r.repo,
            Self::Check(r) => &r.repo,
            Self::Remove(r) => &r.repo,
        }
    }

    /// The answer when the job could not be run at all.
    fn failed(self, reason: String) -> GitEvent {
        match self {
            Self::Create(r) => GitEvent::Created {
                agent: r.agent,
                task: r.task,
                result: Err(GitError::Other(reason)),
            },
            Self::Check(r) => GitEvent::Checked {
                agent: r.agent,
                task: r.task,
                merge: MergeState::Unknown(reason),
                dirty_files: 0,
                base: None,
                generation: r.generation,
            },
            Self::Remove(r) => GitEvent::Removed {
                agent: r.agent,
                task: r.task,
                result: Err(reason),
            },
        }
    }
}

/// [`Git`] over the `git` binary.
#[derive(Debug)]
pub struct GitCli {
    worktrees_dir: PathBuf,
    tx: mpsc::Sender<Input>,
    env: Vec<(OsString, OsString)>,
    workers: HashMap<PathBuf, std_mpsc::Sender<Job>>,
}

impl GitCli {
    /// `env` is added to every git child: production passes none; tests point
    /// `HOME` and `GIT_CONFIG_GLOBAL` at a temp dir.
    pub fn new(
        worktrees_dir: PathBuf,
        tx: mpsc::Sender<Input>,
        env: Vec<(OsString, OsString)>,
    ) -> Self {
        Self {
            worktrees_dir,
            tx,
            env,
            workers: HashMap::new(),
        }
    }

    fn submit(&mut self, job: Job) {
        let repo = job.repo().to_path_buf();
        let job = match self.workers.get(&repo) {
            Some(worker) => match worker.send(job) {
                Ok(()) => return,
                // The worker is gone: start a new one.
                Err(std_mpsc::SendError(job)) => job,
            },
            None => job,
        };
        match self.spawn_worker() {
            Ok(worker) => match worker.send(job) {
                Ok(()) => {
                    self.workers.insert(repo, worker);
                }
                Err(std_mpsc::SendError(job)) => {
                    self.answer_now(job.failed("git worker stopped".to_owned()));
                }
            },
            Err(e) => self.answer_now(job.failed(format!("cannot start a git worker: {e}"))),
        }
    }

    fn spawn_worker(&self) -> io::Result<std_mpsc::Sender<Job>> {
        let (jobs, rx) = std_mpsc::channel::<Job>();
        let worker = Worker {
            git: Runner {
                env: self.env.clone(),
                timeout: GIT_TIMEOUT,
            },
            worktrees_dir: self.worktrees_dir.clone(),
        };
        let tx = self.tx.clone();
        thread::Builder::new()
            .name("duohive-git".to_owned())
            .spawn(move || {
                for job in rx {
                    if tx.blocking_send(Input::Git(worker.run(job))).is_err() {
                        return;
                    }
                }
            })?;
        Ok(jobs)
    }

    /// Answers from the caller's thread, which may be async: never blocks.
    fn answer_now(&self, event: GitEvent) {
        if let Err(e) = self.tx.try_send(Input::Git(event)) {
            tracing::warn!(error = %e, "cannot report a git failure");
        }
    }
}

impl Git for GitCli {
    fn create(&mut self, req: CreateWorktree) {
        self.submit(Job::Create(req));
    }

    fn check(&mut self, req: CheckWorktree) {
        self.submit(Job::Check(req));
    }

    fn remove(&mut self, req: RemoveWorktree) {
        self.submit(Job::Remove(req));
    }
}

/// Does the git work for one repo, one request at a time.
#[derive(Debug)]
struct Worker {
    git: Runner,
    worktrees_dir: PathBuf,
}

impl Worker {
    fn run(&self, job: Job) -> GitEvent {
        match job {
            Job::Create(r) => {
                let result = self.create(&r);
                if let Err(e) = &result {
                    tracing::info!(agent = %r.agent, error = ?e, "worktree not created");
                }
                GitEvent::Created {
                    agent: r.agent,
                    task: r.task,
                    result,
                }
            }
            Job::Check(r) => {
                let (merge, dirty_files, base) = self.check(&r);
                GitEvent::Checked {
                    agent: r.agent,
                    task: r.task,
                    merge,
                    dirty_files,
                    base,
                    generation: r.generation,
                }
            }
            Job::Remove(r) => {
                let result = self.remove(&r);
                if let Err(e) = &result {
                    tracing::info!(agent = %r.agent, error = %e, "worktree not removed");
                }
                GitEvent::Removed {
                    agent: r.agent,
                    task: r.task,
                    result,
                }
            }
        }
    }

    fn is_repo(&self, repo: &Path) -> bool {
        self.git
            .git(repo, &["rev-parse", "--git-dir"])
            .is_ok_and(|out| out.ok())
    }

    /// `git worktree add -b <branch> -- <path> <base commit>`; an existing
    /// branch is never reused or reset (P2-D7).
    fn create(&self, r: &CreateWorktree) -> Result<(BaseRef, String), GitError> {
        if !self.is_repo(&r.repo) {
            return Err(GitError::NotARepo);
        }
        let base = resolve_base(&self.git, &r.repo)
            .map_err(GitError::Other)?
            .ok_or_else(|| GitError::Other(NO_DEFAULT_BRANCH.to_owned()))?;
        let from = self
            .git
            .text(
                &r.repo,
                &["rev-parse", "--verify", &format!("{}^{{commit}}", base.0)],
            )
            .map_err(GitError::Other)?
            .trim()
            .to_owned();
        if ref_exists(&self.git, &r.repo, &head_ref(&r.branch)).map_err(GitError::Other)? {
            return Err(GitError::BranchExists);
        }
        let mut args: Vec<OsString> = ["worktree", "add", "--quiet", "-b", &r.branch, "--"]
            .map(OsString::from)
            .to_vec();
        args.extend([r.path.clone().into_os_string(), OsString::from(&from)]);
        let out = self.git.git(&r.repo, &args).map_err(GitError::Other)?;
        if out.ok() {
            Ok((base, from))
        } else if out.stderr.contains("a branch named") && out.stderr.contains("already exists") {
            Err(GitError::BranchExists)
        } else {
            Err(GitError::Other(out.reason()))
        }
    }

    /// The merge state, the dirty file count and the base.
    fn check(&self, r: &CheckWorktree) -> (MergeState, u32, Option<BaseRef>) {
        if !self.is_repo(&r.repo) {
            return (MergeState::Unknown("not a git repo".to_owned()), 0, None);
        }
        let base = match resolve_base(&self.git, &r.repo) {
            Ok(Some(base)) => base,
            Ok(None) => return (MergeState::Unknown(NO_DEFAULT_BRANCH.to_owned()), 0, None),
            Err(e) => return (MergeState::Unknown(e), 0, None),
        };
        if r.created_from.trim().is_empty() {
            let reason = "no base commit recorded".to_owned();
            return (MergeState::Unknown(reason), 0, Some(base));
        }
        let merge = match merge_state(&self.git, &r.repo, &r.branch, &base, Some(&r.created_from)) {
            Ok(merge) => merge,
            Err(e) => return (MergeState::Unknown(e), 0, Some(base)),
        };
        match self.git.text(&r.path, &["status", "--porcelain"]) {
            Ok(status) => {
                let dirty = status.lines().filter(|l| !l.is_empty()).count();
                (merge, u32::try_from(dirty).unwrap_or(u32::MAX), Some(base))
            }
            Err(e) => {
                let reason = format!("couldn't read the worktree: {e}");
                (MergeState::Unknown(reason), 0, Some(base))
            }
        }
    }

    /// Removes the worktree, and with `delete_branch` its branch, but only
    /// when a fresh check still finds the branch merged. Nothing is touched
    /// when either check fails.
    fn remove(&self, r: &RemoveWorktree) -> Result<(), String> {
        let path = inside(&self.worktrees_dir, &r.path)?;
        let delete_branch =
            r.delete_branch && ref_exists(&self.git, &r.repo, &head_ref(&r.branch))?;
        if delete_branch {
            self.still_merged(r)?;
        }
        let mut args: Vec<OsString> = ["worktree", "remove"].map(OsString::from).to_vec();
        if r.force {
            args.push(OsString::from("--force"));
        }
        args.extend([OsString::from("--"), path.into_os_string()]);
        self.git.text(&r.repo, &args)?;
        if delete_branch {
            self.git.text(&r.repo, &["branch", "-D", &r.branch])?;
        }
        Ok(())
    }

    /// A stored `Merged` can be stale (a restart, a manual delete that skips
    /// confirming), so the branch is checked again against a fetched base.
    /// No `created_from` comes with a removal, so the fresh test is skipped:
    /// a branch whose tip is on the base loses nothing when deleted.
    fn still_merged(&self, r: &RemoveWorktree) -> Result<(), String> {
        let base = resolve_base(&self.git, &r.repo)?
            .ok_or_else(|| format!("couldn't confirm the merge: {NO_DEFAULT_BRANCH}"))?;
        match merge_state(&self.git, &r.repo, &r.branch, &base, None) {
            Ok(MergeState::Merged(_)) => Ok(()),
            Ok(MergeState::NotMerged { .. }) => Err(NO_LONGER_MERGED.to_owned()),
            Ok(MergeState::Unknown(e)) | Err(e) => Err(format!("couldn't confirm the merge: {e}")),
        }
    }
}

/// `path`, canonicalised, when it lies strictly under `root` (also
/// canonicalised); an error otherwise. A path that no longer exists resolves
/// through its nearest existing ancestor, and may not climb with `..`.
fn inside(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let refuse = || format!("{} is outside the worktrees folder", path.display());
    let root = root.canonicalize().map_err(|_| refuse())?;
    let resolved = resolve(path).ok_or_else(refuse)?;
    if resolved != root && resolved.starts_with(&root) {
        Ok(resolved)
    } else {
        Err(refuse())
    }
}

/// `path` with every symlink and `..` resolved, also when its last parts don't exist.
fn resolve(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut missing = Vec::new();
    let mut existing = path;
    loop {
        match existing.canonicalize() {
            Ok(found) => {
                let mut resolved = found;
                for part in missing.iter().rev() {
                    resolved.push(part);
                }
                return Some(resolved);
            }
            Err(_) => {
                let mut parts = existing.components();
                match parts.next_back()? {
                    Component::Normal(part) => missing.push(part.to_owned()),
                    _ => return None,
                }
                existing = parts.as_path();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn inside_accepts_only_paths_strictly_under_the_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("worktrees");
        fs::create_dir_all(root.join("a").join("t")).unwrap();
        fs::create_dir_all(tmp.path().join("x")).unwrap();
        assert!(inside(&root, &root.join("a").join("t")).is_ok());
        assert!(inside(&root, &root.join("a").join("gone")).is_ok());
        assert!(inside(&root, &root).is_err());
        assert!(inside(&root, &root.join("..").join("x")).is_err());
        assert!(inside(&root, &root.join("gone").join("..").join("..").join("x")).is_err());
        assert!(inside(&root, &tmp.path().join("x")).is_err());
        assert!(inside(&root, Path::new("worktrees/a")).is_err());
        assert!(inside(&tmp.path().join("missing"), &root.join("a")).is_err());
    }

    #[test]
    fn inside_follows_symlinks_out() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("worktrees");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(tmp.path().join("x")).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("x"), root.join("link")).unwrap();
        assert!(inside(&root, &root.join("link")).is_err());
    }
}
