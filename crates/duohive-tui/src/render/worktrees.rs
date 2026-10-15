//! The worktree list (UI §4.9).

use duohive_core::time::Millis;
use duohive_core::worktree::{WorktreeRow, WorktreeState};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::{LIGHT, draw_box, fg, put};
use crate::format::{format_age, pad_right, truncate_end, truncate_middle, width};
use crate::model::{UiModel, WorktreeList};
use crate::theme::Palette;

/// The mock-up's width; narrower screens get the screen less a margin.
const WIDTH: u16 = 100;
const MARGIN: u16 = 2;
/// Cells before the first column: the marker and a gap.
const PREFIX: u16 = 3;
const FOCUS: &str = "▌";
/// The longest an Agent column grows.
const AGENT_MAX: usize = 16;
/// Cells kept between Branch and State.
const BRANCH_GAP: usize = 2;
/// The least a Branch column keeps when the State words need room.
const BRANCH_MIN: usize = 12;
/// Border, header, a blank row, the check line, the hint, border.
const CHROME_ROWS: u16 = 6;
const HINT: &str = "enter open in agent · d remove (asks unless merged and clean) · esc back";
const EMPTY: &str = "No worktrees yet. An agent gets one when you give it work in a new worktree.";
const FAILED_REMOVAL: &str = "couldn't remove";

/// `1 file changed`, `2 files changed`.
pub(crate) fn files_changed(n: u32) -> String {
    let noun = if n == 1 { "file" } else { "files" };
    format!("{n} {noun} changed")
}

/// A state as words; never a glyph or a colour alone (P2-D16).
pub(crate) fn state_word(state: &WorktreeState) -> String {
    match state {
        WorktreeState::Unchecked => "not checked yet".to_owned(),
        WorktreeState::InUse => "in use".to_owned(),
        WorktreeState::Merged | WorktreeState::Removing => "merged · removing".to_owned(),
        WorktreeState::NotMerged { files_changed: n } => {
            format!("not merged · {}", files_changed(*n))
        }
        WorktreeState::MergedDirty => "merged · uncommitted changes, kept".to_owned(),
        // A failed removal already says what it is.
        WorktreeState::CheckFailed(reason) if reason.starts_with(FAILED_REMOVAL) => reason.clone(),
        WorktreeState::CheckFailed(reason) => format!("couldn't check · {reason}"),
    }
}

/// `checked 3m ago against origin/main`, from the latest check on the list.
fn check_line(rows: &[WorktreeRow], now: Millis) -> String {
    match rows
        .iter()
        .filter_map(|r| Some((r.checked_at?, r)))
        .max_by_key(|(t, _)| *t)
    {
        None => "not checked yet".to_owned(),
        Some((at, row)) => {
            let age = format_age(now - at);
            match &row.base {
                Some(base) => format!("checked {age} ago against {}", base.0),
                None => format!("checked {age} ago"),
            }
        }
    }
}

/// The three column widths (Agent, Branch, State) within `room` cells.
fn columns(rows: &[WorktreeRow], room: usize) -> (usize, usize, usize) {
    let longest = |f: &dyn Fn(&WorktreeRow) -> usize| rows.iter().map(f).max().unwrap_or(0);
    let agent = (longest(&|r| width(r.agent.as_str())).clamp(5, AGENT_MAX)) + 2;
    let want_branch = longest(&|r| width(&r.branch)).max(6) + 2;
    let want_state = longest(&|r| width(&state_word(&r.state))).max(5);
    let avail = room.saturating_sub(agent);
    let branch = if want_branch + want_state <= avail {
        want_branch
    } else {
        avail
            .saturating_sub(want_state)
            .max(BRANCH_MIN)
            .min(want_branch)
    };
    (agent, branch, avail.saturating_sub(branch))
}

/// Draws the list centred over `area`.
pub(super) fn draw(
    buf: &mut Buffer,
    area: Rect,
    l: &WorktreeList,
    m: &UiModel,
    now: Millis,
    p: &Palette,
) {
    let w = WIDTH.min(area.width.saturating_sub(MARGIN));
    let h = u16::try_from(m.worktrees.len().max(1))
        .unwrap_or(u16::MAX)
        .saturating_add(CHROME_ROWS)
        .min(area.height);
    if w < 20 || h < CHROME_ROWS + 1 {
        return;
    }
    let r = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    };
    let inside = Style::default().fg(p.fg).bg(p.bg_panel);
    for y in r.y..r.bottom() {
        put(
            buf,
            r.x,
            y,
            &" ".repeat(usize::from(w)),
            usize::from(w),
            inside,
        );
    }
    let title = format!(
        "Worktrees · {} · removed automatically once merged and clean",
        m.worktrees.len()
    );
    draw_box(buf, r, &title, &LIGHT, fg(p.border_focus), inside);
    let x = r.x + 1;
    let room = usize::from(w - 2);
    let muted = Style::default().fg(p.fg_muted).bg(p.bg_panel);
    let (agent_w, branch_w, state_w) =
        columns(&m.worktrees, room.saturating_sub(usize::from(PREFIX)));
    let cells = |a: &str, b: &str, c: &str| {
        format!(
            "{}{}{}",
            pad_right(&truncate_end(a, agent_w), agent_w),
            pad_right(
                &truncate_middle(b, branch_w.saturating_sub(BRANCH_GAP)),
                branch_w
            ),
            truncate_end(c, state_w)
        )
    };
    let indent = " ".repeat(usize::from(PREFIX));
    put(
        buf,
        x,
        r.y + 1,
        &format!("{indent}{}", cells("Agent", "Branch", "State")),
        room,
        muted,
    );
    let rows_h = usize::from(h - CHROME_ROWS);
    if m.worktrees.is_empty() {
        put(
            buf,
            x + PREFIX,
            r.y + 2,
            EMPTY,
            room.saturating_sub(usize::from(PREFIX)),
            muted,
        );
    }
    let cursor = l.cursor.min(m.worktrees.len().saturating_sub(1));
    let first = (cursor + 1).saturating_sub(rows_h);
    for (i, row) in m.worktrees.iter().enumerate().skip(first).take(rows_h) {
        let y = r.y + 2 + u16::try_from(i - first).unwrap_or(0);
        let selected = i == cursor;
        let base = if selected {
            Style::default().fg(p.fg).bg(p.selection)
        } else {
            Style::default().fg(p.fg).bg(p.bg_panel)
        };
        put(buf, x, y, &" ".repeat(room), room, base);
        let marker = if selected { FOCUS } else { " " };
        put(buf, x, y, marker, 1, base.fg(p.border_focus));
        let first_cols = format!(
            "{}{}",
            pad_right(&truncate_end(row.agent.as_str(), agent_w), agent_w),
            pad_right(
                &truncate_middle(&row.branch, branch_w.saturating_sub(BRANCH_GAP)),
                branch_w
            )
        );
        let at = x + PREFIX;
        let next = put(
            buf,
            at,
            y,
            &first_cols,
            room.saturating_sub(usize::from(PREFIX)),
            base,
        );
        let word = truncate_end(&state_word(&row.state), state_w);
        let left = usize::from(r.right().saturating_sub(1).saturating_sub(next));
        put(buf, next, y, &word, left, base.fg(p.fg_muted));
    }
    put(
        buf,
        x + 1,
        r.bottom() - 3,
        &check_line(&m.worktrees, now),
        room.saturating_sub(1),
        muted,
    );
    put(
        buf,
        x + 1,
        r.bottom() - 2,
        HINT,
        room.saturating_sub(1),
        muted,
    );
}
