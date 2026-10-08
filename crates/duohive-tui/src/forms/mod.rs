//! Team, role, agent and Edit org forms, and the New chooser (UI §4.4.1–4.4.2, D11, D14).
//!
//! A form validates on every change: field-local checks (D4 names, counts, worktrees) and
//! the core's `validate` on the org with the draft applied. The engine stays the authority:
//! a saved form waits for it, and a rejection lands on the field it names.

mod agent;
mod chooser;
mod field;
mod give_work;
mod line_edit;
mod org;
mod role;
mod team;

use std::path::Path;

use duohive_core::cli_kind::CliKind;
use duohive_core::command::{Command, OrgEdit};
use duohive_core::engine::preview_edit;
use duohive_core::inherit::{Inherited, expand_home};
use duohive_core::names::{AgentName, NameError, NodeName};
use duohive_core::org::{Fields, Org, RoleKey};
use duohive_core::status::AgentState;
use duohive_core::validate::{DirProbe, DirStatus};

pub use chooser::{Chooser, ChooserItem, NewKind};
pub use field::{Field, FieldId, FieldKind, Look};
pub use give_work::{GiveFocus, GiveOutcome, GiveWork, Variant};
pub use line_edit::LineEdit;

use crate::input::Action;
use crate::model::Confirm;

/// Under a name that can't change now (Task 14's rule).
pub(crate) const RENAME_WHILE_STOPPED: &str = "rename only while stopped with no worktrees";
const INHERIT: &str = "inherit: ";
/// Worktree choices where the field inherits.
const WT_ON: usize = 1;
const WT_OFF: usize = 2;

/// What a key did to a form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormOutcome {
    /// Save: send this edit.
    Edit(OrgEdit),
    /// Close without saving.
    Cancel,
    /// Keep the form open.
    Continue,
    /// Ask this first (Duo's CLI changes while it runs).
    Confirm(Confirm),
}

/// What a form creates or changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    NewAgent(RoleKey),
    EditAgent(AgentName),
    NewRole(Option<NodeName>),
    EditRole(RoleKey),
    NewTeam,
    EditTeam(NodeName),
    Org { duo_state: AgentState },
}

/// An open form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    /// The title in the top border.
    pub title: String,
    /// The fields, top to bottom.
    pub fields: Vec<Field>,
    /// The focused field; `fields.len()` is Save and `fields.len() + 1` Cancel.
    pub focus: usize,
    /// The prompts an agent stacks under, `(level, prompt)`, read-only (UI §4.4.1).
    pub stacked: Vec<(String, Option<String>)>,
    /// A line above the buttons (`<agent> is running: changes apply on next start`).
    pub footer: Option<String>,
    /// The engine refused the last save for a reason no field owns.
    pub rejected: Option<String>,
    /// Saved and waiting for the engine: the next `OrgChanged` closes it.
    pub saving: bool,
    /// The save button's label: `Create` or `Save`.
    pub save_label: String,
    /// The box's width, from the mock-ups.
    pub width: u16,
    pub(crate) target: Target,
    /// The directory the form inherits, when it has a Directory field.
    pub(crate) inherited_dir: Option<String>,
    /// The last directory probed and what was found.
    pub(crate) probed: Option<(String, DirStatus)>,
}

impl Form {
    fn build(title: String, target: Target, fields: Vec<Field>, width: u16) -> Self {
        let save_label = match target {
            Target::NewAgent(_) | Target::NewRole(_) | Target::NewTeam => "Create",
            _ => "Save",
        };
        let mut f = Self {
            title,
            fields,
            focus: 0,
            stacked: Vec::new(),
            footer: None,
            rejected: None,
            saving: false,
            save_label: save_label.to_owned(),
            width,
            target,
            inherited_dir: None,
            probed: None,
        };
        f.focus = f.fields.iter().position(Field::focusable).unwrap_or(0);
        f
    }

    /// Finishes construction: probes the directory and validates.
    fn ready(mut self, org: &Org, dirs: &dyn DirProbe, home: &Path) -> Self {
        self.revalidate(org);
        self.probe_dir(org, dirs, home);
        self
    }

    /// The field editing `id`.
    pub fn field(&self, id: FieldId) -> Option<&Field> {
        self.fields.iter().find(|f| f.id == id)
    }

    fn field_mut(&mut self, id: FieldId) -> Option<&mut Field> {
        self.fields.iter_mut().find(|f| f.id == id)
    }

