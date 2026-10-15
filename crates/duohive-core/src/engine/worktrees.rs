//! Worktrees (TD §5.3): creating one for a launch (P2-D7, P2-D20, P2-D21),
//! then checking and cleaning them up (P2-D6, P2-D16; Review Focus 2).
//!
//! Removal never loses work: a row is removed automatically only when its
//! check says merged with no dirty files **and** no live session runs in it.
//! A row being created (P2-D20) or being removed is never checked or removed.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::lifecycle::{getting_ready, not_found};
use super::timers::WORKTREE_CHECK_EVERY;
use super::{Engine, reject};
use crate::adapter::CliPresence;
use crate::event::Event;
use crate::inherit::{expand_home, resolve};
use crate::names::AgentName;
use crate::ports::{CheckWorktree, CreateWorktree, GitError, RemoveWorktree};
use crate::status::AgentState;
use crate::validate::{DirStatus, agent_problem};
use crate::worktree::{
    BaseRef, MergeState, TaskId, WorktreeRow, WorktreeState, branch_name, task_id, task_slug,
    worktree_path,
};

/// P2-D7: how many branch names are tried before giving up.
const CREATE_ATTEMPTS: u8 = 9;

/// A worktree being created for `agent`'s next launch (P2-D20).
#[derive(Clone, Debug)]
pub(super) struct Creating {
    /// The task the launch starts with.
    task: String,
    /// Which try this is, from 0 (P2-D7).
    attempt: u8,
    /// The row saved for this try.
    row: WorktreeRow,
}

impl Engine {
    /// Whether a worktree is being created for `agent` (P2-D20).
    pub(super) fn creating_worktree(&self, agent: &AgentName) -> bool {
        self.agents
            .get(agent)
            .is_some_and(|rt| rt.creating.is_some())
    }

    /// Give work in a new worktree on a stopped or errored agent: checks the
    /// launch can go ahead, then asks git for the worktree. The launch runs
    /// when git answers `Created`.
    pub(super) fn begin_worktree(&mut self, agent: &AgentName, task: String) -> Vec<Event> {
        let r = match resolve(&self.org, agent) {
            Ok(r) => r,
            Err(e) => return reject(e.message),
        };
        let home = self.cfg.home.clone();
        // D26: every launch probes the agent's directory.
        let written = r.directory.clone();
        let repo = written.as_deref().map(|d| expand_home(d, &home));
        let probe = repo.as_deref().map(|d| self.ports.dirs.probe(d));
        if let Some(problem) = agent_problem(&r, &home, probe) {
            return reject(problem);
        }
        let (Some(written), Some(repo)) = (written, repo) else {
            return reject("no directory set · e to fix");
        };
        if probe != Some(DirStatus::GitRepo) {
            return reject(format!(
                "new worktree needs a git repo; {written} isn't one"
            ));
        }
        let found = matches!(
            self.cli.as_ref().and_then(|m| m.get(&r.cli)),
            Some(CliPresence::Found { .. })
        ) && self.adapters.get(r.cli).is_some();
        if !found {
            return self.launch_fails(agent, Some(task), not_found(r.cli));
        }
        self.rt_mut(agent).last_task = Some(task.clone());
        match self.request_worktree(agent, task, repo, 0) {
            Ok(()) => Vec::new(),
            Err(reason) => reject(reason),
        }
    }

    /// Saves an `Unchecked` row for try `attempt` and calls `Git::create`.
    fn request_worktree(
        &mut self,
        agent: &AgentName,
        task: String,
        repo: PathBuf,
        attempt: u8,
    ) -> Result<(), String> {
        let taken: BTreeSet<TaskId> = self
            .ports
            .store
            .load_worktrees()
            .map_err(|e| format!("couldn't read worktrees: {e}"))?
            .into_iter()
            .filter(|row| row.agent == *agent)
            .map(|row| row.task)
            .collect();
        let id = task_id(&task_slug(&task), &taken, attempt);
        let row = WorktreeRow {
            agent: agent.clone(),
            path: worktree_path(&self.cfg.data_dir, agent, &id),
            branch: branch_name(agent, &id),
            task: id,
            repo,
            created_at: self.ports.clock.now(),
            created_from: String::new(),
            state: WorktreeState::Unchecked,
            checked_at: None,
            base: None,
        };
        self.ports
            .store
            .save_worktree(&row)
            .map_err(|e| format!("couldn't save state: {e}"))?;
        self.ports.git.create(CreateWorktree {
            agent: agent.clone(),
            task: row.task.clone(),
            repo: row.repo.clone(),
            path: row.path.clone(),
            branch: row.branch.clone(),
        });
        self.rt_mut(agent).creating = Some(Creating { task, attempt, row });
        self.worktrees_stale = true;
        Ok(())
    }

