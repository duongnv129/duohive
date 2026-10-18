//! The headless system the binary, the bench and the e2e tests share: the
//! store, the org and settings files, the hook listener, tmux, the engine on
//! its own thread, and CLI detection after the first frame.
//!
//! Threads and runtimes: `run_core` calls the `Store` synchronously, so it runs
//! on its own thread with its own current-thread runtime, never blocking the
//! caller's runtime (the TUI's and the listener's). Any panic ends the system
//! in order: a core panic drops the event sender (the app sees the channel
//! close), and a panicking task makes its supervisor send `Command::Shutdown`.

mod detection;
mod enriching_store;

pub use enriching_store::EnrichingStore;

use std::collections::HashMap;
use std::ffi::OsString;
use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, PoisonError, RwLock};
use std::thread;
use std::time::Duration;

use duohive_core::adapter::{Adapters, CliPresence};
use duohive_core::cli_kind::CliKind;
use duohive_core::command::Command;
use duohive_core::engine::{Engine, EngineConfig, Input, Ports, run_core};
use duohive_core::event::Event;
use duohive_core::hook::HookMessage;
use duohive_core::inherit::resolve;
use duohive_core::org::Org;
use duohive_core::ports::{Git, NoNotifier, Notifier, OrgFileError, Store, StoreError};
use duohive_core::settings::Settings;
use duohive_store::{Paths, SqliteStore, load_settings};
use duohive_tmux::{TmuxConfig, TmuxError, TmuxRuntime};
use tokio::sync::{mpsc, oneshot};
use tokio::task::{AbortHandle, JoinHandle};

use crate::git::GitCli;
use crate::hook_listener::{AgentClis, HookListener, InstanceLock, ListenerError};
use crate::notifier::{ALERT_TIMEOUT, DesktopNotifier, Platform};
use crate::real_ports::{FsDirProbe, RandomIds, SystemClock};
use detection::{detect_all, first_found, run_detection};

/// The tmux session (D31).
const SESSION: &str = "duohive";
const INPUT_QUEUE: usize = 1024;
const COMMAND_QUEUE: usize = 64;
/// D6 and D21.
const START_TIMEOUT: Duration = Duration::from_secs(120);
const STOP_GRACE: Duration = Duration::from_secs(5);
const SETTLE_AFTER: Duration = Duration::from_millis(1500);
const END_HOOK_GRACE: Duration = Duration::from_millis(500);

/// What the caller adds to the system.
#[derive(Debug)]
pub struct SystemOptions {
    /// Receives a copy of every live hook (bench, tests).
    pub tap: Option<mpsc::UnboundedSender<HookMessage>>,
    /// Fired by `run_app` after its first frame; detection waits for it.
    /// `None` detects at once (bench, tests).
    pub first_frame: Option<oneshot::Receiver<()>>,
    /// The home directory `~` expands to and Duo works in (D5), read from the
    /// environment by the caller; empty is [`StartError::NoHome`].
    pub home: PathBuf,
    /// Replaces the desktop notifier: tests and the bench set it so none posts a
    /// real alert. `None` detects the system's own ([`DesktopNotifier::detect`]).
    pub notifier: Option<Box<dyn Notifier>>,
    /// Replaces git: tests that don't exercise worktrees set [`NoGit`](duohive_core::ports::NoGit).
    /// `None` runs the `git` binary ([`GitCli`]).
    pub git: Option<Box<dyn Git>>,
    /// Added to every git child's environment when `git` is `None`: the app
    /// passes none; tests point `HOME` and `GIT_CONFIG_GLOBAL` at a temp dir,
    /// so the user's global git config is never read or written.
    pub git_env: Vec<(OsString, OsString)>,
}

