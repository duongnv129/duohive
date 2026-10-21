//! Move to… (UI §4.4.2): the places, and what the highlighted one changes.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::form::frame_box;
use super::{fg, put};
use crate::format::{pad_right, width};
use crate::forms::{MoveOption, MoveTo};
use crate::theme::Palette;

const WIDTH: u16 = 60;
const TEXT_X: u16 = 3;
const FOCUS: &str = "▌";
const HINT: &str = "↑↓ choose · enter move · esc cancel";

/// The lines under the list for `o`: what it changes, or why it is refused.
fn detail(name: &str, o: &MoveOption) -> Vec<(String, bool)> {
    if let Some(reason) = &o.blocked {
        return vec![(format!("✕ {reason}"), true)];
    }
    if o.changes.is_empty() || o.current {
        return Vec::new();
    }
    let field_w = o.changes.iter().map(|c| width(c.field)).max().unwrap_or(0);
    let mut out = vec![(format!("Moving changes what {name} inherits:"), false)];
    out.extend(o.changes.iter().map(|c| {
        (
            format!(
                "  {} {} → {}",
                pad_right(c.field, field_w + 2),
                c.from,
                c.to
            ),
            false,
        )
    }));
    out
}

/// Draws the picker centred over `area`.
pub(super) fn draw(buf: &mut Buffer, area: Rect, s: &MoveTo, p: &Palette) {
    // Reserve the tallest detail so the box doesn't resize as the highlight moves.
    let tallest = s
        .options
        .iter()
        .map(|o| detail(&s.name, o).len())
        .max()
        .unwrap_or(0);
    let longest = s.options.iter().map(|o| width(&o.label)).max().unwrap_or(0);
    let w = WIDTH.max(u16::try_from(longest + 24).unwrap_or(WIDTH));
    // Blank, the places, blank, the detail, blank, the hint.
    let rows = 1 + s.options.len() + 1 + tallest + usize::from(tallest > 0) + 1;
    let title = format!("Move {} to…", s.name);
    let r = frame_box(buf, area, w, rows, &title, p);
    let inner = usize::from(r.width.saturating_sub(2));
    let inside = Style::default().fg(p.fg).bg(p.bg_panel);
    let mut y = r.y + 2;
    for (i, o) in s.options.iter().enumerate() {
        let focused = i == s.cursor;
        let style = if focused {
            p.selected().fg(p.fg)
        } else if o.current {
            fg(p.fg_muted)
        } else {
            inside
        };
        let delta = isize::try_from(i).unwrap_or(0) - isize::try_from(s.cursor).unwrap_or(0);
        super::hits::record(
            Rect {
                x: r.x + 1,
                y,
                width: r.width - 2,
                height: 1,
            },
            super::hits::row_keys(delta),
        );
        put(
            buf,
            r.x + 1,
            y,
            &" ".repeat(inner),
            inner,
            if focused { style } else { inside },
        );
        put(
            buf,
            r.x + 1,
            y,
            if focused { FOCUS } else { " " },
            1,
            fg(p.border_focus),
        );
        put(buf, r.x + TEXT_X, y, &o.label, inner, style);
        if o.current {
            let x = r.right().saturating_sub(2 + 7 + 1);
            put(buf, x, y, "current", 7, fg(p.fg_muted));
        }
        y += 1;
    }
    y += 1;
    if let Some(o) = s.selected() {
        for (line, error) in detail(&s.name, o) {
            let color = if error { p.error } else { p.fg };
            put(
                buf,
                r.x + TEXT_X,
                y,
                &line,
                inner.saturating_sub(2),
                fg(color),
            );
            y += 1;
        }
    }
    put(
        buf,
        r.x + TEXT_X,
        r.bottom().saturating_sub(2),
        HINT,
        inner,
        fg(p.fg_muted),
    );
}
