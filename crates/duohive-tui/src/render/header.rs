//! Banners, the header line and the footer (UI §4.1, §4.12, D29).

use duohive_core::ports::OrgFileError;
use duohive_core::status::AgentState;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::form::{CHOOSER_HINT, HINT};
use super::{RenderCtx, fg, put};
use crate::focus::Pane;
use crate::format::{truncate_end, width, wrap_capped};
use crate::forms::{FieldKind, SettingsRow, SettingsState, card_editable};
use crate::keymap::{ActionId, Keymap};
use crate::model::{Dialog, SPACE_RETRY, UiModel};
use crate::theme::Palette;

/// Continuation lines of a banner start under its text, past ` ✕ `.
const BANNER_INDENT: &str = "   ";
const MAX_BANNER_LINES: usize = 3;

/// Added to a form's keys while a prompt field has focus.
const NEWLINE_KEY: &str = " · alt+enter new line";
const AGENT_KEYS: &str = " enter type · z zoom · PgUp/PgDn scroll · space give work";
/// A stopped agent has no terminal to type in or scroll.
const AGENT_KEYS_STOPPED: &str = " z zoom · space give work";
/// A team or role's summary: no agent keys apply there.
const SUMMARY_KEYS: &str = " tab pane · esc back";
const CHAT_KEYS: &str = " enter send · alt+enter new line · tab pane";
const SETTINGS_KEYS: &str = " ↑↓ move · space change · esc back";
const SETTINGS_STEP_KEYS: &str = " ↑↓ move · space change · ←→ step · esc back";
/// While a Settings text box is open.
const SETTINGS_EDIT_KEYS: &str = " enter save · esc cancel";
/// Typing's footer starts and ends with these (UI §4.1.2).
const TYPING_LEAD: &str = " TYPING · ";
const TYPING_TAIL: &str = " · ctrl+q stop typing";