/// The system could not start.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// Another DuoHive holds the instance lock (Review Focus 5).
    #[error("DuoHive is already running")]
    AlreadyRunning,
    /// No home directory given.
    #[error("cannot find your home directory: HOME is not set")]
    NoHome,
    /// The start-up work off the async executor failed to run.
    #[error("start-up failed: {0}")]
    Blocking(String),
    /// The running executable, which hooks call, cannot be found.
    #[error("cannot find the duohive executable: {0}")]
    Exe(io::Error),
    /// `state.db` cannot be opened.
    #[error("cannot open the state database: {0}")]
    Store(StoreError),
    /// `run.sock` cannot be bound.
    #[error("cannot listen for hooks: {0}")]
    Listener(io::Error),
    /// tmux is missing, too old, or failed to set up.
    #[error(transparent)]
    Tmux(#[from] TmuxError),
    /// The core thread cannot be spawned.
    #[error("cannot start the core: {0}")]
    Core(io::Error),
}

/// A running system. Dropping it without [`SystemHandle::shutdown`] still asks
/// the listener and the core to stop, but waits for neither.
pub struct SystemHandle {
    /// Commands to the engine.
    pub commands: mpsc::Sender<Command>,
    /// The engine's events, starting with the start-up `Snapshot`.
    pub events: mpsc::UnboundedReceiver<Event>,
    /// The settings at start-up (the defaults when `settings.toml` is invalid, D29).
    pub settings: Settings,
    core: Option<thread::JoinHandle<()>>,
    /// Stops the hook listener (also when dropped).
    listener_stop: Option<oneshot::Sender<()>>,
    /// The listener's supervisor: ends once the listener has stopped.
    listener: Option<JoinHandle<()>>,
    /// The detection and event-forwarding tasks.
    tasks: Vec<AbortHandle>,
    /// One supervisor per task: each ends when its task does.
    supervisors: Vec<JoinHandle<()>>,
    /// The single-instance lock, released last.
    lock: Option<InstanceLock>,
}

impl SystemHandle {
    /// Stops DuoHive, never an agent, losing no hook (Review Focus 3):
    /// 1. the listener stops: `run.sock` goes, so a new hook queues in
    ///    `pending_hooks`, and hooks already sent reach the core;
    /// 2. `Command::Shutdown`: the core stores every hook it has not applied as
    ///    a pending hook, then ends (awaited);
    /// 3. detection and event forwarding are aborted and awaited;
    /// 4. the instance lock goes last, so no second instance starts meanwhile.
    ///
    /// tmux windows keep running; the next start replays the queued hooks.
    pub async fn shutdown(mut self) {
        if let Some(stop) = self.listener_stop.take() {
            let _ = stop.send(());
        }
        if let Some(listener) = self.listener.take() {
            let _ = listener.await;
        }
        let _ = self.commands.send(Command::Shutdown).await;
        if let Some(core) = self.core.take() {
            match tokio::task::spawn_blocking(move || core.join()).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => tracing::error!("the core thread panicked"),
                Err(e) => tracing::error!(error = %e, "cannot join the core thread"),
            }
        }
        for task in &self.tasks {
            task.abort();
        }
        for supervisor in self.supervisors.drain(..) {
            let _ = supervisor.await;
        }
        drop(self.lock.take());
    }
}

impl Drop for SystemHandle {
    /// Without `shutdown`: asks the listener and the core to stop, waiting for
    /// neither. This is weaker than [`SystemHandle::shutdown`]:
    /// - nothing orders the two, so the core may stop before the listener has
    ///   forwarded the hooks it had accepted, and those are lost;
    /// - `Shutdown` is only `try_send`, so a full command queue drops it, and the
    ///   core then runs on until its channels close;
    /// - the instance lock is released at once, while the core may still be
    ///   writing to `state.db`.
    ///
    /// Call `shutdown` whenever an async context is available.
    fn drop(&mut self) {
        if let Some(stop) = self.listener_stop.take() {
            let _ = stop.send(());
        }
        if self.core.is_some() {
            let _ = self.commands.try_send(Command::Shutdown);
        }
    }
}

