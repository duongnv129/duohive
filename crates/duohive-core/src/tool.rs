//! Agents' tools (TD §6.2): names, rights, arguments and results.
//!
//! The helper relays a call as text; DuoHive parses it here (P3-D17). Identity
//! never comes from the arguments: fields the parser doesn't know are ignored.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::card::CardId;
use crate::cli_kind::CliKind;
use crate::command::{AgentDraft, MoveTarget, OrgEdit, RoleDraft, TeamDraft};
use crate::inbox::{MessageId, MessageStatus};
use crate::names::{AgentName, NodeName};
use crate::org::{Fields, NodeKey, RoleKey};
use crate::status::AgentState;
use crate::worktree::{RunIn, TaskId};

/// Names one tool call; counts up from 1 per run and is never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CallId(pub u64);

impl CallId {
    /// A call queued while DuoHive was closed (P3-D24): never published, never resolved.
    pub const REPLAY: CallId = CallId(0);
}

/// What a token may call (TD §6.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rights {
    /// Every agent's tools.
    Agent,
    /// Duo's tools too.
    Duo,
}

/// The nine tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolName {
    /// `list_agents`.
    ListAgents,
    /// `send_message`.
    SendMessage,
    /// `message_status`.
    MessageStatus,
    /// `read_agent`.
    ReadAgent,
    /// `wait_for_agent`.
    WaitForAgent,
    /// `start_agent`.
    StartAgent,
    /// `stop_agent`.
    StopAgent,
    /// `assign_to_role`.
    AssignToRole,
    /// `propose_org_change`.
    ProposeOrgChange,
}

impl ToolName {
    /// Every tool, in the order `tools/list` shows them.
    pub const ALL: [ToolName; 9] = [
        ToolName::ListAgents,
        ToolName::SendMessage,
        ToolName::MessageStatus,
        ToolName::ReadAgent,
        ToolName::WaitForAgent,
        ToolName::StartAgent,
        ToolName::StopAgent,
        ToolName::AssignToRole,
        ToolName::ProposeOrgChange,
    ];

    /// The MCP name.
    pub fn as_str(self) -> &'static str {
        match self {
            ToolName::ListAgents => "list_agents",
            ToolName::SendMessage => "send_message",
            ToolName::MessageStatus => "message_status",
            ToolName::ReadAgent => "read_agent",
            ToolName::WaitForAgent => "wait_for_agent",
            ToolName::StartAgent => "start_agent",
            ToolName::StopAgent => "stop_agent",
            ToolName::AssignToRole => "assign_to_role",
            ToolName::ProposeOrgChange => "propose_org_change",
        }
    }

    /// The tool with this MCP name.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s)
    }

    /// Only Duo may call it.
    pub fn duo_only(self) -> bool {
        !matches!(
            self,
            ToolName::ListAgents | ToolName::SendMessage | ToolName::MessageStatus
        )
    }

    /// It changes something, so it waits in the database while DuoHive is closed (P3-D10).
    pub fn changes(self) -> bool {
        matches!(
            self,
            ToolName::SendMessage
                | ToolName::StartAgent
                | ToolName::StopAgent
                | ToolName::AssignToRole
                | ToolName::ProposeOrgChange
        )
    }
}

/// The tools a token with these rights sees.
pub fn tools_for(rights: Rights) -> Vec<ToolName> {
    ToolName::ALL
        .into_iter()
        .filter(|t| rights == Rights::Duo || !t.duo_only())
        .collect()
}

/// The longest `text` or `task`, in characters (P3-D17).
pub const MAX_TEXT: usize = 16_000;
/// `wait_for_agent`'s timeout when none is given (P3-D11).
pub const WAIT_DEFAULT_MINUTES: u32 = 10;
/// `wait_for_agent`'s longest timeout; more is refused, not clamped (P3-D11).
pub const WAIT_MAX_MINUTES: u32 = 60;
/// The most agents `add_role` creates (D11).
const MAX_ROLE_AGENTS: u64 = 9;

