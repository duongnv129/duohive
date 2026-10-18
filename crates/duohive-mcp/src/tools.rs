//! What `tools/list` shows (P3-D27) and how results look (P3-D16).

use duohive_core::inbox::MessageStatus;
use duohive_core::names::NodeName;
use duohive_core::status::AgentState;
use duohive_core::tool::{
    AgentEntry, ErrorCode, Given, LimitKind, Rights, SendOutcome, ToolName, ToolOk, ToolResult,
    WAIT_DEFAULT_MINUTES, WAIT_MAX_MINUTES, WaitOutcome, tools_for,
};
use serde_json::{Value, json};

use crate::protocol::SockAnswer;

/// One tool as `tools/list` shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSpec {
    /// The MCP name.
    pub name: &'static str,
    /// What the model reads.
    pub description: &'static str,
    /// The arguments' JSON schema.
    pub input_schema: Value,
}

/// The tools a token with `rights` sees.
pub fn specs(rights: Rights) -> Vec<ToolSpec> {
    tools_for(rights).into_iter().map(spec).collect()
}

fn object(properties: &Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required })
}

fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn spec(tool: ToolName) -> ToolSpec {
    let (description, input_schema) = match tool {
        ToolName::ListAgents => (
            "Every agent in the org with its state, last reply and waiting messages.",
            object(&json!({}), &[]),
        ),
        ToolName::SendMessage => (
            "Send a message to an agent. It is typed in when the agent is at rest.",
            object(
                &json!({ "to": string("the agent's name"), "text": string("the message") }),
                &["to", "text"],
            ),
        ),
        ToolName::MessageStatus => (
            "Whether a message you sent is queued, delivered, answered or dropped.",
            object(
                &json!({ "message_id": { "type": "integer", "minimum": 1 } }),
                &["message_id"],
            ),
        ),
        ToolName::ReadAgent => (
            "An agent's screen as text.",
            object(&json!({ "agent": string("the agent's name") }), &["agent"]),
        ),
        ToolName::WaitForAgent => (
            "Wait until an agent's turn ends, it stops, or the timeout passes.",
            object(
                &json!({
                    "agent": string("the agent's name"),
                    "timeout_minutes": {
                        "type": "integer", "minimum": 1, "maximum": WAIT_MAX_MINUTES,
                        "default": WAIT_DEFAULT_MINUTES,
                    },
                }),
                &["agent"],
            ),
        ),
        ToolName::StartAgent => (
            "Start a stopped agent with a task, or give a task to one at rest.",
            object(
                &json!({
                    "agent": string("the agent's name"),
                    "task": string("the task"),
                    "run": { "type": "string", "enum": ["directory", "worktree"] },
                }),
                &["agent", "task"],
            ),
        ),
        ToolName::StopAgent => (
            "Stop an agent. A busy or starting agent needs the user's approval: \
             the call returns pending, and the answer arrives later as a message.",
            object(&json!({ "agent": string("the agent's name") }), &["agent"]),
        ),
        ToolName::AssignToRole => (
            "Give a task to a role; a free agent takes it, or a card proposes a new one.",
            object(
                &json!({
                    "team": { "type": ["string", "null"] },
                    "role": string("the role's name"),
                    "task": string("the task"),
                }),
                &["role", "task"],
            ),
        ),
        ToolName::ProposeOrgChange => (
            "Propose a change to the org. It applies only after the user approves it: \
             the call returns pending, and the answer arrives later as a message.",
            object(&json!({ "change": change_schema() }), &["change"]),
        ),
    };
    ToolSpec {
        name: tool.as_str(),
        description,
        input_schema,
    }
}

/// `change`: a `oneOf` on `op` (P3-D27).
fn change_schema() -> Value {
    let fields = json!({
        "type": "object",
        "properties": {
            "cli": { "type": "string", "enum": ["claude", "opencode"] },
            "flags": { "type": "string" },
            "directory": { "type": "string" },
            "model": { "type": "string" },
            "effort": { "type": "string" },
            "worktree": { "type": "boolean" },
            "prompt": { "type": "string" },
        },
    });
    let team = json!({ "type": ["string", "null"] });
    let role = object(
        &json!({ "team": team, "role": { "type": "string" } }),
        &["role"],
    );
    let node = json!({ "type": "object", "description":
        "{\"agent\": name}, {\"team\": name}, {\"role\": name, \"team\": name or null}" });
    let target = json!({ "description":
        "a node, {\"duo\": true} (the root), {\"before_team\": name} or \"end\"" });
    let op = |name: &str, properties: Value, required: &[&str]| {
        let mut props = properties;
        if let Some(map) = props.as_object_mut() {
            map.insert("op".to_owned(), json!({ "const": name }));
        }
        let mut req = vec!["op"];
        req.extend_from_slice(required);
        object(&props, &req)
    };
    json!({ "oneOf": [
        op("add_team", json!({ "name": { "type": "string" }, "fields": fields }), &["name"]),
        op("add_role", json!({
            "team": team, "name": { "type": "string" }, "fields": fields,
            "agents": { "type": "integer", "minimum": 0, "maximum": 9 },
        }), &["name"]),
        op("add_agent", json!({ "role": role, "name": { "type": "string" }, "fields": fields }),
            &["role", "name"]),
        op("edit_team", json!({ "team": { "type": "string" }, "name": { "type": "string" },
            "fields": fields }), &["team", "name"]),
        op("edit_role", json!({ "role": role, "name": { "type": "string" }, "fields": fields }),
            &["role", "name"]),
        op("edit_agent", json!({ "agent": { "type": "string" }, "name": { "type": "string" },
            "fields": fields }), &["agent", "name"]),
        op("move", json!({ "node": node, "to": target }), &["node", "to"]),
        op("remove", json!({ "node": node }), &["node"]),
    ]})
}

