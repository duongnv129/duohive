//! An approval card in the chat (UI §4.6; P3-D4, P3-D15): its outline is `border`, heavy
//! when focused, with an amber `◆` in its title; the body, then `[y] Approve   [e] Edit
//! [n] Reject` (no `[e]` where it is disabled).

use duohive_core::card::{Card, CardChange};
use duohive_core::command::OrgEdit;
use duohive_core::inherit::inherited_for;
use duohive_core::names::AgentName;
use duohive_core::org::{Fields, NodeKey, Org, RoleKey};
use duohive_core::time::Millis;
use ratatui::style::Color;

use super::{HEAVY, LIGHT, Lines};
use crate::format::{
    age_of, format_age, pad_right, status_glyph, status_word, truncate_end, width,
};
use crate::forms::card_editable;
use crate::model::UiModel;
use crate::theme::Palette;

/// UI §4.6's card is 66 cells wide; narrower where the chat is.
pub(crate) const CARD_WIDTH: usize = 66;
/// `┃ ` before a line and ` ┃` after it.
const SIDE: usize = 2;
/// Between buttons.
const BUTTON_GAP: &str = "   ";
/// The attention glyph in the title (P3-D4).
const DIAMOND: char = '◆';
/// A second line hangs under the first's text.
const HANG: &str = "  ";

/// One screen row: styled pieces.
pub(crate) type Row = Vec<(String, Color)>;

/// The card's title: `◆ Proposed change`, `◆ Stop squad-1` or `◆ No free agent in
/// Generalist`.
pub(crate) fn card_title(card: &Card, org: &Org) -> String {
    let what = match &card.change {
        CardChange::Org(_) => "Proposed change".to_owned(),
        CardChange::Stop { agent } => format!("Stop {agent}"),
        CardChange::NewAgent { role, .. } => format!("No free agent in {}", role_path(org, role)),
    };
    format!("{DIAMOND} {what}")
}

/// The card's body lines, then its button row.
pub(crate) fn card_lines(card: &Card, m: &UiModel, now: Millis) -> Vec<String> {
    let mut lines = body(card, m, now);
    lines.push(button_row(card));
    lines
}

/// The buttons, by key: `y`, then `e` where the card can be edited, then `n`.
pub(crate) fn buttons(card: &Card) -> Vec<(char, &'static str)> {
    let mut out = vec![('y', "Approve")];
    if card_editable(card) {
        out.push(('e', "Edit"));
    }
    out.push(('n', "Reject"));
    out
}

/// `[k] Label`, one button's text.
pub(crate) fn button_text(key: char, label: &str) -> String {
    format!("[{key}] {label}")
}

/// Which button covers `col`, counted from the first button's first cell.
pub(crate) fn button_at(card: &Card, col: usize) -> Option<char> {
    let mut x = 0;
    for (key, label) in buttons(card) {
        let w = width(&button_text(key, label));
        if (x..x + w).contains(&col) {
            return Some(key);
        }
        x += w + BUTTON_GAP.len();
    }
    None
}

/// Cells from the card's left edge to its text.
pub(crate) fn text_offset() -> usize {
    SIDE
}

/// The card `cells` wide at most: its rows, outline included.
pub(crate) fn card_rows(
    card: &Card,
    m: &UiModel,
    now: Millis,
    focused: bool,
    cells: usize,
    p: &Palette,
) -> Vec<Row> {
    let w = cells.min(CARD_WIDTH);
    let lines: &Lines = if focused { &HEAVY } else { &LIGHT };
    let inner = w.saturating_sub(2 * SIDE);
    let title = card_title(card, &m.org);
    let rest = title.trim_start_matches(DIAMOND).trim_start();
    // `┏━ ◆ ` before the title, then ` ` and `┓`; at least one `━` fills.
    let frame = 7;
    let shown = truncate_end(rest, w.saturating_sub(frame + 1));
    let fill = lines.h.repeat(w.saturating_sub(width(&shown) + frame));
    let mut rows = vec![vec![
        (format!("{}{} ", lines.top[0], lines.h), p.border),
        (DIAMOND.to_string(), p.attention),
        (format!(" {shown} "), p.fg),
        (format!("{fill}{}", lines.top[1]), p.border),
    ]];
    for line in card_lines(card, m, now) {
        rows.push(vec![
            (format!("{} ", lines.v), p.border),
            (pad_right(&truncate_end(&line, inner), inner), p.fg),
            (format!(" {}", lines.v), p.border),
        ]);
    }
    let bottom = lines.h.repeat(w.saturating_sub(2));
    rows.push(vec![(
        format!("{}{bottom}{}", lines.bottom[0], lines.bottom[1]),
        p.border,
    )]);
    rows
}