/// One parsed tool call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolCall {
    /// What `tools/list` shows; not a tool, no action line.
    Rights,
    /// `list_agents`.
    ListAgents,
    /// `send_message`.
    SendMessage {
        /// The recipient.
        to: AgentName,
        /// The text.
        text: String,
    },
    /// `message_status`.
    MessageStatus {
        /// Which message.
        id: MessageId,
    },
    /// `read_agent`.
    ReadAgent {
        /// Which agent.
        agent: AgentName,
    },
    /// `wait_for_agent`.
    WaitForAgent {
        /// Which agent.
        agent: AgentName,
        /// How long to wait at most.
        timeout_minutes: u32,
    },
    /// `start_agent`.
    StartAgent {
        /// Which agent.
        agent: AgentName,
        /// The task it starts with.
        task: String,
        /// Where, or `None` for the agent's resolved default.
        run: Option<RunIn>,
    },
    /// `stop_agent`.
    StopAgent {
        /// Which agent.
        agent: AgentName,
    },
    /// `assign_to_role`.
    AssignToRole {
        /// The role.
        role: RoleKey,
        /// The task.
        task: String,
    },
    /// `propose_org_change`.
    ProposeOrgChange {
        /// The change (boxed: it is the largest call).
        change: Box<OrgEdit>,
    },
}

/// One agent as `list_agents` shows it (P3-D16).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEntry {
    /// Its name.
    pub name: AgentName,
    /// Its team; `None` for Duo and root roles.
    pub team: Option<NodeName>,
    /// Its role; `None` for Duo.
    pub role: Option<NodeName>,
    /// Its state.
    pub state: AgentState,
    /// Quiet overlay.
    pub quiet: bool,
    /// The state was read from the screen.
    pub from_screen: bool,
    /// Seconds in this state.
    pub for_secs: u64,
    /// Its last reply.
    pub last_reply: Option<String>,
    /// Messages waiting for it.
    pub waiting_messages: u32,
    /// Its CLI.
    pub cli: CliKind,
    /// The worktree it runs in.
    pub worktree: Option<TaskId>,
    /// Free for a task to a role.
    pub free: bool,
}

/// How work reached an agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Given {
    /// It was launched with the task.
    Started {
        /// In a new worktree.
        worktree: bool,
    },
    /// The task was typed in as its next turn.
    TypedIn,
    /// The launch waits for git; a tool answer waits for it (P3-D23), never shows it.
    CreatingWorktree,
}

/// What became of a `send_message` (P3-D16).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SendOutcome {
    /// The recipient was at rest and the text went in.
    TypedIn,
    /// It waits; carries the recipient's state.
    Queued(AgentState),
    /// Your message started a stopped or errored agent.
    Started,
}

/// How a `wait_for_agent` ended (P3-D23).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaitOutcome {
    /// The agent was already at rest.
    AtRest,
    /// Its turn ended.
    TurnEnded,
    /// The timeout passed.
    TimedOut,
    /// It stopped.
    Stopped,
    /// It errored.
    Errored,
}

/// A successful tool result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolOk {
    /// The caller's rights, for `tools/list`.
    Rights(Rights),
    /// `list_agents`.
    Agents {
        /// The caller.
        you: AgentName,
        /// Every agent, Duo first.
        agents: Vec<AgentEntry>,
    },
    /// `send_message`.
    Queued {
        /// The message.
        message: MessageId,
        /// What became of it.
        outcome: SendOutcome,
    },
    /// `message_status`.
    Status {
        /// The message.
        message: MessageId,
        /// Its recipient.
        to: AgentName,
        /// Where it stands.
        status: MessageStatus,
        /// What it waits on, or why it was dropped (P3-D16).
        reason: Option<String>,
    },
    /// `read_agent`.
    Screen {
        /// The agent.
        agent: AgentName,
        /// Its state.
        state: AgentState,
        /// Its screen as text.
        text: String,
    },
    /// `wait_for_agent`.
    Waited {
        /// The agent.
        agent: AgentName,
        /// How the wait ended.
        outcome: WaitOutcome,
        /// Its state then.
        state: AgentState,
        /// Its last reply then.
        last_reply: Option<String>,
    },
    /// `start_agent` and `assign_to_role`.
    Given {
        /// The agent that took the work.
        agent: AgentName,
        /// How.
        given: Given,
    },
    /// `stop_agent`, stopped directly.
    Stopped {
        /// The agent.
        agent: AgentName,
    },
    /// A card was raised; the answer comes later as a message (P3-D2).
    Pending {
        /// The card.
        card: CardId,
    },
}