    /// Moves focus to `id`'s field.
    pub fn focus_field(&mut self, id: FieldId) {
        if let Some(i) = self.fields.iter().position(|f| f.id == id) {
            self.focus = i;
        }
    }

    /// Save has focus.
    pub fn on_save(&self) -> bool {
        self.focus == self.fields.len()
    }

    /// Cancel has focus.
    pub fn on_cancel(&self) -> bool {
        self.focus == self.fields.len() + 1
    }

    /// Errors left to fix before Save works. A refusal no field owns isn't one: Save
    /// stays available to retry it.
    pub fn error_count(&self) -> usize {
        self.fields.iter().filter(|f| f.error.is_some()).count()
    }

    /// Applies a key. Tab order skips read-only fields and ends on Save, then Cancel.
    pub fn handle(&mut self, a: Action, org: &Org) -> FormOutcome {
        match a {
            Action::Esc => return FormOutcome::Cancel,
            Action::Save => return self.save(org),
            Action::Tab => self.step(true),
            Action::BackTab => self.step(false),
            Action::DialogEnter if self.on_save() => return self.save(org),
            Action::DialogEnter if self.on_cancel() => return FormOutcome::Cancel,
            Action::DialogEnter => self.step(true),
            _ => {
                let changed = self.fields.get_mut(self.focus).is_some_and(|f| f.apply(a));
                if changed {
                    self.revalidate(org);
                }
            }
        }
        FormOutcome::Continue
    }

    /// Pastes into the focused field.
    pub fn paste(&mut self, text: &str, org: &Org) {
        let changed = self
            .fields
            .get_mut(self.focus)
            .is_some_and(|f| f.paste(text));
        if changed {
            self.revalidate(org);
        }
    }

    /// Takes the engine's refusal of the last save: on the field it names, else on the form.
    pub fn reject(&mut self, reason: String) {
        self.saving = false;
        let owner = if reason.starts_with("flags:") {
            Some(FieldId::Flags)
        } else if [
            "an agent named",
            "a role named",
            "a team named",
            "name ",
            RENAME_WHILE_STOPPED,
        ]
        .iter()
        .any(|p| reason.starts_with(p))
        {
            Some(FieldId::Name)
        } else {
            None
        };
        match owner.and_then(|id| self.field_mut(id)) {
            Some(f) => f.error = Some(reason),
            None => self.rejected = Some(reason),
        }
    }

    fn step(&mut self, forward: bool) {
        let stops = self.fields.len() + 2;
        let mut i = self.focus;
        for _ in 0..stops {
            i = if forward {
                (i + 1) % stops
            } else {
                (i + stops - 1) % stops
            };
            if self.fields.get(i).is_none_or(Field::focusable) {
                self.focus = i;
                return;
            }
        }
    }

    fn save(&mut self, org: &Org) -> FormOutcome {
        if self.saving {
            return FormOutcome::Continue;
        }
        for f in &mut self.fields {
            f.touched = true;
        }
        // Everything else was checked on the last change; an engine refusal stays until then.
        self.check_name(org);
        if self.error_count() > 0 {
            return FormOutcome::Continue;
        }
        let Some(edit) = self.edit(org) else {
            return FormOutcome::Continue;
        };
        match org::restart_confirm(self, org, &edit) {
            Some(c) => FormOutcome::Confirm(c),
            None => {
                self.rejected = None;
                FormOutcome::Edit(edit)
            }
        }
    }

    /// The edit the form describes now, or `None` while a name or count doesn't parse.
    pub fn edit(&self, org: &Org) -> Option<OrgEdit> {
        match &self.target {
            Target::NewAgent(_) | Target::EditAgent(_) => agent::edit(self),
            Target::NewRole(_) | Target::EditRole(_) => role::edit(self),
            Target::NewTeam | Target::EditTeam(_) => team::edit(self),
            Target::Org { .. } => org::edit(self, org),
        }
    }

    /// The settings the fields hold; blank fields inherit.
    pub(crate) fn fields_value(&self) -> Fields {
        let text = |id| self.field(id).and_then(Field::value);
        Fields {
            cli: self
                .field(FieldId::Cli)
                .and_then(|f| inherit_or(f, |i| CliKind::ALL.get(i).copied())),
            flags: text(FieldId::Flags),
            directory: text(FieldId::Directory),
            model: text(FieldId::Model),
            effort: text(FieldId::Effort),
            worktree: self
                .field(FieldId::Worktrees)
                .and_then(|f| match f.selected {
                    WT_ON => Some(true),
                    WT_OFF => Some(false),
                    _ => None,
                }),
            prompt: self.field(FieldId::Prompt).and_then(|f| {
                let t = f.edit.text();
                (!t.trim().is_empty()).then(|| t.to_owned())
            }),
        }
    }

