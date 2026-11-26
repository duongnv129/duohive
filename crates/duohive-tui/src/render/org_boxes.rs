//! Drawing, scrolling, the wheel and hit-testing the org boxes (UI §4.1.5).
//!
//! Every line of the pane is one [`BoxLine`] and one screen row (OB-D1), so drawing, clicks,
//! scrolling and the wheel all read the same list.

use duohive_core::colour::TeamColour;
use duohive_core::names::NodeName;
use duohive_core::org::{NodeKey, RoleKey};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use super::manager_card::{self, chip_text};
use super::{RenderCtx, fg, glyph_color, org_pane, put};
use crate::boxes::{BoxLine, BoxLineKind as K, ROOT_BOX, TEAM_BOX, Tone, box_rows, detail_width};
use crate::focus::{AgentMode, Layout, Pane};
use crate::format::{truncate_end, truncate_middle, width};
use crate::model::UiModel;
use crate::theme::{Look, Palette};

/// Columns from the pane's left edge to a root box: the border and one pad.
const LEFT: u16 = 2;
/// A role box sits two columns inside its team box: the team's border and one pad.
const NEST: u16 = 2;
const FOLD: &str = " ▸";

pub(super) fn draw(buf: &mut Buffer, l: &Layout, m: &UiModel, ctx: &RenderCtx, replies: bool) {
    let Some((r, height)) = org_pane::draw_frame(buf, l, m, ctx) else {
        return;
    };
    let lines = box_rows(m, ctx.now, replies);
    let skip = top_row(&lines, m, height);
    let typed = typing_into(m);
    for (i, line) in lines.iter().skip(skip).take(height).enumerate() {
        let y = r.y + 1 + u16::try_from(i).unwrap_or(u16::MAX);
        let x = r.x + LEFT;
        if marks_drop_target(m, line) {
            // A valid target while dragging, marked by shape as well as colour (UI §4.4.2).
            put(buf, r.x + 1, y, "▹", 1, fg(ctx.palette.fg));
        }
        draw_line(buf, (x, y), line, m, ctx, typed);
    }
}

/// The node drawn on screen row `y`, as [`draw`] lays it out; a second line, a border and a
/// hint belong to the node they are about.
pub(super) fn key_at(l: &Layout, m: &UiModel, replies: bool, y: u16) -> Option<NodeKey> {
    let r = l.org;
    if r.width < 6 || r.height < 2 || y <= r.y {
        return None;
    }
    let height = usize::from(r.height - 1);
    let row = usize::from(y - r.y - 1);
    if row >= height {
        return None;
    }
    let lines = box_rows(m, m.now, replies);
    let skip = top_row(&lines, m, height);
    lines.get(skip + row)?.key.clone()
}

/// The list after one wheel tick: the top line moves by `lines`, the selection stays.
pub(super) fn wheel(l: &Layout, m: &mut UiModel, replies: bool, up: bool, lines: usize) {
    let r = l.org;
    if r.width < 6 || r.height < 2 {
        return;
    }
    let height = usize::from(r.height - 1);
    let rows = box_rows(m, m.now, replies);
    let top = top_row(&rows, m, height);
    let most = rows.len().saturating_sub(height);
    let next = if up {
        top.saturating_sub(lines)
    } else {
        (top + lines).min(most)
    };
    m.org_scroll = Some((next, m.selected.clone()));
}

/// The first line to show: where the wheel left the list while the same node stays selected,
/// else the one that keeps the selection, with its second line, on screen.
fn top_row(lines: &[BoxLine], m: &UiModel, height: usize) -> usize {
    let most = lines.len().saturating_sub(height);
    match &m.org_scroll {
        Some((top, key)) if *key == m.selected => (*top).min(most),
        _ => follow(lines, &m.selected, height).min(most),
    }
}

/// The first line that keeps every line of `selected` on screen.
fn follow(lines: &[BoxLine], selected: &NodeKey, height: usize) -> usize {
    let Some(first) = lines.iter().position(|l| l.key.as_ref() == Some(selected)) else {
        return 0;
    };
    // An agent is its row and its second line; Duo is the whole card; a box is its title.
    let tail = lines[first..]
        .iter()
        .take_while(|l| l.key.as_ref() == Some(selected) && l.kind != K::TeamBottom)
        .take_while(|l| !matches!(l.kind, K::RoleBottom | K::TeamBottom))
        .count()
        .max(1);
    (first + tail).saturating_sub(height).min(first)
}

