//! Agents' tool calls (TD §6.2). The token is checked first, then the caller's
//! rights, so a refused call has no other effect; each tool is answered by the
//! task that builds it. Calls queued while DuoHive was closed run here too,
//! after `Reconnected`, with no one to answer but with their action lines
//! (P3-D10, P3-D24).

use super::Engine;
use super::assign::Assigned;
use super::rights::Caller;
use super::signals::same_token;
use crate::duo::action_line;
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

/// Reads and waits change nothing, so they are never queued while closed.
fn not_replayed() -> ToolError {
    ToolError::new(ErrorCode::DuohiveClosed, "DuoHive was closed")
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

/// A call waiting for an agent's worktree creation (P3-D23).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Parked {
    /// The call to answer.
    pub call_id: CallId,
    /// The agent whose worktree is being created.
    pub agent: AgentName,
    /// The call, when its answer writes an action line (Duo's, P3-D14).
    pub line: Option<ToolCall>,
}

impl Engine {
    /// Answers one tool call with an `Event::ToolResult`, after its action
    /// line when Duo made it (P3-D14). A read or wait is answered when the
    /// agent's screen or state answers it, and a call whose work waits for a
    /// worktree when git answers (P3-D23).
    pub(super) fn on_tool(
        &mut self,
        call_id: CallId,
        token: &Token,
        call: &ToolCall,
    ) -> Vec<Event> {
        let mut out = Vec::new();
        let caller = match self.caller(token) {
            Ok(caller) => caller,
            Err(e) => {
                out.push(Event::ToolResult {
                    call_id,
                    result: Err(e),
                });
                return out;
            }
        };
        let line = (caller.rights == Rights::Duo).then_some(call);
        let Some(result) = self.run_call(&caller, call, Some((call_id, token)), &mut out) else {
            return out;
        };
        self.answer_or_park(call_id, line, result, &mut out);
        out
    }

    /// Publishes `result`, or holds the call while its worktree is created (P3-D23).
    fn answer_or_park(
        &mut self,
        call_id: CallId,
        line: Option<&ToolCall>,
        result: ToolResult,
        out: &mut Vec<Event>,
    ) {
        match result {
            Ok(ToolOk::Given {
                agent,
                given: Given::CreatingWorktree,
            }) => self.park_until_created(call_id, agent, line.cloned()),
            result => self.publish_result(call_id, line, result, out),
        }
    }

    /// Publishes `result` for `call_id`, after `line`'s action line (P3-D14);
    /// a replayed call's result goes nowhere (P3-D24).
    pub(super) fn publish_result(
        &mut self,
        call_id: CallId,
        line: Option<&ToolCall>,
        result: ToolResult,
        out: &mut Vec<Event>,
    ) {
        if let Some((text, word)) = line.and_then(|call| action_line(call, &result)) {
            self.chat_action(text, word, out);
        }
        if call_id != CallId::REPLAY {
            out.push(Event::ToolResult { call_id, result });
        }
    }

    /// Holds `call` until `agent`'s worktree creation ends (P3-D23). A
    /// replayed call is held only for its action line. A held call is
    /// answered even when its caller has since stopped: the work it asked
    /// for went ahead, and its action line records it.
    pub(super) fn park_until_created(
        &mut self,
        call: CallId,
        agent: AgentName,
        line: Option<ToolCall>,
    ) {
        if call != CallId::REPLAY || line.is_some() {
            self.parked.push(Parked {
                call_id: call,
                agent,
                line,
            });
        }
    }

    /// Answers every call held for `agent`'s worktree creation with `result`.
    pub(super) fn answer_parked(
        &mut self,
        agent: &AgentName,
        result: &ToolResult,
        out: &mut Vec<Event>,
    ) {
        let (due, keep): (Vec<Parked>, Vec<Parked>) = std::mem::take(&mut self.parked)
            .into_iter()
            .partition(|p| &p.agent == agent);
        self.parked = keep;
        for p in due {
            self.publish_result(p.call_id, p.line.as_ref(), result.clone(), out);
        }
    }

    /// Runs `call` for `caller`, once its rights allow the tool. `asked` is
    /// the call and token to answer later; `None` for a replayed call.
    /// Returns `None` when a read or wait will answer later.
    fn run_call(
        &mut self,
        caller: &Caller,
        call: &ToolCall,
        asked: Option<(CallId, &Token)>,
        out: &mut Vec<Event>,
    ) -> Option<ToolResult> {
        let Some(tool) = tool_name(call) else {
            return Some(Ok(ToolOk::Rights(caller.rights)));
        };
        if let Err(e) = Self::allowed(caller, tool) {
            return Some(Err(e));
        }
        Some(match call {
            ToolCall::Rights => Ok(ToolOk::Rights(caller.rights)),
            ToolCall::ListAgents => Ok(self.list_agents(&caller.agent)),
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
            ToolCall::ReadAgent { agent } => match asked {
                Some((id, token)) => return self.read_agent(id, token, agent),
                None => Err(not_replayed()),
            },
            ToolCall::WaitForAgent {
                agent,
                timeout_minutes,
            } => match asked {
                Some((id, token)) => {
                    return self.wait_for_agent(id, token, agent, *timeout_minutes);
                }
                None => Err(not_replayed()),
            },
            ToolCall::StartAgent { agent, task, run } => {
                self.start_by_tool(agent, task.clone(), *run, out)
            }
            ToolCall::StopAgent { agent } => self.stop_by_tool(agent, out),
            ToolCall::ProposeOrgChange { change } => self.propose_org_change(change, out),
            ToolCall::AssignToRole { role, task } => self
                .assign(role, task.clone(), None, None, out)
                .map(|assigned| match assigned {
                    Assigned::Given { agent, given } => ToolOk::Given { agent, given },
                    Assigned::Card(card) => ToolOk::Pending { card },
                }),
        })
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

    /// After `Reconnected`: the calls queued while closed, in order, each
    /// with its action line when Duo made it (P3-D24). Their results go
    /// nowhere (`CallId::REPLAY` is never published); a refusal is logged.
    pub(super) fn run_closed_calls(&mut self, out: &mut Vec<Event>) {
        for (caller, call) in std::mem::take(&mut self.closed_calls) {
            let Some(result) = self.run_call(&caller, &call, None, out) else {
                continue;
            };
            if let Err(e) = &result {
                tracing::info!(agent = %caller.agent, ?call, error = %e.message, "queued tool call refused");
            }
            let line = (caller.rights == Rights::Duo).then_some(&call);
            self.answer_or_park(CallId::REPLAY, line, result, out);
        }
    }
}
