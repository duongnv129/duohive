//! The wizard's steps (UI §4.3): keys, validation and the answers Create org sends.

use std::collections::BTreeMap;
use std::path::Path;

use duohive_core::cli_kind::CliKind;
use duohive_core::command::Command;
use duohive_core::inherit::expand_home;
use duohive_core::templates::{EngineeringInput, ProductInput, SquadInput, TemplateInput};
use duohive_core::validate::{DirProbe, DirStatus};

use super::forms::TeamRow;
use super::{Button, FirstRun, Step, TemplateChoice, trimmed};
use crate::forms::LineEdit;
use crate::input::Action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// What a key did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Stay in the wizard.
    Continue,
    /// Quit DuoHive.
    Quit,
    /// Leave the wizard without changing anything (replace mode, UI §4.3).
    Cancel,
    /// Send this command and wait for `Event::OrgCreated` or `Event::OrgReplaced`.
    Create(Box<Command>),
}

/// Maps a key to an action while the wizard is up: it owns every key, so a text
/// field types `q` and only the buttons and `esc` act (UI §4.3).
pub fn map_key(k: KeyEvent) -> Option<Action> {
    if k.modifiers.contains(KeyModifiers::CONTROL) {
        return (k.code == KeyCode::Char('c')).then_some(Action::Quit);
    }
    if k.modifiers.contains(KeyModifiers::ALT) {
        return (k.code == KeyCode::Enter).then_some(Action::NewLine);
    }
    Some(match k.code {
        KeyCode::Esc => Action::Esc,
        KeyCode::Tab => Action::Tab,
        KeyCode::BackTab => Action::BackTab,
        KeyCode::Enter => Action::DialogEnter,
        KeyCode::Up => Action::Up,
        KeyCode::Down => Action::Down,
        KeyCode::Left => Action::Left,
        KeyCode::Right => Action::Right,
        KeyCode::Home => Action::Home,
        KeyCode::End => Action::End,
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Delete => Action::DeleteForward,
        KeyCode::Char(c) => Action::Char(c),
        _ => return None,
    })
}

/// Applies `a` to the wizard.
pub fn key(w: &mut FirstRun, a: Action, dirs: &dyn DirProbe, home: &Path) -> Outcome {
    w.write_error = None;
    // No supported CLI: the blocking screen's only key is Quit (UI §4.3).
    if w.blocked() {
        return match a {
            Action::Char('q') | Action::Esc | Action::Quit => Outcome::Quit,
            _ => Outcome::Continue,
        };
    }
    if w.ask_quit {
        return quit_answer(w, a);
    }
    match w.step {
        Step::You => you_key(w, a, dirs, home),
        Step::Template => template_key(w, a, dirs, home),
        Step::Form => form_key(w, a, dirs, home),
    }
}

fn quit_answer(w: &mut FirstRun, a: Action) -> Outcome {
    match a {
        Action::DialogYes | Action::Char('y') => Outcome::Quit,
        Action::DialogNo | Action::Char('n') | Action::Esc => {
            w.ask_quit = false;
            Outcome::Continue
        }
        _ => Outcome::Continue,
    }
}

fn you_key(w: &mut FirstRun, a: Action, dirs: &dyn DirProbe, home: &Path) -> Outcome {
    let controls = w.controls();
    match a {
        Action::Esc => w.ask_quit = true,
        Action::Tab => step_focus(w, true),
        Action::BackTab => step_focus(w, false),
        Action::Up => step_focus(w, false),
        Action::Down => step_focus(w, true),
        Action::Left if w.focus == 1 => cycle_duo(w, false),
        Action::Right | Action::Char(' ') if w.focus == 1 => cycle_duo(w, true),
        Action::Char(c) if w.focus == 0 && !c.is_control() => w.you.insert(c),
        Action::NewLine if w.focus == 0 => w.you.insert('\n'),
        Action::Backspace if w.focus == 0 => w.you.backspace(),
        Action::DeleteForward if w.focus == 0 => w.you.delete(),
        Action::Left if w.focus == 0 => w.you.left(),
        Action::Right if w.focus == 0 => w.you.right(),
        Action::Home if w.focus == 0 => w.you.home(),
        Action::End if w.focus == 0 => w.you.end(),
        Action::DialogEnter if w.focus == 0 => w.focus = 1,
        Action::DialogEnter if w.focus >= controls => {
            return button(w, w.focus - controls, dirs, home);
        }
        _ => {}
    }
    Outcome::Continue
}

