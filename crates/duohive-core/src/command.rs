//! What the UI (or Duo) asks the engine to do (TD §3.2, phase 1 subset).

use serde::{Deserialize, Serialize};

use crate::names::{AgentName, NodeName};
use crate::org::{DuoConfig, Fields, NodeKey, OrgDefaults, RoleKey};
use crate::settings::Settings;

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
    },
    /// Mark an agent as the one being looked at.
    ViewAgent {
        /// Which agent.
        agent: AgentName,
    },
    /// Change the org.
    EditOrg(OrgEdit),
    /// Re-read `org.toml`.
    ReloadOrg,
    /// Apply new settings.
    UpdateSettings(Settings),
    /// Stop everything and exit.
    Shutdown,
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
}