/// Typing in the agent pane: the selected row is drawn solid (UI §4.1.5).
fn typing_into(m: &UiModel) -> bool {
    m.focus.pane == Pane::Agent && m.focus.agent_mode == AgentMode::Typing
}

fn marks_drop_target(m: &UiModel, line: &BoxLine) -> bool {
    let (Some(d), Some(key)) = (&m.drag, &line.key) else {
        return false;
    };
    matches!(
        line.kind,
        K::ManagerTop | K::TeamTop | K::RoleTop | K::Agent
    ) && crate::input::drop_valid(m, &d.node, key)
}

fn draw_line(
    buf: &mut Buffer,
    at: (u16, u16),
    line: &BoxLine,
    m: &UiModel,
    ctx: &RenderCtx,
    typed: bool,
) {
    let (x, y) = at;
    match line.kind {
        K::ManagerTop | K::ManagerName | K::ManagerCounts | K::ManagerBottom => {
            manager_card::draw(buf, at, line, m, ctx, typed && line.selected);
        }
        K::TeamTop => top_rule(buf, at, ROOT_BOX, line, ('┌', '┐'), true, ctx),
        K::RoleTop if !line.in_team => top_rule(buf, at, ROOT_BOX, line, ('┌', '┐'), true, ctx),
        K::RoleTop => {
            team_sides(buf, at, line, m, ctx);
            top_rule(buf, (x + NEST, y), TEAM_BOX, line, ('╭', '╮'), false, ctx);
        }
        K::TeamBottom => bottom_rule(
            buf,
            at,
            ROOT_BOX,
            ('└', '┘'),
            edge_colour(&ctx.palette, line.hue, line.selected),
        ),
        K::RoleBottom if !line.in_team => {
            bottom_rule(
                buf,
                at,
                ROOT_BOX,
                ('└', '┘'),
                edge_colour(&ctx.palette, line.hue, line.selected),
            );
        }
        K::RoleBottom => {
            team_sides(buf, at, line, m, ctx);
            bottom_rule(
                buf,
                (x + NEST, y),
                TEAM_BOX,
                ('╰', '╯'),
                edge_colour(&ctx.palette, line.hue, line.selected),
            );
        }
        K::Agent | K::Detail | K::EmptyRole => body(buf, at, line, m, ctx, typed),
        K::FilterEmpty => {
            let text = truncate_end(&line.label, ROOT_BOX);
            put(buf, x, y, &text, ROOT_BOX, fg(ctx.palette.fg_muted));
        }
    }
}

/// The border colour of a box in `hue`: the hue at 65% over `bg`, at 100% when selected.
fn edge_colour(p: &Palette, hue: Option<TeamColour>, selected: bool) -> Color {
    match hue {
        Some(c) if selected => p.hue(c),
        Some(c) => p.tint(c, 65),
        None => p.border,
    }
}

/// The team box's two side borders on a line inside it, in the team's border colour.
fn team_sides(buf: &mut Buffer, (x, y): (u16, u16), line: &BoxLine, m: &UiModel, ctx: &RenderCtx) {
    let selected = enclosing_team(m, line).is_some_and(|t| m.selected == NodeKey::Team(t));
    let style = fg(edge_colour(&ctx.palette, line.hue, selected));
    put(buf, x, y, "│", 1, style);
    put(
        buf,
        x + u16::try_from(ROOT_BOX - 1).unwrap_or(0),
        y,
        "│",
        1,
        style,
    );
}

/// The team a line inside a team box belongs to.
fn enclosing_team(m: &UiModel, line: &BoxLine) -> Option<NodeName> {
    match line.key.as_ref()? {
        NodeKey::Team(t) => Some(t.clone()),
        NodeKey::Role(RoleKey { team, .. }) => team.clone(),
        NodeKey::Agent(a) => m.org.find_agent(a)?.team.map(|t| t.name.clone()),
        NodeKey::Duo => None,
    }
}

/// The role a body line belongs to.
fn enclosing_role(m: &UiModel, line: &BoxLine) -> Option<NodeKey> {
    match line.key.as_ref()? {
        k @ NodeKey::Role(_) => Some(k.clone()),
        NodeKey::Agent(a) => {
            let loc = m.org.find_agent(a)?;
            Some(NodeKey::Role(RoleKey {
                team: loc.team.map(|t| t.name.clone()),
                role: loc.role.name.clone(),
            }))
        }
        _ => None,
    }
}

