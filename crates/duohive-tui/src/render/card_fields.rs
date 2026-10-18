//! The rows under an org-change card's first line (UI §4.6): every field the change sets
//! or changes, one row each, so nothing is approved unseen. `+ field: value` for an add,
//! `~ field: old → new` for an edit (unchanged fields left out), `from … → …` for a move
//! and what goes with a remove. A `flags` row is marked: drawn in `status.attention` and
//! shown in full.

use duohive_core::command::{MoveTarget, OrgEdit};
use duohive_core::names::AgentName;
use duohive_core::org::{DuoConfig, Fields, NodeKey, Org, OrgDefaults, Role, RoleKey};

use crate::format::{one_line, truncate_end, width};

/// `inherit`: a field the node leaves to its parent.
const INHERIT: &str = "inherit";
/// An org default with no value.
const UNSET: &str = "unset";
/// A prompt's first line is cut to this, so its line count stays in sight.
const PROMPT_CELLS: usize = 20;
/// An edit row's text width; wider, the new value hangs on a second row.
const ROW_CELLS: usize = super::card::CARD_WIDTH - 4;
/// Before a field row.
const INDENT: &str = "  ";
/// Before a hung `→ new` row: under the field's value.
const HANG_NEW: &str = "      ";

/// One body row and whether it is marked (a `flags` row).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BodyLine {
    /// Its text.
    pub text: String,
    /// Drawn in `status.attention` and wrapped, never cut.
    pub marked: bool,
}

impl BodyLine {
    pub(crate) fn plain(text: String) -> Self {
        BodyLine {
            text,
            marked: false,
        }
    }
}

/// The rows under `edit`'s first line.
pub(crate) fn detail(edit: &OrgEdit, org: &Org) -> Vec<BodyLine> {
    match edit {
        OrgEdit::AddTeam(d) => added(&d.defaults),
        OrgEdit::AddRole { role, agents, .. } => {
            let noun = if *agents == 1 { "agent" } else { "agents" };
            let mut rows = vec![BodyLine::plain(format!("{INDENT}{agents} {noun}"))];
            rows.extend(added(&role.defaults));
            rows
        }
        OrgEdit::AddAgent { agent, .. } => added(&agent.fields),
        OrgEdit::EditTeam { key, draft } => {
            let old = org
                .teams
                .iter()
                .find(|t| &t.name == key)
                .map(|t| &t.defaults);
            changed(key.as_str(), draft.name.as_str(), old, &draft.defaults)
        }
        OrgEdit::EditRole { key, draft } => {
            let old = find_role(org, key).map(|r| &r.defaults);
            changed(key.role.as_str(), draft.name.as_str(), old, &draft.defaults)
        }
        OrgEdit::EditAgent { key, draft } => {
            let old = org.find_agent(key).map(|a| &a.agent.fields);
            changed(key.as_str(), draft.name.as_str(), old, &draft.fields)
        }
        OrgEdit::EditOrgDefaults { defaults, duo } => defaults_changed(org, defaults, duo),
        OrgEdit::Delete(node) => removed(org, node).into_iter().collect(),
        OrgEdit::Move { node, to } => vec![BodyLine::plain(format!(
            "{INDENT}from {} → {}",
            place_of(org, node),
            target(org, to)
        ))],
    }
}

/// `+ field: value` for each field `own` sets.
fn added(own: &Fields) -> Vec<BodyLine> {
    let rows: Vec<BodyLine> = field_values(own)
        .into_iter()
        .filter_map(|(name, value)| {
            value.map(|v| BodyLine {
                text: format!("{INDENT}+ {name}: {v}"),
                marked: name == "flags",
            })
        })
        .collect();
    if rows.is_empty() {
        return vec![BodyLine::plain(format!("{INDENT}every field inherited"))];
    }
    rows
}

/// `~ field: old → new` for each field that differs, a rename first.
fn changed(old_name: &str, new_name: &str, old: Option<&Fields>, new: &Fields) -> Vec<BodyLine> {
    let blank = Fields::default();
    let before = field_values(old.unwrap_or(&blank));
    let after = field_values(new);
    let mut rows = Vec::new();
    if old_name != new_name {
        rows.extend(edit_row("name", old_name, new_name));
    }
    for ((name, was), (_, now)) in before.into_iter().zip(after) {
        if was != now {
            let was = was.unwrap_or_else(|| INHERIT.to_owned());
            let now = now.unwrap_or_else(|| INHERIT.to_owned());
            rows.extend(edit_row(name, &was, &now));
        }
    }
    or_no_changes(rows)
}

