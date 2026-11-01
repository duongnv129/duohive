//! Resuming an old session (v1.1; V11-D3, V11-D4; TD §5.6): listing the
//! sessions a stopped agent could pick up, and marking those another agent holds.

use super::lifecycle::{getting_ready, not_found};
use super::signals::is_valid_session_id;
use super::{Engine, reject};
use crate::adapter::CliPresence;
use crate::command::Command;
use crate::event::Event;
use crate::inherit::{expand_home, resolve};
use crate::launch::ResumeTarget;
use crate::names::AgentName;
use crate::session_catalog::{CatalogError, SessionInfo};
use crate::status::AgentState;
use crate::worktree::{TaskId, WorktreeState};

const NOT_LISTED: &str = "that session isn't in the list \u{b7} open the picker again";

impl Engine {
    /// `ListSessions`: asks the catalog for the sessions that ran in the
    /// agent's directory and in each worktree it still keeps. The answer
    /// arrives later as `Input::SessionsListed`.
    pub(super) fn list_sessions(&mut self, agent: AgentName) -> Vec<Event> {
        if !self.knows(&agent) {
            return reject(format!("no agent named {agent}"));
        }
        if self.creating_worktree(&agent) {
            return reject(getting_ready(&agent));
        }
        if self.state_of(&agent).is_running() {
            return reject(format!("{agent} is running \u{b7} stop it first (s)"));
        }
        let r = match resolve(&self.org, &agent) {
            Ok(r) => r,
            Err(e) => return reject(e.message),
        };
        let cli = r.cli;
        let found = matches!(
            self.cli.as_ref().and_then(|m| m.get(&cli)),
            Some(CliPresence::Found { .. })
        );
        if !found {
            return vec![Event::SessionsUnreadable {
                agent,
                reason: not_found(cli),
            }];
        }
        let mut dirs = Vec::new();
        if let Some(d) = r.directory.as_deref() {
            dirs.push((expand_home(d, &self.cfg.home), None));
        }
        let mut out = Vec::new();
        let kept: Vec<_> = self
            .worktree_rows(&mut out)
            .into_iter()
            .filter(|row| row.agent == agent)
            .map(|row| (row.path, Some(row.task)))
            .collect();
        if !out.is_empty() {
            return out;
        }
        dirs.extend(kept);
        self.ports.sessions.request(&agent, cli, dirs);
        Vec::new()
    }

    /// The catalog's answer: the sessions with `in_use_by` marked, or why not.
    pub(super) fn on_sessions_listed(
        &mut self,
        agent: AgentName,
        result: Result<Vec<SessionInfo>, CatalogError>,
    ) -> Vec<Event> {
        match result {
            Err(e) => vec![Event::SessionsUnreadable {
                agent,
                reason: e.to_string(),
            }],
            Ok(mut sessions) => {
                for s in &mut sessions {
                    s.in_use_by = self.holder_of(&s.id, &agent);
                }
                self.last_listed.insert(agent.clone(), sessions.clone());
                vec![Event::SessionsListed { agent, sessions }]
            }
        }
    }

    /// `ResumeSession`: starts the agent on an old session it was shown (V11-D3).
    /// The checks run in order; the launch then goes the way any start does.
    pub(super) fn resume_session(&mut self, agent: AgentName, session_id: String) -> Vec<Event> {
        if !self.knows(&agent) {
            return reject(format!("no agent named {agent}"));
        }
        if !is_valid_session_id(&session_id) {
            return reject("that isn't a session id".to_owned());
        }
        if self.creating_worktree(&agent) {
            return reject(getting_ready(&agent));
        }
        if self.state_of(&agent).is_running() {
            return reject(format!("{agent} is running \u{b7} stop it first (s)"));
        }
        let Some(session) = self
            .last_listed
            .get(&agent)
            .and_then(|l| l.iter().find(|s| s.id == session_id))
            .cloned()
        else {
            return reject(NOT_LISTED.to_owned());
        };
        // Live, not from the list: two clicks can race (Review Focus 3).
        if let Some(holder) = self.holder_of(&session_id, &agent) {
            return reject(format!("{holder} is using this session"));
        }
        let mut out = Vec::new();
        let worktree = match session.worktree.as_ref() {
            None => None,
            Some(task) => match self.kept_worktree(&agent, task, &mut out) {
                Ok(w) => Some(w),
                Err(reason) => {
                    out.extend(reject(reason));
                    return out;
                }
            },
        };
        if self.replace.is_some() {
            return reject(super::replace::REPLACING.to_owned());
        }
        if self.cli.is_none() {
            self.queued_launches
                .push(Command::ResumeSession { agent, session_id });
            return Vec::new();
        }
        let target = ResumeTarget {
            session_id,
            cwd: session.dir,
        };
        let in_worktree = worktree.is_some();
        out.extend(self.launch(&agent, None, worktree, Some(target)));
        if in_worktree {
            // The row goes back to in use at the next publish (P2-D16).
            self.worktrees_stale = true;
        }
        out
    }

    /// The agent's kept worktree for `task`, with its path, if a launch may use it.
    fn kept_worktree(
        &mut self,
        agent: &AgentName,
        task: &TaskId,
        out: &mut Vec<Event>,
    ) -> Result<(TaskId, std::path::PathBuf), String> {
        let row = self
            .worktree_rows(out)
            .into_iter()
            .find(|r| r.agent == *agent && r.task == *task)
            .ok_or_else(|| NOT_LISTED.to_owned())?;
        if row.state == WorktreeState::Removing {
            return Err(format!("{agent}'s worktree is being removed"));
        }
        Ok((row.task, row.path))
    }

    /// The agent, other than `asking`, whose live session is `id`: running, or
    /// errored with its window still up.
    fn holder_of(&self, id: &str, asking: &AgentName) -> Option<AgentName> {
        self.agents
            .iter()
            .find(|(name, rt)| {
                *name != asking
                    && rt.cli_session_id.as_deref() == Some(id)
                    && (rt.status.state.is_running()
                        || (rt.status.state == AgentState::Errored && rt.window.is_some()))
            })
            .map(|(name, _)| name.clone())
    }
}

#[cfg(test)]
mod tests;