/// A state as one lowercase word.
fn state_word(s: AgentState) -> &'static str {
    match s {
        AgentState::Stopped => "stopped",
        AgentState::Starting => "starting",
        AgentState::Working => "working",
        AgentState::Waiting => "waiting",
        AgentState::Finished => "finished",
        AgentState::Idle => "idle",
        AgentState::Errored => "errored",
    }
}

fn entry(a: &AgentEntry) -> Value {
    json!({
        "name": a.name.as_str(),
        "team": a.team.as_ref().map(NodeName::as_str),
        "role": a.role.as_ref().map(NodeName::as_str),
        "state": state_word(a.state),
        "quiet": a.quiet,
        "from_screen": a.from_screen,
        "for_secs": a.for_secs,
        "last_reply": a.last_reply,
        "waiting_messages": a.waiting_messages,
        "cli": a.cli.binary(),
        "worktree": a.worktree.as_ref().map(|t| t.0.as_str()),
        "free": a.free,
    })
}

fn status_word(s: &MessageStatus) -> &'static str {
    match s {
        MessageStatus::Queued | MessageStatus::Pasting => "queued",
        MessageStatus::Delivered => "delivered",
        MessageStatus::Answered => "answered",
        MessageStatus::Dropped { .. } => "dropped",
    }
}

fn outcome_word(o: WaitOutcome) -> &'static str {
    match o {
        WaitOutcome::AtRest => "at_rest",
        WaitOutcome::TurnEnded => "turn_ended",
        WaitOutcome::TimedOut => "timed_out",
        WaitOutcome::Stopped => "stopped",
        WaitOutcome::Errored => "errored",
    }
}

fn code_word(c: ErrorCode) -> &'static str {
    match c {
        ErrorCode::UnknownToken => "unknown_token",
        ErrorCode::NotAllowed => "not_allowed",
        ErrorCode::DuohiveClosed => "duohive_closed",
        ErrorCode::Interrupted => "interrupted",
        ErrorCode::InvalidArguments => "invalid_arguments",
        ErrorCode::UnknownAgent => "unknown_agent",
        ErrorCode::UnknownRole => "unknown_role",
        ErrorCode::UnknownMessage => "unknown_message",
        ErrorCode::Refused => "refused",
        ErrorCode::Limit(_) => "limit",
        ErrorCode::InvalidChange => "invalid_change",
        ErrorCode::NoScreen => "no_screen",
        ErrorCode::Unavailable => "unavailable",
    }
}

fn limit_word(l: LimitKind) -> &'static str {
    match l {
        LimitKind::Duplicate => "duplicate",
        LimitKind::Rate => "rate",
        LimitKind::InboxFull => "inbox_full",
    }
}

/// A result as the P3-D16 JSON.
pub fn render(result: &ToolResult) -> SockAnswer {
    let ok = match result {
        Err(e) => {
            return SockAnswer::Error {
                code: code_word(e.code).to_owned(),
                message: e.message.clone(),
                limit: match e.code {
                    ErrorCode::Limit(l) => Some(limit_word(l).to_owned()),
                    _ => None,
                },
            };
        }
        Ok(ok) => ok,
    };
    SockAnswer::Ok(match ok {
        ToolOk::Rights(r) => {
            json!({ "rights": match r { Rights::Agent => "agent", Rights::Duo => "duo" } })
        }
        ToolOk::Agents { you, agents } => json!({
            "you": you.as_str(),
            "agents": agents.iter().map(entry).collect::<Vec<_>>(),
        }),
        ToolOk::Queued { message, outcome } => match outcome {
            SendOutcome::TypedIn => json!({ "message_id": message.0, "status": "typed_in" }),
            SendOutcome::Started => json!({ "message_id": message.0, "status": "started" }),
            SendOutcome::Queued(state) => json!({
                "message_id": message.0, "status": "queued",
                "recipient_state": state_word(*state),
            }),
        },
        ToolOk::Status {
            message,
            to,
            status,
            reason,
        } => json!({
            "message_id": message.0, "to": to.as_str(),
            "status": status_word(status), "reason": reason,
        }),
        ToolOk::Screen { agent, state, text } => json!({
            "agent": agent.as_str(), "state": state_word(*state), "screen": text,
        }),
        ToolOk::Waited {
            agent,
            outcome,
            state,
            last_reply,
        } => json!({
            "agent": agent.as_str(), "outcome": outcome_word(*outcome),
            "state": state_word(*state), "last_reply": last_reply,
        }),
        ToolOk::Given { agent, given } => {
            let (result, worktree) = match given {
                Given::Started { worktree } => ("started", *worktree),
                Given::TypedIn => ("typed_in", false),
                // A tool answer waits for git (P3-D23); this is only a fallback.
                Given::CreatingWorktree => ("started", true),
            };
            json!({ "agent": agent.as_str(), "result": result, "worktree": worktree })
        }
        ToolOk::Stopped { agent } => json!({ "agent": agent.as_str(), "result": "stopped" }),
        ToolOk::Pending { card } => json!({ "status": "pending", "card_id": card.0 }),
    })
}
