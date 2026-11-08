//! The docked approval card's keys and clicks (UI §4.6, §4.1.3, §4.11; P3-D7, P3-D22,
//! CD-D1 to CD-D8).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use duohive_core::card::{Card, CardChange, CardId, CardOutcome};
use duohive_core::command::{Command, MoveTarget, OrgEdit};
use duohive_core::event::Event;
use duohive_core::org::NodeKey;
use duohive_core::testkit::n;
use ratatui::layout::Rect;

use super::mouse::on_mouse;
use super::{UiEffect, map_key, update};
use crate::fixtures::{
    add_agent_card, card, clock, ctx, mins, new_agent_card, qa_role, raise, render_lines,
    spec_model, stop_card,
};
use crate::focus::{AgentMode, Pane};
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

/// `cards` pending (oldest first), raised 2m and 1m ago; the org pane has focus.
fn with_cards(cards: Vec<Card>) -> UiModel {
    let mut m = spec_model();
    m.screen = Rect::new(0, 0, W, H);
    let count = u64::try_from(cards.len()).unwrap();
    for (i, c) in (0..).zip(cards) {
        raise(&mut m, c, mins(count - i));
    }
    m.focus.pane = Pane::Org;
    m
}

/// `enter` on card `id`'s row in Needs you.
fn enter_row(m: &mut UiModel, id: u64) -> Vec<UiEffect> {
    m.focus.pane = Pane::NeedsYou;
    let at = m
        .needs_you
        .iter()
        .position(|i| i.card == Some(CardId(id)))
        .unwrap();
    m.focus.needs_you_cursor = at;
    press(m, code(KeyCode::Enter))
}

