//! The Duo chat (UI §4.7; P3-D14, P3-D26): an input whose lines go to Duo, to an agent
//! (`@agent text`) or to a local command (`/help`, `/agents`), and the transcript the
//! engine publishes.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::chat::{ChatEntry, ChatId, ChatKind};
use duohive_core::command::Command;
use duohive_core::names::AgentName;

use crate::format::{pad_right, state_cell, width};
use crate::forms::LineEdit;
use crate::input::{Action, UiEffect};
use crate::model::UiModel;
use crate::suggest::{SuggestKind, suggestions};

/// The history keeps at most this many entries in memory; older ones are dropped.
const MAX_ENTRIES: usize = 1_000;
/// What `/help` prints (P3-D26): the chat's keys.
const HELP: [&str; 3] = [
    "enter send · alt+enter new line · PgUp/PgDn scroll history",
    "plain text goes to Duo · @agent text goes to that agent · @ or / lists them",
    "/agents every agent and its state · esc back to the org pane",
];
/// Ends a dropped message's note: `enter` sends it again (P3-D26, P2-D11).
const SEND_AGAIN: &str = " · enter to send again";
/// Local notes are never stored, so they carry no engine id.
const NOTE_ID: ChatId = ChatId(0);

/// The chat history, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatLines {
    /// The entries: the engine's, and local notes.
    pub entries: Vec<ChatEntry>,
    /// Scrolled back (P2-D25): the first history row shown. `None` follows the newest.
    pub scroll: Option<usize>,
}

/// Where an input line goes (UI §4.7, P3-D26).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// Plain text, or `@Duo text`: to Duo.
    Duo(String),
    /// `@agent text`.
    Agent {
        /// The agent.
        to: AgentName,
        /// The text.
        text: String,
    },
    /// `/help`.
    Help,
    /// `/agents`.
    Agents,
    /// `/word` for any other word.
    Unknown(String),
    /// `@name` for a name the org doesn't have.
    NoAgent(String),
}

/// Where `line` goes: `/word` is a command, `@name text` a message to that agent (Duo's
/// name goes to Duo), anything else is for Duo.
pub fn route(line: &str, m: &UiModel) -> Route {
    let line = line.trim();
    if let Some(command) = line.strip_prefix('/') {
        let word = command.split_whitespace().next().unwrap_or_default();
        return match word {
            "help" => Route::Help,
            "agents" => Route::Agents,
            _ => Route::Unknown(word.to_owned()),
        };
    }
    let Some(rest) = line.strip_prefix('@') else {
        return Route::Duo(line.to_owned());
    };
    let (name, text) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let text = text.trim().to_owned();
    match AgentName::parse(name) {
        Ok(to) if m.org.is_duo(&to) => Route::Duo(text),
        Ok(to) if m.org.find_agent(&to).is_some() => Route::Agent { to, text },
        _ => Route::NoAgent(name.to_owned()),
    }
}

/// Keys while the chat input has focus: every character types, so no letter quits or opens
/// anything; `ctrl+c` does nothing while typing (UI §4.11).
pub(crate) fn chat_key(k: KeyEvent) -> Option<Action> {
    if k.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    if k.modifiers.contains(KeyModifiers::ALT) {
        return (k.code == KeyCode::Enter).then_some(Action::NewLine);
    }
    Some(match k.code {
        KeyCode::Enter => Action::Enter,
        KeyCode::Tab => Action::Tab,
        KeyCode::BackTab => Action::BackTab,
        KeyCode::Esc => Action::Esc,
        KeyCode::Up => Action::Up,
        KeyCode::Down => Action::Down,
        KeyCode::Left => Action::Left,
        KeyCode::Right => Action::Right,
        KeyCode::Home => Action::Home,
        KeyCode::End => Action::End,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Delete => Action::DeleteForward,
        KeyCode::Char(c) => Action::Char(c),
        _ => return None,
    })
}

/// An action in the focused chat: editing, the suggestion list's keys, or `enter` to send.
pub(crate) fn chat_action(m: &mut UiModel, a: Action) -> Vec<UiEffect> {
    if let Some(s) = &mut m.suggest {
        match a {
            Action::Up => {
                s.cursor = s.cursor.saturating_sub(1);
                return Vec::new();
            }
            Action::Down => {
                s.cursor = (s.cursor + 1).min(s.items.len().saturating_sub(1));
                return Vec::new();
            }
            Action::Esc => {
                m.suggest = None;
                return Vec::new();
            }
            Action::Enter | Action::Tab => return take(m, a == Action::Enter),
            _ => {}
        }
    }
    let input = &mut m.chat_input;
    match a {
        Action::Char(c) => input.insert(c),
        Action::NewLine => input.insert('\n'),
        Action::Left => input.left(),
        Action::Right => input.right(),
        Action::Home => input.home(),
        Action::End => input.end(),
        Action::Backspace => input.backspace(),
        Action::DeleteForward => input.delete(),
        Action::Enter => return send(m),
        Action::PageUp | Action::PageDown => {
            if let Some((_, room)) = crate::render::chat_history_fit(m) {
                let page = room.saturating_sub(1).max(1);
                scroll(m, a == Action::PageUp, page);
            }
            return Vec::new();
        }
        _ => return Vec::new(),
    }
    refresh_suggest(m);
    Vec::new()
}