    /// Re-checks every field after a change; engine refusals are cleared.
    fn revalidate(&mut self, org: &Org) {
        self.rejected = None;
        for f in &mut self.fields {
            f.error = None;
        }
        self.check_name(org);
        role::check_agents(self, org);
        self.check_worktrees();
        if let Some(edit) = self.edit(org) {
            self.engine_errors(org, &edit);
        }
    }

    fn check_name(&mut self, org: &Org) {
        let Some(name) = self.field(FieldId::Name) else {
            return;
        };
        if name.kind == FieldKind::ReadOnly {
            return;
        }
        let text = name.edit.text().to_owned();
        let touched = name.touched;
        let error = match &self.target {
            Target::NewAgent(_) | Target::EditAgent(_) | Target::Org { .. } => {
                match AgentName::parse(&text) {
                    Ok(n) => self.name_taken(org, &n),
                    Err(e) => name_error(&e, touched),
                }
            }
            _ => NodeName::parse(&text)
                .err()
                .and_then(|e| name_error(&e, touched)),
        };
        if let (Some(f), Some(e)) = (self.field_mut(FieldId::Name), error) {
            f.error = Some(e);
        }
    }

    /// D4: an agent name is unique across the org and isn't Duo's.
    fn name_taken(&self, org: &Org, name: &AgentName) -> Option<String> {
        let own = match &self.target {
            Target::EditAgent(key) => Some(key),
            Target::Org { .. } => Some(&org.duo.name),
            _ => None,
        };
        let taken = std::iter::once(&org.duo.name)
            .chain(org.agents().into_iter().map(|a| &a.agent.name))
            .any(|n| n == name && Some(n) != own);
        taken.then(|| format!("an agent named {name} already exists"))
    }

    /// The core's checks on the org with the draft applied, on the fields they name.
    fn engine_errors(&mut self, org: &Org, edit: &OrgEdit) {
        let Some(node) = node_path(org, edit) else {
            return;
        };
        let Err(errors) = preview_edit(org, edit) else {
            return;
        };
        for e in errors {
            if e.path.is_empty() {
                // The node is gone (an org reload while the form was open).
                self.rejected = Some(e.message);
                continue;
            }
            let id = match e.path.strip_prefix(node.as_str()) {
                Some(".name") => FieldId::Name,
                Some(".flags") => FieldId::Flags,
                _ => continue,
            };
            if let Some(f) = self.field_mut(id).filter(|f| f.error.is_none()) {
                f.error = Some(e.message);
            }
        }
    }

    /// The directory agents would run in: the field's, else the inherited one.
    fn resolved_dir(&self) -> Option<String> {
        self.field(FieldId::Directory)?;
        self.field(FieldId::Directory)
            .and_then(Field::value)
            .or_else(|| self.inherited_dir.clone())
    }

    /// Probes the resolved directory when it changed, then re-checks the worktree choice.
    pub fn probe_dir(&mut self, org: &Org, dirs: &dyn DirProbe, home: &Path) {
        let dir = self.resolved_dir();
        let same = self.probed.as_ref().map(|(d, _)| d) == dir.as_ref();
        if same {
            return;
        }
        self.probed = dir.map(|d| {
            let status = dirs.probe(&expand_home(&d, home));
            (d, status)
        });
        let suffix = self.probed.as_ref().map(|(_, s)| {
            match s {
                DirStatus::GitRepo => "✓ git repo",
                DirStatus::Found => "✓ found",
                DirStatus::Missing => "✕ not found",
            }
            .to_owned()
        });
        if let Some(f) = self.field_mut(FieldId::Directory) {
            f.suffix = suffix;
        }
        self.revalidate(org);
    }