/// The banner lines for each broken file, `org.toml` first, wrapped to `cells` after a ` · `
/// separator so the fix stays whole; continuation lines are indented under the text.
pub(super) fn banners(m: &UiModel, cells: usize) -> Vec<String> {
    let org = m.org_error.as_ref().map(|e| {
        let where_ = located("org.toml", e);
        format!(
            "✕ {where_}: {} · using your last good org · o open in $EDITOR",
            e.message
        )
    });
    let settings = m.settings_error.as_ref().map(|e| {
        let where_ = located("settings.toml", e);
        format!("✕ {where_}: {} · using default settings", e.message)
    });
    let room = cells.saturating_sub(BANNER_INDENT.len());
    org.into_iter()
        .chain(settings)
        .flat_map(|text| {
            wrap_capped(&text, room, MAX_BANNER_LINES)
                .into_iter()
                .enumerate()
                .map(|(i, line)| {
                    let lead = if i == 0 { " " } else { BANNER_INDENT };
                    format!("{lead}{line}")
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn located(file: &str, e: &OrgFileError) -> String {
    match e.line {
        Some(line) => format!("{file} line {line}"),
        None => file.to_owned(),
    }
}

/// Agents that are working and not quiet.
fn working_count(m: &UiModel) -> usize {
    m.agent_names()
        .into_iter()
        .map(|a| &m.view(a).status)
        .filter(|s| s.state == AgentState::Working && !s.quiet)
        .count()
}

/// The filter a click at column `x` of the header sets: the working count or the need-you
/// count (UI §4.1.1); finished is reached by `f` only.
pub(crate) fn count_at(m: &UiModel, compact: bool, x: u16) -> Option<crate::filter::Filter> {
    use crate::filter::Filter;
    let working = working_count(m);
    let need = m.needs_you.len();
    let agents = m.org.agents().len();
    let noun = if agents == 1 { "agent" } else { "agents" };
    let pieces: Vec<(String, Option<Filter>)> = if compact {
        vec![
            (" DuoHive".to_owned(), None),
            ("  ".to_owned(), None),
            (format!("● {working}"), Some(Filter::Working)),
            (" · ".to_owned(), None),
            (format!("◆ {need}"), Some(Filter::NeedYou)),
        ]
    } else {
        vec![
            (" DuoHive".to_owned(), None),
            (format!("   {agents} {noun}"), None),
            (" · ".to_owned(), None),
            (format!("● {working} working"), Some(Filter::Working)),
            (" · ".to_owned(), None),
            (format!("◆ {need} need you"), Some(Filter::NeedYou)),
        ]
    };
    let mut at = 0usize;
    for (text, filter) in pieces {
        let end = at + width(&text);
        if usize::from(x) >= at && usize::from(x) < end {
            return filter;
        }
        at = end;
    }
    None
}

/// ` DuoHive   6 agents · ● 2 working · ◆ 2 need you`, or ` DuoHive  ● 2 · ◆ 2` when compact.
pub(super) fn draw_header(buf: &mut Buffer, r: Rect, m: &UiModel, ctx: &RenderCtx, compact: bool) {
    let p = &ctx.palette;
    let working = working_count(m);
    let need = m.needs_you.len();
    let w = usize::from(r.width);
    let mut x = put(
        buf,
        r.x,
        r.y,
        " DuoHive",
        w,
        fg(p.fg).add_modifier(Modifier::BOLD),
    );
    let spans: Vec<(String, Style)> = if compact {
        vec![
            ("  ".to_owned(), fg(p.fg)),
            (format!("● {working}"), fg(p.working)),
            (" · ".to_owned(), fg(p.fg_muted)),
            (format!("◆ {need}"), fg(p.attention)),
        ]
    } else {
        let agents = m.org.agents().len();
        let noun = if agents == 1 { "agent" } else { "agents" };
        vec![
            (format!("   {agents} {noun}"), fg(p.fg)),
            (" · ".to_owned(), fg(p.fg_muted)),
            (format!("● {working} working"), fg(p.working)),
            (" · ".to_owned(), fg(p.fg_muted)),
            (format!("◆ {need} need you"), fg(p.attention)),
        ]
    };
    for (text, style) in spans {
        let room = w.saturating_sub(usize::from(x - r.x));
        x = put(buf, x, r.y, &text, room, style);
    }
    if let Some(chip) = m.filter.chip(compact) {
        let room = w.saturating_sub(usize::from(x - r.x));
        x = put(buf, x, r.y, &format!("   {chip}"), room, fg(p.attention));
    }
    if !m.typing() && m.dialog.is_none() {
        let help = format!(" · {} help", m.keymap.key(ActionId::KeyMap).label());
        let room = w.saturating_sub(usize::from(x - r.x));
        // Drop it whole rather than cut it mid-hint on a narrow header.
        if room >= width(&help) {
            put(buf, x, r.y, &help, room, fg(p.fg_muted));
        }
    }
}

/// The footer: Typing's footer (with any note) while typing, the note when one is set,
/// else the keys of the dialog or focused pane.
pub(super) fn draw_footer(buf: &mut Buffer, r: Rect, m: &UiModel, p: &Palette, compact: bool) {
    let text = match (&m.footer_note, &m.dialog) {
        (Some(_), None) if m.focus.pane == Pane::Agent && m.typing() => {
            return typing_footer(buf, r, m, p);
        }
        (Some(note), _) => format!(" {note}"),
        // A form is typing: its keys, never the tree's (`q` types there).
        (None, Some(Dialog::Form(f))) => {
            let prompt = f
                .fields
                .get(f.focus)
                .is_some_and(|x| x.kind == FieldKind::Prompt);
            let newline = if prompt { NEWLINE_KEY } else { "" };
            format!(" {HINT}{newline}")
        }
        (None, Some(Dialog::Chooser(_))) => format!(" {CHOOSER_HINT}"),
        (None, Some(Dialog::Worktrees(_))) =>
        // The list draws its own hint (UI §4.9); a note still shows.
        {
            String::new()
        }
        (None, Some(Dialog::GiveWork(g))) => format!(" {}", g.hint()),
        (None, Some(Dialog::Confirm(c))) => confirm_keys(&c.yes_label),
        (None, Some(Dialog::Settings(s))) => settings_keys(s).to_owned(),
        (None, _) => match m.focus.pane {
            Pane::Org if compact => org_keys_compact(&m.keymap),
            Pane::Org => org_keys(&m.keymap),
            Pane::NeedsYou => needs_you_keys(&m.keymap),
            Pane::Agent if m.typing() => return typing_footer(buf, r, m, p),
            Pane::Agent => agent_keys(m).to_owned(),
            Pane::Chat => chat_keys(m),
        }
        .clone(),
    };
    put(buf, r.x, r.y, &text, usize::from(r.width), fg(p.fg_muted));
}

/// The org pane's keys (P2-D9), with `? help` (UI §4.11).
fn org_keys(km: &Keymap) -> String {
    let k = |a| km.key(a).label();
    format!(
        " tab pane · ↑↓ move · {} give work · {} start/stop · {} new · {} edit · {} zoom · {} help",
        k(ActionId::GiveWork),
        k(ActionId::StartStop),
        k(ActionId::New),
        k(ActionId::Edit),
        k(ActionId::Zoom),
        k(ActionId::KeyMap),
    )
}

/// The org pane's keys in the compact layout (UI §4.1's compact mock).
fn org_keys_compact(km: &Keymap) -> String {
    format!(
        " tab pane · ↑↓ move · {} give work",
        km.key(ActionId::GiveWork).label()
    )
}

/// The Needs you list's keys (UI §4.11).
fn needs_you_keys(km: &Keymap) -> String {
    format!(
        " ↑↓ move · enter go to agent · {} help",
        km.key(ActionId::KeyMap).label()
    )
}

/// The chat's keys, or a focused card's.
fn chat_keys(m: &UiModel) -> String {
    let k = |a| m.keymap.key(a).label();
    match m.focus.card.and_then(|id| m.cards.get(&id)) {
        Some(card) if card_editable(card) => format!(
            " {} approve · {} edit · {} reject · esc input",
            k(ActionId::CardApprove),
            k(ActionId::CardEdit),
            k(ActionId::CardReject)
        ),
        Some(_) => format!(
            " {} approve · {} reject · esc input",
            k(ActionId::CardApprove),
            k(ActionId::CardReject)
        ),
        None => CHAT_KEYS.to_owned(),
    }
}

/// The agent pane's keys for what it shows: a running agent's terminal, a stopped
/// agent's line or a team or role's summary.
fn agent_keys(m: &UiModel) -> &'static str {
    match m.selected_agent() {
        Some(a) if m.view(a).status.state.is_running() => AGENT_KEYS,
        Some(_) => AGENT_KEYS_STOPPED,
        None => SUMMARY_KEYS,
    }
}

/// A confirmation's keys: `y` says yes, `n` or `esc` cancels, `tab` moves between the
/// buttons and `enter` presses the focused one (UI §4.8).
fn confirm_keys(yes_label: &str) -> String {
    let yes = yes_label.to_lowercase();
    format!(" y {yes} · n/esc cancel · tab switch · enter choose")
}

/// Settings' keys (UI §4.10); `←→` only on the controls that step.
fn settings_keys(s: &SettingsState) -> &'static str {
    match s.row {
        _ if s.editing.is_some() => SETTINGS_EDIT_KEYS,
        SettingsRow::Quiet | SettingsRow::Theme => SETTINGS_STEP_KEYS,
        _ => SETTINGS_KEYS,
    }
}

/// Typing mode's footer (UI §4.1.2). A note takes the middle, cut to keep both ends, and
/// loses its `space to try again`: `space` goes to the agent.
pub(super) fn typing_footer(buf: &mut Buffer, r: Rect, m: &UiModel, p: &Palette) {
    let w = usize::from(r.width);
    let middle = match &m.footer_note {
        Some(note) => {
            let note = note.strip_suffix(SPACE_RETRY).unwrap_or(note);
            let room = w.saturating_sub(width(TYPING_LEAD) + width(TYPING_TAIL));
            truncate_end(note, room)
        }
        None => {
            let agent = m
                .selected_agent()
                .map(ToString::to_string)
                .unwrap_or_default();
            format!("every key goes to {agent}, ctrl+c too")
        }
    };
    let text = format!("{TYPING_LEAD}{middle}{TYPING_TAIL}");
    put(buf, r.x, r.y, &text, w, fg(p.attention));
}
