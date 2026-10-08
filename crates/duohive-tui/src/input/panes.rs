//! Keys for the main screen's panes: focus, folding and Needs you (UI §4.11, P2-D9).

use crossterm::event::KeyCode;
use duohive_core::names::AgentName;
use duohive_core::org::{NodeKey, RoleKey};

use super::Action;
use crate::focus::{AgentMode, Pane};
use crate::model::UiModel;
use crate::render::chat_extra_limit;

/// The action for a key in a pane other than the chat (which types).
pub(super) fn pane_key(pane: Pane, code: KeyCode) -> Option<Action> {
    match pane {
        Pane::Org => match code {
            KeyCode::Left => Some(Action::Left),
            KeyCode::Right => Some(Action::Right),
            KeyCode::Enter => Some(Action::Enter),
            code => super::tree_key(code),
        },
        Pane::NeedsYou => match code {
            KeyCode::Up => Some(Action::Up),
            KeyCode::Down => Some(Action::Down),
            KeyCode::Enter => Some(Action::Enter),
            _ => None,
        },
        // Typing mode is mapped before this (it takes every key).
        Pane::Agent => match code {
            KeyCode::Enter => Some(Action::Enter),
            KeyCode::PageUp => Some(Action::PageUp),
            KeyCode::PageDown => Some(Action::PageDown),
            _ => None,
        },
        Pane::Chat => None,
    }
}

/// `tab` and `shift+tab` between panes; the agent pane always starts in Selected mode.
pub(super) fn move_focus(m: &mut UiModel, forward: bool) {
    m.focus.pane = if forward {
        m.focus.next()
    } else {
        m.focus.prev()
    };
    m.focus.agent_mode = AgentMode::Selected;
}

/// `ctrl+↑` / `ctrl+↓`: between UI §3's size and the size that leaves the agent pane 6
/// rows (P2-D3).
pub(super) fn resize_chat(m: &mut UiModel, grow: bool) {
    let step = if grow { 1 } else { -1 };
    let max = chat_extra_limit(m);
    m.chat_rows_extra = m.chat_rows_extra.saturating_add(step).clamp(0, max.max(0));
}

/// `←` folds a team or role, or moves from an agent to its role.
pub(super) fn fold(m: &mut UiModel) {
    match &m.selected {
        NodeKey::Team(_) | NodeKey::Role(_) => {
            m.folded.insert(m.selected.clone());
        }
        NodeKey::Agent(a) => {
            if let Some(loc) = m.org.find_agent(a) {
                m.selected = NodeKey::Role(RoleKey {
                    team: loc.team.map(|t| t.name.clone()),
                    role: loc.role.name.clone(),
                });
            }
        }
        NodeKey::Duo => {}
    }
}

/// `→` unfolds the selected team or role.
pub(super) fn unfold(m: &mut UiModel) {
    m.folded.remove(&m.selected);
}

/// `enter` in the org pane: on an agent or Duo it focuses the agent pane; on a team or
/// role it folds or unfolds (P2-D9).
pub(super) fn enter(m: &mut UiModel) {
    match &m.selected {
        NodeKey::Team(_) | NodeKey::Role(_) => {
            if !m.folded.remove(&m.selected) {
                m.folded.insert(m.selected.clone());
            }
        }
        NodeKey::Agent(_) | NodeKey::Duo => {
            m.focus.pane = Pane::Agent;
            m.focus.agent_mode = AgentMode::Selected;
        }
    }
}

/// `↑↓` in Needs you move its cursor within the list.
pub(super) fn needs_you_move(m: &mut UiModel, down: bool) {
    let last = m.needs_you.len().saturating_sub(1);
    let at = m.focus.needs_you_cursor;
    m.focus.needs_you_cursor = if down {
        (at + 1).min(last)
    } else {
        at.saturating_sub(1)
    };
}

/// `enter` in Needs you: selects the agent under the cursor, unfolds its team and role,
/// and focuses the agent pane in Selected mode (UI §4.1.3).
pub(super) fn needs_you_enter(m: &mut UiModel) -> Option<AgentName> {
    let agent = m.needs_you.get(m.focus.needs_you_cursor)?.agent.clone();
    if m.org.is_duo(&agent) {
        m.selected = NodeKey::Duo;
    } else {
        let loc = m.org.find_agent(&agent)?;
        let team = loc.team.map(|t| t.name.clone());
        if let Some(t) = &team {
            m.folded.remove(&NodeKey::Team(t.clone()));
        }
        m.folded.remove(&NodeKey::Role(RoleKey {
            team,
            role: loc.role.name.clone(),
        }));
        m.selected = NodeKey::Agent(agent.clone());
    }
    m.focus.pane = Pane::Agent;
    m.focus.agent_mode = AgentMode::Selected;
    Some(agent)
}