/// Starts the system (TD §4.1 Start-up). See the module docs.
///
/// # Errors
/// Another instance running, no home, `state.db`, `run.sock`, tmux or the
/// core thread failing.
#[expect(
    clippy::too_many_lines,
    reason = "wires every port in one place; split when a port is added"
)]
pub async fn start_system(
    paths: &Paths,
    adapters: Arc<Adapters>,
    opts: SystemOptions,
) -> Result<SystemHandle, StartError> {
    if opts.home.as_os_str().is_empty() {
        return Err(StartError::NoHome);
    }
    let home = opts.home;
    let notifier_override = opts.notifier;
    let git_override = opts.git;
    let git_env = opts.git_env;
    let hook_exe = std::env::current_exe().map_err(StartError::Exe)?;
    let agents = AgentClis::default();
    let files = {
        let paths = paths.clone();
        let (adapters, agents) = (Arc::clone(&adapters), Arc::clone(&agents));
        // SQLite and file reads block: keep them off the caller's executor.
        tokio::task::spawn_blocking(move || read_files(&paths, adapters, agents))
            .await
            .map_err(|e| StartError::Blocking(e.to_string()))??
    };
    let Files {
        store,
        settings,
        settings_error,
        loaded,
    } = files;

    // ORDER MATTERS: bind before anything spawns a process (tmux below, CLI
    // `--version` detection). On macOS std marks the socket close-on-exec only
    // after creating it, so a process spawned meanwhile would inherit the listener.
    let mut listener = HookListener::bind(&paths.run_sock()).map_err(|e| match e {
        ListenerError::AlreadyRunning => StartError::AlreadyRunning,
        ListenerError::Io(e) => StartError::Listener(e),
    })?;
    let lock = listener.take_lock();

    // From here on processes may be spawned.
    let (org, org_error, first_run_inventory) = match loaded {
        Loaded::Org(org) => (org, None, None),
        // First run: detect once, now, to pick the blank org's CLI. If that is
        // cancelled, the detection task detects instead (never an empty inventory).
        Loaded::FirstRun => match detect_all(Arc::clone(&adapters)).await {
            Some(list) => (Org::blank(first_found(&list)), None, Some(list)),
            None => (Org::blank(CliKind::Claude), None, None),
        },
        // D29: a broken org.toml runs on the last good org, with the banner.
        Loaded::Broken { error, last_good } => (
            last_good.unwrap_or_else(|| Org::blank(CliKind::Claude)),
            Some(error),
            None,
        ),
    };
    let tracker = Tracker {
        agents: Arc::clone(&agents),
        org: Arc::new(RwLock::new(org.clone())),
    };
    tracker.set(&org);

    let (inputs_tx, inputs_rx) = mpsc::channel(INPUT_QUEUE);
    let runtime = TmuxRuntime::connect(tmux_config(paths), inputs_tx.clone()).await?;
    let notifier = match notifier_override {
        Some(notifier) => notifier,
        None => detect_notifier(inputs_tx.clone()).await,
    };
    let git = git_override.unwrap_or_else(|| {
        Box::new(GitCli::new(
            paths.worktrees_dir(),
            inputs_tx.clone(),
            git_env,
        ))
    });
    let ports = Ports {
        runtime: Box::new(runtime),
        store: Box::new(store),
        clock: Box::new(SystemClock),
        ids: Box::new(RandomIds),
        dirs: Arc::new(FsDirProbe),
        notifier,
        git,
    };
    let cfg = EngineConfig {
        home: home.clone(),
        data_dir: paths.data_dir.clone(),
        hook_exe,
        start_timeout: START_TIMEOUT,
        stop_grace: STOP_GRACE,
        settle_after: SETTLE_AFTER,
        end_hook_grace: END_HOOK_GRACE,
    };
    let engine = Engine::new(
        org,
        org_error,
        settings.clone(),
        settings_error,
        cfg,
        ports,
        Arc::clone(&adapters),
    );
    // The detection task detects unless this first run already did.
    let first_run = first_run_inventory.is_some();
    let (events_tx, events) = mpsc::unbounded_channel();
    let (commands, commands_rx) = mpsc::channel(COMMAND_QUEUE);
    let (core_tx, core_rx) = mpsc::unbounded_channel();
    let core = spawn_core(engine, first_run_inventory, commands_rx, inputs_rx, core_tx)?;
    let detection = run_detection(
        opts.first_frame,
        (!first_run).then(|| Arc::clone(&adapters)),
        Arc::clone(&tracker.org),
        home,
        inputs_tx.clone(),
    );
    let (listener_stop, stop) = oneshot::channel::<()>();
    let calls = duohive_mcp::calls::Calls::new();
    let serve = listener.serve_until(
        agents,
        adapters,
        inputs_tx,
        opts.tap,
        calls.clone(),
        async {
            // Sent or dropped: either way, stop.
            let _ = stop.await;
        },
    );
    let (_, listener) = supervise("hook listener", boxed(serve), commands.clone());
    let forward = forward_events(core_rx, events_tx, tracker, calls);
    let mut tasks = Vec::new();
    let mut supervisors = Vec::new();
    for (name, task) in [
        ("detection", boxed(detection)),
        ("event forwarding", boxed(forward)),
    ] {
        let (abort, supervisor) = supervise(name, task, commands.clone());
        tasks.push(abort);
        supervisors.push(supervisor);
    }
    Ok(SystemHandle {
        commands,
        events,
        settings,
        core: Some(core),
        listener_stop: Some(listener_stop),
        listener: Some(listener),
        tasks,
        supervisors,
        lock,
    })
}

