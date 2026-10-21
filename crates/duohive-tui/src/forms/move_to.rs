//! Move to… (UI §4.4.2): the valid targets for a node and what the move changes in what
//! it inherits. Pure over the org; the engine stays the authority on the edit.

use duohive_core::command::{MoveTarget, OrgEdit};
use duohive_core::engine::preview_edit;
use duohive_core::inherit::{ResolvedAgent, resolve};
use duohive_core::names::AgentName;
use duohive_core::org::{NodeKey, Org, RoleKey};

/// One inherited value a move changes: `worktrees   off → on`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InheritedChange {
    /// What it is (`worktrees`, `role prompt`).
    pub field: &'static str,
    /// Before the move.
    pub from: String,
    /// After it.
    pub to: String,
}

/// One place the node can go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveOption {
    /// How it reads in the list.
    pub label: String,
    /// The target the edit carries.
    pub to: MoveTarget,
    /// The node is already there: shown, marked, not choosable.
    pub current: bool,
    /// Why the core would refuse this move (a role's name taken in the team).
    pub blocked: Option<String>,
    /// What the move changes in the node's inherited values.
    pub changes: Vec<InheritedChange>,
}

/// The open picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveTo {
    /// The node being moved.
    pub node: NodeKey,
    /// Its name, for the title.
    pub name: String,
    /// The places, in list order.
    pub options: Vec<MoveOption>,
    /// The highlighted one.
    pub cursor: usize,
}

impl MoveTo {
    /// The picker for `node`, or `None` when there is no other place for it.
    pub fn open(org: &Org, node: &NodeKey) -> Option<Self> {
        let options = valid_targets(org, node);
        let cursor = options.iter().position(|o| !o.current)?;
        Some(Self {
            node: node.clone(),
            name: node_name(node),
            options,
            cursor,
        })
    }

    /// Moves the highlight by one row, staying in the list.
    pub fn step(&mut self, down: bool) {
        let last = self.options.len().saturating_sub(1);
        self.cursor = if down {
            (self.cursor + 1).min(last)
        } else {
            self.cursor.saturating_sub(1)
        };
    }

    /// The highlighted option.
    pub fn selected(&self) -> Option<&MoveOption> {
        self.options.get(self.cursor)
    }

    /// The edit the highlighted option asks for; `None` on the current place or a refused one.
    pub fn edit(&self) -> Option<OrgEdit> {
        let o = self
            .selected()
            .filter(|o| !o.current && o.blocked.is_none())?;
        Some(OrgEdit::Move {
            node: self.node.clone(),
            to: o.to.clone(),
        })
    }
}

/// A node's own name.
pub fn node_name(node: &NodeKey) -> String {
    match node {
        NodeKey::Duo => "Duo".to_owned(),
        NodeKey::Team(t) => t.to_string(),
        NodeKey::Role(k) => k.role.to_string(),
        NodeKey::Agent(a) => a.to_string(),
    }
}

/// Every place `node` could be moved to, the current one included and marked.
pub fn valid_targets(org: &Org, node: &NodeKey) -> Vec<MoveOption> {
    let candidates = match node {
        NodeKey::Duo => Vec::new(),
        NodeKey::Agent(a) => agent_places(org, a),
        NodeKey::Role(k) => role_places(org, k),
        NodeKey::Team(t) => team_places(org, t),
    };
    candidates
        .into_iter()
        .map(|(label, to, current)| {
            let edit = OrgEdit::Move {
                node: node.clone(),
                to: to.clone(),
            };
            let (blocked, changes) = if current {
                (None, Vec::new())
            } else {
                match preview_edit(org, &edit) {
                    Ok(after) => (None, changes(org, &after, node)),
                    Err(errors) => (errors.first().map(|e| refusal(&e.message)), Vec::new()),
                }
            };
            MoveOption {
                label,
                to,
                current,
                blocked,
                changes,
            }
        })
        .collect()
}

type Place = (String, MoveTarget, bool);