fn button_row(card: &Card) -> String {
    buttons(card)
        .into_iter()
        .map(|(k, l)| button_text(k, l))
        .collect::<Vec<_>>()
        .join(BUTTON_GAP)
}

/// The change in UI §4.6's style: `+` adds, `~` changes or moves, `-` removes; a second
/// line hangs under the first with what the new agent runs on, or its task (P3-D15).
fn body(card: &Card, m: &UiModel, now: Millis) -> Vec<String> {
    let org = &m.org;
    match &card.change {
        CardChange::Stop { agent } => vec![stop_line(agent, m, now)],
        CardChange::NewAgent {
            role,
            agent,
            task,
            clone_of,
            ..
        } => {
            let clone = clone_of
                .as_ref()
                .map(|a| format!(" (clone of {a})"))
                .unwrap_or_default();
            let first = task.lines().next().unwrap_or_default();
            vec![
                format!(
                    "+ Add agent {} in {}{clone}",
                    agent.name,
                    role_path(org, role)
                ),
                format!("{HANG}Task  {first}"),
            ]
        }
        CardChange::Org(edit) => edit_lines(edit, card, org),
    }
}

/// `squad-1 is ● working 12m`: the agent's state now.
fn stop_line(agent: &AgentName, m: &UiModel, now: Millis) -> String {
    let status = &m.view(agent).status;
    let age = age_of(status, now)
        .map(|d| format!(" {}", format_age(d)))
        .unwrap_or_default();
    format!(
        "{agent} is {} {}{age}",
        status_glyph(status, None),
        status_word(status)
    )
}

fn edit_lines(edit: &OrgEdit, card: &Card, org: &Org) -> Vec<String> {
    match edit {
        OrgEdit::AddAgent { role, agent } => {
            let known = org.breadcrumb(&NodeKey::Role(role.clone()));
            let new_role = if role_exists(org, role) {
                ""
            } else {
                " (new role)"
            };
            vec![
                format!("+ Add agent {} in {known}{new_role}", agent.name),
                format!(
                    "{HANG}{}",
                    runs_on(org, &NodeKey::Role(role.clone()), &agent.fields)
                ),
            ]
        }
        OrgEdit::EditAgent { key, draft } => {
            let renamed = if &draft.name == key {
                String::new()
            } else {
                format!(" → {}", draft.name)
            };
            let parent = org.find_agent(key).map_or(NodeKey::Duo, |loc| {
                NodeKey::Role(RoleKey {
                    team: loc.team.map(|t| t.name.clone()),
                    role: loc.role.name.clone(),
                })
            });
            vec![
                format!("~ Edit agent {key}{renamed}"),
                format!("{HANG}{}", runs_on(org, &parent, &draft.fields)),
            ]
        }
        OrgEdit::AddRole { agents, .. } => {
            let noun = if *agents == 1 { "agent" } else { "agents" };
            vec![
                format!("+ {}", capitalised(&card.summary(org))),
                format!("{HANG}{agents} {noun}"),
            ]
        }
        OrgEdit::AddTeam(_) => vec![format!("+ {}", capitalised(&card.summary(org)))],
        OrgEdit::Delete(_) => vec![format!("- {}", capitalised(&card.summary(org)))],
        OrgEdit::EditTeam { .. }
        | OrgEdit::EditRole { .. }
        | OrgEdit::EditOrgDefaults { .. }
        | OrgEdit::Move { .. } => vec![format!("~ {}", capitalised(&card.summary(org)))],
    }
}

/// `CLI Claude Code · ~/code/web · worktrees on`: what an agent under `parent` with `own`
/// fields runs on.
fn runs_on(org: &Org, parent: &NodeKey, own: &Fields) -> String {
    let inherited = inherited_for(org, parent);
    let cli = own.cli.unwrap_or(inherited.cli.value);
    let dir = own
        .directory
        .clone()
        .or_else(|| inherited.directory.map(|d| d.value));
    let worktrees = if own.worktree.unwrap_or(inherited.worktree.value) {
        "worktrees on"
    } else {
        "worktrees off"
    };
    let mut parts = vec![format!("CLI {}", cli.display())];
    parts.extend(dir);
    parts.push(worktrees.to_owned());
    parts.join(" · ")
}

fn role_path(org: &Org, role: &RoleKey) -> String {
    org.breadcrumb(&NodeKey::Role(role.clone()))
}

fn role_exists(org: &Org, key: &RoleKey) -> bool {
    let roles = match &key.team {
        Some(t) => match org.teams.iter().find(|x| &x.name == t) {
            Some(team) => &team.roles,
            None => return false,
        },
        None => &org.roles,
    };
    roles.iter().any(|r| r.name == key.role)
}

/// `text` with its first letter upper-cased: a summary as a sentence (UI §4.1.3).
pub(crate) fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}
