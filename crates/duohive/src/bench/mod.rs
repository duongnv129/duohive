//! `duohive bench`: runs a headless system over fake agents and reports DuoHive's
//! idle CPU, memory and hook-to-screen latency against the PRD targets.
//!
//! Everything it touches is its own: temp config and data directories and a
//! tmux server on the socket `duohive-bench-<pid>`, all removed at the end.

mod measure;
mod observe;
mod report;

use std::path::Path;
use std::process::Command as Process;
use std::sync::Arc;
use std::time::{Duration, Instant};

use duohive_cli::fake::fake_adapters;
use duohive_core::cli_kind::CliKind;
use duohive_core::command::Command;
use duohive_core::event::Event;
use duohive_core::hook::HookMessage;
use duohive_core::names::{AgentName, NodeName};
use duohive_core::org::{AgentDef, Fields, Org, Role};
use duohive_core::ports::NoNotifier;
use duohive_core::status::AgentState;
use duohive_core::worktree::RunIn;
use duohive_store::{Paths, save_org};
use duohive_tmux::{TmuxConfig, tmux_version as probe_tmux_version};
use duohive_tui::{QuitSignals, UiModel};
use tokio::sync::mpsc;

use crate::system::{StartError, SystemHandle, SystemOptions, start_system};
pub use measure::percentiles;
use observe::Observer;
pub use report::{BenchOutcome, BenchReport, Machine, Percentiles, ServerUse, Targets};

/// Every fifth agent plays OpenCode, whose fake has no hooks (screen-read path).
const SCREEN_READ_EVERY: u32 = 5;
/// How long the agents may take to start, and to finish a turn or stop.
const START_WAIT: Duration = Duration::from_secs(90);
const TURN_WAIT: Duration = Duration::from_secs(60);
const STOP_WAIT: Duration = Duration::from_secs(30);
/// Quiet time between "all at rest" and the idle measurement, so the last
/// screen reads and frames are over.
const SETTLE: Duration = Duration::from_secs(2);

/// What to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchArgs {
    /// How many agents.
    pub agents: u32,
    /// How long to measure idle CPU, in seconds.
    pub idle_secs: u64,
    /// Tasks given to each agent.
    pub turns: u32,
    /// Print JSON instead of text.
    pub json: bool,
}

impl Default for BenchArgs {
    fn default() -> Self {
        Self {
            agents: 10,
            idle_secs: 30,
            turns: 3,
            json: false,
        }
    }
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
    let (tap_tx, tap) = mpsc::unbounded_channel();
    let start = start_system(
        &env.paths,
        Arc::new(fake_adapters(&exe)),
        SystemOptions {
            tap: Some(tap_tx),
            first_frame: None,
            home: env.home(),
            notifier: Some(Box::new(NoNotifier)),
        },
    );
    let handle = tokio::select! {
        handle = start => handle?,
        () = interrupt.recv() => {
            env.kill_tmux();
            return Err(BenchError::Interrupted);
        }
    };
    let observer = Observer::new(org);
    let mut run = Run {
        handle,
        tap,
        observer,
        names,
        procs: Procs::find(&env.paths.tmux_socket),
    };
    let result = tokio::select! {
        result = run.measure(&a) => result,
        () = interrupt.recv() => Err(BenchError::Interrupted),
    };
    // Interrupted or not: stop the agents, the system and the bench's tmux server.
    let (rss_client, rss_server) = (run.procs.client_peak_mb, run.procs.server_peak_mb);
    let control_client = run.procs.client.is_some();
    run.finish().await;
    env.kill_tmux();
    let m = result?;
    Ok(BenchOutcome {
        report: BenchReport {
            agents: a.agents,
            tmux,
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            idle_cpu_pct: m.idle_cpu_pct,
            peak_rss_mb: measure::peak_rss_mb() + rss_client,
            hook_to_screen_ms: m.hooks,
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
        hook_samples: m.hook_samples,
        pid: std::process::id(),
    })
}

/// What a finished measurement hands back.
struct Measured {
    idle_cpu_pct: f64,
    hooks: Percentiles,
    hook_samples: usize,
    server_cpu_pct: Option<f64>,
}

/// DuoHive's tmux control client and DuoHive's tmux server, sampled by pid.
struct Procs {
    client: Option<u32>,
    server: Option<u32>,
    client_peak_mb: f64,
    server_peak_mb: f64,
}

impl Procs {
    /// The control client is a child of this process on the bench socket; the
    /// server's pid is whatever tmux says it is.
    fn find(socket: &str) -> Self {
        let client = measure::child_pids_matching(socket).first().copied();
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
            client,
            server,
            client_peak_mb: 0.0,
            server_peak_mb: 0.0,
        }
    }

