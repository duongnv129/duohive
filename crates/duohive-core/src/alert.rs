//! Desktop alerts about agents.

use serde::{Deserialize, Serialize};

use crate::names::AgentName;

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
