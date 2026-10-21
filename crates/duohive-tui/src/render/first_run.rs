//! The first-run wizard and the no-CLI blocking screen (UI §4.3).

use std::collections::BTreeMap;

use duohive_core::cli_kind::CliKind;
use duohive_core::inherit::expand_home;
use duohive_core::validate::DirStatus;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};

use super::{LIGHT, RenderCtx, draw_box, fg, put};
use crate::first_run::{FirstRun, Step, TemplateChoice, errors, form_error, trimmed};
use crate::format::width;
use crate::forms::LineEdit;
use crate::model::UiModel;
use crate::theme::Palette;

const WIDTH: u16 = 72;
const MARGIN: u16 = 4;
const FOCUS: &str = "▌";
const LABEL_W: u16 = 14;
const INNER: usize = 28;
/// Engineering Org rows: the name box, the repo box and the status after them.
const ENG_NAME: usize = 18;
const ENG_REPO: usize = 24;
const ENG_REPO_DX: u16 = 23;
const ENG_STATUS_DX: u16 = 50;
/// Where a labelled field's error or directory note goes.
const CELL_DX: u16 = 2 + 14 + 1 + 28 + 2;

/// Draws the wizard (or the blocking screen); returns where the cursor goes.
pub(super) fn draw(buf: &mut Buffer, area: Rect, m: &UiModel, ctx: &RenderCtx) -> Option<Position> {
    let w = m.wizard.as_ref()?;
    let p = &ctx.palette;
    if w.blocked() {
        blocked(buf, area, p);
        return None;
    }
    let rows = row_count(w);
    let r = frame(buf, area, rows, &title(w), p);
    let mut line = Line {
        buf,
        x0: r.x + 1,
        y: r.y + 1,
        right: r.right() - 1,
    };
    match w.step {
        Step::You => you(&mut line, w, p),
        Step::Template => template(&mut line, w, p),
        Step::Form => form(&mut line, w, m, p),
    }
}

struct Line<'a> {
    buf: &'a mut Buffer,
    x0: u16,
    y: u16,
    right: u16,
}

impl Line<'_> {
    /// Writes at `dx` from the content's left edge and returns the next `dx`.
    fn put(&mut self, dx: u16, s: &str, style: Style) -> u16 {
        let x = self.x0 + dx;
        let room = usize::from(self.right.saturating_sub(x));
        put(self.buf, x, self.y, s, room, style);
        dx + u16::try_from(width(s).min(room)).unwrap_or(u16::MAX)
    }

    /// The content's width in cells.
    fn width(&self) -> u16 {
        self.right.saturating_sub(self.x0)
    }

    fn text(&mut self, s: &str, color: Color) {
        self.put(2, s, fg(color));
    }

    fn down(&mut self) {
        self.y += 1;
    }
}

fn frame(buf: &mut Buffer, area: Rect, rows: usize, title: &str, p: &Palette) -> Rect {
    let w = WIDTH.min(area.width.saturating_sub(MARGIN));
    let h = u16::try_from(rows)
        .unwrap_or(u16::MAX)
        .saturating_add(2)
        .min(area.height);
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
    draw_box(buf, r, title, &LIGHT, fg(p.border_focus), inside);
    r
}

fn title(w: &FirstRun) -> String {
    let (step, of) = match w.step {
        Step::You => (1, 3),
        Step::Template if w.replace => (1, 2),
        Step::Template => (2, 3),
        Step::Form if w.replace => (2, 2),
        Step::Form => (3, 3),
    };
    match w.step {
        Step::You => format!("Welcome to DuoHive · step {step} of {of}"),
        Step::Template => format!("Choose a starting org · step {step} of {of}"),
        Step::Form => format!("Set up {} · step {step} of {of}", w.template().name()),
    }
}

fn row_count(w: &FirstRun) -> usize {
    match w.step {
        Step::You => 10,
        Step::Template => 8 + preview(w).len(),
        Step::Form => match w.template() {
            TemplateChoice::AgentSquad => 9,
            TemplateChoice::ProductTeam => 10,
            TemplateChoice::EngineeringOrg => 9 + w.engineering.rows.len(),
            TemplateChoice::Blank => 5,
        },
    }
}

