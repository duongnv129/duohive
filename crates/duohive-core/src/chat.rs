//! The Duo chat transcript (TD §6.5, §7).

use serde::{Deserialize, Serialize};

use crate::card::{CardId, CardOutcome};
use crate::inbox::MessageId;
use crate::time::Millis;

/// Names one chat entry; counts up from 1 and is never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChatId(pub u64);

/// What kind of line a chat entry is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatKind {
    /// Something you typed; `message` when it was an `@agent` message.
    You {
        /// The message it became, if any.
        message: Option<MessageId>,
    },
    /// Duo's reply.
    Duo,
    /// A `→` action line with its result word (P3-D14).
    Action {
        /// The result word.
        result: String,
    },
    /// A collapsed answered card.
    CardResult {
        /// Which card.
        card: CardId,
        /// How it was answered.
        outcome: CardOutcome,
    },
    /// A line the TUI shows and never stores (`/help`, `/agents`).
    Note,
}

/// One line of the transcript.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatEntry {
    /// Its id.
    pub id: ChatId,
    /// When.
    pub at: Millis,
    /// What kind.
    pub kind: ChatKind,
    /// The text.
    pub text: String,
}