fn template_key(w: &mut FirstRun, a: Action, dirs: &dyn DirProbe, home: &Path) -> Outcome {
    let controls = w.controls();
    match a {
        Action::Esc => return back(w),
        Action::Tab => step_focus(w, true),
        Action::BackTab => step_focus(w, false),
        Action::Up if w.focus == 0 => w.cursor = w.cursor.saturating_sub(1),
        Action::Down if w.focus == 0 => {
            w.cursor = (w.cursor + 1).min(TemplateChoice::ALL.len() - 1);
        }
        Action::Up => step_focus(w, false),
        Action::Down => step_focus(w, true),
        Action::DialogEnter if w.focus < controls => go(w, Step::Form),
        Action::DialogEnter => return button(w, w.focus - controls, dirs, home),
        _ => {}
    }
    Outcome::Continue
}

fn form_key(w: &mut FirstRun, a: Action, dirs: &dyn DirProbe, home: &Path) -> Outcome {
    let controls = w.controls();
    match a {
        Action::Esc => return back(w),
        Action::Tab => step_focus(w, true),
        Action::BackTab => step_focus(w, false),
        Action::Up => step_focus(w, false),
        Action::Down => step_focus(w, true),
        Action::DialogEnter if w.focus >= controls => {
            return button(w, w.focus - controls, dirs, home);
        }
        Action::DialogEnter => enter_control(w, dirs, home),
        Action::Char(' ') if is_toggle(w) => toggle(w),
        _ => edit_control(w, a),
    }
    Outcome::Continue
}

/// The focused button's action (UI §4.3).
fn button(w: &mut FirstRun, i: usize, dirs: &dyn DirProbe, home: &Path) -> Outcome {
    match w.actions().get(i).copied() {
        Some(Button::Next) => {
            go(w, next_step(w));
            Outcome::Continue
        }
        Some(Button::Back) => back(w),
        Some(Button::Create | Button::Replace) => create(w, dirs, home),
        Some(Button::Quit) => Outcome::Quit,
        Some(Button::Cancel) => Outcome::Cancel,
        None => Outcome::Continue,
    }
}

fn next_step(w: &FirstRun) -> Step {
    match w.step {
        Step::You | Step::Template => Step::Template,
        Step::Form => Step::Form,
    }
}

/// The Back button, and `esc`: the form goes to the template step, a first run's
/// template step goes to step 1, and a replace's template step cancels.
fn back(w: &mut FirstRun) -> Outcome {
    match w.step {
        Step::Form => {
            go(w, Step::Template);
            Outcome::Continue
        }
        Step::Template if w.replace => Outcome::Cancel,
        Step::Template => {
            go(w, Step::You);
            Outcome::Continue
        }
        Step::You => Outcome::Continue,
    }
}

/// The last button: send the command when every answer is valid.
fn create(w: &FirstRun, dirs: &dyn DirProbe, home: &Path) -> Outcome {
    if !errors(w, dirs, home).is_empty() || form_error(w, dirs, home).is_some() {
        return Outcome::Continue;
    }
    w.command()
        .map_or(Outcome::Continue, |c| Outcome::Create(Box::new(c)))
}

/// Enter on a field: toggles a choice, adds a row, else moves on (the last field to Create).
fn enter_control(w: &mut FirstRun, _dirs: &dyn DirProbe, _home: &Path) {
    if is_toggle(w) {
        toggle(w);
        return;
    }
    let controls = w.controls();
    if w.template() == TemplateChoice::EngineeringOrg && w.focus == 2 * w.engineering.rows.len() + 1
    {
        w.engineering.rows.push(TeamRow::default());
        w.focus = 2 * w.engineering.rows.len() + 1;
        return;
    }
    if w.focus == cli_control(w) {
        cycle_cli(w, true);
        return;
    }
    if w.focus == controls - 1 {
        w.focus = controls + 1;
    } else {
        w.focus = (w.focus + 1).min(controls);
    }
}

