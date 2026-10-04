//! `n`: what can be created at the selection (UI §4.4.2).

use std::path::Path;

use duohive_core::names::NodeName;
use duohive_core::org::{NodeKey, Org, RoleKey};
use duohive_core::validate::DirProbe;

use super::Form;

/// The form a chooser entry opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewKind {
    /// A team.
    Team,
    /// A role in this team, or directly under Duo.
    Role(Option<NodeName>),
    /// An agent in this role.
    Agent(RoleKey),
}

/// One entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChooserItem {
    /// What it says.
    pub label: String,
    /// Whether it can be chosen; a disabled entry's label says why.
    pub enabled: bool,
    /// The form it opens.
    pub opens: Option<NewKind>,
}

/// "New · <x> selected".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chooser {
    /// The title in the top border.
    pub title: String,
    /// The entries.
    pub items: Vec<ChooserItem>,
    /// The focused entry, always an enabled one.
    pub focus: usize,
}

fn item(label: String, opens: NewKind) -> ChooserItem {
    ChooserItem {
        label,
        enabled: true,
        opens: Some(opens),
    }
}

impl Chooser {
    /// On Duo a team or a role; on a team a role in it or a team next to it; on a role or
    /// an agent an agent in that role.
    pub fn for_selection(org: &Org, sel: &NodeKey) -> Chooser {
        let (selected, items) = match sel {
            NodeKey::Team(t) => (
                t.to_string(),
                vec![
                    item(format!("Role in {t}"), NewKind::Role(Some(t.clone()))),
                    item(format!("Team next to {t}"), NewKind::Team),
                    ChooserItem {
                        label: "Agent: select a role first".to_owned(),
                        enabled: false,
                        opens: None,
                    },
                ],
            ),
            NodeKey::Role(k) => (k.role.to_string(), vec![agent_in(k.clone())]),
            NodeKey::Agent(a) => match org.find_agent(a) {
                Some(loc) => {
                    let role = RoleKey {
                        team: loc.team.map(|t| t.name.clone()),
                        role: loc.role.name.clone(),
                    };
                    (a.to_string(), vec![agent_in(role)])
                }
                None => duo(org),
            },
            NodeKey::Duo => duo(org),
        };
        let focus = items.iter().position(|i| i.enabled).unwrap_or(0);
        Chooser {
            title: format!("New · {selected} selected"),
            items,
            focus,
        }
    }

    /// Moves focus to the next enabled entry, stopping at the ends.
    pub fn step(&mut self, down: bool) {
        let mut i = self.focus;
        loop {
            let next = if down {
                i.checked_add(1).filter(|n| *n < self.items.len())
            } else {
                i.checked_sub(1)
            };
            let Some(n) = next else {
                return;
            };
            if self.items[n].enabled {
                self.focus = n;
                return;
            }
            i = n;
        }
    }

    /// The form the focused entry opens.
    pub fn open(&self, org: &Org, dirs: &dyn DirProbe, home: &Path) -> Option<Form> {
        let chosen = self.items.get(self.focus).filter(|i| i.enabled)?;
        Some(match chosen.opens.as_ref()? {
            NewKind::Team => Form::new_team(org, dirs, home),
            NewKind::Role(team) => Form::new_role(org, team.as_ref(), dirs, home),
            NewKind::Agent(role) => Form::new_agent(org, role, dirs, home),
        })
    }
}

fn agent_in(role: RoleKey) -> ChooserItem {
    item(format!("Agent in {}", role.role), NewKind::Agent(role))
}

fn duo(org: &Org) -> (String, Vec<ChooserItem>) {
    (
        org.duo.name.to_string(),
        vec![
            item("Team".to_owned(), NewKind::Team),
            item("Role".to_owned(), NewKind::Role(None)),
        ],
    )
}
