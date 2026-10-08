//! New agent and Edit agent (UI §4.4.1, D14, D27).

use std::path::Path;

use duohive_core::command::{AgentDraft, OrgEdit};
use duohive_core::event::AgentView;
use duohive_core::inherit::inherited_for;
use duohive_core::names::AgentName;
use duohive_core::org::{Fields, NodeKey, Org, RoleKey};
use duohive_core::status::AgentState;
use duohive_core::validate::{DirProbe, next_agent_names};

use super::{
    Field, FieldId, FieldKind, Form, Look, RENAME_WHILE_STOPPED, Target, cli_choice, placeholder,
    worktree_choice,
};

/// The Edit agent mock-up's width.
const WIDTH: u16 = 80;
const INDENT: u16 = 2;
const LABEL: u16 = 12;
/// Effort shares Model's line, at the mock-up's column.
const EFFORT_COL: u16 = 45;
const EFFORT_LABEL: u16 = 8;
const SEP: &str = " › ";

fn look(inner: u16) -> Look {
    Look {
        indent: INDENT,
        label_w: LABEL,
        inner,
        suffix_gap: 2,
        ..Look::default()
    }
}

impl Form {
    /// A new agent in `role`, named by D11.
    pub fn new_agent(org: &Org, role: &RoleKey, dirs: &dyn DirProbe, home: &Path) -> Form {
        let name = next_agent_names(org, role.team.as_ref(), &role.role, 1)
            .into_iter()
            .next()
            .map(String::from)
            .unwrap_or_default();
        let parent = NodeKey::Role(role.clone());
        let title = format!("New agent · {}", org.breadcrumb(&parent));
        let target = Target::NewAgent(role.clone());
        build(org, &parent, title, target, &name, &Fields::default()).ready(org, dirs, home)
    }

    /// Edits `name`; renaming only while it is stopped and has no worktree row
    /// (`has_worktrees`), and a running agent is told when changes apply.
    pub fn edit_agent(
        org: &Org,
        name: &AgentName,
        view: &AgentView,
        has_worktrees: bool,
        dirs: &dyn DirProbe,
        home: &Path,
    ) -> Form {
        let (parent, own) = match org.find_agent(name) {
            Some(loc) => (
                NodeKey::Role(RoleKey {
                    team: loc.team.map(|t| t.name.clone()),
                    role: loc.role.name.clone(),
                }),
                loc.agent.fields.clone(),
            ),
            None => (NodeKey::Duo, Fields::default()),
        };
        let title = format!(
            "Edit agent · {}",
            org.breadcrumb(&NodeKey::Agent(name.clone()))
        );
        let target = Target::EditAgent(name.clone());
        let mut f = build(org, &parent, title, target, name.as_str(), &own);
        let state = view.status.state;
        if state != AgentState::Stopped || has_worktrees {
            lock_name(&mut f);
        }
        if state.is_running() {
            f.footer = Some(format!("{name} is running: changes apply on next start"));
        }
        f.ready(org, dirs, home)
    }
}

/// The name can't change now: read-only, with the reason under it.
pub(super) fn lock_name(f: &mut Form) {
    if let Some(n) = f.fields.iter_mut().find(|x| x.id == FieldId::Name) {
        n.kind = FieldKind::ReadOnly;
        n.note = Some(RENAME_WHILE_STOPPED.to_owned());
    }
    f.focus = f.fields.iter().position(Field::focusable).unwrap_or(0);
}

fn build(
    org: &Org,
    parent: &NodeKey,
    title: String,
    target: Target,
    name: &str,
    own: &Fields,
) -> Form {
    let inherited = inherited_for(org, parent);
    let levels: Vec<&str> = inherited.prompts.iter().map(|(l, _)| l.as_str()).collect();
    let prompt_look = Look {
        heading: Some(format!("Prompt  (stacks under: {})", levels.join(SEP))),
        blank_before: true,
        indent: INDENT,
        inner: 59,
        label_inside: true,
        ..Look::default()
    };
    let effort_look = Look {
        same_row: true,
        col: Some(EFFORT_COL),
        label_w: EFFORT_LABEL,
        ..look(18)
    };
    let fields = vec![
        Field::text(FieldId::Name, "Name", Some(name), look(20)),
        cli_choice(&inherited, own.cli, look(0)),
        Field::text(
            FieldId::Directory,
            "Directory",
            own.directory.as_deref(),
            look(28),
        )
        .with_placeholder(placeholder(inherited.directory.as_ref())),
        Field::text(FieldId::Model, "Model", own.model.as_deref(), look(18))
            .with_placeholder(placeholder(inherited.model.as_ref())),
        Field::text(
            FieldId::Effort,
            "Effort",
            own.effort.as_deref(),
            effort_look,
        )
        .with_placeholder(placeholder(inherited.effort.as_ref())),
        Field::text(FieldId::Flags, "Flags", own.flags.as_deref(), look(30))
            .with_placeholder(placeholder(inherited.flags.as_ref())),
        worktree_choice(&inherited, own.worktree, look(0)),
        Field::prompt("Own", own.prompt.as_deref(), prompt_look),
    ];
    let mut f = Form::build(title, target, fields, WIDTH);
    f.stacked = inherited.prompts.clone();
    f.inherited_dir = inherited.directory.map(|d| d.value);
    f
}

/// `AddAgent` or `EditAgent`, once the name parses.
pub(super) fn edit(f: &Form) -> Option<OrgEdit> {
    let name = AgentName::parse(f.field(FieldId::Name)?.edit.text()).ok()?;
    let draft = AgentDraft {
        name,
        fields: f.fields_value(),
    };
    Some(match &f.target {
        Target::NewAgent(role) => OrgEdit::AddAgent {
            role: role.clone(),
            agent: draft,
        },
        Target::EditAgent(key) => OrgEdit::EditAgent {
            key: key.clone(),
            draft,
        },
        _ => return None,
    })
}