/// `Payments › Backend` or a root role's name, for each role; the agent's own is current.
fn agent_places(org: &Org, agent: &AgentName) -> Vec<Place> {
    let own = org
        .find_agent(agent)
        .map(|l| (l.team.map(|t| t.name.clone()), l.role.name.clone()));
    let keys = org
        .teams
        .iter()
        .flat_map(|t| {
            t.roles.iter().map(|r| RoleKey {
                team: Some(t.name.clone()),
                role: r.name.clone(),
            })
        })
        .chain(org.roles.iter().map(|r| RoleKey {
            team: None,
            role: r.name.clone(),
        }));
    keys.map(|k| {
        let label = match &k.team {
            Some(t) => format!("{t} › {}", k.role),
            None => k.role.to_string(),
        };
        let current = own.as_ref() == Some(&(k.team.clone(), k.role.clone()));
        (label, MoveTarget::Into(NodeKey::Role(k)), current)
    })
    .collect()
}

/// Each team, and the top level; the role's own place is current.
fn role_places(org: &Org, role: &RoleKey) -> Vec<Place> {
    let teams = org.teams.iter().map(|t| {
        (
            t.name.to_string(),
            MoveTarget::Into(NodeKey::Team(t.name.clone())),
            role.team.as_ref() == Some(&t.name),
        )
    });
    let top = std::iter::once((
        "No team (top level)".to_owned(),
        MoveTarget::Into(NodeKey::Duo),
        role.team.is_none(),
    ));
    teams.chain(top).collect()
}

/// `before <team>` for each other team, and `at the end`.
fn team_places(org: &Org, team: &duohive_core::names::NodeName) -> Vec<Place> {
    let at = org.teams.iter().position(|t| &t.name == team);
    let mut out: Vec<Place> = org
        .teams
        .iter()
        .enumerate()
        .filter(|(_, t)| &t.name != team)
        .map(|(i, t)| {
            // Right after it already: moving before the next team changes nothing.
            let current = at.is_some_and(|a| i == a + 1);
            (
                format!("before {}", t.name),
                MoveTarget::Before(t.name.clone()),
                current,
            )
        })
        .collect();
    out.push((
        "at the end".to_owned(),
        MoveTarget::End,
        at.is_some_and(|a| a + 1 == org.teams.len()),
    ));
    out
}

/// The core's collision message in the words of UI §4.4.2.
fn refusal(message: &str) -> String {
    message
        .strip_prefix("a role named ")
        .and_then(|rest| rest.split_once(" already exists in "))
        .map_or_else(
            || message.to_owned(),
            |(role, parent)| format!("{parent} already has a role {role}"),
        )
}

/// What the moved node inherits differently after the move, read off its first agent.
fn changes(before: &Org, after: &Org, node: &NodeKey) -> Vec<InheritedChange> {
    let agent = match node {
        NodeKey::Agent(a) => Some(a.clone()),
        NodeKey::Role(k) => first_agent(before, k),
        NodeKey::Team(_) | NodeKey::Duo => None,
    };
    let Some(agent) = agent else {
        return Vec::new();
    };
    let (Ok(was), Ok(now)) = (resolve(before, &agent), resolve(after, &agent)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut diff = |field: &'static str, from: String, to: String| {
        if from != to {
            out.push(InheritedChange { field, from, to });
        }
    };
    let text = |v: &Option<String>| v.clone().unwrap_or_else(|| "none".to_owned());
    diff(
        "CLI",
        was.cli.display().to_owned(),
        now.cli.display().to_owned(),
    );
    diff("directory", text(&was.directory), text(&now.directory));
    diff("model", text(&was.model), text(&now.model));
    diff("effort", text(&was.effort), text(&now.effort));
    diff("flags", flags(&was), flags(&now));
    diff("worktrees", on_off(was.worktree), on_off(now.worktree));
    if was.prompt != now.prompt {
        let (field, from, to) = prompt_levels(before, after, node, &agent);
        diff(field, from, to);
    }
    out
}

fn first_agent(org: &Org, key: &RoleKey) -> Option<AgentName> {
    org.agents()
        .into_iter()
        .find(|a| a.team.map(|t| &t.name) == key.team.as_ref() && a.role.name == key.role)
        .map(|a| a.agent.name.clone())
}

