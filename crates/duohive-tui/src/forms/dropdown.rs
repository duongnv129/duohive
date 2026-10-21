//! The Model and Effort dropdowns (UI §4.4.1): a text field with a popup of the CLI's list.
//! The typed text is the value; a blank field inherits.

use std::collections::BTreeMap;

use duohive_core::adapter::ModelList;
use duohive_core::cli_kind::CliKind;

use super::INHERIT;
use super::field::{Field, FieldId};

/// Each CLI's list once read, or why it couldn't be (TD §5.2).
pub type Models = BTreeMap<CliKind, Result<ModelList, String>>;

/// Most option rows the popup shows at once.
pub const VISIBLE_ROWS: usize = 6;
/// Why OpenCode's Effort is locked until a model is known (UI §4.4.1).
pub const EFFORT_NEEDS_MODEL: &str = "OpenCode sets effort per model · choose a model";

/// One row of the popup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropRow {
    /// `inherit: <value>`: clears the field.
    Inherit(String),
    /// A listed model or effort.
    Option(String),
    /// `use "<typed>" as typed`: a value the list doesn't have.
    Typed(String),
}

impl DropRow {
    /// The row as drawn.
    pub fn label(&self) -> String {
        match self {
            Self::Inherit(l) | Self::Option(l) => l.clone(),
            Self::Typed(t) => format!("use \"{t}\" as typed"),
        }
    }
}

/// A dropdown's state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dropdown {
    /// The popup is showing.
    pub open: bool,
    /// The highlighted row.
    pub cursor: usize,
    /// The list for the CLI this field resolves to.
    pub options: Vec<String>,
    /// The CLI's list has not arrived yet.
    pub loading: bool,
    /// The text when the popup opened; `esc` puts it back.
    pub before: String,
}

impl Field {
    /// The popup's rows for the text now typed: inherit, the options containing it, and
    /// the typed text when it isn't one of them.
    pub fn drop_rows(&self) -> Vec<DropRow> {
        let typed = self.edit.text().trim();
        let needle = typed.to_lowercase();
        let inherit = self
            .placeholder
            .clone()
            .unwrap_or_else(|| "inherit".to_owned());
        let mut rows = vec![DropRow::Inherit(inherit)];
        rows.extend(
            self.drop
                .options
                .iter()
                .filter(|o| o.to_lowercase().contains(&needle))
                .cloned()
                .map(DropRow::Option),
        );
        if !typed.is_empty() && !self.drop.options.iter().any(|o| o == typed) {
            rows.push(DropRow::Typed(typed.to_owned()));
        }
        rows
    }

    /// Opens the popup on the first row.
    pub(super) fn drop_open(&mut self) {
        self.drop.open = true;
        self.drop.cursor = 0;
        self.drop.before = self.edit.text().to_owned();
    }

    /// Closes the popup; `restore` puts back the text it opened with.
    pub(super) fn drop_close(&mut self, restore: bool) {
        self.drop.open = false;
        if restore {
            self.edit = super::LineEdit::new(&self.drop.before.clone());
        }
    }

    /// Moves the highlight, staying inside the rows.
    pub(super) fn drop_move(&mut self, down: bool) {
        let last = self.drop_rows().len().saturating_sub(1);
        self.drop.cursor = if down {
            (self.drop.cursor + 1).min(last)
        } else {
            self.drop.cursor.saturating_sub(1)
        };
    }

    /// Takes the highlighted row as the value and closes the popup.
    pub(super) fn drop_choose(&mut self) {
        let rows = self.drop_rows();
        match rows.get(self.drop.cursor) {
            Some(DropRow::Inherit(_)) => self.edit = super::LineEdit::new(""),
            Some(DropRow::Option(o)) => self.edit = super::LineEdit::new(o),
            Some(DropRow::Typed(_)) | None => {}
        }
        self.touched = true;
        self.drop.open = false;
    }
}

/// The CLI a choice names, with or without the `inherit: ` lead.
pub fn cli_of(choice: &str) -> Option<CliKind> {
    let name = choice.strip_prefix(INHERIT).unwrap_or(choice);
    CliKind::ALL.into_iter().find(|c| c.display() == name)
}

/// The text after `inherit: `, which is the value the field would inherit.
fn inherited(f: &Field) -> Option<String> {
    f.placeholder
        .as_deref()
        .and_then(|p| p.strip_prefix(INHERIT))
        .map(str::to_owned)
}

/// Sets both dropdowns from `models` for the CLI the form resolves to. `model_changed`
/// clears an Effort the new model doesn't offer.
pub(super) fn refresh(
    fields: &mut [Field],
    models: &Models,
    cli: Option<CliKind>,
    model_changed: bool,
) {
    let entry = cli.and_then(|c| models.get(&c).map(|r| (c, r)));
    let list: Option<&ModelList> = entry.and_then(|(_, r)| r.as_ref().ok());
    let model = fields
        .iter()
        .find(|f| f.id == FieldId::Model)
        .and_then(|f| f.value().or_else(|| inherited(f)));
    for f in fields.iter_mut() {
        match f.id {
            FieldId::Model => {
                f.drop.options = list.map(|l| l.models.clone()).unwrap_or_default();
                f.drop.loading = cli.is_some() && entry.is_none();
                f.note = match entry {
                    Some((c, Err(_))) => Some(format!(
                        "✕ couldn't list {}'s models · type a name or keep inherit",
                        c.binary()
                    )),
                    _ => None,
                };
            }
            FieldId::Effort => {
                let per_model = cli == Some(CliKind::Opencode);
                f.drop.options = match (list, per_model, &model) {
                    (Some(l), false, _) => l.efforts.clone(),
                    (Some(l), true, Some(m)) => l.variants.get(m).cloned().unwrap_or_default(),
                    _ => Vec::new(),
                };
                f.locked = per_model && model.is_none();
                f.note = f.locked.then(|| EFFORT_NEEDS_MODEL.to_owned());
                if f.locked {
                    f.drop.open = false;
                }
                if model_changed
                    && !f.drop.options.is_empty()
                    && f.value().is_some_and(|v| !f.drop.options.contains(&v))
                {
                    f.edit = super::LineEdit::new("");
                }
            }
            _ => {}
        }
    }
}
