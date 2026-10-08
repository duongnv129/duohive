//! Focus, folding, chat resizing and the chat pane's keys (UI §3, §4.11, P2-D8, P2-D9).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::command::Command;
use duohive_core::event::Event;
use duohive_core::names::NodeName;
use duohive_core::org::{NodeKey, RoleKey};
use duohive_core::status::AgentState;
use duohive_core::testkit::n;
use duohive_core::worktree::{RunIn, TaskId};
use ratatui::layout::Rect;

use super::{Action, UiEffect, map_key, update};
use crate::chat::{ChatLine, Who};
use crate::fixtures::{NOW, mins, spec_model, status};
use crate::focus::{Pane, layout};
use crate::model::UiModel;
use crate::tree::tree_rows;

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn code(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::NONE)
}

fn ctrl(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::CONTROL)
}

fn nn(s: &str) -> NodeName {
    NodeName::parse(s).unwrap()
}

fn qa_role() -> NodeKey {
    NodeKey::Role(RoleKey {
        team: Some(nn("Payments")),
        role: nn("QA"),
    })
}

/// Maps and applies one key, as the app loop does.
fn press(m: &mut UiModel, k: KeyEvent) -> Vec<UiEffect> {
    match map_key(m, k) {
        Some(a) => update(m, a),
        None => Vec::new(),
    }
}

fn type_text(m: &mut UiModel, text: &str) {
    for c in text.chars() {
        assert_eq!(press(m, key(c)), []);
    }
}

fn chat_focused() -> UiModel {
    let mut m = spec_model();
    m.focus.pane = Pane::Chat;
    m
}

fn set_duo(m: &mut UiModel, state: AgentState) {
    m.agents.get_mut(&n("Duo")).unwrap().status = status(state, mins(1));
}

#[test]
fn tab_cycles_org_needs_you_agent_chat() {
    for needs_you in [true, false] {
        let mut m = spec_model();
        if !needs_you {
            m.needs_you.clear();
        }
        let mut seen = vec![m.focus.pane];
        for _ in 0..4 {
            assert_eq!(press(&mut m, code(KeyCode::Tab)), []);
            seen.push(m.focus.pane);
        }
        assert_eq!(
            seen,
            [
                Pane::Org,
                Pane::NeedsYou,
                Pane::Agent,
                Pane::Chat,
                Pane::Org
            ]
        );
    }
}

#[test]
fn shift_tab_reverses() {
    let mut m = spec_model();
    let mut seen = Vec::new();
    for _ in 0..4 {
        press(&mut m, code(KeyCode::BackTab));
        seen.push(m.focus.pane);
    }
    assert_eq!(seen, [Pane::Chat, Pane::Agent, Pane::NeedsYou, Pane::Org]);
}

#[test]
fn ctrl_arrows_resize_chat_within_p2_d3_bounds() {
    let mut m = spec_model();
    m.screen = Rect::new(0, 0, 120, 36);
    assert_eq!(map_key(&m, ctrl(KeyCode::Up)), Some(Action::ChatGrow));
    assert_eq!(map_key(&m, ctrl(KeyCode::Down)), Some(Action::ChatShrink));
    let chat = |m: &UiModel| {
        layout(m.screen, m.chat_rows_extra, false)
            .unwrap()
            .chat
            .height
    };
    press(&mut m, ctrl(KeyCode::Up));
    assert_eq!(chat(&m), 7);
    for _ in 0..40 {
        press(&mut m, ctrl(KeyCode::Up));
    }
    // The agent pane keeps 6 rows: 36 - header - footer - 6.
    assert_eq!(m.chat_rows_extra, 22);
    assert_eq!(chat(&m), 28);
    for _ in 0..40 {
        press(&mut m, ctrl(KeyCode::Down));
    }
    // Never below UI §3's size.
    assert_eq!(m.chat_rows_extra, 0);
    assert_eq!(chat(&m), 6);
    // It works from the chat too, where letters type.
    m.focus.pane = Pane::Chat;
    press(&mut m, ctrl(KeyCode::Up));
    assert_eq!(m.chat_rows_extra, 1);
}

#[test]
fn fold_hides_children_and_shows_count() {
    let mut m = spec_model();
    m.selected = qa_role();
    assert_eq!(press(&mut m, code(KeyCode::Left)), []);
    assert!(m.folded.contains(&qa_role()));
    let rows = tree_rows(&m, true, false, NOW);
    assert!(rows.iter().all(|r| r.label != "pay-qa"));
    let qa = rows.iter().find(|r| r.key == Some(qa_role())).unwrap();
    assert_eq!(qa.label, "▸ QA (1)");
    // The wide tree hides the children too, and keeps the count.
    let wide = tree_rows(&m, false, true, NOW);
    assert!(wide.iter().all(|r| r.label != "pay-qa"));
    assert!(wide.iter().any(|r| r.label == "◇ QA (1)"));
    assert_eq!(press(&mut m, code(KeyCode::Right)), []);
    assert!(!m.folded.contains(&qa_role()));
    // `enter` on a group toggles.
    press(&mut m, code(KeyCode::Enter));
    assert!(m.folded.contains(&qa_role()));
    press(&mut m, code(KeyCode::Enter));
    assert!(m.folded.is_empty());
}

