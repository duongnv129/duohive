//! `w`: the worktree list and its keys (UI §4.9, P2-D6).

use duohive_core::command::Command;
use duohive_core::worktree::{WorktreeRow, WorktreeState};

use super::{Action, UiEffect, agent, confirm};
use crate::model::{Confirm, Dialog, UiModel, WorktreeList};
use crate::render::files_changed;

/// Said for a worktree its agent is running in (P2-D6).
fn in_use(row: &WorktreeRow) -> String {
    format!("{} is running in it · stop it first", row.agent)
}

/// `w`: opens the list and asks the engine to check the worktrees first.
pub(super) fn open(m: &mut UiModel) -> Vec<UiEffect> {
    m.dialog = Some(Dialog::Worktrees(WorktreeList::default()));
    vec![UiEffect::Send(Command::CheckWorktrees)]
}

/// A key in the open list: `↑↓` choose, `enter` goes to the agent, `d` removes, `esc` goes back.
pub(super) fn action(m: &mut UiModel, mut l: WorktreeList, a: Action) -> Vec<UiEffect> {
    let last = m.worktrees.len().saturating_sub(1);
    l.cursor = l.cursor.min(last);
    match a {
        Action::Esc | Action::DialogNo => return Vec::new(),
        Action::Up => l.cursor = l.cursor.saturating_sub(1),
        Action::Down => l.cursor = (l.cursor + 1).min(last),
        Action::DialogEnter => {
            let Some(row) = m.worktrees.get(l.cursor) else {
                m.dialog = Some(Dialog::Worktrees(l));
                return Vec::new();
            };
            let agent = row.agent.clone();
            if m.reveal_agent(&agent) {
                return agent::view_if_finished(m);
            }
            m.footer_note = Some(format!("{agent} is no longer in the org"));
        }
        Action::Delete => return remove(m, l),
        _ => {}
    }
    m.dialog = Some(Dialog::Worktrees(l));
    Vec::new()
}

/// `d`, mirroring the engine: in use is refused, merged and clean goes at once, anything
/// else asks first.
fn remove(m: &mut UiModel, l: WorktreeList) -> Vec<UiEffect> {
    let Some(row) = m.worktrees.get(l.cursor).cloned() else {
        m.dialog = Some(Dialog::Worktrees(l));
        return Vec::new();
    };
    let delete = |confirmed| Command::DeleteWorktree {
        agent: row.agent.clone(),
        task: row.task.clone(),
        confirmed,
    };
    let note = match &row.state {
        WorktreeState::InUse => Some(in_use(&row)),
        WorktreeState::Removing => Some(format!("{} is being removed", row.branch)),
        WorktreeState::Merged => {
            m.dialog = Some(Dialog::Worktrees(l));
            return vec![UiEffect::Send(delete(false))];
        }
        _ => None,
    };
    if note.is_some() {
        m.footer_note = note;
        m.dialog = Some(Dialog::Worktrees(l));
        return Vec::new();
    }
    let lines = vec![
        format!(
            "Remove {}'s worktree on {}? {}",
            row.agent,
            row.branch,
            why_asked(&row.state)
        ),
        "Everything in it that isn't committed is lost, including untracked and ignored files. The branch is kept.".to_owned(),
    ];
    let Dialog::Confirm(c) = confirm("Remove worktree?", lines, "Remove", delete(true)) else {
        return Vec::new();
    };
    m.dialog = Some(Dialog::Confirm(Box::new(Confirm {
        back_list: Some(l),
        ..*c
    })));
    Vec::new()
}

/// Why this worktree isn't removed without asking.
fn why_asked(state: &WorktreeState) -> String {
    match state {
        WorktreeState::NotMerged { files_changed: n } => {
            format!("It isn't merged ({}).", files_changed(*n))
        }
        WorktreeState::MergedDirty => "It has uncommitted changes.".to_owned(),
        WorktreeState::CheckFailed(r) if r.starts_with("couldn't remove") => {
            "Its earlier removal failed.".to_owned()
        }
        WorktreeState::CheckFailed(_) => "It couldn't be checked.".to_owned(),
        _ => "It hasn't been checked yet.".to_owned(),
    }
}
