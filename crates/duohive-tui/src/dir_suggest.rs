//! Folder suggestions for a Directory field (UI §4.4.1, DSP-D2 to DSP-D5): the popup's
//! rows and highlight, worked out from the text typed so far.

use std::path::Path;

use duohive_core::inherit::expand_home;
use duohive_core::validate::DirProbe;

/// One folder the popup offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestRow {
    /// What taking the row puts in the field: the typed parent, the name and a `/`.
    pub path: String,
    /// The row as drawn: the typed parent and the name.
    pub label: String,
    /// The folder is a git repository; the drawer adds a muted `git`.
    pub is_git: bool,
}

/// The popup of one Directory field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirSuggest {
    /// The popup is showing.
    pub open: bool,
    /// The highlighted row.
    pub cursor: usize,
    /// The folders offered.
    pub rows: Vec<SuggestRow>,
    /// The text the rows were last worked out for (DSP-D3).
    listed_for: String,
}

/// The typed path split after its last `/`: the parent as typed and the name started.
/// `None` unless the text starts with `~/` or `/`: a relative path gets no popup.
pub fn split_typed(typed: &str) -> Option<(&str, &str)> {
    if !(typed.starts_with("~/") || typed.starts_with('/')) {
        return None;
    }
    let cut = typed.rfind('/')? + 1;
    Some(typed.split_at(cut))
}

impl DirSuggest {
    /// Closes the popup and treats `typed` as already seen, so only a later change opens it.
    pub fn reset(&mut self, typed: &str) {
        self.close();
        self.rows.clear();
        typed.clone_into(&mut self.listed_for);
    }

    /// Lists the folders for `typed` when it differs from the text last listed: the popup
    /// opens with the first row highlighted, or closes when nothing matches.
    pub fn refresh(&mut self, typed: &str, dirs: &dyn DirProbe, home: &Path) {
        if typed == self.listed_for {
            return;
        }
        typed.clone_into(&mut self.listed_for);
        self.cursor = 0;
        self.rows = match split_typed(typed) {
            Some((parent, partial)) => dirs
                .list(&expand_home(parent, home), partial)
                .into_iter()
                .map(|e| SuggestRow {
                    path: format!("{parent}{}/", e.name),
                    label: format!("{parent}{}", e.name),
                    is_git: e.is_git,
                })
                .collect(),
            None => Vec::new(),
        };
        self.open = !self.rows.is_empty();
    }

    /// Closes the popup, keeping the text.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Moves the highlight one row, staying inside the rows.
    pub fn step(&mut self, down: bool) {
        let last = self.rows.len().saturating_sub(1);
        self.cursor = if down {
            (self.cursor + 1).min(last)
        } else {
            self.cursor.saturating_sub(1)
        };
    }

    /// `tab`: the highlighted folder's path with a `/` after it, closing the popup. The caller
    /// puts it in the field and refreshes, which lists the folder's children. `None` when the
    /// popup is closed.
    pub fn go_into(&mut self) -> Option<String> {
        if !self.open {
            return None;
        }
        self.close();
        self.rows.get(self.cursor).map(|r| r.path.clone())
    }

    /// `enter`: the highlighted folder as typed, with no `/` after it, closing the popup. The
    /// text counts as already listed, so the popup stays closed until the text changes and
    /// the folder's children are not listed (typing `/` lists them, DSP-D8). `None` when the
    /// popup is closed.
    pub fn pick(&mut self) -> Option<String> {
        if !self.open {
            return None;
        }
        self.close();
        let label = self.rows.get(self.cursor)?.label.clone();
        self.listed_for.clone_from(&label);
        Some(label)
    }
}

#[cfg(test)]
mod tests;
