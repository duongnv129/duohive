//! The org tree: Duo, teams, roles and agents.

use serde::{Deserialize, Serialize};

use crate::cli_kind::CliKind;
use crate::names::{AgentName, NodeName};

/// Breadcrumb separator.
const SEP: &str = " › ";

/// Optional settings a node may set; unset ones inherit from above.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fields {
    /// Which CLI to run.
    pub cli: Option<CliKind>,
    /// Extra CLI flags, as typed.
    pub flags: Option<String>,
    /// Working directory.
    pub directory: Option<String>,
    /// Model.
    pub model: Option<String>,
    /// Reasoning effort.
    pub effort: Option<String>,
    /// Whether to work in a git worktree.
    pub worktree: Option<bool>,
    /// Prompt text for this level.
    pub prompt: Option<String>,
}

/// Org-level defaults; unlike [`Fields`], always resolved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrgDefaults {
    /// Default CLI.
    pub cli: CliKind,
    /// Default model.
    pub model: Option<String>,
    /// Default effort.
    pub effort: Option<String>,
    /// Default worktree setting.
    pub worktree: bool,
    /// Org prompt.
    pub prompt: Option<String>,
}

/// The coordinating agent at the top of the org.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuoConfig {
    /// Duo's name (default "Duo").
    pub name: AgentName,
    /// The CLI Duo runs on.
    pub cli: CliKind,
}

/// One agent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDef {
    /// Unique name.
    pub name: AgentName,
    /// Its own settings.
    pub fields: Fields,
}

/// A role and its agents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    /// Role name.
    pub name: NodeName,
    /// Settings its agents inherit.
    pub defaults: Fields,
    /// Its agents.
    pub agents: Vec<AgentDef>,
}

/// A team and its roles.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    /// Team name.
    pub name: NodeName,
    /// Settings its roles inherit.
    pub defaults: Fields,
    /// Its roles.
    pub roles: Vec<Role>,
}

/// The whole org.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Org {
    /// Org-wide defaults.
    pub defaults: OrgDefaults,
    /// Duo.
    pub duo: DuoConfig,
    /// Teams, in order.
    pub teams: Vec<Team>,
    /// Roles directly under Duo, in order.
    pub roles: Vec<Role>,
}

/// A role's address: in a team, or directly under Duo.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RoleKey {
    /// The team, or `None` for a root role.
    pub team: Option<NodeName>,
    /// The role.
    pub role: NodeName,
}

/// Any node in the tree.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum NodeKey {
    /// Duo.
    Duo,
    /// A team.
    Team(NodeName),
    /// A role.
    Role(RoleKey),
    /// An agent.
    Agent(AgentName),
}

/// Where an agent sits; a borrowed view, never serialised.
#[derive(Clone, Debug)]
pub struct AgentLoc<'a> {
    /// Its team, if any.
    pub team: Option<&'a Team>,
    /// Its role.
    pub role: &'a Role,
    /// The agent.
    pub agent: &'a AgentDef,
}

impl Org {
    /// An org with only Duo, on `cli`.
    pub fn blank(cli: CliKind) -> Org {
        Org {
            defaults: OrgDefaults {
                cli,
                model: None,
                effort: None,
                worktree: false,
                prompt: None,
            },
            duo: DuoConfig {
                name: AgentName::duo(),
                cli,
            },
            teams: Vec::new(),
            roles: Vec::new(),
        }
    }

    /// Finds an agent by name.
    pub fn find_agent(&self, name: &AgentName) -> Option<AgentLoc<'_>> {
        self.agents().into_iter().find(|a| &a.agent.name == name)
    }

    /// All agents, teams first and then root roles, in tree order.
    pub fn agents(&self) -> Vec<AgentLoc<'_>> {
        let in_teams = self.teams.iter().flat_map(|t| {
            t.roles.iter().flat_map(move |r| {
                r.agents.iter().map(move |agent| AgentLoc {
                    team: Some(t),
                    role: r,
                    agent,
                })
            })
        });
        let at_root = self.roles.iter().flat_map(|r| {
            r.agents.iter().map(move |agent| AgentLoc {
                team: None,
                role: r,
                agent,
            })
        });
        in_teams.chain(at_root).collect()
    }

    /// Whether `name` is Duo's name.
    pub fn is_duo(&self, name: &AgentName) -> bool {
        &self.duo.name == name
    }

    /// Path to a node, e.g. `Payments › Backend › billing-api`.
    pub fn breadcrumb(&self, key: &NodeKey) -> String {
        match key {
            NodeKey::Duo => self.duo.name.to_string(),
            NodeKey::Team(t) => t.to_string(),
            NodeKey::Role(RoleKey { team, role }) => {
                join(team.as_ref().map(NodeName::as_str), role.as_str(), None)
            }
            NodeKey::Agent(a) => match self.find_agent(a) {
                Some(loc) => join(
                    loc.team.map(|t| t.name.as_str()),
                    loc.role.name.as_str(),
                    Some(a.as_str()),
                ),
                None => a.to_string(),
            },
        }
    }
}

fn join(team: Option<&str>, role: &str, agent: Option<&str>) -> String {
    let parts: Vec<&str> = team.into_iter().chain([role]).chain(agent).collect();
    parts.join(SEP)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{example_org, n};

    #[test]
    fn find_agent_in_team_and_root_role() {
        let org = example_org();
        let a = org.find_agent(&n("auth-service")).unwrap();
        assert_eq!(
            (a.team.map(|t| t.name.as_str()), a.role.name.as_str()),
            (Some("Payments"), "Backend")
        );
        let s = org.find_agent(&n("squad-1")).unwrap();
        assert_eq!(
            (s.team.is_none(), s.role.name.as_str()),
            (true, "Generalist")
        );
        assert!(org.find_agent(&n("nobody")).is_none());
    }

    #[test]
    fn breadcrumb_formats() {
        let org = example_org();
        assert_eq!(
            org.breadcrumb(&NodeKey::Agent(n("billing-api"))),
            "Payments › Backend › billing-api"
        );
        assert_eq!(
            org.breadcrumb(&NodeKey::Agent(n("squad-2"))),
            "Generalist › squad-2"
        );
        assert_eq!(org.breadcrumb(&NodeKey::Duo), "Duo");
        assert_eq!(
            org.breadcrumb(&NodeKey::Role(RoleKey {
                team: Some(NodeName::parse("Payments").unwrap()),
                role: NodeName::parse("QA").unwrap(),
            })),
            "Payments › QA"
        );
    }

    #[test]
    fn agents_in_tree_order_teams_first() {
        let names: Vec<_> = example_org()
            .agents()
            .iter()
            .map(|a| a.agent.name.to_string())
            .collect();
        assert_eq!(
            names,
            [
                "auth-service",
                "billing-api",
                "pay-qa",
                "squad-1",
                "squad-2",
                "squad-3"
            ]
        );
    }

    #[test]
    fn blank_org_has_only_duo() {
        let org = Org::blank(CliKind::Opencode);
        assert!(org.is_duo(&n("Duo")));
        assert_eq!(
            (org.duo.cli, org.defaults.cli),
            (CliKind::Opencode, CliKind::Opencode)
        );
        assert!(org.agents().is_empty());
    }
}
