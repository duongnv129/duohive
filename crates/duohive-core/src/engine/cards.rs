//! Approval cards (TD §6.2; P3-D2, P3-D5, P3-D12, P3-D21, P3-D25). A card
//! waits for your answer while Duo keeps working; the answer applies the
//! change as things are then, and reaches Duo later as a message from you.

use super::Engine;
use crate::alert::card_alert;
use crate::card::{Card, CardChange, CardId, CardOutcome, CardSource, answer_text, result_line};
use crate::chat::ChatKind;
use crate::command::OrgEdit;
use crate::event::Event;
use crate::inbox::{MessageKind, Sender};
use crate::org::Fields;
use crate::status::AgentState;
use crate::tool::{ErrorCode, ToolError, ToolOk, ToolResult};
use crate::validate::forbidden_flags_error;

impl Engine {
    /// `propose_org_change` (TD §6.2): a valid change raises a card and
    /// returns at once; removing a node with an agent that isn't stopped is refused.
    pub(super) fn propose_org_change(
        &mut self,
        edit: &OrgEdit,
        out: &mut Vec<Event>,
    ) -> ToolResult {
        if let Err(errors) = super::preview_edit(&self.org, edit) {
            let message = errors.into_iter().next().map(|e| e.message);
            return Err(ToolError::new(
                ErrorCode::InvalidChange,
                message.unwrap_or_default(),
            ));
        }
        if let Some(reason) = self.removal_refusal(edit) {
            return Err(ToolError::new(ErrorCode::Refused, reason));
        }
        let card = self.raise_card(CardSource::Duo, CardChange::Org(edit.clone()), out);
        Ok(ToolOk::Pending { card })
    }

    /// Raises a card: saved, published, a Needs you row (P3-D25), and its
    /// alert unless turned off or start-up is replaying (P3-D5, P2-D14).
    /// Nothing about Duo changes (P3-D2).
    pub(super) fn raise_card(
        &mut self,
        source: CardSource,
        change: CardChange,
        out: &mut Vec<Event>,
    ) -> CardId {
        let card = Card {
            id: CardId(self.next_card),
            source,
            change,
            raised_at: self.ports.clock.now(),
            answer: None,
        };
        self.next_card += 1;
        self.save_card(&card, out);
        let summary = card.summary(&self.org);
        let id = card.id;
        self.cards.push(card.clone());
        out.push(Event::CardRaised(Box::new(card)));
        if !self.replaying && self.settings.alerts.needs_you {
            let alert = card_alert(&self.org.duo.name, &summary);
            self.ports.notifier.notify(&alert);
            out.push(Event::Alert(alert));
        }
        id
    }

    /// `ApproveCard` (P3-D21): applies the change, or `edited` (the same
    /// kind), as things are now; what can't apply is answered not applied.
    pub(super) fn approve_card(&mut self, card: CardId, edited: Option<CardChange>) -> Vec<Event> {
        let at = match self.pending_card(card) {
            Ok(at) => at,
            Err(reason) => return super::reject(reason),
        };
        let proposed = self.cards[at].change.clone();
        let (change, outcome) = match edited {
            None => (proposed, CardOutcome::Approved),
            Some(edited) => match kept_kind(&proposed, edited) {
                Some(change) => (change, CardOutcome::EditedApproved),
                None => return super::reject(EDIT_KEEPS_KIND),
            },
        };
        // Your edit is refused, not answered: the card waits to be fixed (P3-D31).
        if let Some(reason) = set_fields(&change).find_map(forbidden_flags_error) {
            return super::reject(reason);
        }
        let summary = change.summary(&self.org);
        let mut out = Vec::new();
        let outcome = match self.apply_card(&change, &mut out) {
            Ok(()) => outcome,
            Err(reason) => CardOutcome::NotApplied(reason),
        };
        self.answer(at, change, outcome, &summary, &mut out);
        out
    }

    /// `RejectCard`: nothing applies; Duo is told.
    pub(super) fn reject_card(&mut self, card: CardId) -> Vec<Event> {
        let at = match self.pending_card(card) {
            Ok(at) => at,
            Err(reason) => return super::reject(reason),
        };
        let change = self.cards[at].change.clone();
        let summary = change.summary(&self.org);
        let mut out = Vec::new();
        self.answer(at, change, CardOutcome::Rejected, &summary, &mut out);
        out
    }

    /// Where card `id` is among the pending ones, or why it can't be answered.
    fn pending_card(&self, id: CardId) -> Result<usize, String> {
        match self.cards.iter().position(|c| c.id == id) {
            Some(at) => Ok(at),
            // Ids are never reused: a lower one was raised and answered.
            None if id.0 >= 1 && id.0 < self.next_card => {
                Err(format!("card {} was already answered", id.0))
            }
            None => Err(format!("no card {}", id.0)),
        }
    }

