//! The org pane: the tree with state cells and roll-ups, Needs you pinned at its bottom.

use duohive_core::names::AgentName;
use duohive_core::org::NodeKey;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::{
    At, HEAVY, RenderCtx, draw_box, fg, glyph_color, needs_you, put, put_pieces, state_color,
};
use crate::format::{quoted, truncate_end, truncate_middle, width, wrap_words};
use crate::model::{UiModel, unknown_view};
use crate::theme::Palette;
use crate::tree::{TreeRow, TreeRowKind, agent_right, tree_rows};

/// The right column: the 16-cell state cell plus one blank, or 7 + 1 when compact.
const WIDE_RIGHT: usize = 17;
const COMPACT_RIGHT: usize = 8;
/// Needs you takes at most a third of the pane's inside, and always shows one row.
const NEEDS_YOU_SHARE: u16 = 3;
/// The age's cells at the end of a state cell.
const AGE_CELLS: usize = 4;

/// Where one tree row goes.
struct Slot {
    x: u16,
    y: u16,
    /// Content width: the inside less the padding cell on each side.
    width: usize,
    label_width: usize,
}

pub(super) fn draw(buf: &mut Buffer, r: Rect, m: &UiModel, ctx: &RenderCtx, compact: bool) {
    let p = &ctx.palette;
    let panel = Style::default().bg(p.bg_panel);
    draw_box(buf, r, "Org", &HEAVY, fg(p.border_focus), panel);
    if r.width < 6 || r.height < 4 {
        return;
    }
    let inside = r.height - 2;
    let shown = u16::try_from(m.needs_you.len()).unwrap_or(u16::MAX);
    let ny_rows = shown.min((inside / NEEDS_YOU_SHARE).max(1));
    let tree_h = inside.saturating_sub(ny_rows + 1);
    let sep_y = r.y + 1 + tree_h;
    needs_you::draw(
        buf,
        Rect {
            y: sep_y,
            height: ny_rows + 1,
            ..r
        },
        m,
        ctx,
        compact,
    );
    let content = usize::from(r.width - 4);
    let rows = wrap_problems(
        tree_rows(m, compact, ctx.settings.show_last_reply, ctx.now),
        content,
    );
    let right = if compact { COMPACT_RIGHT } else { WIDE_RIGHT };
    let skip = scroll(&rows, &m.selected, usize::from(tree_h));
    for (i, row) in rows.iter().skip(skip).take(usize::from(tree_h)).enumerate() {
        let y = r.y + 1 + u16::try_from(i).unwrap_or(u16::MAX);
        if row.key.as_ref() == Some(&m.selected) {
            let bar = Rect {
                x: r.x + 1,
                y,
                width: r.width - 2,
                height: 1,
            };
            buf.set_style(bar, Style::default().bg(p.selection));
            put(buf, r.x + 1, y, "▌", 1, fg(p.border_focus));
        }
        let slot = Slot {
            x: r.x + 2,
            y,
            width: content,
            label_width: content.saturating_sub(right),
        };
        draw_row(buf, &slot, row, m, ctx, compact);
    }
}

/// Errors and problems wrap onto more lines rather than lose their hint (UI §4.12: errors
/// show how to fix them); replies and notices are cut at the pane edge instead.
fn wrap_problems(rows: Vec<TreeRow>, content: usize) -> Vec<TreeRow> {
    rows.into_iter()
        .flat_map(|row| {
            if row.kind != TreeRowKind::Problem {
                return vec![row];
            }
            let room = content.saturating_sub(width(&row.prefix));
            wrap_words(&row.label, room)
                .into_iter()
                .map(|label| TreeRow {
                    label,
                    ..row.clone()
                })
                .collect()
        })
        .collect()
}

/// The first row to show, so the selected row and its detail line stay on screen.
fn scroll(rows: &[TreeRow], selected: &NodeKey, height: usize) -> usize {
    let at = rows
        .iter()
        .position(|r| r.key.as_ref() == Some(selected))
        .unwrap_or(0);
    (at + 2).saturating_sub(height)
}

