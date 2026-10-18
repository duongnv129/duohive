//! The chat's suggestion list (UI §4.7): `@` lists agents with their state, `/` the
//! commands; typing filters by prefix, `tab` or `enter` takes, `esc` closes.

use unicode_segmentation::UnicodeSegmentation;

use crate::format::state_cell;
use crate::forms::LineEdit;
use crate::model::UiModel;

/// The commands `/` lists, with what each does (P3-D26).
const COMMANDS: [(&str, &str); 2] = [
    ("/help", "commands and keys"),
    ("/agents", "every agent and its state"),
];

/// What the list offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuggestKind {
    /// Agents, Duo first, then tree order.
    Agent,
    /// `/help` and `/agents`.
    Command,
}

/// The open list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggest {
    /// What it offers.
    pub kind: SuggestKind,
    /// Each item: the text it inserts, and its detail (a state cell, or what it does).
    pub items: Vec<(String, String)>,
    /// The highlighted item.
    pub cursor: usize,
}

/// The list for `input` with the cursor at grapheme `cursor`: open while the input starts
/// with `@` or `/` and nothing before the cursor is a space; `None` when nothing matches.
pub fn suggestions(input: &str, cursor: usize, m: &UiModel) -> Option<Suggest> {
    let before: String = input.graphemes(true).take(cursor).collect();
    if before.chars().any(char::is_whitespace) {
        return None;
    }
    let (kind, items) = if let Some(prefix) = before.strip_prefix('@') {
        let items: Vec<(String, String)> = m
            .agent_names()
            .into_iter()
            .filter(|a| starts_with(a.as_str(), prefix))
            .map(|a| {
                let cell = state_cell(&m.view(a).status, m.now, None);
                (format!("@{a}"), cell)
            })
            .collect();
        (SuggestKind::Agent, items)
    } else {
        let prefix = before.strip_prefix('/')?;
        let items: Vec<(String, String)> = COMMANDS
            .iter()
            .filter(|(c, _)| starts_with(c.trim_start_matches('/'), prefix))
            .map(|&(c, d)| (c.to_owned(), d.to_owned()))
            .collect();
        (SuggestKind::Command, items)
    };
    (!items.is_empty()).then_some(Suggest {
        kind,
        items,
        cursor: 0,
    })
}

/// Whether `name` starts with `prefix`, ignoring case.
fn starts_with(name: &str, prefix: &str) -> bool {
    name.to_lowercase().starts_with(&prefix.to_lowercase())
}

impl Suggest {
    /// Puts the highlighted item and a space in place of what is before the cursor;
    /// anything after the cursor follows.
    pub fn take(&self, input: &mut LineEdit) {
        let Some((item, _)) = self.items.get(self.cursor) else {
            return;
        };
        let rest: String = input.text().graphemes(true).skip(input.cursor()).collect();
        let rest = rest.trim_start();
        let mut taken = LineEdit::new(&format!("{item} {rest}"));
        for _ in 0..rest.graphemes(true).count() {
            taken.left();
        }
        *input = taken;
    }
}
