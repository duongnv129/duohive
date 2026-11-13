//! Keys and refreshes for a Directory field's folder popup (UI §4.4.1, DSP-D2, DSP-D3).

use std::path::Path;

use duohive_core::org::Org;
use duohive_core::validate::DirProbe;

use super::{FieldId, Form, FormOutcome, LineEdit};
use crate::input::Action;

impl Form {
    /// Keys for a Directory field whose popup is open: `↑↓` move the highlight, `tab` and
    /// `enter` take it, `esc` closes only the popup. Any other key, and every key while the
    /// popup is closed, is left to the form (`None`).
    pub(super) fn dir_key(&mut self, a: Action, org: &Org) -> Option<FormOutcome> {
        let f = self.fields.get_mut(self.focus)?;
        if f.id != FieldId::Directory || !f.suggest.open {
            return None;
        }
        match a {
            Action::Esc => f.suggest.close(),
            Action::Up => f.suggest.step(false),
            Action::Down => f.suggest.step(true),
            Action::Tab | Action::DialogEnter => {
                let taken = f.suggest.take()?;
                f.edit = LineEdit::new(&taken);
                f.touched = true;
                self.revalidate(org);
            }
            _ => return None,
        }
        Some(FormOutcome::Continue)
    }

    /// Lists folders for the focused Directory field when its text changed, and closes the
    /// popup of every other one. Call it after a key or a paste, as `probe_dir`.
    pub fn refresh_suggest(&mut self, dirs: &dyn DirProbe, home: &Path) {
        let focus = self.focus;
        for (i, f) in self.fields.iter_mut().enumerate() {
            if f.id != FieldId::Directory {
                continue;
            }
            let typed = f.edit.text().to_owned();
            if i == focus {
                f.suggest.refresh(&typed, dirs, home);
            } else {
                f.suggest.reset(&typed);
            }
        }
    }

    /// Closes every popup and takes the text now in the fields as already seen, so focus
    /// alone never opens one.
    pub(super) fn reset_suggest(&mut self) {
        for f in self
            .fields
            .iter_mut()
            .filter(|f| f.id == FieldId::Directory)
        {
            let typed = f.edit.text().to_owned();
            f.suggest.reset(&typed);
        }
    }
}
