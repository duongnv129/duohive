//! End to end: the headless system on a real tmux server (a throwaway socket,
//! never `duohive`), temp dirs and the fake CLIs (`claude` with hooks,
//! `opencode` without), with events read from the `SystemHandle`. Plus the
//! command line and logging.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly on setup errors"
)]

use std::collections::BTreeMap;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::sync::Arc;
use std::time::{Duration, Instant};

use duohive::args::{Cli, USAGE, parse_args};
use duohive::bench::BenchArgs;
use duohive::logging::init_logging;
use duohive::system::{SystemHandle, SystemOptions, start_system};
use duohive_cli::fake::fake_adapters;
use duohive_core::adapter::Adapters;
use duohive_core::cli_kind::CliKind;
use duohive_core::command::{Command, OrgEdit};
use duohive_core::event::{Event, Snapshot};
use duohive_core::names::{AgentName, NodeName};
use duohive_core::org::{AgentDef, Fields, Org, Role};
use duohive_core::ports::{NoGit, NoNotifier};
use duohive_core::status::{AgentState, Status};
use duohive_core::testkit::n;
use duohive_core::worktree::RunIn;
use duohive_store::{Paths, save_org};

const BIN: &str = env!("CARGO_BIN_EXE_duohive");
/// Long enough for a fake agent to start on a loaded machine.
const START: Duration = Duration::from_secs(20);

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

