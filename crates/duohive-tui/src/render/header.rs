//! Banners, the header line and the footer (UI §4.1, §4.12, D29).

use duohive_core::ports::OrgFileError;
use duohive_core::status::AgentState;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::form::{CHOOSER_HINT, HINT};
use super::{RenderCtx, fg, put};
use crate::focus::Pane;
use crate::format::wrap_capped;
use crate::forms::FieldKind;
use crate::model::{Dialog, UiModel};
use crate::theme::Palette;

/// Continuation lines of a banner start under its text, past ` ✕ `.
const BANNER_INDENT: &str = "   ";
const MAX_BANNER_LINES: usize = 3;

/// Added to a form's keys while a prompt field has focus.
const NEWLINE_KEY: &str = " · alt+enter new line";
/// The focused pane's keys (P2-D9); `? help` arrives in phase 4.
const ORG_KEYS: &str =
    " tab pane · ↑↓ move · space give work · s start/stop · n new · e edit · z zoom";
/// The org pane's keys in the compact layout (UI §4.1's compact mock).
const ORG_KEYS_COMPACT: &str = " tab pane · ↑↓ move · space give work";
const NEEDS_YOU_KEYS: &str = " ↑↓ move · enter go to agent";
const AGENT_KEYS: &str = " enter type · z zoom · PgUp/PgDn scroll · space give work";
const CHAT_KEYS: &str = " enter send · alt+enter new line · tab pane";

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

/// ` DuoHive   6 agents · ● 2 working · ◆ 2 need you`, or ` DuoHive  ● 2 · ◆ 2` when compact.
pub(super) fn draw_header(buf: &mut Buffer, r: Rect, m: &UiModel, ctx: &RenderCtx, compact: bool) {
    let p = &ctx.palette;
    let working = m
        .agent_names()
        .into_iter()
        .map(|a| &m.view(a).status)
        .filter(|s| s.state == AgentState::Working && !s.quiet)
        .count();
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
}

/// The footer: the note when one is set, else the keys of the dialog or focused pane.
pub(super) fn draw_footer(buf: &mut Buffer, r: Rect, m: &UiModel, p: &Palette, compact: bool) {
    let text = match (&m.footer_note, &m.dialog) {
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
        (None, _) => match m.focus.pane {
            Pane::Org if compact => ORG_KEYS_COMPACT,
            Pane::Org => ORG_KEYS,
            Pane::NeedsYou => NEEDS_YOU_KEYS,
            Pane::Agent if m.typing() => return typing_footer(buf, r, m, p),
            Pane::Agent => AGENT_KEYS,
            Pane::Chat => CHAT_KEYS,
        }
        .to_owned(),
    };
    put(buf, r.x, r.y, &text, usize::from(r.width), fg(p.fg_muted));
}

/// Typing mode's footer (UI §4.1.2).
fn typing_footer(buf: &mut Buffer, r: Rect, m: &UiModel, p: &Palette) {
    let agent = m
        .selected_agent()
        .map(ToString::to_string)
        .unwrap_or_default();
    let text = format!(" TYPING · every key goes to {agent}, ctrl+c too · ctrl+q stop typing");
    put(buf, r.x, r.y, &text, usize::from(r.width), fg(p.attention));
}
