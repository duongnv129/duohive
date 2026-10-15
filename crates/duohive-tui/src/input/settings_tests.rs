//! The Settings screen's keys (UI §4.10, P2-D13).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::command::Command;
use duohive_core::ports::OrgFileError;
use duohive_core::settings::{Settings, Theme};

use super::{Action, UiEffect, map_key, update};
use crate::fixtures::spec_model;
use crate::focus::{AgentMode, Pane};
use crate::forms::SettingsRow;
use crate::model::{Dialog, UiModel};

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn code(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::NONE)
}

fn press(m: &mut UiModel, k: KeyEvent) -> Vec<UiEffect> {
    match map_key(m, k) {
        Some(a) => update(m, a),
        None => Vec::new(),
    }
}

fn open() -> UiModel {
    let mut m = spec_model();
    m.focus.pane = Pane::Org;
    assert_eq!(press(&mut m, key(',')), []);
    assert!(matches!(m.dialog, Some(Dialog::Settings(_))));
    m
}

fn go_to(m: &mut UiModel, row: SettingsRow) {
    for _ in 0..2 * SettingsRow::ALL.len() {
        let Some(Dialog::Settings(s)) = &m.dialog else {
            panic!("settings not open");
        };
        if s.row == row {
            return;
        }
        let index = |r: SettingsRow| SettingsRow::ALL.iter().position(|x| *x == r);
        let key = if index(row) > index(s.row) {
            KeyCode::Down
        } else {
            KeyCode::Up
        };
        press(m, code(key));
    }
    panic!("no row {row:?}");
}

fn sent(effects: &[UiEffect]) -> Settings {
    match effects {
        [UiEffect::Send(Command::UpdateSettings(s))] => s.clone(),
        other => panic!("not one UpdateSettings: {other:?}"),
    }
}

#[test]
fn comma_opens_settings() {
    for pane in [Pane::Org, Pane::NeedsYou, Pane::Agent] {
        let mut m = spec_model();
        m.focus.pane = pane;
        assert_eq!(map_key(&m, key(',')), Some(Action::Settings));
        assert_eq!(press(&mut m, key(',')), []);
        assert!(matches!(m.dialog, Some(Dialog::Settings(_))), "{pane:?}");
    }
}

#[test]
fn comma_types_in_chat_input() {
    let mut m = spec_model();
    m.focus.pane = Pane::Chat;
    assert_eq!(map_key(&m, key(',')), Some(Action::Char(',')));
    press(&mut m, key(','));
    assert_eq!(m.chat_input.text(), ",");
    assert!(m.dialog.is_none());

    // Typing mode sends it to the agent.
    let mut m = spec_model();
    m.live_agent = Some(duohive_core::testkit::n("auth-service"));
    m.focus.pane = Pane::Agent;
    m.focus.agent_mode = AgentMode::Typing;
    assert!(matches!(map_key(&m, key(',')), Some(Action::Type(_))));

    // A form types it too.
    let mut m = spec_model();
    m.focus.pane = Pane::Org;
    press(&mut m, key('e'));
    assert!(matches!(m.dialog, Some(Dialog::Form(_))));
    assert_eq!(map_key(&m, key(',')), Some(Action::Char(',')));
}

#[test]
fn each_change_sends_update() {
    let mut m = open();
    m.settings = Settings::default();

    go_to(&mut m, SettingsRow::LastReply);
    let s = sent(&press(&mut m, key(' ')));
    assert!(!s.show_last_reply);
    assert_eq!(m.settings, s);

    go_to(&mut m, SettingsRow::ReduceMotion);
    let s = sent(&press(&mut m, code(KeyCode::Enter)));
    assert!(s.reduce_motion);
    assert_eq!(m.settings, s);

    go_to(&mut m, SettingsRow::Theme);
    let s = sent(&press(&mut m, code(KeyCode::Right)));
    assert_eq!(s.theme, Theme::Dark);
    let s = sent(&press(&mut m, code(KeyCode::Enter)));
    assert_eq!(s.theme, Theme::Light);
    let s = sent(&press(&mut m, code(KeyCode::Enter)));
    assert_eq!(s.theme, Theme::Auto);

    go_to(&mut m, SettingsRow::Quiet);
    let s = sent(&press(&mut m, code(KeyCode::Right)));
    assert_eq!(s.quiet_after_minutes, 6);

    go_to(&mut m, SettingsRow::Name);
    assert_eq!(press(&mut m, code(KeyCode::Enter)), []);
    for c in "Duong".chars() {
        assert_eq!(press(&mut m, key(c)), []);
    }
    let s = sent(&press(&mut m, code(KeyCode::Enter)));
    assert_eq!(s.your_name.as_deref(), Some("Duong"));

    // Clearing the name takes it back to none.
    press(&mut m, code(KeyCode::Enter));
    for _ in 0..5 {
        press(&mut m, code(KeyCode::Backspace));
    }
    let s = sent(&press(&mut m, code(KeyCode::Enter)));
    assert_eq!(s.your_name, None);
}

