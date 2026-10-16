//! The mouse on the main screen (P2-D25): the wheel goes to the pane under the pointer,
//! without moving focus or changing mode.

use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::UiEffect;
use crate::chat;
use crate::model::UiModel;
use crate::render::{PaneAreas, pane_areas};

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
