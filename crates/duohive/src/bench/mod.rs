//! `duohive bench`: runs a headless system over fake agents and reports DuoHive's
//! idle CPU, memory and hook-to-screen latency against the PRD targets.
//!
//! Everything it touches is its own: temp config and data directories and a
//! tmux server on the socket `duohive-bench-<pid>`, all removed at the end.
//!
//! With `--view` it also holds one live pane stream open on the first agent,
//! from the moment every agent has started to the end of the idle window
//! (P2-D4, Review Focus 3): its tmux client counts as DuoHive, and the other
//! panes must stay silent on it.

pub mod gate;
mod live_view;
mod measure;
mod observe;
mod report;

use std::path::Path;
use std::process::{Command as Process, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use duohive_cli::fake::fake_adapters_with_stamps;
use duohive_core::cli_kind::CliKind;
use duohive_core::command::Command;
use duohive_core::event::Event;
use duohive_core::hook::HookMessage;
use duohive_core::names::{AgentName, NodeName};
use duohive_core::org::{AgentDef, Fields, Org, Role};
use duohive_core::ports::{NoGit, NoNotifier};
use duohive_core::status::AgentState;
use duohive_core::validate::{DirProbe, DirStatus};
use duohive_core::worktree::RunIn;
use duohive_store::{Paths, save_org};
use duohive_tmux::{TmuxConfig, tmux_version as probe_tmux_version};
use duohive_tui::app::{AppConfig, run_app_on};
use duohive_tui::{QuitSignals, UiModel};
use tokio::sync::mpsc;

use crate::system::{StartError, SystemHandle, SystemOptions, start_system, tmux_config};
pub use live_view::LiveViewUse;
use live_view::{LiveView, next_chunk};
pub use measure::percentiles;
use observe::Observer;
pub use report::{BenchOutcome, BenchReport, Machine, Percentiles, ServerUse, Targets};

/// Every fifth agent plays OpenCode, whose fake has no hooks (screen-read path).
const SCREEN_READ_EVERY: u32 = 5;
/// A run whose 1-minute load is above this is discarded, not counted.
pub(crate) const LOAD_CEILING: f64 = 2.0;
/// Tests run on shared, busy machines: this disables the load gate for them.
pub(crate) const IGNORE_LOAD_VAR: &str = "DUOHIVE_BENCH_IGNORE_LOAD";
/// How long the agents may take to start, and to finish a turn or stop.
const START_WAIT: Duration = Duration::from_secs(90);
const TURN_WAIT: Duration = Duration::from_secs(60);
const STOP_WAIT: Duration = Duration::from_secs(30);

/// `base` made longer for a run with many agents: two more seconds for each agent beyond ten.
/// These waits are the bench's patience, not a target: 100 agents spawning hook processes on a
/// small machine need longer than 10 do, and a bench that gives up measures nothing.
pub(crate) fn patience(base: Duration, agents: usize) -> Duration {
    base + Duration::from_secs(2 * u64::try_from(agents.saturating_sub(10)).unwrap_or(0))
}
/// Quiet time between "all at rest" and the idle measurement, so the last
/// screen reads and frames are over.
const SETTLE: Duration = Duration::from_secs(2);
/// How long the live view's first chunk may take.
const VIEW_WAIT: Duration = Duration::from_secs(15);

/// What to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchArgs {
    /// How many agents.
    pub agents: u32,
    /// How long to measure idle CPU, in seconds.
    pub idle_secs: u64,
    /// Tasks given to each agent.
    pub turns: u32,
    /// Hold one live pane stream open from start to end (P2-D4).
    pub view: bool,
    /// Print JSON instead of text.
    pub json: bool,
}

impl Default for BenchArgs {
    fn default() -> Self {
        Self {
            agents: 10,
            idle_secs: 60,
            // Derived from the agent count by `turns_for`: only the default may be 0.
            turns: 0,
            view: false,
            json: false,
        }
    }
}

/// How many turns a run needs so that both the hook and the screen-read sample
/// sets reach 100: every fifth agent is read from the screen, so both kinds
/// need `100 / (agents / SCREEN_READ_EVERY)` turns at the fewest. A request of
/// 0 means "derive"; any other value is the caller's.
pub fn turns_for(agents: u32, requested: u32) -> u32 {
    if requested > 0 {
        return requested;
    }
    let screen_read = (agents / SCREEN_READ_EVERY).max(1);
    // The hook set needs `100 / agents` turns; the screen-read set, being
    // smaller, needs more. Round up so both reach 100.
    100_u32.div_ceil(agents).max(100_u32.div_ceil(screen_read))
}

