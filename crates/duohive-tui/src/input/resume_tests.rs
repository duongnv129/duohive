//! `r`: resume an old session of a stopped or errored agent (UI §4.13).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::command::Command;
use duohive_core::names::AgentName;
use duohive_core::org::{NodeKey, RoleKey};
use duohive_core::status::AgentState;
use duohive_core::testkit::n;

use super::{UiEffect, map_key, update};
use crate::fixtures::{ctx, mins, render_lines, spec_model, status, view};
use crate::focus::Pane;
use crate::forms::{LineEdit, SettingsRow, SettingsState};
use crate::keymap::{ActionId, parse_binding};
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

fn selected(state: AgentState) -> (UiModel, AgentName) {
    let mut m = spec_model();
    let agent = n("pay-qa");
    m.agents
        .insert(agent.clone(), view(status(state, mins(30))));
    m.selected = NodeKey::Agent(agent.clone());
    m.focus.pane = Pane::Org;
    (m, agent)
}

fn list(agent: &AgentName) -> Vec<UiEffect> {
    vec![UiEffect::Send(Command::ListSessions {
        agent: agent.clone(),
    })]
}

#[test]
fn r_on_stopped_agent_sends_list_sessions() {
    let (mut m, agent) = selected(AgentState::Stopped);
    assert_eq!(press(&mut m, key('r')), list(&agent));
}

#[test]
fn r_on_errored_agent_sends_list_sessions() {
    let (mut m, agent) = selected(AgentState::Errored);
    assert_eq!(press(&mut m, key('r')), list(&agent));
}

#[test]
fn r_on_running_agent_says_stop_it_first() {
    for state in [
        AgentState::Starting,
        AgentState::Working,
        AgentState::Waiting,
        AgentState::Finished,
        AgentState::Idle,
    ] {
        let (mut m, _) = selected(state);
        assert_eq!(press(&mut m, key('r')), [], "{state:?}");
        assert_eq!(
            m.footer_note.as_deref(),
            Some("pay-qa is running · stop it first (s)"),
            "{state:?}"
        );
    }
}

#[test]
fn r_on_team_or_role_does_nothing() {
    let (mut m, _) = selected(AgentState::Stopped);
    let nodes = [
        NodeKey::Team(duohive_core::names::NodeName::parse("Payments").unwrap()),
        NodeKey::Role(RoleKey {
            team: Some(duohive_core::names::NodeName::parse("Payments").unwrap()),
            role: duohive_core::names::NodeName::parse("QA").unwrap(),
        }),
        NodeKey::Duo,
    ];
    for node in nodes {
        m.selected = node.clone();
        m.footer_note = None;
        assert_eq!(press(&mut m, key('r')), [], "{node:?}");
        assert_eq!(m.footer_note, None, "{node:?}");
    }
}

#[test]
fn r_in_a_dialog_is_not_resume() {
    let (mut m, _) = selected(AgentState::Stopped);
    m.dialog = Some(Dialog::Settings(SettingsState {
        row: SettingsRow::Name,
        editing: Some(LineEdit::new("")),
    }));
    assert_eq!(press(&mut m, key('r')), []);
    m.dialog = Some(Dialog::Worktrees(WorktreeList::default()));
    assert_eq!(press(&mut m, key('r')), []);
    assert_eq!(m.footer_note, None);
}

#[test]
fn rebinding_resume_moves_the_key() {
    let (mut m, agent) = selected(AgentState::Stopped);
    let binding = parse_binding("x").expect("x is a key");
    assert_eq!(m.keymap.rebind(ActionId::Resume, binding), Ok(()));
    assert_eq!(press(&mut m, key('x')), list(&agent));
    assert_eq!(press(&mut m, key('r')), []);
}

#[test]
fn resume_loads_from_settings_toml_keys() {
    let saved = std::collections::BTreeMap::from([("resume".to_owned(), "x".to_owned())]);
    let (km, notices) = crate::keymap::Keymap::from_settings(&saved);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(km.key(ActionId::Resume).label(), "x");
}

fn in_agent_pane(state: AgentState) -> (UiModel, AgentName) {
    let (mut m, agent) = selected(state);
    m.focus.pane = Pane::Agent;
    m.focus.agent_mode = crate::focus::AgentMode::Selected;
    (m, agent)
}

#[test]
fn r_in_agent_pane_selected_on_stopped_agent_sends_list_sessions() {
    let (mut m, agent) = in_agent_pane(AgentState::Stopped);
    assert_eq!(press(&mut m, key('r')), list(&agent));
}

