//! `m`: Move to… and its keys (UI §4.4.2).

use duohive_core::command::Command;

use super::{Action, UiEffect};
use crate::forms::{MoveTo, node_name};
use crate::model::{Dialog, UiModel};

const ORG_FILE_BROKEN: &str = "org.toml has an error · fix it, then press o to reload";

/// `m` on the selection: the picker, or the reason there is none.
pub(super) fn open(m: &mut UiModel) -> Vec<UiEffect> {
    if m.org_error.is_some() {
        m.footer_note = Some(ORG_FILE_BROKEN.to_owned());
        return Vec::new();
    }
    let node = m.selected.clone();
    match MoveTo::open(&m.org, &node) {
        Some(picker) => m.dialog = Some(Dialog::MoveTo(Box::new(picker))),
        // Duo has nowhere to go and is not named in a refusal.
        None if node == duohive_core::org::NodeKey::Duo => {}
        None => m.footer_note = Some(format!("no other place to move {}", node_name(&node))),
    }
    Vec::new()
}

/// A key in the picker: `↑↓` choose, `enter` moves, `esc` cancels.
pub(super) fn action(m: &mut UiModel, mut picker: Box<MoveTo>, a: Action) -> Vec<UiEffect> {
    match a {
        Action::Esc | Action::DialogNo => return Vec::new(),
        Action::Up => picker.step(false),
        Action::Down => picker.step(true),
        Action::DialogEnter => {
            // The current place and a refused one stay open; the refusal is on screen.
            if let Some(edit) = picker.edit() {
                return vec![UiEffect::Send(Command::EditOrg(edit))];
            }
        }
        _ => {}
    }
    m.dialog = Some(Dialog::MoveTo(picker));
    Vec::new()
}
