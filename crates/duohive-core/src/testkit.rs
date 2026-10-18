//! Test-only fixtures and fakes shared across crates.

use crate::adapter::{
    CliAdapter, CliPresence, DialogCheck, HookCoverage, HookSignal, LaunchError, ScreenSignal,
    ScreenState,
};
use crate::alert::Alert;
use crate::chat::{ChatEntry, ChatKind};
use crate::cli_kind::CliKind;
use crate::hook::HookMessage;
use crate::inbox::{Message, MessageId, MessageStatus};
use crate::inherit::expand_home;
use crate::launch::{Key, LaunchCommand, LaunchSpec, Token, WindowId};
use crate::names::{AgentName, NodeName};
use crate::org::{AgentDef, Fields, Org, Role, Team};
use crate::ports::{
    CheckWorktree, Clock, CreateWorktree, Git, Ids, Notifier, OrgFileError, PaneView,
    QueuedToolCall, RemoveWorktree, Runtime, SessionRow, Store, StoreError,
};
use crate::settings::Settings;
use crate::status::WaitingOn;
use crate::time::Millis;
use crate::validate::{DirProbe, DirStatus};
use crate::view::{PaneChunk, PaneInput, PaneSize, PaneStream, StreamId, ViewError};
use crate::worktree::{TaskId, WorktreeRow};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Parses an agent name, panicking on invalid input.
pub fn n(s: &str) -> AgentName {
    AgentName::parse(s).unwrap()
}

fn agent(name: &str) -> AgentDef {
    AgentDef {
        name: n(name),
        fields: Fields::default(),
    }
}

fn role(name: &str, agents: &[&str]) -> Role {
    Role {
        name: NodeName::parse(name).unwrap(),
        defaults: Fields::default(),
        agents: agents.iter().map(|a| agent(a)).collect(),
    }
}

/// The PRD's example org: Payments (Backend, QA) plus a root Generalist role.
pub fn example_org() -> Org {
    let mut org = Org::blank(CliKind::Claude);
    org.teams = vec![Team {
        name: NodeName::parse("Payments").unwrap(),
        defaults: Fields::default(),
        roles: vec![
            role("Backend", &["auth-service", "billing-api"]),
            role("QA", &["pay-qa"]),
        ],
    }];
    org.roles = vec![role("Generalist", &["squad-1", "squad-2", "squad-3"])];
    org
}

/// A [`DirProbe`] with canned answers (a path not listed is `Found`) and a
/// count of probe calls.
pub struct FakeDirs(pub HashMap<PathBuf, DirStatus>, pub Arc<AtomicUsize>);

impl FakeDirs {
    /// Fake with these statuses and no calls yet.
    pub fn new(statuses: HashMap<PathBuf, DirStatus>) -> Self {
        Self(statuses, Arc::new(AtomicUsize::new(0)))
    }
}

impl DirProbe for FakeDirs {
    fn probe(&self, dir: &Path) -> DirStatus {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.get(dir).copied().unwrap_or(DirStatus::Found)
    }
}

/// A clock the test moves by hand; clones share the same time.
#[derive(Clone, Debug)]
pub struct FakeClock(Arc<AtomicU64>);

impl FakeClock {
    /// A clock showing `start`.
    pub fn new(start: Millis) -> Self {
        Self(Arc::new(AtomicU64::new(start.0)))
    }

    /// Moves time forward.
    pub fn advance(&self, by: Duration) {
        let ms = u64::try_from(by.as_millis()).unwrap_or(u64::MAX);
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Millis {
        Millis(self.0.load(Ordering::SeqCst))
    }
}

/// A clock that follows tokio's clock from `start`, so paused-time tests move it.
#[derive(Clone, Debug)]
pub struct TokioClock {
    start: Millis,
    origin: tokio::time::Instant,
}

impl TokioClock {
    /// A clock showing `start` now.
    pub fn new(start: Millis) -> Self {
        Self {
            start,
            origin: tokio::time::Instant::now(),
        }
    }
}

impl Clock for TokioClock {
    fn now(&self) -> Millis {
        self.start + self.origin.elapsed()
    }
}

/// Tokens `tok-1`, `tok-2`, and so on; boundaries `b00000000001`, `b00000000002`, ….
#[derive(Debug, Default)]
pub struct FakeIds {
    issued: u64,
    boundaries: u64,
}

impl Ids for FakeIds {
    fn new_token(&mut self) -> Token {
        self.issued += 1;
        Token(format!("tok-{}", self.issued))
    }
    fn boundary(&mut self) -> String {
        self.boundaries += 1;
        format!("b{:011x}", self.boundaries)
    }
}

/// One call made on a [`FakeRuntime`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeCall {
    /// `start`.
    Start(AgentName, LaunchCommand),
    /// `stop`.
    Stop(AgentName, Vec<Key>, Duration),
    /// `kill`.
    Kill(AgentName, WindowId),
    /// `paste`: agent, before keys, text, submit keys.
    Paste(AgentName, Vec<Key>, String, Vec<Key>),
    /// `capture`.
    Capture(AgentName),
    /// `inventory`.
    Inventory,
}

