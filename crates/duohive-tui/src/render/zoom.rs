//! The zoomed agent's header and footer (UI §4.2).

use duohive_core::names::AgentName;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;

use super::header::typing_footer;
use super::{fg, put};
use crate::format::{truncate_end, width};
use crate::model::UiModel;
use crate::theme::Palette;

/// The right end of the header; the trailing space is the screen's margin.
const RESTORE: &str = "z restore ";
const KEYS: &str = " enter type · z restore · PgUp/PgDn scroll";

/// ` DuoHive › <agent> (zoomed)`, with `z restore` at the right.
pub(super) fn draw_header(buf: &mut Buffer, r: Rect, agent: &AgentName, p: &Palette) {
    let w = usize::from(r.width);
    let x = put(
        buf,
        r.x,
        r.y,
        " DuoHive",
        w,
        fg(p.fg).add_modifier(Modifier::BOLD),
    );
    let used = usize::from(x - r.x);
    let rest = format!(" › {agent} (zoomed)");
    let hint = width(RESTORE);
    let room = w.saturating_sub(used + hint + 1);
    let rest = truncate_end(&rest, room);
    put(buf, x, r.y, &rest, w.saturating_sub(used), fg(p.fg));
    if w >= used + hint {
        let at = r.x + u16::try_from(w - hint).unwrap_or(0);
        put(buf, at, r.y, RESTORE, hint, fg(p.fg_muted));
    }
}

/// Typing's footer while typing, else the zoom keys (or the note, cut first) and the
/// Needs you count (UI §4.2); cut with `…` to the screen's width.
pub(super) fn draw_footer(buf: &mut Buffer, r: Rect, m: &UiModel, p: &Palette) {
    let w = usize::from(r.width);
    if m.typing() {
        return typing_footer(buf, r, m, p);
    }
    let need = need_you(m);
    let lead = match &m.footer_note {
        Some(note) => {
            let room = w.saturating_sub(width(&need) + 1).max(w / 2);
            format!(" {}", truncate_end(note, room))
        }
        None => KEYS.to_owned(),
    };
    let text = truncate_end(&format!("{lead}{need}"), w);
    // The Needs you count starts at its `◆`, after the separator.
    let split = text
        .char_indices()
        .find(|&(_, c)| c == '◆')
        .map_or(text.len(), |(i, _)| i);
    let (keys, need) = text.split_at(split);
    let x = put(buf, r.x, r.y, keys, w, fg(p.fg_muted));
    let room = w.saturating_sub(usize::from(x - r.x));
    put(buf, x, r.y, need, room, fg(p.attention));
}

/// ` · ◆ 2 need you (squad-3, billing-api)`, or nothing when no one needs you.
fn need_you(m: &UiModel) -> String {
    if m.needs_you.is_empty() {
        return String::new();
    }
    let names: Vec<String> = m.needs_you.iter().map(|i| i.agent.to_string()).collect();
    format!(" · ◆ {} need you ({})", names.len(), names.join(", "))
}
