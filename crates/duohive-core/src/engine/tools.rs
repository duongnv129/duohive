//! Agents' tool calls (TD §6.2). The token is checked first, then the caller's
//! rights, so a refused call has no other effect; each tool is answered by the
//! task that builds it.

use super::Engine;
use crate::event::Event;
use crate::launch::Token;
use crate::tool::{CallId, ErrorCode, ToolCall, ToolError, ToolName, ToolOk, ToolResult};

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
    /// Answers one tool call with an `Event::ToolResult`.
    pub(super) fn on_tool(
        &mut self,
        call_id: CallId,
        token: &Token,
        call: &ToolCall,
    ) -> Vec<Event> {
        let mut out = Vec::new();
        let result = self.tool_result(token, call, &mut out);
        out.push(Event::ToolResult { call_id, result });
        out
    }

    fn tool_result(&mut self, token: &Token, call: &ToolCall, _out: &mut Vec<Event>) -> ToolResult {
        let caller = self.caller(token)?;
        let Some(tool) = tool_name(call) else {
            return Ok(ToolOk::Rights(caller.rights));
        };
        Self::allowed(&caller, tool)?;
        Err(ToolError::new(ErrorCode::Unavailable, "not built yet"))
    }
}