/// A [`Runtime`] that records its calls.
#[derive(Clone, Debug, Default)]
pub struct FakeRuntime {
    /// Calls so far, oldest first.
    pub calls: Arc<Mutex<Vec<RuntimeCall>>>,
    /// The dialog check of each `paste`, oldest first.
    pub dialog_checks: Arc<Mutex<Vec<Option<DialogCheck>>>>,
}

impl FakeRuntime {
    fn record(&self, call: RuntimeCall) {
        self.calls.lock().unwrap().push(call);
    }
}

impl Runtime for FakeRuntime {
    fn start(&mut self, agent: &AgentName, cmd: LaunchCommand) {
        self.record(RuntimeCall::Start(agent.clone(), cmd));
    }
    fn stop(&mut self, agent: &AgentName, quit: Vec<Key>, grace: Duration) {
        self.record(RuntimeCall::Stop(agent.clone(), quit, grace));
    }
    fn kill(&mut self, agent: &AgentName, window: &WindowId) {
        self.record(RuntimeCall::Kill(agent.clone(), window.clone()));
    }
    fn paste(
        &mut self,
        agent: &AgentName,
        before: Vec<Key>,
        dialog: Option<DialogCheck>,
        text: String,
        submit: Vec<Key>,
    ) {
        self.dialog_checks.lock().unwrap().push(dialog);
        self.record(RuntimeCall::Paste(agent.clone(), before, text, submit));
    }
    fn capture(&mut self, agent: &AgentName) {
        self.record(RuntimeCall::Capture(agent.clone()));
    }
    fn inventory(&mut self) {
        self.record(RuntimeCall::Inventory);
    }
}

/// What a [`FakeStore`] holds; read and steer it through [`FakeStore::state`].
#[derive(Debug)]
pub struct FakeStoreState {
    /// What `load_org` returns.
    pub org_result: Result<Option<Org>, OrgFileError>,
    /// What `last_good_org` returns.
    pub last_good: Option<Org>,
    /// Makes the next save (org, settings or session) fail, once.
    pub fail_next_save: bool,
    /// Every org saved, oldest first.
    pub saved_orgs: Vec<Org>,
    /// Every `mark_last_good` call that succeeded, oldest first.
    pub marked_last_good: Vec<Org>,
    /// Every settings value saved, oldest first.
    pub saved_settings: Vec<Settings>,
    /// Session rows by agent.
    pub sessions: BTreeMap<AgentName, SessionRow>,
    /// Every `delete_session` call, oldest first.
    pub deleted_sessions: Vec<AgentName>,
    /// Worktree rows by agent and task.
    pub worktrees: BTreeMap<(AgentName, TaskId), WorktreeRow>,
    /// Queued hooks, oldest first.
    pub pending: Vec<HookMessage>,
    /// Tool calls queued while closed, oldest first.
    pub queued_tool_calls: Vec<QueuedToolCall>,
    /// Messages by id.
    pub messages: BTreeMap<MessageId, Message>,
    /// Chat entries, oldest first.
    pub chat: Vec<ChatEntry>,
}

impl Default for FakeStoreState {
    fn default() -> Self {
        Self {
            org_result: Ok(None),
            last_good: None,
            fail_next_save: false,
            saved_orgs: Vec::new(),
            marked_last_good: Vec::new(),
            saved_settings: Vec::new(),
            sessions: BTreeMap::new(),
            deleted_sessions: Vec::new(),
            worktrees: BTreeMap::new(),
            pending: Vec::new(),
            queued_tool_calls: Vec::new(),
            messages: BTreeMap::new(),
            chat: Vec::new(),
        }
    }
}