/// The bench could not finish.
#[derive(Debug, thiserror::Error)]
pub enum BenchError {
    /// A file, directory or process failed.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// tmux is missing or did not answer `-V`.
    #[error("cannot run tmux: {0}")]
    Tmux(String),
    /// The org could not be written.
    #[error("cannot write the bench org: {0}")]
    Org(String),
    /// The system did not start.
    #[error(transparent)]
    Start(#[from] StartError),
    /// A phase took too long.
    #[error("timed out waiting for {0}")]
    Timeout(&'static str),
    /// Ctrl-C or SIGTERM: everything the bench started was cleaned up.
    #[error("interrupted")]
    Interrupted,
    /// The engine stopped on its own.
    #[error("the engine stopped unexpectedly")]
    EngineGone,
    /// `--view`: the live pane view could not attach or open.
    #[error("cannot open the live view: {0}")]
    View(String),
    /// The machine was too busy to measure: the gate discards the run.
    #[error("discarded: load {load:.2}")]
    LoadTooHigh {
        /// The 1-minute load average when the run began.
        load: f64,
    },
}

/// Runs the bench and returns what it measured.
///
/// # Errors
/// See [`BenchError`].
pub async fn run_bench(a: BenchArgs) -> Result<BenchReport, BenchError> {
    Ok(run_bench_detailed(a).await?.report)
}

/// Like [`run_bench`], with the machine's load and the bench's pid.
///
/// # Errors
/// See [`BenchError`].
pub async fn run_bench_detailed(a: BenchArgs) -> Result<BenchOutcome, BenchError> {
    let tmux = tmux_version()?;
    let machine_start = measure::load_average();
    let env = BenchEnv::new()?;
    let names = agent_names(a.agents);
    let org = bench_org(&names, &env.workdir);
    save_org(&env.paths, &org).map_err(|e| BenchError::Org(e.to_string()))?;
    let exe = std::env::current_exe()?;
    // Registered once, before anything starts: no signal falls between two waits.
    let mut interrupt = QuitSignals::install();
    // The gate is a machine that is not otherwise busy: the bench's own agents
    // are not up yet, so this is the honest reading.
    if let Some([one, ..]) = measure::load_average()
        && one > LOAD_CEILING
        && std::env::var_os(IGNORE_LOAD_VAR).is_none()
    {
        env.kill_tmux();
        return Err(BenchError::LoadTooHigh { load: one });
    }
    let (tap_tx, tap) = mpsc::unbounded_channel();
    let adapters = Arc::new(fake_adapters_with_stamps(&exe, &env.stamp_dir()));
    let stamps = stamp_paths(&names, &env.stamp_dir());
    let start = start_system(
        &env.paths,
        adapters,
        SystemOptions {
            tap: Some(tap_tx),
            first_frame: None,
            home: env.home(),
            notifier: Some(Box::new(NoNotifier)),
            git: Some(Box::new(NoGit)),
            git_env: Vec::new(),
        },
    );
    let handle = tokio::select! {
        handle = start => handle?,
        () = interrupt.recv() => {
            env.kill_tmux();
            return Err(BenchError::Interrupted);
        }
    };
    let observer = Observer::with_stamps(org, stamps);
    let mut run = Run {
        handle: Some(handle),
        tap,
        observer,
        names,
        procs: Procs::find(&env.paths.tmux_socket),
        paths: env.paths.clone(),
        live: None,
    };
    let result = tokio::select! {
        result = run.measure(&a, &exe) => result,
        () = interrupt.recv() => Err(BenchError::Interrupted),
    };
    // Interrupted or not: stop the agents, the system and the bench's tmux server.
    let (rss_client, rss_server) = (run.procs.client_peak_mb, run.procs.server_peak_mb);
    let control_client = !run.procs.clients.is_empty();
    // The view's client counts only when its own pid was found and sampled.
    let live_view = run.live.as_ref().map(|live| LiveViewUse {
        client_counted: live
            .client_pid()
            .is_some_and(|pid| run.procs.clients.contains(&pid)),
        ..live.usage()
    });
    // The reopen is timed after the first system has gone, with the agents'
    // tmux sessions left behind: the state a user comes back to.
    let reopen = if result.is_ok() {
        run.live = None;
        run.shutdown_system().await;
        Some(measure_reopen(&env, &exe).await)
    } else {
        None
    };
    run.finish().await;
    env.kill_tmux();
    let mut m = result?;
    m.first_frame_ms = reopen.transpose()?;
    Ok(BenchOutcome {
        report: BenchReport {
            agents: a.agents,
            tmux,
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            idle_cpu_pct: m.idle_cpu_pct,
            peak_rss_mb: measure::peak_rss_mb() + rss_client,
            rss_end_mb: m.rss_end_mb,
            hook_to_screen_ms: m.hooks,
            tilde_ms: m.tilde,
            first_frame_ms: m.first_frame_ms,
        },
        machine: Machine {
            cpus: std::thread::available_parallelism().map_or(1, usize::from),
            load_start: machine_start,
            load_end: measure::load_average(),
        },
        tmux_server: m.server_cpu_pct.map(|cpu_pct| ServerUse {
            cpu_pct,
            rss_mb: rss_server,
        }),
        control_client,
        live_view,
        hook_samples: m.hook_samples,
        helpers_rss_mb: m.helpers_rss_mb,
        exec_ms: m.exec_ms,
        pid: std::process::id(),
    })
}

/// What a finished measurement hands back.
struct Measured {
    idle_cpu_pct: f64,
    hooks: Percentiles,
    hook_samples: usize,
    server_cpu_pct: Option<f64>,
    /// DuoHive (this process plus its control clients) at the end of the window.
    rss_end_mb: f64,
    /// The screen-read freshness samples.
    tilde: Percentiles,
    /// The reopen's first frame, when it was timed.
    first_frame_ms: Option<f64>,
    /// The `duohive mcp` helpers' summed resident size.
    helpers_rss_mb: Option<f64>,
    /// `duohive --version`'s wall time.
    exec_ms: Option<f64>,
}

/// DuoHive's tmux clients (the control client, plus the live view's with
/// `--view`) and DuoHive's tmux server, sampled by pid.
struct Procs {
    clients: Vec<u32>,
    server: Option<u32>,
    client_peak_mb: f64,
    server_peak_mb: f64,
}

impl Procs {
    /// The clients are children of this process on the bench socket; the
    /// server's pid is whatever tmux says it is.
    fn find(socket: &str) -> Self {
        let clients = measure::child_pids_matching(socket);
        let server = Process::new("tmux")
            .args([
                "-f",
                "/dev/null",
                "-L",
                socket,
                "display-message",
                "-p",
                "#{pid}",
            ])
            .env_remove("TMUX")
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok());
        Self {
            clients,
            server,
            client_peak_mb: 0.0,
            server_peak_mb: 0.0,
        }
    }

    /// Reads the clients (summed) and the server, keeping the largest resident
    /// sizes seen.
    fn sample(&mut self) -> (Option<measure::ProcStats>, Option<measure::ProcStats>) {
        let client = self
            .clients
            .iter()
            .filter_map(|pid| measure::process_stats(*pid))
            .reduce(|a, b| measure::ProcStats {
                cpu: a.cpu + b.cpu,
                rss_mb: a.rss_mb + b.rss_mb,
            });
        let server = self.server.and_then(measure::process_stats);
        if let Some(c) = client {
            self.client_peak_mb = self.client_peak_mb.max(c.rss_mb);
        }
        if let Some(s) = server {
            self.server_peak_mb = self.server_peak_mb.max(s.rss_mb);
        }
        (client, server)
    }
}

fn agent_names(count: u32) -> Vec<AgentName> {
    (1..=count)
        .filter_map(|i| AgentName::parse(&format!("bench-{i}")).ok())
        .collect()
}

/// Duo plus one root role `Bench` holding the agents, working in `dir`.
fn bench_org(names: &[AgentName], dir: &Path) -> Org {
    let mut org = Org::blank(CliKind::Claude);
    let agents = names
        .iter()
        .zip(1..)
        .map(|(name, i)| AgentDef {
            name: name.clone(),
            fields: Fields {
                cli: (i % SCREEN_READ_EVERY == 0).then_some(CliKind::Opencode),
                ..Fields::default()
            },
        })
        .collect();
    if let Ok(name) = NodeName::parse("Bench") {
        org.roles = vec![Role {
            name,
            defaults: Fields {
                directory: Some(dir.display().to_string()),
                ..Fields::default()
            },
            agents,
        }];
    }
    org
}

/// The tmux version, as `duohive-tmux` reads it (`3.7c`).
fn tmux_version() -> Result<String, BenchError> {
    let cfg = TmuxConfig {
        tmux_bin: "tmux".into(),
        socket: String::new(),
        session: String::new(),
        buffer_name: String::new(),
        run_root: std::path::PathBuf::new(),
    };
    match probe_tmux_version(&cfg) {
        Ok(Some(v)) => Ok(v.raw.trim_start_matches("tmux ").to_owned()),
        Ok(None) => Err(BenchError::Tmux("tmux not found".to_owned())),
        Err(e) => Err(BenchError::Tmux(e.to_string())),
    }
}

/// Temp config and data directories and the bench's tmux socket, removed on drop.
struct BenchEnv {
    _tmp: tempfile::TempDir,
    paths: Paths,
    workdir: std::path::PathBuf,
}

impl BenchEnv {
    fn new() -> Result<Self, BenchError> {
        let tmp = tempfile::Builder::new()
            .prefix("duohive-bench-")
            .tempdir()?;
        let paths = Paths {
            config_dir: tmp.path().join("config"),
            data_dir: tmp.path().join("data"),
            tmux_socket: format!("duohive-bench-{}", std::process::id()),
        };
        let workdir = tmp.path().join("work");
        for dir in [
            &paths.config_dir,
            &paths.data_dir,
            &workdir,
            &tmp.path().join("stamps"),
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(Self {
            _tmp: tmp,
            paths,
            workdir,
        })
    }

    /// The home the bench's system expands `~` to: its own work dir, never the user's.
    fn home(&self) -> std::path::PathBuf {
        self.workdir.clone()
    }

    /// Where the fakes append their prompt times.
    fn stamp_dir(&self) -> std::path::PathBuf {
        self._tmp.path().join("stamps")
    }

    fn kill_tmux(&self) {
        let _ = Process::new("tmux")
            .args([
                "-f",
                "/dev/null",
                "-L",
                &self.paths.tmux_socket,
                "kill-server",
            ])
            .env_remove("TMUX")
            .output();
    }
}

impl Drop for BenchEnv {
    /// Also on an early return or a panic: the bench's tmux server never outlives it.
    fn drop(&mut self) {
        self.kill_tmux();
    }
}

/// The bench gives work in the agent's own directory, never a worktree.
fn give_work_command(agent: AgentName, task: String) -> Command {
    Command::GiveWork {
        agent,
        task,
        run: RunIn::Directory,
    }
}

/// The system, its hook tap and the screen stand-in (plus, with `--view`, the
/// live stream), driven together.
struct Run {
    handle: Option<SystemHandle>,
    tap: mpsc::UnboundedReceiver<HookMessage>,
    observer: Observer,
    names: Vec<AgentName>,
    procs: Procs,
    paths: Paths,
    live: Option<LiveView>,
}

impl Run {
    /// Start, work, settle, then watch idle: returns every timing the report
    /// prints.
    async fn measure(
        &mut self,
        a: &BenchArgs,
        exe: &std::path::Path,
    ) -> Result<Measured, BenchError> {
        self.pump(Duration::from_secs(5), |m| !m.agents.is_empty())
            .await?
            .then_some(())
            .ok_or(BenchError::Timeout("the first snapshot"))?;
        for agent in self.names.clone() {
            self.send(Command::StartAgent { agent }).await?;
        }
        self.wait_all(
            "every agent to start",
            patience(START_WAIT, self.names.len()),
            at_rest,
        )
        .await?;
        if a.view {
            self.open_view().await?;
        }
        self.procs.sample();
        let mut helpers = self.spawn_helpers(a.agents);
        let turns = turns_for(a.agents, a.turns);
        for turn in 1..=turns {
            let task = format!("bench task {turn}");
            for agent in self.names.clone() {
                let task = task.clone();
                self.send(give_work_command(agent, task)).await?;
            }
            let reply = format!("done: {task}");
            let turn_wait = patience(TURN_WAIT, self.names.len());
            self.wait_all("every agent to finish a turn", turn_wait, move |m, n| {
                at_rest(m, n) && m.view(n).last_reply.as_deref() == Some(reply.as_str())
            })
            .await?;
            self.procs.sample();
        }
        self.pump(SETTLE, |_| false).await?;
        let idle = Duration::from_secs(a.idle_secs);
        let (client_before, server_before) = self.procs.sample();
        let cpu_before = measure::cpu_time();
        let started = Instant::now();
        self.pump(idle, |_| false).await?;
        let cpu_after = measure::cpu_time();
        let wall = started.elapsed();
        let (client_after, server_after) = self.procs.sample();
        let used = |before: Option<Duration>, after: Option<Duration>| {
            before.zip(after).map(|(b, a)| a.saturating_sub(b))
        };
        // DuoHive = this process + its control client(s).
        let own = used(cpu_before, cpu_after).unwrap_or_default();
        let client =
            used(client_before.map(|c| c.cpu), client_after.map(|c| c.cpu)).unwrap_or_default();
        let server = used(server_before.map(|c| c.cpu), server_after.map(|c| c.cpu));
        // Memory is the end of the window, not the peak: the live view and the
        // screen reads settle well before it.
        let rss_end_mb = Self::rss_end_mb(client_after);
        let (hooks, hook_samples) = self.observer.hook_percentiles();
        let (tilde, _) = self.observer.tilde_percentiles();
        Ok(Measured {
            idle_cpu_pct: measure::cpu_percent(own + client, wall),
            hooks,
            hook_samples,
            server_cpu_pct: server.map(|u| measure::cpu_percent(u, wall)),
            rss_end_mb,
            tilde,
            first_frame_ms: None,
            helpers_rss_mb: Some(helpers_rss(&mut helpers)),
            exec_ms: measure::exec_once(exe),
        })
    }

