//! Forms and the New chooser over the main screen (UI §4.4.1–4.4.2).

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};

use super::{LIGHT, draw_box, fg, put};
use crate::format::{one_line, pad_right, truncate_end, width};
use crate::forms::{Chooser, Field, FieldId, FieldKind, Form, VISIBLE_ROWS};
use crate::input::Action;
use crate::theme::Palette;

const MARGIN: u16 = 4;
/// Headings, footers and stacked prompts start this far in.
const TEXT_X: u16 = 2;
/// Cells between a control and the next one on its line.
const ROW_GAP: u16 = 3;
const FOCUS: &str = "▌";
pub(super) const HINT: &str = "tab next field · ctrl+s save · esc cancel";
const CHOOSER_WIDTH: u16 = 40;
pub(super) const CHOOSER_HINT: &str = "↑↓ choose · enter open form · esc";
const INHERIT: &str = "inherit: ";
const POPUP_HINT: &str = "type to filter · ↑↓ · enter choose · esc close";
/// The popup is at least this wide inside its border.
const POPUP_MIN: usize = 24;

/// One line inside a form.
enum Row<'a> {
    Blank,
    Text(u16, String, Color),
    Stacked(&'a str, Option<&'a str>, usize),
    Fields(Vec<usize>),
    Buttons,
}

/// The box: centred, at most the form's width, as tall as its rows (clipped to the screen).
pub(super) fn frame_box(
    buf: &mut Buffer,
    area: Rect,
    w: u16,
    rows: usize,
    title: &str,
    p: &Palette,
) -> Rect {
    let w = w.min(area.width.saturating_sub(MARGIN));
    let h = u16::try_from(rows)
        .unwrap_or(u16::MAX)
        .saturating_add(2)
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
    draw_box(buf, r, title, &LIGHT, fg(p.border_focus), inside);
    r
}

/// Draws a form; returns where the terminal cursor goes (the focused text field).
pub(super) fn draw_form(buf: &mut Buffer, area: Rect, f: &Form, p: &Palette) -> Option<Position> {
    let rows = rows(f, p);
    let r = frame_box(buf, area, f.width, rows.len(), &f.title, p);
    let x0 = r.x + 1;
    let right = r.right().saturating_sub(1);
    let mut cursor = None;
    let mut popup: Option<(u16, u16, &Field)> = None;
    for (i, row) in rows.iter().enumerate() {
        let y = r.y + 1 + u16::try_from(i).unwrap_or(u16::MAX);
        if y >= r.bottom().saturating_sub(1) {
            break;
        }
        let line = Line { x0, y, right };
        match row {
            Row::Blank => {}
            Row::Text(x, text, color) => {
                line.put(x0 + x, text, fg(*color), buf);
            }
            Row::Stacked(level, prompt, level_w) => {
                draw_stacked(buf, line, level, *prompt, *level_w, p);
            }
            Row::Fields(group) => {
                if let Some(c) = draw_fields(buf, line, f, group, p, &mut popup) {
                    cursor = Some(c);
                }
            }
            Row::Buttons => draw_buttons(buf, line, f, p),
        }
    }
    if let Some((x, y, fld)) = popup {
        draw_popup(buf, area, (x, y), fld, right, p);
    }
    cursor
}

/// Where a row goes: its first inner cell, its line and the border's column.
#[derive(Clone, Copy)]
struct Line {
    x0: u16,
    y: u16,
    right: u16,
}

impl Line {
    /// Writes `s` at `x`, never past the border; returns the next x.
    fn put(self, x: u16, s: &str, style: Style, buf: &mut Buffer) -> u16 {
        let room = usize::from(self.right.saturating_sub(x));
        if room == 0 {
            return x;
        }
        put(buf, x, self.y, s, room, style)
    }
}

/// The rows of a form, top to bottom.
fn rows<'a>(f: &'a Form, p: &Palette) -> Vec<Row<'a>> {
    let mut out = vec![Row::Blank];
    for group in groups(f) {
        let Some(first) = group.first().and_then(|i| f.fields.get(*i)) else {
            continue;
        };
        if first.look.blank_before {
            out.push(Row::Blank);
        }
        if let Some(h) = &first.look.heading {
            out.push(Row::Text(TEXT_X, h.clone(), p.fg));
        }
        if first.id == FieldId::Prompt {
            let level_w = f.stacked.iter().map(|(l, _)| width(l)).max().unwrap_or(0);
            out.extend(
                f.stacked
                    .iter()
                    .map(|(l, t)| Row::Stacked(l.as_str(), t.as_deref(), level_w)),
            );
        }
        let under = first.look.indent + first.look.label_w;
        let lines: Vec<Row<'a>> = group
            .iter()
            .filter_map(|i| f.fields.get(*i))
            .filter_map(|fld| match (&fld.error, &fld.note) {
                (Some(e), _) => Some(Row::Text(under, format!("✕ {e}"), p.error)),
                (None, Some(n)) => Some(Row::Text(under, n.clone(), p.fg_muted)),
                (None, None) => None,
            })
            .collect();
        out.push(Row::Fields(group));
        out.extend(lines);
    }
    out.push(Row::Blank);
    if let Some(note) = &f.footer {
        out.push(Row::Text(TEXT_X, note.clone(), p.fg));
    }
    if let Some(r) = &f.rejected {
        out.push(Row::Text(TEXT_X, format!("✕ {r}"), p.error));
    }
    out.push(Row::Buttons);
    out
}

