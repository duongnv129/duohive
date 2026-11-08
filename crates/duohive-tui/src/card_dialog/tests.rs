//! The card dialog's keys, queue and clicks (UI §4.6; CDL-D1 to CDL-D5).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use duohive_core::card::{Card, CardChange, CardId, CardOutcome};
use duohive_core::cli_kind::CliKind;
use duohive_core::command::{AgentDraft, Command, MoveTarget, OrgEdit};
use duohive_core::event::Event;
use duohive_core::org::{Fields, NodeKey};
use duohive_core::testkit::n;
use ratatui::layout::Rect;

use crate::fixtures::{
    add_agent_card, card, clock, ctx, mins, new_agent_card, qa_role, raise, render_lines,
    spec_model, stop_card,
};
use crate::focus::Pane;
use crate::forms::{FieldId, FormPurpose};
use crate::input::mouse::on_mouse;
use crate::input::{UiEffect, map_key, update};
use crate::model::{Dialog, UiModel};

const W: u16 = 120;
const H: u16 = 40;

fn code(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::NONE)
}

fn key(c: char) -> KeyEvent {
    code(KeyCode::Char(c))
}

fn press(m: &mut UiModel, k: KeyEvent) -> Vec<UiEffect> {
    match map_key(m, k) {
        Some(a) => update(m, a),
        None => Vec::new(),
    }
}

/// `cards` pending, oldest first, the org pane focused.
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
fn open_row(m: &mut UiModel, id: u64) -> Vec<UiEffect> {
    m.focus.pane = Pane::NeedsYou;
    let at = m
        .needs_you
        .iter()
        .position(|i| i.card == Some(CardId(id)))
        .unwrap();
    m.focus.needs_you_cursor = at;
    press(m, code(KeyCode::Enter))
}

fn opened(c: Card) -> UiModel {
    let id = c.id.0;
    let mut m = with_cards(vec![c]);
    open_row(&mut m, id);
    m
}

fn shown(m: &UiModel) -> CardDialog {
    match &m.dialog {
        Some(Dialog::Card(d)) => *d,
        other => panic!("no card dialog: {other:?}"),
    }
}

use super::CardDialog;

fn approve(id: u64, edited: Option<CardChange>) -> Vec<UiEffect> {
    vec![UiEffect::Send(Command::ApproveCard {
        card: CardId(id),
        edited: edited.map(Box::new),
    })]
}

fn reject(id: u64) -> Vec<UiEffect> {
    vec![UiEffect::Send(Command::RejectCard { card: CardId(id) })]
}

fn answered(m: &mut UiModel, id: u64) {
    m.apply(Event::CardAnswered {
        card: CardId(id),
        outcome: CardOutcome::Approved,
    });
}

fn text(m: &UiModel) -> String {
    render_lines(m, &ctx(), W, H).join("\n")
}

#[test]
fn enter_on_a_card_row_opens_the_dialog_on_reject_and_changes_nothing_else() {
    let mut m = with_cards(vec![stop_card(1, clock(10, 15))]);
    let selected = m.selected.clone();
    assert_eq!(open_row(&mut m, 1), []);
    let d = shown(&m);
    assert_eq!(d.card, CardId(1));
    assert_eq!(
        super::focusable(&m, d.card)[d.focus],
        crate::keymap::ActionId::CardReject
    );
    assert_eq!(m.selected, selected);
    assert_eq!(m.focus.pane, Pane::NeedsYou);
}

#[test]
fn a_raised_card_never_opens_by_itself() {
    let m = with_cards(vec![stop_card(1, clock(10, 15))]);
    assert_eq!(m.dialog, None);
}

#[test]
fn title_shows_the_place_among_all_pending_cards() {
    let mut m = with_cards(vec![
        new_agent_card(1, clock(10, 15)),
        add_agent_card(2, clock(10, 21)),
    ]);
    open_row(&mut m, 2);
    let t = text(&m);
    assert!(t.contains("◆ Proposed change · 2 of 2"), "{t}");
    let mut m = opened(stop_card(1, clock(10, 15)));
    assert!(text(&m).contains("◆ Stop squad-1"));
    assert!(!text(&m).contains(" of "), "one card has no position");
    m.dialog = None;
}

