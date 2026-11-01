//! `r`: resume an old session of a stopped or errored agent (UI §4.13).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::command::Command;
use duohive_core::names::AgentName;
use duohive_core::org::{NodeKey, RoleKey};
use duohive_core::status::AgentState;
use duohive_core::testkit::n;

use super::{UiEffect, map_key, update};
use crate::fixtures::{mins, spec_model, status, view};
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