    /// Spawns one idle `duohive mcp` per agent, beside the fakes: their total is
    /// reported (the agents' own helpers are excluded from the memory target).
    fn spawn_helpers(&self, agents: u32) -> Vec<std::process::Child> {
        let Ok(exe) = std::env::current_exe() else {
            return Vec::new();
        };
        (0..agents)
            .filter_map(|_| {
                Process::new(&exe)
                    .arg("mcp")
                    .env("DUOHIVE_TOKEN", "")
                    .env("DUOHIVE_DATA_DIR", &self.paths.data_dir)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .ok()
            })
            .collect()
    }

    /// This process's resident size plus its control clients'.
    fn rss_end_mb(client: Option<measure::ProcStats>) -> f64 {
        measure::current_rss_mb(std::process::id()).unwrap_or(0.0)
            + client.map_or(0.0, |c| c.rss_mb)
    }

    /// Opens the one live stream, on the first agent, and waits for its first chunk.
    async fn open_view(&mut self) -> Result<(), BenchError> {
        let agent = self
            .names
            .first()
            .cloned()
            .ok_or_else(|| BenchError::View("no agent to view".to_owned()))?;
        let socket = self.paths.tmux_socket.clone();
        let mut live = LiveView::attach(tmux_config(&self.paths), &socket, agent).await?;
        let first = tokio::time::timeout(VIEW_WAIT, live.next())
            .await
            .map_err(|_| BenchError::Timeout("the live view's first chunk"))?;
        live.take(first);
        if !live.is_open() {
            return Err(BenchError::View("the stream closed at once".to_owned()));
        }
        if let Some(pid) = live.client_pid() {
            self.procs.clients.push(pid);
        }
        self.live = Some(live);
        Ok(())
    }