fn flags(a: &ResolvedAgent) -> String {
    if a.flags.is_empty() {
        "none".to_owned()
    } else {
        a.flags.join(" ")
    }
}

fn on_off(b: bool) -> String {
    if b { "on" } else { "off" }.to_owned()
}

/// Which prompt level changed: a moved agent gets another role's prompt, a moved role another team's.
fn prompt_levels(
    before: &Org,
    after: &Org,
    node: &NodeKey,
    agent: &AgentName,
) -> (&'static str, String, String) {
    let place = |org: &Org| {
        org.find_agent(agent).map(|l| {
            (
                l.role.name.to_string(),
                l.team
                    .map_or_else(|| "none".to_owned(), |t| t.name.to_string()),
            )
        })
    };
    let (was, now) = (place(before), place(after));
    let pick = |p: Option<(String, String)>, role: bool| {
        p.map_or_else(String::new, |(r, t)| if role { r } else { t })
    };
    match node {
        NodeKey::Role(_) => ("team prompt", pick(was, false), pick(now, false)),
        _ => ("role prompt", pick(was, true), pick(now, true)),
    }
}

#[cfg(test)]
mod tests {
    use duohive_core::testkit::{example_org, n};

    use super::*;

    fn nn(s: &str) -> duohive_core::names::NodeName {
        duohive_core::names::NodeName::parse(s).unwrap()
    }

    fn labels(opts: &[MoveOption]) -> Vec<String> {
        opts.iter().map(|o| o.label.clone()).collect()
    }

    #[test]
    fn invalid_targets_absent() {
        let org = example_org();
        // An agent goes into roles only: no team, no Duo.
        let agent = valid_targets(&org, &NodeKey::Agent(n("auth-service")));
        assert!(
            agent
                .iter()
                .all(|o| matches!(o.to, MoveTarget::Into(NodeKey::Role(_))))
        );
        // A team has positions only.
        let team = valid_targets(&org, &NodeKey::Team(nn("Payments")));
        assert!(
            team.iter()
                .all(|o| matches!(o.to, MoveTarget::Before(_) | MoveTarget::End))
        );
        assert!(valid_targets(&org, &NodeKey::Duo).is_empty());
    }

    #[test]
    fn current_target_marked_and_skipped() {
        let org = example_org();
        let picker = MoveTo::open(&org, &NodeKey::Agent(n("auth-service"))).unwrap();
        let current = picker.options.iter().position(|o| o.current).unwrap();
        assert_eq!(picker.options[current].label, "Payments › Backend");
        assert_ne!(picker.cursor, current, "the cursor starts on a place to go");
        let mut on_current = picker.clone();
        on_current.cursor = current;
        assert_eq!(on_current.edit(), None);
    }

    #[test]
    fn move_on_team_lists_positions() {
        let mut org = example_org();
        org.teams.push(duohive_core::org::Team {
            name: nn("Mobile"),
            defaults: Default::default(),
            roles: Vec::new(),
        });
        let opts = valid_targets(&org, &NodeKey::Team(nn("Payments")));
        assert_eq!(labels(&opts), ["before Mobile", "at the end"]);
        // Payments is first, so `before Mobile` leaves it where it is.
        assert!(opts[0].current && !opts[1].current);
    }

    #[test]
    fn move_sends_org_edit_move() {
        let org = example_org();
        let picker = MoveTo::open(&org, &NodeKey::Agent(n("auth-service"))).unwrap();
        let Some(OrgEdit::Move { node, to }) = picker.edit() else {
            panic!("no move edit");
        };
        assert_eq!(node, NodeKey::Agent(n("auth-service")));
        assert!(matches!(to, MoveTarget::Into(NodeKey::Role(_))));
    }

    #[test]
    fn no_other_place_gives_no_picker() {
        let org = example_org();
        assert!(MoveTo::open(&org, &NodeKey::Duo).is_none());
    }

    #[test]
    fn refusal_uses_the_spec_words() {
        assert_eq!(
            refusal("a role named QA already exists in Payments"),
            "Payments already has a role QA"
        );
        assert_eq!(refusal("something else"), "something else");
    }
}
