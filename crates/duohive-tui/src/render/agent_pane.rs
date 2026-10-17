//! The agent pane: the selected agent's live terminal, or the selected node's summary
//! (UI §4.1.2, P2-D3).

use duohive_core::cli_kind::CliKind;
use duohive_core::inherit::{inherited_for, resolve};
use duohive_core::names::AgentName;
use duohive_core::org::{Fields, NodeKey, RoleKey};
use duohive_core::status::{AgentState, WaitingOn};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use super::{LiveScreen, RenderCtx, draw_box, fg, focus_title, pane_style, put};
use crate::focus::Pane;
use crate::format::{
    marker, one_line, status_glyph, status_word, truncate_end, truncate_middle, width,
};
use crate::model::UiModel;
use crate::theme::Palette;
use crate::tree::roll_up_counts;

const EMPTY_ORG: &str = "Your org is empty · n to add a team, role or agent";
const STOPPED: &str = "○ stopped · space to give work";
const ERRORED: &str = "✕ errored · s to stop";
/// The padding cell inside the border (UI §3).
const PAD: &str = " ";
/// Indent of a one-line message, after the padding cell.
const MESSAGE_INDENT: &str = "  ";
const NONE: &str = "(none)";
/// Separates the parts of a title.
const SEP: &str = " · ";
/// The cells the top edge takes around the title inside the corners: `━ ` and ` ━`.
const TITLE_FRAME: usize = 3;
/// The fewest cells of a cut worktree or directory worth showing, `…` included.
const MIN_PLACE: usize = 4;
/// The label column of a team or role summary.
const LABEL_CELLS: usize = 10;
const STATE_WORDS: [&str; 4] = ["working", "waiting", "quiet", "errored"];
/// The rows a full team or role summary takes with its defaults on one line.
const SUMMARY_ROWS: usize = 6;
/// How to answer Claude's folder-trust dialog, whose highlighted choice is `No, exit`
/// (P2-D23): the longest that fits the bottom edge.
const TRUST_HINTS: [&str; 2] = [
    "Claude asks to trust this folder · choose Yes (↓ then enter) · enter alone exits",
    "To trust: ↓ then enter · enter alone exits",
];
/// The cells the bottom edge keeps around a hint: the corners, one `─` and a space each side.
const HINT_FRAME: usize = 5;

