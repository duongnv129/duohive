//! The mouse on the main screen (P2-D25): where the wheel goes, what it never does, and
//! the chat history's scrolling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use duohive_core::status::AgentState;
use duohive_core::time::Millis;
use ratatui::layout::Rect;

use super::mouse::{WHEEL_LINES, on_mouse};
use super::{Action, UiEffect};
use crate::chat::{ChatLine, Who, chat_action, chat_key};
use crate::fixtures::{ctx, mins, render_lines, spec_model, status, view};
use crate::focus::{AgentMode, Pane};
use crate::model::UiModel;
use crate::render::{PaneAreas, chat_history_fit, pane_areas};

const W: u16 = 120;
const H: u16 = 36;

fn model() -> UiModel {
    let mut m = spec_model();
    m.screen = Rect::new(0, 0, W, H);
    m
}

fn areas(m: &UiModel) -> PaneAreas {
    pane_areas(m).unwrap()
}

fn mouse(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

fn wheel(up: bool, x: u16, y: u16) -> MouseEvent {
    let kind = if up {
        MouseEventKind::ScrollUp
    } else {
        MouseEventKind::ScrollDown
    };
    mouse(kind, x, y)
}

fn middle(r: Rect) -> (u16, u16) {
    (r.x + r.width / 2, r.y + r.height / 2)
}

#[test]
fn wheel_over_agent_pane_asks_live_for_a_tick_at_the_inner_cell() {
    let mut m = model();
    let live = areas(&m).live;
    let effects = on_mouse(&mut m, wheel(true, live.x + 4, live.y + 2));
    assert_eq!(
        effects,
        [UiEffect::Wheel {
            up: true,
            col: 5,
            row: 3
        }]
    );
}

#[test]
fn wheel_cell_is_inner_pane_relative_and_border_clamped() {
    let mut m = model();
    let a = areas(&m);
    // The pane's left border, on the live area's second row.
    let effects = on_mouse(&mut m, wheel(false, a.agent.x, a.live.y + 1));
    assert_eq!(
        effects,
        [UiEffect::Wheel {
            up: false,
            col: 1,
            row: 2
        }]
    );
    // The bottom-right border corner takes the last inner cell.
    let effects = on_mouse(
        &mut m,
        wheel(false, a.agent.right() - 1, a.agent.bottom() - 1),
    );
    assert_eq!(
        effects,
        [UiEffect::Wheel {
            up: false,
            col: a.live.width,
            row: a.live.height
        }]
    );
}

#[test]
fn horizontal_wheel_ignored() {
    let mut m = model();
    let (x, y) = middle(areas(&m).live);
    for kind in [MouseEventKind::ScrollLeft, MouseEventKind::ScrollRight] {
        assert_eq!(on_mouse(&mut m, mouse(kind, x, y)), []);
    }
}

#[test]
fn wheel_does_not_move_focus() {
    let mut m = model();
    m.focus.pane = Pane::Org;
    let (x, y) = middle(areas(&m).live);
    on_mouse(&mut m, wheel(true, x, y));
    assert_eq!(m.focus.pane, Pane::Org);
    m.focus.pane = Pane::Agent;
    m.focus.agent_mode = AgentMode::Typing;
    let (x, y) = middle(areas(&m).chat);
    on_mouse(&mut m, wheel(true, x, y));
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
}

#[test]
fn wheel_ignored_over_org_and_needs_you() {
    let mut m = model();
    let before = m.clone();
    let a = areas(&m);
    for r in [a.org, a.needs_you] {
        let (x, y) = middle(r);
        assert_eq!(on_mouse(&mut m, wheel(true, x, y)), []);
    }
    assert_eq!(m, before);
}

#[test]
fn mouse_ignored_in_dialogs_confirmations_settings_worktrees_and_too_small() {
    let base = model();
    let (x, y) = middle(areas(&base).live);
    let click = mouse(
        MouseEventKind::Down(crossterm::event::MouseButton::Left),
        x,
        y,
    );
    let opened: [fn(&mut UiModel); 4] = [
        |m| drop(super::settings::open(m)),
        |m| drop(super::worktrees::open(m)),
        |m| drop(super::give_work::open(m)),
        |m| m.dialog = Some(super::confirm("Stop?", Vec::new(), "Stop", stop_duo(m))),
    ];
    for open in opened {
        let mut m = base.clone();
        open(&mut m);
        assert!(m.dialog.is_some());
        let before = m.clone();
        assert_eq!(on_mouse(&mut m, wheel(true, x, y)), []);
        assert_eq!(on_mouse(&mut m, click), []);
        assert_eq!(m, before);
    }
    let mut small = model();
    small.screen = Rect::new(0, 0, 40, 10);
    assert_eq!(on_mouse(&mut small, wheel(true, 5, 5)), []);
}

fn stop_duo(m: &UiModel) -> duohive_core::command::Command {
    duohive_core::command::Command::StopAgent {
        agent: m.org.duo.name.clone(),
    }
}

/// A model whose chat holds `count` one-row lines `m01x`, `m02x`, …, Duo at rest.
fn chatty(count: usize) -> UiModel {
    let mut m = model();
    let duo = m.org.duo.name.clone();
    m.agents
        .insert(duo, view(status(AgentState::Idle, mins(1))));
    m.chat.lines = (1..=count)
        .map(|i| ChatLine {
            at: Millis(0),
            who: Who::You,
            text: format!("m{i:02}x"),
        })
        .collect();
    m
}

fn shown(m: &UiModel) -> String {
    render_lines(m, &ctx(), W, H).join("\n")
}

#[test]
fn wheel_scrolls_chat_history() {
    let mut m = chatty(40);
    let (len, room) = chat_history_fit(&m).unwrap();
    assert_eq!(len, 40);
    let (x, y) = middle(areas(&m).chat);
    assert_eq!(on_mouse(&mut m, wheel(true, x, y)), []);
    let lines = usize::from(WHEEL_LINES);
    assert_eq!(m.chat.scroll, Some(len - room - lines));
    let text = shown(&m);
    assert!(text.contains("m37x"), "{text}");
    assert!(!text.contains("m38x"), "{text}");
}

#[test]
fn chat_scrolled_title() {
    let mut m = chatty(40);
    let (x, y) = middle(areas(&m).chat);
    on_mouse(&mut m, wheel(true, x, y));
    let duo = m.org.duo.name.to_string();
    let text = shown(&m);
    let title = text
        .lines()
        .find(|l| l.contains(&format!("⬢ {duo} · ◦ idle")))
        .unwrap_or_default();
    assert!(title.contains("· scrolled"), "{title}");
}

#[test]
fn chat_scrolled_stays_put_on_new_line() {
    let mut m = chatty(40);
    let (x, y) = middle(areas(&m).chat);
    on_mouse(&mut m, wheel(true, x, y));
    m.chat.lines.push(ChatLine {
        at: Millis(0),
        who: Who::Duo,
        text: "m41x".to_owned(),
    });
    let text = shown(&m);
    assert!(text.contains("m37x") && !text.contains("m38x"), "{text}");
}

#[test]
fn chat_wheel_past_bottom_follows_newest() {
    let mut m = chatty(40);
    let (x, y) = middle(areas(&m).chat);
    on_mouse(&mut m, wheel(true, x, y));
    on_mouse(&mut m, wheel(false, x, y));
    assert_eq!(m.chat.scroll, None);
    assert!(shown(&m).contains("m40x"));
}

#[test]
fn chat_send_returns_to_newest() {
    let mut m = chatty(40);
    let (x, y) = middle(areas(&m).chat);
    on_mouse(&mut m, wheel(true, x, y));
    m.focus.pane = Pane::Chat;
    chat_action(&mut m, Action::Char('h'));
    let effects = chat_action(&mut m, Action::Enter);
    assert!(
        matches!(effects.as_slice(), [UiEffect::Send(_)]),
        "{effects:?}"
    );
    assert_eq!(m.chat.scroll, None);
}

#[test]
fn chat_pgup_pgdn_page_with_focus() {
    let mut m = chatty(40);
    m.focus.pane = Pane::Chat;
    let (len, room) = chat_history_fit(&m).unwrap();
    let page = room - 1;
    let pgup = chat_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)).unwrap();
    let pgdn = chat_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)).unwrap();
    chat_action(&mut m, pgup);
    assert_eq!(m.chat.scroll, Some(len - room - page));
    chat_action(&mut m, pgdn);
    assert_eq!(m.chat.scroll, None);
}