/// Which limit refused a message (TD §6.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LimitKind {
    /// The same text to the same agent within 10 minutes.
    Duplicate,
    /// More than 5 a minute from one sender to one recipient.
    Rate,
    /// 20 already waiting for the recipient.
    InboxFull,
}

/// A tool error's code (P3-D16).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    /// No running launch has this token.
    UnknownToken,
    /// The caller's rights don't include the tool.
    NotAllowed,
    /// DuoHive is closed (reads only, P3-D10).
    DuohiveClosed,
    /// DuoHive closed during the call (P3-D24).
    Interrupted,
    /// The arguments are missing or ill-typed.
    InvalidArguments,
    /// No agent has that name.
    UnknownAgent,
    /// No role has that name.
    UnknownRole,
    /// No message of the caller's has that id.
    UnknownMessage,
    /// Refused by state or an org rule.
    Refused,
    /// A message limit.
    Limit(LimitKind),
    /// The proposed change is not valid.
    InvalidChange,
    /// The agent has no screen to read.
    NoScreen,
    /// Not built yet.
    Unavailable,
}

/// A tool error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolError {
    /// Its code.
    pub code: ErrorCode,
    /// What to tell the model.
    pub message: String,
}

impl ToolError {
    /// An error with this code and message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// A tool call's result.
pub type ToolResult = Result<ToolOk, ToolError>;

/// Parses one call's arguments (P3-D17, P3-D27).
pub fn parse_call(tool: &str, args: &Value) -> Result<ToolCall, ToolError> {
    let Some(name) = ToolName::parse(tool) else {
        return Err(invalid(format!("unknown tool {tool}")));
    };
    let empty = Map::new();
    let a = Args(args.as_object().unwrap_or(&empty));
    Ok(match name {
        ToolName::ListAgents => ToolCall::ListAgents,
        ToolName::SendMessage => ToolCall::SendMessage {
            to: a.agent("to")?,
            text: a.text("text")?,
        },
        ToolName::MessageStatus => ToolCall::MessageStatus {
            id: MessageId(a.u64("message_id")?),
        },
        ToolName::ReadAgent => ToolCall::ReadAgent {
            agent: a.agent("agent")?,
        },
        ToolName::WaitForAgent => ToolCall::WaitForAgent {
            agent: a.agent("agent")?,
            timeout_minutes: a.timeout()?,
        },
        ToolName::StartAgent => ToolCall::StartAgent {
            agent: a.agent("agent")?,
            task: a.text("task")?,
            run: a.run()?,
        },
        ToolName::StopAgent => ToolCall::StopAgent {
            agent: a.agent("agent")?,
        },
        ToolName::AssignToRole => ToolCall::AssignToRole {
            role: RoleKey {
                team: a.opt_node("team")?,
                role: a.node("role")?,
            },
            task: a.text("task")?,
        },
        ToolName::ProposeOrgChange => ToolCall::ProposeOrgChange {
            change: Box::new(parse_change(a.get("change")?)?),
        },
    })
}

fn invalid(message: impl Into<String>) -> ToolError {
    ToolError::new(ErrorCode::InvalidArguments, message)
}

/// An argument object, read field by field; unknown fields are never looked at.
struct Args<'a>(&'a Map<String, Value>);

