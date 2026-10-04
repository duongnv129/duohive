//! Keys to actions, and actions to model changes and effects (UI §4.8, §4.11, D8, D16).

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use duohive_core::command::{Command, OrgEdit};
use duohive_core::names::AgentName;
use duohive_core::org::{NodeKey, Org, Role, RoleKey};
use duohive_core::status::AgentState;

use crate::format::{age_of, format_age, status_word};
use crate::forms::{Chooser, Form, FormOutcome};
use crate::model::{Confirm, Dialog, UiModel};
use crate::tree::tree_rows;

const GROUP_START_STOP: &str = "s starts or stops an agent";
const DUO_NOT_DELETABLE: &str = "Duo can't be deleted";
/// The engine refuses every edit while `org.toml` doesn't parse (Review Focus 2).
const ORG_FILE_BROKEN: &str = "org.toml has an error · fix it first (o)";

/// What a key asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Select the node above.
    Up,
    /// Select the node below.
    Down,
    /// Start a stopped agent, stop an errored one, or ask before stopping a running one.
    StartStop,
    /// New here…: the chooser.
    New,
    /// Edit the selection; on Duo, Edit org.
    Edit,
    /// Delete the selection, asking first.
    Delete,
    /// Last reply on / off.
    ToggleReplies,
    /// Open `org.toml` in `$EDITOR`, then reload it.
    OpenOrgFile,
    /// Quit; agents keep running.
    Quit,
    /// `y` in a dialog.
    DialogYes,
    /// `n` in a dialog.
    DialogNo,
    /// `enter` in a dialog: act on the focused button.
    DialogEnter,
    /// Back one level; never quits.
    Esc,
    /// Next button or field.
    Tab,
    /// Previous button or field.
    BackTab,
    /// A typed character, in a form.
    Char(char),
    /// ← in a form: cursor left, or the previous choice.
    Left,
    /// → in a form: cursor right, or the next choice.
    Right,
    /// Cursor to the start.
    Home,
    /// Cursor to the end.
    End,
    /// Delete before the cursor.
    Backspace,
    /// Delete under the cursor.
    DeleteForward,
    /// `ctrl+s`: save the form.
    Save,
    /// `alt+enter`: a newline in a prompt field.
    NewLine,
}

/// What the app loop does after an action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEffect {
    /// Send a command to the core.
    Send(Command),
    /// Leave the app.
    Quit,
    /// Hand the terminal to `$EDITOR` on this file, then reload the org.
    OpenEditor(PathBuf),
}

/// The action for a key press, given what has focus; `None` for keys with no meaning here.
///
/// A form is typing (UI §4.11): `q` types, and `ctrl+c` does nothing there, so a stray
/// `ctrl+c` never throws a draft away; `esc` cancels the form first.
pub fn map_key(m: &UiModel, k: KeyEvent) -> Option<Action> {
    if k.kind == KeyEventKind::Release {
        return None;
    }
    if matches!(m.dialog, Some(Dialog::Form(_))) {
        return form_key(k);
    }
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl {
        return (k.code == KeyCode::Char('c')).then_some(Action::Quit);
    }
    if k.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    let common = match k.code {
        KeyCode::Esc => Some(Action::Esc),
        KeyCode::Tab => Some(Action::Tab),
        KeyCode::BackTab => Some(Action::BackTab),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    };
    if common.is_some() {
        return common;
    }
    match &m.dialog {
        Some(Dialog::Confirm(_)) => match k.code {
            KeyCode::Char('y') => Some(Action::DialogYes),
            KeyCode::Char('n') => Some(Action::DialogNo),
            KeyCode::Enter => Some(Action::DialogEnter),
            _ => None,
        },
        Some(Dialog::Chooser(_)) => match k.code {
            KeyCode::Up => Some(Action::Up),
            KeyCode::Down => Some(Action::Down),
            KeyCode::Enter => Some(Action::DialogEnter),
            _ => None,
        },
        Some(Dialog::Form(_)) | None => tree_key(k.code),
    }
}

