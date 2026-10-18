//! Messages between you and agents, and between agents (TD §6.3).

use serde::{Deserialize, Serialize};

use crate::card::CardId;
use crate::names::AgentName;
use crate::time::Millis;

/// Names one message; counts up from 1 and is never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MessageId(pub u64);

/// Who sent a message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sender {
    /// You, from the chat (`@agent text`) or a card answer.
    You,
    /// An agent, identified by its token (never by a name it typed).
    Agent(AgentName),
}

/// What a message carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageKind {
    /// Plain text.
    Text,
    /// Your answer to a card, sent to Duo (P3-D2, P3-D12).
    CardAnswer(CardId),
}

/// Where a message stands (TD §6.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageStatus {
    /// Waiting for the recipient to be at rest.
    Queued,
    /// Typed in and taken.
    Delivered,
    /// The recipient replied to the sender (agent-to-agent only, P3-D18).
    Answered,
    /// Never delivered, and never retried.
    Dropped {
        /// Why.
        reason: String,
    },
}

/// One message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Its id.
    pub id: MessageId,
    /// Who sent it.
    pub from: Sender,
    /// Who it is for.
    pub to: AgentName,
    /// What it carries.
    pub kind: MessageKind,
    /// The text, as sent.
    pub text: String,
    /// Where it stands.
    pub status: MessageStatus,
    /// When it was queued.
    pub queued_at: Millis,
    /// When it was delivered, if it was.
    pub delivered_at: Option<Millis>,
}
