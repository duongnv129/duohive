//! What the UI (or Duo) asks the engine to do (TD §3.2, phase 1 subset).

use serde::{Deserialize, Serialize};

use crate::card::{CardChange, CardId};
use crate::cli_kind::CliKind;
use crate::launch::Token;
use crate::names::{AgentName, NodeName};
use crate::org::{DuoConfig, Fields, NodeKey, OrgDefaults, RoleKey};
use crate::settings::Settings;
use crate::templates::TemplateInput;
use crate::tool::{CallId, ToolCall};
use crate::worktree::{RunIn, TaskId};

/// A request to the engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Start an agent.
    StartAgent {
        /// Which agent.
        agent: AgentName,
    },
    /// Stop an agent.
    StopAgent {
        /// Which agent.
        agent: AgentName,
    },
    /// Hand an agent a task.
    GiveWork {
        /// Which agent.
        agent: AgentName,
        /// The task text, verbatim.
        task: String,
        /// Where the agent works.
        run: RunIn,
    },
    /// Mark an agent as the one being looked at.
    ViewAgent {
        /// Which agent.
        agent: AgentName,
    },
    /// Change the org.
    EditOrg(OrgEdit),
    /// Check every worktree now (the list opened; TD §5.3).
    CheckWorktrees,
    /// Remove a worktree by hand. Distinct from the `RemoveWorktree` port request:
    /// anything but merged and clean needs `confirmed` (P2-D6).
    DeleteWorktree {
        /// Its agent.
        agent: AgentName,
        /// Its task.
        task: TaskId,
        /// The user confirmed removing a worktree that isn't merged and clean.
        confirmed: bool,
    },
    /// Re-read `org.toml`.
    ReloadOrg,
    /// Apply new settings.
    UpdateSettings(Settings),
    /// Stop everything and exit.
    Shutdown,
    /// An agent's tool call, from the MCP endpoint (TD §6.1). Answered by
    /// `Event::ToolResult` with the same `call_id`.
    Tool {
        /// The endpoint's id for the call.
        call_id: CallId,
        /// The caller's launch token; identity comes only from it.
        token: Token,
        /// The parsed call.
        call: ToolCall,
    },
    /// Your `@agent text` message.
    SendMessage {
        /// The recipient.
        to: AgentName,
        /// The text, typed in verbatim.
        text: String,
    },
    /// A line you typed to Duo.
    ChatToDuo {
        /// The text.
        text: String,
    },
    /// Your task for a role (UI §4.5).
    AssignToRole {
        /// The role.
        role: RoleKey,
        /// The task.
        task: String,
        /// The agent you picked, or `None` for the one at rest longest.
        pick: Option<AgentName>,
        /// Where it runs, for a stopped or errored pick.
        run: Option<RunIn>,
    },
    /// Approve a card, optionally edited.
    ApproveCard {
        /// Which card.
        card: CardId,
        /// The edited change, if you edited it.
        edited: Option<Box<CardChange>>,
    },
    /// Reject a card.
    RejectCard {
        /// Which card.
        card: CardId,
    },
    /// You are typing into an agent's pane, or stopped (P3-D19).
    Typing {
        /// The agent, or `None` when typing ends.
        agent: Option<AgentName>,
    },
    /// Create the org the first-run wizard built (UI §4.3): accepted only while
    /// no org exists.
    CreateOrg {
        /// The template and its answers.
        template: TemplateInput,
        /// Your name, for `settings.toml`.
        your_name: Option<String>,
        /// Duo's CLI, which `[org] cli` takes.
        duo_cli: CliKind,
    },
    /// Replace the org with the one `template` builds (UI §4.8): every agent
    /// the old tree has is stopped first, then the new org is written.
    NewOrgFromTemplate {
        /// The template and its answers.
        template: TemplateInput,
    },
}

/// Where a node moves (P3-D20).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MoveTarget {
    /// Under another node: an agent into a role, a role into a team or the root.
    Into(NodeKey),
    /// A team, before another team.
    Before(NodeName),
    /// A team, after every other team.
    End,
}

/// A team being added or edited.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamDraft {
    /// Its name.
    pub name: NodeName,
    /// Its defaults.
    pub defaults: Fields,
}

/// A role being added or edited.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleDraft {
    /// Its name.
    pub name: NodeName,
    /// Its defaults.
    pub defaults: Fields,
}

/// An agent being added or edited.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDraft {
    /// Its name.
    pub name: AgentName,
    /// Its own fields.
    pub fields: Fields,
}

/// A change to the org.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrgEdit {
    /// Add a team.
    AddTeam(TeamDraft),
    /// Add a role with `agents` new agents (D11).
    AddRole {
        /// The team, or `None` for a root role.
        team: Option<NodeName>,
        /// The role.
        role: RoleDraft,
        /// How many agents to create.
        agents: u8,
    },
    /// Add an agent to a role.
    AddAgent {
        /// The role.
        role: RoleKey,
        /// The agent.
        agent: AgentDraft,
    },
    /// Edit a team.
    EditTeam {
        /// The team to edit.
        key: NodeName,
        /// Its new values.
        draft: TeamDraft,
    },
    /// Edit a role.
    EditRole {
        /// The role to edit.
        key: RoleKey,
        /// Its new values.
        draft: RoleDraft,
    },
    /// Edit an agent.
    EditAgent {
        /// The agent to edit.
        key: AgentName,
        /// Its new values.
        draft: AgentDraft,
    },
    /// Edit the org defaults and Duo.
    EditOrgDefaults {
        /// New org defaults.
        defaults: OrgDefaults,
        /// New Duo configuration.
        duo: DuoConfig,
    },
    /// Delete a node and everything under it.
    Delete(NodeKey),
    /// Move a node (P3-D20).
    Move {
        /// The node.
        node: NodeKey,
        /// Where it goes.
        to: MoveTarget,
    },
}
