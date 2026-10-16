//! Drawing the main screen (UI §3, §4.1) and the full-screen messages (UI §4.12).

mod agent_pane;
mod chat_pane;
mod dialog;
mod form;
mod give_work;
mod header;
mod needs_you;
mod org_pane;
mod screens;
mod settings;
mod worktrees;
mod zoom;

use duohive_core::names::AgentName;
use duohive_core::settings::Settings;
use duohive_core::status::{AgentState, Status};
use duohive_core::time::Millis;
use duohive_core::view::PaneSize;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

pub use screens::{BlockingScreen, render_blocking};
pub(crate) use worktrees::files_changed;

use crate::focus::{Pane, chat_extra_max_below, layout_below, shows_replies, zoom_area};
use crate::format::width;
use crate::model::UiModel;
use crate::term_view::TermView;
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
    /// The local time zone's offset from UTC, in seconds, for chat times.
    pub utc_offset_secs: i32,
}

impl RenderCtx {
    /// The spinner frame to draw, or `None` with Reduce motion on.
    fn spinner(&self) -> Option<usize> {
        (!self.settings.reduce_motion).then_some(self.spinner)
    }
}

/// The live terminal the agent pane may show: drawn only while `agent` is selected.
#[derive(Clone, Copy)]
pub struct LiveScreen<'a> {
    /// Whose terminal it is.
    pub agent: &'a AgentName,
    /// The terminal, or the frozen scrollback snapshot (P2-D12).
    pub term: &'a TermView,
    /// The first row of `term` to draw (scrolled back: above the bottom).
    pub first_row: u16,
    /// `NO_COLOR` is set: the terminal's colours are dropped.
    pub no_color: bool,
}

impl std::fmt::Debug for LiveScreen<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveScreen")
            .field("agent", &self.agent)
            .field("first_row", &self.first_row)
            .field("no_color", &self.no_color)
            .finish_non_exhaustive()
    }
}

/// Draws the main screen, the agent pane showing summaries only.
pub fn render(frame: &mut Frame, m: &UiModel, ctx: &RenderCtx) {
    render_live(frame, m, ctx, None);
}

/// Draws the main screen; the agent pane shows `live` when it belongs to the selection.
pub fn render_live(frame: &mut Frame, m: &UiModel, ctx: &RenderCtx, live: Option<LiveScreen<'_>>) {
    draw_live(frame, m, ctx, live);
    crate::theme::finish(frame.buffer_mut());
}

fn draw_live(frame: &mut Frame, m: &UiModel, ctx: &RenderCtx, live: Option<LiveScreen<'_>>) {
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
    let zoomed = m
        .zoomed_agent()
        .and_then(|a| Some((a, zoom_area(area, banner_rows)?)));
    for (i, line) in banners.iter().enumerate() {
        let y = area.y + u16::try_from(i).unwrap_or(u16::MAX);
        put(buf, area.x, y, line, usize::from(area.width), fg(p.error));
    }
    let row = |y: u16| Rect {
        y,
        height: 1,
        ..area
    };
    if let Some((agent, pane)) = zoomed {
        zoom::draw_header(buf, row(area.y + banner_rows), agent, p);
        zoom::draw_footer(buf, row(area.bottom() - 1), m, p);
        agent_pane::draw(buf, pane, m, ctx, live);
        if let Some(Some(at)) = m
            .dialog
            .as_ref()
            .map(|d| dialog::draw(buf, area, d, m, ctx.now, p))
        {
            frame.set_cursor_position(at);
        }
        return;
    }
    header::draw_header(buf, row(area.y + banner_rows), m, ctx, l.compact);
    header::draw_footer(buf, row(area.bottom() - 1), m, p, l.compact);
    let replies = ctx.settings.show_last_reply && shows_replies(area);
    org_pane::draw(buf, &l, m, ctx, replies);
    agent_pane::draw(buf, l.agent, m, ctx, live);
    let chat_cursor = chat_pane::draw(buf, l.chat, m, ctx, l.compact);
    let cursor = match &m.dialog {
        Some(d) => dialog::draw(buf, area, d, m, ctx.now, p),
        None => chat_cursor,
    };
    if let Some(at) = cursor {
        frame.set_cursor_position(at);
    }
}

