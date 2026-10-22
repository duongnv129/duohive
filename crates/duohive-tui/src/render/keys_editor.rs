//! The key bindings editor (UI §4.10): every action and its key, with capture and the
//! refusals on the row.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::{LIGHT, draw_box, fg, put};
use crate::format::pad_right;
use crate::keymap::{ActionId, KeysState};
use crate::model::UiModel;
use crate::theme::Palette;

const TITLE: &str = "Key bindings";
/// The mock-up's width plus room for the full conflict message on the row; narrower
/// screens get the screen less a margin.
const WIDTH: u16 = 76;
const MARGIN: u16 = 2;
/// Border, a blank row, the column headings, the actions, a blank, the footer, border.
const HEIGHT: u16 = 22;
const FOCUS: &str = "▌";
const ACTION_HEAD: &str = "Action";
const KEY_HEAD: &str = "Key";
/// Cells to the Key column.
const KEY_X: u16 = 26;
/// A row that is waiting for its key.
const CAPTURE: &str = "press the new key · esc cancels";
const FOOTER: &str = "enter rebind (press the new key) · backspace default · esc back";
/// The foot of a list with actions below it (UI §4.10).
const MORE: &str = "↓ more";

/// A click on row `i` presses `↑↓` to it, then `enter` (rebind).
fn record_row(r: Rect, y: u16, i: usize, cursor: usize) {
    let delta = isize::try_from(i).unwrap_or(0) - isize::try_from(cursor).unwrap_or(0);
    let area = Rect {
        x: r.x + 1,
        y,
        width: r.width - 2,
        height: 1,
    };
    super::hits::record(area, super::hits::row_keys(delta));
}

/// Draws the editor centred over `area`.
pub(super) fn draw(buf: &mut Buffer, area: Rect, st: &KeysState, m: &UiModel, p: &Palette) {
    let w = WIDTH.min(area.width.saturating_sub(MARGIN));
    let h = HEIGHT.min(area.height.saturating_sub(MARGIN));
    if w < 40 || h < 8 {
        return;
    }
    let r = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    };
    let inside = Style::default().fg(p.fg).bg(p.bg_panel);
    for y in r.y..r.bottom() {
        put(
            buf,
            r.x,
            y,
            &" ".repeat(usize::from(w)),
            usize::from(w),
            inside,
        );
    }
    draw_box(buf, r, TITLE, &LIGHT, fg(p.border_focus), inside);
    let muted = Style::default().fg(p.fg_muted).bg(p.bg_panel);
    let room = usize::from(w - 2);

    put(buf, r.x + 1 + 2, r.y + 2, ACTION_HEAD, room, muted);
    put(
        buf,
        r.x + 1 + KEY_X,
        r.y + 2,
        KEY_HEAD,
        room.saturating_sub(usize::from(KEY_X)),
        muted,
    );

    let footer_y = r.bottom() - 2;
    let notice_rows = u16::try_from(m.keys_notices.len()).unwrap_or(u16::MAX);
    let list_top = r.y + 3;
    let list_bottom = footer_y.saturating_sub(notice_rows);
    let visible = usize::from(list_bottom.saturating_sub(list_top));

    let mut start = st.scroll;
    if st.cursor < start {
        start = st.cursor;
    }
    let overflow = start + visible < ActionId::ALL.len();
    let shown = if overflow {
        visible.saturating_sub(1)
    } else {
        visible
    };
    if shown > 0 && st.cursor >= start + shown {
        start = st.cursor + 1 - shown;
    }

    for (i, action) in ActionId::ALL.iter().enumerate().skip(start).take(shown) {
        let y = list_top + u16::try_from(i - start).unwrap_or(0);
        let focused = i == st.cursor;
        let marker = if focused { FOCUS } else { " " };
        let row = if focused {
            p.selected().fg(p.fg)
        } else {
            Style::default().fg(p.fg).bg(p.bg_panel)
        };
        record_row(r, y, i, st.cursor);
        let label = format!("{marker} {:<width$}", action.label(), width = 23);
        put(buf, r.x + 1, y, &label, room, row);
        let text = if focused && st.capturing {
            CAPTURE.to_owned()
        } else if focused && st.notice.is_some() {
            st.notice.clone().unwrap_or_default()
        } else {
            m.keymap.key(*action).label()
        };
        let style = if focused && st.notice.is_some() {
            Style::default().fg(p.attention).bg(p.selection)
        } else {
            row
        };
        put(
            buf,
            r.x + 1 + KEY_X,
            y,
            &pad_right(&text, room.saturating_sub(usize::from(KEY_X))),
            room.saturating_sub(usize::from(KEY_X)),
            style,
        );
    }

    if overflow && shown > 0 {
        let y = list_top + u16::try_from(shown).unwrap_or(u16::MAX);
        put(buf, r.x + 1 + 2, y, MORE, room, muted);
    }

    let notes_top = list_bottom;
    for (i, note) in m.keys_notices.iter().enumerate() {
        let y = notes_top + u16::try_from(i).unwrap_or(0);
        if y >= footer_y {
            break;
        }
        put(buf, r.x + 1 + 2, y, note, room, fg(p.fg_muted));
    }
    put(buf, r.x + 1 + 2, footer_y, FOOTER, room, muted);
}