#[test]
fn moving_skips_folded_children() {
    let mut m = spec_model();
    let team = NodeKey::Team(nn("Payments"));
    m.selected = team.clone();
    press(&mut m, code(KeyCode::Left));
    press(&mut m, code(KeyCode::Down));
    assert_eq!(
        m.selected,
        NodeKey::Role(RoleKey {
            team: None,
            role: nn("Generalist"),
        })
    );
    // ← on an agent goes to its role.
    m.selected = NodeKey::Agent(n("squad-1"));
    press(&mut m, code(KeyCode::Left));
    assert_eq!(
        m.selected,
        NodeKey::Role(RoleKey {
            team: None,
            role: nn("Generalist"),
        })
    );
    assert!(!m.folded.contains(&m.selected));
}

#[test]
fn enter_on_an_agent_focuses_the_agent_pane() {
    let mut m = spec_model();
    assert_eq!(press(&mut m, code(KeyCode::Enter)), []);
    assert_eq!(m.focus.pane, Pane::Agent);
    let mut m = spec_model();
    m.selected = NodeKey::Duo;
    press(&mut m, code(KeyCode::Enter));
    assert_eq!(m.focus.pane, Pane::Agent);
}

#[test]
fn needs_you_arrows_move_its_cursor() {
    let mut m = spec_model();
    m.focus.pane = Pane::NeedsYou;
    press(&mut m, code(KeyCode::Down));
    assert_eq!(m.focus.needs_you_cursor, 1);
    press(&mut m, code(KeyCode::Down));
    assert_eq!(m.focus.needs_you_cursor, 1);
    press(&mut m, code(KeyCode::Up));
    press(&mut m, code(KeyCode::Up));
    assert_eq!(m.focus.needs_you_cursor, 0);
    // The tree's selection doesn't move, and the tree's keys don't act here.
    assert_eq!(m.selected, NodeKey::Agent(n("auth-service")));
    assert_eq!(map_key(&m, key('s')), None);
}

#[test]
fn esc_never_quits() {
    for pane in [Pane::Org, Pane::NeedsYou, Pane::Agent, Pane::Chat] {
        let mut m = spec_model();
        m.focus.pane = pane;
        assert!(!press(&mut m, code(KeyCode::Esc)).contains(&UiEffect::Quit));
    }
}