impl<'a> Args<'a> {
    fn get(&self, field: &str) -> Result<&'a Value, ToolError> {
        self.0
            .get(field)
            .filter(|v| !v.is_null())
            .ok_or_else(|| invalid(format!("{field}: missing")))
    }

    fn str(&self, field: &str) -> Result<&'a str, ToolError> {
        self.get(field)?
            .as_str()
            .ok_or_else(|| invalid(format!("{field}: must be a string")))
    }

    fn agent(&self, field: &str) -> Result<AgentName, ToolError> {
        AgentName::parse(self.str(field)?)
            .map_err(|e| invalid(format!("{field}: not an agent name ({e})")))
    }

    fn node(&self, field: &str) -> Result<NodeName, ToolError> {
        NodeName::parse(self.str(field)?).map_err(|e| invalid(format!("{field}: {e}")))
    }

    fn opt_node(&self, field: &str) -> Result<Option<NodeName>, ToolError> {
        match self.0.get(field) {
            None | Some(Value::Null) => Ok(None),
            Some(_) => self.node(field).map(Some),
        }
    }

    fn text(&self, field: &str) -> Result<String, ToolError> {
        let s = self.str(field)?;
        if s.chars().count() > MAX_TEXT {
            return Err(invalid(format!(
                "{field}: longer than {MAX_TEXT} characters"
            )));
        }
        Ok(s.to_owned())
    }

    fn u64(&self, field: &str) -> Result<u64, ToolError> {
        self.get(field)?
            .as_u64()
            .ok_or_else(|| invalid(format!("{field}: must be a whole number")))
    }

    fn timeout(&self) -> Result<u32, ToolError> {
        let field = "timeout_minutes";
        if matches!(self.0.get(field), None | Some(Value::Null)) {
            return Ok(WAIT_DEFAULT_MINUTES);
        }
        match u32::try_from(self.u64(field)?) {
            Ok(m) if (1..=WAIT_MAX_MINUTES).contains(&m) => Ok(m),
            _ => Err(invalid(format!("{field}: must be 1 to {WAIT_MAX_MINUTES}"))),
        }
    }

    fn run(&self) -> Result<Option<RunIn>, ToolError> {
        match self.0.get("run") {
            None | Some(Value::Null) => Ok(None),
            Some(_) => match self.str("run")? {
                "directory" => Ok(Some(RunIn::Directory)),
                "worktree" => Ok(Some(RunIn::NewWorktree)),
                _ => Err(invalid("run: must be \"directory\" or \"worktree\"")),
            },
        }
    }

    fn object(&self, field: &str) -> Result<Args<'a>, ToolError> {
        self.get(field)?
            .as_object()
            .map(Args)
            .ok_or_else(|| invalid(format!("{field}: must be an object")))
    }

    fn fields(&self) -> Result<Fields, ToolError> {
        let f = match self.0.get("fields") {
            None | Some(Value::Null) => return Ok(Fields::default()),
            Some(_) => self.object("fields")?,
        };
        let opt_str = |k: &str| -> Result<Option<String>, ToolError> {
            match f.0.get(k) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(s)) => Ok(Some(s.clone())),
                Some(_) => Err(invalid(format!("fields.{k}: must be a string"))),
            }
        };
        let cli = match opt_str("cli")?.as_deref() {
            None => None,
            Some("claude") => Some(CliKind::Claude),
            Some("opencode") => Some(CliKind::Opencode),
            Some(_) => return Err(invalid("fields.cli: must be \"claude\" or \"opencode\"")),
        };
        let worktree = match f.0.get("worktree") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(b)) => Some(*b),
            Some(_) => return Err(invalid("fields.worktree: must be true or false")),
        };
        Ok(Fields {
            cli,
            flags: opt_str("flags")?,
            directory: opt_str("directory")?,
            model: opt_str("model")?,
            effort: opt_str("effort")?,
            worktree,
            prompt: opt_str("prompt")?,
        })
    }

    fn role_key(&self, field: &str) -> Result<RoleKey, ToolError> {
        let r = self.object(field)?;
        Ok(RoleKey {
            team: r.opt_node("team")?,
            role: r.node("role")?,
        })
    }
}