    /// Applies an approved change by the state now (P3-D21).
    fn apply_card(&mut self, change: &CardChange, out: &mut Vec<Event>) -> Result<(), String> {
        match change {
            CardChange::Org(edit) => self.apply_edit(edit, out),
            CardChange::Stop { agent } => {
                if self.knows(agent) && self.state_of(agent) == AgentState::Stopped {
                    return Err(format!("{agent} is already stopped"));
                }
                let events = self.stop_agent(agent);
                if let Some(reason) = refusal(&events) {
                    return Err(reason);
                }
                out.extend(events);
                Ok(())
            }
            CardChange::NewAgent {
                role,
                agent,
                task,
                run,
                ..
            } => {
                let add = OrgEdit::AddAgent {
                    role: role.clone(),
                    agent: agent.clone(),
                };
                self.apply_edit(&add, out)?;
                // The agent exists now; a launch that fails shows on its row.
                if let Err(e) = self.give(&agent.name, task.clone(), *run, out) {
                    tracing::info!(agent = %agent.name, refusal = ?e, "new agent didn't take its task");
                }
                Ok(())
            }
        }
    }

    /// Records the answer: saved, `CardAnswered`, a chat line, and the
    /// answer to Duo as your message (P3-D12).
    fn answer(
        &mut self,
        at: usize,
        change: CardChange,
        outcome: CardOutcome,
        summary: &str,
        out: &mut Vec<Event>,
    ) {
        let mut card = self.cards.remove(at);
        let id = card.id;
        let line = result_line(&outcome, summary);
        let text = answer_text(id, &outcome, summary);
        let kind = ChatKind::CardResult {
            card: id,
            outcome: outcome.clone(),
        };
        out.push(Event::CardAnswered {
            card: id,
            outcome: outcome.clone(),
        });
        card.change = change;
        card.answer = Some((outcome, self.ports.clock.now()));
        self.save_card(&card, out);
        self.chat_entry(kind, line, out);
        let duo = self.org.duo.name.clone();
        if let Err(e) = self.send(Sender::You, &duo, MessageKind::CardAnswer(id), text, out) {
            tracing::warn!(card = id.0, error = %e.message, "card answer not sent to Duo");
        }
    }

    /// Saves `card`; a failure is reported, not fatal.
    fn save_card(&mut self, card: &Card, out: &mut Vec<Event>) {
        if let Err(e) = self.ports.store.save_card(card) {
            out.push(Event::CommandRejected {
                reason: format!("couldn't save state: {e}"),
            });
        }
    }

    /// Start-up: the pending cards; ids continue from the highest.
    pub(super) fn load_cards(&mut self, out: &mut Vec<Event>) {
        match self.ports.store.load_cards() {
            Ok(mut cards) => {
                cards.sort_by_key(|c| c.id);
                self.next_card = cards.last().map_or(1, |c| c.id.0 + 1);
                cards.retain(|c| c.answer.is_none());
                self.cards = cards;
            }
            Err(e) => out.push(Event::CommandRejected {
                reason: format!("couldn't load cards: {e}"),
            }),
        }
    }
}

/// Why an edited approval is refused.
const EDIT_KEEPS_KIND: &str = "an edit must keep the card's kind";

/// `edited` when it is the same kind as `proposed`: a stop can't be edited,
/// and a new-agent card keeps its task and what it was cloned from (P3-D7).
fn kept_kind(proposed: &CardChange, edited: CardChange) -> Option<CardChange> {
    match (proposed, edited) {
        (CardChange::Org(_), edited @ CardChange::Org(_)) => Some(edited),
        (
            CardChange::NewAgent { task, clone_of, .. },
            CardChange::NewAgent {
                role, agent, run, ..
            },
        ) => Some(CardChange::NewAgent {
            role,
            agent,
            task: task.clone(),
            run,
            clone_of: clone_of.clone(),
        }),
        _ => None,
    }
}

/// The fields `change` sets, at every node it adds or edits.
fn set_fields(change: &CardChange) -> impl Iterator<Item = &Fields> {
    let fields = match change {
        CardChange::NewAgent { agent, .. } => Some(&agent.fields),
        CardChange::Stop { .. } => None,
        CardChange::Org(edit) => match edit {
            OrgEdit::AddTeam(d) | OrgEdit::EditTeam { draft: d, .. } => Some(&d.defaults),
            OrgEdit::AddRole { role: d, .. } | OrgEdit::EditRole { draft: d, .. } => {
                Some(&d.defaults)
            }
            OrgEdit::AddAgent { agent: d, .. } | OrgEdit::EditAgent { draft: d, .. } => {
                Some(&d.fields)
            }
            OrgEdit::EditOrgDefaults { .. } | OrgEdit::Delete(_) | OrgEdit::Move { .. } => None,
        },
    };
    fields.into_iter()
}

/// The first refusal among `events`.
fn refusal(events: &[Event]) -> Option<String> {
    events.iter().find_map(|e| match e {
        Event::CommandRejected { reason } => Some(reason.clone()),
        _ => None,
    })
}

#[cfg(test)]
mod tests;