/// The org defaults' and Duo's changes.
fn defaults_changed(org: &Org, defaults: &OrgDefaults, duo: &DuoConfig) -> Vec<BodyLine> {
    let opt = |v: &Option<String>| v.as_deref().map_or_else(|| UNSET.to_owned(), one_line);
    let prompt = |v: &Option<String>| v.as_deref().map_or_else(|| UNSET.to_owned(), prompt_value);
    let old = &org.defaults;
    let pairs = [
        (
            "cli",
            old.cli.display().to_owned(),
            defaults.cli.display().to_owned(),
        ),
        ("model", opt(&old.model), opt(&defaults.model)),
        ("effort", opt(&old.effort), opt(&defaults.effort)),
        ("worktree", on_off(old.worktree), on_off(defaults.worktree)),
        ("prompt", prompt(&old.prompt), prompt(&defaults.prompt)),
        ("Duo name", org.duo.name.to_string(), duo.name.to_string()),
        (
            "Duo cli",
            org.duo.cli.display().to_owned(),
            duo.cli.display().to_owned(),
        ),
    ];
    let rows = pairs
        .into_iter()
        .filter(|(_, was, now)| was != now)
        .flat_map(|(name, was, now)| edit_row(name, &was, &now))
        .collect();
    or_no_changes(rows)
}

/// `~ name: old → new`, or, wider than a card, the new value hung on a second row.
fn edit_row(name: &str, was: &str, now: &str) -> Vec<BodyLine> {
    let marked = name == "flags";
    let one = format!("{INDENT}~ {name}: {was} → {now}");
    if marked || width(&one) <= ROW_CELLS {
        return vec![BodyLine { text: one, marked }];
    }
    vec![
        BodyLine::plain(format!("{INDENT}~ {name}: {was}")),
        BodyLine::plain(format!("{HANG_NEW}→ {now}")),
    ]
}

fn or_no_changes(rows: Vec<BodyLine>) -> Vec<BodyLine> {
    if rows.is_empty() {
        return vec![BodyLine::plain(format!("{INDENT}no changes"))];
    }
    rows
}

/// Each field by name, in UI §4.6's order, with its shown value when set.
fn field_values(f: &Fields) -> [(&'static str, Option<String>); 7] {
    let text = |v: &Option<String>| v.as_deref().map(one_line);
    [
        ("cli", f.cli.map(|c| c.display().to_owned())),
        ("flags", text(&f.flags)),
        ("directory", text(&f.directory)),
        ("model", text(&f.model)),
        ("effort", text(&f.effort)),
        ("worktree", f.worktree.map(on_off)),
        ("prompt", f.prompt.as_deref().map(prompt_value)),
    ]
}

fn on_off(on: bool) -> String {
    if on { "on" } else { "off" }.to_owned()
}

/// A prompt's first line, cut, then `(<n> lines)` when it has more than one.
fn prompt_value(prompt: &str) -> String {
    let first = prompt.lines().next().unwrap_or_default().trim();
    let shown = if first.is_empty() {
        "(empty)".to_owned()
    } else {
        truncate_end(first, PROMPT_CELLS)
    };
    match prompt.lines().count() {
        0 | 1 => shown,
        n => format!("{shown} ({n} lines)"),
    }
}

/// What a remove takes with it: `with 2 roles, 5 agents`.
fn removed(org: &Org, node: &NodeKey) -> Option<BodyLine> {
    let count =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let text = match node {
        NodeKey::Team(t) => {
            let team = org.teams.iter().find(|x| &x.name == t)?;
            let agents = team.roles.iter().map(|r| r.agents.len()).sum();
            format!(
                "with {}, {}",
                count(team.roles.len(), "role", "roles"),
                count(agents, "agent", "agents")
            )
        }
        NodeKey::Role(key) => {
            let role = find_role(org, key)?;
            format!("with {}", count(role.agents.len(), "agent", "agents"))
        }
        NodeKey::Agent(_) | NodeKey::Duo => return None,
    };
    Some(BodyLine::plain(format!("{INDENT}{text}")))
}

/// Where `node` sits now, in a move target's words.
fn place_of(org: &Org, node: &NodeKey) -> String {
    match node {
        NodeKey::Duo => org.duo.name.to_string(),
        NodeKey::Agent(a) => agent_place(org, a),
        NodeKey::Role(key) => key
            .team
            .as_ref()
            .map_or_else(|| org.duo.name.to_string(), ToString::to_string),
        NodeKey::Team(t) => {
            let next = org
                .teams
                .iter()
                .skip_while(|x| &x.name != t)
                .nth(1)
                .map(|x| x.name.clone());
            next.map_or_else(|| "the end".to_owned(), |n| format!("before {n}"))
        }
    }
}

fn agent_place(org: &Org, agent: &AgentName) -> String {
    org.find_agent(agent).map_or_else(String::new, |loc| {
        org.breadcrumb(&NodeKey::Role(RoleKey {
            team: loc.team.map(|t| t.name.clone()),
            role: loc.role.name.clone(),
        }))
    })
}

fn target(org: &Org, to: &MoveTarget) -> String {
    match to {
        MoveTarget::Into(node) => org.breadcrumb(node),
        MoveTarget::Before(team) => format!("before {team}"),
        MoveTarget::End => "the end".to_owned(),
    }
}

pub(crate) fn find_role<'a>(org: &'a Org, key: &RoleKey) -> Option<&'a Role> {
    let roles = match &key.team {
        Some(t) => &org.teams.iter().find(|x| &x.name == t)?.roles,
        None => &org.roles,
    };
    roles.iter().find(|r| r.name == key.role)
}