/// The system's own notifier, or none where the platform has no support.
/// `notify-send --help` blocks, so detection runs off the executor.
async fn detect_notifier(inputs: mpsc::Sender<Input>) -> Box<dyn Notifier> {
    let Some(platform) = Platform::current() else {
        return Box::new(NoNotifier);
    };
    let path_env: HashMap<String, String> = std::env::var("PATH")
        .map(|path| HashMap::from([("PATH".to_owned(), path)]))
        .unwrap_or_default();
    let detect = move || DesktopNotifier::detect(platform, &path_env, inputs, ALERT_TIMEOUT);
    match tokio::task::spawn_blocking(detect).await {
        Ok(notifier) => Box::new(notifier),
        Err(e) => {
            tracing::warn!(error = %e, "desktop alert detection failed");
            Box::new(NoNotifier)
        }
    }
}

/// What start-up reads from disk.
struct Files {
    store: EnrichingStore,
    settings: Settings,
    settings_error: Option<OrgFileError>,
    loaded: Loaded,
}

/// `org.toml` at start-up.
enum Loaded {
    Org(Org),
    /// No `org.toml` yet.
    FirstRun,
    /// D29: it doesn't parse; the last good org, if any, stands in.
    Broken {
        error: OrgFileError,
        last_good: Option<Org>,
    },
}

/// Opens `state.db` and reads `settings.toml` and `org.toml` (blocking).
fn read_files(
    paths: &Paths,
    adapters: Arc<Adapters>,
    agents: AgentClis,
) -> Result<Files, StartError> {
    let sqlite = SqliteStore::open(paths).map_err(StartError::Store)?;
    let mut store = EnrichingStore::new(sqlite, adapters, agents);
    // D29: an invalid settings.toml starts on the defaults, with the banner.
    let (settings, settings_error) = match load_settings(&paths.settings_toml()) {
        Ok(s) => (s, None),
        Err(e) => (Settings::default(), Some(e)),
    };
    let loaded = match store.load_org() {
        Ok(Some(org)) => Loaded::Org(org),
        Ok(None) => Loaded::FirstRun,
        Err(error) => Loaded::Broken {
            error,
            last_good: store.last_good_org(),
        },
    };
    Ok(Files {
        store,
        settings,
        settings_error,
        loaded,
    })
}

type Task = std::pin::Pin<Box<dyn Future<Output = ()> + Send>>;

fn boxed(f: impl Future<Output = ()> + Send + 'static) -> Task {
    Box::pin(f)
}

/// DuoHive's tmux server for `paths` (D31): the system's runtime and the live
/// pane view both connect with it.
pub fn tmux_config(paths: &Paths) -> TmuxConfig {
    TmuxConfig {
        tmux_bin: PathBuf::from("tmux"),
        socket: paths.tmux_socket.clone(),
        session: SESSION.to_owned(),
        buffer_name: format!("duohive-{}", std::process::id()),
        run_root: paths.data_dir.join("run"),
    }
}