/// An in-memory [`Store`]; clones share one state, so a test keeps a handle
/// after boxing the store into the engine.
#[derive(Clone, Debug, Default)]
pub struct FakeStore {
    /// The shared state.
    pub state: Arc<Mutex<FakeStoreState>>,
}

impl FakeStore {
    fn check_save(st: &mut FakeStoreState) -> Result<(), StoreError> {
        if std::mem::take(&mut st.fail_next_save) {
            return Err(StoreError("disk full".to_owned()));
        }
        Ok(())
    }
}

impl Store for FakeStore {
    fn load_org(&mut self) -> Result<Option<Org>, OrgFileError> {
        self.state.lock().unwrap().org_result.clone()
    }
    fn last_good_org(&mut self) -> Option<Org> {
        self.state.lock().unwrap().last_good.clone()
    }
    fn save_org(&mut self, org: &Org) -> Result<(), StoreError> {
        let mut st = self.state.lock().unwrap();
        Self::check_save(&mut st)?;
        st.saved_orgs.push(org.clone());
        st.last_good = Some(org.clone());
        st.org_result = Ok(Some(org.clone()));
        Ok(())
    }
    fn mark_last_good(&mut self, org: &Org) -> Result<(), StoreError> {
        let mut st = self.state.lock().unwrap();
        Self::check_save(&mut st)?;
        st.marked_last_good.push(org.clone());
        st.last_good = Some(org.clone());
        Ok(())
    }
    fn save_settings(&mut self, s: &Settings) -> Result<(), StoreError> {
        let mut st = self.state.lock().unwrap();
        Self::check_save(&mut st)?;
        st.saved_settings.push(s.clone());
        Ok(())
    }
    fn load_sessions(&mut self) -> Result<Vec<SessionRow>, StoreError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .sessions
            .values()
            .cloned()
            .collect())
    }
    fn save_session(&mut self, row: &SessionRow) -> Result<(), StoreError> {
        let mut st = self.state.lock().unwrap();
        Self::check_save(&mut st)?;
        st.sessions.insert(row.agent.clone(), row.clone());
        Ok(())
    }
    fn delete_session(&mut self, agent: &AgentName) -> Result<(), StoreError> {
        let mut st = self.state.lock().unwrap();
        st.sessions.remove(agent);
        st.deleted_sessions.push(agent.clone());
        Ok(())
    }
    fn load_worktrees(&mut self) -> Result<Vec<WorktreeRow>, StoreError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .worktrees
            .values()
            .cloned()
            .collect())
    }
    fn save_worktree(&mut self, row: &WorktreeRow) -> Result<(), StoreError> {
        let mut st = self.state.lock().unwrap();
        Self::check_save(&mut st)?;
        st.worktrees
            .insert((row.agent.clone(), row.task.clone()), row.clone());
        Ok(())
    }
    fn delete_worktree(&mut self, agent: &AgentName, task: &TaskId) -> Result<(), StoreError> {
        self.state
            .lock()
            .unwrap()
            .worktrees
            .remove(&(agent.clone(), task.clone()));
        Ok(())
    }
    fn take_pending_hooks(&mut self) -> Result<Vec<HookMessage>, StoreError> {
        Ok(std::mem::take(&mut self.state.lock().unwrap().pending))
    }
    fn queue_pending_hook(&mut self, msg: &HookMessage, _at: Millis) -> Result<(), StoreError> {
        self.state.lock().unwrap().pending.push(msg.clone());
        Ok(())
    }

    fn take_queued_tool_calls(&mut self) -> Result<Vec<QueuedToolCall>, StoreError> {
        Ok(std::mem::take(
            &mut self.state.lock().unwrap().queued_tool_calls,
        ))
    }

    fn load_messages(&mut self, since: Millis) -> Result<Vec<Message>, StoreError> {
        let st = self.state.lock().unwrap();
        let newest = st.messages.keys().next_back().copied();
        Ok(st
            .messages
            .values()
            .filter(|m| {
                matches!(m.status, MessageStatus::Queued | MessageStatus::Delivered)
                    || m.queued_at >= since
                    || Some(m.id) == newest
            })
            .cloned()
            .collect())
    }

    fn find_message(&mut self, id: MessageId) -> Result<Option<Message>, StoreError> {
        Ok(self.state.lock().unwrap().messages.get(&id).cloned())
    }

    fn save_message(&mut self, m: &Message) -> Result<(), StoreError> {
        let mut st = self.state.lock().unwrap();
        Self::check_save(&mut st)?;
        st.messages.insert(m.id, m.clone());
        Ok(())
    }

    fn load_chat(&mut self, last: usize) -> Result<Vec<ChatEntry>, StoreError> {
        let st = self.state.lock().unwrap();
        Ok(st.chat[st.chat.len().saturating_sub(last)..].to_vec())
    }

    fn save_chat(&mut self, entry: &ChatEntry) -> Result<(), StoreError> {
        let mut st = self.state.lock().unwrap();
        Self::check_save(&mut st)?;
        if entry.kind != ChatKind::Note {
            st.chat.push(entry.clone());
        }
        Ok(())
    }
}

