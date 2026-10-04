//! The ports the engine talks through; real ones live in other crates.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::hook::HookMessage;
use crate::launch::{Key, LaunchCommand, Token, WindowId};
use crate::names::AgentName;
use crate::org::Org;
use crate::settings::Settings;
use crate::status::Status;
use crate::time::Millis;

/// The wall clock.
pub trait Clock: Send {
    /// Now.
    fn now(&self) -> Millis;
}

/// Makes per-launch tokens.
pub trait Ids: Send {
    /// A fresh token.
    fn new_token(&mut self) -> Token;
}

/// The terminal multiplexer. Fire-and-forget; results return as [`RuntimeEvent`] inputs.
pub trait Runtime: Send {
    /// Creates `<run_root>/<agent>/` (0700), writes `cmd.files` and `launch.sh` there, opens the window.
    fn start(&mut self, agent: &AgentName, cmd: LaunchCommand);
    /// Sends `quit` keys, then kills the window after `grace`.
    fn stop(&mut self, agent: &AgentName, quit: Vec<Key>, grace: Duration);
    /// Kills that window only, never a newer one for the same agent.
    fn kill(&mut self, agent: &AgentName, window: &WindowId);
    /// Pastes `text`, then sends `submit`.
    fn paste(&mut self, agent: &AgentName, text: String, submit: Vec<Key>);
    /// Asks for the agent's screen.
    fn capture(&mut self, agent: &AgentName);
    /// Asks for the list of live windows.
    fn inventory(&mut self);
}

/// What the runtime reports back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeEvent {
    /// A window opened.
    Started {
        /// Which agent.
        agent: AgentName,
        /// Its window.
        window: WindowId,
    },
    /// A window could not be opened.
    StartFailed {
        /// Which agent.
        agent: AgentName,
        /// Why.
        reason: String,
    },
    /// The pane showed activity.
    Activity {
        /// Which agent.
        agent: AgentName,
        /// Unix seconds of the last activity.
        at_secs: u64,
    },
    /// The pane's process ended.
    PaneDied {
        /// Which agent.
        agent: AgentName,
        /// Its window.
        window: WindowId,
        /// Exit status; `None` when killed by a signal.
        status: Option<i32>,
    },
    /// The window is gone.
    WindowGone {
        /// Which agent.
        agent: AgentName,
        /// Its window.
        window: WindowId,
    },
    /// A captured screen.
    Screen {
        /// Which agent.
        agent: AgentName,
        /// The text.
        text: String,
    },
    /// A paste failed.
    PasteFailed {
        /// Which agent.
        agent: AgentName,
        /// Why.
        reason: String,
    },
    /// The live windows.
    Inventory(Vec<LiveWindow>),
    /// The tmux connection dropped.
    ConnectionLost {
        /// Why.
        reason: String,
    },
}

/// A window found by an inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveWindow {
    /// Which agent.
    pub agent: AgentName,
    /// Its window.
    pub window: WindowId,
    /// `Some(status)` when the pane is dead (status `None` = killed by a signal).
    pub dead: Option<Option<i32>>,
    /// Unix seconds of the last activity.
    pub activity_secs: u64,
}

/// A problem in a hand-edited file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrgFileError {
    /// The line, when known.
    pub line: Option<usize>,
    /// What is wrong.
    pub message: String,
}

/// An agent's persisted session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRow {
    /// Which agent.
    pub agent: AgentName,
    /// The launch token.
    pub token: Option<Token>,
    /// The window.
    pub window: Option<WindowId>,
    /// The status.
    pub status: Status,
    /// The last reply.
    pub last_reply: Option<String>,
    /// The launch carried a task.
    pub task_at_launch: bool,
    /// A session-end hook was seen.
    pub saw_end_hook: bool,
    /// The last task given.
    pub last_task: Option<String>,
}

/// A store failure.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct StoreError(pub String);

/// Persistence. Data access goes only through this port.
pub trait Store: Send {
    /// Loads `org.toml`; `Ok(None)` when there is no file yet.
    fn load_org(&mut self) -> Result<Option<Org>, OrgFileError>;
    /// The last org that parsed.
    fn last_good_org(&mut self) -> Option<Org>;
    /// Saves the org.
    fn save_org(&mut self, org: &Org) -> Result<(), StoreError>;
    /// Saves the settings.
    fn save_settings(&mut self, s: &Settings) -> Result<(), StoreError>;
    /// All session rows.
    fn load_sessions(&mut self) -> Result<Vec<SessionRow>, StoreError>;
    /// Inserts or replaces a session row.
    fn save_session(&mut self, row: &SessionRow) -> Result<(), StoreError>;
    /// Removes an agent's session row.
    fn delete_session(&mut self, agent: &AgentName) -> Result<(), StoreError>;
    /// Hooks queued while DuoHive was closed, oldest first; deletes them.
    fn take_pending_hooks(&mut self) -> Result<Vec<HookMessage>, StoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::*;

    #[test]
    fn fake_clock_advances_and_ids_count() {
        let c = FakeClock::new(Millis(1_000));
        let handle = c.clone();
        handle.advance(Duration::from_secs(2));
        assert_eq!(c.now(), Millis(3_000));
        let mut ids = FakeIds::default();
        assert_eq!(ids.new_token(), Token("tok-1".into()));
        assert_eq!(ids.new_token(), Token("tok-2".into()));
    }

    #[test]
    fn fake_runtime_records_calls() {
        let mut rt = FakeRuntime::default();
        let calls = rt.calls.clone();
        rt.capture(&n("a"));
        rt.inventory();
        rt.kill(&n("a"), &WindowId("@1".into()));
        assert_eq!(
            *calls.lock().unwrap(),
            vec![
                RuntimeCall::Capture(n("a")),
                RuntimeCall::Inventory,
                RuntimeCall::Kill(n("a"), WindowId("@1".into())),
            ]
        );
    }

    #[test]
    fn fake_store_round_trips_and_can_fail_once() {
        let mut s = FakeStore::default();
        assert_eq!(s.load_org(), Ok(None));
        s.fail_next_save = true;
        assert!(s.save_org(&example_org()).is_err());
        assert!(s.save_org(&example_org()).is_ok());
        let row = SessionRow {
            agent: n("a"),
            token: None,
            window: None,
            status: crate::status::Status {
                state: crate::status::AgentState::Stopped,
                quiet: false,
                quiet_since: None,
                from_screen: false,
                since: Millis(0),
                error: None,
            },
            last_reply: None,
            task_at_launch: false,
            saw_end_hook: false,
            last_task: None,
        };
        s.save_session(&row).unwrap();
        assert_eq!(s.load_sessions().unwrap(), vec![row]);
        s.delete_session(&n("a")).unwrap();
        assert!(s.load_sessions().unwrap().is_empty());
        s.pending.push(HookMessage {
            token: "t".into(),
            agent: "a".into(),
            event: "e".into(),
            payload: serde_json::json!({}),
        });
        assert_eq!(s.take_pending_hooks().unwrap().len(), 1);
        assert!(s.take_pending_hooks().unwrap().is_empty());
    }
}