/// Runs the core on its own thread and current-thread runtime (it blocks on the
/// store): first the engine's start-up (its `Snapshot`, then the first run's
/// inventory), then the driver loop.
fn spawn_core(
    mut engine: Engine,
    first_run_inventory: Option<Vec<(CliKind, CliPresence)>>,
    commands: mpsc::Receiver<Command>,
    inputs: mpsc::Receiver<Input>,
    events: mpsc::UnboundedSender<Event>,
) -> Result<thread::JoinHandle<()>, StartError> {
    thread::Builder::new()
        .name("duohive-core".to_owned())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build();
            let rt = match runtime {
                Ok(rt) => rt,
                // Dropping the event sender tells the app the core is gone.
                Err(e) => return tracing::error!(error = %e, "cannot build the core runtime"),
            };
            let mut first = engine.start_up();
            if let Some(list) = first_run_inventory {
                first.extend(engine.handle(Input::CliInventory(list)));
            }
            for event in first {
                let _ = events.send(event);
            }
            rt.block_on(run_core(engine, commands, inputs, events));
        })
        .map_err(StartError::Core)
}

/// Spawns `task` and a supervisor that shuts the system down if it panics.
fn supervise(
    name: &'static str,
    task: Task,
    commands: mpsc::Sender<Command>,
) -> (AbortHandle, JoinHandle<()>) {
    let inner = tokio::spawn(task);
    let abort = inner.abort_handle();
    let supervisor = tokio::spawn(async move {
        if let Err(e) = inner.await
            && e.is_panic()
        {
            tracing::error!(task = name, "task panicked: shutting down");
            let _ = commands.send(Command::Shutdown).await;
        }
    });
    (abort, supervisor)
}

/// Passes the core's events on, keeping the agents' CLIs and the org current.
/// Ends when the core does; drops the core's events once the app stops listening.
async fn forward_events(
    mut from_core: mpsc::UnboundedReceiver<Event>,
    to_app: mpsc::UnboundedSender<Event>,
    tracker: Tracker,
    calls: duohive_mcp::calls::Calls,
) {
    while let Some(event) = from_core.recv().await {
        // A tool result goes to its call, never to the app.
        if let Event::ToolResult { call_id, result } = event {
            calls.resolve(call_id, result);
            continue;
        }
        tracker.track(&event);
        if to_app.send(event).is_err() {
            return;
        }
    }
}

/// The org as last reported, and each agent's CLI from it: hook enrichment
/// (live and replayed, D22) and the directory probes read them.
struct Tracker {
    agents: AgentClis,
    org: Arc<RwLock<Org>>,
}

impl Tracker {
    fn track(&self, event: &Event) {
        match event {
            Event::Snapshot(s) => self.set(&s.org),
            Event::OrgChanged { org, .. } => self.set(org),
            _ => {}
        }
    }

    fn set(&self, org: &Org) {
        let clis = std::iter::once(org.duo.name.clone())
            .chain(org.agents().into_iter().map(|a| a.agent.name.clone()))
            .filter_map(|name| {
                let cli = resolve(org, &name).ok()?.cli;
                Some((name.as_str().to_owned(), cli))
            })
            .collect();
        *self.agents.write().unwrap_or_else(PoisonError::into_inner) = clis;
        org.clone_into(&mut self.org.write().unwrap_or_else(PoisonError::into_inner));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_panicking_task_shuts_the_system_down() {
        let (tx, mut rx) = mpsc::channel(1);
        // `resume_unwind` panics without the panic hook: no noise in the output.
        let task = boxed(async { std::panic::resume_unwind(Box::new("boom")) });
        let (_abort, supervisor) = supervise("test", task, tx);
        supervisor.await.unwrap();
        assert_eq!(rx.recv().await, Some(Command::Shutdown));
    }

    #[tokio::test]
    async fn an_aborted_task_shuts_nothing_down() {
        let (tx, mut rx) = mpsc::channel(1);
        let (abort, supervisor) = supervise("test", boxed(std::future::pending()), tx);
        abort.abort();
        supervisor.await.unwrap();
        assert_eq!(rx.recv().await, None);
    }
}