/// How a title is drawn: the hue, bold; selected, a chip (the hue as the background).
fn title_style(p: &Palette, hue: Option<TeamColour>, selected: bool) -> Style {
    let Some(c) = hue else {
        return fg(p.fg).add_modifier(Modifier::BOLD);
    };
    if !selected {
        return fg(p.hue(c)).add_modifier(Modifier::BOLD);
    }
    match p.look {
        Look::Dark | Look::Light => fg(chip_text(p)).bg(p.hue(c)).add_modifier(Modifier::BOLD),
        Look::Auto | Look::Mono => fg(p.hue(c)).add_modifier(Modifier::REVERSED | Modifier::BOLD),
    }
}

/// A box's top rule with its title, and its roll-up at the right when it has one. A team's and
/// a top-level role's title sits at the left (`┌─ ▣ Payments ──── ● 1 ◆ 1 ─┐`); a role inside a
/// team is centred (`╭──── ◇ Backend ────╮`).
fn top_rule(
    buf: &mut Buffer,
    (x, y): (u16, u16),
    w: usize,
    line: &BoxLine,
    corners: (char, char),
    left: bool,
    ctx: &RenderCtx,
) {
    let p = &ctx.palette;
    let edge = fg(edge_colour(p, line.hue, line.selected));
    let title = title_style(p, line.hue, line.selected);
    let (glyph, name) = line
        .label
        .split_at(line.label.find(' ').map_or(0, |i| i + 1));
    let fold = if line.folded { FOLD } else { "" };
    let right = &line.right;
    let right_w = if right.is_empty() {
        0
    } else {
        width(right) + 4
    };
    let name_room = w
        .saturating_sub(if left { 5 } else { 6 } + right_w + width(glyph) + width(fold))
        .max(1);
    let name = truncate_middle(name, name_room);
    let text = format!(" {glyph}{name}{fold} ");
    let mut cx = x;
    cx = put(buf, cx, y, &corners.0.to_string(), 1, edge);
    let lead = if left { 1 } else { (w - 2 - width(&text)) / 2 };
    cx = put(buf, cx, y, &"─".repeat(lead), lead, edge);
    cx = put(buf, cx, y, &text, width(&text), title);
    let tail = w.saturating_sub(1 + lead + width(&text) + 1);
    if right.is_empty() {
        put(
            buf,
            cx,
            y,
            &format!("{}{}", "─".repeat(tail), corners.1),
            tail + 1,
            edge,
        );
        return;
    }
    let fill = tail.saturating_sub(right_w - 1);
    cx = put(buf, cx, y, &"─".repeat(fill), fill, edge);
    cx = put(buf, cx, y, " ", 1, edge);
    roll_up(buf, cx, y, right, p);
    let end = x + u16::try_from(w - 3).unwrap_or(0);
    put(buf, end, y, &format!(" ─{}", corners.1), 3, edge);
}

/// A roll-up, each `glyph count` pair in its glyph's colour; the `~` in plain text.
fn roll_up(buf: &mut Buffer, x: u16, y: u16, text: &str, p: &Palette) {
    let mut current = p.fg;
    let mut cx = x;
    for c in text.chars() {
        if matches!(c, '●' | '◆' | '◷' | '✕') {
            current = glyph_color(p, c);
        }
        let colour = if c == '~' { p.fg } else { current };
        cx = put(buf, cx, y, &c.to_string(), 1, fg(colour));
    }
}

fn bottom_rule(
    buf: &mut Buffer,
    (x, y): (u16, u16),
    w: usize,
    corners: (char, char),
    colour: Color,
) {
    let rule = format!("{}{}{}", corners.0, "─".repeat(w - 2), corners.1);
    put(buf, x, y, &rule, w, fg(colour));
}

/// An agent row, its second line, or an empty role's hint, inside its role box.
fn body(
    buf: &mut Buffer,
    (x, y): (u16, u16),
    line: &BoxLine,
    m: &UiModel,
    ctx: &RenderCtx,
    typed: bool,
) {
    let p = &ctx.palette;
    let (box_x, w) = if line.in_team {
        team_sides(buf, (x, y), line, m, ctx);
        (x + NEST, TEAM_BOX)
    } else {
        (x, ROOT_BOX)
    };
    let role_selected = enclosing_role(m, line).is_some_and(|k| m.selected == k);
    let edge = fg(edge_colour(p, line.hue, role_selected));
    put(buf, box_x, y, "│", 1, edge);
    put(
        buf,
        box_x + u16::try_from(w - 1).unwrap_or(0),
        y,
        "│",
        1,
        edge,
    );
    let inner = Rect::new(box_x + 1, y, u16::try_from(w - 2).unwrap_or(0), 1);
    let room = w - 2;
    let row = RowStyle::of(p, line, typed && line.selected);
    if let Some(fill) = row.fill {
        buf.set_style(inner, fill);
    }
    match line.kind {
        K::Agent => agent_row(buf, inner, room, line, &row, p),
        K::Detail => {
            let style = match (line.tone, line.selected) {
                (_, true) => row.text,
                (Tone::Problem, false) => row.text.fg(p.error),
                (Tone::Notice | Tone::Plain, false) => row.text.fg(p.fg_muted),
            };
            let text = truncate_end(&line.label, detail_width(line.in_team));
            put(buf, inner.x + 2, y, &text, room - 2, style);
        }
        _ => {
            let text = truncate_end(&line.label, detail_width(line.in_team));
            put(
                buf,
                inner.x + 2,
                y,
                &text,
                room - 2,
                row.text.fg(p.fg_muted),
            );
        }
    }
}

