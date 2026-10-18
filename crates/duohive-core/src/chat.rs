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

/// How many entries the snapshot carries: the last 500 (P3-Q5). The `chat`
/// table keeps every entry.
pub const CHAT_LOAD: usize = 500;

/// `text` on one line, cut to `max` characters with `…` (P3-D14's quotes):
/// runs of whitespace, newlines included, become one space.
pub fn cut(text: &str, max: usize) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        return one_line;
    }
    let kept: String = one_line.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

#[cfg(test)]
mod tests {
    use super::cut;

    #[test]
    fn cut_keeps_short_text_and_joins_lines() {
        assert_eq!(cut("  rerun\n the  tests ", 40), "rerun the tests");
        assert_eq!(cut("abcdef", 6), "abcdef");
        assert_eq!(cut("abcdefg", 6), "abcde…");
        assert_eq!(cut("abcd efg", 6), "abcd…");
    }
}
