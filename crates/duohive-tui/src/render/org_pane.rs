//! The org pane: the tree with state cells and roll-ups, Needs you pinned at its bottom.

use duohive_core::names::AgentName;
use duohive_core::org::{NodeKey, Org};
use duohive_core::settings::OrgLayout;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use super::{
    At, RenderCtx, draw_box, fg, focus_title, glyph_color, needs_you, pane_style, put, put_pieces,
    state_color,
};
use crate::focus::{Layout, Pane};
use crate::format::{quoted, truncate_end, truncate_middle, width, wrap_capped};
use crate::model::UiModel;
use crate::theme::{Look, Palette};
use crate::tree::{TreeRow, TreeRowKind, agent_right, tree_rows};

/// The right column: the 16-cell state cell plus one blank, or 7 + 1 when compact.
const WIDE_RIGHT: usize = 17;
const COMPACT_RIGHT: usize = 8;
/// An error or problem wraps onto at most this many lines.
const MAX_PROBLEM_LINES: usize = 3;
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

/// The layout actually drawn: the boxes only when chosen and the screen is not compact
/// (UI §3); every other case is the tree.
pub(crate) fn drawn_layout(setting: OrgLayout, compact: bool) -> OrgLayout {
    if setting == OrgLayout::Boxes && !compact {
        OrgLayout::Boxes
    } else {
        OrgLayout::Tree
    }
}

/// Draws the org pane in the layout [`drawn_layout`] picks. Until the boxes land (OB.6) the
/// `Boxes` arm draws the tree.
pub(super) fn draw(buf: &mut Buffer, l: &Layout, m: &UiModel, ctx: &RenderCtx, replies: bool) {
    match drawn_layout(ctx.settings.org_layout, l.compact) {
        OrgLayout::Tree | OrgLayout::Boxes => draw_tree(buf, l, m, ctx, replies),
    }
}

fn draw_tree(buf: &mut Buffer, l: &Layout, m: &UiModel, ctx: &RenderCtx, replies: bool) {
    let p = &ctx.palette;
    let compact = l.compact;
    let r = Rect {
        height: l.org.height + l.needs_you.height,
        ..l.org
    };
    let (lines, border, inside) = pane_style(p, m.focus.pane == Pane::Org);
    draw_box(buf, r, "Org", lines, border, inside);
    if m.focus.pane == Pane::Org {
        focus_title(buf, r, "Org", p);
    }
    needs_you::draw(buf, l.needs_you, m, ctx, compact);
    if r.width < 6 || l.org.height < 2 {
        return;
    }
    let tree_h = l.org.height - 1;
    let content = usize::from(r.width - 4);
    let rows = wrap_problems(tree_rows(m, compact, replies, ctx.now), content);
    let right = if compact { COMPACT_RIGHT } else { WIDE_RIGHT };
    let skip = top_row(&rows, m, usize::from(tree_h));
    for (i, row) in rows.iter().skip(skip).take(usize::from(tree_h)).enumerate() {
        let y = r.y + 1 + u16::try_from(i).unwrap_or(u16::MAX);
        if let (Some(d), Some(key)) = (&m.drag, &row.key) {
            // A valid target while dragging: marked in the bar column too, not by colour alone.
            if crate::input::drop_valid(m, &d.node, key) {
                let bar = Rect {
                    x: r.x + 1,
                    y,
                    width: r.width - 2,
                    height: 1,
                };
                buf.set_style(bar, p.selected());
                put(buf, r.x + 1, y, "▹", 1, fg(p.fg));
            }
        }
        let selected = row.key.as_ref() == Some(&m.selected);
        if let (true, Some(key)) = (selected, &row.key) {
            let (bar_colour, wash) = selection_colours(p, &m.org, key);
            let bar = Rect {
                x: r.x + 1,
                y,
                width: r.width - 2,
                height: 1,
            };
            if let Some(wash) = wash {
                buf.set_style(bar, Style::default().bg(wash));
            }
            put(buf, r.x + 1, y, "▌", 1, fg(bar_colour));
        }
        let slot = Slot {
            x: r.x + 2,
            y,
            width: content,
            label_width: content.saturating_sub(right),
        };
        draw_row(buf, &slot, row, m, ctx, (compact, selected));
    }
}

/// The `▌` bar's colour and the wash behind a selected tree row (UI §4.1.6): the node's hue
/// and the hue at 30% over `bg`; Duo's `manager` colour and 26%. Under `auto` and `NO_COLOR`
/// no background is painted, so there is no wash.
fn selection_colours(p: &Palette, org: &Org, key: &NodeKey) -> (Color, Option<Color>) {
    let paints = matches!(p.look, Look::Dark | Look::Light);
    match (key, org.colour_of(key)) {
        (NodeKey::Duo, _) | (_, None) => (p.manager, paints.then(|| p.tint_manager(26))),
        (_, Some(c)) => (p.hue(c), paints.then(|| p.tint(c, 30))),
    }
}

