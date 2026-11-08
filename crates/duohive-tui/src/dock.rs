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

/// The card docked on the agent pane now. An empty org still shows Duo's cards: they are
/// how Duo sets one up. Nothing shows while reconnecting or with a team or role selected.
pub(crate) fn shown_dock(m: &UiModel) -> Option<Dock> {
    if m.reconnecting > 0 {
        return None;
    }
    dock_for(m, m.selected_agent()?)
}

/// Whether `id` is the card drawn now: only that card may take the keys.
pub(crate) fn is_shown(m: &UiModel, id: CardId) -> bool {
    shown_dock(m).is_some_and(|d| d.card == id)
}

/// Selects the agent card `id` docks on and gives its oldest pending card the keys (UI
/// §4.6): the agent pane, Selected mode, `y`, `e` and `n` live. Returns the agent for the
/// `ViewAgent` effect; `None` when no pane shows a card (reconnecting, an unknown card).
pub(crate) fn focus_card(m: &mut UiModel, id: CardId) -> Option<AgentName> {
    let agent = dock_agent(m.cards.get(&id)?, &m.org);
    if !m.reveal_agent(&agent) {
        return None;
    }
    m.suggest = None;
    m.focus.card = shown_dock(m).map(|d| d.card);
    m.focus.card.map(|_| agent)
}

/// After a card is answered: its agent's next card takes the focus the answered one had
/// (CD-D4), when it had it.
pub(crate) fn pass_focus(m: &mut UiModel, answered: &Card) {
    if m.focus.card != Some(answered.id) {
        return;
    }
    let agent = dock_agent(answered, &m.org);
    m.focus.card = dock_for(m, &agent)
        .filter(|_| shown_agent(m) == Some(&agent))
        .map(|d| d.card);
}

#[cfg(test)]
mod tests;