    /// `on` needs a git repo: disabled, with the reason, while the directory isn't one.
    fn check_worktrees(&mut self) {
        let reason = match &self.probed {
            Some((d, status)) if *status != DirStatus::GitRepo => {
                Some(format!("new worktree needs a git repo; {d} isn't one"))
            }
            _ => None,
        };
        if matches!(self.target, Target::Org { .. }) {
            return;
        }
        let Some(f) = self.field_mut(FieldId::Worktrees) else {
            return;
        };
        f.disabled = if reason.is_some() {
            vec![WT_ON]
        } else {
            Vec::new()
        };
        // Blocks only when turned on in this form; `on` already saved stays, with the note.
        if f.selected == WT_ON && f.initial != WT_ON && reason.is_some() {
            f.error = reason;
            f.note = None;
        } else {
            f.note = reason;
        }
    }
}

/// An empty name is an error only once the field was touched.
fn name_error(e: &NameError, touched: bool) -> Option<String> {
    (touched || *e != NameError::Empty).then(|| e.to_string())
}

/// The chosen entry, where entry 0 is `inherit: …`: `None` inherits.
fn inherit_or<T>(f: &Field, pick: impl Fn(usize) -> Option<T>) -> Option<T> {
    f.selected.checked_sub(1).and_then(pick)
}

/// `inherit: <value>`.
pub(crate) fn inherit_label(value: &str) -> String {
    format!("{INHERIT}{value}")
}

/// `inherit: <value>` for a value the level would inherit, else nothing.
pub(crate) fn placeholder<T: ToString>(
    v: Option<&duohive_core::inherit::InheritedValue<T>>,
) -> Option<String> {
    v.map(|v| inherit_label(&v.value.to_string()))
}

/// The CLI choice: `inherit: <cli>`, then every CLI.
pub(crate) fn cli_choice(inherited: &Inherited, own: Option<CliKind>, look: Look) -> Field {
    let options = std::iter::once(inherit_label(inherited.cli.value.display()))
        .chain(CliKind::ALL.iter().map(|c| c.display().to_owned()))
        .collect();
    let selected = own
        .and_then(|c| CliKind::ALL.iter().position(|k| *k == c))
        .map_or(0, |i| i + 1);
    Field::choices(FieldId::Cli, "CLI", options, selected, look)
}

/// The worktree choice: `inherit: on|off`, `on`, `off`.
pub(crate) fn worktree_choice(inherited: &Inherited, own: Option<bool>, look: Look) -> Field {
    let on_off = |b: bool| if b { "on" } else { "off" };
    let options = vec![
        inherit_label(on_off(inherited.worktree.value)),
        "on".to_owned(),
        "off".to_owned(),
    ];
    let selected = match own {
        None => 0,
        Some(true) => WT_ON,
        Some(false) => WT_OFF,
    };
    Field::choices(FieldId::Worktrees, "Worktrees", options, selected, look)
}

/// `team[T].role[R]` or `role[R]`: validate's path for a role.
fn role_path(team: Option<&NodeName>, role: &NodeName) -> String {
    match team {
        Some(t) => format!("team[{t}].role[{role}]"),
        None => format!("role[{role}]"),
    }
}

/// Validate's path for the node an edit leaves behind; `None` for the org form.
fn node_path(org: &Org, edit: &OrgEdit) -> Option<String> {
    Some(match edit {
        OrgEdit::AddTeam(d) => format!("team[{}]", d.name),
        OrgEdit::EditTeam { draft, .. } => format!("team[{}]", draft.name),
        OrgEdit::AddRole { team, role, .. } => role_path(team.as_ref(), &role.name),
        OrgEdit::EditRole { key, draft } => role_path(key.team.as_ref(), &draft.name),
        OrgEdit::AddAgent { role, agent } => {
            format!(
                "{}.agent[{}]",
                role_path(role.team.as_ref(), &role.role),
                agent.name
            )
        }
        OrgEdit::EditAgent { key, draft } => {
            let loc = org.find_agent(key)?;
            let team = loc.team.map(|t| &t.name);
            format!("{}.agent[{}]", role_path(team, &loc.role.name), draft.name)
        }
        OrgEdit::EditOrgDefaults { .. } | OrgEdit::Delete(_) => return None,
    })
}

/// A confirmation that opens on Cancel and returns to the form on cancel.
pub(crate) fn confirm(title: String, lines: Vec<String>, yes: &str, edit: OrgEdit) -> Confirm {
    Confirm {
        title,
        lines,
        yes_label: yes.to_owned(),
        focus_yes: false,
        on_yes: Command::EditOrg(edit),
        back: None,
        back_list: None,
    }
}

#[cfg(test)]
mod tests;