fn tree_key(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Up => Some(Action::Up),
        KeyCode::Down => Some(Action::Down),
        KeyCode::Char('s') => Some(Action::StartStop),
        KeyCode::Char('n') => Some(Action::New),
        KeyCode::Char('e') => Some(Action::Edit),
        KeyCode::Char('d') => Some(Action::Delete),
        KeyCode::Char('v') => Some(Action::ToggleReplies),
        KeyCode::Char('o') => Some(Action::OpenOrgFile),
        _ => None,
    }
}

/// Keys in a form: editing keys, `ctrl+s` saves, `alt+enter` is a newline.
fn form_key(k: KeyEvent) -> Option<Action> {
    if k.modifiers.contains(KeyModifiers::CONTROL) {
        return (k.code == KeyCode::Char('s')).then_some(Action::Save);
    }
    if k.modifiers.contains(KeyModifiers::ALT) {
        return (k.code == KeyCode::Enter).then_some(Action::NewLine);
    }
    Some(match k.code {
        KeyCode::Esc => Action::Esc,
        KeyCode::Tab => Action::Tab,
        KeyCode::BackTab => Action::BackTab,
        KeyCode::Enter => Action::DialogEnter,
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

/// Applies an action to the model; pure. Any key clears the footer note.
pub fn update(m: &mut UiModel, a: Action) -> Vec<UiEffect> {
    m.footer_note = None;
    if a == Action::Quit {
        return vec![UiEffect::Quit];
    }
    match m.dialog.take() {
        Some(Dialog::Confirm(c)) => return confirm_action(m, c, a),
        Some(Dialog::Form(f)) => return form_action(m, f, a),
        Some(Dialog::Chooser(c)) => {
            chooser_action(m, c, a);
            return Vec::new();
        }
        None => {}
    }
    match a {
        Action::Up => select_next(m, false),
        Action::Down => select_next(m, true),
        Action::StartStop => start_stop(m),
        Action::Delete => delete(m),
        Action::New => new_here(m),
        Action::Edit => edit_here(m),
        Action::ToggleReplies => {
            m.settings.show_last_reply = !m.settings.show_last_reply;
            vec![UiEffect::Send(Command::UpdateSettings(m.settings.clone()))]
        }
        Action::OpenOrgFile => vec![UiEffect::OpenEditor(m.org_toml.clone())],
        _ => Vec::new(),
    }
}

/// Pasted text goes to the open form's focused field; elsewhere it is dropped.
pub fn paste(m: &mut UiModel, text: &str) {
    if let Some(Dialog::Form(f)) = &mut m.dialog {
        f.paste(text, &m.org);
        f.probe_dir(&m.org, m.dirs.probe(), &m.home);
    }
}

/// `y`, `n`, `enter`, `esc` and `tab` in a confirmation; one a form asked goes back to it.
fn confirm_action(m: &mut UiModel, mut c: Box<Confirm>, a: Action) -> Vec<UiEffect> {
    let yes = match a {
        Action::DialogYes => true,
        Action::DialogEnter => c.focus_yes,
        Action::DialogNo | Action::Esc => false,
        Action::Tab | Action::BackTab => {
            c.focus_yes = !c.focus_yes;
            m.dialog = Some(Dialog::Confirm(c));
            return Vec::new();
        }
        _ => {
            m.dialog = Some(Dialog::Confirm(c));
            return Vec::new();
        }
    };
    if let Some(mut back) = c.back {
        back.saving = yes;
        m.dialog = Some(Dialog::Form(back));
    }
    if yes {
        vec![UiEffect::Send(c.on_yes)]
    } else {
        Vec::new()
    }
}

/// A key in a form: a save sends the edit and leaves the form waiting for the engine.
fn form_action(m: &mut UiModel, mut f: Box<Form>, a: Action) -> Vec<UiEffect> {
    f.sync_duo_state(m.view(&m.org.duo.name).status.state);
    let out = f.handle(a, &m.org);
    match out {
        FormOutcome::Continue => {
            f.probe_dir(&m.org, m.dirs.probe(), &m.home);
            m.dialog = Some(Dialog::Form(f));
            Vec::new()
        }
        FormOutcome::Cancel => Vec::new(),
        FormOutcome::Edit(edit) => {
            f.saving = true;
            m.dialog = Some(Dialog::Form(f));
            vec![UiEffect::Send(Command::EditOrg(edit))]
        }
        FormOutcome::Confirm(c) => {
            m.dialog = Some(Dialog::Confirm(Box::new(Confirm { back: Some(f), ..c })));
            Vec::new()
        }
    }
}

/// `↑↓` (and `tab`) choose, `enter` opens the form, `esc` closes.
fn chooser_action(m: &mut UiModel, mut c: Chooser, a: Action) {
    match a {
        Action::Up | Action::BackTab => c.step(false),
        Action::Down | Action::Tab => c.step(true),
        Action::DialogEnter => {
            m.dialog = c
                .open(&m.org, m.dirs.probe(), &m.home)
                .map(|f| Dialog::Form(Box::new(f)));
            return;
        }
        Action::Esc | Action::DialogNo => return,
        _ => {}
    }
    m.dialog = Some(Dialog::Chooser(c));
}

/// `n`: the chooser for the selection.
fn new_here(m: &mut UiModel) -> Vec<UiEffect> {
    if m.org_error.is_some() {
        return note(m, ORG_FILE_BROKEN);
    }
    m.dialog = Some(Dialog::Chooser(Chooser::for_selection(&m.org, &m.selected)));
    Vec::new()
}

/// `e`: the selection's form; on Duo, Edit org.
fn edit_here(m: &mut UiModel) -> Vec<UiEffect> {
    if m.org_error.is_some() {
        return note(m, ORG_FILE_BROKEN);
    }
    let (dirs, home) = (m.dirs.probe(), m.home.as_path());
    let form = match &m.selected {
        NodeKey::Duo => Form::edit_org(&m.org, m.view(&m.org.duo.name)),
        NodeKey::Agent(a) if m.org.is_duo(a) => Form::edit_org(&m.org, m.view(a)),
        NodeKey::Agent(a) => Form::edit_agent(&m.org, a, m.view(a), dirs, home),
        NodeKey::Role(k) => Form::edit_role(&m.org, k, dirs, home),
        NodeKey::Team(t) => Form::edit_team(&m.org, t, dirs, home),
    };
    m.dialog = Some(Dialog::Form(Box::new(form)));
    Vec::new()
}

/// Moves the selection to the next node row (detail lines are skipped); landing on a
/// finished agent marks it seen (UI §2.1).
fn select_next(m: &mut UiModel, down: bool) -> Vec<UiEffect> {
    let keys: Vec<NodeKey> = tree_rows(m, false, false, m.now)
        .into_iter()
        .filter_map(|r| r.key)
        .collect();
    let at = keys.iter().position(|k| k == &m.selected);
    let next = match (at, down) {
        (None, _) => Some(0),
        (Some(i), true) => (i + 1 < keys.len()).then_some(i + 1),
        (Some(i), false) => i.checked_sub(1),
    };
    let Some(key) = next.and_then(|i| keys.get(i)) else {
        return Vec::new();
    };
    m.selected = key.clone();
    match agent_of(m, key) {
        Some(a) if m.view(&a).status.state == AgentState::Finished => {
            vec![UiEffect::Send(Command::ViewAgent { agent: a })]
        }
        _ => Vec::new(),
    }
}

/// The agent a node names: Duo's name for Duo, `None` for teams and roles.
fn agent_of(m: &UiModel, key: &NodeKey) -> Option<AgentName> {
    match key {
        NodeKey::Duo => Some(m.org.duo.name.clone()),
        NodeKey::Agent(a) => Some(a.clone()),
        NodeKey::Team(_) | NodeKey::Role(_) => None,
    }
}

/// `s`: stopped → start; errored → stop (D16); running → ask first (UI §4.8).
fn start_stop(m: &mut UiModel) -> Vec<UiEffect> {
    let Some(agent) = agent_of(m, &m.selected) else {
        m.footer_note = Some(GROUP_START_STOP.to_owned());
        return Vec::new();
    };
    let status = &m.view(&agent).status;
    match status.state {
        AgentState::Stopped => vec![UiEffect::Send(Command::StartAgent { agent })],
        AgentState::Errored => vec![UiEffect::Send(Command::StopAgent { agent })],
        _ => {
            let age = age_of(status, m.now)
                .map(|d| format!(" ({})", format_age(d)))
                .unwrap_or_default();
            let line = format!("Stop {agent}? It is {}{age}.", status_word(status));
            m.dialog = Some(confirm(
                "Stop agent?",
                vec![line],
                "Stop",
                Command::StopAgent { agent },
            ));
            Vec::new()
        }
    }
}

/// `d`: asks before deleting a stopped node; refuses with the reason in the footer otherwise.
fn delete(m: &mut UiModel) -> Vec<UiEffect> {
    let key = m.selected.clone();
    let (title, first, agents) = match &key {
        NodeKey::Duo => return note(m, DUO_NOT_DELETABLE),
        NodeKey::Agent(a) if m.org.is_duo(a) => return note(m, DUO_NOT_DELETABLE),
        NodeKey::Agent(a) => {
            let Some(loc) = m.org.find_agent(a) else {
                return Vec::new();
            };
            let role = NodeKey::Role(RoleKey {
                team: loc.team.map(|t| t.name.clone()),
                role: loc.role.name.clone(),
            });
            let first = format!("Delete agent {a} in {}?", m.org.breadcrumb(&role));
            ("Delete agent?", first, vec![a.clone()])
        }
        NodeKey::Role(k) => {
            let Some(role) = find_role(&m.org, k) else {
                return Vec::new();
            };
            let place = match &k.team {
                Some(t) => format!(" in {t}"),
                None => String::new(),
            };
            let first = format!("Delete role {}{place}?", k.role);
            ("Delete role?", first, names(std::slice::from_ref(role)))
        }
        NodeKey::Team(t) => {
            let Some(team) = m.org.teams.iter().find(|x| &x.name == t) else {
                return Vec::new();
            };
            (
                "Delete team?",
                format!("Delete team {t}?"),
                names(&team.roles),
            )
        }
    };
    if let Some(busy) = agents
        .iter()
        .find(|a| m.view(a).status.state != AgentState::Stopped)
    {
        return note(m, &format!("stop {busy} first"));
    }
    let lines = match &key {
        NodeKey::Agent(_) => vec![first],
        _ => removes(m, &first, &agents),
    };
    let on_yes = Command::EditOrg(OrgEdit::Delete(key));
    m.dialog = Some(confirm(title, lines, "Delete", on_yes));
    Vec::new()
}

/// `<first> It removes n agents:` and one indented line per agent (UI §4.8).
fn removes(m: &UiModel, first: &str, agents: &[AgentName]) -> Vec<String> {
    let head = match agents.len() {
        0 => format!("{first} It has no agents."),
        1 => format!("{first} It removes 1 agent:"),
        k => format!("{first} It removes {k} agents:"),
    };
    std::iter::once(head)
        .chain(
            agents
                .iter()
                .map(|a| format!("  {a} ({})", m.view(a).status.state.word())),
        )
        .collect()
}

fn names(roles: &[Role]) -> Vec<AgentName> {
    roles
        .iter()
        .flat_map(|r| r.agents.iter().map(|a| a.name.clone()))
        .collect()
}

fn find_role<'a>(org: &'a Org, k: &RoleKey) -> Option<&'a Role> {
    let roles = match &k.team {
        Some(t) => &org.teams.iter().find(|x| &x.name == t)?.roles,
        None => &org.roles,
    };
    roles.iter().find(|r| r.name == k.role)
}

fn note(m: &mut UiModel, text: &str) -> Vec<UiEffect> {
    m.footer_note = Some(text.to_owned());
    Vec::new()
}

/// A confirmation that opens with focus on Cancel.
fn confirm(title: &str, lines: Vec<String>, yes: &str, on_yes: Command) -> Dialog {
    Dialog::Confirm(Box::new(Confirm {
        title: title.to_owned(),
        lines,
        yes_label: yes.to_owned(),
        focus_yes: false,
        on_yes,
        back: None,
    }))
}

#[cfg(test)]
mod tests;
