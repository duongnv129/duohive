//! `↑` and `↓` inside the Own prompt (UI §4.4.1 *Moving*): they move the cursor between the
//! box's lines and leave it only from the first or last line.

use super::{FieldKind, Form, FormOutcome};
use crate::input::Action;

impl Form {
    /// Moves the prompt's cursor a line when the focused field is a prompt and has a line
    /// that way; `None` leaves the key to the form (which steps to the neighbouring field).
    pub(super) fn prompt_key(&mut self, a: Action) -> Option<FormOutcome> {
        let f = self.fields.get_mut(self.focus)?;
        if f.kind != FieldKind::Prompt {
            return None;
        }
        let cells = f.prompt_cells();
        let moved = match a {
            Action::Up => f.edit.line_up(cells),
            Action::Down => f.edit.line_down(cells),
            _ => return None,
        };
        moved.then_some(FormOutcome::Continue)
    }
}
