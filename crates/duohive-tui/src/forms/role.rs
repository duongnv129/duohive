//! New role and Edit role (UI §4.4.1, D11): the defaults its agents inherit, and on a new
//! role how many agents to create now. The team form is this form without Agents.

use std::path::Path;

use duohive_core::command::{OrgEdit, RoleDraft};
use duohive_core::inherit::inherited_for;
use duohive_core::names::NodeName;
use duohive_core::org::{Fields, NodeKey, Org, RoleKey};
use duohive_core::validate::{DirProbe, next_agent_names};

use super::{
    Field, FieldId, Form, Look, MAX_NEW_AGENTS, Target, cli_choice, placeholder, worktree_choice,
};

/// The New role mock-up's width.
const WIDTH: u16 = 72;
const LABEL: u16 = 13;
const DEFAULT_LABEL: u16 = 11;
const CREATE_NOW: &str = "create this many now";

fn top(inner: u16) -> Look {
    Look {
        indent: 2,
        label_w: LABEL,
        inner,
        suffix_gap: 1,
        ..Look::default()
    }
}

fn default_look(inner: u16) -> Look {
    Look {
        indent: 4,
        label_w: DEFAULT_LABEL,
        inner,
        suffix_gap: 2,
        ..Look::default()
    }
}

impl Form {
    /// A new role in `team`, or directly under Duo.
    pub fn new_role(org: &Org, team: Option<&NodeName>, dirs: &dyn DirProbe, home: &Path) -> Form {
        let title = match team {
            Some(t) => format!("New role · in {t}"),
            None => "New role".to_owned(),
        };
        let target = Target::NewRole(team.cloned());
        let parent = parent(team);
        let mut fields = group_fields(org, &parent, "", &Fields::default(), "Role prompt");
        fields.push(Field::count(FieldId::Agents, "Agents", "1", top(0)));
        group_form(org, &parent, title, target, fields).ready(org, dirs, home)
    }

    /// Edits the role at `key`.
    pub fn edit_role(org: &Org, key: &RoleKey, dirs: &dyn DirProbe, home: &Path) -> Form {
        let title = format!(
            "Edit role · {}",
            org.breadcrumb(&NodeKey::Role(key.clone()))
        );
        let own = find_role(org, key).unwrap_or_default();
        let parent = parent(key.team.as_ref());
        let fields = group_fields(org, &parent, key.role.as_str(), &own, "Role prompt");
        group_form(org, &parent, title, Target::EditRole(key.clone()), fields)
            .ready(org, dirs, home)
    }
}

fn parent(team: Option<&NodeName>) -> NodeKey {
    team.map_or(NodeKey::Duo, |t| NodeKey::Team(t.clone()))
}

fn find_role(org: &Org, key: &RoleKey) -> Option<Fields> {
    let roles = match &key.team {
        Some(t) => &org.teams.iter().find(|x| &x.name == t)?.roles,
        None => &org.roles,
    };
    roles
        .iter()
        .find(|r| r.name == key.role)
        .map(|r| r.defaults.clone())
}

/// Name, the defaults block and the level's prompt: the role and team forms' fields.
pub(super) fn group_fields(
    org: &Org,
    parent: &NodeKey,
    name: &str,
    own: &Fields,
    prompt_label: &str,
) -> Vec<Field> {
    let inherited = inherited_for(org, parent);
    let from = match parent {
        NodeKey::Team(t) => t.to_string(),
        _ => "the org".to_owned(),
    };
    let cli_look = Look {
        heading: Some(format!(
            "Defaults for its agents (empty = inherit from {from})"
        )),
        ..default_look(0)
    };
    vec![
        Field::text(FieldId::Name, "Name", Some(name), top(20)),
        cli_choice(&inherited, own.cli, cli_look),
        Field::text(
            FieldId::Directory,
            "Directory",
            own.directory.as_deref(),
            default_look(25),
        )
        .with_placeholder(placeholder(inherited.directory.as_ref())),
        Field::text(
            FieldId::Model,
            "Model",
            own.model.as_deref(),
            default_look(20),
        )
        .with_placeholder(placeholder(inherited.model.as_ref())),
        Field::text(
            FieldId::Effort,
            "Effort",
            own.effort.as_deref(),
            default_look(20),
        )
        .with_placeholder(placeholder(inherited.effort.as_ref())),
        Field::text(
            FieldId::Flags,
            "Flags",
            own.flags.as_deref(),
            default_look(25),
        )
        .with_placeholder(placeholder(inherited.flags.as_ref())),
        worktree_choice(&inherited, own.worktree, default_look(0)),
        Field::prompt(prompt_label, own.prompt.as_deref(), top(48)),
    ]
}

/// A role or team form over `fields`, inheriting from `parent`.
pub(super) fn group_form(
    org: &Org,
    parent: &NodeKey,
    title: String,
    target: Target,
    fields: Vec<Field>,
) -> Form {
    let mut f = Form::build(title, target, fields, WIDTH);
    f.inherited_dir = inherited_for(org, parent).directory.map(|d| d.value);
    f
}

/// `AddRole` or `EditRole`, once the name (and count) parse.
pub(super) fn edit(f: &Form) -> Option<OrgEdit> {
    let draft = RoleDraft {
        name: NodeName::parse(f.field(FieldId::Name)?.edit.text()).ok()?,
        defaults: f.fields_value(),
    };
    Some(match &f.target {
        Target::NewRole(team) => OrgEdit::AddRole {
            team: team.clone(),
            role: draft,
            agents: count(f)?,
        },
        Target::EditRole(key) => OrgEdit::EditRole {
            key: key.clone(),
            draft,
        },
        _ => return None,
    })
}

fn count(f: &Form) -> Option<u8> {
    f.field(FieldId::Agents)?
        .edit
        .text()
        .parse::<u8>()
        .ok()
        .filter(|n| *n <= MAX_NEW_AGENTS)
}

/// The Agents count is bounded, and its suffix names the first agent it creates (D11).
pub(super) fn check_agents(f: &mut Form, org: &Org) {
    let Target::NewRole(team) = &f.target else {
        return;
    };
    let first = f
        .field(FieldId::Name)
        .and_then(|n| NodeName::parse(n.edit.text()).ok())
        .and_then(|role| {
            next_agent_names(org, team.as_ref(), &role, 1)
                .into_iter()
                .next()
        });
    let valid = count(f).is_some();
    let Some(agents) = f.fields.iter_mut().find(|x| x.id == FieldId::Agents) else {
        return;
    };
    agents.suffix = Some(match first {
        Some(name) => format!("{CREATE_NOW} ({name} …)"),
        None => CREATE_NOW.to_owned(),
    });
    if !valid {
        agents.error = Some(format!("from 0 to {MAX_NEW_AGENTS}"));
    }
}