/// A scriptable [`CliAdapter`].
#[derive(Clone, Debug)]
pub struct StubAdapter {
    /// Which CLI it stands in for.
    pub kind: CliKind,
    /// What `coverage()` returns.
    pub coverage: HookCoverage,
    /// What `detect()` returns.
    pub presence: CliPresence,
    /// What `startup_prompts()` returns.
    pub startup_prompts: bool,
    /// Every spec `launch` received, oldest first; shared by clones.
    pub launches: Arc<Mutex<Vec<LaunchSpec>>>,
    /// What `enrich_hook` does; `None` leaves the payload alone.
    pub enrich: Option<fn(&str, &mut serde_json::Value)>,
    /// When set, `launch` fails with this text (after recording the spec).
    pub launch_error: Option<String>,
    /// What `before_paste_keys()` returns.
    pub before_paste: Vec<Key>,
    /// What `dialog_check()` returns.
    pub dialog: Option<DialogCheck>,
}

impl StubAdapter {
    /// Complete coverage, found at 2.1.0, no start-up prompts.
    pub fn new(kind: CliKind) -> Self {
        Self {
            kind,
            coverage: HookCoverage {
                ready: true,
                busy: true,
                needs_input: true,
                turn_end: true,
                session_end: true,
                last_reply: true,
            },
            presence: CliPresence::Found {
                version: "2.1.0".to_owned(),
            },
            startup_prompts: false,
            launches: Arc::new(Mutex::new(Vec::new())),
            enrich: None,
            launch_error: None,
            before_paste: Vec::new(),
            dialog: None,
        }
    }

    /// Sets what `enrich_hook` does.
    pub fn with_enrich(mut self, f: fn(&str, &mut serde_json::Value)) -> Self {
        self.enrich = Some(f);
        self
    }

    /// Makes `launch` fail with `reason`.
    pub fn with_launch_error(mut self, reason: &str) -> Self {
        self.launch_error = Some(reason.to_owned());
        self
    }
}

