//! Selecting text with a plain drag (UI §5): where a selection starts, what a drag that
//! belongs to a node or the chat edge still does, how the text is read and drawn.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::Position;
use ratatui::style::Modifier;

use super::mouse::on_mouse;
use super::mouse_tests::{areas, cell_of, model, mouse};
use super::select::{SelPane, TextSel};
use super::{UiEffect, map_key, selected_text, update};
use crate::fixtures::{ctx, render_buffer, render_lines};

fn press(x: u16, y: u16) -> crossterm::event::MouseEvent {
    mouse(MouseEventKind::Down(MouseButton::Left), x, y)
}

fn drag(x: u16, y: u16) -> crossterm::event::MouseEvent {
    mouse(MouseEventKind::Drag(MouseButton::Left), x, y)
}

fn release(x: u16, y: u16) -> crossterm::event::MouseEvent {
    mouse(MouseEventKind::Up(MouseButton::Left), x, y)
}

#[test]
fn press_drag_release_in_the_agent_pane_selects_and_asks_to_copy() {
    let mut m = model();
    let a = areas(&m).agent;
    let (x, y) = (a.x + 3, a.y + 2);
    on_mouse(&mut m, press(x, y));
    assert_eq!(m.selection.map(|s| s.pane), Some(SelPane::Agent));
    on_mouse(&mut m, drag(x + 6, y + 1));
    let effects = on_mouse(&mut m, release(x + 6, y + 1));
    assert_eq!(effects, [UiEffect::CopySelection]);
    let sel = m.selection.unwrap();
    assert_eq!(sel.anchor, Position::new(x, y));
    assert_eq!(sel.head, Position::new(x + 6, y + 1));
}

#[test]
fn a_plain_click_selects_nothing() {
    let mut m = model();
    let a = areas(&m).agent;
    let (x, y) = (a.x + 3, a.y + 2);
    on_mouse(&mut m, press(x, y));
    let effects = on_mouse(&mut m, release(x, y));
    assert_eq!(effects, []);
    assert_eq!(m.selection, None);
}

#[test]
fn a_press_on_the_border_starts_no_selection() {
    let mut m = model();
    let a = areas(&m).agent;
    on_mouse(&mut m, press(a.x, a.y + 3));
    assert_eq!(m.selection, None);
}

#[test]
fn dragging_an_org_row_moves_it_and_does_not_select() {
    let mut m = model();
    let (x, y) = cell_of(&m, areas(&m).org, "pay-qa");
    on_mouse(&mut m, press(x, y));
    assert!(m.drag.is_some(), "the node is being carried");
    assert_eq!(m.selection, None);
    on_mouse(&mut m, drag(x + 2, y));
    assert_eq!(m.selection, None);
}

#[test]
fn the_chat_edge_still_resizes() {
    let mut m = model();
    let chat = areas(&m).chat;
    on_mouse(&mut m, press(chat.x + 5, chat.y));
    assert!(m.edge_drag.is_some());
    assert_eq!(m.selection, None);
}

#[test]
fn the_selection_stays_inside_its_pane() {
    let mut m = model();
    let pane = areas(&m);
    let (x, y) = (pane.agent.x + 3, pane.agent.y + 2);
    on_mouse(&mut m, press(x, y));
    // Out over the org pane on the left and far below the screen's panes.
    on_mouse(&mut m, drag(pane.org.x + 1, pane.agent.bottom() + 5));
    let head = m.selection.unwrap().head;
    assert!(head.x > pane.agent.x, "{head:?}");
    assert!(head.y < pane.agent.bottom() - 1, "{head:?}");
}

#[test]
fn a_new_press_or_a_key_clears_the_selection() {
    let mut m = model();
    let a = areas(&m).agent;
    on_mouse(&mut m, press(a.x + 3, a.y + 2));
    on_mouse(&mut m, drag(a.x + 8, a.y + 2));
    on_mouse(&mut m, release(a.x + 8, a.y + 2));
    assert!(m.selection.is_some());
    on_mouse(&mut m, press(a.x + 10, a.y + 4));
    assert_ne!(
        m.selection.map(|s| s.anchor),
        Some(Position::new(a.x + 3, a.y + 2))
    );

    on_mouse(&mut m, drag(a.x + 14, a.y + 4));
    let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
    if let Some(action) = map_key(&m, down) {
        update(&mut m, action);
    }
    assert_eq!(m.selection, None);
}

#[test]
fn a_zoomed_agent_pane_can_be_selected() {
    let mut m = model();
    let z = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE);
    if let Some(action) = map_key(&m, z) {
        update(&mut m, action);
    }
    let a = areas(&m);
    assert!(a.zoomed);
    on_mouse(&mut m, press(a.agent.x + 3, a.agent.y + 2));
    on_mouse(&mut m, drag(a.agent.x + 9, a.agent.y + 3));
    let effects = on_mouse(&mut m, release(a.agent.x + 9, a.agent.y + 3));
    assert_eq!(effects, [UiEffect::CopySelection]);
}

#[test]
fn the_text_is_what_the_pane_drew() {
    let mut m = model();
    let a = areas(&m).agent;
    let (x0, y0) = (a.x + 2, a.y + 1);
    m.selection = Some(TextSel {
        pane: SelPane::Agent,
        anchor: Position::new(x0, y0),
        head: Position::new(x0 + 9, y0 + 2),
    });
    let buf = render_buffer(&m, &ctx(), m.screen.width, m.screen.height);
    let lines = render_lines(&m, &ctx(), m.screen.width, m.screen.height);
    let cut = |y: u16, from: u16, to: u16| -> String {
        let row: Vec<char> = lines[usize::from(y)].chars().collect();
        row[usize::from(from)..=usize::from(to)]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_owned()
    };
    let inner_left = a.x + 1;
    let inner_right = a.right() - 2;
    let expected = [
        cut(y0, x0, inner_right),
        cut(y0 + 1, inner_left, inner_right),
        cut(y0 + 2, inner_left, x0 + 9),
    ]
    .join("\n");
    assert_eq!(selected_text(&buf, &m).trim_end(), expected.trim_end());
}

#[test]
fn a_backwards_selection_reads_the_same_text() {
    let mut m = model();
    let a = areas(&m).agent;
    let (p, q) = (
        Position::new(a.x + 2, a.y + 1),
        Position::new(a.x + 12, a.y + 2),
    );
    m.selection = Some(TextSel {
        pane: SelPane::Agent,
        anchor: p,
        head: q,
    });
    let buf = render_buffer(&m, &ctx(), m.screen.width, m.screen.height);
    let forward = selected_text(&buf, &m);
    m.selection = Some(TextSel {
        pane: SelPane::Agent,
        anchor: q,
        head: p,
    });
    assert_eq!(selected_text(&buf, &m), forward);
}

#[test]
fn the_selection_is_drawn_reversed() {
    let mut m = model();
    let a = areas(&m).agent;
    let (x, y) = (a.x + 3, a.y + 2);
    m.selection = Some(TextSel {
        pane: SelPane::Agent,
        anchor: Position::new(x, y),
        head: Position::new(x + 4, y),
    });
    let buf = render_buffer(&m, &ctx(), m.screen.width, m.screen.height);
    assert!(buf[(x, y)].modifier.contains(Modifier::REVERSED));
    assert!(buf[(x + 4, y)].modifier.contains(Modifier::REVERSED));
    assert!(!buf[(x + 5, y)].modifier.contains(Modifier::REVERSED));
}
