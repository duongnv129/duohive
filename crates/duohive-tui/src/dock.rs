//! Where an approval card docks and what queues behind it (UI §4.6; CD-D1, CD-D2): a card
//! sits over the top of the agent pane of the agent it concerns, the oldest first.

use duohive_core::card::{Card, CardChange, CardId};
use duohive_core::command::OrgEdit;
use duohive_core::names::AgentName;
use duohive_core::org::{NodeKey, Org};

use crate::model::UiModel;

/// The oldest pending card of an agent, and where it stands in that agent's queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Dock {
    /// The card shown.
    pub card: CardId,
    /// Its place in the queue, from 1.
    pub position: usize,
    /// Pending cards for the agent.
    pub total: usize,
}

/// The agent whose pane `card` docks on: the agent the change stops, edits, moves or
/// deletes; Duo for everything else, and for an agent the org no longer has.
pub(crate) fn dock_agent(card: &Card, org: &Org) -> AgentName {
    let named = match &card.change {
        CardChange::Stop { agent } => Some(agent),
        CardChange::Org(
            OrgEdit::EditAgent { key: agent, .. }
            | OrgEdit::Delete(NodeKey::Agent(agent))
            | OrgEdit::Move {
                node: NodeKey::Agent(agent),
                ..
            },
        ) => Some(agent),
        CardChange::Org(_) | CardChange::NewAgent { .. } => None,
    };
    match named {
        Some(agent) if org.is_duo(agent) || org.find_agent(agent).is_some() => agent.clone(),
        _ => org.duo.name.clone(),
    }
}

/// The oldest pending card docked on `agent`, oldest by `(raised_at, id)`.
pub(crate) fn dock_for(m: &UiModel, agent: &AgentName) -> Option<Dock> {
    let mut queue: Vec<&Card> = m
        .cards
        .values()
        .filter(|c| dock_agent(c, &m.org) == *agent)
        .collect();
    queue.sort_by_key(|c| (c.raised_at, c.id));
    let first = queue.first()?;
    Some(Dock {
        card: first.id,
        position: 1,
        total: queue.len(),
    })
}

/// The agent whose pane is shown: none while reconnecting, with an empty org, or with a
/// team or role selected.
pub(crate) fn shown_agent(m: &UiModel) -> Option<&AgentName> {
    let empty = m.org.teams.is_empty() && m.org.roles.is_empty();
    if m.reconnecting > 0 || empty {
        return None;
    }
    m.selected_agent()
}

/// The card docked on the agent pane now.
pub(crate) fn shown_dock(m: &UiModel) -> Option<Dock> {
    dock_for(m, shown_agent(m)?)
}

#[cfg(test)]
mod tests;