pub(super) fn draw(
    buf: &mut Buffer,
    r: Rect,
    m: &UiModel,
    ctx: &RenderCtx,
    live: Option<LiveScreen<'_>>,
) {
    let p = &ctx.palette;
    let cells = usize::from(r.width.saturating_sub(2));
    let rows = usize::from(r.height.saturating_sub(2));
    let (title, lines) = content(m, ctx, cells, rows);
    let (border_lines, border, inside) = pane_style(p, m.focus.pane == Pane::Agent);
    draw_box(buf, r, &title, border_lines, border, inside);
    if m.focus.pane == Pane::Agent {
        focus_title(buf, r, &title, p);
    }
    if r.width < 5 || r.height < 3 {
        return;
    }
    let room = usize::from(r.width).saturating_sub(HINT_FRAME);
    if let Some(hint) = waiting_hint(m, room) {
        let at = r.x + 2;
        let text = format!(" {hint} ");
        put(buf, at, r.bottom() - 1, &text, room + 2, fg(p.attention));
    }
    if let Some(s) = live.filter(|s| shown_agent(m) == Some(s.agent)) {
        let cursor = m.typing() && !m.scrolled;
        s.term
            .render_from(live_area(r), buf, s.first_row, cursor, s.no_color);
        return;
    }
    // From the border's inside; each line starts with `PAD` itself.
    let inner = Rect {
        x: r.x + 1,
        y: r.y + 1,
        width: r.width - 2,
        height: r.height - 2,
    };
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

/// Where the live terminal goes in the pane `r`: inside the border and the padding cell
/// on each side (UI §3).
pub(super) fn live_area(r: Rect) -> Rect {
    Rect {
        x: r.x.saturating_add(2),
        y: r.y.saturating_add(1),
        width: r.width.saturating_sub(4),
        height: r.height.saturating_sub(2),
    }
}

/// How to answer what the shown agent waits on, fitted to `room` cells (P2-D23). It
/// goes in the pane's bottom edge, so the live terminal keeps every row.
fn waiting_hint(m: &UiModel, room: usize) -> Option<String> {
    let status = &m.view(shown_agent(m)?).status;
    if status.state != AgentState::Waiting {
        return None;
    }
    let hints = match status.waiting_on? {
        WaitingOn::TrustFolder => &TRUST_HINTS,
    };
    let hint = hints.iter().find(|h| width(h) <= room).or(hints.last())?;
    Some(truncate_end(hint, room))
}

/// The agent whose pane is shown: none while reconnecting, with an empty org, or with a
/// team or role selected.
fn shown_agent(m: &UiModel) -> Option<&AgentName> {
    let empty = m.org.teams.is_empty() && m.org.roles.is_empty();
    if m.reconnecting > 0 || empty {
        return None;
    }
    m.selected_agent()
}

/// The pane's title and body for the current state and selection.
fn content(
    m: &UiModel,
    ctx: &RenderCtx,
    cells: usize,
    rows: usize,
) -> (String, Vec<Line<'static>>) {
    let p = &ctx.palette;
    if m.reconnecting > 0 {
        let noun = if m.reconnecting == 1 {
            "agent"
        } else {
            "agents"
        };
        let text = format!("Reconnecting to {} {noun}…", m.reconnecting);
        return (String::new(), message(&text, p.fg));
    }
    if m.org.teams.is_empty() && m.org.roles.is_empty() {
        return (String::new(), message(EMPTY_ORG, p.fg));
    }
    match &m.selected {
        NodeKey::Duo => agent(m, ctx, &m.org.duo.name, cells),
        NodeKey::Agent(a) => agent(m, ctx, a, cells),
        NodeKey::Team(t) => {
            let Some(team) = m.org.teams.iter().find(|x| &x.name == t) else {
                return (String::new(), Vec::new());
            };
            let names: Vec<&AgentName> = team
                .roles
                .iter()
                .flat_map(|r| r.agents.iter().map(|a| &a.name))
                .collect();
            let title = format!("▣ {t}");
            (
                title,
                summary(m, p, &names, &team.defaults, &NodeKey::Duo, cells, rows),
            )
        }
        NodeKey::Role(key) => role(m, p, key, cells, rows),
    }
}

fn role(
    m: &UiModel,
    p: &Palette,
    key: &RoleKey,
    cells: usize,
    rows: usize,
) -> (String, Vec<Line<'static>>) {
    let (roles, parent, title) = match &key.team {
        Some(t) => {
            let roles = m
                .org
                .teams
                .iter()
                .find(|x| &x.name == t)
                .map(|x| &x.roles[..]);
            (
                roles,
                NodeKey::Team(t.clone()),
                format!("◇ {} · in {t}", key.role),
            )
        }
        None => (
            Some(&m.org.roles[..]),
            NodeKey::Duo,
            format!("◇ {}", key.role),
        ),
    };
    let Some(role) = roles.and_then(|rs| rs.iter().find(|r| r.name == key.role)) else {
        return (String::new(), Vec::new());
    };
    let names: Vec<&AgentName> = role.agents.iter().map(|a| &a.name).collect();
    (
        title,
        summary(m, p, &names, &role.defaults, &parent, cells, rows),
    )
}

/// A blank line, then `text` indented.
fn message(text: &str, color: ratatui::style::Color) -> Vec<Line<'static>> {
    vec![
        Line::default(),
        Line::from(Span::styled(
            format!("{PAD}{MESSAGE_INDENT}{text}"),
            fg(color),
        )),
    ]
}

