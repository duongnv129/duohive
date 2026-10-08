//! Worktrees: creating one for a launch (TD §5.3; P2-D7, P2-D20, P2-D21).

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::lifecycle::not_found;
use super::{Engine, reject};
use crate::adapter::CliPresence;
use crate::event::Event;
use crate::inherit::{expand_home, resolve};
use crate::names::AgentName;
use crate::ports::{CreateWorktree, GitError};
use crate::status::AgentState;
use crate::validate::{DirStatus, agent_problem};
use crate::worktree::{
    BaseRef, TaskId, WorktreeRow, WorktreeState, branch_name, task_id, task_slug, worktree_path,
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
        if let Err(e) = self.ports.store.save_worktree(&row) {
            out.push(Event::CommandRejected {
                reason: format!("couldn't save state: {e}"),
            });
        }
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
        if let Err(e) = self.ports.store.delete_worktree(&row.agent, &row.task) {
            out.push(Event::CommandRejected {
                reason: format!("couldn't save state: {e}"),
            });
        }
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