#[test]
fn r_in_agent_pane_selected_on_errored_agent_sends_list_sessions() {
    let (mut m, agent) = in_agent_pane(AgentState::Errored);
    assert_eq!(press(&mut m, key('r')), list(&agent));
}

#[test]
fn r_in_agent_pane_selected_on_running_agent_shows_stop_it_first() {
    let (mut m, _) = in_agent_pane(AgentState::Working);
    assert_eq!(press(&mut m, key('r')), []);
    assert_eq!(
        m.footer_note.as_deref(),
        Some("pay-qa is running · stop it first (s)")
    );
}

#[test]
fn r_in_agent_pane_typing_goes_to_the_agent() {
    let (mut m, _) = in_agent_pane(AgentState::Working);
    m.focus.agent_mode = crate::focus::AgentMode::Typing;
    m.live_agent = Some(n("pay-qa"));
    let out = press(&mut m, key('r'));
    assert!(matches!(out.as_slice(), [UiEffect::Pane(_)]), "{out:?}");
}

#[test]
fn r_in_agent_pane_with_a_team_role_or_duo_selected_does_nothing() {
    let (mut m, _) = in_agent_pane(AgentState::Stopped);
    let name = |s| duohive_core::names::NodeName::parse(s).unwrap();
    for node in [
        NodeKey::Team(name("Payments")),
        NodeKey::Role(RoleKey {
            team: Some(name("Payments")),
            role: name("QA"),
        }),
        NodeKey::Duo,
    ] {
        m.selected = node.clone();
        m.footer_note = None;
        assert_eq!(press(&mut m, key('r')), [], "{node:?}");
        assert_eq!(m.footer_note, None, "{node:?}");
    }
}

#[test]
fn rebinding_resume_to_an_agent_pane_key_is_refused() {
    let mut m = spec_model();
    let zoom = m.keymap.key(ActionId::Zoom);
    assert_eq!(
        m.keymap.rebind(ActionId::Resume, zoom),
        Err(crate::keymap::RebindError::Conflict {
            key: "z".to_owned(),
            action: ActionId::Zoom
        })
    );
}

// ---- While the sessions are read ----

fn reading_note(m: &UiModel, agent: &AgentName) -> String {
    let cli = duohive_core::inherit::resolve(&m.org, agent)
        .map_or(m.org.defaults.cli, |r| r.cli)
        .display();
    format!("reading {cli}'s sessions…")
}

#[test]
fn r_sets_the_reading_note() {
    let (mut m, agent) = selected(AgentState::Stopped);
    assert_eq!(press(&mut m, key('r')), list(&agent));
    let note = reading_note(&m, &agent);
    assert!(note.starts_with("reading ") && note.ends_with("'s sessions…"));
    assert_eq!(m.footer_note, Some(note));
}

#[test]
fn reading_note_is_replaced_by_the_error_note() {
    let (mut m, agent) = selected(AgentState::Stopped);
    press(&mut m, key('r'));
    m.apply(duohive_core::event::Event::SessionsUnreadable {
        agent: agent.clone(),
        reason: "disk gone".to_owned(),
    });
    let note = m.footer_note.clone().expect("a note");
    assert!(note.starts_with("Couldn't read "), "{note}");
    assert_eq!(m.error_note, Some(note));
}

#[test]
fn reading_note_does_not_stick_after_the_picker_opens() {
    let (mut m, agent) = selected(AgentState::Stopped);
    press(&mut m, key('r'));
    m.apply(duohive_core::event::Event::SessionsListed {
        agent,
        sessions: Vec::new(),
    });
    assert!(matches!(m.dialog, Some(Dialog::Resume(_))));
    assert_eq!(m.footer_note, None);
}