fn you(line: &mut Line, w: &FirstRun, p: &Palette) -> Option<Position> {
    line.down();
    line.text("DuoHive runs your AI coding agents as a team.", p.fg);
    line.down();
    line.text("Duo, your manager agent, helps you steer it.", p.fg);
    line.down();
    line.down();
    let cursor = field(line, "Your name", &w.you, w.focus == 0, p, None);
    line.down();
    cli_line(line, "Duo runs on", w.duo_cli, w, w.focus == 1, p);
    line.down();
    line.text(&found_line(w), p.fg_muted);
    line.down();
    if w.ask_quit {
        line.text("Quit DuoHive?  y quit · n stay", p.attention);
    }
    line.down();
    buttons(line, w, p, "enter next · esc quit (asks first)", true);
    cursor
}

fn template(line: &mut Line, w: &FirstRun, p: &Palette) -> Option<Position> {
    line.down();
    for (i, choice) in TemplateChoice::ALL.iter().enumerate() {
        let selected = i == w.cursor;
        line.put(0, if selected { FOCUS } else { " " }, fg(p.border_focus));
        let style = if selected {
            p.selected().fg(p.fg)
        } else {
            fg(p.fg_muted)
        };
        line.put(1, &format!("{:<17}", choice.name()), style);
        line.put(18, choice.blurb(), style);
        line.down();
    }
    line.down();
    line.text("┄ Preview ┄", p.fg_muted);
    line.down();
    for (text, muted) in preview(w) {
        line.text(&text, if muted { p.fg_muted } else { p.fg });
        line.down();
    }
    let hint = if w.replace {
        "↑↓ choose · enter · esc cancel"
    } else {
        "↑↓ choose · enter · esc"
    };
    buttons(line, w, p, hint, true);
    None
}

/// The template's tree, as step 2 previews it.
fn preview(w: &FirstRun) -> Vec<(String, bool)> {
    let mut out = vec![("⬢ Duo".to_owned(), false)];
    match w.template() {
        TemplateChoice::AgentSquad => {
            out.push(("└─ ◇ Generalist".to_owned(), false));
            let size = w
                .squad
                .size
                .text()
                .trim()
                .parse::<u32>()
                .unwrap_or(3)
                .clamp(1, 6);
            for i in 1..=size {
                let branch = if i == size { "   └─" } else { "   ├─" };
                out.push((format!("{branch} squad-{i}"), false));
            }
        }
        TemplateChoice::ProductTeam => {
            let name = trimmed(&w.product.name).unwrap_or("Product");
            out.push((format!("└─ ▣ {name}"), false));
            for (i, role) in ["Backend", "Frontend", "QA"].iter().enumerate() {
                let branch = if i == 2 { "   └─" } else { "   ├─" };
                out.push((format!("{branch} ◇ {role}"), false));
            }
        }
        TemplateChoice::EngineeringOrg => {
            let rows: Vec<&str> = w
                .engineering
                .rows
                .iter()
                .filter_map(|r| trimmed(&r.name))
                .collect();
            let rows = if rows.is_empty() {
                vec!["(your teams)"]
            } else {
                rows
            };
            for (i, name) in rows.iter().enumerate() {
                let branch = if i + 1 == rows.len() {
                    "└─"
                } else {
                    "├─"
                };
                out.push((format!("{branch} ▣ {name}"), false));
            }
        }
        TemplateChoice::Blank => out.push(("You and Duo only.".to_owned(), true)),
    }
    out
}

