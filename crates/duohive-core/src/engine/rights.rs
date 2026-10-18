//! Who is calling a tool, and what they may call (TD §6.1, §6.2).

use super::Engine;
use super::signals::same_token;
use crate::launch::Token;
use crate::names::AgentName;
use crate::status::AgentState;
use crate::tool::{ErrorCode, Rights, ToolError, ToolName};

/// A tool call's caller: the agent whose current launch holds the token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Caller {
    /// The agent.
    pub agent: AgentName,
    /// What it may call.
    pub rights: Rights,
}

impl Engine {
    /// The agent whose current launch holds `token`; a stopped agent holds none.
    /// Identity never comes from the call's arguments (P3-D17).
    pub(super) fn caller(&self, token: &Token) -> Result<Caller, ToolError> {
        let mut found = None;
        // Every agent is compared, so the time taken says nothing about which matched.
        for (agent, rt) in &self.agents {
            let holds = rt
                .token
                .as_ref()
                .is_some_and(|t| same_token(&t.0, &token.0));
            if holds && rt.status.state != AgentState::Stopped {
                found = Some(Caller {
                    agent: agent.clone(),
                    rights: if rt.launched_as_duo {
                        Rights::Duo
                    } else {
                        Rights::Agent
                    },
                });
            }
        }
        found.ok_or_else(|| {
            ToolError::new(ErrorCode::UnknownToken, "no running agent has this token")
        })
    }

    /// Refuses a tool the caller's rights don't include.
    pub(super) fn allowed(caller: &Caller, tool: ToolName) -> Result<(), ToolError> {
        if tool.duo_only() && caller.rights != Rights::Duo {
            return Err(ToolError::new(
                ErrorCode::NotAllowed,
                format!("{} is Duo's tool", tool.as_str()),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