    /// `Created`: launch in the new worktree, try the next branch name, or give up.
    pub(super) fn on_created(
        &mut self,
        agent: &AgentName,
        task: &TaskId,
        result: Result<(BaseRef, String), GitError>,
    ) -> Vec<Event> {
        let Some(c) = self
            .agents
            .get_mut(agent)
            .and_then(|rt| rt.creating.take_if(|c| c.row.task == *task))
        else {
            tracing::debug!(%agent, task = %task.0, "worktree answer for no creation in progress");
            return Vec::new();
        };
        match result {
            Ok((base, from)) => self.launch_in_worktree(agent, c, base, from),
            Err(GitError::BranchExists) if c.attempt + 1 < CREATE_ATTEMPTS => {
                let mut out = Vec::new();
                self.drop_worktree_row(&c.row, &mut out);
                let retry = self.request_worktree(agent, c.task, c.row.repo, c.attempt + 1);
                if let Err(reason) = retry {
                    out.extend(self.creation_failed(agent, &reason));
                }
                out
            }
            Err(e) => {
                let mut out = Vec::new();
                self.drop_worktree_row(&c.row, &mut out);
                out.extend(self.creation_failed(agent, &describe(&e)));
                out
            }
        }
    }

    /// Launches in the created worktree; its row records the base and, once the
    /// session runs there, `InUse`.
    fn launch_in_worktree(
        &mut self,
        agent: &AgentName,
        c: Creating,
        base: BaseRef,
        from: String,
    ) -> Vec<Event> {
        let Creating { task, row, .. } = c;
        let id = row.task.clone();
        let mut out = self.launch(agent, Some(task), Some((id.clone(), row.path.clone())));
        let in_use = self.agents.get(agent).and_then(|rt| rt.worktree.as_ref()) == Some(&id);
        let row = WorktreeRow {
            base: Some(base),
            created_from: from,
            state: if in_use {
                WorktreeState::InUse
            } else {
                WorktreeState::Unchecked
            },
            ..row
        };
        self.store_row(&row, &mut out);
        out
    }

    /// Creation failed: a stopped agent stays stopped and an errored one stays
    /// errored (P2-D21), with the error on its row; `last_task` keeps the task.
    fn creation_failed(&mut self, agent: &AgentName, error: &str) -> Vec<Event> {
        let rt = self.rt_mut(agent);
        rt.creating = None;
        rt.status.error = Some(format!("couldn't create worktree: {error} · space"));
        let status = rt.status.clone();
        let mut out = vec![Event::AgentStatusChanged {
            agent: agent.clone(),
            status,
        }];
        // A stopped agent has no session row.
        if self.state_of(agent) != AgentState::Stopped {
            self.save_row(agent, &mut out);
        }
        out
    }

    /// Deletes a worktree row; a failure is reported, not fatal.
    fn drop_worktree_row(&mut self, row: &WorktreeRow, out: &mut Vec<Event>) {
        self.worktrees_stale = true;
        self.worktree_generations
            .remove(&(row.agent.clone(), row.task.clone()));
        if let Err(e) = self.ports.store.delete_worktree(&row.agent, &row.task) {
            out.push(Event::CommandRejected {
                reason: format!("couldn't save state: {e}"),
            });
        }
    }
}

impl Engine {
    /// Whether a live session runs in `row` (P2-D6): its agent is running and
    /// its current session was launched there. `SessionRow.worktree` restores
    /// this after a restart; a held row (a running agent no org names) counts.
    fn in_use(&self, row: &WorktreeRow) -> bool {
        let running = self.agents.get(&row.agent).is_some_and(|rt| {
            rt.status.state.is_running() && rt.worktree.as_ref() == Some(&row.task)
        });
        running || self.held_in(&row.agent, &row.task) || self.orphan_in(&row.agent, &row.task)
    }

    /// `row`'s incarnation, given now when it has none.
    fn generation(&mut self, row: &WorktreeRow) -> u64 {
        let next = &mut self.next_generation;
        *self
            .worktree_generations
            .entry((row.agent.clone(), row.task.clone()))
            .or_insert_with(|| {
                *next += 1;
                *next
            })
    }

    /// Whether `row` is the one being created for its agent's next launch
    /// (P2-D20). Its `Created` answer owns it: it is neither checked nor removed.
    fn being_created(&self, row: &WorktreeRow) -> bool {
        self.agents
            .get(&row.agent)
            .and_then(|rt| rt.creating.as_ref())
            .is_some_and(|c| c.row.task == row.task)
    }

