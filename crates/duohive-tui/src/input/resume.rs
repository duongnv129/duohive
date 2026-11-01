//! The resume picker's keys and the engine's two answers (UI §4.13).

use crossterm::event::{KeyCode, KeyEvent};
use duohive_core::command::Command;
use duohive_core::inherit::resolve;
use duohive_core::names::AgentName;
use duohive_core::session_catalog::SessionInfo;

use super::{Action, UiEffect, confirm};
use crate::forms::{ResumePicker, age_text, clean};
use crate::model::{Dialog, UiModel};

/// What a session with no title is called.
pub(crate) const UNTITLED: &str = "(untitled)";
/// Shown where an agent has no directory of its own.
const NO_DIR: &str = "its directory";

/// The title as the screen shows it.
pub(crate) fn title_of(s: &SessionInfo) -> String {
    let t = clean(&s.title);
    if t.is_empty() { UNTITLED.to_owned() } else { t }
}

/// A key in the picker: everything printable goes to the search box.
pub(super) fn map_key(k: KeyEvent) -> Option<Action> {
    Some(match k.code {
        KeyCode::Esc => Action::Esc,
        KeyCode::Enter => Action::DialogEnter,
        KeyCode::Up => Action::Up,
        KeyCode::Down => Action::Down,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Char(c) => Action::Char(c),
        _ => return None,
    })
}

/// An action in the open picker.
pub(super) fn action(m: &mut UiModel, mut p: Box<ResumePicker>, a: Action) -> Vec<UiEffect> {
    match a {
        Action::Esc if !p.search.is_empty() => p.clear_search(),
        Action::Esc => return Vec::new(),
        Action::Up => p.step(false),
        Action::Down => p.step(true),
        Action::PageUp => p.page(false),
        Action::PageDown => p.page(true),
        Action::Char(c) => p.type_char(c),
        Action::Backspace => p.backspace(),
        Action::DialogEnter => return enter(m, p),
        _ => {}
    }
    m.dialog = Some(Dialog::Resume(p));
    Vec::new()
}

/// `enter` on the highlighted session: resume it, ask first, or say who has it.
fn enter(m: &mut UiModel, mut p: Box<ResumePicker>) -> Vec<UiEffect> {
    let Some(s) = p.selected().cloned() else {
        m.dialog = Some(Dialog::Resume(p));
        return Vec::new();
    };
    let on_yes = Command::ResumeSession {
        agent: p.agent.clone(),
        session_id: s.id.clone(),
    };
    if let Some(holder) = &s.in_use_by {
        m.footer_note = Some(format!("{holder} is using this session"));
        m.dialog = Some(Dialog::Resume(p));
        return Vec::new();
    }
    if !p.needs_confirm(&s, m.now.0) {
        return vec![UiEffect::Send(on_yes)];
    }
    let ago = age_text(m.now.0.saturating_sub(s.updated_ms));
    let line = format!(
        "Resume \"{}\"? It was active {ago} and may still be open in another terminal.",
        title_of(&s)
    );
    p.confirm = Some(p.cursor);
    let Dialog::Confirm(mut c) = confirm("Resume session?", vec![line], "Resume", on_yes) else {
        return Vec::new();
    };
    c.back_resume = Some(p);
    m.dialog = Some(Dialog::Confirm(c));
    Vec::new()
}

impl UiModel {
    /// The engine listed `agent`'s sessions: open the picker, unless a dialog is up (the
    /// answer is dropped; focus is never stolen) or the agent is gone.
    pub(crate) fn sessions_listed(&mut self, agent: &AgentName, sessions: Vec<SessionInfo>) {
        // The answer is in: "reading…" has done its job, whether or not the picker opens.
        if self.footer_note.as_deref() == Some(self.reading_note(agent).as_str()) {
            self.footer_note = None;
        }
        let known = self.org.is_duo(agent) || self.org.find_agent(agent).is_some();
        if self.dialog.is_some() || self.wizard.is_some() || !known {
            return;
        }
        let resolved = resolve(&self.org, agent).ok();
        let cli = resolved.as_ref().map_or(self.org.defaults.cli, |r| r.cli);
        let dir = resolved
            .and_then(|r| r.directory)
            .map_or_else(|| NO_DIR.to_owned(), |d| clean(&d));
        let picker = ResumePicker::new(agent.clone(), cli.display().to_owned(), dir, sessions);
        self.dialog = Some(Dialog::Resume(Box::new(picker)));
    }

    /// The footer note while `agent`'s CLI is read: `reading Claude Code's sessions…`.
    pub(crate) fn reading_note(&self, agent: &AgentName) -> String {
        let cli = resolve(&self.org, agent)
            .map_or(self.org.defaults.cli, |r| r.cli)
            .display();
        format!("reading {cli}'s sessions…")
    }

    /// The sessions couldn't be listed: a note in `status.error`, the agent unchanged.
    pub(crate) fn sessions_failed(&mut self, agent: &AgentName, reason: &str) {
        let cli = resolve(&self.org, agent)
            .map_or(self.org.defaults.cli, |r| r.cli)
            .display();
        let note = format!("Couldn't read {cli}'s sessions: {}", clean(reason));
        self.footer_note = Some(note.clone());
        self.error_note = Some(note);
    }
}
