//! The ports the engine talks through; real ones live in other crates.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::alert::Alert;
use crate::hook::HookMessage;
use crate::launch::{Key, LaunchCommand, Token, WindowId};
use crate::names::AgentName;
use crate::org::Org;
use crate::settings::Settings;
use crate::status::Status;
use crate::time::Millis;
use crate::view::{PaneInput, PaneSize, PaneStream, StreamId, ViewError};
use crate::worktree::{BaseRef, MergeState, TaskId};

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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveWindow {
    /// Which agent.
    pub agent: AgentName,
    /// Its window.
    pub window: WindowId,
    /// `Some(status)` when the pane is dead (status `None` = killed by a signal).
    #[serde(with = "nested_option")]
    pub dead: Option<Option<i32>>,
    /// Unix seconds of the last activity.
    pub activity_secs: u64,
}

/// `Option<Option<T>>` as `null` or a one-element array, so `Some(None)` survives JSON.
mod nested_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<Option<i32>>, s: S) -> Result<S::Ok, S::Error> {
        v.map(|inner| [inner]).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<i32>>, D::Error> {
        Ok(Option::<[Option<i32>; 1]>::deserialize(d)?.map(|[inner]| inner))
    }
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// The last org that parsed and validated.
    fn last_good_org(&mut self) -> Option<Org>;
    /// Saves the org; it becomes the last good org too (the engine saves only validated orgs).
    fn save_org(&mut self, org: &Org) -> Result<(), StoreError>;
    /// Records `org` as the last good org: one the engine accepted at start-up or on reload (D10).
    fn mark_last_good(&mut self, org: &Org) -> Result<(), StoreError>;
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
    /// Queues a hook DuoHive took but did not apply before shutting down, after
    /// any already queued, for the next start to replay (Review Focus 3).
    fn queue_pending_hook(&mut self, msg: &HookMessage, at: Millis) -> Result<(), StoreError>;
}

/// A live view of one agent's pane. Opening a stream closes any other.
pub trait PaneView: Send + Sync {
    /// Streams `agent`'s pane at `size`. The first chunk is a `Reset`, or, when the agent
    /// has no window (`"<agent> is not running"`) or the view can't stream it, a
    /// `Closed`, which is then the only chunk: that case is `Ok`, not
    /// [`ViewError::NotRunning`].
    fn open(&self, agent: &AgentName, size: PaneSize) -> Result<PaneStream, ViewError>;
    /// Resizes the pane to follow the view.
    fn resize(&self, stream: StreamId, size: PaneSize) -> Result<(), ViewError>;
    /// Sends the user's input to the pane.
    fn send(&self, stream: StreamId, input: PaneInput) -> Result<(), ViewError>;
    /// Asks for the last `lines` lines of history; the answer arrives on the receiver.
    fn scrollback(
        &self,
        stream: StreamId,
        lines: u16,
    ) -> Result<tokio::sync::oneshot::Receiver<Vec<u8>>, ViewError>;
}

/// Desktop alerts.
pub trait Notifier: Send + std::fmt::Debug {
    /// Whether alerts can be shown at all.
    fn available(&self) -> bool;
    /// Shows `alert`.
    fn notify(&mut self, alert: &Alert);
}

/// Git worktrees. Fire-and-forget; answers return as [`GitEvent`] inputs.
pub trait Git: Send + std::fmt::Debug {
    /// Creates a worktree and its branch.
    fn create(&mut self, req: CreateWorktree);
    /// Reports whether the worktree's branch has been merged and whether it is dirty.
    fn check(&mut self, req: CheckWorktree);
    /// Removes the worktree, and its branch when asked.
    fn remove(&mut self, req: RemoveWorktree);
}

/// A request to create a worktree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateWorktree {
    /// Which agent.
    pub agent: AgentName,
    /// Which task.
    pub task: TaskId,
    /// The repository.
    pub repo: PathBuf,
    /// Where the worktree goes.
    pub path: PathBuf,
    /// The branch to create.
    pub branch: String,
}

/// A request to check a worktree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckWorktree {
    /// Which agent.
    pub agent: AgentName,
    /// Which task.
    pub task: TaskId,
    /// The repository.
    pub repo: PathBuf,
    /// The worktree.
    pub path: PathBuf,
    /// Its branch.
    pub branch: String,
    /// The base commit the branch was created on.
    pub created_from: String,
}

/// A request to remove a worktree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveWorktree {
    /// Which agent.
    pub agent: AgentName,
    /// Which task.
    pub task: TaskId,
    /// The repository.
    pub repo: PathBuf,
    /// The worktree.
    pub path: PathBuf,
    /// Its branch.
    pub branch: String,
    /// Also delete the branch.
    pub delete_branch: bool,
    /// Remove even when the worktree is dirty.
    pub force: bool,
}

