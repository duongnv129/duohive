//! The approval card docked over the top of the agent pane (UI §4.6; CD-D1 to CD-D7).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::agent_pane::live_area;
use super::card::{CARD_WIDTH, Look, Row, card_rows};
use super::{RenderCtx, put};
use crate::dock::shown_dock;
use crate::model::UiModel;

/// Fewest inner rows a pane needs for a card to be drawn (CD-D7).
const MIN_PANE_ROWS: usize = 7;
/// Fewest rows a card is allowed, however small the pane is.
const MIN_CARD_ROWS: usize = 5;

/// Where the card goes in the pane `pane`: the live terminal's top-left, as wide as a
/// card is, at most.
pub(super) fn dock_rect(pane: Rect, rows: usize) -> Rect {
    let live = live_area(pane);
    Rect {
        x: live.x,
        y: live.y,
        width: live
            .width
            .min(u16::try_from(CARD_WIDTH).unwrap_or(u16::MAX)),
        height: u16::try_from(rows).unwrap_or(u16::MAX).min(live.height),
    }
}

/// The card's rows for the pane `pane`: the first of the shown agent's queue, cut to half
/// the pane; none when the pane is too small.
pub(super) fn dock_rows(m: &UiModel, ctx: &RenderCtx, pane: Rect) -> Option<Vec<Row>> {
    let inner = usize::from(pane.height.saturating_sub(2));
    if inner < MIN_PANE_ROWS || pane.width < 8 {
        return None;
    }
    let dock = shown_dock(m)?;
    let card = m.cards.get(&dock.card)?;
    let look = Look {
        focused: m.focus.card == Some(dock.card),
        cells: usize::from(live_area(pane).width),
        queue: Some((dock.position, dock.total)),
        max_rows: (inner / 2).max(MIN_CARD_ROWS),
    };
    Some(card_rows(card, m, ctx.now, look, &ctx.palette))
}

/// Draws the card over the pane's top rows; the terminal underneath keeps its size.
pub(super) fn draw(buf: &mut Buffer, pane: Rect, m: &UiModel, ctx: &RenderCtx) {
    let Some(rows) = dock_rows(m, ctx, pane) else {
        return;
    };
    let at = dock_rect(pane, rows.len());
    for (i, row) in rows.iter().take(usize::from(at.height)).enumerate() {
        let y = at.y + u16::try_from(i).unwrap_or(0);
        let mut x = at.x;
        for (text, colour) in row {
            let room = usize::from(at.right().saturating_sub(x));
            x = put(
                buf,
                x,
                y,
                text,
                room,
                Style::default().fg(*colour).bg(ctx.palette.bg),
            );
        }
    }
}