/// The control index of the CLI radio for the current template.
fn cli_control(w: &FirstRun) -> usize {
    match w.template() {
        TemplateChoice::AgentSquad => 2,
        TemplateChoice::ProductTeam => 1,
        TemplateChoice::EngineeringOrg => 0,
        TemplateChoice::Blank => usize::MAX,
    }
}

/// The focused control toggles on and off.
fn is_toggle(w: &FirstRun) -> bool {
    match w.template() {
        TemplateChoice::AgentSquad => w.step == Step::Form && w.focus == 3,
        TemplateChoice::ProductTeam => w.step == Step::Form && w.focus == 5,
        TemplateChoice::EngineeringOrg => {
            w.step == Step::Form && w.focus == 2 * w.engineering.rows.len() + 2
        }
        TemplateChoice::Blank => false,
    }
}

/// Flips the worktree choice; validation shows why `on` can't stand until a repo is set.
fn toggle(w: &mut FirstRun) {
    match w.template() {
        TemplateChoice::AgentSquad => w.squad.worktrees = !w.squad.worktrees,
        TemplateChoice::ProductTeam => w.product.worktrees = !w.product.worktrees,
        TemplateChoice::EngineeringOrg => w.engineering.worktrees = !w.engineering.worktrees,
        TemplateChoice::Blank => {}
    }
}

/// Editing keys for the focused text field; choices cycle.
fn edit_control(w: &mut FirstRun, a: Action) {
    match w.template() {
        TemplateChoice::AgentSquad => match w.focus {
            0 => edit(&mut w.squad.size, a),
            1 => edit(&mut w.squad.directory, a),
            2 => cycle_on_arrows(w, a),
            4 => edit(&mut w.squad.prompt, a),
            _ => {}
        },
        TemplateChoice::ProductTeam => match w.focus {
            0 => edit(&mut w.product.name, a),
            1 => cycle_on_arrows(w, a),
            2..=4 => edit(&mut w.product.directories[w.focus - 2], a),
            _ => {}
        },
        TemplateChoice::EngineeringOrg => {
            let rows = w.engineering.rows.len();
            if w.focus == 0 {
                cycle_on_arrows(w, a);
            } else if w.focus <= 2 * rows {
                let i = (w.focus - 1) / 2;
                let row = &mut w.engineering.rows[i];
                if w.focus % 2 == 1 {
                    edit(&mut row.name, a);
                } else {
                    edit(&mut row.repo, a);
                }
            }
        }
        TemplateChoice::Blank => {}
    }
}

fn edit(e: &mut LineEdit, a: Action) {
    match a {
        Action::Char(c) if !c.is_control() => e.insert(c),
        Action::NewLine => e.insert('\n'),
        Action::Backspace => e.backspace(),
        Action::DeleteForward => e.delete(),
        Action::Left => e.left(),
        Action::Right => e.right(),
        Action::Home => e.home(),
        Action::End => e.end(),
        _ => {}
    }
}

fn cycle_on_arrows(w: &mut FirstRun, a: Action) {
    match a {
        Action::Left => cycle_cli(w, false),
        Action::Right => cycle_cli(w, true),
        _ => {}
    }
}

/// Moves to the next installed CLI (the radios of a missing one stay disabled).
fn cycle_cli(w: &mut FirstRun, forward: bool) {
    let current = match w.template() {
        TemplateChoice::AgentSquad => w.squad.cli,
        TemplateChoice::ProductTeam => w.product.cli,
        TemplateChoice::EngineeringOrg => w.engineering.cli,
        TemplateChoice::Blank => return,
    };
    let Some(pick) = next_available(w, current, forward) else {
        return;
    };
    match w.template() {
        TemplateChoice::AgentSquad => w.squad.cli = pick,
        TemplateChoice::ProductTeam => w.product.cli = pick,
        TemplateChoice::EngineeringOrg => w.engineering.cli = pick,
        TemplateChoice::Blank => {}
    }
}

