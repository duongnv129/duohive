//! The mouse on the main screen (P2-D25): the wheel goes to the pane under the pointer,
//! without moving focus or changing mode; a left press does what a key does, and never
//! reaches the agent.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use duohive_core::command::Command;
use duohive_core::status::AgentState;
use ratatui::layout::{Position, Rect};

use super::{UiEffect, agent, agent_of, panes};
use crate::chat;
use crate::focus::{AgentMode, Pane};
use crate::model::UiModel;
use crate::render::{PaneAreas, needs_you_item_at, org_key_at, pane_areas};

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
/// stays.
fn on_click(m: &mut UiModel, areas: &PaneAreas, at: Position) -> Vec<UiEffect> {
    if areas.agent.contains(at) {
        m.focus.pane = Pane::Agent;
        return agent::view_if_finished(m);
    }
    let pane = if areas.org.contains(at) {
        Pane::Org
    } else if areas.needs_you.contains(at) {
        Pane::NeedsYou
    } else if areas.chat.contains(at) {
        Pane::Chat
    } else {
        return Vec::new();
    };
    m.focus.agent_mode = AgentMode::Selected;
    m.focus.pane = pane;
    match pane {
        Pane::Org => select_row(m, at.y),
        Pane::NeedsYou => match needs_you_item_at(m, at.y) {
            Some(i) => {
                m.focus.needs_you_cursor = i;
                panes::needs_you_enter(m)
                    .map(|agent| UiEffect::Send(Command::ViewAgent { agent }))
                    .into_iter()
                    .collect()
            }
            None => Vec::new(),
        },
        Pane::Agent | Pane::Chat => Vec::new(),
    }
}

/// Selects the tree node on row `y`, as `↑↓` onto it would (a finished agent is marked
/// seen); a team or role isn't folded.
fn select_row(m: &mut UiModel, y: u16) -> Vec<UiEffect> {
    let Some(key) = org_key_at(m, y) else {
        return Vec::new();
    };
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