    async fn send(&self, command: Command) -> Result<(), BenchError> {
        self.handle
            .as_ref()
            .ok_or(BenchError::EngineGone)?
            .commands
            .send(command)
            .await
            .map_err(|_| BenchError::EngineGone)
    }

    /// Waits until `ok(model, agent)` holds for every bench agent.
    async fn wait_all(
        &mut self,
        what: &'static str,
        limit: Duration,
        ok: impl Fn(&UiModel, &AgentName) -> bool,
    ) -> Result<(), BenchError> {
        let names = self.names.clone();
        let done = self.pump(limit, |m| names.iter().all(|n| ok(m, n))).await?;
        done.then_some(()).ok_or(BenchError::Timeout(what))
    }

    /// Stops every agent and the system, never failing: this is cleanup.
    async fn finish(mut self) {
        for agent in self.names.clone() {
            let _ = self.send(Command::StopAgent { agent }).await;
        }
        let names = self.names.clone();
        let _ = self
            .pump(patience(STOP_WAIT, names.len()), |m| {
                names
                    .iter()
                    .all(|n| m.view(n).status.state == AgentState::Stopped)
            })
            .await;
        self.shutdown_system().await;
    }

    /// Stops the system, leaving the agents' tmux sessions behind, as quitting
    /// the app does: the state the reopen is measured from.
    async fn shutdown_system(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.shutdown().await;
        }
    }