#[test]
fn y_e_n_and_the_buttons_show_in_the_dialog() {
    let m = opened(add_agent_card(2, clock(10, 21)));
    let t = text(&m);
    assert!(t.contains("[ y Approve ]"), "{t}");
    assert!(t.contains("[ e Edit ]"), "{t}");
    assert!(t.contains("[ n Reject ]"), "{t}");
}

#[test]
fn arrows_move_between_buttons_and_skip_edit_on_a_stop_card() {
    let mut m = opened(add_agent_card(2, clock(10, 21)));
    assert_eq!(shown(&m).focus, 2);
    press(&mut m, code(KeyCode::Left));
    assert_eq!(shown(&m).focus, 1);
    press(&mut m, code(KeyCode::Up));
    assert_eq!(shown(&m).focus, 0);
    press(&mut m, code(KeyCode::Right));
    press(&mut m, code(KeyCode::Down));
    assert_eq!(shown(&m).focus, 2);
    press(&mut m, code(KeyCode::Right));
    assert_eq!(shown(&m).focus, 0, "wraps");

    let mut m = opened(stop_card(1, clock(10, 15)));
    assert_eq!(super::focusable(&m, CardId(1)).len(), 2);
    press(&mut m, code(KeyCode::Left));
    assert_eq!(shown(&m).focus, 0);
    press(&mut m, code(KeyCode::Right));
    assert_eq!(shown(&m).focus, 1, "Edit is skipped");
}

#[test]
fn enter_presses_the_focused_button() {
    let mut m = opened(add_agent_card(2, clock(10, 21)));
    assert_eq!(press(&mut m, code(KeyCode::Enter)), reject(2));
    let mut m = opened(add_agent_card(2, clock(10, 21)));
    press(&mut m, code(KeyCode::Left));
    press(&mut m, code(KeyCode::Left));
    assert_eq!(press(&mut m, code(KeyCode::Enter)), approve(2, None));
}

#[test]
fn y_approves_then_waits_and_the_answer_shows_the_next_card() {
    let mut m = with_cards(vec![
        new_agent_card(1, clock(10, 15)),
        add_agent_card(2, clock(10, 21)),
    ]);
    open_row(&mut m, 1);
    assert_eq!(press(&mut m, key('y')), approve(1, None));
    assert!(shown(&m).sent);
    // A second key while the answer is on the way sends nothing.
    assert_eq!(press(&mut m, key('y')), []);
    assert_eq!(press(&mut m, key('n')), []);
    answered(&mut m, 1);
    assert_eq!(shown(&m).card, CardId(2));
    assert!(!shown(&m).sent);
}

#[test]
fn the_last_answer_closes_the_dialog() {
    let mut m = opened(stop_card(1, clock(10, 15)));
    assert_eq!(press(&mut m, key('n')), reject(1));
    answered(&mut m, 1);
    assert_eq!(m.dialog, None);
}

#[test]
fn a_card_answered_elsewhere_moves_the_dialog_on() {
    let mut m = with_cards(vec![
        new_agent_card(1, clock(10, 15)),
        add_agent_card(2, clock(10, 21)),
    ]);
    open_row(&mut m, 2);
    answered(&mut m, 2);
    assert_eq!(shown(&m).card, CardId(1));
    answered(&mut m, 1);
    assert_eq!(m.dialog, None);
}

#[test]
fn a_refused_answer_can_be_given_again() {
    let mut m = opened(stop_card(1, clock(10, 15)));
    press(&mut m, key('n'));
    m.apply(Event::CommandRejected {
        reason: "no such card".to_owned(),
    });
    assert!(!shown(&m).sent);
    assert_eq!(press(&mut m, key('n')), reject(1));
}

#[test]
fn esc_closes_and_the_card_stays_pending() {
    let mut m = opened(stop_card(1, clock(10, 15)));
    assert_eq!(press(&mut m, code(KeyCode::Esc)), []);
    assert_eq!(m.dialog, None);
    assert!(m.cards.contains_key(&CardId(1)));
    // Even with an answer on the way, esc closes.
    let mut m = opened(stop_card(1, clock(10, 15)));
    press(&mut m, key('n'));
    press(&mut m, code(KeyCode::Esc));
    assert_eq!(m.dialog, None);
}

