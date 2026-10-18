//! The mouse on the main screen (P2-D25): the wheel goes to the pane under the pointer,
//! without moving focus or changing mode; a left press does what a key does, and never
//! reaches the agent.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use duohive_core::command::Command;
use duohive_core::org::NodeKey;
use duohive_core::status::AgentState;
use ratatui::layout::{Position, Rect};

use super::{UiEffect, agent, agent_of, cards, panes};
use crate::chat;
use crate::focus::{AgentMode, Pane};
use crate::model::UiModel;
use crate::render::{PaneAreas, chat_card_at, needs_you_item_at, org_key_at, pane_areas};

/// Lines one wheel tick scrolls DuoHive's own views (the snapshot, the chat history).
pub(crate) const WHEEL_LINES: u16 = 3;

/// What a mouse event does. Nothing while a dialog is open or the screen is too small.
pub(crate) fn on_mouse(m: &mut UiModel, ev: MouseEvent) -> Vec<UiEffect> {
    if m.dialog.is_some() {
        return Vec::new();
    }
    let Some(areas) = pane_areas(m) else {
        return Vec::new();
    };
    let at = Position::new(ev.column, ev.row);
    match ev.kind {
        MouseEventKind::ScrollUp => on_wheel(m, &areas, at, true),
        MouseEventKind::ScrollDown => on_wheel(m, &areas, at, false),
        MouseEventKind::Down(MouseButton::Left) if !areas.zoomed => on_click(m, &areas, at),
        _ => Vec::new(),
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