impl CliAdapter for StubAdapter {
    fn kind(&self) -> CliKind {
        self.kind
    }
    fn detect(&self) -> CliPresence {
        self.presence.clone()
    }
    fn tested_versions(&self) -> &'static [&'static str] {
        &["2.1"]
    }
    fn coverage(&self) -> HookCoverage {
        self.coverage
    }
    fn launch(&self, spec: &LaunchSpec) -> Result<LaunchCommand, LaunchError> {
        let args = spec
            .task
            .iter()
            .flat_map(|t| ["--task".to_owned(), t.clone()])
            .collect();
        self.launches.lock().unwrap().push(spec.clone());
        if let Some(reason) = &self.launch_error {
            return Err(LaunchError::Invalid(reason.clone()));
        }
        // As `launch_cwd` does: the spec's own `cwd` (a new worktree) first.
        let cwd = spec.cwd.clone().unwrap_or_else(|| {
            spec.agent
                .directory
                .as_deref()
                .map_or_else(|| spec.home.clone(), |d| expand_home(d, &spec.home))
        });
        Ok(LaunchCommand {
            program: self.kind.binary().to_owned(),
            args,
            env: Vec::new(),
            cwd,
            files: Vec::new(),
        })
    }
    fn parse_hook(&self, event: &str, payload: &serde_json::Value) -> Option<HookSignal> {
        match event {
            "session-start" => Some(HookSignal::Ready),
            "busy" => Some(HookSignal::Busy),
            "needs-input" => Some(HookSignal::NeedsInput {
                prompt: payload["prompt"].as_str().map(str::to_owned),
            }),
            "turn-end" => Some(HookSignal::TurnEnded {
                last_reply: payload["last_reply"].as_str().map(str::to_owned),
            }),
            "turn-failed" => Some(HookSignal::TurnFailed {
                reason: payload["reason"]
                    .as_str()
                    .unwrap_or("turn failed")
                    .to_owned(),
            }),
            "session-end" => Some(HookSignal::SessionEnded),
            _ => None,
        }
    }
    /// The last non-blank line names the state (`BUSY`, `ASK`, `REST`, or `TRUST`
    /// for a trust dialog); at rest, the non-blank line above it is the last reply.
    fn read_screen(&self, screen: &str) -> ScreenSignal {
        let mut lines = screen.lines().rev().filter(|l| !l.trim().is_empty());
        let last = lines.next().map(str::trim_end);
        let waiting_on = (last == Some("TRUST")).then_some(WaitingOn::TrustFolder);
        let state = match last {
            Some("BUSY") => Some(ScreenState::Busy),
            Some("ASK" | "TRUST") => Some(ScreenState::NeedsInput),
            Some("REST") => Some(ScreenState::AtRest),
            _ => None,
        };
        let last_reply = match state {
            Some(ScreenState::AtRest) => lines.next().map(|l| l.trim().to_owned()),
            _ => None,
        };
        ScreenSignal {
            state,
            last_reply,
            waiting_on,
        }
    }
    fn quit_keys(&self) -> Vec<Key> {
        vec![Key::Text("/exit".to_owned()), Key::Enter]
    }
    fn submit_keys(&self) -> Vec<Key> {
        vec![Key::Enter]
    }
    fn before_paste_keys(&self) -> Vec<Key> {
        self.before_paste.clone()
    }
    fn dialog_check(&self) -> Option<DialogCheck> {
        self.dialog
    }
    fn startup_prompts(&self) -> bool {
        self.startup_prompts
    }
    fn enrich_hook(&self, event: &str, payload: &mut serde_json::Value) {
        if let Some(f) = self.enrich {
            f(event, payload);
        }
    }
}

/// One call made on a [`FakePaneView`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaneCall {
    /// `open`.
    Open(AgentName, PaneSize),
    /// `resize`.
    Resize(StreamId, PaneSize),
    /// `send`.
    Send(StreamId, PaneInput),
    /// `scrollback`.
    Scrollback(StreamId, u16),
}

#[derive(Debug, Default)]
struct FakePaneState {
    calls: Vec<PaneCall>,
    next_id: u64,
    streams: HashMap<StreamId, tokio::sync::mpsc::Sender<PaneChunk>>,
    scrollback: Vec<u8>,
    not_ready: bool,
}

/// Chunks a [`FakePaneView`] stream holds before a push is dropped.
const FAKE_STREAM_CAPACITY: usize = 256;

/// A [`PaneView`] that records its calls; the test pushes chunks and closes streams.
/// Clones share the same state.
#[derive(Clone, Debug, Default)]
pub struct FakePaneView {
    state: Arc<Mutex<FakePaneState>>,
}

impl FakePaneView {
    /// Calls so far, oldest first.
    pub fn calls(&self) -> Vec<PaneCall> {
        self.state.lock().unwrap().calls.clone()
    }

    /// What later `ready` calls answer (`true` until set).
    pub fn set_ready(&self, ready: bool) {
        self.state.lock().unwrap().not_ready = !ready;
    }

    /// What every later `scrollback` answers.
    pub fn set_scrollback(&self, bytes: Vec<u8>) {
        self.state.lock().unwrap().scrollback = bytes;
    }

    /// Delivers `chunk` on `stream`; dropped when the stream is closed or full.
    pub fn push(&self, stream: StreamId, chunk: PaneChunk) {
        let st = self.state.lock().unwrap();
        if let Some(tx) = st.streams.get(&stream) {
            tx.try_send(chunk).ok();
        }
    }