#[test]
fn e_on_a_card_with_nothing_to_edit_only_says_so() {
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
        let mut m = opened(c);
        assert_eq!(press(&mut m, key('e')), []);
        assert!(matches!(m.dialog, Some(Dialog::Card(_))));
        assert_eq!(m.footer_note.as_deref(), Some("nothing to edit · y or n"));
        assert!(text(&m).contains("[ e Edit ]"), "dimmed, still drawn");
    }
}

#[test]
fn edit_opens_the_form_esc_returns_to_the_card_save_approves() {
    let mut m = opened(add_agent_card(2, clock(10, 21)));
    assert_eq!(press(&mut m, key('e')), []);
    let Some(Dialog::Form(f)) = &m.dialog else {
        panic!("no form: {:?}", m.dialog);
    };
    assert_eq!(f.purpose, FormPurpose::Card(CardId(2)));
    assert_eq!(f.field(FieldId::Name).unwrap().edit.text(), "pay-web");
    press(&mut m, code(KeyCode::Esc));
    assert_eq!(shown(&m).card, CardId(2));
    assert!(!shown(&m).sent);

    press(&mut m, key('e'));
    let CardChange::Org(proposed) = add_agent_card(2, clock(10, 21)).change else {
        unreachable!()
    };
    let out = press(
        &mut m,
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
    );
    assert_eq!(out, approve(2, Some(CardChange::Org(proposed))));
    assert!(shown(&m).sent, "waits for the answer");
    answered(&mut m, 2);
    assert_eq!(m.dialog, None);
}

#[test]
fn e_on_a_new_agent_card_keeps_its_task() {
    let mut m = opened(new_agent_card(2, clock(10, 21)));
    press(&mut m, key('e'));
    let out = press(
        &mut m,
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
    );
    let [
        UiEffect::Send(Command::ApproveCard {
            edited: Some(c), ..
        }),
    ] = out.as_slice()
    else {
        panic!("{out:?}");
    };
    assert!(matches!(**c, CardChange::NewAgent { .. }), "{c:?}");
}

#[test]
fn opens_while_reconnecting_and_with_an_empty_org() {
    let mut m = with_cards(vec![stop_card(1, clock(10, 15))]);
    m.reconnecting = 1;
    open_row(&mut m, 1);
    assert_eq!(shown(&m).card, CardId(1));
}

fn find(m: &UiModel, needle: &str) -> (u16, u16) {
    let lines = render_lines(m, &ctx(), W, H);
    let (y, line) = lines
        .iter()
        .enumerate()
        .find(|(_, l)| l.contains(needle))
        .unwrap_or_else(|| panic!("{needle} not on screen:\n{}", lines.join("\n")));
    let x = line[..line.find(needle).unwrap()].chars().count();
    (u16::try_from(x).unwrap(), u16::try_from(y).unwrap())
}

fn click(m: &mut UiModel, (x, y): (u16, u16)) -> Vec<UiEffect> {
    on_mouse(
        m,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x + 2,
            row: y,
            modifiers: KeyModifiers::NONE,
        },
    )
}

#[test]
fn a_click_on_a_button_presses_it() {
    let mut m = opened(add_agent_card(2, clock(10, 21)));
    let at = find(&m, "[ n Reject ]");
    assert_eq!(click(&mut m, at), reject(2));
    let mut m = opened(add_agent_card(2, clock(10, 21)));
    let at = find(&m, "[ y Approve ]");
    assert_eq!(click(&mut m, at), approve(2, None));
    let mut m = opened(add_agent_card(2, clock(10, 21)));
    let at = find(&m, "[ e Edit ]");
    click(&mut m, at);
    assert!(
        matches!(&m.dialog, Some(Dialog::Form(f)) if f.purpose == FormPurpose::Card(CardId(2)))
    );
}

