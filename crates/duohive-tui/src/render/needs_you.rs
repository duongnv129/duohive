//! Needs you, pinned at the bottom of the org pane (UI §4.1.3).

use duohive_core::event::NeedsYouItem;
use duohive_core::status::AgentState;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::{At, HEAVY, RenderCtx, edge, fg, glyph_color, put_pieces};
use crate::format::{QUIET_GLYPH, format_age, pad_right, quoted, truncate_middle};
use crate::model::UiModel;

/// Name cells: 12 beside the word when wide, 16 when compact.
const WIDE_NAME: usize = 12;
const COMPACT_NAME: usize = 16;
/// The word is padded to 9 cells (D23) and the age right-aligned in 4.
const WORD_CELLS: usize = 9;
const AGE_CELLS: usize = 4;
/// Blank cells between the age and the excerpt.
const EXCERPT_GAP: &str = "  ";

/// Draws the `┣━ Needs you (n) ━┫` separator at `r.y` and up to `r.height - 1` rows under it.
pub(super) fn draw(buf: &mut Buffer, r: Rect, m: &UiModel, ctx: &RenderCtx, compact: bool) {
    let p = &ctx.palette;
    let title = format!("Needs you ({})", m.needs_you.len());
    edge(buf, r, HEAVY.tee, &title, &HEAVY, fg(p.border_focus));
    let content = usize::from(r.width.saturating_sub(4));
    let rows = usize::from(r.height.saturating_sub(1));
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
    for (i, item) in m.needs_you.iter().take(shown).enumerate() {
        let y = r.y + 1 + u16::try_from(i).unwrap_or(u16::MAX);
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
        let name_cells = if compact { COMPACT_NAME } else { WIDE_NAME };
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
