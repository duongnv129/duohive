//! `,`: Settings and its keys (UI §4.10, P2-D13). Every change is sent at once.

use duohive_core::command::Command;
use duohive_core::settings::Settings;

use super::{Action, UiEffect};
use crate::forms::{
    LineEdit, SettingsRow, SettingsState, cycled, name_typed, quiet_stepped, quiet_typed, toggled,
};
use crate::model::{Dialog, UiModel};

/// `,`: opens Settings on the first control.
pub(super) fn open(m: &mut UiModel) -> Vec<UiEffect> {
    m.dialog = Some(Dialog::Settings(SettingsState::default()));
    Vec::new()
}

/// A key in Settings: `↑↓` move, `space`/`enter` switch or open a box, `←→` step, `esc` goes back.
pub(super) fn action(m: &mut UiModel, mut s: SettingsState, a: Action) -> Vec<UiEffect> {
    if s.editing.is_some() {
        return edit(m, s, a);
    }
    let mut next = None;
    match a {
        Action::Esc | Action::DialogNo => return Vec::new(),
        Action::Up | Action::BackTab => s.row = s.row.prev(),
        Action::Down | Action::Tab => s.row = s.row.next(),
        Action::DialogEnter if s.row.is_text() => {
            s.editing = Some(LineEdit::new(&current(m, s.row)))
        }
        Action::DialogEnter => next = Some(toggled(&m.settings, s.row)),
        Action::Left | Action::Right => next = stepped(m, s.row, a == Action::Right),
        _ => {}
    }
    m.dialog = Some(Dialog::Settings(s));
    apply(m, next)
}

/// The text a box starts with.
fn current(m: &UiModel, row: SettingsRow) -> String {
    match row {
        SettingsRow::Quiet => m.settings.quiet_after_minutes.to_string(),
        _ => m.settings.your_name.clone().unwrap_or_default(),
    }
}

/// `←→` on a control: Quiet after or Theme; the others have nothing to step.
fn stepped(m: &UiModel, row: SettingsRow, forward: bool) -> Option<Settings> {
    match row {
        SettingsRow::Quiet => Some(quiet_stepped(&m.settings, forward)),
        SettingsRow::Theme => Some(Settings {
            theme: cycled(m.settings.theme, forward),
            ..m.settings.clone()
        }),
        _ => None,
    }
}

/// A key in an open text box: `enter` commits, `esc` drops it, the rest edits.
fn edit(m: &mut UiModel, mut s: SettingsState, a: Action) -> Vec<UiEffect> {
    let Some(mut line) = s.editing.take() else {
        return Vec::new();
    };
    let mut next = None;
    match a {
        Action::Esc => {}
        Action::DialogEnter => {
            next = Some(match s.row {
                SettingsRow::Quiet => quiet_typed(&m.settings, line.text()),
                _ => name_typed(&m.settings, line.text()),
            });
        }
        _ => {
            match a {
                Action::Char(c) if s.row != SettingsRow::Quiet || c.is_ascii_digit() => {
                    line.insert(c);
                }
                Action::Backspace => line.backspace(),
                Action::DeleteForward => line.delete(),
                Action::Left => line.left(),
                Action::Right => line.right(),
                Action::Home => line.home(),
                Action::End => line.end(),
                _ => {}
            }
            s.editing = Some(line);
        }
    }
    m.dialog = Some(Dialog::Settings(s));
    apply(m, next)
}

/// Keeps `next` in the model and sends it, when it changes anything.
fn apply(m: &mut UiModel, next: Option<Settings>) -> Vec<UiEffect> {
    match next {
        Some(s) if s != m.settings => {
            m.settings = s.clone();
            vec![UiEffect::Send(Command::UpdateSettings(s))]
        }
        _ => Vec::new(),
    }
}
