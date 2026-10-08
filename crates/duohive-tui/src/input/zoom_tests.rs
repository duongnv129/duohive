//! `z`: zoom the agent pane and restore (UI §4.2, P2-D5, P2-D15).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::names::NodeName;
use duohive_core::org::NodeKey;
use duohive_core::status::AgentState;
use duohive_core::testkit::n;
use duohive_core::view::PaneInput;
use ratatui::layout::Rect;

use super::{UiEffect, map_key, update};
use crate::fixtures::spec_model;
use crate::focus::{AgentMode, Pane};
use crate::model::UiModel;
use crate::render::live_size;

fn press(m: &mut UiModel, c: KeyCode) -> Vec<UiEffect> {
    match map_key(m, KeyEvent::new(c, KeyModifiers::NONE)) {
        Some(a) => update(m, a),
        None => Vec::new(),
    }
}

fn z() -> KeyCode {
    KeyCode::Char('z')
}

fn model() -> UiModel {
    let mut m = spec_model();
    m.screen = Rect::new(0, 0, 100, 30);
    m
}

#[test]
fn z_toggles_zoom_and_resizes_view() {
    let mut m = model();
    let normal = live_size(&m).unwrap();
    assert!(!m.zoomed);
    press(&mut m, z());
    assert!(m.zoomed);
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    // Full width and every row between the header and the footer, inside the border.
    let zoomed = live_size(&m).unwrap();
    assert_eq!((zoomed.cols, zoomed.rows), (100 - 4, 30 - 2 - 2));
    assert!(zoomed.cols > normal.cols && zoomed.rows > normal.rows);
    // From the agent pane in Selected mode, `z` restores.
    press(&mut m, z());
    assert!(!m.zoomed);
    assert_eq!(live_size(&m).unwrap(), normal);
    // `z` from the agent pane zooms too.
    press(&mut m, z());
    assert!(m.zoomed);
}

#[test]
fn zoom_from_agent_pane_selected_mode() {
    let mut m = model();
    m.focus.pane = Pane::Agent;
    press(&mut m, z());
    assert!(m.zoomed);
}

#[test]
fn zoomed_keys_enter_types_and_page_scrolls() {
    let mut m = model();
    m.live_agent = Some(n("auth-service"));
    press(&mut m, z());
    assert_eq!(
        press(&mut m, KeyCode::PageUp),
        [UiEffect::Scroll { up: true }]
    );
    press(&mut m, KeyCode::Enter);
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
    assert!(m.zoomed);
}

#[test]
fn z_while_typing_is_sent_to_agent() {
    let mut m = model();
    m.live_agent = Some(n("auth-service"));
    press(&mut m, z());
    press(&mut m, KeyCode::Enter);
    assert_eq!(
        press(&mut m, z()),
        [UiEffect::Pane(PaneInput::Text("z".into()))]
    );
    assert!(m.zoomed);
}

#[test]
fn z_on_stopped_agent_zooms_its_stopped_line() {
    let mut m = model();
    m.selected = NodeKey::Agent(n("pay-qa"));
    assert_eq!(m.view(&n("pay-qa")).status.state, AgentState::Stopped);
    press(&mut m, z());
    assert!(m.zoomed);
}

#[test]
fn z_on_role_notes() {
    let mut m = model();
    m.selected = NodeKey::Team(NodeName::parse("Payments").unwrap());
    press(&mut m, z());
    assert!(!m.zoomed);
    assert_eq!(m.footer_note.as_deref(), Some("z zooms an agent"));
    // Any key clears the note.
    press(&mut m, KeyCode::Down);
    assert_eq!(m.footer_note, None);
}

#[test]
fn esc_restores_while_zoomed_and_never_quits() {
    let mut m = model();
    press(&mut m, z());
    assert_eq!(
        map_key(&m, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        Some(super::Action::Esc)
    );
    assert_eq!(press(&mut m, KeyCode::Esc), []);
    assert!(!m.zoomed);
    // Typing keeps its keys: esc goes to the agent.
    m.live_agent = Some(n("auth-service"));
    press(&mut m, z());
    press(&mut m, KeyCode::Enter);
    assert_eq!(
        press(&mut m, KeyCode::Esc),
        [UiEffect::Pane(PaneInput::Key(
            duohive_core::view::PaneKey::Escape
        ))]
    );
    assert!(m.zoomed);
}

#[test]
fn restore_returns_focus_to_the_pane_zoom_started_from() {
    let mut m = model();
    assert_eq!(m.focus.pane, Pane::Org);
    press(&mut m, z());
    assert_eq!(m.focus.pane, Pane::Agent);
    press(&mut m, z());
    assert_eq!(m.focus.pane, Pane::Org);
    m.focus.pane = Pane::Agent;
    press(&mut m, z());
    press(&mut m, KeyCode::Esc);
    assert_eq!(m.focus.pane, Pane::Agent);
}

#[test]
fn org_change_to_a_team_selection_clears_zoom() {
    use duohive_core::event::Event;
    let mut m = model();
    press(&mut m, z());
    assert!(m.zoomed);
    let mut org = m.org.clone();
    for t in &mut org.teams {
        for r in &mut t.roles {
            r.agents.retain(|a| a.name != n("auth-service"));
        }
    }
    m.apply(Event::OrgChanged {
        org: Box::new(org),
        problems: Default::default(),
    });
    assert_eq!(m.selected, NodeKey::Duo);
    // Duo is an agent, so zoom stays; a selection that is no agent clears it.
    m.selected = NodeKey::Team(NodeName::parse("Payments").unwrap());
    m.apply(Event::OrgChanged {
        org: Box::new(m.org.clone()),
        problems: Default::default(),
    });
    assert!(!m.zoomed);
}