/// Field indexes by line: a field drawn on the previous one's line joins its group.
fn groups(f: &Form) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = Vec::new();
    for (i, fld) in f.fields.iter().enumerate() {
        match out.last_mut() {
            Some(g) if fld.look.same_row => g.push(i),
            _ => out.push(vec![i]),
        }
    }
    out
}

/// `┊ Payments  (none)`: one level the agent's prompt stacks under, read-only.
fn draw_stacked(
    buf: &mut Buffer,
    line: Line,
    level: &str,
    prompt: Option<&str>,
    level_w: usize,
    p: &Palette,
) {
    let x = line.put(line.x0 + TEXT_X, "┊ ", fg(p.fg_muted), buf);
    let x = line.put(x, &pad_right(level, level_w + 2), fg(p.fg_muted), buf);
    let room = usize::from(line.right.saturating_sub(x));
    let text = match prompt {
        Some(t) => truncate_end(&one_line(t), room),
        None => "(none)".to_owned(),
    };
    line.put(x, &text, fg(p.fg_muted), buf);
}

/// A line of fields: label, control, suffix; returns the cursor when one is focused.
fn draw_fields<'a>(
    buf: &mut Buffer,
    line: Line,
    f: &'a Form,
    group: &[usize],
    p: &Palette,
    popup: &mut Option<(u16, u16, &'a Field)>,
) -> Option<Position> {
    let mut x = line.x0;
    let mut cursor = None;
    for (k, &i) in group.iter().enumerate() {
        let Some(fld) = f.fields.get(i) else {
            continue;
        };
        let look = &fld.look;
        let mut cx = match (k, look.col) {
            (0, _) => line.x0 + look.indent,
            (_, Some(col)) => line.x0 + col,
            (_, None) => x + ROW_GAP,
        };
        if !look.label_inside {
            cx = line.put(
                cx,
                &pad_right(&fld.label, usize::from(look.label_w)),
                fg(p.fg),
                buf,
            );
        }
        let focused = f.focus == i;
        if focused {
            line.put(cx.saturating_sub(1), FOCUS, fg(p.border_focus), buf);
        }
        if focused && fld.drop.open {
            *popup = Some((cx, line.y, fld));
        }
        let (next, at) = draw_control(buf, line, cx, fld, focused, p);
        record_field(f, i, fld, cx, next, line.y);
        cursor = cursor.or(at);
        cx = next;
        if let Some(s) = &fld.suffix {
            let color = match s.chars().next() {
                Some('✓') => p.done,
                Some('✕') => p.error,
                _ => p.fg_muted,
            };
            let gap = " ".repeat(usize::from(look.suffix_gap));
            cx = line.put(cx, &format!("{gap}{s}"), fg(color), buf);
        }
        x = cx;
    }
    cursor
}

/// A click on a field focuses it; on a choice it also steps it, on a dropdown it opens it.
fn record_field(f: &Form, i: usize, fld: &Field, from: u16, to: u16, y: u16) {
    if fld.locked || fld.kind == FieldKind::ReadOnly || to <= from {
        return;
    }
    let mut keys = vec![Action::Tab; f.tabs_to(i)];
    match fld.kind {
        FieldKind::Choice(_) => keys.push(Action::Right),
        FieldKind::Dropdown if !fld.drop.open => keys.push(Action::DialogEnter),
        _ => {}
    }
    if fld.drop.open && f.focus == i {
        return;
    }
    super::hits::record(
        Rect {
            x: from,
            y,
            width: to - from,
            height: 1,
        },
        keys,
    );
}

