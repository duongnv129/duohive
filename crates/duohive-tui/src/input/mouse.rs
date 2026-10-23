//! The mouse on the main screen (P2-D25): the wheel goes to the pane under the pointer,
//! without moving focus or changing mode; a left press does what a key does, and never
//! reaches the agent.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use duohive_core::command::Command;
use duohive_core::org::NodeKey;
use duohive_core::status::AgentState;
use ratatui::layout::{Position, Rect};

use super::{UiEffect, agent, agent_of, cards, drag, menu, panes, select};
use crate::chat;
use crate::filter::Filter;
use crate::focus::{AgentMode, Pane};
use crate::model::UiModel;
use crate::render::{
    PaneAreas, chat_card_at, chat_extra_limit, header_count_at, needs_you_item_at, needs_you_wheel,
    org_key_at, org_wheel, pane_areas,
};

/// Lines one wheel tick scrolls DuoHive's own views (the snapshot, the chat history).
pub(crate) const WHEEL_LINES: u16 = 3;

/// What a mouse event does. Nothing outside the screen or on one too small for the panes;
/// in a dialog or the first-run wizard only a left press, which does what its key does.
pub(crate) fn on_mouse(m: &mut UiModel, ev: MouseEvent) -> Vec<UiEffect> {
    let at = Position::new(ev.column, ev.row);
    if !m.screen.contains(at) {
        return Vec::new();
    }
    if m.menu.is_some() {
        return on_menu(m, ev, at);
    }
    if m.dialog.is_some() || m.wizard.is_some() {
        return on_dialog(m, ev, at);
    }
    let Some(areas) = pane_areas(m) else {
        return Vec::new();
    };
    if matches!(ev.kind, MouseEventKind::Drag(MouseButton::Left))
        && m.selection.is_some()
        && m.drag.is_none()
        && m.edge_drag.is_none()
    {
        select::extend(m, at);
        return Vec::new();
    }
    match ev.kind {
        MouseEventKind::ScrollUp => on_wheel(m, &areas, at, true),
        MouseEventKind::ScrollDown => on_wheel(m, &areas, at, false),
        MouseEventKind::Down(MouseButton::Left) if !areas.zoomed => {
            m.selection = None;
            on_press(m, &areas, at)
        }
        // Zoomed, a press does nothing else; text in the agent pane can still be selected.
        MouseEventKind::Down(MouseButton::Left) => {
            m.selection = None;
            select::start(m, &areas, at);
            Vec::new()
        }
        MouseEventKind::Down(MouseButton::Right) if !areas.zoomed && !m.typing() => {
            on_right_click(m, &areas, at)
        }
        MouseEventKind::Drag(MouseButton::Left) if !areas.zoomed && !m.typing() => {
            on_drag(m, &areas, at);
            Vec::new()
        }
        MouseEventKind::Up(MouseButton::Left) => {
            m.edge_drag = None;
            let dropped = drag::drop_it(m);
            if dropped.is_empty() && m.selection.is_some() {
                select::finish(m)
            } else {
                dropped
            }
        }
        _ => Vec::new(),
    }
}

/// A press with a menu open: a row runs, anywhere else closes it.
fn on_menu(m: &mut UiModel, ev: MouseEvent, at: Position) -> Vec<UiEffect> {
    let pressed = matches!(
        ev.kind,
        MouseEventKind::Down(MouseButton::Left | MouseButton::Right)
    );
    if !pressed {
        return Vec::new();
    }
    let hit = m.menu.as_ref().and_then(|menu| {
        let r = crate::render::menu::rect(m.screen, menu);
        let inside = r.contains(at) && at.y > r.y && at.y + 1 < r.bottom();
        inside.then(|| usize::from(at.y - r.y - 1))
    });
    match hit {
        Some(row) => menu::choose(m, row),
        None => {
            m.menu = None;
            Vec::new()
        }
    }
}

/// A left press in a dialog or the wizard: the keys the clicked button or row stands for.
fn on_dialog(m: &mut UiModel, ev: MouseEvent, at: Position) -> Vec<UiEffect> {
    if ev.kind != MouseEventKind::Down(MouseButton::Left) {
        return Vec::new();
    }
    let Some(keys) = crate::render::keys_at(m, at) else {
        return Vec::new();
    };
    keys.into_iter().flat_map(|k| super::update(m, k)).collect()
}

/// A left press on the main screen: a header count sets the filter, the chat's top edge
/// starts a resize, a row selects and starts a drag.
fn on_press(m: &mut UiModel, areas: &PaneAreas, at: Position) -> Vec<UiEffect> {
    if let Some(f) = header_count_at(m, at.x, at.y) {
        crate::filter::set(m, if m.filter == f { Filter::Off } else { f });
        return Vec::new();
    }
    if areas.chat.contains(at) && at.y == areas.chat.y {
        m.edge_drag = Some((at.y, m.chat_rows_extra));
    }
    let key = areas
        .org
        .contains(at)
        .then(|| org_key_at(m, at.y))
        .flatten();
    let out = on_click(m, areas, at);
    if let Some(key) = key {
        drag::start(m, key);
    } else if m.edge_drag.is_none() {
        select::start(m, areas, at);
    }
    out
}

