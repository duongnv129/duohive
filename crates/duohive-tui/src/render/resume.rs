//! The resume picker (UI §4.13): the search box, two lines per session, a hint row.

use duohive_core::session_catalog::SessionInfo;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;

use super::form::frame_box;
use super::{fg, put};
use crate::format::{truncate_end, truncate_middle, width};
use crate::forms::{ResumePicker, age_text, clean, size_text};
use crate::input::resume::title_of;
use crate::theme::Palette;

/// The mock-up's width; narrower screens get the screen less a margin.
const WIDTH: u16 = 78;
/// Rows each entry takes: title, details, blank.
const ENTRY_ROWS: usize = 3;
/// Search row, blank, separator, hint.
const CHROME_ROWS: usize = 4;
/// Border rows plus the screen rows left over.
const SCREEN_MARGIN: usize = 4;
/// Longest branch shown before the in-use and recent marks, so those are never cut.
const BRANCH_CELLS: usize = 32;
/// The least of the lead worth showing next to the marks; below it only the marks show.
const MIN_LEAD_CELLS: usize = 8;
/// Cells from the box's edge to a row's marker, and to its text.
const MARKER_X: u16 = 2;
const TEXT_X: u16 = 4;
const FOCUS: &str = "▌";
const SEARCH_GLYPH: &str = "⌕ ";
const PLACEHOLDER: &str = "Search…";
const HINT: &str = "enter resume · type to search · ↑↓ move · esc cancel";
const HINT_SEARCHING: &str = "enter resume · type to search · ↑↓ move · esc clear search";
const EMPTY_HINT: &str = "esc close";
const NO_MATCH: &str = "No sessions match";
const MORE: &str = "↓ more";
const SEP: &str = " · ";

/// The details of a session in two parts: the lead (age, branch, size, worktree; branch
/// and size are left out when the CLI doesn't know them) and the marks (who has it, or
/// that it may be open elsewhere), which carry state and are the last to be cut.
fn details(s: &SessionInfo, recent: bool, now_ms: u64) -> (String, String) {
    let mut parts = vec![age_text(now_ms.saturating_sub(s.updated_ms))];
    let branch = s.branch.as_deref().map(clean).unwrap_or_default();
    if !branch.is_empty() {
        parts.push(truncate_middle(&branch, BRANCH_CELLS));
    }
    if s.size_bytes > 0 {
        parts.push(size_text(s.size_bytes));
    }
    if let Some(task) = &s.worktree {
        parts.push(format!("worktree {}", clean(&task.0)));
    }
    let mut marks = Vec::new();
    if let Some(holder) = &s.in_use_by {
        marks.push(format!("in use by {holder}"));
    }
    if recent {
        marks.push("may be open elsewhere".to_owned());
    }
    (parts.join(SEP), marks.join(SEP))
}

/// The details line fitted to `room` cells. When it is too long the lead is cut first, so
/// the marks stay whole; a lead that would be left under `MIN_LEAD_CELLS` is dropped, and
/// only marks wider than `room` themselves are cut.
fn fit_details(lead: &str, marks: &str, room: usize) -> String {
    if marks.is_empty() {
        return truncate_end(lead, room);
    }
    let whole = format!("{lead}{SEP}{marks}");
    if width(&whole) <= room {
        return whole;
    }
    let lead_room = room.saturating_sub(width(marks) + width(SEP));
    if lead_room < MIN_LEAD_CELLS {
        return truncate_end(marks, room);
    }
    format!("{}{SEP}{marks}", truncate_end(lead, lead_room))
}

/// Draws the picker centred over `area`; returns where the terminal cursor goes.
pub(super) fn draw(
    buf: &mut Buffer,
    area: Rect,
    s: &ResumePicker,
    now_ms: u64,
    p: &Palette,
) -> Option<Position> {
    if s.sessions.is_empty() {
        draw_empty(buf, area, s, p);
        return None;
    }
    let visible = s.visible();
    let capacity =
        (usize::from(area.height).saturating_sub(CHROME_ROWS + SCREEN_MARGIN) / ENTRY_ROWS).max(1);
    let shown = capacity.min(visible.len().max(1));
    let title = format!("Resume {} ({})", s.agent, s.count_label());
    let r = frame_box(
        buf,
        area,
        WIDTH,
        CHROME_ROWS + ENTRY_ROWS * shown,
        &title,
        p,
    );
    let room = usize::from(r.width.saturating_sub(TEXT_X + 1));
    let inside = Style::default().fg(p.fg).bg(p.bg_panel);
    let muted = Style::default().fg(p.fg_muted).bg(p.bg_panel);
    let cursor = draw_search(buf, r, &s.search, room, inside, muted);
    let cursor_row = s.cursor.min(visible.len().saturating_sub(1));
    let first = (cursor_row + 1).saturating_sub(shown);
    if visible.is_empty() {
        put(buf, r.x + TEXT_X, r.y + 3, NO_MATCH, room, muted);
    }
    for (i, session) in visible.iter().enumerate().skip(first).take(shown) {
        let y = r.y + 3 + u16::try_from((i - first) * ENTRY_ROWS).unwrap_or(0);
        let recent = s.needs_confirm(session, now_ms);
        let selected = i == cursor_row;
        draw_entry(buf, r, y, (session, recent, selected), now_ms, p);
        let delta = isize::try_from(i).unwrap_or(0) - isize::try_from(cursor_row).unwrap_or(0);
        super::hits::record(
            Rect::new(r.x + 1, y, r.width.saturating_sub(2), 2),
            super::hits::row_keys(delta),
        );
    }
    draw_foot(
        buf,
        r,
        !s.search.is_empty(),
        first + shown < visible.len(),
        p,
    );
    Some(cursor)
}