/// `tab` or `enter` on the list: the item goes into the input. `enter` on a command
/// already typed in full sends it.
fn take(m: &mut UiModel, enter: bool) -> Vec<UiEffect> {
    let Some(s) = m.suggest.take() else {
        return Vec::new();
    };
    let typed = m.chat_input.text().trim();
    let whole = s.items.get(s.cursor).is_some_and(|(item, _)| item == typed);
    if enter && whole && s.kind == SuggestKind::Command {
        return send(m);
    }
    s.take(&mut m.chat_input);
    Vec::new()
}

/// The suggestion list follows the input; the highlighted item stays when it still shows.
pub(crate) fn refresh_suggest(m: &mut UiModel) {
    let was = m
        .suggest
        .as_ref()
        .and_then(|s| s.items.get(s.cursor))
        .map(|(item, _)| item.clone());
    let mut next = suggestions(m.chat_input.text(), m.chat_input.cursor(), m);
    if let (Some(s), Some(was)) = (&mut next, was) {
        s.cursor = s.items.iter().position(|(i, _)| *i == was).unwrap_or(0);
    }
    m.suggest = next;
}

/// `enter`: the line goes where [`route`] says; a line that can't go stays, with the
/// footer saying why (P3-D26).
fn send(m: &mut UiModel) -> Vec<UiEffect> {
    let line = m.chat_input.text().trim().to_owned();
    if line.is_empty() {
        return Vec::new();
    }
    let command = match route(&line, m) {
        Route::Duo(text) => Command::ChatToDuo { text },
        Route::Agent { to, text } => Command::SendMessage { to, text },
        Route::Help => {
            let lines = HELP.iter().map(|&l| l.to_owned()).collect();
            notes(m, lines);
            return Vec::new();
        }
        Route::Agents => {
            let lines = agent_lines(m);
            notes(m, lines);
            return Vec::new();
        }
        Route::Unknown(word) => {
            m.footer_note = Some(format!("unknown command /{word} · /help lists them"));
            return Vec::new();
        }
        Route::NoAgent(name) => {
            m.footer_note = Some(format!("no agent named {name}"));
            return Vec::new();
        }
    };
    m.chat_input = LineEdit::default();
    m.chat_sent = Some(line);
    m.chat.scroll = None;
    vec![UiEffect::Send(command)]
}

/// Local lines that aren't stored (`/help`, `/agents`); the input is cleared.
fn notes(m: &mut UiModel, lines: Vec<String>) {
    m.chat_input = LineEdit::default();
    m.chat.scroll = None;
    for text in lines {
        let entry = ChatEntry {
            id: NOTE_ID,
            at: m.now,
            kind: ChatKind::Note,
            text,
        };
        push(&mut m.chat, entry);
    }
}

/// `/agents`: Duo, then every agent in tree order, each with its state cell.
fn agent_lines(m: &UiModel) -> Vec<String> {
    let names = m.agent_names();
    let cells = names.iter().map(|a| width(a.as_str())).max().unwrap_or(0) + 2;
    names
        .into_iter()
        .map(|a| {
            let cell = state_cell(&m.view(a).status, m.now, None);
            format!("{}{}", pad_right(a.as_str(), cells), cell.trim_end())
        })
        .collect()
}

/// Scrolls the history `rows` up or down (P2-D25). Following the newest, the history
/// shows the newest entry's start (friction entry 13); down past it shows the very
/// bottom, and down from the bottom follows the newest again.
pub(crate) fn scroll(m: &mut UiModel, up: bool, rows: usize) {
    let Some(fit) = crate::render::chat_history(m) else {
        return;
    };
    let bottom = fit.len.saturating_sub(fit.room);
    let follow = fit.follow.min(bottom);
    let top = m.chat.scroll.map_or(follow, |t| t.min(bottom));
    m.chat.scroll = if up {
        Some(top.saturating_sub(rows))
    } else if (top < follow && top + rows >= follow) || top >= bottom {
        None
    } else {
        Some((top + rows).min(bottom))
    };
}

/// Your message was refused (`MessageRefused`): the line comes back into the input with
/// the footer saying why.
pub(crate) fn refused(m: &mut UiModel, reason: String) {
    m.footer_note = Some(reason);
    if let Some(line) = m.chat_sent.take() {
        put_back(m, &line);
    }
}

/// Your message was dropped (P3-D26): its text comes back, addressed as it was, and
/// `enter` sends it again.
pub(crate) fn dropped(m: &mut UiModel, to: &AgentName, text: &str, reason: &str) {
    let line = if m.org.is_duo(to) {
        text.to_owned()
    } else {
        format!("@{to} {text}")
    };
    m.footer_note = Some(format!("{reason}{SEND_AGAIN}"));
    put_back(m, &line);
}

/// Puts `line` into the input, before anything typed since (on a line of its own).
fn put_back(m: &mut UiModel, line: &str) {
    let typed = m.chat_input.text();
    m.chat_input = if typed.is_empty() {
        LineEdit::new(line)
    } else {
        LineEdit::new(&format!("{line}\n{typed}"))
    };
    m.suggest = None;
}

/// P2-D11's refusal while a delivery is pending.
pub(crate) fn still_taking(agent: &AgentName) -> String {
    format!("{agent} is still taking the last task")
}

/// Appends an entry, dropping the oldest past the cap.
pub(crate) fn push(chat: &mut ChatLines, entry: ChatEntry) {
    chat.entries.push(entry);
    let over = chat.entries.len().saturating_sub(MAX_ENTRIES);
    chat.entries.drain(..over);
}