/// The node drawn on screen row `y` of the tree, as [`draw`] lays it out; a detail line
/// (a reply, a problem) belongs to the node above it.
pub(super) fn key_at(l: &Layout, m: &UiModel, replies: bool, y: u16) -> Option<NodeKey> {
    match drawn_layout(m.settings.org_layout, l.compact) {
        OrgLayout::Tree | OrgLayout::Boxes => key_at_tree(l, m, replies, y),
    }
}

fn key_at_tree(l: &Layout, m: &UiModel, replies: bool, y: u16) -> Option<NodeKey> {
    let r = l.org;
    if r.width < 6 || r.height < 2 || y <= r.y {
        return None;
    }
    let tree_h = usize::from(r.height - 1);
    let row = usize::from(y - r.y - 1);
    if row >= tree_h {
        return None;
    }
    let content = usize::from(r.width - 4);
    let rows = wrap_problems(tree_rows(m, l.compact, replies, m.now), content);
    let skip = top_row(&rows, m, tree_h);
    rows.iter()
        .take(skip + row + 1)
        .skip(skip)
        .rev()
        .find_map(|r| r.key.clone())
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
            wrap_capped(&row.label, room, MAX_PROBLEM_LINES)
                .into_iter()
                .map(|label| TreeRow {
                    label,
                    ..row.clone()
                })
                .collect()
        })
        .collect()
}

/// The first row to show: where the wheel left the tree while the same node stays selected,
/// else the one that keeps the selection on screen.
fn top_row(rows: &[TreeRow], m: &UiModel, height: usize) -> usize {
    match &m.org_scroll {
        Some((top, key)) if *key == m.selected => (*top).min(rows.len().saturating_sub(height)),
        _ => scroll(rows, &m.selected, height),
    }
}

/// The tree after one wheel tick: the top row moves by `lines`, the selection stays.
pub(super) fn wheel(l: &Layout, m: &mut UiModel, replies: bool, up: bool, lines: usize) {
    match drawn_layout(m.settings.org_layout, l.compact) {
        OrgLayout::Tree | OrgLayout::Boxes => wheel_tree(l, m, replies, up, lines),
    }
}

fn wheel_tree(l: &Layout, m: &mut UiModel, replies: bool, up: bool, lines: usize) {
    let r = l.org;
    if r.width < 6 || r.height < 2 {
        return;
    }
    let height = usize::from(r.height - 1);
    let content = usize::from(r.width - 4);
    let rows = wrap_problems(tree_rows(m, l.compact, replies, m.now), content);
    let top = top_row(&rows, m, height);
    let most = rows.len().saturating_sub(height);
    let next = if up {
        top.saturating_sub(lines)
    } else {
        (top + lines).min(most)
    };
    m.org_scroll = Some((next, m.selected.clone()));
}

/// The first row to show, so the selected row and all its detail lines stay on screen.
fn scroll(rows: &[TreeRow], selected: &NodeKey, height: usize) -> usize {
    let at = rows
        .iter()
        .position(|r| r.key.as_ref() == Some(selected))
        .unwrap_or(0);
    let details = rows
        .iter()
        .skip(at + 1)
        .take_while(|r| r.key.is_none())
        .count();
    (at + 1 + details).saturating_sub(height).min(at)
}

