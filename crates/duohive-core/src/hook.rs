//! A hook as it travels from `duohive hook` to the engine.

use serde::{Deserialize, Serialize};

/// One line on `run.sock`, or one `pending_hooks` row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookMessage {
    /// The per-launch token that authenticates the hook.
    pub token: String,
    /// The agent's name.
    pub agent: String,
    /// The CLI's event name.
    pub event: String,
    /// The CLI's payload, as received.
    pub payload: serde_json::Value,
    /// When `duohive hook` built the message (Unix ms, wall clock): replay
    /// orders by it (Review Focus 3). `0` from a sender that predates it.
    #[serde(default)]
    pub sent_at_ms: u64,
}
