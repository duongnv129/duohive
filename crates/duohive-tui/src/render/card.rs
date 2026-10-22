//! An approval card in the chat (UI §4.6; P3-D4, P3-D15): its outline is `border`, heavy
//! when focused, with an amber `◆` in its title; the body, then `[y] Approve   [e] Edit
//! [n] Reject` (no `[e]` where it is disabled). An org change lists every field it sets
//! or changes ([`card_fields`](super::card_fields)); a `flags` row is amber and never cut.

use duohive_core::card::{Card, CardChange};
use duohive_core::command::OrgEdit;
use duohive_core::names::AgentName;
use duohive_core::org::{NodeKey, Org, RoleKey};
use duohive_core::time::Millis;
use ratatui::style::Color;

use super::card_fields::{BodyLine, detail, find_role};
use super::{HEAVY, LIGHT, Lines};
use crate::format::{
    age_of, chunk_cells, format_age, pad_right, status_glyph, status_word, truncate_end, width,
};
use crate::forms::card_editable;
use crate::keymap::{ActionId, Keymap};
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
/// A marked row's text past its first screen row hangs this far in.
const WRAP_HANG: usize = 4;

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

/// The card's body lines, then its button row, as text (tests read cards this way).
#[cfg(test)]
pub(crate) fn card_lines(card: &Card, m: &UiModel, now: Millis) -> Vec<String> {
    let mut lines: Vec<String> = body(card, m, now).into_iter().map(|l| l.text).collect();
    lines.push(button_row(card, &m.keymap));
    lines
}

/// The buttons, by action: approve, then edit where the card can be edited, then reject.
pub(crate) fn buttons(card: &Card) -> Vec<(ActionId, &'static str)> {
    let mut out = vec![(ActionId::CardApprove, "Approve")];
    if card_editable(card) {
        out.push((ActionId::CardEdit, "Edit"));
    }
    out.push((ActionId::CardReject, "Reject"));
    out
}

/// The key that asks for `id`, as the card shows it.
fn key_label(km: &Keymap, id: ActionId) -> String {
    km.key(id).label()
}

/// `[k] Label`, one button's text.
pub(crate) fn button_text(key: &str, label: &str) -> String {
    format!("[{key}] {label}")
}

/// Which button covers `col`, counted from the first button's first cell.
pub(crate) fn button_at(km: &Keymap, card: &Card, col: usize) -> Option<ActionId> {
    let mut x = 0;
    for (id, label) in buttons(card) {
        let w = width(&button_text(&key_label(km, id), label));
        if (x..x + w).contains(&col) {
            return Some(id);
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
    let mut body = body(card, m, now);
    body.push(BodyLine::plain(button_row(card, &m.keymap)));
    for line in body {
        let (pieces, colour) = if line.marked {
            (wrap_hanging(&line.text, inner), p.attention)
        } else {
            (vec![truncate_end(&line.text, inner)], p.fg)
        };
        for piece in pieces {
            rows.push(vec![
                (format!("{} ", lines.v), p.border),
                (pad_right(&piece, inner), colour),
                (format!(" {}", lines.v), p.border),
            ]);
        }
    }
    let bottom = lines.h.repeat(w.saturating_sub(2));
    rows.push(vec![(
        format!("{}{bottom}{}", lines.bottom[0], lines.bottom[1]),
        p.border,
    )]);
    rows
}

/// `text` in full over rows of `cells`, breaking between words where it can; rows after
/// the first hang [`WRAP_HANG`] cells in.
fn wrap_hanging(text: &str, cells: usize) -> Vec<String> {
    let body = text.trim_start_matches(' ');
    let lead = " ".repeat(text.len() - body.len());
    wrap_words_whole(body, cells.saturating_sub(lead.len()))
        .into_iter()
        .map(|r| format!("{lead}{r}"))
        .collect()
}

/// [`wrap_hanging`] for text with no leading spaces.
fn wrap_words_whole(text: &str, cells: usize) -> Vec<String> {
    let hang = " ".repeat(WRAP_HANG.min(cells / 2));
    let mut rows: Vec<String> = Vec::new();
    let mut row = String::new();
    for word in text.split(' ') {
        let room = if rows.is_empty() {
            cells
        } else {
            cells - hang.len()
        };
        let joined = if row.is_empty() {
            word.to_owned()
        } else {
            format!("{row} {word}")
        };
        if width(&joined) <= room {
            row = joined;
            continue;
        }
        if !row.is_empty() {
            rows.push(std::mem::take(&mut row));
        }
        let room = cells - if rows.is_empty() { 0 } else { hang.len() };
        let mut pieces = chunk_cells(word, room);
        row = pieces.pop().unwrap_or_default();
        rows.extend(pieces);
    }
    rows.push(row);
    rows.into_iter()
        .enumerate()
        .map(|(i, r)| if i == 0 { r } else { format!("{hang}{r}") })
        .collect()
}

fn button_row(card: &Card, km: &Keymap) -> String {
    buttons(card)
        .into_iter()
        .map(|(id, l)| button_text(&key_label(km, id), l))
        .collect::<Vec<_>>()
        .join(BUTTON_GAP)
}

/// The change in UI §4.6's style: `+` adds, `~` changes or moves, `-` removes; the rows
/// under it hang with every field the change sets, or the new agent's task (P3-D15).
fn body(card: &Card, m: &UiModel, now: Millis) -> Vec<BodyLine> {
    let org = &m.org;
    let plain = |lines: Vec<String>| lines.into_iter().map(BodyLine::plain).collect();
    match &card.change {
        CardChange::Stop { agent } => plain(vec![stop_line(agent, m, now)]),
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
            plain(vec![
                format!(
                    "+ Add agent {} in {}{clone}",
                    agent.name,
                    role_path(org, role)
                ),
                format!("{HANG}Task  {first}"),
            ])
        }
        CardChange::Org(edit) => {
            let mut lines = vec![BodyLine::plain(head_line(edit, card, org))];
            lines.extend(detail(edit, org));
            lines
        }
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

/// An org change's first line: `+ Add agent pay-web in Payments › QA (new role)`,
/// `~ Edit role Payments › QA`, `- Remove team Growth`.
fn head_line(edit: &OrgEdit, card: &Card, org: &Org) -> String {
    let summary = capitalised(&card.summary(org));
    match edit {
        OrgEdit::AddAgent { role, agent } => {
            let new_role = if find_role(org, role).is_some() {
                ""
            } else {
                " (new role)"
            };
            format!(
                "+ Add agent {} in {}{new_role}",
                agent.name,
                role_path(org, role)
            )
        }
        OrgEdit::EditAgent { key, .. } => format!("~ Edit agent {key}"),
        OrgEdit::AddRole { .. } | OrgEdit::AddTeam(_) => format!("+ {summary}"),
        OrgEdit::Delete(_) => format!("- {summary}"),
        OrgEdit::EditTeam { .. }
        | OrgEdit::EditRole { .. }
        | OrgEdit::EditOrgDefaults { .. }
        | OrgEdit::Move { .. } => format!("~ {summary}"),
    }
}

fn role_path(org: &Org, role: &RoleKey) -> String {
    org.breadcrumb(&NodeKey::Role(role.clone()))
}

/// `text` with its first letter upper-cased: a summary as a sentence (UI §4.1.3).
pub(crate) fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}