    /// Reads both processes, keeping the largest resident size seen.
    fn sample(&mut self) -> (Option<measure::ProcStats>, Option<measure::ProcStats>) {
        let client = self.client.and_then(measure::process_stats);
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
        for dir in [&paths.config_dir, &paths.data_dir, &workdir] {
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

/// The system, its hook tap and the screen stand-in, driven together.
struct Run {
    handle: SystemHandle,
    tap: mpsc::UnboundedReceiver<HookMessage>,
    observer: Observer,
    names: Vec<AgentName>,
    procs: Procs,
}

impl Run {
    /// Start, work, settle, then watch idle: returns idle CPU and the hook timings.
    async fn measure(&mut self, a: &BenchArgs) -> Result<Measured, BenchError> {
        self.pump(Duration::from_secs(5), |m| !m.agents.is_empty())
            .await?
            .then_some(())
            .ok_or(BenchError::Timeout("the first snapshot"))?;
        for agent in self.names.clone() {
            self.send(Command::StartAgent { agent }).await?;
        }
        self.wait_all("every agent to start", START_WAIT, at_rest)
            .await?;
        self.procs.sample();
        for turn in 1..=a.turns {
            let task = format!("bench task {turn}");
            for agent in self.names.clone() {
                let task = task.clone();
                self.send(give_work_command(agent, task)).await?;
            }
            let reply = format!("done: {task}");
            self.wait_all("every agent to finish a turn", TURN_WAIT, move |m, n| {
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
        // DuoHive = this process + its control client.
        let own = used(cpu_before, cpu_after).unwrap_or_default();
        let client =
            used(client_before.map(|c| c.cpu), client_after.map(|c| c.cpu)).unwrap_or_default();
        let server = used(server_before.map(|c| c.cpu), server_after.map(|c| c.cpu));
        let mut samples = self.observer.samples().to_vec();
        Ok(Measured {
            idle_cpu_pct: measure::cpu_percent(own + client, wall),
            hooks: percentiles(&mut samples),
            hook_samples: samples.len(),
            server_cpu_pct: server.map(|u| measure::cpu_percent(u, wall)),
        })
    }

    async fn send(&self, command: Command) -> Result<(), BenchError> {
        self.handle
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
            .pump(STOP_WAIT, |m| {
                names
                    .iter()
                    .all(|n| m.view(n).status.state == AgentState::Stopped)
            })
            .await;
        self.handle.shutdown().await;
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
                event = self.handle.events.recv() => {
                    let event: Event = event.ok_or(BenchError::EngineGone)?;
                    self.observer.event(event);
                }
                msg = self.tap.recv(), if tap_open => match msg {
                    Some(msg) => self.observer.hook(&msg),
                    None => tap_open = false,
                },
                () = sleep_until_some(frame_at) => {}
                () = tokio::time::sleep_until(deadline) => return Ok(done(self.observer.model())),
            }
            self.observer.draw_if_due();
        }
    }
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
            peak_rss_mb: 11.2,
            hook_to_screen_ms: Percentiles {
                p50: 12.0,
                p95: 40.0,
                max: 61.0,
            },
        };
        let expected = "\
duohive bench · 10 agents · tmux 3.7c · macos-aarch64
idle CPU        0.4 %    target < 1 %
memory (peak)   11.2 MB  target < 20 MB
hook → screen   p50 12 ms · p95 40 ms · max 61 ms   target < 1000 ms
";
        assert_eq!(report.to_string(), expected);
    }

    fn report(p: Percentiles) -> BenchReport {
        BenchReport {
            agents: 10,
            tmux: "3.7c".to_owned(),
            platform: "macos-aarch64".to_owned(),
            idle_cpu_pct: 0.4,
            peak_rss_mb: 11.2,
            hook_to_screen_ms: p,
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
            hook_samples: 4,
            pid: 1,
        };
        let text = outcome.to_string();
        assert!(text.contains("tmux server"), "{text}");
        assert!(text.contains("not DuoHive"), "{text}");
        assert!(text.contains("not measured"), "{text}");
        assert!(text.contains("10-agent targets"), "{text}");
        assert_eq!(outcome.to_json()["tmux_server"]["rss_mb"], 5.0);
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
            hook_samples: 0,
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
            hook_to_screen_ms: Percentiles {
                p50: 12.0,
                p95: 1500.0,
                max: 1600.0,
            },
        };
        let text = report.to_string();
        assert_eq!(text.matches("MISSED").count(), 3, "{text}");
    }
}