#[test]
fn reading_note_goes_with_the_next_action() {
    let (mut m, _) = selected(AgentState::Stopped);
    press(&mut m, key('r'));
    press(&mut m, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(m.footer_note, None);
}

#[test]
fn r_again_while_reading_still_sends_the_command() {
    let (mut m, agent) = selected(AgentState::Stopped);
    assert_eq!(press(&mut m, key('r')), list(&agent));
    assert_eq!(press(&mut m, key('r')), list(&agent));
    assert!(m.footer_note.is_some());
}

// ---- A saved `r` binding beats Resume's default (v1 compatibility) ----

fn with_saved_keys(m: &mut UiModel, keys: &[(&str, &str)]) {
    let mut settings = m.settings.clone();
    settings.keys = keys
        .iter()
        .map(|(a, k)| ((*a).to_owned(), (*k).to_owned()))
        .collect();
    m.set_settings(settings);
}

const TAKEN_NOTICE: &str = "resume: r is taken by move · rebind it in Keys";

#[test]
fn saved_key_r_for_another_action_beats_the_resume_default() {
    let (mut m, _) = selected(AgentState::Stopped);
    with_saved_keys(&mut m, &[("move", "r")]);
    let out = press(&mut m, key('r'));
    assert!(
        !out.iter()
            .any(|e| matches!(e, UiEffect::Send(Command::ListSessions { .. }))),
        "{out:?}"
    );
    assert!(m.dialog.is_some(), "r should open Move: {:?}", m.dialog);
    assert_eq!(m.keymap.key(ActionId::Resume).label(), "");
    assert_eq!(m.keymap.key(ActionId::Move).label(), "r");
    assert_eq!(m.keys_notices, [TAKEN_NOTICE.to_owned()]);
}

#[test]
fn unbound_resume_is_not_advertised() {
    let (mut m, agent) = selected(AgentState::Stopped);
    with_saved_keys(&mut m, &[("move", "r")]);
    m.focus.pane = Pane::Agent;
    let footer = render_lines(&m, &ctx(), 120, 36);
    let last = footer.last().cloned().unwrap_or_default();
    assert!(!last.contains("resume"), "{last}");
    // The node menu shows no key; the row still works by mouse.
    let menu = super::menu::build(
        &m,
        NodeKey::Agent(agent.clone()),
        ratatui::layout::Position::new(0, 0),
    );
    let at = menu
        .rows
        .iter()
        .position(|r| r.action == ActionId::Resume)
        .expect("resume row");
    assert_eq!(menu.rows[at].key, "");
    m.menu = Some(menu);
    assert_eq!(super::menu::choose(&mut m, at), list(&agent));
}

#[test]
fn bound_resume_is_advertised_in_the_agent_footer() {
    let (mut m, _) = selected(AgentState::Stopped);
    m.focus.pane = Pane::Agent;
    let last = render_lines(&m, &ctx(), 120, 36)
        .last()
        .cloned()
        .unwrap_or_default();
    assert!(last.contains("r resume"), "{last}");
}

#[test]
fn explicit_resume_binding_wins_without_notice() {
    let (mut m, agent) = selected(AgentState::Stopped);
    with_saved_keys(&mut m, &[("move", "r"), ("resume", "x")]);
    assert!(m.keys_notices.is_empty(), "{:?}", m.keys_notices);
    assert_eq!(press(&mut m, key('x')), list(&agent));
}

#[test]
fn no_collision_leaves_defaults_unchanged() {
    let (mut m, agent) = selected(AgentState::Stopped);
    with_saved_keys(&mut m, &[("move", "u")]);
    assert!(m.keys_notices.is_empty(), "{:?}", m.keys_notices);
    assert_eq!(m.keymap.key(ActionId::Resume).label(), "r");
    assert_eq!(press(&mut m, key('r')), list(&agent));
}

// ---- The picker (UI §4.13) ----

mod picker {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
    use duohive_core::command::Command;
    use duohive_core::event::Event;
    use duohive_core::testkit::n;
    use ratatui::layout::Rect;

    use super::super::mouse::on_mouse;
    use super::super::mouse_tests::{cell_of, mouse, press};
    use super::super::{Action, UiEffect, map_key, update};
    use crate::fixtures::{resume_model, three_sessions};
    use crate::forms::ResumePicker;
    use crate::model::{Dialog, UiModel, WorktreeList};

    fn code(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    fn press_key(m: &mut UiModel, k: KeyEvent) -> Vec<UiEffect> {
        match map_key(m, k) {
            Some(a) => update(m, a),
            None => Vec::new(),
        }
    }

    fn type_str(m: &mut UiModel, s: &str) {
        for c in s.chars() {
            let out = press_key(m, code(KeyCode::Char(c)));
            assert_eq!(out, [], "{c}");
        }
    }

    fn picker(m: &UiModel) -> &ResumePicker {
        match &m.dialog {
            Some(Dialog::Resume(p)) => p,
            other => panic!("not the picker: {other:?}"),
        }
    }

    fn resume_cmd(title: &str) -> UiEffect {
        UiEffect::Send(Command::ResumeSession {
            agent: n("pay-qa"),
            session_id: format!("id-{title}"),
        })
    }

    #[test]
    fn the_answer_opens_the_picker_titled_with_the_agent() {
        let (m, agent) = resume_model(three_sessions());
        let p = picker(&m);
        assert_eq!(p.agent, agent);
        assert_eq!(p.cursor, 0);
        assert_eq!(p.cli_label, "Claude Code");
        assert_eq!(p.sessions.len(), 3);
    }

    #[test]
    fn answer_is_dropped_when_another_dialog_is_open() {
        let (mut m, agent) = resume_model(Vec::new());
        m.dialog = Some(Dialog::Worktrees(WorktreeList::default()));
        m.apply(Event::SessionsListed {
            agent,
            sessions: three_sessions(),
        });
        assert!(matches!(m.dialog, Some(Dialog::Worktrees(_))));
    }

    #[test]
    fn answer_for_an_unknown_agent_is_dropped() {
        let (mut m, _) = resume_model(Vec::new());
        m.dialog = None;
        m.apply(Event::SessionsListed {
            agent: n("ghost"),
            sessions: three_sessions(),
        });
        assert_eq!(m.dialog, None);
    }

    #[test]
    fn unreadable_is_a_footer_note_and_no_dialog() {
        let (mut m, agent) = resume_model(Vec::new());
        m.dialog = None;
        let before = m.agents.clone();
        m.apply(Event::SessionsUnreadable {
            agent,
            reason: "boom".to_owned(),
        });
        assert_eq!(m.dialog, None);
        assert_eq!(
            m.footer_note.as_deref(),
            Some("Couldn't read Claude Code's sessions: boom")
        );
        assert_eq!(m.agents, before);
    }

    #[test]
    fn enter_sends_resume_session() {
        let (mut m, _) = resume_model(three_sessions());
        press_key(&mut m, code(KeyCode::Down));
        let out = press_key(&mut m, code(KeyCode::Enter));
        assert_eq!(out, [resume_cmd("Payout ledger migration")]);
        assert_eq!(m.dialog, None);
    }

    #[test]
    fn esc_clears_search_then_closes() {
        let (mut m, _) = resume_model(three_sessions());
        type_str(&mut m, "web");
        assert_eq!(picker(&m).search, "web");
        press_key(&mut m, code(KeyCode::Esc));
        assert_eq!(picker(&m).search, "");
        assert_eq!(picker(&m).visible().len(), 3);
        press_key(&mut m, code(KeyCode::Esc));
        assert_eq!(m.dialog, None);
    }

    #[test]
    fn backspace_edits_the_search() {
        let (mut m, _) = resume_model(three_sessions());
        type_str(&mut m, "abc");
        press_key(&mut m, code(KeyCode::Backspace));
        assert_eq!(picker(&m).search, "ab");
    }

    #[test]
    fn typing_goes_to_search_not_global_keys() {
        let (mut m, _) = resume_model(three_sessions());
        type_str(&mut m, "qsr? ,wzmfn");
        assert_eq!(picker(&m).search, "qsr? ,wzmfn");
        assert!(matches!(m.dialog, Some(Dialog::Resume(_))));
    }

    #[test]
    fn in_use_entry_ignores_enter_and_footer_says_who() {
        let mut sessions = three_sessions();
        sessions[1].in_use_by = Some(n("squad-2"));
        let (mut m, _) = resume_model(sessions);
        press_key(&mut m, code(KeyCode::Down));
        let out = press_key(&mut m, code(KeyCode::Enter));
        assert_eq!(out, []);
        assert!(matches!(m.dialog, Some(Dialog::Resume(_))));
        assert_eq!(
            m.footer_note.as_deref(),
            Some("squad-2 is using this session")
        );
    }

    #[test]
    fn enter_on_recent_asks_with_cancel_focused() {
        let (mut m, _) = resume_model(three_sessions());
        let out = press_key(&mut m, code(KeyCode::Enter));
        assert_eq!(out, []);
        let Some(Dialog::Confirm(c)) = &m.dialog else {
            panic!("no confirmation: {:?}", m.dialog);
        };
        assert_eq!(
            c.lines,
            [
                r#"Resume "Retry loop for failed payouts"? It was active 3 minutes ago and may still be open in another terminal."#
            ]
        );
        assert!(!c.focus_yes);
        assert_eq!(c.yes_label, "Resume");
    }

    #[test]
    fn cancel_returns_to_the_picker_unchanged() {
        let (mut m, _) = resume_model(three_sessions());
        type_str(&mut m, "payout");
        press_key(&mut m, code(KeyCode::Down));
        let before = picker(&m).clone();
        // Cursor 1 is "Retry..."? "payout" matches two: the recent one is first.
        press_key(&mut m, code(KeyCode::Up));
        press_key(&mut m, code(KeyCode::Enter));
        assert!(matches!(m.dialog, Some(Dialog::Confirm(_))));
        // `enter` with Cancel focused cancels.
        let out = press_key(&mut m, code(KeyCode::Enter));
        assert_eq!(out, []);
        let after = picker(&m);
        assert_eq!(after.search, "payout");
        assert_eq!(after.cursor, 0);
        assert_eq!(after.confirm, None);
        assert_eq!(after.sessions, before.sessions);
    }

    #[test]
    fn y_on_the_confirmation_resumes() {
        let (mut m, _) = resume_model(three_sessions());
        press_key(&mut m, code(KeyCode::Enter));
        let out = press_key(&mut m, code(KeyCode::Char('y')));
        assert_eq!(out, [resume_cmd("Retry loop for failed payouts")]);
        assert_eq!(m.dialog, None);
    }

    #[test]
    fn click_row_selects_and_double_click_or_enter_resumes() {
        let (mut m, _) = resume_model(three_sessions());
        let (x, y) = cell_of(&m, Rect::new(0, 0, 120, 36), "Flaky webhook test");
        let out = on_mouse(&mut m, press(x, y));
        assert_eq!(out, [resume_cmd("Flaky webhook test")]);
        assert_eq!(m.dialog, None);
    }

    #[test]
    fn click_on_a_recent_row_asks_first() {
        let (mut m, _) = resume_model(three_sessions());
        let (x, y) = cell_of(&m, Rect::new(0, 0, 120, 36), "Retry loop");
        assert_eq!(on_mouse(&mut m, press(x, y)), []);
        assert!(matches!(m.dialog, Some(Dialog::Confirm(_))));
    }

    #[test]
    fn click_confirm_buttons() {
        let (mut m, _) = resume_model(three_sessions());
        press_key(&mut m, code(KeyCode::Enter));
        let screen = Rect::new(0, 0, 120, 36);
        let (x, y) = cell_of(&m, screen, "[ n Cancel ]");
        assert_eq!(on_mouse(&mut m, press(x, y)), []);
        assert!(matches!(m.dialog, Some(Dialog::Resume(_))));
        press_key(&mut m, code(KeyCode::Enter));
        let (x, y) = cell_of(&m, screen, "[ y Resume ]");
        let out = on_mouse(&mut m, press(x, y));
        assert_eq!(out, [resume_cmd("Retry loop for failed payouts")]);
        assert_eq!(m.dialog, None);
    }

    #[test]
    fn click_outside_the_dialog_is_ignored() {
        let (mut m, _) = resume_model(three_sessions());
        assert_eq!(on_mouse(&mut m, press(0, 0)), []);
        assert!(matches!(m.dialog, Some(Dialog::Resume(_))));
    }

    #[test]
    fn the_wheel_scrolls_the_list() {
        let (mut m, _) = resume_model(three_sessions());
        let wheel = |kind: MouseEventKind| -> MouseEvent { mouse(kind, 60, 18) };
        on_mouse(&mut m, wheel(MouseEventKind::ScrollDown));
        assert_eq!(picker(&m).cursor, 1);
        on_mouse(&mut m, wheel(MouseEventKind::ScrollUp));
        assert_eq!(picker(&m).cursor, 0);
    }

    #[test]
    fn empty_list_opens_and_esc_closes() {
        let (mut m, _) = resume_model(Vec::new());
        assert!(matches!(m.dialog, Some(Dialog::Resume(_))));
        assert_eq!(press_key(&mut m, code(KeyCode::Enter)), []);
        assert!(matches!(m.dialog, Some(Dialog::Resume(_))));
        press_key(&mut m, code(KeyCode::Esc));
        assert_eq!(m.dialog, None);
    }

    #[test]
    fn up_down_page_clamp() {
        let (mut m, _) = resume_model(three_sessions());
        press_key(&mut m, code(KeyCode::Up));
        assert_eq!(picker(&m).cursor, 0);
        press_key(&mut m, code(KeyCode::PageDown));
        assert_eq!(picker(&m).cursor, 2);
        press_key(&mut m, code(KeyCode::Down));
        assert_eq!(picker(&m).cursor, 2);
        press_key(&mut m, code(KeyCode::PageUp));
        assert_eq!(picker(&m).cursor, 0);
        let _ = Action::Up;
    }
}
