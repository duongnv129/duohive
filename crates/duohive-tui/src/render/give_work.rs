//! The Give work dialog over the main screen (UI §4.5).

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use unicode_segmentation::UnicodeSegmentation;

use super::form::frame_box;
use super::{fg, put};
use crate::format::{width, wrap_words};
use crate::forms::{GiveFocus, GiveWork, Variant};
use crate::theme::Palette;
use duohive_core::worktree::RunIn;

/// The mock-ups' width; narrower screens get the screen less a margin.
const WIDTH: u16 = 90;
/// Cells between the border and the text.
const PAD: u16 = 1;
/// `Task  `, `Run   `: the label column.
const LABEL: u16 = 6;
/// The task box shows this many rows and scrolls past them.
const MAX_TASK_ROWS: usize = 4;
const FOCUS: &str = "▌";
/// Cells after the hint, as in the mock-ups.
const HINT_RIGHT: u16 = 6;

/// The task wrapped to `cells` columns, and the cursor's `(row, column)` in it.
fn wrap_task(text: &str, cursor: usize, cells: usize) -> (Vec<String>, (usize, usize)) {
    let mut rows = vec![String::new()];
    let (mut col, mut at) = (0, (0, 0));
    let mut count = 0;
    for (i, g) in text.graphemes(true).enumerate() {
        count = i + 1;
        let newline = g == "\n" || g == "\r\n";
        let w = width(g);
        if !newline && col + w > cells && col > 0 {
            rows.push(String::new());
            col = 0;
        }
        if i == cursor {
            at = (rows.len() - 1, col);
        }
        if newline {
            rows.push(String::new());
            col = 0;
        } else if let Some(row) = rows.last_mut() {
            row.push_str(g);
            col += w;
        }
    }
    if cursor >= count {
        if col >= cells {
            rows.push(String::new());
            col = 0;
        }
        at = (rows.len() - 1, col);
    }
    (rows, at)
}

/// Draws the dialog centred over `area`; returns where the terminal cursor goes.
pub(super) fn draw(buf: &mut Buffer, area: Rect, g: &GiveWork, p: &Palette) -> Option<Position> {
    let w = WIDTH.min(area.width.saturating_sub(4));
    let text_w = usize::from(w.saturating_sub(2 * (1 + PAD) + LABEL + 4));
    if text_w == 0 {
        return None;
    }
    let (rows, (crow, ccol)) = wrap_task(g.task.text(), g.task.cursor(), text_w);
    let first = (crow + 1).saturating_sub(MAX_TASK_ROWS);
    let shown: Vec<&String> = rows.iter().skip(first).take(MAX_TASK_ROWS).collect();
    let body = body_rows(g, usize::from(w.saturating_sub(2 * (1 + PAD) + LABEL)));
    let note = usize::from(g.note.is_some());
    let inner = shown.len() + note + body.len() + 1;
    let r = frame_box(buf, area, w, inner, &g.title(), p);
    let x0 = r.x + 1 + PAD;
    let right = r.right().saturating_sub(1);
    let at = |x: u16, y: u16, s: &str, st: Style, buf: &mut Buffer| {
        put(buf, x, y, s, usize::from(right.saturating_sub(x)), st)
    };
    let task_focus = g.focus == GiveFocus::Task;
    let mut y = r.y + 1;
    for (i, row) in shown.iter().enumerate() {
        let label = if i == 0 { "Task " } else { "     " };
        let mut x = at(x0, y, label, fg(p.fg), buf);
        let mark = if task_focus && i == 0 { FOCUS } else { " " };
        x = at(x, y, mark, fg(p.border_focus), buf);
        x = at(x, y, "[ ", fg(p.fg), buf);
        x = at(x, y, row, fg(p.fg), buf);
        let fill = "_".repeat(text_w.saturating_sub(width(row)));
        x = at(x, y, &fill, fg(p.fg_muted), buf);
        at(x, y, " ]", fg(p.fg), buf);
        y += 1;
    }
    if let Some(n) = &g.note {
        at(x0, y, n, fg(p.attention), buf);
        y += 1;
    }
    for row in &body {
        draw_body(buf, row, (x0, y, right), g, p);
        y += 1;
    }
    let hint = g.hint();
    let hx = right.saturating_sub(HINT_RIGHT + u16::try_from(width(hint)).unwrap_or(0));
    at(
        hx.max(x0),
        r.bottom().saturating_sub(2),
        hint,
        fg(p.fg_muted),
        buf,
    );
    let row = crow.checked_sub(first)?;
    task_focus.then(|| Position {
        x: x0 + LABEL + 2 + u16::try_from(ccol).unwrap_or(0),
        y: r.y + 1 + u16::try_from(row).unwrap_or(0),
    })
}