    /// Whether `stream` is open and its receiver is still held (the viewer hasn't dropped it).
    pub fn receiving(&self, stream: StreamId) -> bool {
        let st = self.state.lock().unwrap();
        st.streams.get(&stream).is_some_and(|tx| !tx.is_closed())
    }

    /// Ends `stream` with a `Closed` chunk; later calls on it fail with `Closed`.
    pub fn close(&self, stream: StreamId, reason: &str) {
        let tx = self.state.lock().unwrap().streams.remove(&stream);
        if let Some(tx) = tx {
            tx.try_send(PaneChunk::Closed {
                reason: reason.to_owned(),
            })
            .ok();
        }
    }

    fn live(&self, stream: StreamId, call: PaneCall) -> Result<(), ViewError> {
        let mut st = self.state.lock().unwrap();
        st.calls.push(call);
        if st.streams.contains_key(&stream) {
            Ok(())
        } else {
            Err(ViewError::Closed)
        }
    }
}

impl PaneView for FakePaneView {
    fn open(&self, agent: &AgentName, size: PaneSize) -> Result<PaneStream, ViewError> {
        let mut st = self.state.lock().unwrap();
        st.calls.push(PaneCall::Open(agent.clone(), size));
        for (_, tx) in st.streams.drain() {
            tx.try_send(PaneChunk::Closed {
                reason: "another stream opened".to_owned(),
            })
            .ok();
        }
        st.next_id += 1;
        let id = StreamId(st.next_id);
        let (tx, rx) = tokio::sync::mpsc::channel(FAKE_STREAM_CAPACITY);
        st.streams.insert(id, tx);
        Ok(PaneStream { id, rx })
    }

    fn resize(&self, stream: StreamId, size: PaneSize) -> Result<(), ViewError> {
        self.live(stream, PaneCall::Resize(stream, size))
    }

    fn send(&self, stream: StreamId, input: PaneInput) -> Result<(), ViewError> {
        self.live(stream, PaneCall::Send(stream, input))
    }

    fn scrollback(
        &self,
        stream: StreamId,
        lines: u16,
    ) -> Result<tokio::sync::oneshot::Receiver<Vec<u8>>, ViewError> {
        self.live(stream, PaneCall::Scrollback(stream, lines))?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        tx.send(self.state.lock().unwrap().scrollback.clone()).ok();
        Ok(rx)
    }

    fn ready(&self) -> bool {
        !self.state.lock().unwrap().not_ready
    }
}

/// A [`Notifier`] that records its alerts.
#[derive(Clone, Debug)]
pub struct FakeNotifier {
    available: bool,
    /// Alerts shown so far, oldest first.
    pub alerts: Arc<Mutex<Vec<Alert>>>,
}

impl FakeNotifier {
    /// A notifier that reports `available`.
    pub fn new(available: bool) -> Self {
        Self {
            available,
            alerts: Arc::default(),
        }
    }
}

impl Notifier for FakeNotifier {
    fn available(&self) -> bool {
        self.available
    }
    fn notify(&mut self, alert: &Alert) {
        self.alerts.lock().unwrap().push(alert.clone());
    }
}

/// One call made on a [`FakeGit`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitCall {
    /// `create`.
    Create(CreateWorktree),
    /// `check`.
    Check(CheckWorktree),
    /// `remove`.
    Remove(RemoveWorktree),
}

/// A [`Git`] that records its calls; the test answers each with a `GitEvent` input.
#[derive(Clone, Debug, Default)]
pub struct FakeGit {
    /// Calls so far, oldest first.
    pub calls: Arc<Mutex<Vec<GitCall>>>,
}

impl Git for FakeGit {
    fn create(&mut self, req: CreateWorktree) {
        self.calls.lock().unwrap().push(GitCall::Create(req));
    }
    fn check(&mut self, req: CheckWorktree) {
        self.calls.lock().unwrap().push(GitCall::Check(req));
    }
    fn remove(&mut self, req: RemoveWorktree) {
        self.calls.lock().unwrap().push(GitCall::Remove(req));
    }
}

