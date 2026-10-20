//! The keymap through input: rebinding, capture and `?` (UI §4.10, §4.11).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::command::Command;

use super::{Action, UiEffect, map_key, update};
use crate::fixtures::spec_model;
use crate::focus::{AgentMode, Pane};
use crate::forms::{LineEdit, SettingsRow, SettingsState};
use crate::keymap::{ActionId, KeysState, parse_binding};
use crate::model::{Dialog, UiModel, WorktreeList};

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn press(m: &mut UiModel, k: KeyEvent) -> Vec<UiEffect> {
    match map_key(m, k) {
        Some(a) => update(m, a),
        None => Vec::new(),
    }
}

fn editor(m: &mut UiModel, cursor: usize) {
    m.dialog = Some(Dialog::Keys(KeysState {
        cursor,
        ..KeysState::default()
    }));
}

fn keys_state(m: &UiModel) -> &KeysState {
    match &m.dialog {
        Some(Dialog::Keys(s)) => s,
        other => panic!("not the editor: {other:?}"),
    }
}

#[test]
fn rebind_applies_to_map_key() {
    let mut m = spec_model();
    m.focus.pane = Pane::Org;
    let binding = parse_binding("x").expect("x is a key");
    assert_eq!(m.keymap.rebind(ActionId::StartStop, binding), Ok(()));
    assert_eq!(map_key(&m, key('x')), Some(Action::StartStop));
    assert_eq!(map_key(&m, key('s')), None);
}

#[test]
fn typing_and_forms_unaffected() {
    let mut m = spec_model();
    m.focus.pane = Pane::Agent;
    m.focus.agent_mode = AgentMode::Typing;
    assert!(matches!(map_key(&m, key('?')), Some(Action::Type(_))));

    let mut m = spec_model();
    m.dialog = Some(Dialog::Settings(SettingsState {
        row: SettingsRow::Name,
        editing: Some(LineEdit::new("")),
    }));
    assert_eq!(map_key(&m, key('?')), Some(Action::Char('?')));

    let mut m = spec_model();
    m.focus.pane = Pane::Chat;
    assert_eq!(map_key(&m, key('?')), Some(Action::Char('?')));
}

#[test]
fn esc_cancels_capture() {
    let mut m = spec_model();
    editor(&mut m, 0);
    press(&mut m, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(keys_state(&m).capturing);
    press(&mut m, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!keys_state(&m).capturing);
    assert_eq!(m.keymap.key(ActionId::GiveWork).label(), "space");
}

#[test]
fn enter_captures_next_key_and_sends_update_settings() {
    let mut m = spec_model();
    editor(&mut m, 0);
    press(&mut m, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let out = press(&mut m, key('x'));
    assert_eq!(m.keymap.key(ActionId::GiveWork).label(), "x");
    assert!(!keys_state(&m).capturing);
    let sent = out
        .into_iter()
        .find_map(|e| match e {
            UiEffect::Send(Command::UpdateSettings(s)) => Some(s),
            _ => None,
        })
        .expect("UpdateSettings");
    assert_eq!(sent.keys.get("give_work").map(String::as_str), Some("x"));
    assert_eq!(
        m.settings.keys.get("give_work").map(String::as_str),
        Some("x")
    );
}

#[test]
fn reserved_key_refused_with_message() {
    let mut m = spec_model();
    editor(&mut m, 0);
    press(&mut m, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(press(&mut m, key('q')), []);
    assert_eq!(keys_state(&m).notice.as_deref(), Some("q is reserved"));
    assert_eq!(m.keymap.key(ActionId::GiveWork).label(), "space");
}

#[test]
fn conflict_message_names_the_action() {
    let mut m = spec_model();
    editor(&mut m, 0);
    press(&mut m, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(press(&mut m, key('s')), []);
    assert_eq!(
        keys_state(&m).notice.as_deref(),
        Some("s is already Start / stop · rebind that first")
    );
}

#[test]
fn question_mark_opens_the_key_map_selected_mode_only() {
    let mut m = spec_model();
    m.focus.pane = Pane::Org;
    press(&mut m, key('?'));
    assert!(matches!(m.dialog, Some(Dialog::KeyMap(_))));

    // `?` closes it, as `esc` does.
    press(&mut m, key('?'));
    assert!(m.dialog.is_none());

    let mut m = spec_model();
    m.focus.pane = Pane::Chat;
    assert_eq!(map_key(&m, key('?')), Some(Action::Char('?')));
}

#[test]
fn help_key_works_zoomed() {
    let mut m = spec_model();
    m.zoomed = true;
    press(&mut m, key('?'));
    assert!(matches!(m.dialog, Some(Dialog::KeyMap(_))));
}

#[test]
fn question_mark_ignored_in_dialogs() {
    let mut m = spec_model();
    m.dialog = Some(Dialog::Worktrees(WorktreeList::default()));
    assert_eq!(map_key(&m, key('?')), None);
}