#[test]
fn chat_enter_gives_work_to_duo() {
    let mut m = chat_focused();
    type_text(&mut m, "what's blocking Payments?");
    assert_eq!(
        press(&mut m, code(KeyCode::Enter)),
        [UiEffect::Send(Command::GiveWork {
            agent: n("Duo"),
            task: "what's blocking Payments?".to_owned(),
            run: RunIn::Directory,
        })]
    );
    assert_eq!(m.chat_input.text(), "");
    assert_eq!(
        m.chat.lines,
        [ChatLine {
            at: m.now,
            who: Who::You,
            text: "what's blocking Payments?".to_owned(),
        }]
    );
    // An empty input sends nothing.
    assert_eq!(press(&mut m, code(KeyCode::Enter)), []);
    // alt+enter is a new line, sent as part of the task.
    type_text(&mut m, "a");
    press(&mut m, KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
    type_text(&mut m, "b");
    assert_eq!(m.chat_input.text(), "a\nb");
}

#[test]
fn chat_refused_keeps_text_and_notes() {
    let mut m = chat_focused();
    set_duo(&mut m, AgentState::Waiting);
    type_text(&mut m, "hi");
    assert_eq!(press(&mut m, code(KeyCode::Enter)), []);
    assert_eq!(m.chat_input.text(), "hi");
    assert_eq!(
        m.footer_note.as_deref(),
        Some("Duo is waiting · send again when it is at rest")
    );
    assert!(m.chat.lines.is_empty());
    // The engine refuses too (Duo got busy meanwhile): the text comes back.
    let mut m = chat_focused();
    type_text(&mut m, "hi");
    press(&mut m, code(KeyCode::Enter));
    assert!(m.apply(Event::GiveWorkRefused {
        agent: n("Duo"),
        state: AgentState::Working,
        pending: false,
    }));
    assert_eq!(m.chat_input.text(), "hi");
    assert_eq!(
        m.footer_note.as_deref(),
        Some("Duo is working · send again when it is at rest")
    );
    assert!(m.chat.lines.is_empty());
    // A delivery still pending (P2-D11).
    m.apply(Event::GiveWorkRefused {
        agent: n("Duo"),
        state: AgentState::Idle,
        pending: true,
    });
    assert_eq!(
        m.footer_note.as_deref(),
        Some("Duo is still taking the last task")
    );
}

#[test]
fn chat_not_taken_returns_text() {
    let mut m = chat_focused();
    type_text(&mut m, "run the tests");
    press(&mut m, code(KeyCode::Enter));
    assert_eq!(m.chat_input.text(), "");
    assert!(m.apply(Event::GiveWorkNotTaken {
        agent: n("Duo"),
        task: TaskId("run-the-tests".to_owned()),
        reason: "no sign it started".to_owned(),
    }));
    assert_eq!(m.chat_input.text(), "run the tests");
    assert_eq!(
        m.footer_note.as_deref(),
        Some("Duo didn't take the task · enter to send again")
    );
}

#[test]
fn letter_keys_type_in_chat() {
    let mut m = chat_focused();
    for c in ['q', 'w', ',', 'z', 's'] {
        assert_eq!(map_key(&m, key(c)), Some(Action::Char(c)));
        let effects = press(&mut m, key(c));
        assert!(effects.is_empty(), "{c}: {effects:?}");
        assert_eq!(m.dialog, None);
    }
    assert_eq!(m.chat_input.text(), "qw,zs");
    assert_eq!(m.focus.pane, Pane::Chat);
    // ctrl+c doesn't quit while typing.
    assert_eq!(map_key(&m, ctrl(KeyCode::Char('c'))), None);
    press(&mut m, code(KeyCode::Backspace));
    press(&mut m, code(KeyCode::Left));
    press(&mut m, code(KeyCode::Delete));
    assert_eq!(m.chat_input.text(), "qw,");
}

#[test]
fn duo_turn_end_adds_its_reply_to_chat() {
    let mut m = chat_focused();
    set_duo(&mut m, AgentState::Working);
    m.apply(Event::LastReplyChanged {
        agent: n("Duo"),
        reply: Some("billing-api is waiting on approval.".to_owned()),
    });
    let idle = status(AgentState::Idle, mins(0));
    m.apply(Event::AgentStatusChanged {
        agent: n("Duo"),
        status: idle.clone(),
    });
    assert_eq!(
        m.chat.lines,
        [ChatLine {
            at: idle.since,
            who: Who::Duo,
            text: "billing-api is waiting on approval.".to_owned(),
        }]
    );
    // Idle again (a repeat) adds nothing; another agent's turn end adds nothing.
    m.apply(Event::AgentStatusChanged {
        agent: n("Duo"),
        status: idle,
    });
    m.apply(Event::AgentStatusChanged {
        agent: n("squad-1"),
        status: status(AgentState::Idle, mins(0)),
    });
    assert_eq!(m.chat.lines.len(), 1);
}

#[test]
fn paste_goes_into_the_focused_chat() {
    let mut m = chat_focused();
    type_text(&mut m, "a");
    super::paste(&mut m, "b c\nd");
    assert_eq!(m.chat_input.text(), "ab c\nd");
    // Elsewhere, with no form open, a paste is dropped as before.
    let mut m = spec_model();
    super::paste(&mut m, "x");
    assert_eq!(m.chat_input.text(), "");
}

#[test]
fn returned_task_goes_back_before_newly_typed_text() {
    let mut m = chat_focused();
    type_text(&mut m, "run the tests");
    press(&mut m, code(KeyCode::Enter));
    type_text(&mut m, "and lint");
    m.apply(Event::GiveWorkNotTaken {
        agent: n("Duo"),
        task: TaskId("run-the-tests".to_owned()),
        reason: "no sign it started".to_owned(),
    });
    assert_eq!(m.chat_input.text(), "run the tests\nand lint");
    assert!(m.chat.lines.is_empty());
}

#[test]
fn same_reply_on_a_later_turn_is_kept() {
    let mut m = chat_focused();
    let reply = "done.";
    m.apply(Event::LastReplyChanged {
        agent: n("Duo"),
        reply: Some(reply.to_owned()),
    });
    // Duo launching (starting → idle) is not a turn: nothing joins the history.
    set_duo(&mut m, AgentState::Starting);
    m.apply(Event::AgentStatusChanged {
        agent: n("Duo"),
        status: status(AgentState::Idle, mins(0)),
    });
    assert!(m.chat.lines.is_empty());
    for _ in 0..2 {
        m.apply(Event::AgentStatusChanged {
            agent: n("Duo"),
            status: status(AgentState::Working, mins(0)),
        });
        m.apply(Event::AgentStatusChanged {
            agent: n("Duo"),
            status: status(AgentState::Idle, mins(0)),
        });
    }
    let texts: Vec<&str> = m.chat.lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, [reply, reply]);
}

#[test]
fn chat_resizing_leaves_room_for_banners() {
    let mut m = spec_model();
    m.screen = Rect::new(0, 0, 120, 36);
    m.org_error = Some(duohive_core::ports::OrgFileError {
        line: Some(1),
        message: "bad".to_owned(),
    });
    for _ in 0..40 {
        press(&mut m, ctrl(KeyCode::Up));
    }
    // One banner row: 36 - banner - header - footer - 6 agent rows - 6 chat rows.
    assert_eq!(m.chat_rows_extra, 21);
}