    /// Feeds events and hooks to the observer, drawing frames, until `done`
    /// holds (`true`) or `limit` passes (`false`).
    async fn pump(
        &mut self,
        limit: Duration,
        done: impl Fn(&UiModel) -> bool,
    ) -> Result<bool, BenchError> {
        let deadline = tokio::time::Instant::now() + limit;
        let mut tap_open = true;
        loop {
            if done(self.observer.model()) {
                return Ok(true);
            }
            let frame_at = self
                .observer
                .next_frame_at()
                .map(tokio::time::Instant::from_std);
            tokio::select! {
                event = self
                    .handle
                    .as_mut()
                    .ok_or(BenchError::EngineGone)?
                    .events
                    .recv() => {
                    let event: Event = event.ok_or(BenchError::EngineGone)?;
                    self.observer.event(event);
                }
                msg = self.tap.recv(), if tap_open => match msg {
                    Some(msg) => self.observer.hook(&msg),
                    None => tap_open = false,
                },
                chunk = next_chunk(self.live.as_mut()) => {
                    if let Some(live) = self.live.as_mut() {
                        live.take(chunk);
                    }
                }
                () = sleep_until_some(frame_at) => {}
                () = tokio::time::sleep_until(deadline) => return Ok(done(self.observer.model())),
            }
            self.observer.draw_if_due();
        }
    }
}

/// The stamp file of every bench agent, for the screen-read timing.
fn stamp_paths(
    names: &[AgentName],
    dir: &std::path::Path,
) -> std::collections::HashMap<AgentName, std::path::PathBuf> {
    names
        .iter()
        .map(|n| (n.clone(), dir.join(n.as_str())))
        .collect()
}

/// The helpers' summed resident size in MB, then kills them: they are the bench's.
fn helpers_rss(children: &mut [std::process::Child]) -> f64 {
    let total: f64 = children
        .iter()
        .filter_map(|c| measure::current_rss_mb(c.id()))
        .sum();
    for child in children.iter_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }
    total
}

