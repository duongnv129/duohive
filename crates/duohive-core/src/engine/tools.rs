//! Agents' tool calls (TD §6.2). The token is checked first, then the caller's
//! rights, so a refused call has no other effect; each tool is answered by the
//! task that builds it. Calls queued while DuoHive was closed run here too,
//! after `Reconnected`, with no one to answer (P3-D10, P3-D24).

use super::Engine;
use super::assign::Assigned;
use super::rights::Caller;
use super::signals::same_token;
use crate::event::Event;
use crate::inbox::{MessageKind, Sender};
use crate::launch::Token;
use crate::names::AgentName;
use crate::ports::SessionRow;
use crate::tool::{
    CallId, ErrorCode, Given, Rights, ToolCall, ToolError, ToolName, ToolOk, ToolResult, parse_call,
};

/// The caller whose session row holds `token`, with that session's rights (P3-D30).
fn row_caller(rows: &[SessionRow], token: &Token) -> Option<Caller> {
    let mut found = None;
    // Every row is compared, so the time taken says nothing about which matched.
    for row in rows {
        let holds = row
            .token
            .as_ref()
            .is_some_and(|t| same_token(&t.0, &token.0));
        if holds {
            found = Some(Caller {
                agent: row.agent.clone(),
                rights: if row.launched_as_duo {
                    Rights::Duo
                } else {
                    Rights::Agent
                },
            });
        }
    }
    found
}

/// The tool a call is for; `None` for `Rights`, which isn't a tool.
fn tool_name(call: &ToolCall) -> Option<ToolName> {
    Some(match call {
        ToolCall::Rights => return None,
        ToolCall::ListAgents => ToolName::ListAgents,
        ToolCall::SendMessage { .. } => ToolName::SendMessage,
        ToolCall::MessageStatus { .. } => ToolName::MessageStatus,
        ToolCall::ReadAgent { .. } => ToolName::ReadAgent,
        ToolCall::WaitForAgent { .. } => ToolName::WaitForAgent,
        ToolCall::StartAgent { .. } => ToolName::StartAgent,
        ToolCall::StopAgent { .. } => ToolName::StopAgent,
        ToolCall::AssignToRole { .. } => ToolName::AssignToRole,
        ToolCall::ProposeOrgChange { .. } => ToolName::ProposeOrgChange,
    })
}

impl Engine {
    /// Answers one tool call with an `Event::ToolResult`; a call whose work
    /// waits for a worktree is answered when git answers (P3-D23).
    pub(super) fn on_tool(
        &mut self,
        call_id: CallId,
        token: &Token,
        call: &ToolCall,
    ) -> Vec<Event> {
        let mut out = Vec::new();
        let result = self
            .caller(token)
            .and_then(|caller| self.run_call(&caller, call, &mut out));
        match result {
            Ok(ToolOk::Given {
                agent,
                given: Given::CreatingWorktree,
            }) => self.park_until_created(call_id, agent),
            result => out.push(Event::ToolResult { call_id, result }),
        }
        out
    }

    /// Holds `call` until `agent`'s worktree creation ends (P3-D23). A
    /// replayed call has no one to answer, so it isn't held.
    pub(super) fn park_until_created(&mut self, call: CallId, agent: AgentName) {
        if call != CallId::REPLAY {
            self.parked.push((call, agent));
        }
    }

    /// Answers every call held for `agent`'s worktree creation with `result`.
    pub(super) fn answer_parked(
        &mut self,
        agent: &AgentName,
        result: &ToolResult,
        out: &mut Vec<Event>,
    ) {
        let (due, keep) = std::mem::take(&mut self.parked)
            .into_iter()
            .partition(|(_, a)| a == agent);
        self.parked = keep;
        for (call_id, _) in due {
            out.push(Event::ToolResult {
                call_id,
                result: result.clone(),
            });
        }
    }

    /// Runs `call` for `caller`, once its rights allow the tool.
    fn run_call(&mut self, caller: &Caller, call: &ToolCall, out: &mut Vec<Event>) -> ToolResult {
        let Some(tool) = tool_name(call) else {
            return Ok(ToolOk::Rights(caller.rights));
        };
        Self::allowed(caller, tool)?;
        match call {
            ToolCall::SendMessage { to, text } => self
                .send(
                    Sender::Agent(caller.agent.clone()),
                    to,
                    MessageKind::Text,
                    text.clone(),
                    out,
                )
                .map(|(message, outcome)| ToolOk::Queued { message, outcome }),
            ToolCall::MessageStatus { id } => self.message_status(&caller.agent, *id),
            ToolCall::ProposeOrgChange { change } => self.propose_org_change(change, out),
            ToolCall::AssignToRole { role, task } => self
                .assign(role, task.clone(), None, None, out)
                .map(|assigned| match assigned {
                    Assigned::Given { agent, given } => ToolOk::Given { agent, given },
                    Assigned::Card(card) => ToolOk::Pending { card },
                }),
            _ => Err(ToolError::new(ErrorCode::Unavailable, "not built yet")),
        }
    }

    /// Start-up: takes the calls queued while DuoHive was closed and finds each
    /// one's caller among the session rows just loaded (P3-D24). An unknown
    /// token or arguments that don't parse drop the call with a log line.
    pub(super) fn take_closed_calls(&mut self, rows: &[SessionRow], out: &mut Vec<Event>) {
        let calls = match self.ports.store.take_queued_tool_calls() {
            Ok(calls) => calls,
            Err(e) => {
                out.push(Event::CommandRejected {
                    reason: format!("couldn't read queued tool calls: {e}"),
                });
                return;
            }
        };
        for queued in calls {
            let Some(caller) = row_caller(rows, &queued.token) else {
                tracing::warn!(request = queued.id, tool = %queued.tool, "queued tool call dropped: no session holds its token");
                continue;
            };
            match parse_call(&queued.tool, &queued.arguments) {
                Ok(call) => self.closed_calls.push((caller, call)),
                Err(e) => tracing::warn!(
                    request = queued.id, tool = %queued.tool, agent = %caller.agent, error = %e.message,
                    "queued tool call dropped: bad arguments"
                ),
            }
        }
    }

    /// After `Reconnected`: the calls queued while closed, in order. Their
    /// results go nowhere (`CallId::REPLAY` is never published); a refusal is logged.
    pub(super) fn run_closed_calls(&mut self, out: &mut Vec<Event>) {
        for (caller, call) in std::mem::take(&mut self.closed_calls) {
            if let Err(e) = self.run_call(&caller, &call, out) {
                tracing::info!(agent = %caller.agent, ?call, error = %e.message, "queued tool call refused");
            }
        }
    }
}