#[test]
fn quiet_minimum_one() {
    let mut m = open();
    m.settings.quiet_after_minutes = 2;
    go_to(&mut m, SettingsRow::Quiet);
    assert_eq!(
        sent(&press(&mut m, code(KeyCode::Left))).quiet_after_minutes,
        1
    );
    // Already at the minimum: nothing changes, so nothing is sent.
    assert_eq!(press(&mut m, code(KeyCode::Left)), []);
    assert_eq!(m.settings.quiet_after_minutes, 1);

    // Typed 0, or nothing usable, becomes 1.
    m.settings.quiet_after_minutes = 5;
    press(&mut m, code(KeyCode::Enter));
    press(&mut m, code(KeyCode::Backspace));
    press(&mut m, key('0'));
    assert_eq!(
        sent(&press(&mut m, code(KeyCode::Enter))).quiet_after_minutes,
        1
    );
    press(&mut m, code(KeyCode::Enter));
    press(&mut m, code(KeyCode::Backspace));
    press(&mut m, key('x'));
    press(&mut m, code(KeyCode::Enter));
    assert_eq!(m.settings.quiet_after_minutes, 1);
    press(&mut m, code(KeyCode::Enter));
    press(&mut m, code(KeyCode::Backspace));
    for c in "12".chars() {
        press(&mut m, key(c));
    }
    assert_eq!(
        sent(&press(&mut m, code(KeyCode::Enter))).quiet_after_minutes,
        12
    );
}

#[test]
fn alert_toggle_sends_update_with_alerts_field() {
    let mut m = open();
    go_to(&mut m, SettingsRow::Finished);
    let s = sent(&press(&mut m, key(' ')));
    assert!(!s.alerts.finished);
    assert!(s.alerts.needs_you);
    go_to(&mut m, SettingsRow::NeedsYou);
    let s = sent(&press(&mut m, key(' ')));
    assert!(!s.alerts.needs_you && !s.alerts.finished);
    let s = sent(&press(&mut m, key(' ')));
    assert!(s.alerts.needs_you && !s.alerts.finished);
}

#[test]
fn reduce_motion_stops_spinner() {
    // The app loop schedules the spinner from `settings.reduce_motion` (app tests cover the
    // ticks); this is the key that flips it, taking effect at once.
    let mut m = open();
    m.settings.reduce_motion = false;
    go_to(&mut m, SettingsRow::ReduceMotion);
    let s = sent(&press(&mut m, key(' ')));
    assert!(s.reduce_motion);
    assert!(m.settings.reduce_motion);
    let s = sent(&press(&mut m, key(' ')));
    assert!(!s.reduce_motion);
}

#[test]
fn broken_settings_file_edits_in_memory() {
    let mut m = open();
    m.settings_error = Some(OrgFileError {
        line: Some(3),
        message: "invalid type".to_owned(),
    });
    go_to(&mut m, SettingsRow::ReduceMotion);
    // The engine keeps the file (D29); the UI still sends and shows the change.
    let s = sent(&press(&mut m, key(' ')));
    assert!(s.reduce_motion);
    assert!(m.settings.reduce_motion);
    assert!(m.settings_error.is_some());
}

#[test]
fn esc_goes_back_and_never_quits() {
    let mut m = open();
    assert_eq!(map_key(&m, code(KeyCode::Esc)), Some(Action::Esc));
    assert_eq!(press(&mut m, code(KeyCode::Esc)), []);
    assert!(m.dialog.is_none());

    // While editing, esc drops the edit and stays in Settings.
    let mut m = open();
    press(&mut m, code(KeyCode::Enter));
    press(&mut m, key('Z'));
    assert_eq!(press(&mut m, code(KeyCode::Esc)), []);
    assert!(matches!(m.dialog, Some(Dialog::Settings(_))));
    assert_eq!(m.settings.your_name, None);
    // `q` types while editing, instead of quitting.
    press(&mut m, code(KeyCode::Enter));
    assert_eq!(map_key(&m, key('q')), Some(Action::Char('q')));
}