    /// Whether `agent` has any worktree row; renames are refused while it has.
    pub(super) fn has_worktrees(&mut self, agent: &AgentName) -> Result<bool, String> {
        let rows = self
            .ports
            .store
            .load_worktrees()
            .map_err(|e| format!("couldn't read worktrees: {e}"))?;
        Ok(rows.iter().any(|row| row.agent == *agent))
    }

    /// Every worktree row; a failure is reported and reads as none.
    fn worktree_rows(&mut self, out: &mut Vec<Event>) -> Vec<WorktreeRow> {
        self.ports.store.load_worktrees().unwrap_or_else(|e| {
            out.push(Event::CommandRejected {
                reason: format!("couldn't read worktrees: {e}"),
            });
            Vec::new()
        })
    }

    /// One row by agent and task.
    fn worktree_row(
        &mut self,
        agent: &AgentName,
        task: &TaskId,
        out: &mut Vec<Event>,
    ) -> Option<WorktreeRow> {
        self.worktree_rows(out)
            .into_iter()
            .find(|row| row.agent == *agent && row.task == *task)
    }

    /// Saves `row` and marks the list for publishing; a failure is reported.
    /// Returns whether it was saved.
    fn store_row(&mut self, row: &WorktreeRow, out: &mut Vec<Event>) -> bool {
        self.worktrees_stale = true;
        match self.ports.store.save_worktree(row) {
            Ok(()) => true,
            Err(e) => {
                out.push(Event::CommandRejected {
                    reason: format!("couldn't save state: {e}"),
                });
                false
            }
        }
    }

    /// Asks git to check each row that isn't being removed or created; with
    /// `only`, just that agent's rows (it entered finished). TD §5.3's
    /// triggers: finished, the first `DirInventory`, `CheckWorktrees`, and
    /// the 10-minute timer.
    pub(super) fn check_worktrees(&mut self, only: Option<&AgentName>, out: &mut Vec<Event>) {
        self.worktrees_stale = true;
        for row in self.worktree_rows(out) {
            let skip = row.state == WorktreeState::Removing
                || self.being_created(&row)
                || only.is_some_and(|a| *a != row.agent);
            if skip {
                continue;
            }
            let generation = self.generation(&row);
            self.ports.git.check(CheckWorktree {
                agent: row.agent,
                task: row.task,
                repo: row.repo,
                path: row.path,
                branch: row.branch,
                created_from: row.created_from,
                generation,
            });
        }
    }

    /// `Checked`: in use stays `InUse` whatever the result (P2-D16); merged
    /// with no dirty files is removed, keeping nothing; anything else is kept
    /// and listed. `Unknown` is never merged. While reconnecting, nothing is
    /// removed: a session the inventory hasn't settled may still run there,
    /// so merged and clean stays `Merged` until the next check. An answer for
    /// an older row with the same agent and task is ignored.
    pub(super) fn on_checked(
        &mut self,
        agent: &AgentName,
        task: &TaskId,
        merge: MergeState,
        dirty_files: u32,
        base: Option<BaseRef>,
        generation: u64,
    ) -> Vec<Event> {
        let mut out = Vec::new();
        let key = (agent.clone(), task.clone());
        if self.worktree_generations.get(&key) != Some(&generation) {
            tracing::debug!(%agent, task = %task.0, "check answer for an older row");
            return out;
        }
        let Some(row) = self.worktree_row(agent, task, &mut out) else {
            tracing::debug!(%agent, task = %task.0, "check answer for no worktree row");
            return out;
        };
        if row.state == WorktreeState::Removing || self.being_created(&row) {
            return out;
        }
        let state = if self.in_use(&row) {
            WorktreeState::InUse
        } else {
            match merge {
                MergeState::Merged(_) if dirty_files == 0 && self.reconnect.is_some() => {
                    WorktreeState::Merged
                }
                MergeState::Merged(_) if dirty_files == 0 => WorktreeState::Removing,
                MergeState::Merged(_) => WorktreeState::MergedDirty,
                MergeState::NotMerged { files_changed } => {
                    WorktreeState::NotMerged { files_changed }
                }
                MergeState::Unknown(reason) => WorktreeState::CheckFailed(reason),
            }
        };
        let row = WorktreeRow {
            state,
            checked_at: Some(self.ports.clock.now()),
            base: base.or(row.base),
            ..row
        };
        // Removed only once the row says so, so a restart re-checks it.
        if self.store_row(&row, &mut out) && row.state == WorktreeState::Removing {
            self.ports.git.remove(remove_request(row, true, false));
        }
        out
    }

