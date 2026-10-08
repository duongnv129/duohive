//! Drawing the main screen (UI §3, §4.1) and the full-screen messages (UI §4.12).

mod agent_pane;
mod chat_pane;
mod dialog;
mod form;
mod header;
mod needs_you;
mod org_pane;
mod screens;

use duohive_core::settings::Settings;
use duohive_core::status::{AgentState, Status};
use duohive_core::time::Millis;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

pub use screens::{BlockingScreen, render_blocking};

use crate::focus::{Pane, layout_below, shows_replies};
use crate::format::width;
use crate::model::UiModel;
use crate::theme::Palette;

/// What drawing needs besides the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderCtx {
    /// The time ages are measured to.
    pub now: Millis,
    /// The spinner frame (any number; taken modulo the frame count).
    pub spinner: usize,
    /// Colours.
    pub palette: Palette,
    /// Reduce motion and last-reply visibility come from here.
    pub settings: Settings,
}

impl RenderCtx {
    /// The spinner frame to draw, or `None` with Reduce motion on.
    fn spinner(&self) -> Option<usize> {
        (!self.settings.reduce_motion).then_some(self.spinner)
    }
}

/// Draws the main screen.
pub fn render(frame: &mut Frame, m: &UiModel, ctx: &RenderCtx) {
    let area = frame.area();
    let p = &ctx.palette;
    let buf = frame.buffer_mut();
    buf.set_style(area, Style::default().fg(p.fg).bg(p.bg));
    let banners = header::banners(m, usize::from(area.width));
    let banner_rows = u16::try_from(banners.len()).unwrap_or(u16::MAX);
    let typing = m.focus.pane == Pane::Chat;
    let Some(l) = layout_below(area, banner_rows, m.chat_rows_extra, typing) else {
        screens::too_small(buf, area, m.needs_you.len());
        return;
    };
    let l = l.with_needs_you(m.needs_you.len());
    for (i, line) in banners.iter().enumerate() {
        let y = area.y + u16::try_from(i).unwrap_or(u16::MAX);
        put(buf, area.x, y, line, usize::from(area.width), fg(p.error));
    }
    let row = |y: u16| Rect {
        y,
        height: 1,
        ..area
    };
    header::draw_header(buf, row(area.y + banner_rows), m, ctx, l.compact);
    header::draw_footer(buf, row(area.bottom() - 1), m, p, l.compact);
    let replies = ctx.settings.show_last_reply && shows_replies(area);
    org_pane::draw(buf, &l, m, ctx, replies);
    agent_pane::draw(buf, l.agent, m, ctx);
    let chat_cursor = chat_pane::draw(buf, l.chat, m, ctx, l.compact);
    let cursor = match &m.dialog {
        Some(d) => dialog::draw(buf, area, d, p),
        None => chat_cursor,
    };
    if let Some(at) = cursor {
        frame.set_cursor_position(at);
    }
}

/// The border lines and inside style for a pane, heavy and on `bg.panel` when it has focus.
fn pane_style(p: &Palette, focused: bool) -> (&'static Lines, Style, Style) {
    if focused {
        (&HEAVY, fg(p.border_focus), Style::default().bg(p.bg_panel))
    } else {
        (&LIGHT, fg(p.border), Style::default().bg(p.bg))
    }
}

/// A style with only a foreground colour.
fn fg(c: Color) -> Style {
    Style::default().fg(c)
}

/// The colour token for a status (UI §2.1).
fn state_color(p: &Palette, s: &Status) -> Color {
    if s.quiet {
        return p.attention;
    }
    match s.state {
        AgentState::Stopped | AgentState::Starting | AgentState::Idle => p.fg_muted,
        AgentState::Working => p.working,
        AgentState::Waiting => p.attention,
        AgentState::Finished => p.done,
        AgentState::Errored => p.error,
    }
}

/// The colour for a roll-up or Needs you glyph.
fn glyph_color(p: &Palette, glyph: char) -> Color {
    match glyph {
        '●' => p.working,
        '◆' | '◷' => p.attention,
        '✕' => p.error,
        _ => p.fg,
    }
}

/// Writes `s` at (x, y), at most `max` cells and never past the buffer; returns the next x.
fn put(buf: &mut Buffer, x: u16, y: u16, s: &str, max: usize, style: Style) -> u16 {
    let a = buf.area;
    if y < a.y || y >= a.bottom() || x >= a.right() {
        return x;
    }
    buf.set_stringn(x, y, s, max, style).0
}

/// Line-drawing characters for a pane.
struct Lines {
    top: [&'static str; 2],
    bottom: [&'static str; 2],
    tee: [&'static str; 2],
    h: &'static str,
    v: &'static str,
}

/// The focused pane's heavy border (UI §2.2).
const HEAVY: Lines = Lines {
    top: ["┏", "┓"],
    bottom: ["┗", "┛"],
    tee: ["┣", "┫"],
    h: "━",
    v: "┃",
};
/// Other panes' light border.
const LIGHT: Lines = Lines {
    top: ["┌", "┐"],
    bottom: ["└", "┘"],
    tee: ["├", "┤"],
    h: "─",
    v: "│",
};

/// Where a run of text goes: a position and the cells left on its line.
#[derive(Clone, Copy, Debug)]
struct At {
    x: u16,
    y: u16,
    room: usize,
}

/// Writes styled pieces one after another, within `at.room` cells; returns where the next goes.
fn put_pieces<S: AsRef<str>>(
    buf: &mut Buffer,
    at: At,
    pieces: impl IntoIterator<Item = (S, Color)>,
) -> At {
    pieces.into_iter().fold(at, |at, (text, color)| {
        let x = put(buf, at.x, at.y, text.as_ref(), at.room, fg(color));
        At {
            x,
            room: at.room.saturating_sub(usize::from(x - at.x)),
            ..at
        }
    })
}

/// A horizontal edge `<l>━ title ━━━<r>` along the one-row `r`; the title is cut to fit.
fn edge(buf: &mut Buffer, r: Rect, ends: [&str; 2], title: &str, l: &Lines, st: Style) {
    let w = usize::from(r.width);
    if w < 2 {
        return;
    }
    let mut text = format!("{}{}", ends[0], l.h);
    let room = w.saturating_sub(5);
    if !title.is_empty() && room > 0 {
        text.push_str(&format!(" {} ", crate::format::truncate_end(title, room)));
    }
    let fill = w.saturating_sub(width(&text) + 1);
    text.push_str(&l.h.repeat(fill));
    text.push_str(ends[1]);
    put(buf, r.x, r.y, &text, w, st);
}

/// Draws a box around `r` with `title` in its top edge, and fills its inside with `inside`.
fn draw_box(buf: &mut Buffer, r: Rect, title: &str, l: &Lines, border: Style, inside: Style) {
    if r.width < 2 || r.height < 2 {
        return;
    }
    buf.set_style(r, inside);
    edge(buf, r, l.top, title, l, border);
    for y in r.y + 1..r.bottom() - 1 {
        put(buf, r.x, y, l.v, 1, border);
        put(buf, r.right() - 1, y, l.v, 1, border);
    }
    let bottom = Rect {
        y: r.bottom() - 1,
        ..r
    };
    edge(buf, bottom, l.bottom, "", l, border);
}

#[cfg(test)]
mod pane_tests;
#[cfg(test)]
mod screen_tests;
#[cfg(test)]
mod tests;
