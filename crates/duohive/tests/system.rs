//! The headless system: start-up, detection, broken files, `bench-agent`,
//! replay enrichment (D22) and shutdown. Each test runs its own tmux server
//! on a throwaway socket, never `duohive`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly on setup errors"
)]

use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as StdCommand, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use duohive::hook_listener::HookListener;
use duohive::system::{EnrichingStore, StartError, SystemHandle, SystemOptions, start_system};
use duohive_cli::fake::{FakeCliAdapter, fake_adapters};
use duohive_core::adapter::{
    Adapters, CliAdapter, CliPresence, HookCoverage, HookSignal, LaunchError, ScreenSignal,
};
use duohive_core::cli_kind::CliKind;
use duohive_core::command::Command;
use duohive_core::event::{Event, Snapshot};
use duohive_core::hook::HookMessage;
use duohive_core::launch::{Key, LaunchCommand, LaunchSpec};
use duohive_core::names::{AgentName, NodeName};
use duohive_core::org::{AgentDef, Fields, Org, Role};
use duohive_core::ports::{NoGit, NoNotifier, Store};
use duohive_core::status::AgentState;
use duohive_core::testkit::{StubAdapter, example_org, n};
use duohive_core::time::Millis;
use duohive_store::db::{insert_pending_hook, open_db_for_hook};
use duohive_store::{Paths, SqliteStore, save_org};
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

const BIN: &str = env!("CARGO_BIN_EXE_duohive");

fn random_hex() -> String {
    let mut h = RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    );
    h.write_u32(std::process::id());
    format!("{:016x}", h.finish())
}

/// Temp config and data dirs plus a throwaway tmux socket, killed on drop.
struct Env {
    tmp: tempfile::TempDir,
    paths: Paths,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config_dir: tmp.path().join("config"),
            data_dir: tmp.path().join("data"),
            tmux_socket: format!("duohive-test-{}", random_hex()),
        };
        std::fs::create_dir_all(&paths.config_dir).unwrap();
        Self { tmp, paths }
    }

    fn tmux(&self, args: &[&str]) -> String {
        let out = StdCommand::new("tmux")
            .args(["-f", "/dev/null", "-L", &self.paths.tmux_socket])
            .args(args)
            .env_remove("TMUX")
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    }

    fn write_org(&self, org: &Org) {
        save_org(&self.paths, org).unwrap();
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = self.tmux(&["kill-server"]);
    }
}

fn fakes() -> Arc<Adapters> {
    Arc::new(fake_adapters(Path::new(BIN)))
}

fn opts() -> SystemOptions {
    SystemOptions {
        tap: None,
        first_frame: None,
        home: std::env::temp_dir(),
        notifier: Some(Box::new(NoNotifier)),
        git: Some(Box::new(NoGit)),
        git_env: Vec::new(),
    }
}

/// Duo plus one root role `Bench` holding `agent`, working in `dir`.
fn bench_org(agent: &str, dir: &str) -> Org {
    let mut org = Org::blank(CliKind::Claude);
    org.roles = vec![Role {
        name: NodeName::parse("Bench").unwrap(),
        defaults: Fields {
            directory: Some(dir.to_owned()),
            ..Fields::default()
        },
        agents: vec![AgentDef {
            name: n(agent),
            fields: Fields::default(),
        }],
    }];
    org
}

/// The next event matching `pred` within `within`, else `None`.
async fn wait_for(
    h: &mut SystemHandle,
    within: Duration,
    mut pred: impl FnMut(&Event) -> bool,
) -> Option<Event> {
    let deadline = Instant::now() + within;
    loop {
        let left = deadline.checked_duration_since(Instant::now())?;
        match tokio::time::timeout(left, h.events.recv()).await {
            Ok(Some(ev)) if pred(&ev) => return Some(ev),
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => return None,
        }
    }
}