/// `⌕ Search…` or the typed search; returns the cell the terminal cursor sits in.
fn draw_search(
    buf: &mut Buffer,
    r: Rect,
    search: &str,
    room: usize,
    inside: Style,
    muted: Style,
) -> Position {
    put(buf, r.x + MARKER_X, r.y + 1, SEARCH_GLYPH, room, muted);
    let x = r.x + TEXT_X;
    if search.is_empty() {
        put(buf, x, r.y + 1, PLACEHOLDER, room, muted);
        return Position::new(x, r.y + 1);
    }
    let shown = truncate_end(&clean_keep_spaces(search), room.saturating_sub(1));
    put(buf, x, r.y + 1, &shown, room, inside);
    let used = u16::try_from(width(&shown)).unwrap_or(0);
    Position::new(x + used, r.y + 1)
}

/// The search as typed (a trailing space is meaningful while typing), without controls.
fn clean_keep_spaces(search: &str) -> String {
    search.chars().filter(|c| !c.is_control()).collect()
}

/// One session: the marker and title, then the details in the muted foreground.
fn draw_entry(
    buf: &mut Buffer,
    r: Rect,
    y: u16,
    (session, recent, selected): (&SessionInfo, bool, bool),
    now_ms: u64,
    p: &Palette,
) {
    let inner = usize::from(r.width.saturating_sub(2));
    let room = usize::from(r.width.saturating_sub(TEXT_X + 1));
    let base = if selected {
        p.selected().fg(p.fg)
    } else {
        Style::default().fg(p.fg).bg(p.bg_panel)
    };
    for dy in 0..2 {
        put(buf, r.x + 1, y + dy, &" ".repeat(inner), inner, base);
    }
    if selected {
        put(buf, r.x + MARKER_X, y, FOCUS, 1, base.fg(p.border_focus));
    }
    let title = truncate_end(&title_of(session), room);
    put(buf, r.x + TEXT_X, y, &title, room, base);
    let (lead, marks) = details(session, recent, now_ms);
    let line = fit_details(&lead, &marks, room);
    put(buf, r.x + TEXT_X, y + 1, &line, room, base.fg(p.fg_muted));
}

/// The separator, the hint, and `↓ more` when entries are below the window.
fn draw_foot(buf: &mut Buffer, r: Rect, searching: bool, more: bool, p: &Palette) {
    let w = usize::from(r.width);
    let sep = format!("├{}┤", "─".repeat(w.saturating_sub(2)));
    put(buf, r.x, r.bottom() - 3, &sep, w, fg(p.border_focus));
    let hint = if searching { HINT_SEARCHING } else { HINT };
    let muted = Style::default().fg(p.fg_muted).bg(p.bg_panel);
    let more_w = if more { width(MORE) + 2 } else { 0 };
    let room = usize::from(r.width.saturating_sub(TEXT_X)).saturating_sub(more_w);
    put(
        buf,
        r.x + MARKER_X,
        r.bottom() - 2,
        &truncate_end(hint, room),
        room,
        muted,
    );
    if more {
        let x = r
            .right()
            .saturating_sub(1 + u16::try_from(width(MORE)).unwrap_or(0) + 1);
        put(buf, x, r.bottom() - 2, MORE, width(MORE), muted);
    }
}

/// No sessions at all: what was looked for, and how to leave.
fn draw_empty(buf: &mut Buffer, area: Rect, s: &ResumePicker, p: &Palette) {
    let title = format!("Resume {}", s.agent);
    let r = frame_box(buf, area, WIDTH, CHROME_ROWS, &title, p);
    let room = usize::from(r.width.saturating_sub(TEXT_X + 1));
    let muted = Style::default().fg(p.fg_muted).bg(p.bg_panel);
    let line = format!("No {} sessions for {}", s.cli_label, s.dir_label);
    put(
        buf,
        r.x + MARKER_X,
        r.y + 2,
        &truncate_end(&line, room + 2),
        room + 2,
        Style::default().fg(p.fg).bg(p.bg_panel),
    );
    let w = usize::from(r.width);
    let sep = format!("├{}┤", "─".repeat(w.saturating_sub(2)));
    put(buf, r.x, r.bottom() - 3, &sep, w, fg(p.border_focus));
    put(buf, r.x + MARKER_X, r.bottom() - 2, EMPTY_HINT, room, muted);
}
