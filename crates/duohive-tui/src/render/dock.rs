//! The approval card docked over the top of the agent pane (UI §4.6; CD-D1 to CD-D7).

use duohive_core::card::CardId;
use duohive_core::time::Millis;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;

use super::agent_pane::live_area;
use super::card::{CARD_WIDTH, Look, Row, button_at, card_rows, text_offset};
use super::{RenderCtx, put};
use crate::dock::shown_dock;
use crate::keymap::ActionId;
use crate::model::UiModel;
use crate::theme::Palette;

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
pub(super) fn dock_rows(m: &UiModel, now: Millis, p: &Palette, pane: Rect) -> Option<Vec<Row>> {
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
    Some(card_rows(card, m, now, look, p))
}

/// Draws the card over the pane's top rows; the terminal underneath keeps its size.
pub(super) fn draw(buf: &mut Buffer, pane: Rect, m: &UiModel, ctx: &RenderCtx) {
    let Some(rows) = dock_rows(m, ctx.now, &ctx.palette, pane) else {
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

/// What a press at `at` hits in the agent pane `pane`: the docked card, and the button
/// under it (the card's last row but one is its button row).
#[cfg_attr(not(test), expect(dead_code, reason = "the mouse reads it from CD.4"))]
pub(super) fn card_at(m: &UiModel, pane: Rect, at: Position) -> Option<(CardId, Option<ActionId>)> {
    let rows = dock_rows(m, m.now, &Palette::dark(), pane)?;
    let rect = dock_rect(pane, rows.len());
    if !rect.contains(at) {
        return None;
    }
    let id = shown_dock(m)?.card;
    let row = usize::from(at.y - rect.y);
    let button = (row + 2 == rows.len())
        .then(|| usize::from(at.x - rect.x).checked_sub(text_offset()))
        .flatten()
        .and_then(|col| button_at(&m.keymap, m.cards.get(&id)?, col));
    Some((id, button))
}