/// `[ value ▾ ]` or `[ text____ ]`; returns the next x and, for focused text, the cursor.
fn draw_control(
    buf: &mut Buffer,
    line: Line,
    x: u16,
    fld: &Field,
    focused: bool,
    p: &Palette,
) -> (u16, Option<Position>) {
    let read_only = fld.kind == FieldKind::ReadOnly;
    let bracket = if focused {
        fg(p.border_focus)
    } else if read_only {
        fg(p.fg_muted)
    } else {
        fg(p.fg)
    };
    let x = line.put(x, "[ ", bracket, buf);
    if let FieldKind::Choice(_) = &fld.kind {
        let choice = fld.choice();
        let color = if choice.starts_with(INHERIT) {
            p.fg_muted
        } else {
            p.fg
        };
        let x = line.put(x, &format!("{choice} ▾"), fg(color), buf);
        return (line.put(x, " ]", bracket, buf), None);
    }
    let prefix = if fld.look.label_inside {
        format!("{}: ", fld.label)
    } else {
        String::new()
    };
    let text_w = width(fld.edit.text());
    let inner = match fld.kind {
        FieldKind::Count => text_w.max(1),
        _ => usize::from(fld.look.inner),
    };
    let arrow = fld.kind == FieldKind::Dropdown;
    // A dropdown keeps two cells for ` ▾` inside the brackets.
    let avail = inner
        .saturating_sub(width(&prefix))
        .saturating_sub(if arrow { 2 } else { 0 });
    let x = line.put(x, &prefix, fg(p.fg), buf);
    let fill = if fld.kind == FieldKind::Prompt {
        " "
    } else {
        "_"
    };
    let (shown, color, col) = match &fld.placeholder {
        _ if arrow && fld.drop.loading && fld.edit.text().is_empty() => {
            ("loading models…".to_owned(), p.fg_muted, 0)
        }
        Some(ph) if fld.edit.text().is_empty() => (truncate_end(ph, avail), p.fg_muted, 0),
        // A count is short and shown whole; the cursor may sit on the space after it.
        _ if fld.kind == FieldKind::Count => (fld.edit.text().to_owned(), p.fg, fld.edit.cursor()),
        _ => {
            let (vis, col) = fld.edit.window(avail);
            let color = if read_only { p.fg_muted } else { p.fg };
            (vis, color, col)
        }
    };
    let text_x = x;
    let x = line.put(x, &shown, fg(color), buf);
    let x = line.put(
        x,
        &fill.repeat(avail.saturating_sub(width(&shown))),
        fg(p.fg_muted),
        buf,
    );
    let x = if arrow {
        let mark = if fld.drop.open { " ▴" } else { " ▾" };
        line.put(x, mark, fg(p.fg_muted), buf)
    } else {
        x
    };
    let cursor = (focused && !read_only).then(|| Position {
        x: text_x + u16::try_from(col).unwrap_or(0),
        y: line.y,
    });
    (line.put(x, " ]", bracket, buf), cursor)
}

/// The popup under a Model or Effort field (UI §4.4.1): at most six rows, `… N more` when
/// the list goes on, and a key hint under it. It is clipped to the screen.
fn draw_popup(
    buf: &mut Buffer,
    area: Rect,
    (x, y): (u16, u16),
    fld: &Field,
    right: u16,
    p: &Palette,
) {
    let rows = fld.drop_rows();
    let first = (fld.drop.cursor + 1).saturating_sub(VISIBLE_ROWS);
    let shown: Vec<_> = rows.iter().skip(first).take(VISIBLE_ROWS).collect();
    let more = rows.len().saturating_sub(first + shown.len());
    let longest = rows.iter().map(|r| width(&r.label())).max().unwrap_or(0);
    let hint = POPUP_HINT;
    let inner = longest.max(POPUP_MIN);
    let w = u16::try_from(inner + 4).unwrap_or(u16::MAX).min(area.width);
    let lines = shown.len() + usize::from(more > 0);
    let h = u16::try_from(lines + 2).unwrap_or(u16::MAX);
    let x = x.min(area.right().saturating_sub(w));
    let top = y + 1;
    if top + h + 1 > area.bottom() {
        return;
    }
    let r = Rect {
        x,
        y: top,
        width: w,
        height: h,
    };
    let inside = Style::default().fg(p.fg).bg(p.bg_panel);
    draw_box(buf, r, "", &LIGHT, fg(p.border_focus), inside);
    for (i, row) in shown.iter().enumerate() {
        let ry = r.y + 1 + u16::try_from(i).unwrap_or(0);
        let focused = first + i == fld.drop.cursor;
        let delta =
            isize::try_from(first + i).unwrap_or(0) - isize::try_from(fld.drop.cursor).unwrap_or(0);
        super::hits::record(
            Rect {
                x: r.x + 1,
                y: ry,
                width: w - 2,
                height: 1,
            },
            super::hits::row_keys(delta),
        );
        let style = if focused {
            p.selected().fg(p.fg)
        } else {
            inside
        };
        let text = format!("{}{}", if focused { FOCUS } else { " " }, row.label());
        put(
            buf,
            r.x + 1,
            ry,
            &pad_right(&text, usize::from(w) - 2),
            usize::from(w) - 2,
            style,
        );
    }
    if more > 0 {
        let ry = r.y + 1 + u16::try_from(shown.len()).unwrap_or(0);
        let text = format!(" … {more} more · ↓ scroll");
        let inner_w = usize::from(w) - 2;
        put(
            buf,
            r.x + 1,
            ry,
            &pad_right(&text, inner_w),
            inner_w,
            inside.fg(p.fg_muted),
        );
    }
    // Under the box, over whatever the form has there.
    let hint_w = usize::from(right.saturating_sub(r.x));
    let hint_style = Style::default().fg(p.fg_muted).bg(p.bg_panel);
    put(
        buf,
        r.x,
        r.bottom(),
        &pad_right(hint, hint_w),
        hint_w,
        hint_style,
    );
}

