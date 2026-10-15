//! Desktop alerts about agents.

use serde::{Deserialize, Serialize};

use crate::names::AgentName;
use crate::status::{AgentState, Status};

/// Why an alert fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertKind {
    /// The agent is waiting for the user.
    NeedsYou,
    /// The agent finished its task.
    Finished,
}

/// One desktop alert.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alert {
    /// Which agent.
    pub agent: AgentName,
    /// Why.
    pub kind: AlertKind,
    /// The alert's title.
    pub title: String,
    /// The alert's body.
    pub body: String,
}

/// The alert a move from `before` to `after` raises, if any: the agent entered
/// waiting or errored, went quiet, or (not Duo) finished.
pub fn alert_for(
    agent: &AgentName,
    is_duo: bool,
    before: &Status,
    after: &Status,
    last_reply: Option<&str>,
) -> Option<Alert> {
    let kind = if after.state != before.state {
        match after.state {
            AgentState::Waiting | AgentState::Errored => AlertKind::NeedsYou,
            AgentState::Finished if !is_duo => AlertKind::Finished,
            _ => return None,
        }
    } else if after.quiet && !before.quiet {
        AlertKind::NeedsYou
    } else {
        return None;
    };
    let title = match kind {
        AlertKind::NeedsYou => format!("{agent} needs you"),
        AlertKind::Finished => format!("{agent} finished"),
    };
    Some(Alert {
        agent: agent.clone(),
        kind,
        title,
        body: last_reply.unwrap_or_default().to_owned(),
    })
}
