//! What the engine tells the UI (TD §3.2).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::alert::Alert;
use crate::names::AgentName;
use crate::org::Org;
use crate::ports::OrgFileError;
use crate::status::{AgentState, Status};
use crate::time::Millis;
use crate::worktree::{TaskId, WorktreeRow};

/// What the UI shows for one agent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentView {
    /// State and overlays.
    pub status: Status,
    /// The last reply, if any.
    pub last_reply: Option<String>,
    /// A notice such as an untested CLI version.
    pub notice: Option<String>,
    /// A configuration problem on the row.
    pub problem: Option<String>,
    /// The worktree the agent runs in, if any.
    pub worktree: Option<TaskId>,
    /// A delivery is waiting on the agent.
    pub delivery_pending: bool,
}

/// One entry in the Needs you list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedsYouItem {
    /// The agent.
    pub agent: AgentName,
    /// Its state.
    pub state: AgentState,
    /// Whether it is quiet.
    pub quiet: bool,
    /// When it entered this state (for quiet, when the silence began, D13).
    pub since: Millis,
    /// A short excerpt of what it needs.
    pub excerpt: Option<String>,
}

/// Everything the UI needs to draw a first frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// The org.
    pub org: Org,
    /// Per-agent views.
    pub agents: BTreeMap<AgentName, AgentView>,
    /// Needs you, oldest first.
    pub needs_you: Vec<NeedsYouItem>,
    /// `org.toml` problem, if any.
    pub org_error: Option<OrgFileError>,
    /// D29: `settings.toml` invalid at start-up, defaults in use.
    pub settings_error: Option<OrgFileError>,
    /// Running session rows awaiting the tmux inventory (UI §4.1.2); 0 = none.
    pub reconnecting: u32,
    /// The worktrees; empty until they are listed.
    pub worktrees: Vec<WorktreeRow>,
}

