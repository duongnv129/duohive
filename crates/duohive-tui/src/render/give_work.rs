//! The Give work dialog over the main screen (UI §4.5).

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use unicode_segmentation::UnicodeSegmentation;

use super::form::frame_box;
use super::{fg, put};
use crate::format::{format_age, width, wrap_words};
use crate::forms::{GiveFocus, GiveWork, Variant};
use crate::theme::Palette;
use duohive_core::time::Millis;
use duohive_core::worktree::{RunIn, TaskId};

/// The mock-ups' width; narrower screens get the screen less a margin.
const WIDTH: u16 = 90;
/// Cells between the border and the text.
const PAD: u16 = 1;
/// `Task  `, `Run   `: the label column.
const LABEL: u16 = 6;
/// A role's label column: `Goes to  ` (UI §4.5).
const ROLE_LABEL: u16 = 9;
const GOES_TO: &str = "Goes to";
const PICK_HINT: &str = "↑↓ pick another";
/// Cells after `↑↓ pick another`, as in the role mock-up.
const PICK_RIGHT: u16 = 17;
const NO_FREE: &str = "No free agent · enter proposes one";
const CARD_HOLDS: &str = "(the approval card holds the task)";
const GIVE_DISABLED: &str = "[ Give work ] (disabled)";
const SEND_AS_MESSAGE: &str = "[ Send as message instead ]";
const GAP: &str = "   ";
/// The task box shows this many rows and scrolls past them.
const MAX_TASK_ROWS: usize = 4;
const FOCUS: &str = "▌";
/// Cells after the hint, as in the mock-ups.
const HINT_RIGHT: u16 = 6;

/// The task wrapped at word boundaries to `cells` columns, and the cursor's `(row, column)`
/// in it. A word longer than a row is broken only as a last resort; spaces past the edge
/// hang there and are not drawn.
pub(super) fn wrap_task(text: &str, cursor: usize, cells: usize) -> (Vec<String>, (usize, usize)) {
    let gs: Vec<&str> = text.graphemes(true).collect();
    // `(start, end)` grapheme ranges, one per row; newlines belong to no row.
    let mut ranges = Vec::new();
    let mut line_start = 0;
    for (i, g) in gs.iter().enumerate() {
        if is_newline(g) {
            ranges.extend(wrap_line(&gs, line_start, i, cells));
            line_start = i + 1;
        }
    }
    ranges.extend(wrap_line(&gs, line_start, gs.len(), cells));
    let cols = |start: usize, end: usize| gs[start..end].iter().map(|g| width(g)).sum::<usize>();
    // The row holding the cursor: the next row at a soft break, the row a newline ends.
    let row = ranges
        .iter()
        .position(|&(s, e)| (s..e).contains(&cursor))
        .or_else(|| ranges.iter().position(|&(_, e)| e == cursor))
        .unwrap_or(ranges.len() - 1);
    let (start, _) = ranges[row];
    let mut at = (row, cols(start, cursor.min(gs.len())).min(cells));
    let mut rows: Vec<String> = ranges
        .iter()
        .map(|&(s, e)| {
            let mut used = 0;
            gs[s..e]
                .iter()
                .take_while(|g| {
                    used += width(g);
                    used <= cells
                })
                .copied()
                .collect()
        })
        .collect();
    if cursor >= gs.len() && at.1 >= cells {
        rows.push(String::new());
        at = (rows.len() - 1, 0);
    }
    (rows, at)
}

fn is_newline(g: &str) -> bool {
    g == "\n" || g == "\r\n"
}

/// The rows of the line `gs[start..end]` (no newlines in it), greedily by word.
fn wrap_line(gs: &[&str], start: usize, end: usize, cells: usize) -> Vec<(usize, usize)> {
    let is_space = |i: usize| gs[i].chars().all(char::is_whitespace);
    let mut out = Vec::new();
    let (mut row, mut col, mut i) = (start, 0, start);
    while i < end {
        let word_end = (i..end).find(|&j| is_space(j)).unwrap_or(end);
        let spaces_end = (word_end..end).find(|&j| !is_space(j)).unwrap_or(end);
        let word: usize = gs[i..word_end].iter().map(|g| width(g)).sum();
        if col > 0 && col + word > cells {
            out.push((row, i));
            (row, col) = (i, 0);
        }
        if word > cells {
            // The last resort: a word wider than a row is broken where the row ends.
            for (j, g) in gs.iter().enumerate().take(word_end).skip(i) {
                let w = width(g);
                if col > 0 && col + w > cells {
                    out.push((row, j));
                    (row, col) = (j, 0);
                }
                col += w;
            }
        } else {
            col += word;
        }
        col += gs[word_end..spaces_end]
            .iter()
            .map(|g| width(g))
            .sum::<usize>();
        i = spaces_end;
    }
    out.push((row, end));
    out
}

