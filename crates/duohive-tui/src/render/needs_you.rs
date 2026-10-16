//! Needs you, pinned at the bottom of the org pane (UI §4.1.3).

use duohive_core::event::NeedsYouItem;
use duohive_core::status::AgentState;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::{At, HEAVY, LIGHT, RenderCtx, edge, fg, glyph_color, put, put_pieces};
use crate::focus::Pane;
use crate::format::{QUIET_GLYPH, format_age, pad_right, quoted, truncate_middle, width};
use crate::model::UiModel;

/// Name cells beside the word when wide: the longest name, at least 12 (UI §4.1's mock)
/// and at most 20, never cutting harder than compact's 16. Longer names are cut in the
/// middle (UI §2.4) and the excerpt takes the rest.
const WIDE_NAME_MIN: usize = 12;
const WIDE_NAME_MAX: usize = 20;
const COMPACT_NAME: usize = 16;
/// The word is padded to 9 cells (D23) and the age right-aligned in 4.
const WORD_CELLS: usize = 9;
const AGE_CELLS: usize = 4;
/// Blank cells between the age and the excerpt.
const EXCERPT_GAP: &str = "  ";

/// Heavy below a light org pane: the separator when Needs you alone has focus.
const HEAVY_BELOW: [&str; 2] = ["┢", "┪"];

/// Draws the `┣━ Needs you (n) ━┫` separator at `r.y`, the rows under it and the org pane's
/// bottom border at `r.bottom() - 1`. With focus, this part's border is heavy (UI §2.2).
/// The Needs you item drawn on screen row `y` of `r`, as [`draw`] lays them out; not
/// the `… n more` row.
pub(super) fn item_at(r: Rect, m: &UiModel, y: u16) -> Option<usize> {
    if r.height < 2 || r.width < 2 || y <= r.y {
        return None;
    }
    let rows = usize::from(r.height.saturating_sub(2));
    let shown = if m.needs_you.len() > rows {
        rows.saturating_sub(1)
    } else {
        rows
    };
    let i = usize::from(y - r.y - 1);
    (i < shown.min(m.needs_you.len())).then_some(i)
}

pub(super) fn draw(buf: &mut Buffer, r: Rect, m: &UiModel, ctx: &RenderCtx, compact: bool) {
    let p = &ctx.palette;
    if r.height < 2 || r.width < 2 {
        return;
    }
    let (sep, lines, border) = match m.focus.pane {
        Pane::Org => (HEAVY.tee, &HEAVY, fg(p.border_focus)),
        Pane::NeedsYou => (HEAVY_BELOW, &HEAVY, fg(p.border_focus)),
        Pane::Agent | Pane::Chat => (LIGHT.tee, &LIGHT, fg(p.border)),
    };
    let focused = m.focus.pane == Pane::NeedsYou;
    if focused {
        let inside = Rect {
            y: r.y + 1,
            height: r.height - 1,
            ..r
        };
        buf.set_style(inside, Style::default().bg(p.bg_panel));
    }
    let title = format!("Needs you ({})", m.needs_you.len());
    edge(buf, r, sep, &title, lines, border);
    for y in r.y + 1..r.bottom() - 1 {
        put(buf, r.x, y, lines.v, 1, border);
        put(buf, r.right() - 1, y, lines.v, 1, border);
    }
    let bottom = Rect {
        y: r.bottom() - 1,
        height: 1,
        ..r
    };
    edge(buf, bottom, lines.bottom, "", lines, border);
    let content = usize::from(r.width.saturating_sub(4));
    let rows = usize::from(r.height.saturating_sub(2));
    let overflow = m.needs_you.len() > rows;
    // When the items don't fit, the last row says how many are left out.
    let shown = if overflow {
        rows.saturating_sub(1)
    } else {
        rows
    };
    if overflow {
        let more = format!("… {} more", m.needs_you.len() - shown);
        let y = r.y + 1 + u16::try_from(shown).unwrap_or(u16::MAX);
        let at = At {
            x: r.x + 2,
            y,
            room: content,
        };
        put_pieces(buf, at, [(more, p.fg_muted)]);
    }
    let name_cells = if compact {
        COMPACT_NAME
    } else {
        let longest = m
            .needs_you
            .iter()
            .take(shown)
            .map(|i| width(i.agent.as_str()));
        longest
            .max()
            .unwrap_or(0)
            .clamp(WIDE_NAME_MIN, WIDE_NAME_MAX)
    };
    for (i, item) in m.needs_you.iter().take(shown).enumerate() {
        let y = r.y + 1 + u16::try_from(i).unwrap_or(u16::MAX);
        if focused && i == m.focus.needs_you_cursor {
            let bar = Rect {
                x: r.x + 1,
                y,
                width: r.width - 2,
                height: 1,
            };
            buf.set_style(bar, Style::default().bg(p.selection));
            put(buf, r.x + 1, y, "▌", 1, fg(p.border_focus));
        }
        let (glyph, word) = if item.quiet {
            (QUIET_GLYPH, "quiet")
        } else {
            (item.state.glyph(), item.state.word())
        };
        let color = if item.state == AgentState::Errored && !item.quiet {
            p.error
        } else {
            glyph_color(p, glyph)
        };
        let name = pad_right(
            &truncate_middle(item.agent.as_str(), name_cells),
            name_cells,
        );
        let age = format!("{:>AGE_CELLS$}", format_age(ctx.now - item.since));
        let mut parts = vec![(format!("{glyph} "), color), (format!("{name} "), p.fg)];
        if !compact {
            parts.push((format!("{word:<WORD_CELLS$}"), color));
        }
        parts.push((age, p.fg_muted));
        let start = At {
            x: r.x + 2,
            y,
            room: content,
        };
        let at = put_pieces(buf, start, parts);
        if let Some(excerpt) = excerpt(item, compact, at.room) {
            put_pieces(buf, at, [(excerpt, p.fg_muted)]);
        }
    }
}

/// The quoted excerpt after the age, cut to `room` (wide layout only).
fn excerpt(item: &NeedsYouItem, compact: bool, room: usize) -> Option<String> {
    let text = item.excerpt.as_deref().filter(|_| !compact)?;
    let cells = room.checked_sub(EXCERPT_GAP.len())?;
    (cells > 2).then(|| format!("{EXCERPT_GAP}{}", quoted(text, cells)))
}
