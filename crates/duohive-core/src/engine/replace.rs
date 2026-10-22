//! Replacing the org from a template (UI §4.8): stop the old tree's agents,
//! then write the new org. The sequence is pending state on the engine so a
//! stop that is still in flight is waited for, and a stop that never finishes
//! gives up at its deadline without writing anything.

use std::collections::BTreeSet;
use std::time::Duration;

use super::lifecycle::getting_ready;
use super::org_edit::ORG_FILE_BROKEN;
use super::{Engine, reject};
use crate::card::CardOutcome;
use crate::command::Command;
use crate::event::Event;
use crate::inbox::MessageStatus;
use crate::names::AgentName;
use crate::org::Org;
use crate::status::AgentState;
use crate::templates::{OrgTree, TemplateInput};
use crate::time::Millis;
use crate::tool::ToolCall;
use crate::validate::validate;

/// Why every command that would change the tree is refused while replacing.
pub(super) const REPLACING: &str = "the org is being replaced";
/// Why a replace is refused until start-up has reconnected.
const RECONNECTING: &str = "still reconnecting · try again";
/// Why a removed agent's queued message is dropped.
const REMOVED: &str = "agent removed";
/// The extra time a stop gets past its grace before the replace gives up.
const STOP_MARGIN: Duration = Duration::from_secs(1);

/// A replace waiting for the old tree's agents to stop.
#[derive(Clone, Debug)]
pub(super) struct PendingReplace {
    /// The org to make current once every stop is in.
    pub(super) org: Org,
    /// Agents the replace still waits on.
    pub(super) waiting: BTreeSet<AgentName>,
    /// When it gives up (the stop grace plus a margin).
    pub(super) deadline: Millis,
}

impl Engine {
    /// Starts a replace (UI §4.8). The new org is built and validated first,
    /// so a bad template never stops anything; then every agent of the old
    /// tree, Duo excepted, is stopped, and the org is written once all are in.
    pub(super) fn start_replace(&mut self, template: &TemplateInput) -> Vec<Event> {
        if self.replace.is_some() {
            return reject(REPLACING.to_owned());
        }
        if self.reconnect.is_some() {
            return reject(RECONNECTING.to_owned());
        }
        if self.org_error.is_some() {
            return reject(ORG_FILE_BROKEN.to_owned());
        }
        if let Some(agent) = self
            .agent_names()
            .into_iter()
            .find(|a| self.creating_worktree(a))
        {
            return reject(getting_ready(&agent));
        }
        let kept: BTreeSet<AgentName> = match self.ports.store.load_worktrees() {
            Ok(rows) => rows.into_iter().map(|row| row.agent).collect(),
            Err(e) => return reject(format!("couldn't read the worktrees: {e}")),
        };
        let org = match build(&self.org, template, &kept) {
            Ok(org) => org,
            Err(reason) => return reject(reason),
        };
        let now = self.ports.clock.now();
        let deadline = now + self.cfg.stop_grace + STOP_MARGIN;
        let stop_set: Vec<AgentName> = self
            .agent_names()
            .into_iter()
            .filter(|a| !self.org.is_duo(a) && self.needs_stop(a))
            .collect();
        let mut out = Vec::new();
        for agent in &stop_set {
            // A restart asked for before the replace must not undo the stop.
            self.rt_mut(agent).restart_after_stop = false;
            out.extend(self.stop_agent(agent));
        }
        let waiting: BTreeSet<AgentName> = stop_set
            .into_iter()
            .filter(|a| self.state_of(a).is_running())
            .collect();
        if waiting.is_empty() {
            self.replace_ready = true;
        }
        self.replace = Some(PendingReplace {
            org,
            waiting,
            deadline,
        });
        out
    }

    /// Whether the replace must stop `agent`: it runs, or it errored leaving a
    /// window that must not outlive the old tree.
    fn needs_stop(&self, agent: &AgentName) -> bool {
        let state = self.state_of(agent);
        state.is_running()
            || (state == AgentState::Errored
                && self.agents.get(agent).is_some_and(|rt| rt.window.is_some()))
    }