fn form(line: &mut Line, w: &FirstRun, m: &UiModel, p: &Palette) -> Option<Position> {
    let dirs = m.dirs.probe();
    let errs = errors(w, dirs, &m.home);
    // A refusal from the engine (a failed write) sits where a form problem would.
    let problem = w
        .write_error
        .clone()
        .or_else(|| form_error(w, dirs, &m.home));
    line.down();
    let cursor = match w.template() {
        TemplateChoice::AgentSquad => squad(line, w, p, &errs, m),
        TemplateChoice::ProductTeam => product(line, w, p, &errs, m),
        TemplateChoice::EngineeringOrg => engineering(line, w, p, &errs, m),
        TemplateChoice::Blank => {
            line.text(
                "You and Duo only. Add teams, roles and agents later with n.",
                p.fg,
            );
            line.down();
            None
        }
    };
    if let Some(problem) = &problem {
        line.text(&format!("✕ {problem}"), p.error);
        line.down();
    }
    line.down();
    let hint = match w.template() {
        TemplateChoice::AgentSquad | TemplateChoice::Blank => "You can change all of this later.",
        TemplateChoice::ProductTeam => {
            "Each role gets one agent and a starter prompt you can edit."
        }
        TemplateChoice::EngineeringOrg => {
            "Each team gets Backend and QA roles with one agent each."
        }
    };
    line.text(hint, p.fg_muted);
    line.down();
    buttons(
        line,
        w,
        p,
        "tab · enter · esc",
        problem.is_none() && errs.is_empty(),
    );
    cursor
}

fn squad(
    line: &mut Line,
    w: &FirstRun,
    p: &Palette,
    errs: &BTreeMap<usize, String>,
    m: &UiModel,
) -> Option<Position> {
    let mut cursor = field(line, "Agents", &w.squad.size, w.focus == 0, p, errs.get(&0));
    line.down();
    cursor = cursor.or(dir_field(
        line,
        "Directory",
        &w.squad.directory,
        w.focus == 1,
        p,
        m,
        errs.get(&1),
    ));
    line.down();
    cli_line(line, "CLI", w.squad.cli, w, w.focus == 2, p);
    line.down();
    check(
        line,
        "Worktrees",
        "own worktree per task",
        w.squad.worktrees,
        w.focus == 3,
        p,
        errs.get(&3),
    );
    line.down();
    cursor.or(field(
        line,
        "Role prompt",
        &w.squad.prompt,
        w.focus == 4,
        p,
        None,
    ))
}

fn product(
    line: &mut Line,
    w: &FirstRun,
    p: &Palette,
    errs: &BTreeMap<usize, String>,
    m: &UiModel,
) -> Option<Position> {
    let mut cursor = field(
        line,
        "Team name",
        &w.product.name,
        w.focus == 0,
        p,
        errs.get(&0),
    );
    line.down();
    cli_line(line, "CLI", w.product.cli, w, w.focus == 1, p);
    line.down();
    for (i, label) in ["Backend dir", "Frontend dir", "QA dir"].iter().enumerate() {
        cursor = cursor.or(dir_field(
            line,
            label,
            &w.product.directories[i],
            w.focus == i + 2,
            p,
            m,
            errs.get(&(i + 2)),
        ));
        line.down();
    }
    check(
        line,
        "Worktrees",
        "own worktree per task",
        w.product.worktrees,
        w.focus == 5,
        p,
        errs.get(&5),
    );
    line.down();
    cursor
}

fn engineering(
    line: &mut Line,
    w: &FirstRun,
    p: &Palette,
    errs: &BTreeMap<usize, String>,
    m: &UiModel,
) -> Option<Position> {
    cli_line(line, "CLI", w.engineering.cli, w, w.focus == 0, p);
    line.down();
    line.put(2, "name", fg(p.fg_muted));
    line.put(ENG_REPO_DX, "repo", fg(p.fg_muted));
    line.down();
    let mut cursor = None;
    let rows = w.engineering.rows.len();
    for i in 0..rows {
        let row = &w.engineering.rows[i];
        line.put(
            0,
            if w.focus == 1 + 2 * i { FOCUS } else { " " },
            fg(p.border_focus),
        );
        cursor = cursor.or(boxed_w(
            line,
            2,
            ENG_NAME,
            &row.name,
            w.focus == 1 + 2 * i,
            p,
        ));
        cursor = cursor.or(boxed_w(
            line,
            ENG_REPO_DX,
            ENG_REPO,
            &row.repo,
            w.focus == 2 + 2 * i,
            p,
        ));
        let error = errs.get(&(1 + 2 * i)).or(errs.get(&(2 + 2 * i)));
        let status = match error {
            Some(e) => format!("✕ {e}"),
            None => dir_suffix(&row.repo, m),
        };
        line.put(ENG_STATUS_DX, &status, suffix_style(&status, p));
        line.down();
    }
    let add = 2 * rows + 1;
    let add_style = if w.focus == add {
        p.selected().fg(p.fg)
    } else {
        fg(p.fg)
    };
    line.put(4, "[ + Add team ]", add_style);
    line.put(20, "empty rows are ignored", fg(p.fg_muted));
    line.down();
    check(
        line,
        "Worktrees",
        "own worktree per task",
        w.engineering.worktrees,
        w.focus == 2 * rows + 2,
        p,
        errs.get(&(2 * rows + 2)),
    );
    line.down();
    cursor
}