/// A right press on a row: select it and open its menu at the pointer.
fn on_right_click(m: &mut UiModel, areas: &PaneAreas, at: Position) -> Vec<UiEffect> {
    if !areas.org.contains(at) {
        return Vec::new();
    }
    let Some(key) = org_key_at(m, at.y) else {
        return Vec::new();
    };
    let out = select(m, key.clone());
    m.menu = Some(menu::build(m, key, at));
    out
}

/// The pointer moves with the left button held: resize the chat or carry a node.
fn on_drag(m: &mut UiModel, areas: &PaneAreas, at: Position) {
    if let Some((from, extra)) = m.edge_drag {
        let moved = i16::try_from(from).unwrap_or(0) - i16::try_from(at.y).unwrap_or(0);
        let limit = chat_extra_limit(m).max(0);
        m.chat_rows_extra = (extra + moved).clamp(0, limit);
        return;
    }
    if m.drag.is_some() {
        let over = areas
            .org
            .contains(at)
            .then(|| org_key_at(m, at.y))
            .flatten();
        drag::hover(m, over);
    }
}

/// A left press: focus the pane under it (`tab`), and act on the row under it. Leaving
/// the agent pane leaves Typing (`ctrl+q`); a press inside keeps it. The scrolled view
/// stays. The row is read before focus moves: the chat grows while it has focus, so the
/// layout after the move isn't the one that was clicked.
fn on_click(m: &mut UiModel, areas: &PaneAreas, at: Position) -> Vec<UiEffect> {
    if areas.agent.contains(at) {
        m.focus.pane = Pane::Agent;
        m.focus.card = None;
        return agent::view_if_finished(m);
    }
    let (pane, key, item) = if areas.org.contains(at) {
        (Pane::Org, org_key_at(m, at.y), None)
    } else if areas.needs_you.contains(at) {
        (Pane::NeedsYou, None, needs_you_item_at(m, at.y))
    } else if areas.chat.contains(at) {
        return on_chat(m, at);
    } else {
        return Vec::new();
    };
    m.focus.agent_mode = AgentMode::Selected;
    m.focus.pane = pane;
    m.focus.card = None;
    if let Some(key) = key {
        return select(m, key);
    }
    let Some(i) = item else {
        return Vec::new();
    };
    m.focus.needs_you_cursor = i;
    panes::needs_you_enter(m)
        .map(|agent| UiEffect::Send(Command::ViewAgent { agent }))
        .into_iter()
        .collect()
}

/// A press in the chat: on a card it focuses that card, and on `[y]`, `[e]` or `[n]` acts
/// as the key (UI §4.6); elsewhere it focuses the input. The card is read before focus
/// moves (the chat grows with focus).
fn on_chat(m: &mut UiModel, at: Position) -> Vec<UiEffect> {
    let hit = chat_card_at(m, at);
    m.focus.agent_mode = AgentMode::Selected;
    m.focus.pane = Pane::Chat;
    m.focus.card = None;
    let Some((card, button)) = hit else {
        return Vec::new();
    };
    chat::focus_card(m, card);
    match button {
        Some(key) => cards::press_button(m, key),
        None => Vec::new(),
    }
}

/// Selects `key`, as `↑↓` onto it would (a finished agent is marked seen); a team or
/// role isn't folded.
fn select(m: &mut UiModel, key: NodeKey) -> Vec<UiEffect> {
    if key == m.selected {
        return Vec::new();
    }
    m.selected = key;
    match agent_of(m, &m.selected) {
        Some(a) if m.view(&a).status.state == AgentState::Finished => {
            vec![UiEffect::Send(Command::ViewAgent { agent: a })]
        }
        _ => Vec::new(),
    }
}

fn on_wheel(m: &mut UiModel, areas: &PaneAreas, at: Position, up: bool) -> Vec<UiEffect> {
    if areas.agent.contains(at) {
        let (col, row) = inner_cell(areas.live, at);
        return vec![UiEffect::Wheel { up, col, row }];
    }
    if !areas.zoomed && areas.chat.contains(at) {
        chat::scroll(m, up, usize::from(WHEEL_LINES));
    } else if !areas.zoomed && areas.org.contains(at) {
        org_wheel(m, up, usize::from(WHEEL_LINES));
    } else if !areas.zoomed && areas.needs_you.contains(at) {
        needs_you_wheel(m, up);
    }
    Vec::new()
}

/// `at` as a 1-based cell of `live`; a pointer on the border or padding takes the
/// nearest cell inside.
fn inner_cell(live: Rect, at: Position) -> (u16, u16) {
    let clamp = |v: u16, start: u16, len: u16| v.clamp(start, start + len.max(1) - 1) - start + 1;
    (
        clamp(at.x, live.x, live.width),
        clamp(at.y, live.y, live.height),
    )
}
