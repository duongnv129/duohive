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
use duohive_core::status::AgentState;
use duohive_store::{Paths, save_org};
use duohive_tui::UiModel;
use tokio::sync::mpsc;

use crate::system::{StartError, SystemHandle, SystemOptions, start_system};
pub use measure::percentiles;
use observe::Observer;
pub use report::{BenchOutcome, BenchReport, Machine, Percentiles, Targets};

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
    let (tap_tx, tap) = mpsc::unbounded_channel();
    let handle = start_system(
        &env.paths,
        Arc::new(fake_adapters(&exe)),
        SystemOptions {
            tap: Some(tap_tx),
            first_frame: None,
        },
    )
    .await?;
    let observer = Observer::new(org, &env.paths.tmux_socket);
    let mut run = Run {
        handle,
        tap,
        observer,
        names,
    };
    let result = run.measure(&a).await;
    run.finish().await;
    env.kill_tmux();
    let (idle_cpu_pct, hook_to_screen_ms, hook_samples) = result?;
    Ok(BenchOutcome {
        report: BenchReport {
            agents: a.agents,
            tmux,
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            idle_cpu_pct,
            peak_rss_mb: measure::peak_rss_mb(),
            hook_to_screen_ms,
        },
        machine: Machine {
            cpus: std::thread::available_parallelism().map_or(1, usize::from),
            load_start: machine_start,
            load_end: measure::load_average(),
        },
        hook_samples,
        pid: std::process::id(),
    })
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

/// `tmux -V` without its `tmux ` prefix.
fn tmux_version() -> Result<String, BenchError> {
    let out = Process::new("tmux")
        .args(["-f", "/dev/null", "-V"])
        .env_remove("TMUX")
        .output()
        .map_err(|e| BenchError::Tmux(e.to_string()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text.trim().trim_start_matches("tmux ").to_owned())
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

/// The system, its hook tap and the screen stand-in, driven together.
struct Run {
    handle: SystemHandle,
    tap: mpsc::UnboundedReceiver<HookMessage>,
    observer: Observer,
    names: Vec<AgentName>,
}

impl Run {
    /// Start, work, settle, then watch idle: returns idle CPU and the hook timings.
    async fn measure(&mut self, a: &BenchArgs) -> Result<(f64, Percentiles, usize), BenchError> {
        self.pump(Duration::from_secs(5), |m| !m.agents.is_empty())
            .await?
            .then_some(())
            .ok_or(BenchError::Timeout("the first snapshot"))?;
        for agent in self.names.clone() {
            self.send(Command::StartAgent { agent }).await?;
        }
        self.wait_all("every agent to start", START_WAIT, at_rest)
            .await?;
        for turn in 1..=a.turns {
            let task = format!("bench task {turn}");
            for agent in self.names.clone() {
                let task = task.clone();
                self.send(Command::GiveWork { agent, task }).await?;
            }
            let reply = format!("done: {task}");
            self.wait_all("every agent to finish a turn", TURN_WAIT, move |m, n| {
                at_rest(m, n) && m.view(n).last_reply.as_deref() == Some(reply.as_str())
            })
            .await?;
        }
        self.pump(SETTLE, |_| false).await?;
        let idle = Duration::from_secs(a.idle_secs);
        let cpu_before = measure::cpu_time();
        let started = Instant::now();
        self.pump(idle, |_| false).await?;
        let idle_cpu_pct = match (cpu_before, measure::cpu_time()) {
            (Some(before), Some(after)) => {
                measure::cpu_percent(after.saturating_sub(before), started.elapsed())
            }
            _ => 0.0,
        };
        let mut samples = self.observer.samples().to_vec();
        Ok((idle_cpu_pct, percentiles(&mut samples), samples.len()))
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
