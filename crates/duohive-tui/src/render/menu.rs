//! The right-click menu (UI §4.4.2): a box at the pointer, moved in to stay on screen.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::{LIGHT, draw_box, fg, put};
use crate::format::{pad_right, width};
use crate::input::Menu;
use crate::theme::Palette;

/// Cells between the label column and the key column.
const GAP: usize = 2;

/// The row text: the label, plus ` · reason` when it is off.
fn label(r: &crate::input::MenuRow) -> String {
    match &r.disabled {
        Some(why) => format!("{} · {why}", r.label),
        None => r.label.clone(),
    }
}

/// The box for `menu` on a screen `area`: placed at the pointer, then moved in.
pub(crate) fn rect(area: Rect, menu: &Menu) -> Rect {
    let label_w = menu
        .rows
        .iter()
        .map(|r| width(&label(r)))
        .max()
        .unwrap_or(0);
    let key_w = menu.rows.iter().map(|r| width(&r.key)).max().unwrap_or(0);
    let w = u16::try_from(label_w + GAP + key_w + 4)
        .unwrap_or(u16::MAX)
        .min(area.width);
    let h = u16::try_from(menu.rows.len() + 2)
        .unwrap_or(u16::MAX)
        .min(area.height);
    let x = menu.at.x.min(area.right().saturating_sub(w)).max(area.x);
    let y = menu.at.y.min(area.bottom().saturating_sub(h)).max(area.y);
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

/// Draws the menu over everything.
pub(super) fn draw(buf: &mut Buffer, area: Rect, menu: &Menu, p: &Palette) {
    let r = rect(area, menu);
    let inside = Style::default().fg(p.fg).bg(p.bg_panel);
    let title = crate::forms::node_name(&menu.node);
    draw_box(buf, r, &title, &LIGHT, fg(p.border_focus), inside);
    let inner = usize::from(r.width.saturating_sub(2));
    let key_w = menu.rows.iter().map(|x| width(&x.key)).max().unwrap_or(0);
    for (i, row) in menu.rows.iter().enumerate() {
        let y = r.y + 1 + u16::try_from(i).unwrap_or(0);
        let focused = i == menu.cursor;
        let style = if row.disabled.is_some() {
            fg(p.fg_muted).bg(p.bg_panel)
        } else if focused {
            p.selected().fg(p.fg)
        } else {
            inside
        };
        let left = format!(" {}", label(row));
        let text = format!(
            "{}{}",
            pad_right(&left, inner.saturating_sub(key_w + 1)),
            pad_right(&row.key, key_w + 1)
        );
        put(buf, r.x + 1, y, &text, inner, style);
    }
}