/// `<agent> · <CLI> · worktree <task-id>` or `· <dir>`; Duo's `⬢ <name> · <CLI> · your
/// manager agent`. `· TYPING` and `· scrolled` follow the mode (UI §4.1.2, P2-D12). Cut
/// to `room` cells, the mode word always stays: the place goes first, then the name is
/// cut in the middle (UI §2.4).
fn agent_title(m: &UiModel, name: &AgentName, room: usize) -> String {
    let resolved = resolve(&m.org, name).ok();
    let cli = resolved
        .as_ref()
        .map_or(m.org.defaults.cli, |r| r.cli)
        .short();
    // The place: a fixed label and a value that may be cut (Duo's is never cut).
    let (lead, place) = if m.org.is_duo(name) {
        (format!("⬢ {name}"), Some(("your manager agent", None)))
    } else {
        let place = match &m.view(name).worktree {
            Some(task) => Some(("worktree ", Some(task.0.clone()))),
            None => resolved.and_then(|r| r.directory).map(|d| ("", Some(d))),
        };
        (name.to_string(), place)
    };
    let mode = match (m.typing(), m.scrolled) {
        (true, true) => " · TYPING · scrolled",
        (true, false) => " · TYPING",
        (false, true) => " · scrolled",
        (false, false) => "",
    };
    let head = format!("{lead} · {cli}");
    let fixed = width(&head) + width(mode);
    let place = place.map_or(String::new(), |(label, value)| {
        let value = value.unwrap_or_default();
        let full = format!("{SEP}{label}{value}");
        if fixed + width(&full) <= room || mode.is_empty() {
            return full;
        }
        let left = room.saturating_sub(fixed + width(SEP) + width(label));
        if value.is_empty() || left < MIN_PLACE {
            return String::new();
        }
        format!("{SEP}{label}{}", truncate_middle(&value, left))
    });
    if place.is_empty() && fixed > room && !mode.is_empty() {
        let tail = format!(" · {cli}{mode}");
        let lead = truncate_middle(&lead, room.saturating_sub(width(&tail)));
        return format!("{lead}{tail}");
    }
    format!("{head}{place}{mode}")
}

/// The title, and as the body (until the live terminal shows) its state; how to give it
/// work or stop it when not running.
fn agent(
    m: &UiModel,
    ctx: &RenderCtx,
    name: &AgentName,
    cells: usize,
) -> (String, Vec<Line<'static>>) {
    let p = &ctx.palette;
    let title = agent_title(m, name, cells.saturating_sub(TITLE_FRAME));
    let state = m.view(name).status.state;
    let body = match state {
        AgentState::Stopped => message(STOPPED, p.fg_muted),
        AgentState::Errored => message(ERRORED, p.error),
        _ => {
            let status = &m.view(name).status;
            let line = format!(
                "{}{} {}",
                marker(status),
                status_glyph(status, ctx.spinner()),
                status_word(status)
            );
            message(&line, super::state_color(p, status))
        }
    };
    (title, body)
}

/// A team or role summary: counts, defaults and prompt, then the edit key. When the pane
/// is too short for all of it, the defaults and prompt go first (UI §3's shrink order).
fn summary(
    m: &UiModel,
    p: &Palette,
    names: &[&AgentName],
    own: &Fields,
    parent: &NodeKey,
    cells: usize,
    rows: usize,
) -> Vec<Line<'static>> {
    let views: Vec<_> = names.iter().filter_map(|n| m.agents.get(*n)).collect();
    let noun = if names.len() == 1 { "agent" } else { "agents" };
    let mut counts = vec![Span::styled(
        format!("{PAD}{} {noun}", names.len()),
        fg(p.fg),
    )];
    for ((glyph, n), word) in roll_up_counts(&views).into_iter().zip(STATE_WORDS) {
        if n > 0 {
            counts.push(Span::styled(" · ", fg(p.fg_muted)));
            counts.push(Span::styled(
                format!("{glyph} {n} {word}"),
                fg(super::glyph_color(p, glyph)),
            ));
        }
    }
    let from_above = inherited_for(&m.org, parent);
    let cli = setting(
        own.cli.map(CliKind::display),
        Some(from_above.cli.value.display()),
    );
    let model = setting(
        own.model.as_deref(),
        from_above.model.as_ref().map(|v| v.value.as_str()),
    );
    let worktrees = setting(
        own.worktree.map(on_off),
        Some(on_off(from_above.worktree.value)),
    );
    let prompt = own
        .prompt
        .as_deref()
        .map(one_line)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| NONE.to_owned());
    // Room after the padding cell and the label, leaving the right padding cell.
    let room = cells.saturating_sub(LABEL_CELLS + 2);
    let mut defaults = defaults_lines(p, cli, model, room);
    defaults.push(value_line(
        Span::raw(hanging()),
        "worktrees ",
        worktrees,
        room,
        p,
    ));
    let prompt = truncate_end(&prompt, room);
    let keys = Line::from(Span::styled(format!("{PAD}e edit"), fg(p.fg_muted)));
    if rows < SUMMARY_ROWS + defaults.len() - 2 {
        return vec![Line::from(counts), Line::default(), keys];
    }
    let mut lines = vec![Line::from(counts)];
    lines.extend(defaults);
    lines.extend([
        Line::from(vec![label("Prompt", p), Span::styled(prompt, fg(p.fg))]),
        Line::default(),
        keys,
    ]);
    lines
}

