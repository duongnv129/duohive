//! The phase-2 chat with Duo (P2-D8): an input whose text goes to Duo as Give work, and an
//! in-memory history of what you sent and Duo's replies.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::command::Command;
use duohive_core::names::AgentName;
use duohive_core::status::AgentState;
use duohive_core::time::Millis;
use duohive_core::worktree::RunIn;

use crate::forms::LineEdit;
use crate::input::{Action, UiEffect};
use crate::model::UiModel;

/// The history keeps at most this many lines; older ones are dropped.
const MAX_LINES: usize = 1_000;

/// Who said a chat line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    /// You.
    You,
    /// Duo.
    Duo,
}

/// One line of chat history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatLine {
    /// When it was said.
    pub at: Millis,
    /// Who said it.
    pub who: Who,
    /// What was said.
    pub text: String,
}

/// The chat history, oldest first (P2-D8).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatLines {
    /// The lines.
    pub lines: Vec<ChatLine>,
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

/// An action in the focused chat: editing, or `enter` to send.
pub(crate) fn chat_action(m: &mut UiModel, a: Action) -> Vec<UiEffect> {
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
        _ => {}
    }
    Vec::new()
}

/// `enter`: the text goes to Duo as Give work in its own directory (P2-D8), unless Duo is
/// busy, in which case the text stays and the footer says why.
fn send(m: &mut UiModel) -> Vec<UiEffect> {
    let text = m.chat_input.text().trim().to_owned();
    if text.is_empty() {
        return Vec::new();
    }
    let duo = m.org.duo.name.clone();
    let view = m.view(&duo);
    let state = view.status.state;
    if view.delivery_pending {
        m.footer_note = Some(still_taking(&duo));
        return Vec::new();
    }
    if matches!(
        state,
        AgentState::Starting | AgentState::Working | AgentState::Waiting
    ) {
        m.footer_note = Some(busy(&duo, state));
        return Vec::new();
    }
    m.chat_input = LineEdit::default();
    m.chat_sent = Some(text.clone());
    push(
        &mut m.chat,
        ChatLine {
            at: m.now,
            who: Who::You,
            text: text.clone(),
        },
    );
    vec![UiEffect::Send(Command::GiveWork {
        agent: duo,
        task: text,
        run: RunIn::Directory,
    })]
}

/// The engine refused what the chat sent: the text goes back into the input.
pub(crate) fn refused(m: &mut UiModel, state: AgentState, pending: bool) {
    let duo = m.org.duo.name.clone();
    m.footer_note = Some(if pending {
        still_taking(&duo)
    } else {
        busy(&duo, state)
    });
    take_back(m);
}

/// Duo showed no sign of starting the task (P2-D11): the text goes back into the input.
pub(crate) fn not_taken(m: &mut UiModel) {
    m.footer_note = Some(format!(
        "{} didn't take the task · space to try again",
        m.org.duo.name
    ));
    take_back(m);
}

/// Duo's turn ended (it went idle): its reply joins the history (P2-D8).
pub(crate) fn duo_status_changed(m: &mut UiModel, before: AgentState) {
    let duo = m.org.duo.name.clone();
    let status = m.view(&duo).status.clone();
    if !status.state.is_at_rest() {
        // Duo took the task: nothing to put back any more.
        m.chat_sent = None;
    }
    if before == AgentState::Idle || status.state != AgentState::Idle {
        return;
    }
    let Some(reply) = m.view(&duo).last_reply.clone() else {
        return;
    };
    let repeat = m
        .chat
        .lines
        .iter()
        .rev()
        .find(|l| l.who == Who::Duo)
        .is_some_and(|l| l.text == reply);
    if !repeat {
        push(
            &mut m.chat,
            ChatLine {
                at: status.since,
                who: Who::Duo,
                text: reply,
            },
        );
    }
}

/// Puts the last sent text back into an empty input and drops its history line.
fn take_back(m: &mut UiModel) {
    let Some(text) = m.chat_sent.take() else {
        return;
    };
    if let Some(i) = m
        .chat
        .lines
        .iter()
        .rposition(|l| l.who == Who::You && l.text == text)
    {
        m.chat.lines.remove(i);
    }
    if m.chat_input.text().is_empty() {
        m.chat_input = LineEdit::new(&text);
    }
}

fn busy(duo: &AgentName, state: AgentState) -> String {
    format!("{duo} is {} · send again when it is at rest", state.word())
}

/// P2-D11's refusal while a delivery is pending.
pub(crate) fn still_taking(agent: &AgentName) -> String {
    format!("{agent} is still taking the last task")
}

/// Appends a line, dropping the oldest past the cap.
fn push(chat: &mut ChatLines, line: ChatLine) {
    chat.lines.push(line);
    let over = chat.lines.len().saturating_sub(MAX_LINES);
    chat.lines.drain(..over);
}