fn focused(c: Card) -> UiModel {
    let id = c.id;
    let mut m = with_cards(vec![c]);
    enter_row(&mut m, id.0);
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

fn answered(id: u64) -> Event {
    Event::CardAnswered {
        card: CardId(id),
        outcome: CardOutcome::Approved,
    }
}

#[test]
fn enter_on_a_card_row_selects_its_agent_and_focuses_the_oldest_card() {
    // Both dock on Duo: id 1 is the older.
    let mut m = with_cards(vec![
        new_agent_card(1, clock(10, 15)),
        add_agent_card(2, clock(10, 21)),
    ]);
    let out = enter_row(&mut m, 2);
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    assert_eq!(m.selected, NodeKey::Duo);
    assert_eq!(m.focus.card, Some(CardId(1)));
    assert_eq!(
        out,
        [UiEffect::Send(Command::ViewAgent {
            agent: m.org.duo.name.clone()
        })]
    );
    // The oldest card takes the keys.
    assert_eq!(
        press(&mut m, key('n')),
        [UiEffect::Send(Command::RejectCard { card: CardId(1) })]
    );
}

#[test]
fn a_stop_card_row_selects_the_agent_it_stops() {
    let mut m = with_cards(vec![stop_card(1, clock(10, 15))]);
    enter_row(&mut m, 1);
    assert_eq!(m.selected, NodeKey::Agent(n("squad-1")));
    assert_eq!(m.focus.card, Some(CardId(1)));
}

#[test]
fn yen_act_only_while_the_card_has_focus_in_selected_mode() {
    let mut m = with_cards(vec![add_agent_card(2, clock(10, 21))]);
    m.selected = NodeKey::Duo;
    m.focus.pane = Pane::Agent;
    for c in ['y', 'n'] {
        let out = press(&mut m, key(c));
        assert!(
            !out.iter().any(|e| matches!(
                e,
                UiEffect::Send(Command::ApproveCard { .. } | Command::RejectCard { .. })
            )),
            "{out:?}"
        );
    }
    assert_eq!(m.dialog, None);

    let mut m = focused(add_agent_card(2, clock(10, 21)));
    assert_eq!(press(&mut m, key('y')), approve(2, None));

    let mut m = focused(add_agent_card(2, clock(10, 21)));
    assert_eq!(
        press(&mut m, key('n')),
        [UiEffect::Send(Command::RejectCard { card: CardId(2) })]
    );
}

#[test]
fn keys_go_to_the_agent_while_typing_with_a_docked_card() {
    let mut m = with_cards(vec![add_agent_card(2, clock(10, 21))]);
    m.selected = NodeKey::Duo;
    m.focus.pane = Pane::Agent;
    m.focus.agent_mode = AgentMode::Typing;
    let out = press(&mut m, key('y'));
    assert!(
        !out.iter().any(|e| matches!(
            e,
            UiEffect::Send(Command::ApproveCard { .. } | Command::RejectCard { .. })
        )),
        "{out:?}"
    );
    assert_eq!(m.focus.card, None);
    assert_eq!(m.cards.len(), 1);
}

#[test]
fn esc_clears_card_focus_moves_to_org_and_keeps_the_card_docked() {
    let mut m = focused(add_agent_card(2, clock(10, 21)));
    assert_eq!(press(&mut m, code(KeyCode::Esc)), []);
    assert_eq!(m.focus.pane, Pane::Org);
    assert_eq!(m.focus.card, None);
    assert!(m.cards.contains_key(&CardId(2)));
}

#[test]
fn tab_leaves_card_focus() {
    let mut m = focused(add_agent_card(2, clock(10, 21)));
    press(&mut m, code(KeyCode::Tab));
    assert_eq!(m.focus.card, None);
    assert_eq!(m.focus.pane, Pane::Chat);
}

#[test]
fn answering_the_focused_card_focuses_the_next_and_one_key_answers_one_card() {
    let mut m = with_cards(vec![
        new_agent_card(1, clock(10, 15)),
        add_agent_card(2, clock(10, 21)),
    ]);
    enter_row(&mut m, 1);
    assert_eq!(press(&mut m, key('y')), approve(1, None));
    m.apply(answered(1));
    assert_eq!(m.focus.card, Some(CardId(2)));
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(press(&mut m, key('y')), approve(2, None));
    m.apply(answered(2));
    assert_eq!(m.focus.card, None);
}

#[test]
fn card_focus_is_dropped_when_the_card_is_answered_with_none_left() {
    let mut m = focused(add_agent_card(2, clock(10, 21)));
    m.apply(answered(2));
    assert_eq!(m.focus.card, None);
}

#[test]
fn no_card_keys_with_a_team_selected_or_while_reconnecting() {
    let mut m = focused(add_agent_card(2, clock(10, 21)));
    m.selected = NodeKey::Team(duohive_core::names::NodeName::parse("Payments").unwrap());
    let out = press(&mut m, key('y'));
    assert!(!out.contains(&approve(2, None)[0]), "{out:?}");

    let mut m = focused(add_agent_card(2, clock(10, 21)));
    m.reconnecting = 1;
    let out = press(&mut m, key('y'));
    assert!(!out.contains(&approve(2, None)[0]), "{out:?}");
}

#[test]
fn up_in_an_empty_chat_input_no_longer_focuses_a_card() {
    let mut m = with_cards(vec![add_agent_card(2, clock(10, 21))]);
    m.focus.pane = Pane::Chat;
    assert_eq!(press(&mut m, code(KeyCode::Up)), []);
    assert_eq!(m.focus.card, None);
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
    assert_eq!(m.focus.pane, Pane::Agent);
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
        assert!(footer(&m).starts_with(" y approve · n reject · esc back"));
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
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(m.focus.card, Some(CardId(2)));
}

#[test]
fn footer_names_the_card_keys_when_a_card_has_focus() {
    let m = focused(add_agent_card(2, clock(10, 21)));
    assert!(footer(&m).starts_with(" y approve · e edit · n reject · esc back"));
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
fn click_on_a_needs_you_card_row_does_as_enter() {
    let mut m = with_cards(vec![
        new_agent_card(1, clock(10, 15)),
        add_agent_card(2, clock(10, 21)),
    ]);
    // The first `◆ Duo` row is the older card's.
    let row = find(&m, "◆ Duo");
    assert!(row.0 < 50, "a Needs you row");
    assert_eq!(
        click(&mut m, row),
        [UiEffect::Send(Command::ViewAgent {
            agent: m.org.duo.name.clone()
        })]
    );
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(m.focus.card, Some(CardId(1)));
}

#[test]
fn zoom_keeps_card_focus_and_card_keys_work_zoomed() {
    let mut m = focused(add_agent_card(2, clock(10, 21)));
    m.zoomed = true;
    assert_eq!(press(&mut m, key('y')), approve(2, None));
    assert_eq!(m.focus.card, Some(CardId(2)));
}

fn up(m: &mut UiModel, (x, y): (u16, u16)) -> Vec<UiEffect> {
    let ev = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    on_mouse(m, ev)
}

/// squad-1 selected, a stop card for it docked, the org pane focused.
fn docked_stop() -> UiModel {
    let mut m = with_cards(vec![stop_card(1, clock(10, 21))]);
    m.selected = NodeKey::Agent(n("squad-1"));
    m.live_agent = Some(n("squad-1"));
    m
}

#[test]
fn click_on_the_docked_card_focuses_it_and_does_not_start_typing() {
    let mut m = docked_stop();
    let at = find(&m, "squad-1 is");
    assert_eq!(click(&mut m, at), []);
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    assert_eq!(m.focus.card, Some(CardId(1)));
    assert_eq!(up(&mut m, at), []);
    assert!(!m.typing());
    assert_eq!(m.typing_wanted, None);
}

#[test]
fn click_on_each_docked_button_acts() {
    let mut m = with_cards(vec![add_agent_card(2, clock(10, 21))]);
    m.selected = NodeKey::Duo;
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

#[test]
fn click_on_the_card_while_typing_leaves_typing_then_acts() {
    let mut m = docked_stop();
    m.focus.pane = Pane::Agent;
    m.focus.agent_mode = AgentMode::Typing;
    let at = find(&m, "[n] Reject");
    assert_eq!(
        click(&mut m, at),
        [UiEffect::Send(Command::RejectCard { card: CardId(1) })]
    );
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
}

#[test]
fn click_in_the_agent_pane_below_the_card_still_starts_typing() {
    let mut m = docked_stop();
    m.focus.pane = Pane::Org;
    let a = crate::render::pane_areas(&m).unwrap();
    let at = (a.live.x + 2, a.live.bottom() - 2);
    click(&mut m, at);
    up(&mut m, at);
    assert_eq!(m.focus.card, None);
    assert!(m.typing() || m.typing_wanted.is_some());
}

#[test]
fn click_in_the_chat_never_focuses_a_card() {
    let mut m = docked_stop();
    let a = crate::render::pane_areas(&m).unwrap();
    click(&mut m, (a.chat.x + 3, a.chat.y + 1));
    assert_eq!(m.focus.pane, Pane::Chat);
    assert_eq!(m.focus.card, None);
}

#[test]
fn z_keeps_card_focus_and_esc_restores_the_zoom_first() {
    let mut m = focused(stop_card(1, clock(10, 21)));
    press(&mut m, key('z'));
    assert!(m.zoomed, "z zooms");
    assert_eq!(m.focus.card, Some(CardId(1)));
    // A zoomed, focused card answers by key.
    assert_eq!(
        press(&mut m, key('n')),
        [UiEffect::Send(Command::RejectCard { card: CardId(1) })]
    );
    assert_eq!(press(&mut m, code(KeyCode::Esc)), []);
    assert!(!m.zoomed, "esc restores the zoom first");
    assert_eq!(m.focus.card, Some(CardId(1)));
}

#[test]
fn a_card_that_is_not_the_one_drawn_never_takes_the_keys() {
    // Card 1 (stop squad-1) has focus; the selection moves to Duo, whose own card 2 is
    // the one drawn now.
    let mut m = with_cards(vec![
        stop_card(1, clock(10, 15)),
        add_agent_card(2, clock(10, 21)),
    ]);
    enter_row(&mut m, 1);
    assert_eq!(m.focus.card, Some(CardId(1)));
    m.selected = NodeKey::Duo;
    let out = press(&mut m, key('y'));
    assert!(
        !out.iter().any(|e| matches!(
            e,
            UiEffect::Send(Command::ApproveCard { .. } | Command::RejectCard { .. })
        )),
        "{out:?}"
    );
    assert!(!footer(&m).contains("approve"), "{}", footer(&m));
}

#[test]
fn a_card_raised_with_an_empty_org_is_reachable_on_duo() {
    let mut m = with_cards(vec![add_agent_card(2, clock(10, 21))]);
    m.org.teams.clear();
    m.org.roles.clear();
    m.selected = NodeKey::Duo;
    enter_row(&mut m, 2);
    assert_eq!(m.focus.card, Some(CardId(2)));
    assert!(
        render_lines(&m, &ctx(), W, H)
            .join("\n")
            .contains("┏━ ◆ Proposed change")
    );
    assert_eq!(press(&mut m, key('y')), approve(2, None));
}

#[test]
fn a_card_hidden_by_a_small_pane_takes_no_keys() {
    let mut m = focused(add_agent_card(2, clock(10, 21)));
    // `ctrl+↑` grows the chat until the agent pane is too small to draw the card.
    m.chat_rows_extra = 100;
    assert!(
        !render_lines(&m, &ctx(), W, H)
            .join("\n")
            .contains("◆ Proposed change")
    );
    let out = press(&mut m, key('y'));
    assert!(
        !out.iter().any(|e| matches!(
            e,
            UiEffect::Send(Command::ApproveCard { .. } | Command::RejectCard { .. })
        )),
        "{out:?}"
    );
    assert!(!footer(&m).contains("approve"), "{}", footer(&m));
}

#[test]
fn rejecting_the_first_of_two_cards_in_an_empty_org_focuses_the_next() {
    let mut m = with_cards(vec![
        new_agent_card(1, clock(10, 15)),
        add_agent_card(2, clock(10, 21)),
    ]);
    m.org.teams.clear();
    m.org.roles.clear();
    m.selected = NodeKey::Duo;
    enter_row(&mut m, 1);
    assert_eq!(m.focus.card, Some(CardId(1)));
    m.apply(answered(1));
    assert_eq!(m.focus.card, Some(CardId(2)));
}

#[test]
fn click_on_the_docked_card_of_a_finished_agent_marks_it_seen() {
    let draft = duohive_core::command::AgentDraft {
        name: n("squad-2"),
        fields: duohive_core::org::Fields::default(),
    };
    let edit = card(
        1,
        clock(10, 21),
        CardChange::Org(OrgEdit::EditAgent {
            key: n("squad-2"),
            draft,
        }),
    );
    let mut m = with_cards(vec![edit]);
    m.selected = NodeKey::Agent(n("squad-2"));
    let at = find(&m, "~ Edit agent squad-2");
    let out = click(&mut m, at);
    assert!(
        out.contains(&UiEffect::Send(Command::ViewAgent {
            agent: n("squad-2")
        })),
        "{out:?}"
    );
}
