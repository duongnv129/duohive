//! The mouse on the main screen (P2-D25): where the wheel goes, what it never does, and
//! the chat history's scrolling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use duohive_core::command::Command;
use duohive_core::org::NodeKey;
use duohive_core::status::AgentState;
use duohive_core::testkit::n;
use duohive_core::time::Millis;
use ratatui::layout::Rect;

use super::mouse::{WHEEL_LINES, on_mouse};
use super::{Action, UiEffect};
use crate::chat::{chat_action, chat_key};
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
    let you = duohive_core::chat::ChatKind::You { message: None };
    m.chat.entries = (1..=count)
        .map(|i| {
            let id = u64::try_from(i).unwrap();
            crate::fixtures::chat_entry(id, Millis(0), you.clone(), &format!("m{i:02}x"))
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
    m.apply(duohive_core::event::Event::ChatLine(
        crate::fixtures::chat_entry(41, Millis(0), duohive_core::chat::ChatKind::Duo, "m41x"),
    ));
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

fn press(x: u16, y: u16) -> MouseEvent {
    mouse(MouseEventKind::Down(MouseButton::Left), x, y)
}

/// The screen cell where `text` is drawn inside `r`, found on the rendered frame.
fn cell_of(m: &UiModel, r: Rect, text: &str) -> (u16, u16) {
    let lines = render_lines(m, &ctx(), W, H);
    for y in r.y..r.bottom() {
        let line: String = lines[usize::from(y)]
            .chars()
            .skip(usize::from(r.x))
            .take(usize::from(r.width))
            .collect();
        if let Some(i) = line.find(text) {
            let x = u16::try_from(line[..i].chars().count()).unwrap();
            return (r.x + x, y);
        }
    }
    panic!("{text} not drawn in {r:?}");
}

fn no_pane_input(effects: &[UiEffect]) -> bool {
    effects
        .iter()
        .all(|e| !matches!(e, UiEffect::Pane(_) | UiEffect::Wheel { .. }))
}

#[test]
fn click_pane_focuses() {
    for pane in [Pane::Org, Pane::NeedsYou, Pane::Agent, Pane::Chat] {
        let mut m = model();
        m.focus.pane = if pane == Pane::Org {
            Pane::Chat
        } else {
            Pane::Org
        };
        let a = areas(&m);
        let r = match pane {
            Pane::Org => a.org,
            Pane::NeedsYou => a.needs_you,
            Pane::Agent => a.agent,
            Pane::Chat => a.chat,
        };
        // A cell with no row: Needs you's title row, else the bottom-right inner corner.
        let at = if pane == Pane::NeedsYou {
            press(r.x + 2, r.y)
        } else {
            press(r.right() - 2, r.bottom() - 2)
        };
        on_mouse(&mut m, at);
        assert_eq!(m.focus.pane, pane, "{pane:?}");
    }
}

#[test]
fn click_agent_selects_and_shows_terminal() {
    let mut m = model();
    let (x, y) = cell_of(&m, areas(&m).org, "billing-api");
    let effects = on_mouse(&mut m, press(x, y));
    assert_eq!(m.selected, NodeKey::Agent(n("billing-api")));
    assert_eq!(m.focus.pane, Pane::Org);
    assert!(no_pane_input(&effects));
    // A finished agent is marked seen, as `↓` onto it does.
    let (x, y) = cell_of(&m, areas(&m).org, "squad-2");
    let effects = on_mouse(&mut m, press(x, y));
    assert_eq!(
        effects,
        [UiEffect::Send(Command::ViewAgent {
            agent: n("squad-2")
        })]
    );
}

#[test]
fn click_duo_selects() {
    let mut m = model();
    let (x, y) = cell_of(&m, areas(&m).org, "your manager agent");
    on_mouse(&mut m, press(x, y));
    assert_eq!(m.selected, NodeKey::Duo);
}

#[test]
fn click_team_and_role_select_without_folding() {
    let mut m = model();
    let (x, y) = cell_of(&m, areas(&m).org, "Payments");
    on_mouse(&mut m, press(x, y));
    assert!(matches!(m.selected, NodeKey::Team(_)), "{:?}", m.selected);
    assert!(m.folded.is_empty());
    let (x, y) = cell_of(&m, areas(&m).org, "Backend");
    on_mouse(&mut m, press(x, y));
    assert!(matches!(m.selected, NodeKey::Role(_)), "{:?}", m.selected);
    assert!(m.folded.is_empty());
}

#[test]
fn click_needs_you_row_acts_as_enter() {
    let mut m = model();
    let a = areas(&m);
    let (x, y) = cell_of(&m, a.needs_you, "billing-api");
    let effects = on_mouse(&mut m, press(x, y));
    assert_eq!(m.selected, NodeKey::Agent(n("billing-api")));
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    assert_eq!(
        effects,
        [UiEffect::Send(Command::ViewAgent {
            agent: n("billing-api")
        })]
    );
}

#[test]
fn click_agent_pane_does_not_start_typing() {
    let mut m = model();
    m.focus.pane = Pane::Org;
    let (x, y) = middle(areas(&m).live);
    let effects = on_mouse(&mut m, press(x, y));
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    assert!(no_pane_input(&effects));
}

fn typing() -> UiModel {
    let mut m = model();
    m.focus.pane = Pane::Agent;
    m.focus.agent_mode = AgentMode::Typing;
    m
}

#[test]
fn click_inside_while_typing_keeps_typing() {
    let mut m = typing();
    let (x, y) = middle(areas(&m).live);
    let effects = on_mouse(&mut m, press(x, y));
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
    assert!(no_pane_input(&effects));
}

#[test]
fn click_outside_while_typing_leaves_typing() {
    let mut m = typing();
    let (x, y) = cell_of(&m, areas(&m).org, "billing-api");
    on_mouse(&mut m, press(x, y));
    assert_eq!(m.focus.agent_mode, AgentMode::Selected);
    assert_eq!(m.focus.pane, Pane::Org);
    assert_eq!(m.selected, NodeKey::Agent(n("billing-api")));
}

#[test]
fn click_never_reaches_agent() {
    let mut m = typing();
    let a = areas(&m);
    for r in [a.live, a.agent, a.org, a.needs_you, a.chat] {
        let (x, y) = middle(r);
        assert!(no_pane_input(&on_mouse(&mut m, press(x, y))));
    }
}

#[test]
fn click_keeps_scrolled_view() {
    let mut m = model();
    m.scrolled = true;
    let (x, y) = middle(areas(&m).live);
    on_mouse(&mut m, press(x, y));
    assert!(m.scrolled);
    let (x, y) = middle(areas(&m).chat);
    on_mouse(&mut m, press(x, y));
    assert!(m.scrolled);
}

#[test]
fn click_other_agent_shows_it_live() {
    let mut m = model();
    m.scrolled = true;
    let (x, y) = cell_of(&m, areas(&m).org, "billing-api");
    on_mouse(&mut m, press(x, y));
    // The selection moved: the app loop drops the old stream and its snapshot, and
    // opens billing-api's (live_tests: stream_closes_on_selection_change).
    assert_eq!(m.selected, NodeKey::Agent(n("billing-api")));
}

#[test]
fn click_ignored_while_zoomed() {
    let mut m = model();
    m.focus.pane = Pane::Agent;
    m.zoomed = true;
    let before = m.clone();
    let a = areas(&m);
    for (x, y) in [middle(a.live), (0, 3), (W - 1, H - 2)] {
        assert_eq!(on_mouse(&mut m, press(x, y)), []);
    }
    assert_eq!(m, before);
}

#[test]
fn right_middle_click_and_drag_ignored() {
    let mut m = model();
    m.focus.pane = Pane::Chat;
    let before = m.clone();
    let (x, y) = cell_of(&m, areas(&m).org, "billing-api");
    for kind in [
        MouseEventKind::Down(MouseButton::Right),
        MouseEventKind::Down(MouseButton::Middle),
        MouseEventKind::Drag(MouseButton::Left),
    ] {
        assert_eq!(on_mouse(&mut m, mouse(kind, x, y)), []);
    }
    assert_eq!(m, before);
}

#[test]
fn click_release_ignored() {
    let mut m = model();
    m.focus.pane = Pane::Chat;
    let before = m.clone();
    let (x, y) = cell_of(&m, areas(&m).org, "billing-api");
    assert_eq!(
        on_mouse(&mut m, mouse(MouseEventKind::Up(MouseButton::Left), x, y)),
        []
    );
    assert_eq!(m, before);
}

#[test]
fn click_with_chat_focused_hits_the_row_drawn() {
    // The chat grows while it has focus; the click is on the layout drawn then.
    let mut m = model();
    m.focus.pane = Pane::Chat;
    let a = areas(&m);
    let (x, y) = cell_of(&m, a.needs_you, "billing-api");
    let effects = on_mouse(&mut m, press(x, y));
    assert_eq!(m.focus.pane, Pane::Agent);
    assert_eq!(
        effects,
        [UiEffect::Send(Command::ViewAgent {
            agent: n("billing-api")
        })]
    );
    let mut m = model();
    m.focus.pane = Pane::Chat;
    let (x, y) = cell_of(&m, areas(&m).org, "squad-1");
    on_mouse(&mut m, press(x, y));
    assert_eq!(m.selected, NodeKey::Agent(n("squad-1")));
}

/// P2-D25's scrolling over `ChatEntry` history, with friction entry 13: following the
/// newest, a reply taller than the history shows its start, and scrolling counts from
/// what is shown.
#[test]
fn chat_scroll_kept_from_p2_d25() {
    let mut m = chatty(40);
    let (_, room) = chat_history_fit(&m).unwrap();
    let tall: Vec<String> = (1..=room + 4).map(|i| format!("r{i:02}y")).collect();
    m.apply(duohive_core::event::Event::ChatLine(
        crate::fixtures::chat_entry(
            41,
            Millis(0),
            duohive_core::chat::ChatKind::Duo,
            &tall.join("\n"),
        ),
    ));
    let (len, _) = chat_history_fit(&m).unwrap();
    assert_eq!(len, 40 + room + 4);
    let text = shown(&m);
    assert!(
        text.contains("r01y") && !text.contains(&tall[room]),
        "{text}"
    );
    let (x, y) = middle(areas(&m).chat);
    on_mouse(&mut m, wheel(true, x, y));
    assert_eq!(m.chat.scroll, Some(40 - usize::from(WHEEL_LINES)));
    on_mouse(&mut m, wheel(false, x, y));
    assert_eq!(m.chat.scroll, None);
}