fn draw_row(
    buf: &mut Buffer,
    s: &Slot,
    row: &TreeRow,
    m: &UiModel,
    ctx: &RenderCtx,
    (compact, selected): (bool, bool),
) {
    let p = &ctx.palette;
    // On the selected row every text is `fg`, which holds 4.5:1 on the wash (UI §6).
    let quiet = if selected { p.fg } else { p.fg_muted };
    let x = put(buf, s.x, s.y, &row.prefix, s.width, fg(p.fg_muted));
    let used = width(&row.prefix);
    let detail_room = s.width.saturating_sub(used);
    // Leave one blank cell before the right column.
    let label_room = s.label_width.saturating_sub(used + 1);
    match row.kind {
        TreeRowKind::Duo => {
            let name = m.org.duo.name.to_string();
            let head = format!("⬢ {name}");
            let x = put(
                buf,
                x,
                s.y,
                &head,
                label_room,
                fg(p.accent_duo).add_modifier(Modifier::BOLD),
            );
            let rest = label_room.saturating_sub(width(&head));
            let tail = row.label.strip_prefix(head.as_str()).unwrap_or_default();
            put(buf, x, s.y, tail, rest, fg(quiet));
        }
        TreeRowKind::Team | TreeRowKind::Role => {
            let colours = GroupColours::of(row, m, p, selected);
            draw_group(buf, (x, s.y), &row.label, label_room, &colours);
        }
        TreeRowKind::Agent => {
            let inbox = envelope(row.inbox, compact);
            let name_room = label_room.saturating_sub(width(&inbox));
            let name = truncate_middle(&row.label, name_room);
            let name_style = if selected {
                fg(p.emphasis()).add_modifier(Modifier::BOLD)
            } else {
                fg(p.fg)
            };
            let x = put(buf, x, s.y, &name, name_room, name_style);
            put(buf, x, s.y, &inbox, label_room - width(&name), fg(p.fg));
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
        TreeRowKind::EmptyRole | TreeRowKind::EmptyOrg | TreeRowKind::FilterEmpty => {
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
        (TreeRowKind::Duo, _) => draw_state(buf, at, &m.org.duo.name, m, ctx, (compact, selected)),
        (TreeRowKind::Agent, Some(NodeKey::Agent(a))) => {
            draw_state(buf, at, a, m, ctx, (compact, selected));
        }
        _ => draw_roll_up(buf, at, &row.right, &ctx.palette, selected),
    }
}

/// The colours of a team's or role's glyph and name (UI §2.3, §4.1.6).
struct GroupColours {
    glyph: Style,
    name: Style,
}

impl GroupColours {
    /// Not selected: the glyph and name in the node's hue, bold. Selected: the glyph in `fg`
    /// and the name bold, white on `dark`, so the hue stays in the bar and the wash.
    fn of(row: &TreeRow, m: &UiModel, p: &Palette, selected: bool) -> Self {
        let bold = Modifier::BOLD;
        if selected {
            return Self {
                glyph: fg(p.fg),
                name: fg(p.emphasis()).add_modifier(bold),
            };
        }
        let hue = row
            .key
            .as_ref()
            .and_then(|k| m.org.colour_of(k))
            .map_or(p.fg, |c| p.hue(c));
        Self {
            glyph: fg(hue).add_modifier(bold),
            name: fg(hue).add_modifier(bold),
        }
    }
}

/// A team or role row's label: the glyph, then the name cut in the middle to fit.
fn draw_group(buf: &mut Buffer, (x, y): (u16, u16), label: &str, room: usize, c: &GroupColours) {
    let (glyph, name) = label.split_at(label.find(' ').map_or(0, |i| i + 1));
    let x = put(buf, x, y, glyph, room, c.glyph);
    let left = room.saturating_sub(width(glyph));
    put(buf, x, y, &truncate_middle(name, left), left, c.name);
}

/// ` ✉ 1` after an agent's name, ` ✉1` in the compact tree (UI §4.1); empty with none.
fn envelope(waiting: u32, compact: bool) -> String {
    match (waiting, compact) {
        (0, _) => String::new(),
        (n, true) => format!(" ✉{n}"),
        (n, false) => format!(" ✉ {n}"),
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
    (compact, selected): (bool, bool),
) {
    let p = &ctx.palette;
    let status = &m.view(agent).status;
    let cell: Vec<char> = agent_right(status, ctx.now, compact, ctx.spinner())
        .chars()
        .collect();
    let (marker, rest) = cell.split_at(1.min(cell.len()));
    let (state, age) = rest.split_at(rest.len().saturating_sub(AGE_CELLS));
    let text = |chars: &[char]| chars.iter().collect::<String>();
    let colour = if selected {
        p.fg
    } else {
        state_color(p, status)
    };
    // Under `auto` only the glyph carries the state's colour; the word is muted.
    let (glyph, word) = state.split_at(if p.glyph_only { 1.min(state.len()) } else { 0 });
    let word_colour = if p.glyph_only && !selected {
        p.fg_muted
    } else {
        colour
    };
    let parts = [
        (text(marker), p.fg),
        (text(glyph), colour),
        (text(word), word_colour),
        (text(age), if selected { p.fg } else { p.fg_muted }),
    ];
    put_pieces(buf, at, parts);
}

/// A roll-up, each `glyph count` pair in its glyph's colour; the `~` marker in plain text.
fn draw_roll_up(buf: &mut Buffer, at: At, text: &str, p: &Palette, selected: bool) {
    let mut current = p.fg;
    let pieces = text.chars().map(|c| {
        if matches!(c, '●' | '◆' | '◷' | '✕') && !selected {
            current = glyph_color(p, c);
        }
        let color = if c == '~' { p.fg } else { current };
        (c.to_string(), color)
    });
    put_pieces(buf, at, pieces.collect::<Vec<_>>());
}
