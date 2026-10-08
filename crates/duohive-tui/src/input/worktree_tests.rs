//! The worktree list's keys (UI §4.9, P2-D6).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::command::Command;
use duohive_core::testkit::n;
use duohive_core::worktree::{TaskId, WorktreeState};

use super::{Action, UiEffect, map_key, update};
use crate::fixtures::{spec_model, spec_worktrees, worktree_row};
use crate::focus::{AgentMode, Pane};
use crate::model::{Dialog, UiModel};

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn press(m: &mut UiModel, k: KeyEvent) -> Vec<UiEffect> {
    match map_key(m, k) {
        Some(a) => update(m, a),
        None => Vec::new(),
    }
}

fn open() -> UiModel {
    let mut m = spec_model();
    m.worktrees = spec_worktrees();
    m.focus.pane = Pane::Org;
    assert_eq!(
        press(&mut m, key('w')),
        [UiEffect::Send(Command::CheckWorktrees)]
    );
    m
}

fn cursor_to(m: &mut UiModel, row: usize) {
    for _ in 0..row {
        press(m, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
}

fn delete_of(agent: &str, task: &str, confirmed: bool) -> UiEffect {
    UiEffect::Send(Command::DeleteWorktree {
        agent: n(agent),
        task: TaskId(task.to_owned()),
        confirmed,
    })
}

#[test]
fn w_opens_and_requests_check() {
    let m = open();
    assert!(matches!(m.dialog, Some(Dialog::Worktrees(_))));
    for pane in [Pane::NeedsYou, Pane::Agent] {
        let mut m = spec_model();
        m.focus.pane = pane;
        assert_eq!(
            press(&mut m, key('w')),
            [UiEffect::Send(Command::CheckWorktrees)]
        );
    }
}

#[test]
fn w_types_in_chat_input() {
    let mut m = spec_model();
    m.focus.pane = Pane::Chat;
    assert_eq!(map_key(&m, key('w')), Some(Action::Char('w')));
    press(&mut m, key('w'));
    assert_eq!(m.chat_input.text(), "w");
    assert!(m.dialog.is_none());

    // Typing mode sends it to the agent.
    let mut m = spec_model();
    m.live_agent = Some(n("auth-service"));
    m.focus.pane = Pane::Agent;
    m.focus.agent_mode = AgentMode::Typing;
    assert!(matches!(map_key(&m, key('w')), Some(Action::Type(_))));
}

#[test]
fn w_types_in_a_form() {
    let mut m = spec_model();
    m.focus.pane = Pane::Org;
    press(&mut m, key('e'));
    assert!(matches!(m.dialog, Some(Dialog::Form(_))));
    assert_eq!(map_key(&m, key('w')), Some(Action::Char('w')));
}

#[test]
fn d_on_merged_clean_removes_without_asking() {
    let mut m = open();
    m.worktrees = vec![worktree_row("pay-qa", "retry-cases", WorktreeState::Merged)];
    assert_eq!(
        press(&mut m, key('d')),
        [delete_of("pay-qa", "retry-cases", false)]
    );
    assert!(matches!(m.dialog, Some(Dialog::Worktrees(_))));
}

#[test]
fn d_on_unmerged_asks_with_cancel_focused() {
    let mut m = open();
    cursor_to(&mut m, 2);
    assert_eq!(press(&mut m, key('d')), []);
    let Some(Dialog::Confirm(c)) = &m.dialog else {
        panic!("no confirmation");
    };
    assert_eq!(c.title, "Remove worktree?");
    assert!(!c.focus_yes);
    assert_eq!(c.yes_label, "Remove");
    assert!(
        c.lines
            .iter()
            .any(|l| l.contains("duohive/squad-2/flaky-date"))
    );
    // y sends the confirmed removal, then the list is back.
    assert_eq!(
        press(&mut m, key('y')),
        [delete_of("squad-2", "flaky-date", true)]
    );
    assert!(matches!(m.dialog, Some(Dialog::Worktrees(_))));
}

#[test]
fn cancelling_the_confirmation_returns_to_the_list() {
    let mut m = open();
    cursor_to(&mut m, 3);
    press(&mut m, key('d'));
    assert!(matches!(m.dialog, Some(Dialog::Confirm(_))));
    assert_eq!(press(&mut m, key('n')), []);
    let Some(Dialog::Worktrees(l)) = &m.dialog else {
        panic!("no list");
    };
    assert_eq!(l.cursor, 3);
}

#[test]
fn d_on_in_use_notes() {
    let mut m = open();
    assert_eq!(press(&mut m, key('d')), []);
    assert_eq!(
        m.footer_note.as_deref(),
        Some("auth-service is running in it · stop it first")
    );
    assert!(matches!(m.dialog, Some(Dialog::Worktrees(_))));
}

#[test]
fn enter_goes_to_agent() {
    let mut m = open();
    cursor_to(&mut m, 2);
    press(&mut m, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(m.dialog.is_none());
    assert_eq!(m.selected, duohive_core::org::NodeKey::Agent(n("squad-2")));
    assert_eq!(m.focus.pane, Pane::Agent);
}

#[test]
fn esc_goes_back_and_never_quits() {
    let mut m = open();
    let out = press(&mut m, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(out, []);
    assert!(m.dialog.is_none());
}

#[test]
fn an_event_keeps_the_cursor_on_its_row() {
    let mut m = open();
    cursor_to(&mut m, 2);
    let rows = spec_worktrees().into_iter().skip(1).collect();
    m.apply(duohive_core::event::Event::Worktrees(rows));
    let Some(Dialog::Worktrees(l)) = &m.dialog else {
        panic!("no list");
    };
    assert_eq!(l.cursor, 1);
}

#[test]
fn edit_locks_the_name_of_an_agent_with_a_worktree_row() {
    let mut m = spec_model();
    m.agents.get_mut(&n("pay-qa")).unwrap().status = crate::fixtures::status(
        duohive_core::status::AgentState::Stopped,
        crate::fixtures::mins(1),
    );
    m.worktrees = vec![worktree_row(
        "pay-qa",
        "retry-cases",
        WorktreeState::MergedDirty,
    )];
    m.selected = duohive_core::org::NodeKey::Agent(n("pay-qa"));
    m.focus.pane = Pane::Org;
    press(&mut m, key('e'));
    let Some(Dialog::Form(f)) = &m.dialog else {
        panic!("no form");
    };
    let name = f.field(crate::forms::FieldId::Name).unwrap();
    assert_eq!(name.kind, crate::forms::FieldKind::ReadOnly);
}