/// A node object: `{"agent"}`, `{"team"}`, `{"role", "team"}` or `{"duo": true}`.
fn parse_node(v: &Value, field: &str, root_ok: bool) -> Result<NodeKey, ToolError> {
    let Some(map) = v.as_object() else {
        return Err(invalid(format!("{field}: must be a node object")));
    };
    let a = Args(map);
    if map.get("duo").and_then(Value::as_bool) == Some(true) {
        return if root_ok {
            Ok(NodeKey::Duo)
        } else {
            Err(invalid(format!("{field}: the root is only a move target")))
        };
    }
    if map.contains_key("agent") {
        return a.agent("agent").map(NodeKey::Agent);
    }
    if map.contains_key("role") {
        return Ok(NodeKey::Role(RoleKey {
            team: a.opt_node("team")?,
            role: a.node("role")?,
        }));
    }
    if map.contains_key("team") {
        return a.node("team").map(NodeKey::Team);
    }
    Err(invalid(format!("{field}: names no agent, team or role")))
}

fn parse_change(v: &Value) -> Result<OrgEdit, ToolError> {
    let Some(map) = v.as_object() else {
        return Err(invalid("change: must be an object"));
    };
    let c = Args(map);
    let op = c.str("op").map_err(|_| invalid("change.op: missing"))?;
    let named = |field: &str| c.node(field).map_err(prefixed);
    Ok(match op {
        "add_team" => OrgEdit::AddTeam(TeamDraft {
            name: named("name")?,
            defaults: c.fields().map_err(prefixed)?,
        }),
        "add_role" => {
            let agents = match map.get("agents") {
                None | Some(Value::Null) => 0,
                Some(_) => match c.u64("agents") {
                    Ok(n) if n <= MAX_ROLE_AGENTS => n,
                    _ => return Err(invalid("change.agents: must be 0 to 9")),
                },
            };
            OrgEdit::AddRole {
                team: c.opt_node("team").map_err(prefixed)?,
                role: RoleDraft {
                    name: named("name")?,
                    defaults: c.fields().map_err(prefixed)?,
                },
                agents: u8::try_from(agents).unwrap_or(0),
            }
        }
        "add_agent" => OrgEdit::AddAgent {
            role: c.role_key("role").map_err(prefixed)?,
            agent: AgentDraft {
                name: c.agent("name").map_err(prefixed)?,
                fields: c.fields().map_err(prefixed)?,
            },
        },
        "edit_team" => OrgEdit::EditTeam {
            key: named("team")?,
            draft: TeamDraft {
                name: named("name")?,
                defaults: c.fields().map_err(prefixed)?,
            },
        },
        "edit_role" => OrgEdit::EditRole {
            key: c.role_key("role").map_err(prefixed)?,
            draft: RoleDraft {
                name: named("name")?,
                defaults: c.fields().map_err(prefixed)?,
            },
        },
        "edit_agent" => OrgEdit::EditAgent {
            key: c.agent("agent").map_err(prefixed)?,
            draft: AgentDraft {
                name: c.agent("name").map_err(prefixed)?,
                fields: c.fields().map_err(prefixed)?,
            },
        },
        "move" => {
            let node = parse_node(c.get("node").map_err(prefixed)?, "change.node", false)?;
            let to = c.get("to").map_err(prefixed)?;
            let target = match to {
                Value::String(s) if s == "end" => MoveTarget::End,
                Value::Object(m) if m.contains_key("before_team") => {
                    MoveTarget::Before(Args(m).node("before_team").map_err(prefixed)?)
                }
                _ => MoveTarget::Into(parse_node(to, "change.to", true)?),
            };
            let positional = matches!(target, MoveTarget::Before(_) | MoveTarget::End);
            if positional && !matches!(node, NodeKey::Team(_)) {
                return Err(invalid(
                    "change.to: before_team and \"end\" move teams only",
                ));
            }
            OrgEdit::Move { node, to: target }
        }
        "remove" => OrgEdit::Delete(parse_node(
            c.get("node").map_err(prefixed)?,
            "change.node",
            false,
        )?),
        other => return Err(invalid(format!("change.op: unknown op {other}"))),
    })
}

/// Names a field inside `change`.
fn prefixed(e: ToolError) -> ToolError {
    if e.message.starts_with("change.") {
        e
    } else {
        invalid(format!("change.{}", e.message))
    }
}

#[cfg(test)]
pub(crate) mod tests;