/// Moves Duo's CLI and with it every template's default (UI §4.3).
fn cycle_duo(w: &mut FirstRun, forward: bool) {
    let Some(pick) = next_available(w, w.duo_cli, forward) else {
        return;
    };
    w.duo_cli = pick;
    w.squad.cli = pick;
    w.product.cli = pick;
    w.engineering.cli = pick;
}

/// The next CLI after `current` that is installed.
fn next_available(w: &FirstRun, current: CliKind, forward: bool) -> Option<CliKind> {
    let found: Vec<CliKind> = CliKind::ALL
        .into_iter()
        .filter(|k| w.available(*k))
        .collect();
    if found.len() < 2 {
        return None;
    }
    let at = found.iter().position(|k| *k == current).unwrap_or(0);
    let next = if forward {
        (at + 1) % found.len()
    } else {
        (at + found.len() - 1) % found.len()
    };
    Some(found[next])
}

fn go(w: &mut FirstRun, step: Step) {
    w.step = step;
    // Blank has no fields, so the first control would be `← Back`; start on Create org.
    w.focus = usize::from(step == Step::Form && w.controls() == 0);
}

fn step_focus(w: &mut FirstRun, forward: bool) {
    let total = w.controls() + w.buttons().len();
    if total == 0 {
        return;
    }
    let focus = w.focus.min(total - 1);
    w.focus = if forward {
        (focus + 1) % total
    } else {
        (focus + total - 1) % total
    };
}

/// The errors under the step's fields, by control index.
pub fn errors(w: &FirstRun, dirs: &dyn DirProbe, home: &Path) -> BTreeMap<usize, String> {
    let mut out = BTreeMap::new();
    match w.template() {
        TemplateChoice::AgentSquad if w.step == Step::Form => {
            let size = w.squad.size.text().trim();
            if !matches!(size.parse::<u32>(), Ok(1..=100)) {
                out.insert(0, "agents must be 1–100".to_owned());
            }
            dir_error(&w.squad.directory, 1, dirs, home, &mut out);
            if w.squad.worktrees && repos_ok(&w.squad.directory, dirs, home).is_some_and(|ok| !ok) {
                out.insert(3, "not a git repo".to_owned());
            }
        }
        TemplateChoice::ProductTeam if w.step == Step::Form => {
            if trimmed(&w.product.name).is_none() {
                out.insert(0, "required".to_owned());
            }
            for (i, d) in w.product.directories.iter().enumerate() {
                dir_error(d, i + 2, dirs, home, &mut out);
            }
            if w.product.worktrees && !product_dirs(&w.product.directories, dirs, home) {
                out.insert(5, "not a git repo".to_owned());
            }
        }
        TemplateChoice::EngineeringOrg if w.step == Step::Form => {
            for (i, row) in w.engineering.rows.iter().enumerate() {
                if trimmed(&row.name).is_none() && trimmed(&row.repo).is_none() {
                    continue;
                }
                if trimmed(&row.name).is_none() {
                    out.insert(1 + 2 * i, "required".to_owned());
                }
                dir_error(&row.repo, 2 + 2 * i, dirs, home, &mut out);
            }
            if w.engineering.worktrees && !engineering_repos_ok(w, dirs, home) {
                out.insert(
                    2 * w.engineering.rows.len() + 2,
                    "not a git repo".to_owned(),
                );
            }
        }
        _ => {}
    }
    out
}

/// A problem that belongs to no field (no teams, a repeated team name).
pub fn form_error(w: &FirstRun, _dirs: &dyn DirProbe, _home: &Path) -> Option<String> {
    if w.step != Step::Form || w.template() != TemplateChoice::EngineeringOrg {
        return None;
    }
    let names: Vec<&str> = w
        .engineering
        .rows
        .iter()
        .filter_map(|r| trimmed(&r.name))
        .collect();
    if names.is_empty() {
        return Some("add at least one team".to_owned());
    }
    let mut seen = std::collections::BTreeSet::new();
    for name in names {
        if !seen.insert(name) {
            return Some(format!("duplicate team name: {name}"));
        }
    }
    None
}

