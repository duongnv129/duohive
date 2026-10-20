//! A confirmation dialog over the main screen (UI §4.8).

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;

use super::{LIGHT, draw_box, fg, put};
use crate::format::{width, wrap_words};
use crate::model::{Confirm, Dialog, UiModel};
use crate::theme::Palette;
use duohive_core::time::Millis;

/// The mock-ups' dialog width; narrower screens get the screen less a margin.
const WIDTH: u16 = 64;
const MARGIN: u16 = 4;
/// Cells between the border and the text, on each side.
const PAD: u16 = 2;
/// Cells after the last button, as in the mock-ups.
const BUTTONS_RIGHT: u16 = 6;
const FOCUS: &str = "▌";
const GAP: &str = "   ";

/// Draws the open dialog centred over `area`; returns where the terminal cursor goes.
pub(super) fn draw(
    buf: &mut Buffer,
    area: Rect,
    d: &Dialog,
    m: &UiModel,
    now: Millis,
    p: &Palette,
) -> Option<Position> {
    match d {
        Dialog::Confirm(c) => draw_confirm(buf, area, c, p),
        Dialog::Form(f) => return super::form::draw_form(buf, area, f, p),
        Dialog::Chooser(c) => super::form::draw_chooser(buf, area, c, p),
        Dialog::GiveWork(g) => return super::give_work::draw(buf, area, g, now, p),
        Dialog::Settings(s) => return super::settings::draw(buf, area, s, m, p),
        Dialog::Worktrees(l) => super::worktrees::draw(buf, area, *l, m, now, p),
        Dialog::KeyMap(s) => super::help::draw(buf, area, *s, m, p),
        Dialog::Keys(s) => super::keys_editor::draw(buf, area, s, m, p),
    }
    None
}

fn draw_confirm(buf: &mut Buffer, area: Rect, c: &Confirm, p: &Palette) {
    let w = WIDTH.min(area.width.saturating_sub(MARGIN));
    let text_w = usize::from(w.saturating_sub(2 + 2 * PAD));
    if text_w == 0 {
        return;
    }
    let lines: Vec<String> = c.lines.iter().flat_map(|l| wrap_words(l, text_w)).collect();
    // Border, blank, the lines, blank, buttons, border.
    let h = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(5)
        .min(area.height);
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
    draw_box(buf, r, &c.title, &LIGHT, fg(p.border_focus), inside);
    let x = r.x + 1 + PAD;
    let last_text_row = r.bottom().saturating_sub(3);
    for (i, line) in lines.iter().enumerate() {
        let y = r.y + 2 + u16::try_from(i).unwrap_or(u16::MAX);
        if y > last_text_row {
            break;
        }
        put(buf, x, y, line, text_w, fg(p.fg));
    }
    draw_buttons(buf, r, c, p);
}

/// `[ y Stop ]   ▌[ n Cancel ]`, right-aligned, `▌` before the focused button.
fn draw_buttons(buf: &mut Buffer, r: Rect, c: &Confirm, p: &Palette) {
    let yes = format!("[ y {} ]", c.yes_label);
    let no = "[ n Cancel ]";
    let marker = |focused: bool| if focused { FOCUS } else { " " };
    let total = 2 * width(FOCUS) + width(&yes) + width(GAP) + width(no);
    let y = r.bottom().saturating_sub(2);
    let right = usize::from(r.right().saturating_sub(1 + BUTTONS_RIGHT));
    let Some(start) = right.checked_sub(total) else {
        return;
    };
    let start = u16::try_from(start).unwrap_or(r.x).max(r.x + 1);
    let button = |focused: bool| {
        if focused {
            p.selected().fg(p.fg)
        } else {
            fg(p.fg_muted)
        }
    };
    let pieces = [
        (marker(c.focus_yes), fg(p.border_focus)),
        (yes.as_str(), button(c.focus_yes)),
        (GAP, fg(p.fg)),
        (marker(!c.focus_yes), fg(p.border_focus)),
        (no, button(!c.focus_yes)),
    ];
    let room = usize::from(r.right().saturating_sub(1));
    pieces.into_iter().fold(start, |x, (text, style)| {
        put(buf, x, y, text, room.saturating_sub(usize::from(x)), style)
    });
}
