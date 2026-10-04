//! Banners, the header line and the footer (UI §4.1, §4.12, D29).

use duohive_core::ports::OrgFileError;
use duohive_core::status::AgentState;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::{RenderCtx, fg, put};
use crate::model::UiModel;
use crate::theme::Palette;

/// Phase 1 keys (D8): no `? help`, no `space` until phase 2.
const FOOTER_KEYS: &str =
    " ↑↓ move · s start/stop · n new · e edit · d delete · v replies · q quit";

/// One banner line per broken file: `org.toml` first, then `settings.toml`.
pub(super) fn banners(m: &UiModel) -> Vec<String> {
    let org = m.org_error.as_ref().map(|e| {
        let where_ = located("org.toml", e);
        format!(
            " ✕ {where_}: {} · using your last good org · o open in $EDITOR",
            e.message
        )
    });
    let settings = m.settings_error.as_ref().map(|e| {
        let where_ = located("settings.toml", e);
        format!(" ✕ {where_}: {} · using default settings", e.message)
    });
    org.into_iter().chain(settings).collect()
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
        .agents
        .values()
        .filter(|v| v.status.state == AgentState::Working && !v.status.quiet)
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

/// The footer: the note when one is set, else the keys.
pub(super) fn draw_footer(buf: &mut Buffer, r: Rect, m: &UiModel, p: &Palette) {
    let text = match &m.footer_note {
        Some(note) => format!(" {note}"),
        None => FOOTER_KEYS.to_owned(),
    };
    put(buf, r.x, r.y, &text, usize::from(r.width), fg(p.fg_muted));
}