fn dir_error(
    e: &LineEdit,
    index: usize,
    dirs: &dyn DirProbe,
    home: &Path,
    out: &mut BTreeMap<usize, String>,
) {
    match repo_status(e, dirs, home) {
        None => {
            out.insert(index, "required".to_owned());
        }
        Some(DirStatus::Missing) => {
            out.insert(index, "not found".to_owned());
        }
        _ => {}
    }
}

/// The directory's status; `None` when the field is blank.
fn repo_status(e: &LineEdit, dirs: &dyn DirProbe, home: &Path) -> Option<DirStatus> {
    trimmed(e).map(|d| dirs.probe(&expand_home(d, home)))
}

/// `Some(true)` when every directory either is a repo or is blank.
fn repos_ok(e: &LineEdit, dirs: &dyn DirProbe, home: &Path) -> Option<bool> {
    repo_status(e, dirs, home).map(|s| s == DirStatus::GitRepo)
}

fn product_dirs(dirs_in: &[LineEdit; 3], dirs: &dyn DirProbe, home: &Path) -> bool {
    dirs_in.iter().all(|d| match repo_status(d, dirs, home) {
        Some(s) => s == DirStatus::GitRepo,
        None => true,
    })
}

fn engineering_repos_ok(w: &FirstRun, dirs: &dyn DirProbe, home: &Path) -> bool {
    w.engineering.rows.iter().all(|r| {
        trimmed(&r.name).is_none() || repo_status(&r.repo, dirs, home) == Some(DirStatus::GitRepo)
    })
}

/// The template the answers describe, when every answer is valid.
pub fn build_input(w: &FirstRun) -> Option<TemplateInput> {
    match w.template() {
        TemplateChoice::AgentSquad => {
            let size: u32 = w.squad.size.text().trim().parse().ok()?;
            if !(1..=100).contains(&size) {
                return None;
            }
            Some(TemplateInput::AgentSquad(SquadInput {
                size,
                directory: trimmed(&w.squad.directory)?.to_owned(),
                cli: w.squad.cli,
                worktrees: w.squad.worktrees,
                prompt: trimmed(&w.squad.prompt).map(ToOwned::to_owned),
            }))
        }
        TemplateChoice::ProductTeam => Some(TemplateInput::ProductTeam(ProductInput {
            team_name: trimmed(&w.product.name)?.to_owned(),
            cli: w.product.cli,
            directories: std::array::from_fn(|i| {
                trimmed(&w.product.directories[i])
                    .unwrap_or_default()
                    .to_owned()
            }),
            worktrees: w.product.worktrees,
        })),
        TemplateChoice::EngineeringOrg => {
            let teams: Vec<(String, String)> = w
                .engineering
                .rows
                .iter()
                .filter_map(|r| Some((trimmed(&r.name)?.to_owned(), trimmed(&r.repo)?.to_owned())))
                .collect();
            (!teams.is_empty()).then_some(TemplateInput::EngineeringOrg(EngineeringInput {
                cli: w.engineering.cli,
                worktrees: w.engineering.worktrees,
                teams,
            }))
        }
        TemplateChoice::Blank => Some(TemplateInput::Blank),
    }
}

/// Pastes `text` into the focused text field.
pub fn paste(w: &mut FirstRun, text: &str) {
    let one_line: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
        .filter(|c| !c.is_control())
        .collect();
    match w.template() {
        TemplateChoice::AgentSquad => match w.focus {
            0 => w.squad.size.insert_str(&one_line),
            1 => w.squad.directory.insert_str(&one_line),
            4 => w.squad.prompt.insert_str(&one_line),
            _ => {}
        },
        TemplateChoice::ProductTeam => match w.focus {
            0 => w.product.name.insert_str(&one_line),
            2..=4 => w.product.directories[w.focus - 2].insert_str(&one_line),
            _ => {}
        },
        TemplateChoice::EngineeringOrg => {
            let rows = w.engineering.rows.len();
            if w.focus > 0 && w.focus <= 2 * rows {
                let i = (w.focus - 1) / 2;
                let row = &mut w.engineering.rows[i];
                if w.focus % 2 == 1 {
                    row.name.insert_str(&one_line);
                } else {
                    row.repo.insert_str(&one_line);
                }
            }
        }
        TemplateChoice::Blank => {}
    }
}
