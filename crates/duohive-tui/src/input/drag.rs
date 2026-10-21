//! Dragging a node onto another to move it (UI §4.4.2): valid targets highlight while
//! dragging; an invalid drop is refused with the reason; a drop that changes inherited
//! values opens Move to… to confirm.

use duohive_core::command::{Command, MoveTarget, OrgEdit};
use duohive_core::engine::preview_edit;
use duohive_core::org::{NodeKey, Org, RoleKey};

use super::UiEffect;
use crate::forms::MoveTo;
use crate::model::{Dialog, UiModel};

/// Where a drag is hovering and whether the node may go there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropTarget {
    /// The row under the pointer.
    pub over: NodeKey,
    /// The move a drop there asks for.
    pub to: MoveTarget,
    /// The core accepts it.
    pub valid: bool,
    /// Why not, for the footer.
    pub reason: Option<String>,
}

/// A node being dragged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drag {
    /// The node pressed on.
    pub node: NodeKey,
    /// What it is over now.
    pub over: Option<DropTarget>,
}

/// The target a drop of `node` on `over` means, or `None` over itself or a no-op.
pub(crate) fn target_for(org: &Org, node: &NodeKey, over: &NodeKey) -> Option<DropTarget> {
    if node == over || *node == NodeKey::Duo {
        return None;
    }
    let invalid = |reason: &str| DropTarget {
        over: over.clone(),
        to: MoveTarget::End,
        valid: false,
        reason: Some(reason.to_owned()),
    };
    let to = match (node, over) {
        (NodeKey::Agent(_), NodeKey::Role(k)) => MoveTarget::Into(NodeKey::Role(k.clone())),
        (NodeKey::Agent(_), NodeKey::Agent(b)) => {
            let loc = org.find_agent(b)?;
            MoveTarget::Into(NodeKey::Role(RoleKey {
                team: loc.team.map(|t| t.name.clone()),
                role: loc.role.name.clone(),
            }))
        }
        (NodeKey::Agent(_), _) => return Some(invalid("an agent moves only into a role")),
        (NodeKey::Role(_), NodeKey::Team(_) | NodeKey::Duo) => MoveTarget::Into(over.clone()),
        (NodeKey::Role(_), _) => return Some(invalid("a role moves only into a team or Duo")),
        (NodeKey::Team(_), NodeKey::Team(t)) => MoveTarget::Before(t.clone()),
        (NodeKey::Team(_), NodeKey::Duo) => MoveTarget::End,
        (NodeKey::Team(_), _) => return Some(invalid("a team moves only among the teams")),
        (NodeKey::Duo, _) => return None,
    };
    let edit = OrgEdit::Move {
        node: node.clone(),
        to: to.clone(),
    };
    let current = MoveTo::open(org, node)
        .and_then(|p| p.options.into_iter().find(|o| o.to == to))
        .is_some_and(|o| o.current);
    if current {
        return None;
    }
    Some(match preview_edit(org, &edit) {
        Ok(_) => DropTarget {
            over: over.clone(),
            to,
            valid: true,
            reason: None,
        },
        Err(e) => DropTarget {
            over: over.clone(),
            to,
            valid: false,
            reason: e.first().map(|x| refusal(&x.message)),
        },
    })
}

/// The core's collision text in the words of UI §4.4.2.
fn refusal(message: &str) -> String {
    message
        .strip_prefix("a role named ")
        .and_then(|rest| rest.split_once(" already exists in "))
        .map_or_else(
            || message.to_owned(),
            |(role, parent)| format!("{parent} already has a role {role}"),
        )
}

/// Whether dropping `node` on `over` would be accepted (the rows to highlight while dragging).
pub(crate) fn drop_valid(m: &UiModel, node: &NodeKey, over: &NodeKey) -> bool {
    target_for(&m.org, node, over).is_some_and(|t| t.valid)
}

/// A left press on a node starts a drag; Duo can't be dragged.
pub(crate) fn start(m: &mut UiModel, node: NodeKey) {
    m.drag = (node != NodeKey::Duo).then_some(Drag { node, over: None });
}

/// The pointer moved over `over` (a row, or nothing) with the button held.
pub(crate) fn hover(m: &mut UiModel, over: Option<NodeKey>) {
    let Some(d) = m.drag.as_ref() else {
        return;
    };
    let target = over.and_then(|o| target_for(&m.org, &d.node, &o));
    if let Some(d) = m.drag.as_mut() {
        d.over = target;
    }
}

/// The button was released: move, ask, refuse or nothing.
pub(crate) fn drop_it(m: &mut UiModel) -> Vec<UiEffect> {
    let Some(d) = m.drag.take() else {
        return Vec::new();
    };
    let Some(t) = d.over else {
        return Vec::new();
    };
    if !t.valid {
        m.footer_note = t.reason;
        return Vec::new();
    }
    let Some(mut picker) = MoveTo::open(&m.org, &d.node) else {
        return Vec::new();
    };
    let Some(at) = picker.options.iter().position(|o| o.to == t.to) else {
        return Vec::new();
    };
    if picker.options[at].changes.is_empty() {
        return vec![UiEffect::Send(Command::EditOrg(OrgEdit::Move {
            node: d.node,
            to: t.to,
        }))];
    }
    picker.cursor = at;
    m.dialog = Some(Dialog::MoveTo(Box::new(picker)));
    Vec::new()
}
