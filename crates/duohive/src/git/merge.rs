//! The base branch (P2-D17) and merge detection (TD §5.3): fresh, normal,
//! rebase, squash, in that order.

use std::path::Path;

use duohive_core::worktree::{BaseRef, MergeKind, MergeState};

use super::run::Runner;

/// The remote whose default branch is the base.
const REMOTE: &str = "origin";
/// The local defaults after `init.defaultBranch`, in order.
const LOCAL_DEFAULTS: [&str; 2] = ["main", "master"];
/// The identity `commit-tree` uses, so a machine without one can still check squashes.
const SQUASH_IDENTITY: [&str; 4] = [
    "-c",
    "user.name=duohive",
    "-c",
    "user.email=duohive@localhost",
];

/// The fully qualified ref of a local branch.
pub(super) fn head_ref(branch: &str) -> String {
    format!("refs/heads/{branch}")
}

/// Whether `reference` names an existing ref.
pub(super) fn ref_exists(git: &Runner, repo: &Path, reference: &str) -> Result<bool, String> {
    let out = git.git(repo, &["rev-parse", "--verify", "--quiet", reference])?;
    Ok(out.ok())
}

/// The base branch, fetched quietly first when it is `origin`'s; `None` when
/// there is none. A failed fetch is ignored: offline, the local refs answer.
pub(super) fn resolve_base(git: &Runner, repo: &Path) -> Result<Option<BaseRef>, String> {
    let remote_branch = if has_origin(git, repo)? {
        origin_head(git, repo)?.or_else(|| remote_head(git, repo))
    } else {
        None
    };
    if let Some(branch) = remote_branch {
        let _ = git.git(repo, &["fetch", "--quiet", REMOTE, branch.as_str()]);
        let tracking = format!("{REMOTE}/{branch}");
        if ref_exists(git, repo, &format!("refs/remotes/{tracking}"))? {
            return Ok(Some(BaseRef(tracking)));
        }
    }
    local_default(git, repo)
}

fn has_origin(git: &Runner, repo: &Path) -> Result<bool, String> {
    let remotes = git.text(repo, &["remote"])?;
    Ok(remotes.lines().any(|r| r.trim() == REMOTE))
}

/// `<b>` from `refs/remotes/origin/HEAD` (`origin/<b>`), when it is set.
fn origin_head(git: &Runner, repo: &Path) -> Result<Option<String>, String> {
    let out = git.git(
        repo,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )?;
    if !out.ok() {
        return Ok(None);
    }
    let prefix = format!("{REMOTE}/");
    Ok(out
        .stdout
        .trim()
        .strip_prefix(&prefix)
        .filter(|b| !b.is_empty())
        .map(str::to_owned))
}

/// `<b>` from `git ls-remote --symref origin HEAD`; `None` offline.
fn remote_head(git: &Runner, repo: &Path) -> Option<String> {
    let out = git
        .text(repo, &["ls-remote", "--symref", REMOTE, "HEAD"])
        .ok()?;
    out.lines().find_map(|line| {
        let (target, name) = line.strip_prefix("ref:")?.trim().split_once('\t')?;
        (name.trim() == "HEAD")
            .then(|| target.trim().strip_prefix("refs/heads/"))
            .flatten()
            .filter(|b| !b.is_empty())
            .map(str::to_owned)
    })
}

/// The first that exists of `init.defaultBranch`, `main`, `master`.
fn local_default(git: &Runner, repo: &Path) -> Result<Option<BaseRef>, String> {
    let configured = git.git(repo, &["config", "--get", "init.defaultBranch"])?;
    let configured = configured
        .ok()
        .then(|| configured.stdout.trim().to_owned())
        .filter(|b| !b.is_empty());
    for candidate in configured
        .into_iter()
        .chain(LOCAL_DEFAULTS.map(str::to_owned))
    {
        if ref_exists(git, repo, &head_ref(&candidate))? {
            return Ok(Some(BaseRef(candidate)));
        }
    }
    Ok(None)
}

/// Whether `branch` has reached `base`. With `created_from`, a branch with no
/// commits after it is never merged (P2-D17).
pub(super) fn merge_state(
    git: &Runner,
    repo: &Path,
    branch: &str,
    base: &BaseRef,
    created_from: Option<&str>,
) -> Result<MergeState, String> {
    let tip = head_ref(branch);
    if !ref_exists(git, repo, &tip)? {
        return Err(format!("branch {branch} not found"));
    }
    let base = base.0.as_str();
    // 1. Fresh.
    if let Some(from) = created_from {
        let count = git.text(repo, &["rev-list", "--count", &format!("{from}..{tip}")])?;
        if count.trim() == "0" {
            return Ok(MergeState::NotMerged { files_changed: 0 });
        }
    }
    // 2. Normal: a merge commit or a fast-forward.
    let ancestor = git.git(repo, &["merge-base", "--is-ancestor", &tip, base])?;
    match ancestor.code {
        Some(0) => return Ok(MergeState::Merged(MergeKind::Normal)),
        Some(1) => {}
        _ => return Err(ancestor.reason()),
    }
    // 3. Rebase: every branch commit has an equivalent on the base.
    if all_applied(git, repo, base, &tip)? {
        return Ok(MergeState::Merged(MergeKind::Rebase));
    }
    // 4. Squash: the branch's whole change, as one commit, has an equivalent.
    let merge_base = git.text(repo, &["merge-base", base, &tip])?;
    let merge_base = merge_base.trim();
    let tree = format!("{tip}^{{tree}}");
    let mut args: Vec<&str> = SQUASH_IDENTITY.to_vec();
    args.extend(["commit-tree", &tree, "-p", merge_base, "-m", "squash"]);
    let synthetic = git.text(repo, &args)?;
    if all_applied(git, repo, base, synthetic.trim())? {
        return Ok(MergeState::Merged(MergeKind::Squash));
    }
    let files = git.text(
        repo,
        &["diff", "--name-only", &format!("{merge_base}..{tip}")],
    )?;
    let files_changed = files.lines().filter(|l| !l.is_empty()).count();
    Ok(MergeState::NotMerged {
        files_changed: u32::try_from(files_changed).unwrap_or(u32::MAX),
    })
}

/// `git cherry <base> <head>` lists no `+` line.
fn all_applied(git: &Runner, repo: &Path, base: &str, head: &str) -> Result<bool, String> {
    let cherry = git.text(repo, &["cherry", base, head])?;
    Ok(!cherry.lines().any(|l| l.starts_with('+')))
}