    /// Applies a replace whose stops are all in; a no-op without one.
    pub(super) fn apply_replace(&mut self, out: &mut Vec<Event>) {
        let Some(pending) = self.replace.take_if(|p| p.waiting.is_empty()) else {
            return;
        };
        let old = std::mem::replace(&mut self.org, pending.org);
        let now = self.ports.clock.now();
        if let Err(e) = self.ports.store.replace_org(&self.org, now) {
            self.org = old;
            out.push(Event::OrgReplaceFailed {
                agent: None,
                reason: format!("couldn't write org.toml: {e}"),
            });
            return;
        }
        let old_names: BTreeSet<AgentName> = old
            .agents()
            .into_iter()
            .map(|a| a.agent.name.clone())
            .collect();
        let new_names: BTreeSet<AgentName> = self
            .org
            .agents()
            .into_iter()
            .map(|a| a.agent.name.clone())
            .collect();
        let removed: BTreeSet<AgentName> = old_names.difference(&new_names).cloned().collect();
        for agent in &removed {
            self.delete_row(agent, out);
        }
        self.sync_agents();
        self.close_for_removed(&removed, now, out);
        out.push(self.org_changed(&old));
        out.push(Event::OrgReplaced);
    }

    /// Drops what was waiting on agents the new org no longer names: their
    /// queued messages never go in, and every open card is closed as not
    /// applied (the tree it was raised against is gone).
    fn close_for_removed(
        &mut self,
        removed: &BTreeSet<AgentName>,
        now: Millis,
        out: &mut Vec<Event>,
    ) {
        for message in &mut self.inbox {
            if removed.contains(&message.to) && message.status.is_waiting() {
                message.status = MessageStatus::Dropped {
                    reason: REMOVED.to_owned(),
                };
                if let Err(e) = self.ports.store.save_message(message) {
                    out.push(Event::CommandRejected {
                        reason: format!("couldn't save state: {e}"),
                    });
                }
            }
        }
        for card in &mut self.cards {
            if card.answer.is_none() {
                card.answer = Some((CardOutcome::NotApplied(REMOVED.to_owned()), now));
                if let Err(e) = self.ports.store.save_card(card) {
                    out.push(Event::CommandRejected {
                        reason: format!("couldn't save state: {e}"),
                    });
                }
            }
        }
    }
}

/// Whether `c` would change the tree while a replace waits for its stops.
fn blocked_while_replacing(c: &Command) -> bool {
    match c {
        Command::GiveWork { .. }
        | Command::AssignToRole { .. }
        | Command::StartAgent { .. }
        | Command::EditOrg(_)
        | Command::ReloadOrg
        | Command::ApproveCard { .. }
        | Command::NewOrgFromTemplate { .. } => true,

        Command::Tool { call, .. } => matches!(
            call,
            ToolCall::StartAgent { .. }
                | ToolCall::AssignToRole { .. }
                | ToolCall::ProposeOrgChange { .. }
        ),
        _ => false,
    }
}

/// Why a command is refused while replacing, when it is.
pub(super) fn replacing_refusal(c: &Command) -> Option<&'static str> {
    blocked_while_replacing(c).then_some(REPLACING)
}

/// The org `template` builds over `old`, keeping its defaults and Duo.
fn build(old: &Org, template: &TemplateInput, kept: &BTreeSet<AgentName>) -> Result<Org, String> {
    let tree = OrgTree::build(template, kept).map_err(|e| e.to_string())?;
    let org = Org {
        defaults: old.defaults.clone(),
        duo: old.duo.clone(),
        teams: tree.teams,
        roles: tree.roles,
    };
    match validate(&org).into_iter().next() {
        Some(e) => Err(e.to_string()),
        None => Ok(org),
    }
}

#[cfg(test)]
mod tests;
