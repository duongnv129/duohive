//! One form field: what it holds, how keys and pastes change it, and where it is drawn.

use crate::forms::line_edit::LineEdit;
use crate::input::Action;

/// Which setting a field edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldId {
    /// The agent's, role's, team's or Duo's name.
    Name,
    /// The CLI (a default, for the org form).
    Cli,
    /// Duo's CLI ("Runs on").
    DuoCli,
    /// The working directory.
    Directory,
    /// The model (free text, D14).
    Model,
    /// The reasoning effort (free text, D14).
    Effort,
    /// Extra CLI flags.
    Flags,
    /// Worktrees on or off.
    Worktrees,
    /// This level's own prompt.
    Prompt,
    /// How many agents a new role creates (D11).
    Agents,
}

/// How a field is edited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// One line of text.
    Text,
    /// One of these, cycled with ←→; the first is `inherit: <value>` where the field inherits.
    Choice(Vec<String>),
    /// Text that keeps newlines (alt+enter).
    Prompt,
    /// Digits only.
    Count,
    /// Shown, never edited, never focused.
    ReadOnly,
}

/// Where and how wide a field is drawn; cells are counted inside the form's border.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Look {
    /// A line drawn above the field.
    pub heading: Option<String>,
    /// A blank line above the field (and its heading).
    pub blank_before: bool,
    /// Cells before the label.
    pub indent: u16,
    /// The label is padded to this many cells.
    pub label_w: u16,
    /// Cells of text inside `[ … ]`.
    pub inner: u16,
    /// Drawn on the previous field's line.
    pub same_row: bool,
    /// On a shared line: the label's cell; `None` follows the previous control.
    pub col: Option<u16>,
    /// The label is written inside the brackets (`[ Own: … ]`).
    pub label_inside: bool,
    /// Spaces between the control and its suffix.
    pub suffix_gap: u16,
}

/// One field of a form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// The setting it edits.
    pub id: FieldId,
    /// Its label.
    pub label: String,
    /// How it is edited.
    pub kind: FieldKind,
    /// Its text (empty for choices).
    pub edit: LineEdit,
    /// What is wrong with it, shown as `✕ <error>` under it.
    pub error: Option<String>,
    /// A muted line under it (why it is read-only, why a choice is disabled).
    pub note: Option<String>,
    /// Shown in `fg.muted` while the text is empty: `inherit: <value>`.
    pub placeholder: Option<String>,
    /// Shown after the control: `✓ git repo`, `create this many now (…)`.
    pub suffix: Option<String>,
    /// The chosen entry of a choice.
    pub selected: usize,
    /// The entry chosen when the form opened.
    pub initial: usize,
    /// Choice entries that can't be picked now.
    pub disabled: Vec<usize>,
    /// Changed by the user (an empty required name is an error only once touched).
    pub touched: bool,
    /// Placement.
    pub look: Look,
}

impl Field {
    fn new(id: FieldId, label: &str, kind: FieldKind, look: Look) -> Self {
        Self {
            id,
            label: label.to_owned(),
            kind,
            edit: LineEdit::default(),
            error: None,
            note: None,
            placeholder: None,
            suffix: None,
            selected: 0,
            initial: 0,
            disabled: Vec::new(),
            touched: false,
            look,
        }
    }

    /// A text field holding `value`, showing `placeholder` while empty.
    pub fn text(id: FieldId, label: &str, value: Option<&str>, look: Look) -> Self {
        Self {
            edit: LineEdit::new(value.unwrap_or_default()),
            ..Self::new(id, label, FieldKind::Text, look)
        }
    }

    /// A multi-line prompt field.
    pub fn prompt(label: &str, value: Option<&str>, look: Look) -> Self {
        Self {
            edit: LineEdit::new(value.unwrap_or_default()),
            ..Self::new(FieldId::Prompt, label, FieldKind::Prompt, look)
        }
    }

    /// A digits-only field.
    pub fn count(id: FieldId, label: &str, value: &str, look: Look) -> Self {
        Self {
            edit: LineEdit::new(value),
            ..Self::new(id, label, FieldKind::Count, look)
        }
    }

