//! The agent pane's keys: Selected and Typing modes, and scrolling back (UI §4.1.2,
//! P2-D9, P2-D12).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::command::Command;
use duohive_core::status::AgentState;

use super::{Action, UiEffect, agent_of};
use crate::focus::{AgentMode, Pane};
use crate::keys_out::key_to_input;
use crate::model::UiModel;

/// In Typing mode every key goes to the agent, except `ctrl+q`, which leaves.
pub(super) fn typing_key(k: KeyEvent) -> Action {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && matches!(k.code, KeyCode::Char('q' | 'Q')) {
        Action::StopTyping
    } else {
        Action::Type(k)
    }
}

/// An action while the agent pane has focus.
pub(super) fn agent_action(m: &mut UiModel, a: Action) -> Vec<UiEffect> {
    match a {
        Action::Type(k) => key_to_input(k).map(UiEffect::Pane).into_iter().collect(),
        Action::StopTyping => {
            m.focus.agent_mode = AgentMode::Selected;
            Vec::new()
        }
        Action::Enter => {
            start_typing(m);
            Vec::new()
        }
        Action::PageUp => vec![UiEffect::Scroll { up: true }],
        Action::PageDown => vec![UiEffect::Scroll { up: false }],
        _ => Vec::new(),
    }
}

/// `enter`: Typing mode on a running agent, back on the live terminal (P2-D12).
fn start_typing(m: &mut UiModel) {
    let Some(agent) = agent_of(m, &m.selected) else {
        return;
    };
    if m.view(&agent).status.state.is_running() {
        m.focus.agent_mode = AgentMode::Typing;
        m.scrolled = false;
    }
}

/// Focusing the agent pane on a finished agent marks it seen (UI §2.1).
pub(super) fn view_if_finished(m: &UiModel) -> Vec<UiEffect> {
    if m.focus.pane != Pane::Agent {
        return Vec::new();
    }
    match agent_of(m, &m.selected) {
        Some(agent) if m.view(&agent).status.state == AgentState::Finished => {
            vec![UiEffect::Send(Command::ViewAgent { agent })]
        }
        _ => Vec::new(),
    }
}