/// Why a git request failed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitError {
    /// The branch name is taken.
    BranchExists,
    /// The directory is not a git repository.
    NotARepo,
    /// Anything else.
    Other(String),
}

/// What git reports back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitEvent {
    /// A worktree was created, or not.
    Created {
        /// Which agent.
        agent: AgentName,
        /// Which task.
        task: TaskId,
        /// The base ref and its commit id, or why it failed.
        result: Result<(BaseRef, String), GitError>,
    },
    /// A worktree was checked.
    Checked {
        /// Which agent.
        agent: AgentName,
        /// Which task.
        task: TaskId,
        /// Whether the branch reached its base.
        merge: MergeState,
        /// Files with uncommitted changes.
        dirty_files: u32,
        /// The base ref found, if any.
        base: Option<BaseRef>,
    },
    /// A worktree was removed, or not.
    Removed {
        /// Which agent.
        agent: AgentName,
        /// Which task.
        task: TaskId,
        /// Why it failed, if it did.
        result: Result<(), String>,
    },
}

/// A [`Notifier`] for systems with no desktop alerts.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoNotifier;

impl Notifier for NoNotifier {
    fn available(&self) -> bool {
        false
    }
    fn notify(&mut self, _alert: &Alert) {}
}

/// A [`Git`] that answers nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoGit;

impl Git for NoGit {
    fn create(&mut self, _req: CreateWorktree) {}
    fn check(&mut self, _req: CheckWorktree) {}
    fn remove(&mut self, _req: RemoveWorktree) {}
}

/// A [`PaneView`] with no live view behind it: every stream closes before its first
/// `Reset`, and every other call fails with [`ViewError::Closed`].
#[derive(Clone, Copy, Debug, Default)]
pub struct NoPaneView;

impl PaneView for NoPaneView {
    fn open(&self, _agent: &AgentName, _size: PaneSize) -> Result<PaneStream, ViewError> {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        // A fresh channel has room for one chunk.
        let _ = tx.try_send(crate::view::PaneChunk::Closed {
            reason: "the live view isn't available".to_owned(),
        });
        Ok(PaneStream {
            id: StreamId(0),
            rx,
        })
    }

    fn resize(&self, _stream: StreamId, _size: PaneSize) -> Result<(), ViewError> {
        Err(ViewError::Closed)
    }

    fn send(&self, _stream: StreamId, _input: PaneInput) -> Result<(), ViewError> {
        Err(ViewError::Closed)
    }