/// A labelled text field; its error follows the box in red.
fn field(
    line: &mut Line,
    label: &str,
    edit: &LineEdit,
    focused: bool,
    p: &Palette,
    error: Option<&String>,
) -> Option<Position> {
    line.put(0, if focused { FOCUS } else { " " }, fg(p.border_focus));
    if !label.is_empty() {
        line.put(
            2,
            &format!("{label:<width$}", width = usize::from(LABEL_W)),
            fg(p.fg),
        );
    }
    let at = boxed(line, 2 + LABEL_W, edit, focused, p);
    if let Some(e) = error {
        line.put(CELL_DX, &format!("✕ {e}"), fg(p.error));
    }
    at
}

/// A directory field: its box, then `✓ git repo` or its error where the note goes.
fn dir_field(
    line: &mut Line,
    label: &str,
    edit: &LineEdit,
    focused: bool,
    p: &Palette,
    m: &UiModel,
    error: Option<&String>,
) -> Option<Position> {
    line.put(0, if focused { FOCUS } else { " " }, fg(p.border_focus));
    line.put(
        2,
        &format!("{label:<width$}", width = usize::from(LABEL_W)),
        fg(p.fg),
    );
    let at = boxed(line, 2 + LABEL_W, edit, focused, p);
    let text = match error {
        Some(e) => format!("✕ {e}"),
        None => dir_suffix(edit, m),
    };
    line.put(CELL_DX, &text, suffix_style(&text, p));
    at
}

/// `[ text ]` at `dx`; the cursor sits after the text when focused.
fn boxed(
    line: &mut Line,
    dx: u16,
    edit: &LineEdit,
    focused: bool,
    p: &Palette,
) -> Option<Position> {
    boxed_w(line, dx, INNER, edit, focused, p)
}

/// `[ text ]` `cells` wide at `dx`.
fn boxed_w(
    line: &mut Line,
    dx: u16,
    cells: usize,
    edit: &LineEdit,
    focused: bool,
    p: &Palette,
) -> Option<Position> {
    // Only the focused box scrolls to the cursor; the others show their text's start.
    let (shown, at) = if focused {
        edit.window(cells)
    } else {
        (edit.head(cells), 0)
    };
    let border = if focused { p.border_focus } else { p.border };
    line.put(dx, "[", fg(border));
    let after = line.put(dx + 1, &format!("{shown:<cells$}"), fg(p.fg));
    line.put(after, "]", fg(border));
    focused.then(|| Position {
        x: dx + 1 + u16::try_from(at).unwrap_or(0),
        y: line.y,
    })
}

/// `(•) Claude Code  (-) OpenCode`, the missing one muted.
fn cli_line(
    line: &mut Line,
    label: &str,
    selected: CliKind,
    w: &FirstRun,
    focused: bool,
    p: &Palette,
) {
    line.put(0, if focused { FOCUS } else { " " }, fg(p.border_focus));
    line.put(
        2,
        &format!("{label:<width$}", width = usize::from(LABEL_W)),
        fg(p.fg),
    );
    let mut dx = 2 + LABEL_W;
    for cli in CliKind::ALL {
        let found = w.available(cli);
        let mark = if !found {
            "(-)"
        } else if cli == selected {
            "(•)"
        } else {
            "( )"
        };
        let color = if !found {
            p.fg_muted
        } else if cli == selected && focused {
            p.border_focus
        } else {
            p.fg
        };
        dx = line.put(dx, &format!("{mark} {}", cli.display()), fg(color)) + 3;
    }
}