/// `fix N errors to save` or the key hint, then `[ Save ]  [ Cancel ]` at the right.
fn draw_buttons(buf: &mut Buffer, line: Line, f: &Form, p: &Palette) {
    let errors = f.error_count();
    let (left, color) = match errors {
        0 => (HINT.to_owned(), p.fg_muted),
        1 => ("fix 1 error to save".to_owned(), p.error),
        n => (format!("fix {n} errors to save"), p.error),
    };
    let save = format!("[ {} ]", f.save_label);
    let cancel = "[ Cancel ]";
    let total = width(&save) + 2 + width(cancel);
    // The mock-ups leave 8 cells after the buttons in an 80-wide form, 6 in a 72-wide one.
    let margin = if f.width >= 80 { 8 } else { 6 };
    let end = usize::from(line.right.saturating_sub(margin));
    let Some(start) = end.checked_sub(total) else {
        return;
    };
    let start = u16::try_from(start).unwrap_or(line.x0).max(line.x0 + 1);
    // The hint shows only where it fits before the buttons (the 72-wide forms have no room).
    let left_x = line.x0 + TEXT_X;
    if usize::from(left_x) + width(&left) < usize::from(start) {
        line.put(left_x, &left, fg(color), buf);
    }
    let style = |focused: bool, enabled: bool| {
        if !enabled {
            fg(p.fg_muted)
        } else if focused {
            p.selected().fg(p.fg)
        } else {
            fg(p.fg)
        }
    };
    let can_save = errors == 0 && !f.saving;
    let x = line.put(start, &save, style(f.on_save(), can_save), buf);
    let cancel_x = x + 2;
    let button = |from: u16, text: &str| Rect {
        x: from,
        y: line.y,
        width: u16::try_from(width(text)).unwrap_or(0),
        height: 1,
    };
    super::hits::record(button(start, &save), vec![Action::Save]);
    super::hits::record(button(cancel_x, cancel), vec![Action::Esc]);
    line.put(cancel_x, cancel, style(f.on_cancel(), true), buf);
    if f.on_save() {
        line.put(start - 1, FOCUS, fg(p.border_focus), buf);
    }
    if f.on_cancel() {
        line.put(cancel_x - 1, FOCUS, fg(p.border_focus), buf);
    }
}

/// The New chooser: entries, a blank line and the key hint.
pub(super) fn draw_chooser(buf: &mut Buffer, area: Rect, c: &Chooser, p: &Palette) {
    let longest = c.items.iter().map(|i| width(&i.label)).max().unwrap_or(0);
    let w = u16::try_from(longest + 4)
        .unwrap_or(u16::MAX)
        .max(CHOOSER_WIDTH);
    let r = frame_box(buf, area, w, c.items.len() + 2, &c.title, p);
    let x0 = r.x + 1;
    let right = r.right().saturating_sub(1);
    for (i, item) in c.items.iter().enumerate() {
        let y = r.y + 1 + u16::try_from(i).unwrap_or(u16::MAX);
        let line = Line { x0, y, right };
        let focused = i == c.focus;
        let delta = isize::try_from(i).unwrap_or(0) - isize::try_from(c.focus).unwrap_or(0);
        super::hits::record(
            Rect {
                x: x0,
                y,
                width: r.width.saturating_sub(2),
                height: 1,
            },
            super::hits::row_keys(delta),
        );
        if focused {
            line.put(x0, FOCUS, fg(p.border_focus), buf);
        }
        let style = if !item.enabled {
            fg(p.fg_muted)
        } else if focused {
            p.selected().fg(p.fg)
        } else {
            fg(p.fg)
        };
        line.put(x0 + 2, &item.label, style, buf);
    }
    let y = r.bottom().saturating_sub(2);
    Line { x0, y, right }.put(x0 + 1, CHOOSER_HINT, fg(p.fg_muted), buf);
}