/// The bench's directories all "exist": nothing on the reopen's screen depends
/// on the disk probe.
struct NoDirs;

impl DirProbe for NoDirs {
    fn probe(&self, _dir: &std::path::Path) -> DirStatus {
        DirStatus::Found
    }
}

/// How long the reopen's first frame took: stop the system leaving the agents in
/// tmux, start it again on the same paths, and time from just before the second
/// `start_system` to the first complete draw into a `TestBackend`.
///
/// The agents' tmux sessions outlive the first system, as they do when a user
/// quits and comes back: that is the case the target names.
async fn measure_reopen(env: &BenchEnv, exe: &std::path::Path) -> Result<f64, BenchError> {
    let adapters = Arc::new(fake_adapters_with_stamps(exe, &env.stamp_dir()));
    let (frame_tx, frame_rx) = tokio::sync::oneshot::channel::<()>();
    let started = Instant::now();
    let opened = start_system(
        &env.paths,
        adapters,
        SystemOptions {
            tap: None,
            first_frame: None,
            home: env.home(),
            notifier: Some(Box::new(NoNotifier)),
            git: Some(Box::new(NoGit)),
            git_env: Vec::new(),
        },
    )
    .await?;
    let mut handle = opened;
    let mut events = std::mem::replace(&mut handle.events, mpsc::unbounded_channel().1);
    let commands = handle.commands.clone();
    // The app quits on the first input: no terminal, so nothing types into it.
    let (input_tx, input_rx) = mpsc::channel(1);
    let _ = input_tx.clone();
    let cfg = AppConfig {
        pane_view: Arc::new(duohive_core::ports::NoPaneView),
        org_toml: env.paths.org_toml(),
        settings: handle.settings.clone(),
        utc_offset_secs: 0,
        no_color: true,
        editor: None,
        dirs: Arc::new(NoDirs),
        home: env.home(),
        halted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        running: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    };
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(160, 48))
        .map_err(|e| BenchError::Io(std::io::Error::other(e.to_string())))?;
    let app = run_app_on(
        &mut terminal,
        input_rx,
        cfg,
        &mut events,
        commands.clone(),
        Some(frame_tx),
    );
    tokio::select! {
        _ = frame_rx => {}
        result = app => {
            result.map_err(|e| BenchError::View(e.to_string()))?;
            return Err(BenchError::Timeout("the reopen's first frame"));
        }
    }
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    drop(input_tx);
    handle.shutdown().await;
    Ok(elapsed)
}