    fn scrollback(
        &self,
        _stream: StreamId,
        _lines: u16,
    ) -> Result<tokio::sync::oneshot::Receiver<Vec<u8>>, ViewError> {
        Err(ViewError::Closed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alert::{Alert, AlertKind};
    use crate::testkit::*;
    use crate::view::*;
    use crate::worktree::*;

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

    fn row() -> SessionRow {
        SessionRow {
            agent: n("a"),
            token: Some(Token("t".into())),
            window: Some(WindowId("@1".into())),
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
        }
    }

    #[test]
    fn fake_store_round_trips_and_can_fail_once() {
        let mut s = FakeStore::default();
        assert_eq!(s.load_org(), Ok(None));
        s.state.lock().unwrap().fail_next_save = true;
        assert!(s.save_org(&example_org()).is_err());
        assert!(s.save_org(&example_org()).is_ok());
        s.save_session(&row()).unwrap();
        assert_eq!(s.load_sessions().unwrap(), vec![row()]);
        s.delete_session(&n("a")).unwrap();
        assert!(s.load_sessions().unwrap().is_empty());
        s.state.lock().unwrap().pending.push(HookMessage {
            sent_at_ms: 0,
            token: "t".into(),
            agent: "a".into(),
            event: "e".into(),
            payload: serde_json::json!({}),
        });
        assert_eq!(s.take_pending_hooks().unwrap().len(), 1);
        assert!(s.take_pending_hooks().unwrap().is_empty());
    }

    #[test]
    fn fake_store_is_observable_and_steerable_once_boxed() {
        let handle = FakeStore::default();
        let state = handle.state.clone();
        let mut boxed: Box<dyn Store> = Box::new(handle.clone());
        boxed.save_org(&example_org()).unwrap();
        boxed.save_settings(&Settings::default()).unwrap();
        boxed.delete_session(&n("a")).unwrap();
        state.lock().unwrap().fail_next_save = true;
        assert!(boxed.save_session(&row()).is_err());
        let st = handle.state.lock().unwrap();
        assert_eq!(st.saved_orgs.len(), 1);
        assert_eq!(st.saved_settings.len(), 1);
        assert_eq!(st.deleted_sessions, vec![n("a")]);
    }

    #[test]
    fn port_types_round_trip_json() {
        let ev = RuntimeEvent::Inventory(vec![LiveWindow {
            agent: n("a"),
            window: WindowId("@2".into()),
            dead: Some(None),
            activity_secs: 7,
        }]);
        let back: RuntimeEvent =
            serde_json::from_str(&serde_json::to_string(&ev).unwrap()).unwrap();
        assert_eq!(back, ev);
        let back: SessionRow =
            serde_json::from_str(&serde_json::to_string(&row()).unwrap()).unwrap();
        assert_eq!(back, row());
    }

    fn create_req() -> CreateWorktree {
        CreateWorktree {
            agent: n("a"),
            task: TaskId("t-1".into()),
            repo: "/repo".into(),
            path: "/repo/.wt/t-1".into(),
            branch: "duohive/t-1".into(),
        }
    }

    fn check_req() -> CheckWorktree {
        CheckWorktree {
            agent: n("a"),
            task: TaskId("t-1".into()),
            repo: "/repo".into(),
            path: "/repo/.wt/t-1".into(),
            branch: "duohive/t-1".into(),
            created_from: "abc123".into(),
        }
    }

    fn remove_req() -> RemoveWorktree {
        RemoveWorktree {
            agent: n("a"),
            task: TaskId("t-1".into()),
            repo: "/repo".into(),
            path: "/repo/.wt/t-1".into(),
            branch: "duohive/t-1".into(),
            delete_branch: true,
            force: false,
        }
    }

    #[test]
    fn fakes_record_calls() {
        let mut git = FakeGit::default();
        let calls = git.calls.clone();
        git.create(create_req());
        git.check(check_req());
        git.remove(remove_req());
        assert_eq!(
            *calls.lock().unwrap(),
            vec![
                GitCall::Create(create_req()),
                GitCall::Check(check_req()),
                GitCall::Remove(remove_req()),
            ]
        );

        let mut notifier = FakeNotifier::new(true);
        let alerts = notifier.alerts.clone();
        let alert = Alert {
            agent: n("a"),
            kind: AlertKind::NeedsYou,
            title: "a needs you".into(),
            body: "ok?".into(),
        };
        assert!(notifier.available());
        notifier.notify(&alert);
        notifier.notify(&alert);
        assert_eq!(*alerts.lock().unwrap(), vec![alert.clone(), alert]);
        assert!(!FakeNotifier::new(false).available());

        let view = FakePaneView::default();
        let size = PaneSize { cols: 80, rows: 24 };
        let mut stream = view.open(&n("a"), size).unwrap();
        view.resize(stream.id, size).unwrap();
        view.send(stream.id, PaneInput::Text("hi".into())).unwrap();
        view.scrollback(stream.id, 50).unwrap();
        assert_eq!(
            view.calls(),
            vec![
                PaneCall::Open(n("a"), size),
                PaneCall::Resize(stream.id, size),
                PaneCall::Send(stream.id, PaneInput::Text("hi".into())),
                PaneCall::Scrollback(stream.id, 50),
            ]
        );
        view.push(stream.id, PaneChunk::Bytes(b"x".to_vec()));
        view.close(stream.id, "gone");
        assert_eq!(stream.rx.try_recv(), Ok(PaneChunk::Bytes(b"x".to_vec())));
        assert_eq!(
            stream.rx.try_recv(),
            Ok(PaneChunk::Closed {
                reason: "gone".into()
            })
        );
        assert_eq!(
            view.send(stream.id, PaneInput::Key(PaneKey::Enter)),
            Err(ViewError::Closed)
        );
    }

    #[test]
    fn no_pane_view_streams_close_before_any_reset() {
        let view = NoPaneView;
        let size = PaneSize { cols: 80, rows: 24 };
        let mut stream = view.open(&n("a"), size).unwrap();
        assert!(matches!(stream.rx.try_recv(), Ok(PaneChunk::Closed { .. })));
        assert!(stream.rx.try_recv().is_err());
        assert_eq!(view.resize(stream.id, size), Err(ViewError::Closed));
        assert_eq!(
            view.send(stream.id, PaneInput::Text("x".into())),
            Err(ViewError::Closed)
        );
        assert!(view.scrollback(stream.id, 10).is_err());
    }

    #[test]
    fn fake_pane_view_knows_when_a_stream_is_dropped() {
        let view = FakePaneView::default();
        let size = PaneSize { cols: 80, rows: 24 };
        let stream = view.open(&n("a"), size).unwrap();
        let id = stream.id;
        assert!(view.receiving(id));
        drop(stream);
        assert!(!view.receiving(id));
        assert!(!view.receiving(StreamId(99)));
    }

    #[test]
    fn opening_a_second_stream_closes_the_first() {
        let view = FakePaneView::default();
        let size = PaneSize { cols: 80, rows: 24 };
        let mut first = view.open(&n("a"), size).unwrap();
        let second = view.open(&n("b"), size).unwrap();
        assert_ne!(first.id, second.id);
        assert!(matches!(first.rx.try_recv(), Ok(PaneChunk::Closed { .. })));
        assert_eq!(view.resize(first.id, size), Err(ViewError::Closed));
    }

    #[test]
    fn null_ports_do_nothing() {
        let mut notifier = NoNotifier;
        assert!(!notifier.available());
        notifier.notify(&Alert {
            agent: n("a"),
            kind: AlertKind::Finished,
            title: String::new(),
            body: String::new(),
        });
        // NoGit takes requests and never answers: it holds no channel to answer on.
        let mut git = NoGit;
        git.create(create_req());
        git.check(check_req());
        git.remove(remove_req());
    }

    #[test]
    fn git_events_round_trip_json() {
        let events = vec![
            GitEvent::Created {
                agent: n("a"),
                task: TaskId("t".into()),
                result: Ok((BaseRef("origin/main".into()), "abc".into())),
            },
            GitEvent::Created {
                agent: n("a"),
                task: TaskId("t".into()),
                result: Err(GitError::BranchExists),
            },
            GitEvent::Created {
                agent: n("a"),
                task: TaskId("t".into()),
                result: Err(GitError::Other("boom".into())),
            },
            GitEvent::Checked {
                agent: n("a"),
                task: TaskId("t".into()),
                merge: MergeState::NotMerged { files_changed: 3 },
                dirty_files: 1,
                base: Some(BaseRef("main".into())),
            },
            GitEvent::Checked {
                agent: n("a"),
                task: TaskId("t".into()),
                merge: MergeState::Merged(MergeKind::Squash),
                dirty_files: 0,
                base: None,
            },
            GitEvent::Checked {
                agent: n("a"),
                task: TaskId("t".into()),
                merge: MergeState::Unknown("no base".into()),
                dirty_files: 0,
                base: None,
            },
            GitEvent::Removed {
                agent: n("a"),
                task: TaskId("t".into()),
                result: Err("busy".into()),
            },
        ];
        for ev in events {
            let json = serde_json::to_string(&ev).unwrap();
            assert_eq!(
                serde_json::from_str::<GitEvent>(&json).unwrap(),
                ev,
                "{json}"
            );
        }
    }

    #[test]
    fn pane_types_round_trip_json() {
        let size = PaneSize {
            cols: 120,
            rows: 40,
        };
        let json = serde_json::to_string(&size).unwrap();
        assert_eq!(serde_json::from_str::<PaneSize>(&json).unwrap(), size);
        let inputs = vec![
            PaneInput::Text("x".into()),
            PaneInput::Paste("p".into()),
            PaneInput::Key(PaneKey::F(5)),
            PaneInput::Key(PaneKey::Ctrl('c')),
            PaneInput::Key(PaneKey::Alt('b')),
            PaneInput::Key(PaneKey::BackTab),
        ];
        for i in inputs {
            let json = serde_json::to_string(&i).unwrap();
            assert_eq!(
                serde_json::from_str::<PaneInput>(&json).unwrap(),
                i,
                "{json}"
            );
        }
        let chunks = vec![
            PaneChunk::Reset {
                bytes: vec![27, 91],
                cursor: (3, 4),
            },
            PaneChunk::Bytes(vec![1, 2]),
            PaneChunk::Closed { reason: "r".into() },
        ];
        for c in chunks {
            let json = serde_json::to_string(&c).unwrap();
            assert_eq!(
                serde_json::from_str::<PaneChunk>(&json).unwrap(),
                c,
                "{json}"
            );
        }
        let alert = Alert {
            agent: n("a"),
            kind: AlertKind::NeedsYou,
            title: "t".into(),
            body: "b".into(),
        };
        let json = serde_json::to_string(&alert).unwrap();
        assert_eq!(serde_json::from_str::<Alert>(&json).unwrap(), alert);
        assert_eq!(
            ViewError::NotRunning(n("a")).to_string(),
            "a is not running"
        );
    }
}