/// Draws the dialog centred over `area`; returns where the terminal cursor goes.
pub(super) fn draw(
    buf: &mut Buffer,
    area: Rect,
    g: &GiveWork,
    now: Millis,
    p: &Palette,
) -> Option<Position> {
    let label = label_cells(g);
    let w = WIDTH.min(area.width.saturating_sub(4));
    let text_w = usize::from(w.saturating_sub(2 * (1 + PAD) + label + 4));
    if text_w == 0 {
        return None;
    }
    let (rows, (crow, ccol)) = wrap_task(g.task.text(), g.task.cursor(), text_w);
    let first = (crow + 1).saturating_sub(MAX_TASK_ROWS);
    let shown: Vec<&String> = rows.iter().skip(first).take(MAX_TASK_ROWS).collect();
    let body = body_rows(g, usize::from(w.saturating_sub(2 * (1 + PAD) + label)), now);
    let note = usize::from(g.note.is_some());
    // The busy variant ends on its buttons, as UI §4.5's mock; the footer has its keys.
    let hint_row = usize::from(g.variant != Variant::Busy);
    let inner = shown.len() + note + body.len() + hint_row;
    let r = frame_box(buf, area, w, inner, &g.title(), p);
    let x0 = r.x + 1 + PAD;
    let right = r.right().saturating_sub(1);
    let at = |x: u16, y: u16, s: &str, st: Style, buf: &mut Buffer| {
        put(buf, x, y, s, usize::from(right.saturating_sub(x)), st)
    };
    let task_focus = g.focus == GiveFocus::Task;
    let task_label = format!("{:<1$}", "Task", usize::from(label - 1));
    let mut y = r.y + 1;
    for (i, row) in shown.iter().enumerate() {
        let lead = if i == 0 {
            task_label.clone()
        } else {
            " ".repeat(task_label.len())
        };
        let mut x = at(x0, y, &lead, fg(p.fg), buf);
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
    if hint_row == 1 {
        let hint = g.hint();
        let hx = right.saturating_sub(HINT_RIGHT + cells(hint));
        at(
            hx.max(x0),
            r.bottom().saturating_sub(2),
            hint,
            fg(p.fg_muted),
            buf,
        );
    }
    let row = crow.checked_sub(first)?;
    task_focus.then(|| Position {
        x: x0 + label + 2 + u16::try_from(ccol).unwrap_or(0),
        y: r.y + 1 + u16::try_from(row).unwrap_or(0),
    })
}

/// The label column: `Task  `, or a role's wider `Goes to  ` (UI §4.5's mocks).
fn label_cells(g: &GiveWork) -> u16 {
    if g.role().is_some() {
        ROLE_LABEL
    } else {
        LABEL
    }
}

fn cells(s: &str) -> u16 {
    u16::try_from(width(s)).unwrap_or(u16::MAX)
}

/// A line under the task box.
enum Body {
    /// The Run choice.
    Run,
    /// Muted text at the label column.
    Muted(String),
    /// Text at the label column, after its label when it has one.
    Labelled(Option<&'static str>, String),
    /// A role's `Goes to`: the picked agent and how long it has been at rest.
    GoesTo(String),
    /// Text from the left edge.
    Plain(String),
    /// The busy variant's buttons.
    Buttons,
}

/// The lines under the task box for the dialog's variant; `cells` is the room after the label.
fn body_rows(g: &GiveWork, cells: usize, now: Millis) -> Vec<Body> {
    match &g.variant {
        Variant::Fresh => run_choice(g),
        Variant::NextTurn { worktree } => next_turn(g, worktree.as_ref(), cells),
        Variant::Busy => {
            let line = format!(
                "{agent} is {}. Give work when it is at rest, or send this as a message: \
                 it is typed in when {agent} is at rest.",
                g.state.word(),
                agent = g.agent,
            );
            let mut out = plain_lines(&line, cells + usize::from(label_cells(g)));
            out.push(Body::Buttons);
            out
        }
        Variant::Pending => vec![Body::Plain(format!(
            "{} is still taking the last task",
            g.agent
        ))],
        Variant::Role { .. } => {
            let at_rest = format_age(now - g.since);
            let mut out = vec![Body::GoesTo(format!(
                "{} ({} {at_rest})",
                g.agent,
                g.state.word()
            ))];
            if g.has_run_choice() {
                out.extend(run_choice(g));
            } else {
                out.extend(next_turn(g, g.pick_worktree(), cells));
            }
            out
        }
        Variant::RoleNoneFree { .. } => vec![
            Body::Labelled(Some(GOES_TO), NO_FREE.to_owned()),
            Body::Labelled(None, CARD_HOLDS.to_owned()),
        ],
    }
}

/// The Run radios, why `new worktree` can't be chosen, and what a fresh session does.
fn run_choice(g: &GiveWork) -> Vec<Body> {
    let mut out = vec![Body::Run];
    if let Some(why) = &g.worktree_blocked {
        out.push(Body::Muted(why.clone()));
    }
    out.push(Body::Plain("Starts a fresh session.".to_owned()));
    out
}

/// `Run   in <where>, as the next turn of <agent>'s session`.
fn next_turn(g: &GiveWork, worktree: Option<&TaskId>, cells: usize) -> Vec<Body> {
    let place = match (worktree, &g.dir) {
        (Some(id), _) => format!("worktree {}", id.0),
        (None, Some(d)) => d.clone(),
        (None, None) => "its directory".to_owned(),
    };
    let line = format!("in {place}, as the next turn of {}'s session", g.agent);
    wrap_words(&line, cells)
        .into_iter()
        .enumerate()
        .map(|(i, l)| Body::Labelled((i == 0).then_some("Run"), l))
        .collect()
}

fn plain_lines(text: &str, cells: usize) -> Vec<Body> {
    wrap_words(text, cells)
        .into_iter()
        .map(Body::Plain)
        .collect()
}

fn draw_body(buf: &mut Buffer, row: &Body, pos: (u16, u16, u16), g: &GiveWork, p: &Palette) {
    let (x0, y, right) = pos;
    let label = label_cells(g);
    let at = |x: u16, s: &str, st: Style, buf: &mut Buffer| {
        put(buf, x, y, s, usize::from(right.saturating_sub(x)), st)
    };
    match row {
        Body::Plain(t) => {
            at(x0, t, fg(p.fg), buf);
        }
        Body::Muted(t) => {
            at(x0 + label, t, fg(p.fg_muted), buf);
        }
        Body::Labelled(name, t) => {
            if let Some(name) = name {
                at(x0, name, fg(p.fg), buf);
            }
            at(x0 + label, t, fg(p.fg), buf);
        }
        Body::GoesTo(t) => {
            at(x0, GOES_TO, fg(p.fg), buf);
            let mark = if g.focus == GiveFocus::Pick {
                FOCUS
            } else {
                " "
            };
            let x = at(x0 + label, mark, fg(p.border_focus), buf);
            let x = at(x, t, fg(p.fg), buf);
            let hx = right.saturating_sub(PICK_RIGHT + cells(PICK_HINT));
            at(hx.max(x + 3), PICK_HINT, fg(p.fg_muted), buf);
        }
        Body::Run => draw_run(buf, (x0, y, right), g, p),
        Body::Buttons => draw_buttons(buf, (x0, y, right), g, p),
    }
}

/// `[ Give work ] (disabled)   ▌[ Send as message instead ]`, centred.
fn draw_buttons(buf: &mut Buffer, pos: (u16, u16, u16), g: &GiveWork, p: &Palette) {
    let (x0, y, right) = pos;
    let at = |x: u16, s: &str, st: Style, buf: &mut Buffer| {
        put(buf, x, y, s, usize::from(right.saturating_sub(x)), st)
    };
    let mark = if g.focus == GiveFocus::SendAsMessage {
        FOCUS
    } else {
        " "
    };
    let all = cells(GIVE_DISABLED) + cells(GAP) + cells(mark) + cells(SEND_AS_MESSAGE);
    let room = right.saturating_sub(x0);
    let mut x = x0 + room.saturating_sub(all) / 2;
    x = at(x, GIVE_DISABLED, fg(p.fg_muted), buf);
    x = at(x, GAP, fg(p.fg), buf);
    x = at(x, mark, fg(p.border_focus), buf);
    let send = if g.can_give() { p.fg } else { p.fg_muted };
    at(x, SEND_AS_MESSAGE, fg(send), buf);
}

/// `Run   ( ) new worktree   (•) ~/code/pay-qa        agent default: directory`.
fn draw_run(buf: &mut Buffer, pos: (u16, u16, u16), g: &GiveWork, p: &Palette) {
    let (x0, y, right) = pos;
    let at = |x: u16, s: &str, st: Style, buf: &mut Buffer| {
        put(buf, x, y, s, usize::from(right.saturating_sub(x)), st)
    };
    let run_label = format!("{:<1$}", "Run", usize::from(label_cells(g) - 1));
    let mut x = at(x0, &run_label, fg(p.fg), buf);
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
    let aligned = right.saturating_sub(HINT_RIGHT - 2 + cells(default));
    at(aligned.max(x + 3), default, fg(p.fg_muted), buf);
}