/// Sleeps until `at`, or forever when there is no frame due.
async fn sleep_until_some(at: Option<tokio::time::Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

fn at_rest(m: &UiModel, name: &AgentName) -> bool {
    m.view(name).status.state.is_at_rest()
}

#[cfg(test)]
mod tests {
    #[test]
    fn waits_grow_with_the_agent_count() {
        let base = Duration::from_secs(60);
        assert_eq!(patience(base, 1), base);
        assert_eq!(patience(base, 10), base);
        assert_eq!(patience(base, 100), Duration::from_secs(60 + 180));
        assert!(patience(base, 101) > patience(base, 100));
    }

    use super::*;

    #[test]
    fn existing_call_sites_give_work_in_directory() {
        let agent = AgentName::parse("a-1").unwrap();
        assert_eq!(
            give_work_command(agent.clone(), "bench task 1".to_owned()),
            Command::GiveWork {
                agent,
                task: "bench task 1".to_owned(),
                run: duohive_core::worktree::RunIn::Directory,
            }
        );
    }

    #[test]
    fn turns_default_reaches_100_samples() {
        assert!(turns_for(10, 0) >= 50, "{}", turns_for(10, 0));
        assert!(turns_for(100, 0) >= 5, "{}", turns_for(100, 0));
        assert_eq!(turns_for(100, 7), 7);
        assert_eq!(turns_for(10, 3), 3);
    }

    #[test]
    fn percentiles_of_known_samples() {
        let mut samples: Vec<f64> = (1..=100).rev().map(f64::from).collect();
        let p = percentiles(&mut samples);
        assert!((p.p50 - 50.5).abs() < 1e-9, "{p:?}");
        assert!((p.p95 - 95.05).abs() < 1e-9, "{p:?}");
        assert!((p.max - 100.0).abs() < 1e-9, "{p:?}");
        assert_eq!(percentiles(&mut []), Percentiles::default());
    }

    #[test]
    fn report_formats_like_the_spec() {
        let report = BenchReport {
            agents: 10,
            tmux: "3.7c".to_owned(),
            platform: "macos-aarch64".to_owned(),
            idle_cpu_pct: 0.4,
            peak_rss_mb: 14.2,
            rss_end_mb: 11.2,
            hook_to_screen_ms: Percentiles {
                p50: 12.0,
                p95: 40.0,
                max: 61.0,
            },
            tilde_ms: Percentiles {
                p50: 30.0,
                p95: 60.0,
                max: 90.0,
            },
            first_frame_ms: Some(80.0),
        };
        let expected = "\
duohive bench · 10 agents · tmux 3.7c · macos-aarch64
idle CPU        0.4 %    target < 1 %
memory          11.2 MB  target < 20 MB
peak memory     14.2 MB (reported, not gated)
hook → screen   p50 12 ms · p95 40 ms · max 61 ms   target < 1000 ms
screen-read (~) p50 30 ms · p95 60 ms · max 90 ms   target < 2000 ms (p95)
first frame     80.0 ms   target < 100 ms
";
        assert_eq!(report.to_string(), expected);
    }

    fn report(p: Percentiles) -> BenchReport {
        BenchReport {
            agents: 10,
            tmux: "3.7c".to_owned(),
            platform: "macos-aarch64".to_owned(),
            idle_cpu_pct: 0.4,
            peak_rss_mb: 14.2,
            rss_end_mb: 11.2,
            hook_to_screen_ms: p,
            tilde_ms: Percentiles {
                p50: 30.0,
                p95: 60.0,
                max: 90.0,
            },
            first_frame_ms: Some(80.0),
        }
    }

    #[test]
    fn no_samples_is_a_miss_not_a_pass() {
        let text = report(Percentiles::default()).to_string();
        assert!(text.contains("no samples"), "{text}");
        assert_eq!(text.matches("MISSED").count(), 1, "{text}");
    }

    #[test]
    fn the_hook_target_is_judged_on_the_slowest_hook() {
        let text = report(Percentiles {
            p50: 12.0,
            p95: 40.0,
            max: 1000.0,
        })
        .to_string();
        assert_eq!(text.matches("MISSED").count(), 1, "{text}");
    }

    #[test]
    fn the_outcome_says_what_it_did_not_measure() {
        let outcome = BenchOutcome {
            report: report(Percentiles {
                p50: 1.0,
                p95: 2.0,
                max: 3.0,
            }),
            machine: Machine {
                cpus: 10,
                load_start: None,
                load_end: None,
            },
            tmux_server: Some(ServerUse {
                cpu_pct: 0.2,
                rss_mb: 5.0,
            }),
            control_client: true,
            live_view: Some(LiveViewUse {
                agent: "bench-1".to_owned(),
                streams_opened: 1,
                open_at_end: true,
                closed: None,
                chunks: 12,
                bytes: 2048,
                foreign_output_lines: 0,
                client_pid: Some(7),
                client_counted: true,
            }),
            hook_samples: 4,
            // Whatever was not measured is said so.
            helpers_rss_mb: None,
            exec_ms: None,
            pid: 1,
        };
        let text = outcome.to_string();
        assert!(text.contains("tmux server"), "{text}");
        assert!(text.contains("not DuoHive"), "{text}");
        assert!(text.contains("not measured"), "{text}");
        assert!(text.contains("10-agent targets"), "{text}");
        assert_eq!(outcome.to_json()["tmux_server"]["rss_mb"], 5.0);
        assert!(
            text.contains("1 stream on bench-1 · open to the end · 2.0 KB in 12 chunks"),
            "{text}"
        );
        assert_eq!(outcome.to_json()["live_view"]["streams_opened"], 1);
    }

    #[test]
    fn linux_cpu_granularity_is_noted() {
        let outcome = |platform: &str| BenchOutcome {
            report: BenchReport {
                platform: platform.to_owned(),
                ..report(Percentiles::default())
            },
            machine: Machine {
                cpus: 1,
                load_start: None,
                load_end: None,
            },
            tmux_server: None,
            control_client: true,
            live_view: None,
            hook_samples: 0,
            helpers_rss_mb: None,
            exec_ms: None,
            pid: 1,
        };
        let linux = outcome("linux-x86_64").to_string();
        assert!(linux.contains("whole seconds"), "{linux}");
        let mac = outcome("macos-aarch64").to_string();
        assert!(!mac.contains("whole seconds"), "{mac}");
    }

    #[test]
    fn a_newer_hook_replaces_an_older_one_for_the_same_agent() {
        let mut obs = observe::Observer::new(Org::blank(CliKind::Claude));
        let hook = |event: &str| HookMessage {
            token: String::new(),
            agent: "bench-1".to_owned(),
            event: event.to_owned(),
            payload: serde_json::Value::Null,
            sent_at_ms: 1,
        };
        obs.hook(&hook("busy"));
        obs.hook(&hook("turn-end"));
        assert_eq!(obs.pending_count(), 1);
    }

    #[test]
    fn a_missed_target_is_said_so() {
        let report = BenchReport {
            agents: 10,
            tmux: "3.7c".to_owned(),
            platform: "linux-x86_64".to_owned(),
            idle_cpu_pct: 2.5,
            peak_rss_mb: 30.0,
            rss_end_mb: 30.0,
            hook_to_screen_ms: Percentiles {
                p50: 12.0,
                p95: 1500.0,
                max: 1600.0,
            },
            tilde_ms: Percentiles {
                p50: 30.0,
                p95: 60.0,
                max: 90.0,
            },
            first_frame_ms: Some(80.0),
        };
        let text = report.to_string();
        assert_eq!(text.matches("MISSED").count(), 3, "{text}");
    }
}
