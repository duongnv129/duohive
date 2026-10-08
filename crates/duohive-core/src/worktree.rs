//! Git worktree types: the task a worktree belongs to and how its branch stands.

use serde::{Deserialize, Serialize};

/// Names one task's worktree.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

/// One row of the worktree list. A placeholder until the worktree list lands.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeRow {
    /// The task the worktree belongs to.
    pub task: TaskId,
}
