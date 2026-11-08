//! The approval card dialog (UI §4.6): a form-style box with the card's body and the
//! confirmation's button row, `▌` before the focused button.

use duohive_core::time::Millis;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::card::{CARD_WIDTH, card_body, card_title};
use super::dialog::{BUTTONS_RIGHT, FOCUS, GAP};
use super::form::frame_box;
use super::{fg, hits, put};
use crate::card_dialog::{CardDialog, focusable, place};
use crate::format::width;
use crate::input::Action;
use crate::keymap::ActionId;
use crate::model::UiModel;
use crate::theme::Palette;

/// Cells between the border and the text, on each side.
const PAD: u16 = 2;
/// Rows that are not body: the blank under the top edge, the blank above the buttons and
/// the buttons.
const CHROME: usize = 3;
/// The title's `◆` sits after the edge's `┌─ `.
const DIAMOND_AT: u16 = 3;

/// Draws the dialog for `d` centred over `area`.
pub(super) fn draw(
    buf: &mut Buffer,
    area: Rect,
    d: &CardDialog,
    m: &UiModel,
    now: Millis,
    p: &Palette,
) {
    let Some(card) = m.cards.get(&d.card) else {
        return;
    };
    let w = usize::from(area.width.saturating_sub(4)).min(CARD_WIDTH);
    let cells = w.saturating_sub(2 + 2 * usize::from(PAD)).max(1);
    let slots = usize::from(area.height).saturating_sub(2 + CHROME).max(1);
    let body = card_body(card, m, now, cells, slots, p);
    let mut title = card_title(card, &m.org);
    let (at, total) = place(m, d.card);
    if total > 1 {
        title = format!("{title} · {at} of {total}");
    }
    let r = frame_box(buf, area, cols(CARD_WIDTH), body.len() + CHROME, &title, p);
    let x = r.x + 1 + PAD;
    for (i, (text, colour)) in body.iter().enumerate() {
        let y = r.y + 2 + cols(i);
        put(buf, x, y, text, cells, fg(*colour));
    }
    if let Some(cell) = buf.cell_mut((r.x + DIAMOND_AT, r.y)) {
        cell.set_fg(p.attention);
    }
    draw_buttons(buf, r, d, m, p);
}

/// `▌[ y Approve ]   [ e Edit ]   [ n Reject ]`, right-aligned; a button the card can't
/// take is dim. Each records the action a click on it presses.
fn draw_buttons(buf: &mut Buffer, r: Rect, d: &CardDialog, m: &UiModel, p: &Palette) {
    let reachable = focusable(m, d.card);
    let focused = reachable.get(d.focus).copied();
    let all = [
        (ActionId::CardApprove, "Approve", Action::CardApprove),
        (ActionId::CardEdit, "Edit", Action::CardEdit),
        (ActionId::CardReject, "Reject", Action::CardReject),
    ];
    let texts: Vec<String> = all
        .iter()
        .map(|(id, label, _)| format!("[ {} {label} ]", m.keymap.key(*id).label()))
        .collect();
    let total: usize = texts.iter().map(|t| width(t) + width(FOCUS)).sum::<usize>()
        + width(GAP) * (texts.len() - 1);
    let y = r.bottom().saturating_sub(2);
    let right = usize::from(r.right().saturating_sub(1 + BUTTONS_RIGHT));
    let Some(start) = right.checked_sub(total) else {
        return;
    };
    let room = usize::from(r.right().saturating_sub(1));
    let mut x = u16::try_from(start).unwrap_or(r.x).max(r.x + 1);
    for (i, ((id, _, action), text)) in all.iter().zip(&texts).enumerate() {
        let is_focus = focused == Some(*id) && !d.sent;
        let style = if is_focus {
            p.selected().fg(p.fg)
        } else if reachable.contains(id) && !d.sent {
            fg(p.fg_muted)
        } else {
            fg(p.fg_muted).add_modifier(ratatui::style::Modifier::DIM)
        };
        let marker = if is_focus { FOCUS } else { " " };
        put(
            buf,
            x,
            y,
            marker,
            room.saturating_sub(usize::from(x)),
            Style::default().fg(p.border_focus),
        );
        x += cols(width(FOCUS));
        let w = cols(width(text));
        hits::record(
            Rect {
                x,
                y,
                width: w,
                height: 1,
            },
            vec![*action],
        );
        put(buf, x, y, text, room.saturating_sub(usize::from(x)), style);
        x += w;
        if i + 1 < all.len() {
            x += cols(width(GAP));
        }
    }
}

/// `n` cells as a column count.
fn cols(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}