/// P3-D27's example `change` objects and what each parses to.
#[expect(clippy::too_many_lines, reason = "one example per change shape")]
pub fn change_examples() -> Vec<(serde_json::Value, crate::command::OrgEdit)> {
    use crate::command::{AgentDraft, MoveTarget, OrgEdit, RoleDraft, TeamDraft};
    use crate::org::{NodeKey, RoleKey};
    use serde_json::json;
    let node = |s: &str| NodeName::parse(s).unwrap();
    let agent = |s: &str| AgentName::parse(s).unwrap();
    let role_key = |team: Option<&str>, role: &str| RoleKey {
        team: team.map(node),
        role: node(role),
    };
    let web_fields = Fields {
        directory: Some("~/code/web".to_owned()),
        ..Fields::default()
    };
    let claude = Fields {
        cli: Some(crate::cli_kind::CliKind::Claude),
        ..Fields::default()
    };
    vec![
        (
            json!({"op": "add_team", "name": "Growth", "fields": {}}),
            OrgEdit::AddTeam(TeamDraft {
                name: node("Growth"),
                defaults: Fields::default(),
            }),
        ),
        (
            json!({"op": "add_role", "team": "Payments", "name": "Frontend",
                   "fields": {"directory": "~/code/web"}, "agents": 1}),
            OrgEdit::AddRole {
                team: Some(node("Payments")),
                role: RoleDraft {
                    name: node("Frontend"),
                    defaults: web_fields,
                },
                agents: 1,
            },
        ),
        (
            json!({"op": "add_agent", "role": {"team": "Payments", "role": "Frontend"},
                   "name": "pay-web", "fields": {"cli": "claude"}}),
            OrgEdit::AddAgent {
                role: role_key(Some("Payments"), "Frontend"),
                agent: AgentDraft {
                    name: agent("pay-web"),
                    fields: claude.clone(),
                },
            },
        ),
        (
            json!({"op": "edit_team", "team": "Growth", "name": "Growth", "fields": {}}),
            OrgEdit::EditTeam {
                key: node("Growth"),
                draft: TeamDraft {
                    name: node("Growth"),
                    defaults: Fields::default(),
                },
            },
        ),
        (
            json!({"op": "edit_role", "role": {"team": null, "role": "Generalist"},
                   "name": "Generalist", "fields": {"worktree": true}}),
            OrgEdit::EditRole {
                key: role_key(None, "Generalist"),
                draft: RoleDraft {
                    name: node("Generalist"),
                    defaults: Fields {
                        worktree: Some(true),
                        ..Fields::default()
                    },
                },
            },
        ),
        (
            json!({"op": "edit_agent", "agent": "pay-web", "name": "pay-web",
                   "fields": {"cli": "claude"}}),
            OrgEdit::EditAgent {
                key: agent("pay-web"),
                draft: AgentDraft {
                    name: agent("pay-web"),
                    fields: claude,
                },
            },
        ),
        (
            json!({"op": "move", "node": {"agent": "pay-qa"},
                   "to": {"role": "QA", "team": "Payments"}}),
            OrgEdit::Move {
                node: NodeKey::Agent(agent("pay-qa")),
                to: MoveTarget::Into(NodeKey::Role(role_key(Some("Payments"), "QA"))),
            },
        ),
        (
            json!({"op": "move", "node": {"role": "QA", "team": "Payments"},
                   "to": {"duo": true}}),
            OrgEdit::Move {
                node: NodeKey::Role(role_key(Some("Payments"), "QA")),
                to: MoveTarget::Into(NodeKey::Duo),
            },
        ),
        (
            json!({"op": "move", "node": {"team": "Growth"},
                   "to": {"before_team": "Payments"}}),
            OrgEdit::Move {
                node: NodeKey::Team(node("Growth")),
                to: MoveTarget::Before(node("Payments")),
            },
        ),
        (
            json!({"op": "move", "node": {"team": "Growth"}, "to": "end"}),
            OrgEdit::Move {
                node: NodeKey::Team(node("Growth")),
                to: MoveTarget::End,
            },
        ),
        (
            json!({"op": "remove", "node": {"role": "QA", "team": "Payments"}}),
            OrgEdit::Delete(NodeKey::Role(role_key(Some("Payments"), "QA"))),
        ),
    ]
}