    /// A choice of `options`, `selected` chosen.
    pub fn choices(
        id: FieldId,
        label: &str,
        options: Vec<String>,
        selected: usize,
        look: Look,
    ) -> Self {
        Self {
            selected,
            initial: selected,
            ..Self::new(id, label, FieldKind::Choice(options), look)
        }
    }

    /// With a placeholder.
    pub fn with_placeholder(self, placeholder: Option<String>) -> Self {
        Self {
            placeholder,
            ..self
        }
    }

    /// The chosen entry's text; empty for a field that isn't a choice.
    pub fn choice(&self) -> &str {
        match &self.kind {
            FieldKind::Choice(options) => options.get(self.selected).map_or("", String::as_str),
            _ => "",
        }
    }

    /// The text trimmed, or `None` when blank: a blank field inherits.
    pub fn value(&self) -> Option<String> {
        let t = self.edit.text().trim();
        (!t.is_empty()).then(|| t.to_owned())
    }

    /// Whether tab stops here.
    pub fn focusable(&self) -> bool {
        self.kind != FieldKind::ReadOnly
    }

    /// Whether it holds editable text.
    fn is_text(&self) -> bool {
        matches!(
            self.kind,
            FieldKind::Text | FieldKind::Prompt | FieldKind::Count
        )
    }

    /// Applies an editing key; `true` when the value changed.
    pub fn apply(&mut self, a: Action) -> bool {
        if let FieldKind::Choice(options) = &self.kind {
            let n = options.len();
            if n == 0 {
                return false;
            }
            return match a {
                Action::Left => self.cycle(n, n - 1),
                Action::Right => self.cycle(n, 1),
                _ => false,
            };
        }
        if !self.is_text() {
            return false;
        }
        let before = self.edit.text().to_owned();
        match a {
            Action::Char(c) => self.type_char(c),
            Action::NewLine if self.kind == FieldKind::Prompt => self.edit.insert('\n'),
            Action::Backspace => self.edit.backspace(),
            Action::DeleteForward => self.edit.delete(),
            Action::Left => self.edit.left(),
            Action::Right => self.edit.right(),
            Action::Home => self.edit.home(),
            Action::End => self.edit.end(),
            _ => {}
        }
        self.changed(&before)
    }

    /// Pastes `text`: one-line fields turn line breaks and tabs into spaces, prompts keep
    /// newlines, counts keep digits; `true` when the value changed.
    pub fn paste(&mut self, text: &str) -> bool {
        let unified = text.replace("\r\n", "\n").replace('\r', "\n");
        let clean: String = match self.kind {
            FieldKind::Text => unified
                .chars()
                .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
                .filter(|c| !c.is_control())
                .collect(),
            FieldKind::Prompt => unified
                .chars()
                .map(|c| if c == '\t' { ' ' } else { c })
                .filter(|c| *c == '\n' || !c.is_control())
                .collect(),
            FieldKind::Count => unified.chars().filter(char::is_ascii_digit).collect(),
            FieldKind::Choice(_) | FieldKind::ReadOnly => return false,
        };
        let before = self.edit.text().to_owned();
        self.edit.insert_str(&clean);
        self.changed(&before)
    }

    fn type_char(&mut self, c: char) {
        let ok = match self.kind {
            FieldKind::Count => c.is_ascii_digit(),
            _ => !c.is_control(),
        };
        if ok {
            self.edit.insert(c);
        }
    }

    fn changed(&mut self, before: &str) -> bool {
        let changed = self.edit.text() != before;
        self.touched |= changed;
        changed
    }

    /// Steps `by` (mod `n`) to the next entry that isn't disabled.
    fn cycle(&mut self, n: usize, by: usize) -> bool {
        let next = (1..n)
            .map(|k| (self.selected + k * by) % n)
            .find(|i| !self.disabled.contains(i));
        match next {
            Some(i) => {
                self.selected = i;
                self.touched = true;
                true
            }
            None => false,
        }
    }
}