/// The most rows `ctrl+↑` can add to the chat at the model's screen size, counting the
/// banners above the header as [`render`] draws them (P2-D3).
pub(crate) fn chat_extra_limit(m: &UiModel) -> i16 {
    let banners = header::banners(m, usize::from(m.screen.width)).len();
    let rows = u16::try_from(banners).unwrap_or(u16::MAX);
    chat_extra_max_below(m.screen, rows)
}

/// Where each pane is on the model's screen, as [`render`] draws it (P2-D25 hit-tests
/// against it). Zoomed, only `agent` and `live` count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneAreas {
    /// The org tree.
    pub org: Rect,
    /// Needs you.
    pub needs_you: Rect,
    /// The agent pane, border included (the zoomed pane when zoomed).
    pub agent: Rect,
    /// The chat pane.
    pub chat: Rect,
    /// The live terminal inside the agent pane.
    pub live: Rect,
    /// The agent pane fills the screen.
    pub zoomed: bool,
    /// The 80–99 × 24 layout.
    pub compact: bool,
}

/// [`PaneAreas`] for the model's screen; `None` when the main screen doesn't fit.
pub fn pane_areas(m: &UiModel) -> Option<PaneAreas> {
    let banners = header::banners(m, usize::from(m.screen.width)).len();
    let rows = u16::try_from(banners).unwrap_or(u16::MAX);
    let typing = m.focus.pane == Pane::Chat;
    let l =
        layout_below(m.screen, rows, m.chat_rows_extra, typing)?.with_needs_you(m.needs_you.len());
    let zoomed = m.zoomed_agent().is_some();
    let agent = if zoomed {
        zoom_area(m.screen, rows)?
    } else {
        l.agent
    };
    Some(PaneAreas {
        org: l.org,
        needs_you: l.needs_you,
        agent,
        chat: l.chat,
        live: agent_pane::live_area(agent),
        zoomed,
        compact: l.compact,
    })
}

/// The tree node drawn on screen row `y` of the org pane, as drawn now.
pub(crate) fn org_key_at(m: &UiModel, y: u16) -> Option<duohive_core::org::NodeKey> {
    let banners = header::banners(m, usize::from(m.screen.width)).len();
    let rows = u16::try_from(banners).unwrap_or(u16::MAX);
    let typing = m.focus.pane == Pane::Chat;
    let l =
        layout_below(m.screen, rows, m.chat_rows_extra, typing)?.with_needs_you(m.needs_you.len());
    let replies = m.settings.show_last_reply && shows_replies(m.screen);
    org_pane::key_at(&l, m, replies, y)
}

/// The Needs you item drawn on screen row `y`, as drawn now.
pub(crate) fn needs_you_item_at(m: &UiModel, y: u16) -> Option<usize> {
    needs_you::item_at(pane_areas(m)?.needs_you, m, y)
}

/// The chat history's rows and the rows it can show, as drawn now.
pub(crate) fn chat_history_fit(m: &UiModel) -> Option<(usize, usize)> {
    let a = pane_areas(m)?;
    chat_pane::history_fit(a.chat, m, a.compact)
}

/// The live terminal's size for the model's screen: the agent pane inside its border and
/// side padding (P2-D5). `None` when the main screen doesn't fit (the pane is hidden).
pub fn live_size(m: &UiModel) -> Option<PaneSize> {
    let banners = header::banners(m, usize::from(m.screen.width)).len();
    let rows = u16::try_from(banners).unwrap_or(u16::MAX);
    let typing = m.focus.pane == Pane::Chat;
    let l = layout_below(m.screen, rows, m.chat_rows_extra, typing)?;
    let pane = match m.zoomed_agent() {
        Some(_) => zoom_area(m.screen, rows)?,
        None => l.agent,
    };
    let inner = agent_pane::live_area(pane);
    (inner.width > 0 && inner.height > 0).then_some(PaneSize {
        cols: inner.width,
        rows: inner.height,
    })
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
mod auto_tests;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod pane_tests;
#[cfg(test)]
mod screen_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod worktrees_tests;
#[cfg(test)]
mod zoom_tests;
