//! New team and Edit team: the role form without Agents (D11).

use std::path::Path;

use duohive_core::command::{OrgEdit, TeamDraft};
use duohive_core::names::NodeName;
use duohive_core::org::{Fields, NodeKey, Org};
use duohive_core::validate::DirProbe;

use super::role::{group_fields, group_form};
use super::{FieldId, Form, Target};

const PROMPT: &str = "Team prompt";

impl Form {
    /// A new team.
    pub fn new_team(org: &Org, dirs: &dyn DirProbe, home: &Path) -> Form {
        let fields = group_fields(org, &NodeKey::Duo, "", &Fields::default(), PROMPT);
        group_form(
            org,
            &NodeKey::Duo,
            "New team".to_owned(),
            Target::NewTeam,
            fields,
        )
        .ready(org, dirs, home)
    }

    /// Edits the team `key`.
    pub fn edit_team(org: &Org, key: &NodeName, dirs: &dyn DirProbe, home: &Path) -> Form {
        let own = org
            .teams
            .iter()
            .find(|t| &t.name == key)
            .map(|t| t.defaults.clone())
            .unwrap_or_default();
        let fields = group_fields(org, &NodeKey::Duo, key.as_str(), &own, PROMPT);
        let title = format!("Edit team · {key}");
        group_form(
            org,
            &NodeKey::Duo,
            title,
            Target::EditTeam(key.clone()),
            fields,
        )
        .ready(org, dirs, home)
    }
}

/// `AddTeam` or `EditTeam`, once the name parses.
pub(super) fn edit(f: &Form) -> Option<OrgEdit> {
    let draft = TeamDraft {
        name: NodeName::parse(f.field(FieldId::Name)?.edit.text()).ok()?,
        defaults: f.fields_value(),
    };
    Some(match &f.target {
        Target::NewTeam => OrgEdit::AddTeam(draft),
        Target::EditTeam(key) => OrgEdit::EditTeam {
            key: key.clone(),
            draft,
        },
        _ => return None,
    })
}
