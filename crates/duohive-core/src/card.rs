//! Approval cards: changes Duo proposes and you answer (TD §6.2, UI §4.6).

use serde::{Deserialize, Serialize};

use crate::command::{AgentDraft, OrgEdit};
use crate::names::AgentName;
use crate::org::RoleKey;
use crate::time::Millis;
use crate::worktree::RunIn;

/// Names one card; counts up from 1 and is never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CardId(pub u64);

/// What raised a card; both are shown as Duo's (P3-D3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardSource {
    /// Duo, through a tool.
    Duo,
    /// The engine, for a task to a role with no free agent.
    NoFreeAgent,
}

/// The change a card holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardChange {
    /// A change to the org.
    Org(OrgEdit),
    /// Stop a busy or starting agent.
    Stop {
        /// Which agent.
        agent: AgentName,
    },
    /// Create an agent in a role and give it the task the card holds.
    NewAgent {
        /// The role.
        role: RoleKey,
        /// The new agent.
        agent: AgentDraft,
        /// The task, kept unchanged by Edit (P3-D7).
        task: String,
        /// Where it runs.
        run: RunIn,
        /// The agent whose own fields were copied, if any (P3-D15).
        clone_of: Option<AgentName>,
    },
}

/// How a card was answered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardOutcome {
    /// Approved as proposed.
    Approved,
    /// Edited, then approved.
    EditedApproved,
    /// Rejected; nothing applied.
    Rejected,
    /// Approved, but it could no longer apply (P3-D21).
    NotApplied(String),
}

/// One card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    /// Its id.
    pub id: CardId,
    /// What raised it.
    pub source: CardSource,
    /// The change it holds.
    pub change: CardChange,
    /// When it was raised.
    pub raised_at: Millis,
    /// The answer and when, once answered.
    pub answer: Option<(CardOutcome, Millis)>,
}
