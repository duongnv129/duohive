//! Selecting text with a plain mouse drag (UI §5): a press inside the agent pane, the
//! chat or Needs you starts a selection, dragging extends it within that pane, and the
//! release copies what is selected. A drag that starts on an org-tree row still moves
//! the node, and one on the chat's top edge still resizes it. The selection is drawn
//! reversed and cleared by the next press or key.
//!
//! The text is read from the frame as drawn, so it is whatever the pane shows: the
//! agent's terminal, the chat history, the Needs you rows.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};

use super::UiEffect;
use crate::model::UiModel;
use crate::render::{PaneAreas, pane_areas};

/// The pane a selection lives in; it never crosses into another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelPane {
    /// The agent pane (zoomed or not).
    Agent,
    /// The chat pane.
    Chat,
    /// Needs you.
    NeedsYou,
}

/// A selection: where the press was and where the pointer is now (screen cells).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSel {
    /// The pane it started in.
    pub pane: SelPane,
    /// The press.
    pub anchor: Position,
    /// The pointer now, kept inside the pane.
    pub head: Position,
}

/// A pane's text area: inside its border.
fn inner(r: Rect) -> Rect {
    Rect {
        x: r.x.saturating_add(1),
        y: r.y.saturating_add(1),
        width: r.width.saturating_sub(2),
        height: r.height.saturating_sub(2),
    }
}

fn pane_rect(areas: &PaneAreas, pane: SelPane) -> Rect {
    inner(match pane {
        SelPane::Agent => areas.agent,
        SelPane::Chat => areas.chat,
        SelPane::NeedsYou => areas.needs_you,
    })
}

/// Starts a selection when the press is inside a pane's text. Zoomed, only the agent
/// pane counts.
pub(crate) fn start(m: &mut UiModel, areas: &PaneAreas, at: Position) {
    for pane in [SelPane::Agent, SelPane::Chat, SelPane::NeedsYou] {
        if areas.zoomed && pane != SelPane::Agent {
            continue;
        }
        if pane_rect(areas, pane).contains(at) {
            m.selection = Some(TextSel {
                pane,
                anchor: at,
                head: at,
            });
            return;
        }
    }
}

/// The pointer moved with the button held: the selection's end follows it, kept inside
/// the pane it started in.
pub(crate) fn extend(m: &mut UiModel, at: Position) {
    let Some(sel) = m.selection else { return };
    let Some(areas) = pane_areas(m) else { return };
    let r = pane_rect(&areas, sel.pane);
    if r.width == 0 || r.height == 0 {
        return;
    }
    let head = Position::new(
        at.x.clamp(r.x, r.right() - 1),
        at.y.clamp(r.y, r.bottom() - 1),
    );
    m.selection = Some(TextSel { head, ..sel });
}

/// The button was released: copy a real selection, forget a click.
pub(crate) fn finish(m: &mut UiModel) -> Vec<UiEffect> {
    match m.selection {
        Some(sel) if sel.anchor != sel.head => vec![UiEffect::CopySelection],
        _ => {
            m.selection = None;
            Vec::new()
        }
    }
}

/// The selection's rows as `(y, first x, last x)`, both ends included, in reading order.
fn rows(sel: &TextSel, r: Rect) -> Vec<(u16, u16, u16)> {
    let (a, b) = if (sel.anchor.y, sel.anchor.x) <= (sel.head.y, sel.head.x) {
        (sel.anchor, sel.head)
    } else {
        (sel.head, sel.anchor)
    };
    if r.width == 0 {
        return Vec::new();
    }
    (a.y..=b.y)
        .map(|y| {
            let first = if y == a.y { a.x } else { r.x };
            let last = if y == b.y { b.x } else { r.right() - 1 };
            (y, first, last)
        })
        .collect()
}

/// The selected text as drawn in `buf`: one line per row, trailing spaces dropped.
pub(crate) fn text(buf: &Buffer, m: &UiModel) -> String {
    let (Some(sel), Some(areas)) = (m.selection, pane_areas(m)) else {
        return String::new();
    };
    let r = pane_rect(&areas, sel.pane);
    let mut lines: Vec<String> = rows(&sel, r)
        .into_iter()
        .map(|(y, first, last)| {
            let line: String = (first..=last)
                .filter(|x| buf.area.contains(Position::new(*x, y)))
                .map(|x| buf[(x, y)].symbol())
                .collect();
            line.trim_end().to_owned()
        })
        .collect();
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines.join("\n")
}

/// Draws the selection reversed.
pub(crate) fn paint(buf: &mut Buffer, m: &UiModel) {
    let (Some(sel), Some(areas)) = (m.selection, pane_areas(m)) else {
        return;
    };
    let r = pane_rect(&areas, sel.pane);
    for (y, first, last) in rows(&sel, r) {
        for x in first..=last {
            if buf.area.contains(Position::new(x, y)) {
                buf[(x, y)].set_style(Style::default().add_modifier(Modifier::REVERSED));
            }
        }
    }
}