/// The colours of an agent's lines: the wash or solid fill, and the text that sits on it.
struct RowStyle {
    fill: Option<Style>,
    /// Plain text on the row.
    text: Style,
    /// The name: bold, white on `dark` when selected.
    name: Style,
    /// The `▌` bar.
    bar: Style,
}

impl RowStyle {
    fn of(p: &Palette, line: &BoxLine, typed: bool) -> Self {
        let hue = line.hue;
        let paints = matches!(p.look, Look::Dark | Look::Light);
        let plain = Style::default().fg(p.fg);
        if !line.selected {
            return Self {
                fill: None,
                text: plain,
                name: plain,
                bar: plain,
            };
        }
        if typed {
            // The whole row solid in its hue with `bg` text; reverse video without colour.
            let solid = match (hue, paints) {
                (Some(c), true) => Style::default().fg(p.bg).bg(p.hue(c)),
                (Some(c), false) => fg(p.hue(c)).add_modifier(Modifier::REVERSED),
                (None, _) => Style::default().add_modifier(Modifier::REVERSED),
            };
            return Self {
                fill: Some(solid),
                text: solid,
                name: solid.add_modifier(Modifier::BOLD),
                bar: solid,
            };
        }
        let wash = hue
            .filter(|_| paints)
            .map(|c| Style::default().bg(p.tint(c, 30)));
        let on = |s: Style| wash.map_or(s, |w| s.patch(w));
        Self {
            fill: wash,
            text: on(plain),
            name: on(fg(p.emphasis()).add_modifier(Modifier::BOLD)),
            bar: on(fg(hue.map_or(p.fg, |c| p.hue(c)))),
        }
    }
}

/// One agent row: the marker column (`▌` when selected), the name, `✉ n`, and the icon at the
/// right edge with a pad before the border.
fn agent_row(
    buf: &mut Buffer,
    inner: Rect,
    room: usize,
    line: &BoxLine,
    row: &RowStyle,
    p: &Palette,
) {
    let y = inner.y;
    if line.selected {
        put(buf, inner.x, y, "▌", 1, row.bar);
    }
    let mut cx = put(buf, inner.x + 1, y, &line.label, room, row.name_for(line));
    if line.inbox > 0 {
        cx = put(buf, cx, y, &format!(" ✉ {}", line.inbox), room, row.text);
    }
    let _ = cx; // nothing follows the envelope
    let icon_w = width(&line.right);
    let at = inner.x + u16::try_from(room.saturating_sub(1 + icon_w)).unwrap_or(0);
    let colour = if line.selected {
        row.text
    } else {
        let glyph = line.right.chars().last().unwrap_or(' ');
        fg(icon_colour(p, glyph))
    };
    let tilde = line.right.starts_with('~');
    if tilde {
        put(
            buf,
            at,
            y,
            "~",
            1,
            if line.selected { row.text } else { fg(p.fg) },
        );
        put(buf, at + 1, y, &line.right[1..], 1, colour);
    } else {
        put(buf, at, y, &line.right, icon_w, colour);
    }
}

impl RowStyle {
    /// The name's style: bold on a selected row, plain otherwise.
    fn name_for(&self, line: &BoxLine) -> Style {
        if line.selected { self.name } else { self.text }
    }
}

/// The colour of a row's state icon: the roll-up colours for the glyphs that need you,
/// `status.done` for finished and `fg.muted` for the quiet states (UI §2.1).
fn icon_colour(p: &Palette, glyph: char) -> Color {
    match glyph {
        '✓' => p.done,
        '○' | '◦' | '◌' => p.fg_muted,
        g => glyph_color(p, g),
    }
}
