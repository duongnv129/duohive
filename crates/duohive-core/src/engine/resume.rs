//! Resuming an old session (v1.1; V11-D3, V11-D4; TD §5.6): listing the
//! sessions a stopped agent could pick up, and marking those another agent holds.

use super::lifecycle::{getting_ready, not_found};
use super::signals::is_valid_session_id;
use super::{Engine, reject};
use std::path::PathBuf;

use crate::adapter::CliPresence;
use crate::cli_kind::CliKind;
use crate::command::Command;
use crate::event::Event;
use crate::inherit::{expand_home, resolve};
use crate::launch::ResumeTarget;
use crate::names::AgentName;
use crate::session_catalog::{CatalogError, SessionInfo};
use crate::status::AgentState;
use crate::validate::agent_problem;
use crate::worktree::{TaskId, WorktreeState};

/// What a listing was read for: the agent's CLI and its expanded directory. A
/// same-name edit that changes either makes a listing (or its cache) stale.
pub(super) type ListFingerprint = (CliKind, Option<PathBuf>);

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
            return reject(format!("{agent} is running \u{b7} stop it first"));
        }
        let r = match resolve(&self.org, &agent) {
            Ok(r) => r,
            Err(e) => return reject(e.message),
        };
        if let Some(problem) = agent_problem(&r, &self.cfg.home, None) {
            return reject(problem);
        }
        // Like a start: wait for the first CLI inventory, once per agent.
        if self.cli.is_none() {
            let wait = Command::ListSessions {
                agent: agent.clone(),
            };
            if !self.queued_launches.contains(&wait) {
                self.queued_launches.push(wait);
            }
            return Vec::new();
        }
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
        // One listing per agent at a time: the answer in flight serves a repeat press.
        if self.pending_lists.contains_key(&agent) {
            return Vec::new();
        }
        let fingerprint = (
            cli,
            r.directory
                .as_deref()
                .map(|d| expand_home(d, &self.cfg.home)),
        );
        self.pending_lists.insert(agent.clone(), fingerprint);
        self.ports.sessions.request(&agent, cli, dirs);
        Vec::new()
    }

    /// The fingerprint of `agent` as it is now, if the org resolves it.
    fn fingerprint(&self, agent: &AgentName) -> Option<ListFingerprint> {
        let r = resolve(&self.org, agent).ok()?;
        Some((
            r.cli,
            r.directory
                .as_deref()
                .map(|d| expand_home(d, &self.cfg.home)),
        ))
    }

    /// The catalog's answer: the sessions with `in_use_by` marked, or why not.
    pub(super) fn on_sessions_listed(
        &mut self,
        agent: AgentName,
        result: Result<Vec<SessionInfo>, CatalogError>,
    ) -> Vec<Event> {
        let requested_for = self.pending_lists.remove(&agent);
        match result {
            Err(e) => vec![Event::SessionsUnreadable {
                agent,
                reason: e.to_string(),
            }],
            Ok(_) if requested_for.is_some() && requested_for != self.fingerprint(&agent) => {
                vec![Event::SessionsUnreadable {
                    reason: format!(
                        "{agent} changed while its sessions were being read \u{b7} press r again"
                    ),
                    agent,
                }]
            }
            Ok(mut sessions) => {
                for s in &mut sessions {
                    s.in_use_by = self.holder_of(&s.id, &agent);
                }
                // Nothing resumes an agent the org doesn't have: don't keep its list.
                if let Some(now) = self.fingerprint(&agent).filter(|_| self.knows(&agent)) {
                    self.last_listed
                        .insert(agent.clone(), (now, sessions.clone()));
                }
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
            return reject(format!("{agent} is running \u{b7} stop it first"));
        }
        if self
            .last_listed
            .get(&agent)
            .is_some_and(|(fp, _)| Some(fp) != self.fingerprint(&agent).as_ref())
        {
            // Listed for another CLI or directory: don't resume from it.
            self.last_listed.remove(&agent);
        }
        let Some(session) = self
            .last_listed
            .get(&agent)
            .and_then(|(_, l)| l.iter().find(|s| s.id == session_id))
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
    /// errored with its window still up. Held agents (ruling B) count too.
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
            .or_else(|| self.held_holder_of(id))
    }
}

#[cfg(test)]
mod tests;