/// A line under the task box.
enum Body {
    /// The Run choice.
    Run,
    /// Muted text at the label column.
    Muted(String),
    /// Text at the label column, after `Run`.
    RunText(String, bool),
    /// Text from the left edge.
    Plain(String),
}

/// The lines under the task box for the dialog's variant; `cells` is the room after the label.
fn body_rows(g: &GiveWork, cells: usize) -> Vec<Body> {
    match &g.variant {
        Variant::Fresh => {
            let mut out = vec![Body::Run];
            if let Some(why) = &g.worktree_blocked {
                out.push(Body::Muted(why.clone()));
            }
            out.push(Body::Plain("Starts a fresh session.".to_owned()));
            out
        }
        Variant::NextTurn { worktree } => {
            let place = match (worktree, &g.dir) {
                (Some(id), _) => format!("worktree {}", id.0),
                (None, Some(d)) => d.clone(),
                (None, None) => "its directory".to_owned(),
            };
            let line = format!("in {place}, as the next turn of {}'s session", g.agent);
            wrap_words(&line, cells)
                .into_iter()
                .enumerate()
                .map(|(i, l)| Body::RunText(l, i == 0))
                .collect()
        }
        Variant::Busy => {
            let line = format!(
                "{} is {}. Give work when it is at rest.",
                g.agent,
                g.state.word()
            );
            plain_lines(&line, cells + usize::from(LABEL))
        }
        Variant::Pending => vec![Body::Plain(format!(
            "{} is still taking the last task",
            g.agent
        ))],
    }
}

fn plain_lines(text: &str, cells: usize) -> Vec<Body> {
    wrap_words(text, cells)
        .into_iter()
        .map(Body::Plain)
        .collect()
}

fn draw_body(buf: &mut Buffer, row: &Body, pos: (u16, u16, u16), g: &GiveWork, p: &Palette) {
    let (x0, y, right) = pos;
    let at = |x: u16, s: &str, st: Style, buf: &mut Buffer| {
        put(buf, x, y, s, usize::from(right.saturating_sub(x)), st)
    };
    match row {
        Body::Plain(t) => {
            at(x0, t, fg(p.fg), buf);
        }
        Body::Muted(t) => {
            at(x0 + LABEL, t, fg(p.fg_muted), buf);
        }
        Body::RunText(t, first) => {
            if *first {
                at(x0, "Run", fg(p.fg), buf);
            }
            at(x0 + LABEL, t, fg(p.fg), buf);
        }
        Body::Run => draw_run(buf, (x0, y, right), g, p),
    }
}

/// `Run   ( ) new worktree   (•) ~/code/pay-qa        agent default: directory`.
fn draw_run(buf: &mut Buffer, pos: (u16, u16, u16), g: &GiveWork, p: &Palette) {
    let (x0, y, right) = pos;
    let at = |x: u16, s: &str, st: Style, buf: &mut Buffer| {
        put(buf, x, y, s, usize::from(right.saturating_sub(x)), st)
    };
    let mut x = at(x0, "Run  ", fg(p.fg), buf);
    let mark = if g.focus == GiveFocus::Run {
        FOCUS
    } else {
        " "
    };
    x = at(x, mark, fg(p.border_focus), buf);
    let radio = |on: bool| if on { "(•)" } else { "( )" };
    let wt_style = if g.worktree_blocked.is_some() {
        fg(p.fg_muted)
    } else {
        fg(p.fg)
    };
    let wt = format!("{} new worktree", radio(g.run == RunIn::NewWorktree));
    x = at(x, &wt, wt_style, buf);
    let dir = g.dir.as_deref().unwrap_or("no directory set");
    let in_dir = format!("   {} {dir}", radio(g.run == RunIn::Directory));
    x = at(x, &in_dir, fg(p.fg), buf);
    let default = if g.default_worktree {
        "agent default: worktree"
    } else {
        "agent default: directory"
    };
    let aligned = right.saturating_sub(HINT_RIGHT - 2 + u16::try_from(width(default)).unwrap_or(0));
    at(aligned.max(x + 3), default, fg(p.fg_muted), buf);
}
