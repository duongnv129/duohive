//! A task to a role (TD §5.4; P3-D3, P3-D15, P3-D28). The role's free agent
//! at rest longest takes it; with none free, the engine raises a card for a
//! new agent cloned from the role, whatever Duo's state.

use super::Engine;
use super::lifecycle::GiveRefusal;
use super::org_edit::find_role;
use crate::card::{CardChange, CardId, CardSource};
use crate::command::AgentDraft;
use crate::event::{Event, RoleOutcome};
use crate::inherit::inherited_for;
use crate::names::AgentName;
use crate::org::{NodeKey, Org, RoleKey};
use crate::status::AgentState;
use crate::tool::{ErrorCode, Given, ToolError};
use crate::validate::next_agent_names;
use crate::worktree::RunIn;

/// Who took a task to a role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Assigned {
    /// A free agent took it.
    Given {
        /// Which agent.
        agent: AgentName,
        /// How.
        given: Given,
    },
    /// None was free: a card proposes a new agent (P3-D3).
    Card(CardId),
}

impl Engine {
    /// TD §5.4. `pick` (from Give work's ↑↓) must be free; `run` `None` is the
    /// picked agent's resolved default. The tool and your `AssignToRole` both
    /// come here.
    pub(super) fn assign(
        &mut self,
        role: &RoleKey,
        task: String,
        pick: Option<AgentName>,
        run: Option<RunIn>,
        out: &mut Vec<Event>,
    ) -> Result<Assigned, ToolError> {
        let Some(found) = find_role(&self.org, role) else {
            let path = self.org.breadcrumb(&NodeKey::Role(role.clone()));
            return Err(ToolError::new(
                ErrorCode::UnknownRole,
                format!("no role {path}"),
            ));
        };
        if task.trim().is_empty() {
            return Err(ToolError::new(
                ErrorCode::InvalidArguments,
                "the task is empty",
            ));
        }
        let free = self.free_agents(role);
        let agent = match pick {
            Some(pick) if !found.agents.iter().any(|a| a.name == pick) => {
                let path = self.org.breadcrumb(&NodeKey::Role(role.clone()));
                return Err(ToolError::new(
                    ErrorCode::Refused,
                    format!("{pick} isn't in {path}"),
                ));
            }
            Some(pick) if !free.contains(&pick) => return Err(not_free(&pick)),
            Some(pick) => pick,
            None => match free.into_iter().next() {
                Some(agent) => agent,
                None => {
                    let Some(change) = clone_card(&self.org, role, task) else {
                        return Err(ToolError::new(
                            ErrorCode::Refused,
                            "no name is left for a new agent",
                        ));
                    };
                    let card = self.raise_card(CardSource::NoFreeAgent, change, out);
                    return Ok(Assigned::Card(card));
                }
            },
        };
        let run = run.unwrap_or_else(|| self.default_run(&agent));
        match self.give(&agent, task, run, out) {
            Ok(given) => Ok(Assigned::Given { agent, given }),
            Err(refusal) => Err(give_error(&agent, refusal)),
        }
    }

    /// The role's free agents, at rest longest first; ties keep tree order (P3-D28).
    pub(super) fn free_agents(&self, role: &RoleKey) -> Vec<AgentName> {
        let Some(found) = find_role(&self.org, role) else {
            return Vec::new();
        };
        let mut free: Vec<_> = found
            .agents
            .iter()
            .filter(|a| self.is_free(&a.name))
            .map(|a| {
                let since = self.agents.get(&a.name).map(|rt| rt.status.since);
                (since, a.name.clone())
            })
            .collect();
        // Stable, so equal times keep tree order.
        free.sort_by_key(|(since, _)| *since);
        free.into_iter().map(|(_, name)| name).collect()
    }

    /// Free (TD §5.4): stopped, finished or idle, with no delivery pending
    /// (P2-D11), no worktree being created (P2-D20), and not being stopped.
    fn is_free(&self, agent: &AgentName) -> bool {
        let Some(rt) = self.agents.get(agent) else {
            // No runtime state yet: never launched, so stopped.
            return true;
        };
        matches!(
            rt.status.state,
            AgentState::Stopped | AgentState::Finished | AgentState::Idle
        ) && rt.delivery.is_none()
            && rt.creating.is_none()
            && !rt.stopping
    }

    /// Your `AssignToRole`: who took it, or why not, so the dialog stays
    /// open with the task (P3-D28).
    pub(super) fn assign_yours(
        &mut self,
        role: RoleKey,
        task: String,
        pick: Option<AgentName>,
        run: Option<RunIn>,
    ) -> Vec<Event> {
        let mut out = Vec::new();
        let event = match self.assign(&role, task.clone(), pick, run, &mut out) {
            Ok(Assigned::Given { agent, given }) => Event::RoleAssigned {
                role,
                task,
                outcome: RoleOutcome::Given { agent, given },
            },
            Ok(Assigned::Card(card)) => Event::RoleAssigned {
                role,
                task,
                outcome: RoleOutcome::Card(card),
            },
            Err(e) => Event::AssignRefused {
                role,
                task,
                reason: e.message,
            },
        };
        out.push(event);
        out
    }
}

/// The clone card's change (P3-D15): a new agent named by D11, with the own
/// fields of the role's first agent, holding the task and the clone's
/// resolved worktree default as its Run. `None` when the role is gone or no
/// name is left.
pub(super) fn clone_card(org: &Org, role: &RoleKey, task: String) -> Option<CardChange> {
    let first = find_role(org, role)?.agents.first();
    let name = next_agent_names(org, role.team.as_ref(), &role.role, 1)
        .into_iter()
        .next()?;
    let fields = first.map(|a| a.fields.clone()).unwrap_or_default();
    let worktree = fields.worktree.unwrap_or_else(|| {
        inherited_for(org, &NodeKey::Role(role.clone()))
            .worktree
            .value
    });
    Some(CardChange::NewAgent {
        role: role.clone(),
        agent: AgentDraft { name, fields },
        task,
        run: if worktree {
            RunIn::NewWorktree
        } else {
            RunIn::Directory
        },
        clone_of: first.map(|a| a.name.clone()),
    })
}

/// P3-D28's refusal for a pick that is no longer free.
fn not_free(agent: &AgentName) -> ToolError {
    ToolError::new(ErrorCode::Refused, format!("{agent} isn't free any more"))
}

/// A free agent that still couldn't take the task.
fn give_error(agent: &AgentName, refusal: GiveRefusal) -> ToolError {
    match refusal {
        GiveRefusal::Unknown => {
            ToolError::new(ErrorCode::UnknownAgent, format!("no agent named {agent}"))
        }
        GiveRefusal::EmptyTask => ToolError::new(ErrorCode::InvalidArguments, "the task is empty"),
        GiveRefusal::Busy(_) | GiveRefusal::Pending | GiveRefusal::GettingReady => not_free(agent),
        GiveRefusal::Launch(reason) => ToolError::new(ErrorCode::Refused, reason),
    }
}

#[cfg(test)]
mod tests;