#[test]
fn an_eight_field_card_at_80x24_keeps_its_title_and_buttons() {
    let fields = Fields {
        cli: Some(CliKind::Claude),
        flags: Some("--add-dir ../shared".to_owned()),
        directory: Some("~/code/web".to_owned()),
        model: Some("sonnet".to_owned()),
        effort: Some("high".to_owned()),
        worktree: Some(true),
        prompt: Some("Own the web client.\nKeep it small.".to_owned()),
    };
    let edit = OrgEdit::AddAgent {
        role: qa_role(),
        agent: AgentDraft {
            name: n("pay-web"),
            fields,
        },
    };
    let mut m = with_cards(vec![card(1, clock(10, 15), CardChange::Org(edit))]);
    m.screen = Rect::new(0, 0, 80, 24);
    open_row(&mut m, 1);
    let t = render_lines(&m, &ctx(), 80, 24).join("\n");
    assert!(t.contains("◆ Proposed change"), "{t}");
    assert!(t.contains("[ y Approve ]"), "{t}");
    assert!(t.contains("[ n Reject ]"), "{t}");
    assert!(
        t.contains("--add-dir ../shared"),
        "flags are never cut first:\n{t}"
    );
}

#[test]
fn a_click_on_a_needs_you_card_row_opens_the_dialog_as_enter_does() {
    let mut m = with_cards(vec![
        new_agent_card(1, clock(10, 15)),
        add_agent_card(2, clock(10, 21)),
    ]);
    let at = find(&m, "pay-web");
    on_mouse(
        &mut m,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: at.0,
            row: at.1,
            modifiers: KeyModifiers::NONE,
        },
    );
    assert!(matches!(m.dialog, Some(Dialog::Card(_))), "{:?}", m.dialog);
}

#[test]
fn an_empty_org_still_opens_duos_card() {
    let mut m = with_cards(vec![add_agent_card(2, clock(10, 21))]);
    m.org.teams.clear();
    m.org.roles.clear();
    open_row(&mut m, 2);
    assert_eq!(shown(&m).card, CardId(2));
    assert!(text(&m).contains("[ y Approve ]"));
}

/// A card's edit form waiting for its answer, as the confirmation path leaves it.
fn saving_form(m: &mut UiModel, id: u64) {
    let card = m.cards[&CardId(id)].clone();
    let mut f = crate::forms::form_for_card(&card, m).unwrap();
    f.saving = true;
    m.dialog = Some(Dialog::Form(Box::new(f)));
}

#[test]
fn a_confirmed_edit_moves_on_to_the_next_card() {
    let mut m = with_cards(vec![
        new_agent_card(1, clock(10, 15)),
        add_agent_card(2, clock(10, 21)),
    ]);
    saving_form(&mut m, 2);
    // The edit lands first and must not close the form that waits for the answer.
    m.apply(Event::OrgChanged {
        org: Box::new(m.org.clone()),
        problems: Default::default(),
    });
    assert!(matches!(m.dialog, Some(Dialog::Form(_))), "{:?}", m.dialog);
    answered(&mut m, 2);
    assert_eq!(shown(&m).card, CardId(1));
}

#[test]
fn a_confirmed_edit_of_the_last_card_closes() {
    let mut m = with_cards(vec![add_agent_card(2, clock(10, 21))]);
    saving_form(&mut m, 2);
    m.apply(Event::OrgChanged {
        org: Box::new(m.org.clone()),
        problems: Default::default(),
    });
    answered(&mut m, 2);
    assert_eq!(m.dialog, None);
}

#[test]
fn a_rebound_card_key_with_a_modifier_still_answers() {
    let mut m = opened(add_agent_card(2, clock(10, 21)));
    let ctrl_y = crate::keymap::parse_binding("ctrl+y").unwrap();
    m.keymap
        .rebind(crate::keymap::ActionId::CardApprove, ctrl_y)
        .unwrap();
    let k = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL);
    assert_eq!(press(&mut m, k), approve(2, None));
    let mut m = opened(add_agent_card(2, clock(10, 21)));
    let alt_n = crate::keymap::parse_binding("alt+n").unwrap();
    m.keymap
        .rebind(crate::keymap::ActionId::CardReject, alt_n)
        .unwrap();
    let k = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::ALT);
    assert_eq!(press(&mut m, k), reject(2));
}