    /// `Removed`: the row goes; a failure keeps it, with the reason.
    pub(super) fn on_removed(
        &mut self,
        agent: &AgentName,
        task: &TaskId,
        result: Result<(), String>,
    ) -> Vec<Event> {
        let mut out = Vec::new();
        let Some(row) = self
            .worktree_row(agent, task, &mut out)
            .filter(|row| row.state == WorktreeState::Removing)
        else {
            tracing::debug!(%agent, task = %task.0, "removal answer for no row being removed");
            return out;
        };
        match result {
            Ok(()) => self.drop_worktree_row(&row, &mut out),
            Err(reason) => {
                let row = WorktreeRow {
                    state: WorktreeState::CheckFailed(format!("couldn't remove: {reason}")),
                    ..row
                };
                self.store_row(&row, &mut out);
            }
        }
        out
    }

    /// `DeleteWorktree`, by hand (P2-D6): refused while in use; merged and
    /// clean goes with its branch; anything else needs `confirmed`, and is
    /// then forced with its branch kept, so unmerged work stays on it.
    pub(super) fn delete_worktree(
        &mut self,
        agent: &AgentName,
        task: &TaskId,
        confirmed: bool,
    ) -> Vec<Event> {
        let mut out = Vec::new();
        let Some(row) = self.worktree_row(agent, task, &mut out) else {
            out.extend(reject(format!("no worktree {} for {agent}", task.0)));
            return out;
        };
        let refusal = if self.being_created(&row) {
            Some(getting_ready(agent))
        } else if row.state == WorktreeState::Removing {
            Some(format!("{} is being removed", row.branch))
        } else if self.in_use(&row) {
            Some(format!("{agent} is running in it · stop it first"))
        } else if row.state != WorktreeState::Merged && !confirmed {
            Some(format!(
                "{} isn't merged and clean · confirm to remove it",
                row.branch
            ))
        } else {
            None
        };
        if let Some(reason) = refusal {
            out.extend(reject(reason));
            return out;
        }
        let clean = row.state == WorktreeState::Merged;
        let row = WorktreeRow {
            state: WorktreeState::Removing,
            ..row
        };
        if self.store_row(&row, &mut out) {
            self.ports.git.remove(remove_request(row, clean, !clean));
        }
        out
    }

    /// Start-up: a removal the restart cut short is checked again, never
    /// assumed done.
    pub(super) fn reset_removing(&mut self, out: &mut Vec<Event>) {
        for row in self.worktree_rows(out) {
            if row.state == WorktreeState::Removing {
                let row = WorktreeRow {
                    state: WorktreeState::Unchecked,
                    ..row
                };
                self.store_row(&row, out);
            }
        }
    }

    /// Publishes `Worktrees` when the rows changed since the last one. First
    /// each row's `InUse` is brought in line with the sessions: in use becomes
    /// `InUse`, and a row no longer in use goes back to `Unchecked` until the
    /// next check. Rows being created or removed keep their state.
    pub(super) fn publish_worktrees(&mut self, out: &mut Vec<Event>) {
        if !std::mem::take(&mut self.worktrees_stale) {
            return;
        }
        let mut rows = self.worktree_rows(out);
        for row in &mut rows {
            if row.state == WorktreeState::Removing || self.being_created(row) {
                continue;
            }
            let state = if self.in_use(row) {
                WorktreeState::InUse
            } else if row.state == WorktreeState::InUse {
                WorktreeState::Unchecked
            } else {
                continue;
            };
            if row.state == state {
                continue;
            }
            row.state = state;
            let saved = row.clone();
            self.store_row(&saved, out);
        }
        // Saving above marked the list stale again; these rows are current.
        self.worktrees_stale = false;
        if rows != self.worktrees_shown {
            // The timer is no deadline without worktrees: the first one starts it afresh.
            if self.worktrees_shown.is_empty() && !rows.is_empty() {
                self.worktree_check_due = self.ports.clock.now() + WORKTREE_CHECK_EVERY;
            }
            self.worktrees_shown.clone_from(&rows);
            out.push(Event::Worktrees(rows));
        }
    }
}

/// The port request removing `row`.
fn remove_request(row: WorktreeRow, delete_branch: bool, force: bool) -> RemoveWorktree {
    RemoveWorktree {
        agent: row.agent,
        task: row.task,
        repo: row.repo,
        path: row.path,
        branch: row.branch,
        delete_branch,
        force,
    }
}

/// The `<e>` in `couldn't create worktree: <e>`.
fn describe(e: &GitError) -> String {
    match e {
        GitError::BranchExists => "branch exists".to_owned(),
        GitError::NotARepo => "not a git repo".to_owned(),
        GitError::Other(reason) => reason.clone(),
    }
}

#[cfg(test)]
mod tests;