fn draw_row(
    buf: &mut Buffer,
    s: &Slot,
    row: &TreeRow,
    m: &UiModel,
    ctx: &RenderCtx,
    compact: bool,
) {
    let p = &ctx.palette;
    let x = put(buf, s.x, s.y, &row.prefix, s.width, fg(p.fg_muted));
    let used = width(&row.prefix);
    let detail_room = s.width.saturating_sub(used);
    // Leave one blank cell before the right column.
    let label_room = s.label_width.saturating_sub(used + 1);
    match row.kind {
        TreeRowKind::Duo => {
            let name = m.org.duo.name.to_string();
            let head = format!("⬢ {name}");
            let x = put(buf, x, s.y, &head, label_room, fg(p.accent_duo));
            let rest = label_room.saturating_sub(width(&head));
            put(buf, x, s.y, &row.label[head.len()..], rest, fg(p.fg_muted));
        }
        TreeRowKind::Team | TreeRowKind::Role => {
            let (glyph, name) = row.label.split_at(row.label.find(' ').map_or(0, |i| i + 1));
            let x = put(buf, x, s.y, glyph, label_room, fg(p.fg_muted));
            let room = label_room.saturating_sub(width(glyph));
            put(buf, x, s.y, &truncate_middle(name, room), room, fg(p.fg));
        }
        TreeRowKind::Agent => {
            put(
                buf,
                x,
                s.y,
                &truncate_middle(&row.label, label_room),
                label_room,
                fg(p.fg),
            );
        }
        TreeRowKind::Reply => {
            let text = match row
                .label
                .strip_prefix('"')
                .and_then(|t| t.strip_suffix('"'))
            {
                Some(inner) => quoted(inner, detail_room),
                None => truncate_end(&row.label, detail_room),
            };
            put(buf, x, s.y, &text, detail_room, fg(p.fg_muted));
        }
        TreeRowKind::Problem => {
            put(
                buf,
                x,
                s.y,
                &truncate_end(&row.label, detail_room),
                detail_room,
                fg(p.error),
            );
        }
        TreeRowKind::EmptyRole | TreeRowKind::EmptyOrg => {
            put(
                buf,
                x,
                s.y,
                &truncate_end(&row.label, detail_room),
                detail_room,
                fg(p.fg_muted),
            );
        }
    }
    let at = At {
        x: s.x + u16::try_from(s.label_width).unwrap_or(u16::MAX),
        y: s.y,
        room: s.width.saturating_sub(s.label_width),
    };
    match (&row.kind, &row.key) {
        (TreeRowKind::Duo, _) => draw_state(buf, at, &m.org.duo.name, m, ctx, compact),
        (TreeRowKind::Agent, Some(NodeKey::Agent(a))) => draw_state(buf, at, a, m, ctx, compact),
        _ => draw_roll_up(buf, at, &row.right, &ctx.palette),
    }
}

/// The state cell, redrawn with the spinner: marker, glyph and word in the state's colour,
/// age muted.
fn draw_state(
    buf: &mut Buffer,
    at: At,
    agent: &AgentName,
    m: &UiModel,
    ctx: &RenderCtx,
    compact: bool,
) {
    let p = &ctx.palette;
    let status = m
        .agents
        .get(agent)
        .map_or_else(|| unknown_view().status, |v| v.status.clone());
    let cell: Vec<char> = agent_right(&status, ctx.now, compact, ctx.spinner())
        .chars()
        .collect();
    let (marker, rest) = cell.split_at(1.min(cell.len()));
    let (state, age) = rest.split_at(rest.len().saturating_sub(AGE_CELLS));
    let text = |chars: &[char]| chars.iter().collect::<String>();
    let parts = [
        (text(marker), p.fg),
        (text(state), state_color(p, &status)),
        (text(age), p.fg_muted),
    ];
    put_pieces(buf, at, parts);
}

/// A roll-up, each `glyph count` pair in its glyph's colour; the `~` marker in plain text.
fn draw_roll_up(buf: &mut Buffer, at: At, text: &str, p: &Palette) {
    let mut current = p.fg;
    let pieces = text.chars().map(|c| {
        if matches!(c, '●' | '◆' | '◷' | '✕') {
            current = glyph_color(p, c);
        }
        let color = if c == '~' { p.fg } else { current };
        (c.to_string(), color)
    });
    put_pieces(buf, at, pieces.collect::<Vec<_>>());
}
