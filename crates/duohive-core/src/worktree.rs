//! Git worktree types: the task a worktree belongs to and how its branch stands.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::names::AgentName;
use crate::time::Millis;
use crate::validate::slug;

/// Longest task id (P2-D7).
const ID_MAX: usize = 32;
/// Longest slug when a `-N` suffix has to fit.
const SUFFIXED_MAX: usize = 28;
/// The id of a task whose text gives no usable slug.
const FALLBACK_SLUG: &str = "task";

/// Names one task's worktree.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

/// The branch a worktree branched from: `origin/main` or `main`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseRef(pub String);

/// How a branch was merged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeKind {
    /// A merge commit or fast-forward.
    Normal,
    /// Rebased onto the base.
    Rebase,
    /// Squashed into one commit.
    Squash,
}

/// Whether a branch has reached its base.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeState {
    /// It has.
    Merged(MergeKind),
    /// It has not.
    NotMerged {
        /// Files that differ from the base.
        files_changed: u32,
    },
    /// It could not be told.
    Unknown(String),
}

/// Where an agent works.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunIn {
    /// The agent's own directory.
    Directory,
    /// A fresh git worktree.
    NewWorktree,
}

/// Where a worktree stands (P2-D6).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorktreeState {
    /// Created; not checked yet.
    Unchecked,
    /// Its agent is working in it.
    InUse,
    /// Its branch has not reached the base.
    NotMerged {
        /// Files that differ from the base.
        files_changed: u32,
    },
    /// Merged, with uncommitted changes left.
    MergedDirty,
    /// Merged and clean.
    Merged,
    /// Being removed.
    Removing,
    /// The check failed.
    CheckFailed(String),
}

/// One agent's worktree for one task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeRow {
    /// The agent that works in it.
    pub agent: AgentName,
    /// The task it was made for.
    pub task: TaskId,
    /// The repository it belongs to.
    pub repo: PathBuf,
    /// Where it lives.
    pub path: PathBuf,
    /// Its branch.
    pub branch: String,
    /// When it was created.
    pub created_at: Millis,
    /// The base commit its branch was created on (P2-D17).
    pub created_from: String,
    /// Where it stands.
    pub state: WorktreeState,
    /// When it was last checked.
    pub checked_at: Option<Millis>,
    /// The branch it is compared with.
    pub base: Option<BaseRef>,
}

/// The slug of a task's first line, at most 32 characters, or `task` when nothing is left (P2-D7).
pub fn task_slug(task: &str) -> String {
    let first = task.trim_start().lines().next().unwrap_or_default();
    let short: String = slug(first).chars().take(ID_MAX).collect();
    let short = short.trim_end_matches('-');
    if short.is_empty() {
        FALLBACK_SLUG.to_owned()
    } else {
        short.to_owned()
    }
}

/// A task id from `slug` that is not in `taken`. `attempt` 0 gives the first free one;
/// `n` skips `n` more free ones, for when git reports the branch exists (P2-D7).
pub fn task_id(slug: &str, taken: &BTreeSet<TaskId>, attempt: u8) -> TaskId {
    let base = if slug.is_empty() { FALLBACK_SLUG } else { slug };
    let suffixed: String = base.chars().take(SUFFIXED_MAX).collect();
    let suffixed = suffixed.trim_end_matches('-');
    let mut skip = attempt;
    for k in 1u32.. {
        let id = TaskId(if k == 1 {
            base.chars().take(ID_MAX).collect()
        } else {
            format!("{suffixed}-{k}")
        });
        if taken.contains(&id) {
            continue;
        }
        if skip == 0 {
            return id;
        }
        skip -= 1;
    }
    TaskId(base.to_owned())
}

/// The branch for an agent's task (P2-D7).
pub fn branch_name(agent: &AgentName, id: &TaskId) -> String {
    format!("duohive/{agent}/{}", id.0)
}

/// Where an agent's task worktree goes. Only joins paths.
pub fn worktree_path(data_dir: &Path, agent: &AgentName, id: &TaskId) -> PathBuf {
    data_dir.join("worktrees").join(agent.as_str()).join(&id.0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Component, Path};

    use super::*;
    use crate::testkit::n;

    fn is_safe(s: &str) -> bool {
        (1..=32).contains(&s.len())
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    }

    #[test]
    fn task_slugs_are_safe() {
        let long = "x".repeat(2_000);
        let hostile = [
            "/",
            "..",
            "../../etc/passwd",
            "🔥🔥",
            long.as_str(),
            "",
            "   \n",
        ];
        for text in hostile {
            let slug = task_slug(text);
            assert!(is_safe(&slug), "{text:?} gave {slug:?}");
            let path = worktree_path(
                Path::new("/d"),
                &n("squad-1"),
                &task_id(&slug, &BTreeSet::new(), 0),
            );
            assert!(path.starts_with("/d/worktrees/squad-1"), "{path:?}");
            assert!(!path.components().any(|c| c == Component::ParentDir));
        }
        assert_eq!(task_slug(""), "task");
        assert_eq!(task_slug("🔥"), "task");
        assert_eq!(task_slug("Fix the Tests!\nsecond line"), "fix-the-tests");
        assert_eq!(task_slug(&long).len(), 32);
    }

    #[test]
    fn task_ids_get_suffixes() {
        let taken: BTreeSet<TaskId> = ["fix", "fix-2"].map(|s| TaskId(s.into())).into();
        assert_eq!(task_id("fix", &taken, 0), TaskId("fix-3".into()));
        assert_eq!(task_id("fix", &taken, 1), TaskId("fix-4".into()));
        assert_eq!(task_id("other", &taken, 0), TaskId("other".into()));
        let long = "a".repeat(32);
        let taken: BTreeSet<TaskId> = [TaskId(long.clone())].into();
        let id = task_id(&long, &taken, 0);
        assert_eq!(id.0, format!("{}-2", "a".repeat(28)));
        assert!(id.0.len() <= 32);
        let hyphen = format!("{}-bbbb", "a".repeat(27));
        let taken: BTreeSet<TaskId> = [TaskId(hyphen.clone())].into();
        assert_eq!(
            task_id(&hyphen, &taken, 0).0,
            format!("{}-2", "a".repeat(27))
        );
    }

    #[test]
    fn branch_names_follow_the_agent_and_id() {
        assert_eq!(
            branch_name(&n("squad-1"), &TaskId("fix".into())),
            "duohive/squad-1/fix"
        );
    }
}
