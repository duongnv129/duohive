//! Move to… (UI §4.4.2): opening it, its refusals and the edit `enter` sends.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::command::{Command, MoveTarget, OrgEdit};
use duohive_core::names::NodeName;
use duohive_core::org::{Fields, NodeKey, Org, Role, RoleKey};
use duohive_core::testkit::{example_org, n};

use super::{UiEffect, map_key, update};
use crate::fixtures::model_for;
use crate::focus::Pane;
use crate::model::{Dialog, UiModel};

fn nn(s: &str) -> NodeName {
    NodeName::parse(s).unwrap()
}

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

/// The spec's org: `pay-qa` in Payments › QA (worktrees off, prompt `QA`), and a root
/// Generalist role (worktrees on, prompt `Generalist`).
pub(crate) fn spec_org() -> Org {
    let mut org = example_org();
    for r in &mut org.teams[0].roles {
        if r.name == nn("QA") {
            r.defaults.worktree = Some(false);
            r.defaults.prompt = Some("QA".to_owned());
        }
    }
    for r in &mut org.roles {
        if r.name == nn("Generalist") {
            r.defaults.worktree = Some(true);
            r.defaults.prompt = Some("Generalist".to_owned());
        }
    }
    org
}

fn model_on(node: NodeKey) -> UiModel {
    let mut m = model_for(spec_org());
    m.selected = node;
    m.focus.pane = Pane::Org;
    m
}

fn picker(m: &UiModel) -> &crate::forms::MoveTo {
    match &m.dialog {
        Some(Dialog::MoveTo(p)) => p,
        other => panic!("no move picker: {other:?}"),
    }
}

#[test]
fn move_lists_inherited_changes() {
    let mut m = model_on(NodeKey::Agent(n("pay-qa")));
    press(&mut m, key('m'));
    let generalist = picker(&m)
        .options
        .iter()
        .find(|o| o.label == "Generalist")
        .unwrap();
    let changes: Vec<(&str, &str, &str)> = generalist
        .changes
        .iter()
        .map(|c| (c.field, c.from.as_str(), c.to.as_str()))
        .collect();
    assert!(changes.contains(&("worktrees", "off", "on")), "{changes:?}");
    assert!(
        changes.contains(&("role prompt", "QA", "Generalist")),
        "{changes:?}"
    );
}

#[test]
fn move_sends_org_edit_move() {
    let mut m = model_on(NodeKey::Agent(n("pay-qa")));
    press(&mut m, key('m'));
    // The cursor starts on the first place that isn't current.
    let to = picker(&m).selected().unwrap().to.clone();
    let out = press(&mut m, code(KeyCode::Enter));
    assert_eq!(
        out,
        [UiEffect::Send(Command::EditOrg(OrgEdit::Move {
            node: NodeKey::Agent(n("pay-qa")),
            to
        }))]
    );
    assert!(m.dialog.is_none());
}

#[test]
fn move_disabled_when_no_target() {
    let mut org = spec_org();
    org.teams.clear();
    org.roles.truncate(1);
    org.roles[0].agents.truncate(1);
    let only = org.roles[0].agents[0].name.clone();
    let mut m = model_for(org);
    m.selected = NodeKey::Agent(only.clone());
    m.focus.pane = Pane::Org;
    press(&mut m, key('m'));
    assert!(m.dialog.is_none());
    assert_eq!(
        m.footer_note.as_deref(),
        Some(format!("no other place to move {only}").as_str())
    );
}

#[test]
fn move_refused_on_role_name_collision() {
    let mut org = spec_org();
    // Payments already has a Generalist; the root one can't move in.
    org.teams[0].roles.push(Role {
        name: nn("Generalist"),
        defaults: Fields::default(),
        agents: Vec::new(),
    });
    let mut m = model_for(org);
    m.selected = NodeKey::Role(RoleKey {
        team: None,
        role: nn("Generalist"),
    });
    m.focus.pane = Pane::Org;
    press(&mut m, key('m'));
    let target = picker(&m)
        .options
        .iter()
        .position(|o| o.label == "Payments")
        .unwrap();
    while picker(&m).cursor != target {
        press(&mut m, code(KeyCode::Down));
    }
    assert_eq!(
        picker(&m).selected().unwrap().blocked.as_deref(),
        Some("Payments already has a role Generalist")
    );
    let out = press(&mut m, code(KeyCode::Enter));
    assert_eq!(out, [], "a refused move is not sent");
    assert!(m.dialog.is_some(), "the picker stays up with the reason");
}

#[test]
fn m_on_duo_does_nothing() {
    let mut m = model_on(NodeKey::Duo);
    assert_eq!(press(&mut m, key('m')), []);
    assert!(m.dialog.is_none() && m.footer_note.is_none());
}

#[test]
fn esc_cancels() {
    let mut m = model_on(NodeKey::Agent(n("pay-qa")));
    press(&mut m, key('m'));
    assert_eq!(press(&mut m, code(KeyCode::Esc)), []);
    assert!(m.dialog.is_none());
}

#[test]
fn move_on_team_sends_a_position() {
    let mut org = spec_org();
    org.teams.push(duohive_core::org::Team {
        name: nn("Mobile"),
        defaults: Fields::default(),
        roles: Vec::new(),
    });
    let mut m = model_for(org);
    m.selected = NodeKey::Team(nn("Mobile"));
    m.focus.pane = Pane::Org;
    press(&mut m, key('m'));
    let out = press(&mut m, code(KeyCode::Enter));
    assert_eq!(
        out,
        [UiEffect::Send(Command::EditOrg(OrgEdit::Move {
            node: NodeKey::Team(nn("Mobile")),
            to: MoveTarget::Before(nn("Payments")),
        }))]
    );
}
