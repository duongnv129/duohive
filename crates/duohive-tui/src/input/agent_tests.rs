//! The agent pane's keys: Selected and Typing modes, scrolling and focusing (UI §4.1.2,
//! P2-D9, P2-D12).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::command::Command;
use duohive_core::org::NodeKey;
use duohive_core::status::AgentState;
use duohive_core::testkit::n;
use duohive_core::view::{PaneInput, PaneKey};

use super::agent::request_typing;
use super::{UiEffect, map_key, paste, update};
use crate::fixtures::{mins, spec_model, status};
use crate::focus::{AgentMode, Pane};
use crate::model::UiModel;

fn press(m: &mut UiModel, k: KeyEvent) -> Vec<UiEffect> {
    match map_key(m, k) {
        Some(a) => update(m, a),
        None => Vec::new(),
    }
}

fn plain(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::NONE)
}

fn with(c: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent::new(c, mods)
}

/// auth-service (working) selected, its live terminal showing, the agent pane focused,
/// Typing.
fn typing() -> UiModel {
    let mut m = spec_model();
    m.live_agent = Some(n("auth-service"));
    m.focus.pane = Pane::Agent;
    assert_eq!(press(&mut m, plain(KeyCode::Enter)), []);
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
    m
}

#[test]
fn typing_forwards_every_key_but_ctrl_q() {
    let mut m = typing();
    let cases = [
        (plain(KeyCode::Char('q')), PaneInput::Text("q".into())),
        (plain(KeyCode::Esc), PaneInput::Key(PaneKey::Escape)),
        (plain(KeyCode::Tab), PaneInput::Key(PaneKey::Tab)),
        (
            with(KeyCode::Char('c'), KeyModifiers::CONTROL),
            PaneInput::Key(PaneKey::Ctrl('c')),
        ),
        (plain(KeyCode::Char('?')), PaneInput::Text("?".into())),
        (plain(KeyCode::Char(' ')), PaneInput::Text(" ".into())),
        (plain(KeyCode::Up), PaneInput::Key(PaneKey::Up)),
        (plain(KeyCode::F(5)), PaneInput::Key(PaneKey::F(5))),
        (
            with(KeyCode::Char('x'), KeyModifiers::ALT),
            PaneInput::Key(PaneKey::Alt('x')),
        ),
        // Global letter keys type too.
        (plain(KeyCode::Char('s')), PaneInput::Text("s".into())),
        (plain(KeyCode::Char('z')), PaneInput::Text("z".into())),
        (plain(KeyCode::Enter), PaneInput::Key(PaneKey::Enter)),
        (plain(KeyCode::PageUp), PaneInput::Key(PaneKey::PageUp)),
    ];
    for (k, want) in cases {
        assert_eq!(press(&mut m, k), [UiEffect::Pane(want)], "{k:?}");
        assert_eq!(m.focus.pane, Pane::Agent);
        assert_eq!(m.focus.agent_mode, AgentMode::Typing);
        assert_eq!(m.selected, NodeKey::Agent(n("auth-service")));
    }
    let ctrl_q = with(KeyCode::Char('q'), KeyModifiers::CONTROL);
    assert_eq!(press(&mut m, ctrl_q), []);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    assert_eq!(m.focus.pane, Pane::Agent);
    // In Selected mode `q` quits again.
    assert_eq!(press(&mut m, plain(KeyCode::Char('q'))), [UiEffect::Quit]);
}

#[test]
fn paste_while_typing_is_bracketed() {
    let mut m = typing();
    assert_eq!(
        paste(&mut m, "line one\nline two"),
        [UiEffect::Pane(PaneInput::Paste(
            "line one\nline two".into()
        ))]
    );
    // Selected mode: the paste goes nowhere.
    m.focus.agent_mode = AgentMode::Selected;
    assert_eq!(paste(&mut m, "x"), []);
}

#[test]
fn enter_types_only_into_a_running_agent() {
    let mut m = spec_model();
    m.focus.pane = Pane::Agent;
    m.selected = NodeKey::Agent(n("pay-qa"));
    assert_eq!(press(&mut m, plain(KeyCode::Enter)), []);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    m.selected = NodeKey::Duo;
    m.live_agent = Some(n("Duo"));
    press(&mut m, plain(KeyCode::Enter));
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
}

#[test]
fn enter_waits_for_the_live_terminal() {
    let mut m = spec_model();
    m.focus.pane = Pane::Agent;
    // Running, but no stream with a `Reset` yet (or another agent's).
    for live in [None, Some(n("billing-api"))] {
        m.live_agent = live;
        assert_eq!(press(&mut m, plain(KeyCode::Enter)), []);
        assert_eq!(m.focus.agent_mode, AgentMode::Selected);
        assert_eq!(
            m.footer_note.as_deref(),
            Some("auth-service isn't ready yet")
        );
    }
    m.live_agent = Some(n("auth-service"));
    press(&mut m, plain(KeyCode::Enter));
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
    assert_eq!(m.footer_note, None);
}