/// `[x] label  note`, with its error after the note.
fn check(
    line: &mut Line,
    label: &str,
    note: &str,
    on: bool,
    focused: bool,
    p: &Palette,
    error: Option<&String>,
) {
    line.put(0, if focused { FOCUS } else { " " }, fg(p.border_focus));
    line.put(
        2,
        &format!("{label:<width$}", width = usize::from(LABEL_W)),
        fg(p.fg),
    );
    let mark = if on { "[x]" } else { "[ ]" };
    let x = line.put(
        2 + LABEL_W,
        mark,
        fg(if focused { p.border_focus } else { p.fg }),
    );
    line.put(x + 1, note, fg(p.fg_muted));
    if let Some(e) = error {
        line.put(
            2 + LABEL_W + 4 + u16::try_from(width(note)).unwrap_or(0) + 2,
            &format!("✕ {e}"),
            fg(p.error),
        );
    }
}

/// The step's buttons at the right, with a hint at the left.
fn buttons(line: &mut Line, w: &FirstRun, p: &Palette, hint: &str, can_create: bool) {
    use crate::first_run::Button;
    let labels: Vec<String> = w.buttons().iter().map(|b| format!("[ {b} ]")).collect();
    let total: usize = labels.iter().map(|l| width(l)).sum::<usize>() + 3 * (labels.len() - 1);
    let start = usize::from(line.width().saturating_sub(6)).saturating_sub(total);
    let start = u16::try_from(start).unwrap_or(line.width());
    line.put(2, hint, fg(p.fg_muted));
    let primary = w.controls();
    let actions = w.actions();
    let mut x = start;
    for (i, label) in labels.iter().enumerate() {
        let focused = w.focus == primary + i;
        let enabled =
            !matches!(actions.get(i), Some(Button::Create | Button::Replace)) || can_create;
        let color = if !enabled {
            fg(p.fg_muted)
        } else if focused {
            p.selected().fg(p.fg)
        } else {
            fg(p.fg)
        };
        if focused && enabled {
            line.put(x.saturating_sub(1), FOCUS, fg(p.border_focus));
        }
        x = line.put(x, label, color) + 3;
    }
}

fn dir_suffix(edit: &LineEdit, m: &UiModel) -> String {
    let Some(dir) = trimmed(edit) else {
        return String::new();
    };
    match m.dirs.probe().probe(&expand_home(dir, &m.home)) {
        DirStatus::GitRepo => "✓ git repo".to_owned(),
        DirStatus::Found => "✓ found".to_owned(),
        DirStatus::Missing => "✕ not found".to_owned(),
    }
}

fn suffix_style(suffix: &str, p: &Palette) -> Style {
    match suffix.chars().next() {
        Some('✓') => fg(p.done),
        Some('✕') => fg(p.error),
        _ => fg(p.fg_muted),
    }
}

fn found_line(w: &FirstRun) -> String {
    let parts: Vec<String> = CliKind::ALL
        .iter()
        .map(|cli| match w.version(*cli) {
            Some(v) => format!("{} {v}", cli.binary()),
            None => format!("{} not found", cli.binary()),
        })
        .collect();
    format!("found: {}", parts.join(" · "))
}

fn blocked(buf: &mut Buffer, area: Rect, p: &Palette) {
    let rows = [
        "Neither Claude Code nor OpenCode was found on your PATH.",
        "",
        "  Claude Code  claude.ai/code",
        "  OpenCode     opencode.ai",
        "",
        "Install at least one, then run duohive again.",
        "",
    ];
    let r = frame(buf, area, rows.len() + 2, "DuoHive needs an agent CLI", p);
    let mut line = Line {
        buf,
        x0: r.x + 1,
        y: r.y + 2,
        right: r.right() - 1,
    };
    for text in rows {
        line.text(text, p.fg);
        line.down();
    }
    let label = "[ q Quit ]";
    let x = line
        .width()
        .saturating_sub(u16::try_from(width(label) + 8).unwrap_or(0));
    line.put(x, label, fg(p.fg));
}
