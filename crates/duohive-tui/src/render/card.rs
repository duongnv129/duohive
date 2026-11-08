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
use crate::format::{
    age_of, chunk_cells, format_age, status_glyph, status_word, truncate_end, width,
};
use crate::model::UiModel;
use crate::theme::Palette;

/// UI §4.6's card is 66 cells wide; narrower where the chat is.
pub(crate) const CARD_WIDTH: usize = 66;
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

/// The card's body lines as text (tests read cards this way).
#[cfg(test)]
pub(crate) fn card_lines(card: &Card, m: &UiModel, now: Millis) -> Vec<String> {
    body_lines(card, m, now)
        .into_iter()
        .map(|l| l.text)
        .collect()
}

/// The card's body for a dialog `cells` wide, in at most `slots` rows. When it does not
/// all fit, the first line, then the `flags` rows, then the rest are kept, and one
/// `… N more rows` stands for the cut (UI §4.6 *Size*).
pub(crate) fn card_body(
    card: &Card,
    m: &UiModel,
    now: Millis,
    cells: usize,
    slots: usize,
    p: &Palette,
) -> Vec<(String, Color)> {
    let mut body = Vec::new();
    for line in body_lines(card, m, now) {
        if line.marked {
            body.extend(
                wrap_hanging(&line.text, cells)
                    .into_iter()
                    .map(|t| (t, true)),
            );
        } else {
            body.push((truncate_end(&line.text, cells), false));
        }
    }
    let cut = if body.len() <= slots {
        Vec::new()
    } else {
        let marked = body
            .iter()
            .enumerate()
            .filter(|(_, (_, m))| *m)
            .map(|(i, _)| i);
        let rest = (1..body.len()).filter(|i| !body[*i].1);
        let priority: Vec<usize> = std::iter::once(0)
            .chain(marked.filter(|i| *i != 0))
            .chain(rest)
            .collect();
        let kept: Vec<usize> = priority.into_iter().take(slots.saturating_sub(1)).collect();
        (0..body.len()).filter(|i| !kept.contains(i)).collect()
    };
    let mut ellipsis = !cut.is_empty();
    let mut rows = Vec::new();
    for (i, (text, marked)) in body.into_iter().enumerate() {
        if cut.contains(&i) {
            if std::mem::take(&mut ellipsis) {
                rows.push((format!("… {} more rows", cut.len()), p.fg_muted));
            }
            continue;
        }
        rows.push((text, if marked { p.attention } else { p.fg }));
    }
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

/// The change in UI §4.6's style: `+` adds, `~` changes or moves, `-` removes; the rows
/// under it hang with every field the change sets, or the new agent's task (P3-D15).
fn body_lines(card: &Card, m: &UiModel, now: Millis) -> Vec<BodyLine> {
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