/// Temp config, data and home dirs plus a throwaway tmux socket, killed on drop.
struct Env {
    _tmp: tempfile::TempDir,
    paths: Paths,
    home: PathBuf,
    work: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config_dir: tmp.path().join("config"),
            data_dir: tmp.path().join("data"),
            tmux_socket: format!("duohive-test-{}", random_hex()),
        };
        let home = tmp.path().join("home");
        let work = tmp.path().join("work");
        for dir in [&paths.config_dir, &home, &work] {
            std::fs::create_dir_all(dir).unwrap();
        }
        Self {
            _tmp: tmp,
            paths,
            home,
            work,
        }
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

    /// The ids of the windows named `agent`.
    fn windows_of(&self, agent: &str) -> Vec<String> {
        self.tmux(&["list-windows", "-a", "-F", "#{window_name}\t#{window_id}"])
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .filter(|(name, _)| *name == agent)
            .map(|(_, id)| id.to_owned())
            .collect()
    }

    /// The one window named `agent`.
    fn window_of(&self, agent: &str) -> String {
        let ids = self.windows_of(agent);
        assert_eq!(ids.len(), 1, "windows of {agent}: {ids:?}");
        ids[0].clone()
    }

    /// The pid of the process in `window`'s pane (`sh launch.sh` execs the CLI).
    fn pane_pid(&self, window: &str) -> String {
        self.tmux(&["list-panes", "-t", window, "-F", "#{pane_pid}"])
            .trim()
            .to_owned()
    }

    fn opts(&self) -> SystemOptions {
        SystemOptions {
            tap: None,
            first_frame: None,
            home: self.home.clone(),
            notifier: Some(Box::new(NoNotifier)),
            git: Some(Box::new(NoGit)),
        }
    }

    async fn start(&self) -> SystemHandle {
        start_system(&self.paths, fakes(), self.opts())
            .await
            .unwrap()
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

/// Duo on `duo_cli` plus one root role `E2e` holding `agents` on `cli`, working in `dir`.
fn org(agents: &[&str], cli: CliKind, duo_cli: CliKind, dir: &Path) -> Org {
    let mut org = Org::blank(duo_cli);
    org.roles = vec![Role {
        name: NodeName::parse("E2e").unwrap(),
        defaults: Fields {
            cli: Some(cli),
            directory: Some(dir.display().to_string()),
            ..Fields::default()
        },
        agents: agents
            .iter()
            .map(|a| AgentDef {
                name: n(a),
                fields: Fields::default(),
            })
            .collect(),
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

/// Every event until one matches `pred` (included); panics after `within`.
async fn events_until(
    h: &mut SystemHandle,
    within: Duration,
    what: &str,
    mut pred: impl FnMut(&Event) -> bool,
) -> Vec<Event> {
    let deadline = Instant::now() + within;
    let mut seen = Vec::new();
    loop {
        let left = deadline
            .checked_duration_since(Instant::now())
            .unwrap_or_else(|| panic!("no {what} within {within:?}: {seen:#?}"));
        match tokio::time::timeout(left, h.events.recv()).await {
            Ok(Some(ev)) => {
                let done = pred(&ev);
                seen.push(ev);
                if done {
                    return seen;
                }
            }
            Ok(None) => panic!("the core stopped before {what}: {seen:#?}"),
            Err(_) => panic!("no {what} within {within:?}: {seen:#?}"),
        }
    }
}

fn status_of<'e>(ev: &'e Event, agent: &AgentName) -> Option<&'e Status> {
    match ev {
        Event::AgentStatusChanged { agent: a, status } if a == agent => Some(status),
        _ => None,
    }
}

fn is_state(ev: &Event, agent: &AgentName, state: AgentState) -> bool {
    status_of(ev, agent).is_some_and(|s| s.state == state)
}

/// The states `agent` moved through in `events`, in order.
fn moves(events: &[Event], agent: &AgentName) -> Vec<AgentState> {
    events
        .iter()
        .filter_map(|e| status_of(e, agent).map(|s| s.state))
        .collect()
}

async fn send(h: &SystemHandle, c: Command) {
    h.commands.send(c).await.unwrap();
}

async fn start_agent(h: &mut SystemHandle, agent: &AgentName) -> Vec<Event> {
    send(
        h,
        Command::StartAgent {
            agent: agent.clone(),
        },
    )
    .await;
    events_until(h, START, "idle", |e| is_state(e, agent, AgentState::Idle)).await
}

async fn first_snapshot(h: &mut SystemHandle) -> Snapshot {
    match wait_for(h, Duration::from_secs(10), |e| {
        matches!(e, Event::Snapshot(_))
    })
    .await
    {
        Some(Event::Snapshot(s)) => *s,
        other => panic!("no snapshot: {other:?}"),
    }
}

/// Waits until `cond` holds, polling every 100 ms; panics after `within`.
async fn eventually(within: Duration, what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + within;
    while !cond() {
        assert!(Instant::now() < deadline, "{what} not within {within:?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_start_work_finish_view() {
    let env = Env::new();
    save_org(
        &env.paths,
        &org(&["e2e-1"], CliKind::Claude, CliKind::Claude, &env.work),
    )
    .unwrap();
    let mut h = env.start().await;
    let a = n("e2e-1");
    let started = start_agent(&mut h, &a).await;
    assert_eq!(
        moves(&started, &a),
        [AgentState::Starting, AgentState::Idle]
    );

    send(
        &h,
        Command::GiveWork {
            agent: a.clone(),
            task: "say hi".into(),
            run: RunIn::Directory,
        },
    )
    .await;
    let turn = events_until(&mut h, START, "finished", |e| {
        is_state(e, &a, AgentState::Finished)
    })
    .await;
    // P2-D11: the paste leaves the agent idle; its busy hook moves it to working.
    assert_eq!(
        moves(&turn, &a),
        [AgentState::Working, AgentState::Finished]
    );
    let run = |e: &Event| match e {
        Event::AgentRunChanged {
            agent,
            delivery_pending,
            ..
        } if *agent == a => Some(*delivery_pending),
        _ => None,
    };
    assert_eq!(
        turn.iter().filter_map(run).collect::<Vec<_>>(),
        [true, false],
        "{turn:#?}"
    );
    let reply = |e: &Event| match e {
        Event::LastReplyChanged { agent, reply } if *agent == a => reply.clone(),
        _ => None,
    };
    // The reply may be reported just before or just after the state.
    let reply = match turn.iter().rev().find_map(reply) {
        Some(r) => Some(r),
        None => wait_for(&mut h, Duration::from_secs(5), |e| reply(e).is_some())
            .await
            .as_ref()
            .and_then(reply),
    };
    assert_eq!(reply.as_deref(), Some("done: say hi"));

    send(&h, Command::ViewAgent { agent: a.clone() }).await;
    events_until(&mut h, Duration::from_secs(5), "idle after view", |e| {
        is_state(e, &a, AgentState::Idle)
    })
    .await;
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_stop_is_stopped() {
    let env = Env::new();
    save_org(
        &env.paths,
        &org(&["e2e-1"], CliKind::Claude, CliKind::Claude, &env.work),
    )
    .unwrap();
    let mut h = env.start().await;
    let a = n("e2e-1");
    start_agent(&mut h, &a).await;
    assert_eq!(env.windows_of("e2e-1").len(), 1);
    send(&h, Command::StopAgent { agent: a.clone() }).await;
    let stopped = events_until(&mut h, START, "stopped", |e| {
        is_state(e, &a, AgentState::Stopped)
    })
    .await;
    assert!(
        !moves(&stopped, &a).contains(&AgentState::Errored),
        "{stopped:#?}"
    );
    eventually(Duration::from_secs(10), "the window gone", || {
        env.windows_of("e2e-1").is_empty()
    })
    .await;
    h.shutdown().await;
}

/// SIGKILLs the process in `agent`'s only window; returns that window.
fn crash(env: &Env, agent: &str) -> String {
    let window = env.window_of(agent);
    let pid = env.pane_pid(&window);
    let killed = StdCommand::new("kill")
        .args(["-KILL", &pid])
        .status()
        .unwrap();
    assert!(killed.success(), "kill -KILL {pid}");
    window
}

fn errored_unexpectedly(ev: &Event, agent: &AgentName) -> bool {
    status_of(ev, agent).is_some_and(|s| {
        s.state == AgentState::Errored
            && s.error
                .as_deref()
                .is_some_and(|e| e.starts_with("exited unexpectedly"))
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_crash_is_errored_in_needs_you() {
    let env = Env::new();
    save_org(
        &env.paths,
        &org(&["e2e-1"], CliKind::Claude, CliKind::Claude, &env.work),
    )
    .unwrap();
    let mut h = env.start().await;
    let a = n("e2e-1");
    start_agent(&mut h, &a).await;
    crash(&env, "e2e-1");
    let seen = events_until(&mut h, START, "needs you with the errored agent", |e| {
        matches!(e, Event::NeedsYouChanged(items)
            if items.iter().any(|i| i.agent == a && i.state == AgentState::Errored))
    })
    .await;
    assert!(
        seen.iter().any(|e| errored_unexpectedly(e, &a)),
        "{seen:#?}"
    );
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_crash_then_immediate_restart() {
    let env = Env::new();
    save_org(
        &env.paths,
        &org(&["e2e-1"], CliKind::Claude, CliKind::Claude, &env.work),
    )
    .unwrap();
    let mut h = env.start().await;
    let a = n("e2e-1");
    start_agent(&mut h, &a).await;
    let old = crash(&env, "e2e-1");
    events_until(&mut h, START, "errored", |e| errored_unexpectedly(e, &a)).await;
    // At once: before the old window's close arrives.
    send(
        &h,
        Command::GiveWork {
            agent: a.clone(),
            task: "again".into(),
            run: RunIn::Directory,
        },
    )
    .await;
    let mut after = events_until(&mut h, START, "finished", |e| {
        is_state(e, &a, AgentState::Finished)
    })
    .await;
    assert_eq!(
        moves(&after, &a),
        [
            AgentState::Starting,
            AgentState::Working,
            AgentState::Finished
        ]
    );
    // The old window's close, however late, errors nothing.
    while let Some(ev) = wait_for(&mut h, Duration::from_secs(2), |_| true).await {
        after.push(ev);
    }
    assert!(
        !moves(&after, &a).contains(&AgentState::Errored),
        "{after:#?}"
    );
    let new = env.window_of("e2e-1");
    assert_ne!(new, old);
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_duo_cli_change_restarts_duo() {
    let env = Env::new();
    let start_org = org(&[], CliKind::Claude, CliKind::Claude, &env.work);
    save_org(&env.paths, &start_org).unwrap();
    let mut h = env.start().await;
    let duo = start_org.duo.name.clone();
    let mut seen = start_agent(&mut h, &duo).await;
    let first = env.window_of(duo.as_str());

    let mut duo_cfg = start_org.duo.clone();
    duo_cfg.cli = CliKind::Opencode;
    send(
        &h,
        Command::EditOrg(OrgEdit::EditOrgDefaults {
            defaults: start_org.defaults.clone(),
            duo: duo_cfg,
        }),
    )
    .await;
    let restarted = events_until(&mut h, START, "duo idle again", |e| {
        is_state(e, &duo, AgentState::Idle)
    })
    .await;
    assert_eq!(
        moves(&restarted, &duo),
        [AgentState::Stopped, AgentState::Starting, AgentState::Idle]
    );
    seen.extend(restarted);
    assert!(
        !moves(&seen, &duo).contains(&AgentState::Errored),
        "{seen:#?}"
    );
    let second = env.window_of(duo.as_str());
    assert_ne!(second, first);
    let all = env.tmux(&["list-windows", "-a", "-F", "#{window_id}"]);
    assert!(
        !all.lines().any(|w| w == first),
        "{first} still open: {all}"
    );
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_hookless_agent_gets_tilde_states() {
    let env = Env::new();
    save_org(
        &env.paths,
        &org(&["e2e-1"], CliKind::Opencode, CliKind::Claude, &env.work),
    )
    .unwrap();
    let mut h = env.start().await;
    let a = n("e2e-1");
    let started = start_agent(&mut h, &a).await;
    let idle = started.iter().rev().find_map(|e| status_of(e, &a)).unwrap();
    assert!(idle.from_screen, "{idle:?}");

    // P2-D11: the paste leaves it idle until a `~` busy read, taken on the turn's
    // first activity; a slow (3 s) turn is still running when that read lands.
    send(
        &h,
        Command::GiveWork {
            agent: a.clone(),
            task: "say hi slowly".into(),
            run: RunIn::Directory,
        },
    )
    .await;
    let taken = events_until(&mut h, START, "working", |e| {
        is_state(e, &a, AgentState::Working)
    })
    .await;
    let working = taken.iter().rev().find_map(|e| status_of(e, &a)).unwrap();
    assert!(working.from_screen, "{working:?}");
    let working_at = Instant::now();
    let turn = events_until(&mut h, START, "finished", |e| {
        is_state(e, &a, AgentState::Finished)
    })
    .await;
    // The rest of the 3 s turn, the 1.5 s settle, then at most 2 s to read the screen.
    let took = working_at.elapsed();
    assert!(
        took < Duration::from_millis(6500),
        "finished after {took:?}"
    );
    let finished = turn.iter().rev().find_map(|e| status_of(e, &a)).unwrap();
    assert!(finished.from_screen, "{finished:?}");
    h.shutdown().await;
}

/// Each agent's latest state and last reply, from a snapshot and the events after it.
#[derive(Debug, Default)]
struct Board {
    states: BTreeMap<AgentName, AgentState>,
    replies: BTreeMap<AgentName, Option<String>>,
}

impl Board {
    fn apply(&mut self, ev: &Event) {
        match ev {
            Event::Snapshot(s) => {
                for (a, v) in &s.agents {
                    self.states.insert(a.clone(), v.status.state);
                    self.replies.insert(a.clone(), v.last_reply.clone());
                }
            }
            Event::AgentStatusChanged { agent, status } => {
                self.states.insert(agent.clone(), status.state);
            }
            Event::LastReplyChanged { agent, reply } => {
                self.replies.insert(agent.clone(), reply.clone());
            }
            _ => {}
        }
    }
}

/// `agent`'s queued hook events in replay order, read without taking them.
fn pending_events(paths: &Paths, agent: &str) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        paths.state_db(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut rows = db
        .prepare("SELECT event FROM pending_hooks WHERE agent = ?1 ORDER BY sent_at_ms, id")
        .unwrap();
    rows.query_map([agent], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_restart_reconnects() {
    let env = Env::new();
    save_org(
        &env.paths,
        &org(
            &["e2e-a", "e2e-b"],
            CliKind::Claude,
            CliKind::Claude,
            &env.work,
        ),
    )
    .unwrap();
    let mut h = env.start().await;
    let (a, b) = (n("e2e-a"), n("e2e-b"));
    start_agent(&mut h, &a).await;
    start_agent(&mut h, &b).await;
    let windows = (env.window_of("e2e-a"), env.window_of("e2e-b"));
    send(
        &h,
        Command::GiveWork {
            agent: a.clone(),
            task: "slow task".into(),
            run: RunIn::Directory,
        },
    )
    .await;
    events_until(&mut h, START, "working", |e| {
        is_state(e, &a, AgentState::Working)
    })
    .await;
    // The task has reached the agent: P2-D11 reports Working only on its busy hook.
    let (window_a, _) = &windows;
    eventually(Duration::from_secs(10), "the slow turn under way", || {
        env.tmux(&["capture-pane", "-p", "-t", window_a])
            .contains("working…")
    })
    .await;
    h.shutdown().await;

    // The turn ends while DuoHive is closed: its hooks queue in pending_hooks.
    eventually(Duration::from_secs(10), "the slow turn's reply", || {
        env.tmux(&["capture-pane", "-p", "-t", window_a])
            .contains("done: slow task")
    })
    .await;
    // Its hooks fell back to state.db while DuoHive was closed: the replay path.
    eventually(
        Duration::from_secs(10),
        "the turn-end in pending_hooks",
        || {
            pending_events(&env.paths, "e2e-a")
                .last()
                .map(String::as_str)
                == Some("turn-end")
        },
    )
    .await;
    assert_eq!(pending_events(&env.paths, "e2e-b"), Vec::<String>::new());

    let mut h = env.start().await;
    let snap = first_snapshot(&mut h).await;
    assert_eq!(snap.reconnecting, 2, "{snap:?}");
    let mut board = Board::default();
    board.apply(&Event::Snapshot(Box::new(snap)));
    let seen = events_until(&mut h, START, "reconnected", |e| {
        matches!(e, Event::Reconnected)
    })
    .await;
    for ev in &seen {
        board.apply(ev);
    }
    assert_eq!(
        board.states.get(&a),
        Some(&AgentState::Finished),
        "{seen:#?}"
    );
    assert_eq!(
        board.replies.get(&a).cloned().flatten().as_deref(),
        Some("done: slow task")
    );
    assert_eq!(board.states.get(&b), Some(&AgentState::Idle), "{seen:#?}");
    assert_eq!((env.window_of("e2e-a"), env.window_of("e2e-b")), windows);
    h.shutdown().await;
}

fn s<const N: usize>(items: [&str; N]) -> Vec<String> {
    items.iter().map(|i| (*i).to_owned()).collect()
}

#[test]
fn args_parse() {
    assert_eq!(
        parse_args(&s(["hook", "a", "busy"])),
        Ok(Cli::Hook {
            agent: "a".into(),
            event: "busy".into()
        })
    );
    assert_eq!(
        parse_args(&s(["bench", "--agents", "10", "--json"])),
        Ok(Cli::Bench(BenchArgs {
            agents: 10,
            json: true,
            ..BenchArgs::default()
        }))
    );
    assert_eq!(parse_args(&s(["--help"])), Ok(Cli::Help));
    assert_eq!(parse_args(&s(["-h"])), Ok(Cli::Help));
    assert!(parse_args(&s(["frob"])).is_err());
    assert!(USAGE.starts_with("usage: duohive"), "{USAGE}");
    for shown in ["duohive hook", "duohive bench"] {
        assert!(USAGE.contains(shown), "{USAGE}");
    }
    // `bench-agent` is hidden.
    assert!(!USAGE.contains("bench-agent"), "{USAGE}");
}

/// Every file under `dir`, relative to it.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path.strip_prefix(dir).unwrap().to_path_buf());
            }
        }
    }
    out
}

#[test]
fn logging_writes_to_logs_dir_only() {
    use std::os::unix::fs::PermissionsExt;
    let env = Env::new();
    std::fs::create_dir_all(&env.paths.data_dir).unwrap();
    std::fs::write(env.paths.data_dir.join("keep.txt"), "x").unwrap();
    let guard = init_logging(&env.paths).unwrap();
    tracing::info!("hello from the logging test");
    drop(guard);
    let mut files = files_under(&env.paths.data_dir);
    files.sort();
    assert_eq!(files.len(), 2, "{files:?}");
    assert_eq!(files[0], PathBuf::from("keep.txt"));
    let log = &files[1];
    assert!(log.starts_with("logs"), "{log:?}");
    let text = std::fs::read_to_string(env.paths.data_dir.join(log)).unwrap();
    assert!(text.contains("hello from the logging test"), "{text}");
    let file_mode = std::fs::metadata(env.paths.data_dir.join(log))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(file_mode & 0o777, 0o600);
    let mode = std::fs::metadata(env.paths.logs_dir())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
    assert_eq!(files_under(&env.paths.config_dir), Vec::<PathBuf>::new());
}
