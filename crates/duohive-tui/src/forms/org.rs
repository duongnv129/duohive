//! Edit org (UI §4.4.1): Duo's name and CLI, and the org-level defaults. A new CLI for a
//! running Duo restarts it, so the form asks first.

use duohive_core::cli_kind::CliKind;
use duohive_core::command::OrgEdit;
use duohive_core::event::AgentView;
use duohive_core::names::AgentName;
use duohive_core::org::{DuoConfig, Org, OrgDefaults};
use duohive_core::status::AgentState;

use super::{Field, FieldId, Form, Look, Target, confirm};
use crate::model::Confirm;

/// The Edit org mock-up's width.
const WIDTH: u16 = 80;
const LABEL: u16 = 12;
const ON: usize = 1;

fn look(inner: u16) -> Look {
    Look {
        indent: 4,
        label_w: LABEL,
        inner,
        suffix_gap: 3,
        ..Look::default()
    }
}

/// Model and Effort follow the CLI on its line, as the mock-up draws them.
fn inline(label: &str, inner: u16) -> Look {
    Look {
        same_row: true,
        label_w: u16::try_from(label.len() + 1).unwrap_or(0),
        ..look(inner)
    }
}

fn cli_names() -> Vec<String> {
    CliKind::ALL
        .iter()
        .map(|c| c.display().to_owned())
        .collect()
}

fn index_of(cli: CliKind) -> usize {
    CliKind::ALL.iter().position(|c| *c == cli).unwrap_or(0)
}

impl Form {
    /// Edits the org defaults and Duo. Duo's view decides whether its name can change.
    pub fn edit_org(org: &Org, duo: &AgentView) -> Form {
        let d = &org.defaults;
        let name_look = Look {
            heading: Some(org.duo.name.to_string()),
            ..look(20)
        };
        let mut runs_on = Field::choices(
            FieldId::DuoCli,
            "Runs on",
            cli_names(),
            index_of(org.duo.cli),
            look(0),
        );
        runs_on.suffix = Some(format!(
            "changing it restarts {} (asks first)",
            org.duo.name
        ));
        let cli_look = Look {
            heading: Some("Defaults for every agent".to_owned()),
            ..look(0)
        };
        let prompt_look = Look {
            indent: 2,
            label_w: LABEL + 2,
            ..look(46)
        };
        let worktrees = Field::choices(
            FieldId::Worktrees,
            "Worktrees",
            vec!["off".to_owned(), "on".to_owned()],
            usize::from(d.worktree),
            look(0),
        );
        let fields = vec![
            Field::text(
                FieldId::Name,
                "Name",
                Some(org.duo.name.as_str()),
                name_look,
            ),
            runs_on,
            Field::choices(FieldId::Cli, "CLI", cli_names(), index_of(d.cli), cli_look),
            Field::text(
                FieldId::Model,
                "Model",
                d.model.as_deref(),
                inline("Model", 10),
            ),
            Field::text(
                FieldId::Effort,
                "Effort",
                d.effort.as_deref(),
                inline("Effort", 6),
            ),
            worktrees,
            Field::prompt("Org prompt", d.prompt.as_deref(), prompt_look),
        ];
        let target = Target::Org {
            duo_state: duo.status.state,
        };
        let title = "Edit org · saved to org.toml".to_owned();
        let mut f = Form::build(title, target, fields, WIDTH);
        if duo.status.state != AgentState::Stopped {
            super::agent::lock_name(&mut f);
        }
        f.revalidate(org);
        f
    }
}

fn cli_at(f: &Form, id: FieldId) -> Option<CliKind> {
    CliKind::ALL.get(f.field(id)?.selected).copied()
}

/// `EditOrgDefaults`, once Duo's name parses.
pub(super) fn edit(f: &Form, org: &Org) -> Option<OrgEdit> {
    let own = f.fields_value();
    let defaults = OrgDefaults {
        cli: cli_at(f, FieldId::Cli)?,
        model: own.model,
        effort: own.effort,
        worktree: f.field(FieldId::Worktrees)?.selected == ON,
        prompt: own.prompt,
    };
    let duo = DuoConfig {
        name: AgentName::parse(f.field(FieldId::Name)?.edit.text()).ok()?,
        cli: cli_at(f, FieldId::DuoCli).unwrap_or(org.duo.cli),
    };
    Some(OrgEdit::EditOrgDefaults { defaults, duo })
}

/// `Restart Duo on <CLI>?` when the edit changes a running Duo's CLI.
pub(super) fn restart_confirm(f: &Form, org: &Org, edit: &OrgEdit) -> Option<Confirm> {
    let Target::Org { duo_state } = f.target else {
        return None;
    };
    let OrgEdit::EditOrgDefaults { duo, .. } = edit else {
        return None;
    };
    if duo.cli == org.duo.cli || !duo_state.is_running() {
        return None;
    }
    let cli = duo.cli.display();
    let name = &org.duo.name;
    let line = format!(
        "{name} is {}. Saving stops it and starts it again on {cli}.",
        duo_state.word()
    );
    Some(confirm(
        format!("Restart {name} on {cli}?"),
        vec![line],
        "Restart",
        edit.clone(),
    ))
}