#[test]
fn pgup_and_pgdn_scroll_in_selected_mode_and_enter_returns_live() {
    let mut m = spec_model();
    m.focus.pane = Pane::Agent;
    assert_eq!(
        press(&mut m, plain(KeyCode::PageUp)),
        [UiEffect::Scroll { up: true }]
    );
    assert_eq!(
        press(&mut m, plain(KeyCode::PageDown)),
        [UiEffect::Scroll { up: false }]
    );
    m.scrolled = true;
    m.live_agent = Some(n("auth-service"));
    press(&mut m, plain(KeyCode::Enter));
    assert!(!m.scrolled);
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
}

#[test]
fn focusing_finished_agent_sends_view() {
    let mut m = spec_model();
    m.selected = NodeKey::Agent(n("squad-2"));
    m.focus.pane = Pane::NeedsYou;
    assert_eq!(
        press(&mut m, plain(KeyCode::Tab)),
        [UiEffect::Send(Command::ViewAgent {
            agent: n("squad-2")
        })]
    );
    assert_eq!(m.focus.pane, Pane::Agent);
    // From the chat, backwards.
    m.focus.pane = Pane::Chat;
    assert_eq!(
        press(&mut m, plain(KeyCode::BackTab)),
        [UiEffect::Send(Command::ViewAgent {
            agent: n("squad-2")
        })]
    );
    // `enter` in the org pane focuses the pane too.
    m.focus.pane = Pane::Org;
    assert_eq!(
        press(&mut m, plain(KeyCode::Enter)),
        [UiEffect::Send(Command::ViewAgent {
            agent: n("squad-2")
        })]
    );
    // An idle agent: nothing.
    m.agents.get_mut(&n("squad-2")).unwrap().status = status(AgentState::Idle, mins(1));
    m.focus.pane = Pane::NeedsYou;
    assert_eq!(press(&mut m, plain(KeyCode::Tab)), []);
    assert_eq!(m.focus.pane, Pane::Agent);
}

#[test]
fn request_typing_starts_typing_when_live() {
    let mut m = spec_model();
    m.focus.pane = Pane::Agent;
    m.live_agent = Some(n("auth-service"));
    m.scrolled = true;
    request_typing(&mut m, false);
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
    assert!(!m.scrolled);
    assert_eq!(m.typing_wanted, None);
}

#[test]
fn request_typing_waits_when_not_live() {
    let mut m = spec_model();
    m.focus.pane = Pane::Agent;
    for live in [None, Some(n("billing-api"))] {
        m.live_agent = live;
        request_typing(&mut m, false);
        assert_eq!(m.focus.agent_mode, AgentMode::Selected);
        assert_eq!(m.typing_wanted, Some(n("auth-service")));
    }
}

#[test]
fn request_typing_takes_a_finished_agent() {
    let mut m = spec_model();
    m.selected = NodeKey::Agent(n("squad-2"));
    m.live_agent = Some(n("squad-2"));
    request_typing(&mut m, false);
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
}

#[test]
fn request_typing_ignores_stopped_and_errored_agents() {
    let mut m = spec_model();
    m.selected = NodeKey::Agent(n("pay-qa"));
    m.live_agent = Some(n("pay-qa"));
    m.typing_wanted = Some(n("auth-service"));
    request_typing(&mut m, false);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    assert_eq!(m.typing_wanted, None);
    assert_eq!(m.footer_note, None);
    m.agents.get_mut(&n("squad-2")).unwrap().status = status(AgentState::Errored, mins(1));
    m.selected = NodeKey::Agent(n("squad-2"));
    request_typing(&mut m, false);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    assert_eq!(m.typing_wanted, None);
}

#[test]
fn request_typing_on_a_team_does_nothing() {
    let mut m = spec_model();
    m.selected = NodeKey::Team(duohive_core::names::NodeName::parse("Payments").unwrap());
    request_typing(&mut m, false);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    assert_eq!(m.typing_wanted, None);
}

/// The org pane has focus with `node` selected and its live terminal showing.
fn org_row(node: NodeKey, live: Option<&str>) -> UiModel {
    let mut m = spec_model();
    m.focus.pane = Pane::Org;
    m.selected = node;
    m.live_agent = live.map(n);
    m
}

#[test]
fn enter_on_a_running_agent_row_starts_typing() {
    let mut m = org_row(NodeKey::Agent(n("auth-service")), Some("auth-service"));
    assert_eq!(press(&mut m, plain(KeyCode::Enter)), []);
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
    assert!(m.typing());
}

