//! Messages between you and agents, and between agents (TD §6.3).

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::card::CardId;
use crate::names::AgentName;
use crate::time::Millis;
use crate::tool::LimitKind;

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

/// A message the same as one accepted this recently is refused (P3-D18).
pub const DUPLICATE_WINDOW: Duration = Duration::from_secs(600);
/// The window the rate limit counts over (TD §6.3).
pub const RATE_WINDOW: Duration = Duration::from_secs(60);
/// The most messages one agent sends one recipient within [`RATE_WINDOW`].
pub const RATE_LIMIT: usize = 5;
/// The most messages waiting for one recipient.
pub const INBOX_LIMIT: usize = 20;

/// TD §6.3's limits on an agent's message from `from` to `to`, checked against
/// the messages accepted recently (any status). You are never limited.
///
/// # Errors
/// The limit the message would break.
pub fn check_limits(
    recent: &[Message],
    from: &AgentName,
    to: &AgentName,
    text: &str,
    now: Millis,
) -> Result<(), LimitKind> {
    let sender = Sender::Agent(from.clone());
    let within = |m: &Message, window: Duration| now - m.queued_at < window;
    let pair = || recent.iter().filter(|m| m.from == sender && &m.to == to);
    if pair().any(|m| within(m, DUPLICATE_WINDOW) && m.text.trim() == text.trim()) {
        return Err(LimitKind::Duplicate);
    }
    if pair().filter(|m| within(m, RATE_WINDOW)).count() >= RATE_LIMIT {
        return Err(LimitKind::Rate);
    }
    let waiting = recent
        .iter()
        .filter(|m| &m.to == to && m.status == MessageStatus::Queued)
        .count();
    if waiting >= INBOX_LIMIT {
        return Err(LimitKind::InboxFull);
    }
    Ok(())
}

/// An agent's message as pasted (P3-D18), framed by `boundary`, which the
/// caller minted so that it isn't in `text`.
pub fn frame(from: &AgentName, boundary: &str, text: &str) -> String {
    format!(
        "[message from {from} · DuoHive boundary {boundary}]\n{text}\n\
         [end of message {boundary} · reply with send_message to {from}]"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::n;

    fn sent(from: &str, to: &str, text: &str, at: u64, status: MessageStatus) -> Message {
        Message {
            id: MessageId(at),
            from: Sender::Agent(n(from)),
            to: n(to),
            kind: MessageKind::Text,
            text: text.to_owned(),
            status,
            queued_at: Millis(at),
            delivered_at: None,
        }
    }

    #[test]
    fn duplicate_is_trimmed_text_within_ten_minutes() {
        let dropped = MessageStatus::Dropped { reason: "x".into() };
        let recent = [sent("a", "b", "hi", 0, dropped)];
        let check = |text, at| check_limits(&recent, &n("a"), &n("b"), text, Millis(at));
        assert_eq!(check(" hi ", 599_999), Err(LimitKind::Duplicate));
        assert_eq!(check("hi", 600_000), Ok(()));
        assert_eq!(check("hello", 1), Ok(()));
        assert_eq!(
            check_limits(&recent, &n("c"), &n("b"), "hi", Millis(1)),
            Ok(())
        );
    }

    #[test]
    fn rate_and_inbox_full() {
        let five: Vec<Message> = (0..5)
            .map(|i| sent("a", "b", &format!("m{i}"), i, MessageStatus::Delivered))
            .collect();
        assert_eq!(
            check_limits(&five, &n("a"), &n("b"), "new", Millis(10)),
            Err(LimitKind::Rate)
        );
        assert_eq!(
            check_limits(&five, &n("a"), &n("c"), "new", Millis(10)),
            Ok(())
        );
        let full: Vec<Message> = (0..20)
            .map(|i| {
                sent(
                    "x",
                    "b",
                    &format!("m{i}"),
                    i * 60_000,
                    MessageStatus::Queued,
                )
            })
            .collect();
        assert_eq!(
            check_limits(&full, &n("a"), &n("b"), "new", Millis(20 * 60_000)),
            Err(LimitKind::InboxFull)
        );
    }

    #[test]
    fn frame_has_p3_d18_shape() {
        assert_eq!(
            frame(&n("squad-1"), "b00000000001", "run the tests"),
            "[message from squad-1 · DuoHive boundary b00000000001]\nrun the tests\n\
             [end of message b00000000001 · reply with send_message to squad-1]"
        );
    }
}
