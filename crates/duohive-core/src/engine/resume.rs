//! Resuming an old session (v1.1; V11-D3, V11-D4; TD §5.6): listing the
//! sessions a stopped agent could pick up, and marking those another agent holds.

use super::lifecycle::{getting_ready, not_found};
use super::{Engine, reject};
use crate::adapter::CliPresence;
use crate::event::Event;
use crate::inherit::{expand_home, resolve};
use crate::names::AgentName;
use crate::session_catalog::{CatalogError, SessionInfo};
use crate::status::AgentState;

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
                vec![Event::SessionsListed { agent, sessions }]
            }
        }
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
