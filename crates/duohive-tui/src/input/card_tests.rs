//! Approval cards' keys and clicks (UI §4.6, §4.1.3, §4.11; P3-D7, P3-D22, P2-D25).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use duohive_core::card::{Card, CardChange, CardId};
use duohive_core::command::{Command, MoveTarget, OrgEdit};
use duohive_core::org::NodeKey;
use duohive_core::testkit::n;
use ratatui::layout::Rect;

use super::mouse::on_mouse;
use super::{UiEffect, map_key, update};
use crate::fixtures::{
    add_agent_card, card, clock, ctx, mins, new_agent_card, qa_role, raise, render_lines,
    spec_model, stop_card,
};
use crate::focus::Pane;
use crate::forms::{FieldId, FormPurpose};
use crate::model::{Dialog, UiModel};

const W: u16 = 160;
const H: u16 = 48;

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn code(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn press(m: &mut UiModel, k: KeyEvent) -> Vec<UiEffect> {
    match map_key(m, k) {
        Some(a) => update(m, a),
        None => Vec::new(),
    }
}

/// The chat focused, with `cards` pending (oldest first), raised 2m and 1m ago.
fn chat_with(cards: Vec<Card>) -> UiModel {
    let mut m = spec_model();
    m.screen = Rect::new(0, 0, W, H);
    let count = u64::try_from(cards.len()).unwrap();
    for (i, c) in (0..).zip(cards) {
        raise(&mut m, c, mins(count - i));
    }
    m.focus.pane = Pane::Chat;
    m
}

fn focused(c: Card) -> UiModel {
    let id = c.id;
    let mut m = chat_with(vec![c]);
    assert_eq!(press(&mut m, code(KeyCode::Up)), []);
    assert_eq!(m.focus.card, Some(id));
    m
}

fn approve(id: u64, edited: Option<CardChange>) -> Vec<UiEffect> {
    vec![UiEffect::Send(Command::ApproveCard {
        card: CardId(id),
        edited: edited.map(Box::new),
    })]
}

fn footer(m: &UiModel) -> String {
    render_lines(m, &ctx(), W, H).last().cloned().unwrap()
}

#[test]
fn up_from_input_focuses_newest_card() {
    let mut m = chat_with(vec![
        stop_card(1, clock(10, 15)),
        add_agent_card(2, clock(10, 21)),
    ]);
    assert_eq!(press(&mut m, code(KeyCode::Up)), []);
    assert_eq!(m.focus.card, Some(CardId(2)));
    assert!(footer(&m).starts_with(" y approve · e edit · n reject · esc input"));

    let mut m = chat_with(vec![
        stop_card(1, clock(10, 15)),
        add_agent_card(2, clock(10, 21)),
    ]);
    press(&mut m, key('h'));
    press(&mut m, code(KeyCode::Up));
    assert_eq!(m.focus.card, None);
    assert_eq!(m.chat_input.text(), "h");
}

#[test]
fn needs_you_enter_focuses_older_card() {
    let mut m = chat_with(vec![
        stop_card(1, clock(10, 15)),
        add_agent_card(2, clock(10, 21)),
    ]);
    m.focus.pane = Pane::NeedsYou;
    let older = m
        .needs_you
        .iter()
        .position(|i| i.card == Some(CardId(1)))
        .unwrap();
    m.focus.needs_you_cursor = older;
    assert_eq!(press(&mut m, code(KeyCode::Enter)), []);
    assert_eq!(m.focus.pane, Pane::Chat);
    assert_eq!(m.focus.card, Some(CardId(1)));
    // The older card takes the keys.
    assert_eq!(
        press(&mut m, key('n')),
        [UiEffect::Send(Command::RejectCard { card: CardId(1) })]
    );
}

#[test]
fn yen_only_when_focused() {
    let mut m = chat_with(vec![add_agent_card(2, clock(10, 21))]);
    for c in ['y', 'e', 'n'] {
        assert_eq!(press(&mut m, key(c)), []);
    }
    assert_eq!(m.chat_input.text(), "yen");
    assert_eq!(m.dialog, None);

    let mut m = focused(add_agent_card(2, clock(10, 21)));
    assert_eq!(press(&mut m, key('y')), approve(2, None));
    assert_eq!(m.chat_input.text(), "");

    let mut m = focused(add_agent_card(2, clock(10, 21)));
    assert_eq!(
        press(&mut m, key('n')),
        [UiEffect::Send(Command::RejectCard { card: CardId(2) })]
    );
}

#[test]
fn edit_saves_as_approval() {
    let mut m = focused(add_agent_card(2, clock(10, 21)));
    assert_eq!(press(&mut m, key('e')), []);
    let Some(Dialog::Form(f)) = &m.dialog else {
        panic!("no form: {:?}", m.dialog);
    };
    assert_eq!(f.purpose, FormPurpose::Card(CardId(2)));
    assert_eq!(f.title, "New agent · Payments › QA");
    assert_eq!(f.field(FieldId::Name).unwrap().edit.text(), "pay-web");
    assert_eq!(
        f.field(FieldId::Directory).unwrap().edit.text(),
        "~/code/web"
    );
    assert_eq!(f.field(FieldId::Cli).unwrap().choice(), "Claude Code");
    assert_eq!(f.field(FieldId::Worktrees).unwrap().choice(), "on");
    let CardChange::Org(proposed) = add_agent_card(2, clock(10, 21)).change else {
        unreachable!()
    };
    assert_eq!(
        press(&mut m, ctrl('s')),
        approve(2, Some(CardChange::Org(proposed)))
    );
    assert_eq!(m.dialog, None);
    assert_eq!(m.focus.pane, Pane::Chat);
}

#[test]
fn e_on_stop_card_disabled() {
    let remove = card(
        3,
        clock(10, 21),
        CardChange::Org(OrgEdit::Delete(NodeKey::Agent(n("pay-qa")))),
    );
    let moving = card(
        4,
        clock(10, 21),
        CardChange::Org(OrgEdit::Move {
            node: NodeKey::Agent(n("pay-qa")),
            to: MoveTarget::Into(NodeKey::Role(qa_role())),
        }),
    );
    for c in [stop_card(2, clock(10, 21)), remove, moving] {
        let mut m = focused(c);
        assert!(footer(&m).starts_with(" y approve · n reject · esc input"));
        assert_eq!(press(&mut m, key('e')), []);
        assert_eq!(m.dialog, None);
        assert_eq!(m.footer_note.as_deref(), Some("nothing to edit · y or n"));
        assert!(footer(&m).starts_with(" nothing to edit · y or n"));
    }
}

#[test]
fn e_on_new_agent_card_keeps_task() {
    let c = new_agent_card(2, clock(10, 21));
    let mut m = focused(c.clone());
    press(&mut m, key('e'));
    let Some(Dialog::Form(f)) = &m.dialog else {
        panic!("no form: {:?}", m.dialog);
    };
    assert_eq!(f.field(FieldId::Name).unwrap().edit.text(), "squad-4");
    assert_eq!(f.footer.as_deref(), Some("Task  PAY-88d docs: retry guide"));
    press(&mut m, key('x'));
    let CardChange::NewAgent {
        role,
        mut agent,
        task,
        run,
        clone_of,
    } = c.change
    else {
        unreachable!()
    };
    agent.name = n("squad-4x");
    let edited = CardChange::NewAgent {
        role,
        agent,
        task,
        run,
        clone_of,
    };
    assert_eq!(press(&mut m, ctrl('s')), approve(2, Some(edited)));
}

#[test]
fn esc_in_card_form_returns_to_card() {
    let mut m = focused(add_agent_card(2, clock(10, 21)));
    press(&mut m, key('e'));
    assert_eq!(press(&mut m, code(KeyCode::Esc)), []);
    assert_eq!(m.dialog, None);
    assert_eq!(m.focus.pane, Pane::Chat);
    assert_eq!(m.focus.card, Some(CardId(2)));
}

#[test]
fn esc_from_card_returns_to_input() {
    let mut m = focused(add_agent_card(2, clock(10, 21)));
    assert_eq!(press(&mut m, code(KeyCode::Esc)), []);
    assert_eq!(m.focus.pane, Pane::Chat);
    assert_eq!(m.focus.card, None);
    // Another letter types again.
    press(&mut m, key('y'));
    assert_eq!(m.chat_input.text(), "y");
}

/// Where `text` first shows on screen: (column, row).
fn find(m: &UiModel, text: &str) -> (u16, u16) {
    let lines = render_lines(m, &ctx(), W, H);
    let (y, line) = lines
        .iter()
        .enumerate()
        .find(|(_, l)| l.contains(text))
        .unwrap_or_else(|| panic!("{text} not on screen:\n{}", lines.join("\n")));
    let x = line[..line.find(text).unwrap()].chars().count();
    (u16::try_from(x).unwrap(), u16::try_from(y).unwrap())
}

fn click(m: &mut UiModel, (x, y): (u16, u16)) -> Vec<UiEffect> {
    let ev = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    on_mouse(m, ev)
}

#[test]
fn click_card_row_focuses_card() {
    let mut m = chat_with(vec![
        stop_card(1, clock(10, 15)),
        add_agent_card(2, clock(10, 21)),
    ]);
    m.focus.pane = Pane::Org;
    let at = find(&m, "Proposed change");
    assert_eq!(click(&mut m, at), []);
    assert_eq!(m.focus.pane, Pane::Chat);
    assert_eq!(m.focus.card, Some(CardId(2)));

    // A card's row in Needs you does as `enter` there.
    m.focus.pane = Pane::Org;
    m.focus.card = None;
    // The first `◆ Duo` row is the older card's.
    let row = find(&m, "◆ Duo");
    assert!(row.0 < 50, "a Needs you row");
    assert_eq!(click(&mut m, row), []);
    assert_eq!(m.focus.pane, Pane::Chat);
    assert_eq!(m.focus.card, Some(CardId(1)));
}

#[test]
fn click_card_button_acts() {
    let mut m = chat_with(vec![add_agent_card(2, clock(10, 21))]);
    let at = find(&m, "[n] Reject");
    assert_eq!(
        click(&mut m, at),
        [UiEffect::Send(Command::RejectCard { card: CardId(2) })]
    );
    let at = find(&m, "[y] Approve");
    assert_eq!(click(&mut m, (at.0 + 2, at.1)), approve(2, None));
    let at = find(&m, "[e] Edit");
    assert_eq!(click(&mut m, at), []);
    assert!(
        matches!(&m.dialog, Some(Dialog::Form(f)) if f.purpose == FormPurpose::Card(CardId(2)))
    );
}