/// `Defaults  CLI … · model …` on one line when it fits in `room`; otherwise model goes
/// on its own line under CLI, with the same hanging indent as worktrees.
fn defaults_lines(p: &Palette, cli: Setting, model: Setting, room: usize) -> Vec<Line<'static>> {
    let one = width("CLI ") + cli.width() + width(SEP) + width("model ") + model.width();
    if one <= room {
        let mut spans = vec![label("Defaults", p), Span::styled("CLI ", fg(p.fg))];
        spans.push(cli.styled(p, room));
        spans.push(Span::styled(format!("{SEP}model "), fg(p.fg)));
        spans.push(model.styled(p, room));
        return vec![Line::from(spans)];
    }
    vec![
        value_line(label("Defaults", p), "CLI ", cli, room, p),
        value_line(Span::raw(hanging()), "model ", model, room, p),
    ]
}

/// `lead`, then `key` and its setting cut to fit `room`.
fn value_line(
    lead: Span<'static>,
    key: &str,
    value: Setting,
    room: usize,
    p: &Palette,
) -> Line<'static> {
    let left = room.saturating_sub(width(key));
    Line::from(vec![
        lead,
        Span::styled(key.to_owned(), fg(p.fg)),
        value.styled(p, left),
    ])
}

/// The indent that puts a continuation line under the summary's values.
fn hanging() -> String {
    format!("{PAD}{}", " ".repeat(LABEL_CELLS))
}

fn label(text: &str, p: &Palette) -> Span<'static> {
    let pad = LABEL_CELLS.saturating_sub(width(text));
    Span::styled(format!("{PAD}{text}{}", " ".repeat(pad)), fg(p.fg_muted))
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

/// A node's own value, or what it inherits (`inherit: <value>`, UI §2.4).
enum Setting {
    Own(String),
    Inherited(String),
    /// Set nowhere above: nothing to inherit.
    Unset,
}

fn setting(own: Option<&str>, inherited: Option<&str>) -> Setting {
    match (own, inherited) {
        (Some(v), _) => Setting::Own(v.to_owned()),
        (None, Some(v)) => Setting::Inherited(v.to_owned()),
        (None, None) => Setting::Unset,
    }
}

impl Setting {
    fn text(&self) -> String {
        match self {
            Setting::Own(v) => v.clone(),
            Setting::Inherited(v) => format!("inherit: {v}"),
            Setting::Unset => NONE.to_owned(),
        }
    }

    fn width(&self) -> usize {
        width(&self.text())
    }

    /// The setting, cut with `…` to fit `cells`; inherited and unset values are muted.
    fn styled(self, p: &Palette, cells: usize) -> Span<'static> {
        let colour = match self {
            Setting::Own(_) => p.fg,
            Setting::Inherited(_) | Setting::Unset => p.fg_muted,
        };
        Span::styled(truncate_end(&self.text(), cells), fg(colour))
    }
}
