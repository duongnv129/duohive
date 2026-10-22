//! The right-click menu (UI §4.4.2): every action for a node with its key; a disabled row
//! says why. A row does what its key does on the node the menu was opened on.

use crossterm::event::{KeyCode, KeyEvent};
use duohive_core::org::NodeKey;
use duohive_core::status::AgentState;
use ratatui::layout::Position;

use super::{Action, UiEffect, mapped};
use crate::forms::{MoveTo, node_name};
use crate::keymap::ActionId;
use crate::model::UiModel;

/// One row: the action, how it reads, its key and, when it can't run, why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuRow {
    /// What it does.
    pub action: ActionId,
    /// The row's words.
    pub label: String,
    /// The action's key as the key map spells it.
    pub key: String,
    /// Why it is off.
    pub disabled: Option<String>,
}

/// An open menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    /// The node it was opened on.
    pub node: NodeKey,
    /// Where the pointer was.
    pub at: Position,
    /// The rows, top to bottom.
    pub rows: Vec<MenuRow>,
    /// The highlighted row (keyboard).
    pub cursor: usize,
}

/// Whether anything under `node` is running.
fn running_under(m: &UiModel, node: &NodeKey) -> Option<String> {
    let busy = |a: &duohive_core::names::AgentName| m.view(a).status.state != AgentState::Stopped;
    match node {
        NodeKey::Agent(a) => busy(a).then(|| a.to_string()),
        NodeKey::Role(k) => m
            .org
            .agents()
            .into_iter()
            .filter(|l| l.team.map(|t| &t.name) == k.team.as_ref() && l.role.name == k.role)
            .map(|l| l.agent.name.clone())
            .find(|a| busy(a))
            .map(|a| a.to_string()),
        NodeKey::Team(t) => m
            .org
            .agents()
            .into_iter()
            .filter(|l| l.team.map(|x| &x.name) == Some(t))
            .map(|l| l.agent.name.clone())
            .find(|a| busy(a))
            .map(|a| a.to_string()),
        NodeKey::Duo => None,
    }
}

/// The actions a node's menu lists, in the order of the mock-up.
fn actions(node: &NodeKey) -> &'static [ActionId] {
    use ActionId::{Delete, Edit, GiveWork, LastReply, Move, New, StartStop, Worktrees, Zoom};
    match node {
        NodeKey::Duo => &[Edit, New, Zoom, Worktrees, LastReply],
        NodeKey::Team(_) => &[New, Edit, Move, Delete, Worktrees, LastReply],
        NodeKey::Role(_) => &[GiveWork, New, Edit, Move, Delete, Worktrees, LastReply],
        NodeKey::Agent(_) => &[
            GiveWork, StartStop, Edit, New, Move, Delete, Zoom, Worktrees, LastReply,
        ],
    }
}

fn row(m: &UiModel, node: &NodeKey, action: ActionId) -> MenuRow {
    let name = node_name(node);
    let state = match node {
        NodeKey::Agent(a) => Some(m.view(a).status.state),
        _ => None,
    };
    let (label, disabled) = match action {
        ActionId::GiveWork => ("Give work", None),
        ActionId::StartStop => (
            if state == Some(AgentState::Stopped) {
                "Start"
            } else {
                "Stop"
            },
            None,
        ),
        ActionId::Edit if *node == NodeKey::Duo => ("Edit org", None),
        ActionId::Edit => ("Edit", None),
        ActionId::New => ("New here…", None),
        ActionId::Move => (
            "Move to…",
            MoveTo::open(&m.org, node)
                .is_none()
                .then(|| format!("no other place to move {name}")),
        ),
        ActionId::Delete => (
            "Delete",
            running_under(m, node).map(|a| {
                if matches!(node, NodeKey::Agent(_)) {
                    "stop it first".to_owned()
                } else {
                    format!("stop {a} first")
                }
            }),
        ),
        ActionId::Zoom => ("Zoom", None),
        ActionId::Worktrees => ("Worktrees", None),
        ActionId::LastReply => (
            if m.settings.show_last_reply {
                "Hide replies"
            } else {
                "Show replies"
            },
            None,
        ),
        // Not offered on a node.
        _ => ("", None),
    };
    MenuRow {
        action,
        label: label.to_owned(),
        key: m.keymap.key(action).label(),
        disabled,
    }
}

/// The menu for `node`, opened at `at`.
pub(crate) fn build(m: &UiModel, node: NodeKey, at: Position) -> Menu {
    let rows: Vec<MenuRow> = actions(&node).iter().map(|a| row(m, &node, *a)).collect();
    let cursor = rows.iter().position(|r| r.disabled.is_none()).unwrap_or(0);
    Menu {
        node,
        at,
        rows,
        cursor,
    }
}

/// The action a key means in an open menu; any other key closes it.
pub(super) fn map_key(k: KeyEvent) -> Action {
    match k.code {
        KeyCode::Up => Action::Up,
        KeyCode::Down => Action::Down,
        KeyCode::Enter => Action::DialogEnter,
        _ => Action::Esc,
    }
}

/// A key in the open menu.
pub(super) fn action(m: &mut UiModel, a: Action) -> Vec<UiEffect> {
    let Some(menu) = m.menu.as_mut() else {
        return Vec::new();
    };
    match a {
        Action::Up => menu.cursor = menu.cursor.saturating_sub(1),
        Action::Down => menu.cursor = (menu.cursor + 1).min(menu.rows.len().saturating_sub(1)),
        Action::DialogEnter => {
            let at = menu.cursor;
            return choose(m, at);
        }
        _ => m.menu = None,
    }
    Vec::new()
}

/// Runs row `i`: the node is selected and the action runs as its key would; a disabled
/// row says why in the footer.
pub(crate) fn choose(m: &mut UiModel, i: usize) -> Vec<UiEffect> {
    let Some(menu) = m.menu.take() else {
        return Vec::new();
    };
    let Some(row) = menu.rows.get(i) else {
        return Vec::new();
    };
    if let Some(why) = &row.disabled {
        m.footer_note = Some(format!("{} · {why}", row.label));
        return Vec::new();
    }
    m.selected = menu.node.clone();
    m.focus.pane = crate::focus::Pane::Org;
    m.focus.agent_mode = crate::focus::AgentMode::Selected;
    super::update(m, mapped(row.action))
}