/// A change the engine reports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    /// The whole state.
    Snapshot(Box<Snapshot>),
    /// The org changed.
    OrgChanged {
        /// The new org.
        org: Box<Org>,
        /// Per-agent problems.
        problems: BTreeMap<AgentName, String>,
    },
    /// An agent's status changed.
    AgentStatusChanged {
        /// Which agent.
        agent: AgentName,
        /// The new status.
        status: Status,
    },
    /// An agent's last reply changed.
    LastReplyChanged {
        /// Which agent.
        agent: AgentName,
        /// The new reply.
        reply: Option<String>,
    },
    /// An agent's notice changed.
    AgentNoticeChanged {
        /// Which agent.
        agent: AgentName,
        /// The new notice.
        notice: Option<String>,
    },
    /// The Needs you list changed.
    NeedsYouChanged(Vec<NeedsYouItem>),
    /// The `org.toml` problem changed.
    OrgFileError(Option<OrgFileError>),
    /// A command was refused.
    CommandRejected {
        /// Why.
        reason: String,
    },
    /// Give work was refused because the agent is busy.
    GiveWorkRefused {
        /// Which agent.
        agent: AgentName,
        /// Its state.
        state: AgentState,
        /// A delivery is already waiting on it.
        pending: bool,
    },
    /// The start-up inventory has been applied.
    Reconnected,
    /// A desktop alert to show.
    Alert(Alert),
    /// Desktop alerts cannot be shown.
    AlertsUnavailable,
    /// The user clicked an agent's alert.
    AlertClicked {
        /// Which agent.
        agent: AgentName,
    },
    /// Give work was not taken, so the task never started.
    GiveWorkNotTaken {
        /// Which agent.
        agent: AgentName,
        /// The task as given, so it can be offered again.
        task: String,
        /// Why.
        reason: String,
    },
    /// The worktrees changed.
    Worktrees(Vec<WorktreeRow>),
    /// An agent's worktree or pending delivery changed.
    AgentRunChanged {
        /// Which agent.
        agent: AgentName,
        /// The worktree it runs in, if any.
        worktree: Option<TaskId>,
        /// A delivery is waiting on it.
        delivery_pending: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::*;
    use crate::names::{AgentName, NodeName};
    use crate::org::{DuoConfig, Fields, NodeKey, Org, RoleKey};
    use crate::settings::Settings;
    use crate::status::{AgentState, Status};
    use crate::testkit::{example_org, n};
    use crate::time::Millis;

    fn status() -> Status {
        Status {
            state: AgentState::Waiting,
            quiet: false,
            quiet_since: None,
            from_screen: true,
            since: Millis(5),
            error: None,
        }
    }

    fn nn(s: &str) -> NodeName {
        NodeName::parse(s).unwrap()
    }

    fn all_commands() -> Vec<Command> {
        let org = Org::blank(crate::cli_kind::CliKind::Claude);
        let rk = RoleKey {
            team: Some(nn("Payments")),
            role: nn("QA"),
        };
        let team = TeamDraft {
            name: nn("T"),
            defaults: Fields::default(),
        };
        let role = RoleDraft {
            name: nn("R"),
            defaults: Fields::default(),
        };
        let agent = AgentDraft {
            name: n("a-1"),
            fields: Fields::default(),
        };
        let duo: DuoConfig = org.duo.clone();
        vec![
            Command::StartAgent { agent: n("a") },
            Command::StopAgent { agent: n("a") },
            Command::GiveWork {
                agent: n("a"),
                task: "do $(it)".into(),
                run: crate::worktree::RunIn::Directory,
            },
            Command::ViewAgent { agent: n("a") },
            Command::EditOrg(OrgEdit::AddTeam(team.clone())),
            Command::EditOrg(OrgEdit::AddRole {
                team: None,
                role: role.clone(),
                agents: 2,
            }),
            Command::EditOrg(OrgEdit::AddAgent {
                role: rk.clone(),
                agent: agent.clone(),
            }),
            Command::EditOrg(OrgEdit::EditTeam {
                key: nn("T"),
                draft: team,
            }),
            Command::EditOrg(OrgEdit::EditRole {
                key: rk.clone(),
                draft: role,
            }),
            Command::EditOrg(OrgEdit::EditAgent {
                key: n("a"),
                draft: agent,
            }),
            Command::EditOrg(OrgEdit::EditOrgDefaults {
                defaults: org.defaults.clone(),
                duo,
            }),
            Command::EditOrg(OrgEdit::Delete(NodeKey::Role(rk))),
            Command::ReloadOrg,
            Command::UpdateSettings(Settings::default()),
            Command::Shutdown,
        ]
    }

    fn item() -> NeedsYouItem {
        NeedsYouItem {
            agent: n("a"),
            state: AgentState::Waiting,
            quiet: true,
            since: Millis(9),
            excerpt: Some("ok?".into()),
        }
    }

    fn all_events() -> Vec<Event> {
        let err = OrgFileError {
            line: Some(3),
            message: "bad".into(),
        };
        let mut agents = BTreeMap::new();
        agents.insert(
            n("a"),
            AgentView {
                status: status(),
                last_reply: Some("r".into()),
                notice: None,
                problem: Some("p".into()),
                worktree: None,
                delivery_pending: false,
            },
        );
        let snap = Snapshot {
            org: example_org(),
            agents,
            needs_you: vec![item()],
            org_error: Some(err.clone()),
            settings_error: None,
            reconnecting: 2,
            worktrees: Vec::new(),
        };
        let a: AgentName = n("a");
        vec![
            Event::Snapshot(Box::new(snap)),
            Event::OrgChanged {
                org: Box::new(example_org()),
                problems: BTreeMap::from([(a.clone(), "x".to_owned())]),
            },
            Event::AgentStatusChanged {
                agent: a.clone(),
                status: status(),
            },
            Event::LastReplyChanged {
                agent: a.clone(),
                reply: Some("hi".into()),
            },
            Event::AgentNoticeChanged {
                agent: a.clone(),
                notice: None,
            },
            Event::NeedsYouChanged(vec![item()]),
            Event::OrgFileError(Some(err)),
            Event::CommandRejected {
                reason: "no".into(),
            },
            Event::GiveWorkRefused {
                agent: a,
                state: AgentState::Working,
                pending: false,
            },
            Event::Reconnected,
        ]
    }

    #[test]
    fn new_events_round_trip_json() {
        use crate::alert::{Alert, AlertKind};
        use crate::time::Millis;
        use crate::worktree::{RunIn, TaskId, WorktreeRow, WorktreeState};
        let events = vec![
            Event::Alert(Alert {
                agent: n("a"),
                kind: AlertKind::Finished,
                title: "t".into(),
                body: "b".into(),
            }),
            Event::AlertsUnavailable,
            Event::AlertClicked { agent: n("a") },
            Event::GiveWorkNotTaken {
                agent: n("a"),
                task: "Fix the \"flaky\" test\nthen run $(it) ✓".into(),
                reason: "no sign it started".into(),
            },
            Event::Worktrees(vec![WorktreeRow {
                agent: n("a"),
                task: TaskId("t-1".into()),
                repo: "/r".into(),
                path: "/d/worktrees/a/t-1".into(),
                branch: "duohive/a/t-1".into(),
                created_at: Millis(1),
                created_from: "abc".into(),
                state: WorktreeState::CheckFailed("x".into()),
                checked_at: None,
                base: None,
            }]),
            Event::GiveWorkRefused {
                agent: n("a"),
                state: AgentState::Working,
                pending: true,
            },
            Event::AgentRunChanged {
                agent: n("a"),
                worktree: Some(TaskId("t-1".into())),
                delivery_pending: true,
            },
        ];
        for e in events {
            let json = serde_json::to_string(&e).unwrap();
            assert_eq!(serde_json::from_str::<Event>(&json).unwrap(), e, "{json}");
        }
        let c = Command::GiveWork {
            agent: n("a"),
            task: "x".into(),
            run: RunIn::NewWorktree,
        };
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), c);
    }

    #[test]
    fn commands_and_events_round_trip_json() {
        for c in all_commands() {
            let json = serde_json::to_string(&c).unwrap();
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), c, "{json}");
        }
        for e in all_events() {
            let json = serde_json::to_string(&e).unwrap();
            assert_eq!(serde_json::from_str::<Event>(&json).unwrap(), e, "{json}");
        }
    }
}
