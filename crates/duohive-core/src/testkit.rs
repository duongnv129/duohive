//! Test-only fixtures and fakes shared across crates.

use crate::adapter::{
    CliAdapter, CliPresence, HookCoverage, HookSignal, LaunchError, ScreenSignal, ScreenState,
};
use crate::alert::Alert;
use crate::cli_kind::CliKind;
use crate::hook::HookMessage;
use crate::inherit::expand_home;
use crate::launch::{Key, LaunchCommand, LaunchSpec, Token, WindowId};
use crate::names::{AgentName, NodeName};
use crate::org::{AgentDef, Fields, Org, Role, Team};
use crate::ports::{
    CheckWorktree, Clock, CreateWorktree, Git, Ids, Notifier, OrgFileError, PaneView,
    RemoveWorktree, Runtime, SessionRow, Store, StoreError,
};
use crate::settings::Settings;
use crate::time::Millis;
use crate::validate::{DirProbe, DirStatus};
use crate::view::{PaneChunk, PaneInput, PaneSize, PaneStream, StreamId, ViewError};
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

/// Tokens `tok-1`, `tok-2`, and so on.
#[derive(Debug, Default)]
pub struct FakeIds {
    issued: u64,
}

impl Ids for FakeIds {
    fn new_token(&mut self) -> Token {
        self.issued += 1;
        Token(format!("tok-{}", self.issued))
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
    /// `paste`.
    Paste(AgentName, String, Vec<Key>),
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
    fn paste(&mut self, agent: &AgentName, text: String, submit: Vec<Key>) {
        self.record(RuntimeCall::Paste(agent.clone(), text, submit));
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
    /// Queued hooks, oldest first.
    pub pending: Vec<HookMessage>,
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
            pending: Vec::new(),
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
    fn take_pending_hooks(&mut self) -> Result<Vec<HookMessage>, StoreError> {
        Ok(std::mem::take(&mut self.state.lock().unwrap().pending))
    }
    fn queue_pending_hook(&mut self, msg: &HookMessage, _at: Millis) -> Result<(), StoreError> {
        self.state.lock().unwrap().pending.push(msg.clone());
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
        let cwd = spec
            .agent
            .directory
            .as_deref()
            .map_or_else(|| spec.home.clone(), |d| expand_home(d, &spec.home));
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
            "session-end" => Some(HookSignal::SessionEnded),
            _ => None,
        }
    }
    /// The last non-blank line names the state (`BUSY`, `ASK`, `REST`); at rest,
    /// the non-blank line above it is the last reply.
    fn read_screen(&self, screen: &str) -> ScreenSignal {
        let mut lines = screen.lines().rev().filter(|l| !l.trim().is_empty());
        let state = match lines.next().map(str::trim_end) {
            Some("BUSY") => Some(ScreenState::Busy),
            Some("ASK") => Some(ScreenState::NeedsInput),
            Some("REST") => Some(ScreenState::AtRest),
            _ => None,
        };
        let last_reply = match state {
            Some(ScreenState::AtRest) => lines.next().map(|l| l.trim().to_owned()),
            _ => None,
        };
        ScreenSignal { state, last_reply }
    }
    fn quit_keys(&self) -> Vec<Key> {
        vec![Key::Text("/exit".to_owned()), Key::Enter]
    }
    fn submit_keys(&self) -> Vec<Key> {
        vec![Key::Enter]
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
