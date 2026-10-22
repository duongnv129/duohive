//! `?`, the key map, and Settings' key bindings editor (UI §4.10, §4.11).

use duohive_core::command::Command;
use duohive_core::settings::Settings;

use super::{Action, UiEffect};
use crate::keymap::{ActionId, KeyMapState, KeysState, RebindError, captured_binding};
use crate::model::{Dialog, UiModel};

const PAGE: usize = 10;

/// `?`: the key map.
pub(super) fn open(m: &mut UiModel) -> Vec<UiEffect> {
    m.dialog = Some(Dialog::KeyMap(KeyMapState::default()));
    Vec::new()
}

/// Settings' Keys row: the bindings editor.
pub(super) fn open_editor(m: &mut UiModel) -> Vec<UiEffect> {
    m.dialog = Some(Dialog::Keys(KeysState::new()));
    Vec::new()
}

/// `↑↓` and `PgUp/PgDn` scroll the key map; `esc` and `?` close it.
pub(super) fn map_action(m: &mut UiModel, mut s: KeyMapState, a: Action) -> Vec<UiEffect> {
    let last = ActionId::ALL.len().saturating_sub(1);
    match a {
        Action::Esc => {
            m.dialog = None;
            return Vec::new();
        }
        Action::Up => s.scroll = s.scroll.saturating_sub(1),
        Action::Down => s.scroll = (s.scroll + 1).min(last),
        Action::PageUp => s.scroll = s.scroll.saturating_sub(PAGE),
        Action::PageDown => s.scroll = (s.scroll + PAGE).min(last),
        _ => {}
    }
    m.dialog = Some(Dialog::KeyMap(s));
    Vec::new()
}

/// The editor: `↑↓` move, `enter` captures, `backspace` restores the default, `esc` back.
pub(super) fn editor_action(m: &mut UiModel, mut s: KeysState, a: Action) -> Vec<UiEffect> {
    match a {
        Action::Esc => {
            m.dialog = None;
            return Vec::new();
        }
        Action::Up => s.step(false),
        Action::Down => s.step(true),
        Action::PageUp => (0..PAGE).for_each(|_| s.step(false)),
        Action::PageDown => (0..PAGE).for_each(|_| s.step(true)),
        Action::DialogEnter => s.capturing = true,
        Action::Backspace => {
            let action = s.action();
            m.keymap.reset(action);
            s.notice = None;
            m.dialog = Some(Dialog::Keys(s));
            return send(m);
        }
        Action::Capture(k) if k.code == crossterm::event::KeyCode::Esc => {
            s.capturing = false;
            s.notice = None;
        }
        Action::Capture(k) => {
            let Some(binding) = captured_binding(&k) else {
                m.dialog = Some(Dialog::Keys(s));
                return Vec::new();
            };
            let action = s.action();
            s.capturing = false;
            match m.keymap.rebind(action, binding) {
                Ok(()) => {
                    s.notice = None;
                    m.dialog = Some(Dialog::Keys(s));
                    return send(m);
                }
                Err(e) => s.notice = Some(message(&e)),
            }
        }
        _ => {}
    }
    m.dialog = Some(Dialog::Keys(s));
    Vec::new()
}

/// The refusal, as the row shows it (UI §4.10).
fn message(e: &RebindError) -> String {
    match e {
        RebindError::Reserved(key) => format!("{key} is reserved"),
        RebindError::Conflict { key, action } => {
            format!("{key} is already {} · rebind that first", action.label())
        }
    }
}

/// Saves the changed bindings in `settings.toml`, and keeps them in the model.
fn send(m: &mut UiModel) -> Vec<UiEffect> {
    let settings = Settings {
        keys: m.keymap.overrides(),
        ..m.settings.clone()
    };
    m.set_settings(settings.clone());
    vec![UiEffect::Send(Command::UpdateSettings(settings))]
}