#[test]
fn enter_on_an_agent_row_not_live_yet_asks_to_type_once_it_is() {
    let mut m = org_row(NodeKey::Agent(n("auth-service")), None);
    press(&mut m, plain(KeyCode::Enter));
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    assert_eq!(m.typing_wanted, Some(n("auth-service")));
}

#[test]
fn enter_on_the_duo_row_starts_typing() {
    let mut m = org_row(NodeKey::Duo, Some("Duo"));
    press(&mut m, plain(KeyCode::Enter));
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
}

#[test]
fn enter_on_a_stopped_or_errored_agent_row_focuses_the_pane_in_selected() {
    let mut m = org_row(NodeKey::Agent(n("pay-qa")), Some("pay-qa"));
    press(&mut m, plain(KeyCode::Enter));
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    assert_eq!(m.typing_wanted, None);
    let mut m = org_row(NodeKey::Agent(n("squad-2")), Some("squad-2"));
    m.agents.get_mut(&n("squad-2")).unwrap().status = status(AgentState::Errored, mins(1));
    press(&mut m, plain(KeyCode::Enter));
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    assert_eq!(m.typing_wanted, None);
}

#[test]
fn enter_on_a_finished_agent_row_starts_typing_and_marks_it_seen() {
    let mut m = org_row(NodeKey::Agent(n("squad-2")), Some("squad-2"));
    let effects = press(&mut m, plain(KeyCode::Enter));
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
    assert_eq!(
        effects,
        [UiEffect::Send(Command::ViewAgent {
            agent: n("squad-2")
        })]
    );
}

#[test]
fn enter_on_a_team_row_still_folds() {
    let team = NodeKey::Team(duohive_core::names::NodeName::parse("Payments").unwrap());
    let mut m = org_row(team.clone(), None);
    press(&mut m, plain(KeyCode::Enter));
    assert!(m.folded.contains(&team));
    assert_eq!(m.focus.pane, Pane::Org);
    assert_eq!(m.typing_wanted, None);
}

#[test]
fn enter_in_needs_you_on_a_running_agent_starts_typing() {
    let mut m = spec_model();
    m.focus.pane = Pane::NeedsYou;
    m.focus.needs_you_cursor = 0;
    let agent = m.needs_you[0].agent.clone();
    m.live_agent = Some(agent.clone());
    press(&mut m, plain(KeyCode::Enter));
    assert_eq!(m.selected, NodeKey::Agent(agent));
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
}

#[test]
fn tab_and_arrows_never_start_typing() {
    let mut m = spec_model();
    m.live_agent = Some(n("auth-service"));
    m.focus.pane = Pane::NeedsYou;
    press(&mut m, plain(KeyCode::Tab));
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    m.focus.pane = Pane::Org;
    press(&mut m, plain(KeyCode::Down));
    press(&mut m, plain(KeyCode::Up));
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    assert_eq!(m.typing_wanted, None);
}

#[test]
fn keys_are_inert_while_typing_is_pending() {
    let mut m = spec_model();
    m.focus.pane = Pane::Agent;
    request_typing(&mut m, false);
    let note = m.footer_note.clone();
    assert_eq!(note.as_deref(), Some("opening auth-service…"));
    for c in [' ', 'z', 'r', 's', 'q', 'o', 'w'] {
        assert_eq!(press(&mut m, plain(KeyCode::Char(c))), [], "{c}");
        assert!(m.dialog.is_none() && !m.zoomed, "{c}");
        assert_eq!(m.typing_wanted, Some(n("auth-service")), "{c}");
        assert_eq!(m.footer_note, note, "{c}");
    }
    assert_eq!(
        press(&mut m, with(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        []
    );
}

#[test]
fn esc_tab_and_arrows_cancel_a_pending_request_and_act() {
    for code in [
        KeyCode::Esc,
        KeyCode::Tab,
        KeyCode::BackTab,
        KeyCode::Up,
        KeyCode::Down,
    ] {
        let mut m = spec_model();
        m.focus.pane = Pane::Agent;
        request_typing(&mut m, false);
        press(&mut m, plain(code));
        assert_eq!(m.typing_wanted, None, "{code:?}");
        assert_eq!(m.footer_note, None, "{code:?}");
    }
    let mut m = spec_model();
    m.focus.pane = Pane::Agent;
    request_typing(&mut m, false);
    press(&mut m, plain(KeyCode::Esc));
    assert_eq!(
        m.focus.pane,
        Pane::Org,
        "esc still goes back to the org pane"
    );
}

#[test]
fn request_typing_can_keep_a_scrolled_snapshot() {
    let mut m = spec_model();
    m.live_agent = Some(n("auth-service"));
    m.scrolled = true;
    request_typing(&mut m, true);
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
    assert!(m.scrolled);
}
