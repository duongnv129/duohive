//! The side pane: the selected node's summary (UI §4.1.2, D7 phase-1 text).

use duohive_core::cli_kind::CliKind;
use duohive_core::inherit::{inherited_for, resolve};
use duohive_core::names::AgentName;
use duohive_core::org::{Fields, NodeKey, RoleKey};
use duohive_core::status::AgentState;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use super::{LIGHT, RenderCtx, draw_box, fg};
use crate::format::{one_line, truncate_end, width};
use crate::model::UiModel;
use crate::theme::Palette;
use crate::tree::roll_up_counts;

const EMPTY_ORG: &str = "Your org is empty · n to add a team, role or agent";
const STOPPED: &str = "○ stopped · s to start";
const ERRORED: &str = "✕ errored · s to stop";
/// Indent of a one-line message inside the pane, after the padding cell.
const MESSAGE_INDENT: &str = "  ";
/// The label column of a team or role summary.
const LABEL_CELLS: usize = 10;
const STATE_WORDS: [&str; 4] = ["working", "waiting", "quiet", "errored"];

pub(super) fn draw(buf: &mut Buffer, r: Rect, m: &UiModel, ctx: &RenderCtx) {
    let p = &ctx.palette;
    let (title, lines) = content(m, ctx, usize::from(r.width.saturating_sub(4)));
    draw_box(
        buf,
        r,
        &title,
        &LIGHT,
        fg(p.border),
        Style::default().bg(p.bg),
    );
    if r.width < 5 || r.height < 3 {
        return;
    }
    let inner = Rect {
        x: r.x + 2,
        y: r.y + 1,
        width: r.width - 4,
        height: r.height - 2,
    };
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}

/// The pane's title and body for the current state and selection.
fn content(m: &UiModel, ctx: &RenderCtx, cells: usize) -> (String, Vec<Line<'static>>) {
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
        NodeKey::Duo => agent(m, ctx, &m.org.duo.name),
        NodeKey::Agent(a) => agent(m, ctx, a),
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
                summary(m, p, &names, &team.defaults, &NodeKey::Duo, cells),
            )
        }
        NodeKey::Role(key) => role(m, p, key, cells),
    }
}

fn role(m: &UiModel, p: &Palette, key: &RoleKey, cells: usize) -> (String, Vec<Line<'static>>) {
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
    (title, summary(m, p, &names, &role.defaults, &parent, cells))
}

/// A blank line, then `text` indented.
fn message(text: &str, color: ratatui::style::Color) -> Vec<Line<'static>> {
    vec![
        Line::default(),
        Line::from(Span::styled(format!("{MESSAGE_INDENT}{text}"), fg(color))),
    ]
}

/// Title `<agent> · <CLI>`; body: how to attach while it runs, else how to start or stop it.
fn agent(m: &UiModel, ctx: &RenderCtx, name: &AgentName) -> (String, Vec<Line<'static>>) {
    let p = &ctx.palette;
    let cli = resolve(&m.org, name).map_or(m.org.defaults.cli, |r| r.cli);
    let title = format!("{name} · {}", cli.display());
    let state = m
        .agents
        .get(name)
        .map_or(AgentState::Stopped, |v| v.status.state);
    let body = match state {
        AgentState::Stopped => message(STOPPED, p.fg_muted),
        AgentState::Errored => message(ERRORED, p.error),
        _ => message(
            &format!(
                "attach: tmux -L {} attach -t duohive:{name}",
                ctx.tmux_socket
            ),
            p.fg,
        ),
    };
    (title, body)
}

/// A team or role summary: counts, defaults and prompt, then the edit key.
fn summary(
    m: &UiModel,
    p: &Palette,
    names: &[&AgentName],
    own: &Fields,
    parent: &NodeKey,
    cells: usize,
) -> Vec<Line<'static>> {
    let views: Vec<_> = names.iter().filter_map(|n| m.agents.get(*n)).collect();
    let noun = if names.len() == 1 { "agent" } else { "agents" };
    let mut counts = vec![Span::styled(format!("{} {noun}", names.len()), fg(p.fg))];
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
        from_above.cli.value.display(),
    );
    let model = setting(
        own.model.as_deref(),
        from_above.model.as_ref().map_or("none", |v| &v.value),
    );
    let worktrees = setting(own.worktree.map(on_off), on_off(from_above.worktree.value));
    let prompt = own
        .prompt
        .as_deref()
        .map(one_line)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "(none)".to_owned());
    let pad = " ".repeat(LABEL_CELLS);
    let mut defaults = vec![label("Defaults", p), Span::styled("CLI ", fg(p.fg))];
    defaults.push(cli.styled(p));
    defaults.push(Span::styled(" · model ", fg(p.fg)));
    defaults.push(model.styled(p));
    let mut wt = vec![Span::raw(pad), Span::styled("worktrees ", fg(p.fg))];
    wt.push(worktrees.styled(p));
    let prompt = truncate_end(&prompt, cells.saturating_sub(LABEL_CELLS));
    vec![
        Line::from(counts),
        Line::from(defaults),
        Line::from(wt),
        Line::from(vec![label("Prompt", p), Span::styled(prompt, fg(p.fg))]),
        Line::default(),
        Line::from(Span::styled("e edit", fg(p.fg_muted))),
    ]
}

fn label(text: &str, p: &Palette) -> Span<'static> {
    let pad = LABEL_CELLS.saturating_sub(width(text));
    Span::styled(format!("{text}{}", " ".repeat(pad)), fg(p.fg_muted))
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

/// A node's own value, or what it inherits (`inherit: <value>`, UI §2.4).
enum Setting {
    Own(String),
    Inherited(String),
}

fn setting(own: Option<&str>, inherited: &str) -> Setting {
    match own {
        Some(v) => Setting::Own(v.to_owned()),
        None => Setting::Inherited(inherited.to_owned()),
    }
}

impl Setting {
    fn styled(self, p: &Palette) -> Span<'static> {
        match self {
            Setting::Own(v) => Span::styled(v, fg(p.fg)),
            Setting::Inherited(v) => Span::styled(format!("inherit: {v}"), fg(p.fg_muted)),
        }
    }
}
