//! Agents' tool calls (TD §6.2). Each tool is answered by the task that builds it.

use crate::event::Event;
use crate::tool::{CallId, ErrorCode, ToolCall, ToolError};

/// Answers a call whose tool isn't built yet.
pub(super) fn answer(call_id: CallId, _call: &ToolCall) -> Vec<Event> {
    vec![Event::ToolResult {
        call_id,
        result: Err(ToolError::new(ErrorCode::Unavailable, "not built yet")),
    }]
}