async fn first_snapshot(h: &mut SystemHandle) -> Snapshot {
    match wait_for(h, Duration::from_secs(5), |e| {
        matches!(e, Event::Snapshot(_))
    })
    .await
    {
        Some(Event::Snapshot(s)) => *s,
        other => panic!("no snapshot: {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn start_system_refuses_a_second_instance() {
    let env = Env::new();
    let mut first = start_system(&env.paths, fakes(), opts()).await.unwrap();
    let Err(err) = start_system(&env.paths, fakes(), opts()).await else {
        panic!("a second instance started");
    };
    assert!(matches!(err, StartError::AlreadyRunning), "{err:?}");
    assert_eq!(err.to_string(), "DuoHive is already running");
    first
        .commands
        .send(Command::StartAgent { agent: n("nobody") })
        .await
        .unwrap();
    let rejected = wait_for(
        &mut first,
        Duration::from_secs(5),
        |e| matches!(e, Event::CommandRejected { reason } if reason.contains("nobody")),
    )
    .await;
    assert!(rejected.is_some(), "the first instance stopped answering");
    first.shutdown().await;
}

/// The fake adapter, counting `detect()` calls.
struct Counting {
    inner: FakeCliAdapter,
    calls: Arc<AtomicUsize>,
}

impl CliAdapter for Counting {
    fn kind(&self) -> CliKind {
        self.inner.kind()
    }
    fn detect(&self) -> CliPresence {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.detect()
    }
    fn tested_versions(&self) -> &'static [&'static str] {
        self.inner.tested_versions()
    }
    fn coverage(&self) -> HookCoverage {
        self.inner.coverage()
    }
    fn launch(&self, spec: &LaunchSpec) -> Result<LaunchCommand, LaunchError> {
        self.inner.launch(spec)
    }
    fn parse_hook(&self, event: &str, payload: &serde_json::Value) -> Option<HookSignal> {
        self.inner.parse_hook(event, payload)
    }
    fn read_screen(&self, screen: &str) -> ScreenSignal {
        self.inner.read_screen(screen)
    }
    fn quit_keys(&self) -> Vec<Key> {
        self.inner.quit_keys()
    }
    fn submit_keys(&self) -> Vec<Key> {
        self.inner.submit_keys()
    }
    fn startup_prompts(&self) -> bool {
        self.inner.startup_prompts()
    }
}

fn counting(calls: &Arc<AtomicUsize>) -> Arc<Adapters> {
    let wrap = |kind, hooks| -> Box<dyn CliAdapter> {
        Box::new(Counting {
            inner: FakeCliAdapter {
                kind,
                exe: PathBuf::from(BIN),
                hooks,
            },
            calls: Arc::clone(calls),
        })
    };
    Arc::new(Adapters::new(vec![
        wrap(CliKind::Claude, true),
        wrap(CliKind::Opencode, false),
    ]))
}

fn has_dir_problem(ev: &Event) -> bool {
    match ev {
        Event::Snapshot(s) => s.agents.values().any(|a| a.problem.is_some()),
        Event::OrgChanged { problems, .. } => !problems.is_empty(),
        _ => false,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn detection_waits_for_the_first_frame() {
    let env = Env::new();
    let missing = env.tmp.path().join("nowhere").display().to_string();
    env.write_org(&bench_org("far-1", &missing));
    let calls = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = oneshot::channel();
    let mut h = start_system(
        &env.paths,
        counting(&calls),
        SystemOptions {
            first_frame: Some(rx),
            ..opts()
        },
    )
    .await
    .unwrap();
    let early = wait_for(&mut h, Duration::from_millis(500), has_dir_problem).await;
    assert_eq!(early, None, "a directory was probed before the first frame");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    tx.send(()).unwrap();
    let probed = wait_for(
        &mut h,
        Duration::from_secs(2),
        |e| matches!(e, Event::OrgChanged { problems, .. } if !problems.is_empty()),
    )
    .await;
    let Some(Event::OrgChanged { problems, .. }) = probed else {
        panic!("no OrgChanged with the probed problems");
    };
    assert_eq!(
        problems.get(&n("far-1")).map(String::as_str),
        Some(format!("✕ {missing} not found · e to fix").as_str())
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2, "detected twice");
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn start_system_with_broken_files_uses_last_good_and_defaults() {
    let env = Env::new();
    env.write_org(&example_org());
    std::fs::write(env.paths.org_toml(), "cli = \"claud\"\n").unwrap();
    std::fs::write(env.paths.settings_toml(), "quiet_after_minutes = \"x\"\n").unwrap();
    let mut h = start_system(&env.paths, fakes(), opts()).await.unwrap();
    let snap = first_snapshot(&mut h).await;
    assert_eq!(snap.org, example_org());
    assert!(snap.org_error.as_ref().unwrap().line.is_some(), "{snap:?}");
    assert!(snap.settings_error.is_some());
    h.shutdown().await;

    let fresh = Env::new();
    std::fs::write(fresh.paths.org_toml(), "cli = \"claud\"\n").unwrap();
    let mut h = start_system(&fresh.paths, fakes(), opts()).await.unwrap();
    let snap = first_snapshot(&mut h).await;
    assert_eq!(snap.org, Org::blank(CliKind::Claude));
    assert!(snap.org_error.is_some());
    assert!(snap.settings_error.is_none());
    h.shutdown().await;
}

/// A hook listener on `<data>/run.sock` whose hooks arrive on the returned tap.
fn tap_listener(
    data: &Path,
) -> (
    mpsc::UnboundedReceiver<HookMessage>,
    mpsc::Receiver<duohive_core::engine::Input>,
) {
    let listener = HookListener::bind(&data.join("run.sock")).unwrap();
    let (to_core, core) = mpsc::channel(64);
    let (tap_tx, tap) = mpsc::unbounded_channel();
    tokio::spawn(listener.serve(
        Arc::new(RwLock::new(HashMap::new())),
        Arc::new(Adapters::new(vec![])),
        to_core,
        Some(tap_tx),
    ));
    (tap, core)
}

/// A child process killed and reaped on drop, so a failing test leaves none behind.
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// `duohive bench-agent --task <task>` with stdin open and silent.
fn bench_agent(tmp: &Path, task: &str) -> KillOnDrop {
    let child = StdCommand::new(BIN)
        .args(["bench-agent", "--task", task])
        .env_clear()
        .env("HOME", tmp)
        .env("DUOHIVE_TOKEN", "tok-bench")
        .env("DUOHIVE_AGENT", "bench-1")
        .env("DUOHIVE_DATA_DIR", tmp.join("data"))
        .env("DUOHIVE_CONFIG_DIR", tmp.join("config"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    KillOnDrop(child)
}

async fn next_hook(tap: &mut mpsc::UnboundedReceiver<HookMessage>) -> HookMessage {
    tokio::time::timeout(Duration::from_secs(10), tap.recv())
        .await
        .expect("no hook within 10 s")
        .expect("tap closed")
}

fn sent_at(m: &HookMessage) -> u64 {
    m.payload["sent_at_ms"].as_u64().expect("sent_at_ms")
}

/// The time from `busy` to `turn-end` of the first turn, by the agent's own clock.
async fn first_turn_ms(task: &str) -> u64 {
    let tmp = tempfile::tempdir().unwrap();
    let (mut tap, _core) = tap_listener(&tmp.path().join("data"));
    let _child = bench_agent(tmp.path(), task);
    let events: Vec<HookMessage> = vec![
        next_hook(&mut tap).await,
        next_hook(&mut tap).await,
        next_hook(&mut tap).await,
    ];
    let names: Vec<&str> = events.iter().map(|m| m.event.as_str()).collect();
    assert_eq!(names, ["session-start", "busy", "turn-end"]);
    sent_at(&events[2]) - sent_at(&events[1])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bench_agent_slow_turn_takes_three_seconds() {
    let slow = first_turn_ms("a slow task").await;
    assert!(slow >= 3_000, "slow turn took {slow} ms");
    let quick = first_turn_ms("a task").await;
    assert!(quick < 2_000, "turn took {quick} ms");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bench_agent_runs_its_task_first() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut tap, _core) = tap_listener(&tmp.path().join("data"));
    let mut child = bench_agent(tmp.path(), "a task");
    let start = next_hook(&mut tap).await;
    assert_eq!(start.event, "session-start");
    assert_eq!(start.agent, "bench-1");
    assert_eq!(start.token, "tok-bench");
    assert!(start.payload["sent_at_ms"].is_u64());
    assert_eq!(next_hook(&mut tap).await.event, "busy");
    let end = next_hook(&mut tap).await;
    assert_eq!(end.event, "turn-end");
    assert!(
        end.payload["last_reply"]
            .as_str()
            .is_some_and(|r| !r.is_empty())
    );

    let mut stdin = child.0.stdin.take().unwrap();
    stdin.write_all(b"next\n").unwrap();
    assert_eq!(next_hook(&mut tap).await.event, "busy");
    assert_eq!(next_hook(&mut tap).await.event, "turn-end");
    // A stop sends Escape before the quit keys; the ESC byte is not a turn.
    stdin.write_all(b"\x1b/exit\n").unwrap();
    assert_eq!(next_hook(&mut tap).await.event, "session-end");
    let status = tokio::task::spawn_blocking(move || {
        let mut child = child;
        child.0.wait()
    })
    .await
    .unwrap()
    .unwrap();
    assert!(status.success(), "{status:?}");
}

fn add_reply(_event: &str, payload: &mut serde_json::Value) {
    payload["duohive_last_reply"] = json!("from transcript");
}

#[test]
fn replayed_hooks_are_enriched() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths {
        config_dir: tmp.path().join("config"),
        data_dir: tmp.path().join("data"),
        tmux_socket: "duohive-test-unused".to_owned(),
    };
    let store = SqliteStore::open(&paths).unwrap();
    let conn = open_db_for_hook(&paths.state_db(), Duration::from_millis(200)).unwrap();
    let msg = HookMessage {
        sent_at_ms: 0,
        token: "tok".to_owned(),
        agent: "pay-qa".to_owned(),
        event: "turn-end".to_owned(),
        payload: json!({"session_id": "s"}),
    };
    insert_pending_hook(&conn, &msg, Millis(1)).unwrap();
    let adapters = Adapters::new(vec![Box::new(
        StubAdapter::new(CliKind::Claude).with_enrich(add_reply),
    )]);
    let agents = Arc::new(RwLock::new(HashMap::from([(
        "pay-qa".to_owned(),
        CliKind::Claude,
    )])));
    let mut store = EnrichingStore::new(store, Arc::new(adapters), agents);
    let hooks = store.take_pending_hooks().unwrap();
    assert_eq!(hooks.len(), 1);
    assert_eq!(hooks[0].payload["duohive_last_reply"], "from transcript");
    assert_eq!(hooks[0].payload["session_id"], "s");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_leaves_agents_running() {
    let env = Env::new();
    let dir = env.tmp.path().join("work");
    std::fs::create_dir_all(&dir).unwrap();
    env.write_org(&bench_org("bench-1", &dir.display().to_string()));
    let mut h = start_system(&env.paths, fakes(), opts()).await.unwrap();
    let agent: AgentName = n("bench-1");
    h.commands
        .send(Command::StartAgent {
            agent: agent.clone(),
        })
        .await
        .unwrap();
    let idle = wait_for(&mut h, Duration::from_secs(20), |e| {
        matches!(e, Event::AgentStatusChanged { agent: a, status } if *a == agent && status.state == AgentState::Idle)
    })
    .await;
    assert!(idle.is_some(), "the fake agent never went idle");
    h.shutdown().await;
    let windows = env.tmux(&["list-windows", "-a", "-F", "#{window_name}"]);
    assert!(windows.lines().any(|w| w == "bench-1"), "{windows:?}");
    // run.sock is gone, so a hook fired now cannot connect and queues in pending_hooks.
    let sock = env.paths.run_sock();
    assert!(!sock.exists());
    let refused = UnixStream::connect(&sock);
    assert!(refused.is_err(), "run.sock still answers after shutdown");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bench_agent_task_is_handled_like_a_line() {
    // `--task` is trimmed and `/exit` quits, exactly as on stdin.
    let tmp = tempfile::tempdir().unwrap();
    let (mut tap, _core) = tap_listener(&tmp.path().join("data"));
    let child = bench_agent(tmp.path(), "  /exit  ");
    assert_eq!(next_hook(&mut tap).await.event, "session-start");
    assert_eq!(next_hook(&mut tap).await.event, "session-end");
    let status = tokio::task::spawn_blocking(move || {
        let mut child = child;
        child.0.wait()
    })
    .await
    .unwrap()
    .unwrap();
    assert!(status.success(), "{status:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_run_detects_exactly_once() {
    let env = Env::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut h = start_system(&env.paths, counting(&calls), opts())
        .await
        .unwrap();
    assert_eq!(
        first_snapshot(&mut h).await.org,
        Org::blank(CliKind::Claude)
    );
    // The detection task still probes the directories: its OrgChanged ends it.
    let probed = wait_for(&mut h, Duration::from_secs(5), |e| {
        matches!(e, Event::OrgChanged { .. })
    })
    .await;
    assert!(probed.is_some(), "the directories were never probed");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2, "one detect per CLI kind");
    h.shutdown().await;
}

/// The launch token the engine gave `agent`, from its `launch.sh`.
fn launch_token(paths: &Paths, agent: &str) -> String {
    let script =
        std::fs::read_to_string(paths.data_dir.join("run").join(agent).join("launch.sh")).unwrap();
    let line = script
        .lines()
        .find_map(|l| l.strip_prefix("export DUOHIVE_TOKEN="))
        .expect("a token in launch.sh");
    line.trim_matches('\'').to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_hook_lost_at_shutdown() {
    // Review Focus 3: a hook delivered just before shutdown is applied (the
    // row says working) or queued for the next start, never dropped.
    let env = Env::new();
    let dir = env.tmp.path().join("work");
    std::fs::create_dir_all(&dir).unwrap();
    env.write_org(&bench_org("bench-1", &dir.display().to_string()));
    let mut h = start_system(&env.paths, fakes(), opts()).await.unwrap();
    let agent = n("bench-1");
    h.commands
        .send(Command::StartAgent {
            agent: agent.clone(),
        })
        .await
        .unwrap();
    let idle = wait_for(&mut h, Duration::from_secs(20), |e| {
        matches!(e, Event::AgentStatusChanged { agent: a, status } if *a == agent && status.state == AgentState::Idle)
    })
    .await;
    assert!(idle.is_some(), "the fake agent never went idle");
    let hook = HookMessage {
        sent_at_ms: 0,
        token: launch_token(&env.paths, "bench-1"),
        agent: "bench-1".to_owned(),
        event: "busy".to_owned(),
        payload: json!({"sent_at_ms": 1}),
    };
    let mut line = serde_json::to_vec(&hook).unwrap();
    line.push(b'\n');
    let mut stream = UnixStream::connect(env.paths.run_sock()).unwrap();
    stream.write_all(&line).unwrap();
    drop(stream);
    h.shutdown().await;

    let mut store = SqliteStore::open(&env.paths).unwrap();
    let applied = store
        .load_sessions()
        .unwrap()
        .iter()
        .any(|r| r.agent == agent && r.status.state == AgentState::Working);
    let queued = store.take_pending_hooks().unwrap().contains(&hook);
    assert!(applied || queued, "the hook was lost");
}
