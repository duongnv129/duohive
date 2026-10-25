//! The release gate (TD §10, P5-D1/P5-D2): three counted runs with the live
//! view at 10 and at 100 agents, the worst value per target deciding, then one
//! informational run of each without the view. A run the bench discards for
//! machine load is retried twice. This is the one implementation of what
//! `scripts/bench-gate.sh` used to do.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

use super::report::{BenchReport, Percentiles, targets_pass};
use super::{IGNORE_LOAD_VAR, LOAD_CEILING, measure};

/// The agent counts the gate measures.
pub const GATE_TIERS: [u32; 2] = [10, 100];
/// Counted runs per tier; the worst value per target decides.
pub const COUNTED_RUNS: usize = 3;
/// How many times a run discarded for load is tried again.
pub const LOAD_RETRIES: usize = 2;
/// The gate waits for the 1-minute load average to fall to this before it starts
/// a run: below the bench's own ceiling, so the load the run itself adds while it
/// starts (and a `cargo build` just before it) does not push it over.
pub const SETTLE_TARGET: f64 = LOAD_CEILING * 0.85;
/// How long the gate waits for the machine to quiet before one run; after that it
/// starts the run anyway and the bench discards it if the machine is still busy.
pub const SETTLE_MAX: Duration = Duration::from_secs(10 * 60);
/// How often the load is read while waiting.
pub const SETTLE_POLL: Duration = Duration::from_secs(5);
/// The binary-size limit (PRD Success Metrics; MB is 10,000,000 bytes).
pub const MAX_BINARY_BYTES: u64 = 10_000_000;

/// What one bench run came back as.
#[derive(Clone, Debug, PartialEq)]
pub enum RunResult {
    /// A report.
    Report(BenchReport),
    /// The bench discarded the run for machine load (exit 3).
    Discarded,
    /// The run was interrupted (exit 130).
    Interrupted,
    /// Anything else, with what the bench said.
    Failed(String),
}

/// Why the gate ended without a verdict.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum GateError {
    /// A counted run was discarded for load three times.
    #[error("agents {agents}: discarded for load {tries} times")]
    Discarded {
        /// The tier.
        agents: u32,
        /// How many tries were made.
        tries: usize,
    },
    /// A child bench exited 130 (Ctrl-C or SIGTERM reached it).
    #[error("interrupted")]
    Interrupted,
    /// A run failed for another reason.
    #[error("agents {agents}: {why}")]
    Failed {
        /// The tier.
        agents: u32,
        /// What the bench said.
        why: String,
    },
}

impl GateError {
    /// The exit code: 3 discarded for load, 130 interrupted, else 1.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Discarded { .. } => 3,
            Self::Interrupted => 130,
            Self::Failed { .. } => 1,
        }
    }
}

/// One agent count: its counted runs, the worst of them, and the run
/// without the view that is reported but never judged.
#[derive(Clone, Debug, PartialEq)]
pub struct TierReport {
    /// The agent count.
    pub agents: u32,
    /// The counted runs, with the view.
    pub runs: Vec<BenchReport>,
    /// The worst value per target over `runs`.
    pub worst: BenchReport,
    /// One run without the view, when it completed.
    pub without_view: Option<BenchReport>,
}

/// The gate's result.
#[derive(Clone, Debug, PartialEq)]
pub struct GateReport {
    /// 10 and 100 agents.
    pub tiers: Vec<TierReport>,
    /// The package version.
    pub version: String,
    /// `git rev-parse HEAD`, when the gate ran in a checkout.
    pub git_sha: Option<String>,
    /// The size of the executable that ran the gate, in bytes.
    pub binary_bytes: Option<u64>,
    /// Whether the checkout had uncommitted changes (`None` outside a checkout):
    /// a dirty gate run does not vouch for the committed code.
    pub git_dirty: Option<bool>,
    /// Why the gate stopped before every tier was measured, when it did: the tiers that
    /// finished are still here, and the run is never a pass.
    pub error: Option<GateError>,
}

impl GateReport {
    /// `<os>-<arch>` of the first tier's worst run.
    pub fn platform(&self) -> &str {
        self.tiers.first().map_or("unknown", |t| &t.worst.platform)
    }

    /// Whether this platform can fail the gate: Linux numbers are reported,
    /// never judged (TD §10).
    pub fn gating(&self) -> bool {
        !self.platform().starts_with("linux")
    }

    /// Whether every target passed in every tier's worst run.
    pub fn all_pass(&self) -> bool {
        self.error.is_none()
            && !self.tiers.is_empty()
            && self.tiers.iter().all(|t| targets_pass(&t.worst))
    }

    /// Whether the binary is under the size limit; an unknown size is a miss.
    pub fn size_ok(&self) -> bool {
        self.binary_bytes.is_some_and(|b| b < MAX_BINARY_BYTES)
    }

    /// 0 when the gate passes, or fails on a platform that does not gate; else 1.
    pub fn exit_code(&self) -> u8 {
        if let Some(e) = &self.error {
            return e.exit_code();
        }
        u8::from(self.gating() && !(self.all_pass() && self.size_ok()))
    }

    /// The result as JSON (`--json`, and the file the release check reads).
    pub fn to_json(&self) -> Value {
        json!({
            "all_pass": self.all_pass() && self.size_ok(),
            "gating": self.gating(),
            "version": self.version,
            "git_sha": self.git_sha,
            "git_dirty": self.git_dirty,
            "error": self.error.as_ref().map(ToString::to_string),
            "platform": self.platform(),
            "profile": "release",
            "binary_bytes": self.binary_bytes,
            "tiers": self.tiers.iter().map(|t| json!({
                "agents": t.agents,
                "all_pass": targets_pass(&t.worst),
                "worst": t.worst.to_json(),
                "runs": t.runs.iter().map(BenchReport::to_json).collect::<Vec<_>>(),
                "without_view": t.without_view.as_ref().map(BenchReport::to_json),
            })).collect::<Vec<_>>(),
        })
    }

    /// The report as text: each tier's worst run, then the verdict.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for t in &self.tiers {
            out.push_str(&format!(
                "worst of {} runs with the view, {} agents\n{}\n",
                t.runs.len(),
                t.agents,
                t.worst
            ));
            if let Some(w) = &t.without_view {
                out.push_str(&format!(
                    "without the view (information only): idle CPU {:.1} %, memory {:.1} MB\n\n",
                    w.idle_cpu_pct, w.rss_end_mb
                ));
            }
        }
        let size = self
            .binary_bytes
            .map_or_else(|| "unknown".to_owned(), |b| format!("{b} B"));
        out.push_str(&format!(
            "binary {size} (limit {MAX_BINARY_BYTES} B){}\n",
            if self.size_ok() { "" } else { "  MISSED" }
        ));
        if let Some(e) = &self.error {
            out.push_str(&format!("gate stopped early: {e}\n"));
        }
        let verdict = match (self.all_pass() && self.size_ok(), self.gating()) {
            _ if self.error.is_some() => "gate: INCOMPLETE",
            (true, _) => "gate: PASS",
            (false, true) => "gate: FAIL",
            (false, false) => "gate: reported only (this platform does not gate)",
        };
        out.push_str(verdict);
        out.push('\n');
        out
    }
}

/// The worst value per target over `runs`: the largest of each, and `None`
/// for the first frame when any run lacks its timing. `None` for no runs.
pub fn worst_of(runs: &[BenchReport]) -> Option<BenchReport> {
    let first = runs.first()?;
    let max = |f: fn(&BenchReport) -> f64| runs.iter().map(f).fold(f64::MIN, f64::max);
    let pcts = |f: fn(&BenchReport) -> Percentiles| Percentiles {
        p50: runs.iter().map(|r| f(r).p50).fold(f64::MIN, f64::max),
        p95: runs.iter().map(|r| f(r).p95).fold(f64::MIN, f64::max),
        max: runs.iter().map(|r| f(r).max).fold(f64::MIN, f64::max),
    };
    let first_frame = runs
        .iter()
        .map(|r| r.first_frame_ms)
        .try_fold(f64::MIN, |acc, ms| ms.map(|m| acc.max(m)));
    Some(BenchReport {
        agents: first.agents,
        tmux: first.tmux.clone(),
        platform: first.platform.clone(),
        idle_cpu_pct: max(|r| r.idle_cpu_pct),
        peak_rss_mb: max(|r| r.peak_rss_mb),
        rss_end_mb: max(|r| r.rss_end_mb),
        hook_to_screen_ms: pcts(|r| r.hook_to_screen_ms),
        tilde_ms: pcts(|r| r.tilde_ms),
        first_frame_ms: first_frame,
    })
}

/// One run at `agents`, retried while the bench discards it for load.
fn run_with_retries(
    runner: &mut dyn FnMut(u32, bool) -> RunResult,
    agents: u32,
    view: bool,
) -> Result<BenchReport, GateError> {
    let mut tries = 0;
    loop {
        tries += 1;
        match runner(agents, view) {
            RunResult::Report(r) => return Ok(r),
            RunResult::Interrupted => return Err(GateError::Interrupted),
            RunResult::Failed(why) => return Err(GateError::Failed { agents, why }),
            RunResult::Discarded if tries > LOAD_RETRIES => {
                return Err(GateError::Discarded { agents, tries });
            }
            RunResult::Discarded => {}
        }
    }
}

/// What the gate got through: the tiers it finished, and why it stopped if it did.
#[derive(Debug, PartialEq)]
pub struct GateRun {
    /// The finished tiers, in order.
    pub tiers: Vec<TierReport>,
    /// The error that ended the gate early.
    pub error: Option<GateError>,
}

/// One tier: its counted runs, then the run without the view.
fn run_tier(
    runner: &mut dyn FnMut(u32, bool) -> RunResult,
    agents: u32,
) -> Result<TierReport, GateError> {
    let mut runs = Vec::new();
    for _ in 0..COUNTED_RUNS {
        runs.push(run_with_retries(runner, agents, true)?);
    }
    let worst = worst_of(&runs).ok_or_else(|| GateError::Failed {
        agents,
        why: "no counted run".to_owned(),
    })?;
    // Information only: a discard or failure here is not the gate's.
    let without_view = match run_with_retries(runner, agents, false) {
        Ok(r) => Some(r),
        Err(GateError::Interrupted) => return Err(GateError::Interrupted),
        Err(_) => None,
    };
    Ok(TierReport {
        agents,
        runs,
        worst,
        without_view,
    })
}

/// Runs the gate through `runner(agents, view)`. A tier that fails ends the gate, but the
/// tiers measured before it are kept: a late failure must not throw away finished work.
pub fn run_gate(runner: &mut dyn FnMut(u32, bool) -> RunResult) -> GateRun {
    let mut tiers = Vec::new();
    for agents in GATE_TIERS {
        match run_tier(runner, agents) {
            Ok(tier) => tiers.push(tier),
            Err(error) => {
                return GateRun {
                    tiers,
                    error: Some(error),
                };
            }
        }
    }
    GateRun { tiers, error: None }
}

/// Waits until the load is at most `target`, reading it every `poll` for up to
/// `max_wait`. Returns whether it got there; an unreadable load counts as quiet.
/// The reading and the pause are injected so the wait can be tested.
pub fn settle(
    read_load: &mut dyn FnMut() -> Option<f64>,
    pause: &mut dyn FnMut(Duration),
    target: f64,
    max_wait: Duration,
    poll: Duration,
) -> bool {
    let polls = (max_wait.as_secs() / poll.as_secs().max(1)).max(1);
    for _ in 0..polls {
        match read_load() {
            None => return true,
            Some(l) if l <= target => return true,
            Some(_) => pause(poll),
        }
    }
    read_load().is_none_or(|l| l <= target)
}

/// The real wait before a run: skipped when the bench's load check is off, and
/// said on stderr when it has to wait.
fn settle_before_run() {
    if std::env::var_os(IGNORE_LOAD_VAR).is_some() {
        return;
    }
    let mut first = true;
    let mut read = || measure::load_average().map(|l| l[0]);
    let mut pause = |d: Duration| {
        if first {
            first = false;
            let load = measure::load_average().map_or(0.0, |l| l[0]);
            let _ = writeln!(
                std::io::stderr(),
                "duohive bench --gate: waiting for the machine to quiet (load {load:.2}, need {SETTLE_TARGET:.2})"
            );
        }
        std::thread::sleep(d);
    };
    settle(
        &mut read,
        &mut pause,
        SETTLE_TARGET,
        SETTLE_MAX,
        SETTLE_POLL,
    );
}

/// Runs one bench as a child process of `exe` (every run is a fresh process:
/// the bench installs its own signal handlers and temp state).
pub fn run_child(exe: &Path, agents: u32, view: bool) -> RunResult {
    settle_before_run();
    let mut cmd = Command::new(exe);
    cmd.args(["bench", "--agents", &agents.to_string()]);
    if view {
        cmd.arg("--view");
    }
    cmd.arg("--json").stdin(Stdio::null());
    let out = match cmd.output() {
        Ok(o) => o,
        Err(e) => return RunResult::Failed(format!("cannot run {}: {e}", exe.display())),
    };
    match out.status.code() {
        Some(0) => match serde_json::from_slice::<Value>(&out.stdout)
            .map_err(|e| e.to_string())
            .and_then(|v| BenchReport::from_json(&v))
        {
            Ok(r) => RunResult::Report(r),
            Err(e) => RunResult::Failed(format!("unreadable bench output: {e}")),
        },
        Some(3) => RunResult::Discarded,
        Some(130) => RunResult::Interrupted,
        code => RunResult::Failed(format!(
            "bench exited {}: {}",
            code.map_or_else(|| "by signal".to_owned(), |c| c.to_string()),
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// `git rev-parse HEAD` in the current directory, when it answers.
pub fn git_head() -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// Whether the checkout has uncommitted changes, when `git` answers.
pub fn git_dirty() -> Option<bool> {
    let out = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()?;
    out.status.success().then_some(!out.stdout.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcts(max: f64) -> Percentiles {
        Percentiles {
            p50: max / 3.0,
            p95: max / 2.0,
            max,
        }
    }

    /// A run that meets every target at `agents`.
    fn good(agents: u32) -> BenchReport {
        BenchReport {
            agents,
            tmux: "3.7c".to_owned(),
            platform: "macos-aarch64".to_owned(),
            idle_cpu_pct: 0.4,
            peak_rss_mb: 12.0,
            rss_end_mb: 11.0,
            hook_to_screen_ms: pcts(60.0),
            tilde_ms: pcts(80.0),
            first_frame_ms: Some(50.0),
        }
    }

    fn gate_of(worst10: BenchReport, platform: &str, bytes: Option<u64>) -> GateReport {
        let mut worst100 = good(100);
        worst100.platform = platform.to_owned();
        let mut w10 = worst10;
        w10.platform = platform.to_owned();
        GateReport {
            tiers: vec![
                TierReport {
                    agents: 10,
                    runs: vec![w10.clone()],
                    worst: w10,
                    without_view: None,
                },
                TierReport {
                    agents: 100,
                    runs: vec![worst100.clone()],
                    worst: worst100,
                    without_view: None,
                },
            ],
            version: "1.0.0".to_owned(),
            git_sha: Some("abc123".to_owned()),
            binary_bytes: bytes,
            git_dirty: Some(false),
            error: None,
        }
    }

    #[test]
    fn worst_of_takes_the_largest_value_per_target() {
        let mut a = good(10);
        a.idle_cpu_pct = 0.9;
        a.first_frame_ms = Some(70.0);
        let mut b = good(10);
        b.rss_end_mb = 19.0;
        b.tilde_ms = pcts(1900.0);
        let w = worst_of(&[a, b]).unwrap_or_else(|| good(0));
        assert!((w.idle_cpu_pct - 0.9).abs() < 1e-9);
        assert!((w.rss_end_mb - 19.0).abs() < 1e-9);
        assert!((w.tilde_ms.p95 - 950.0).abs() < 1e-9);
        assert_eq!(w.first_frame_ms, Some(70.0));
    }

    /// A load source that answers from `loads`, then repeats the last one.
    fn loads(loads: &[f64]) -> impl FnMut() -> Option<f64> + '_ {
        let mut i = 0;
        move || {
            let l = loads.get(i).or(loads.last()).copied();
            i += 1;
            l
        }
    }

    #[test]
    fn settle_returns_at_once_when_quiet() {
        let mut pauses = 0;
        let ok = settle(
            &mut loads(&[1.0]),
            &mut |_| pauses += 1,
            1.7,
            Duration::from_secs(60),
            Duration::from_secs(5),
        );
        assert!(ok);
        assert_eq!(pauses, 0);
    }

    #[test]
    fn settle_waits_until_load_drops() {
        let mut pauses = 0;
        let ok = settle(
            &mut loads(&[3.0, 2.5, 1.6]),
            &mut |_| pauses += 1,
            1.7,
            Duration::from_secs(60),
            Duration::from_secs(5),
        );
        assert!(ok);
        assert_eq!(pauses, 2);
    }

    #[test]
    fn settle_gives_up_after_the_max_wait() {
        let mut pauses = 0;
        let ok = settle(
            &mut loads(&[3.0]),
            &mut |_| pauses += 1,
            1.7,
            Duration::from_secs(20),
            Duration::from_secs(5),
        );
        assert!(!ok);
        assert_eq!(pauses, 4, "one pause per poll in the window");
    }

    #[test]
    fn settle_treats_an_unreadable_load_as_quiet() {
        let mut none = || None;
        assert!(settle(
            &mut none,
            &mut |_| {},
            1.7,
            Duration::from_secs(20),
            Duration::from_secs(5)
        ));
    }

    #[test]
    fn worst_of_no_runs_is_none() {
        assert_eq!(worst_of(&[]), None);
    }

    #[test]
    fn gate_fails_on_each_missed_target() {
        type Spoil = fn(&mut BenchReport);
        let spoilers: [(&str, Spoil); 5] = [
            ("cpu", |r| r.idle_cpu_pct = 1.5),
            ("memory", |r| r.rss_end_mb = 25.0),
            ("hook", |r| r.hook_to_screen_ms = pcts(1500.0)),
            ("tilde", |r| r.tilde_ms = pcts(5000.0)),
            ("first frame", |r| r.first_frame_ms = Some(500.0)),
        ];
        for (name, spoil) in spoilers {
            let mut bad = good(10);
            spoil(&mut bad);
            let g = gate_of(bad, "macos-aarch64", Some(6_000_000));
            assert!(!g.all_pass(), "{name} should fail the gate");
            assert_eq!(g.exit_code(), 1, "{name}");
        }
    }

    #[test]
    fn gate_passes_at_target() {
        let g = gate_of(good(10), "macos-aarch64", Some(6_000_000));
        assert!(g.all_pass() && g.size_ok());
        assert_eq!(g.exit_code(), 0);
        assert!(g.text().contains("gate: PASS"));
    }

    #[test]
    fn gate_fails_when_first_frame_missing() {
        let mut r = good(10);
        r.first_frame_ms = None;
        let g = gate_of(r, "macos-aarch64", Some(6_000_000));
        assert_eq!(g.exit_code(), 1);
    }

    #[test]
    fn gate_fails_on_an_oversize_or_unknown_binary() {
        let big = gate_of(good(10), "macos-aarch64", Some(10_000_000));
        assert_eq!(big.exit_code(), 1);
        let unknown = gate_of(good(10), "macos-aarch64", None);
        assert_eq!(unknown.exit_code(), 1);
    }

    #[test]
    fn linux_platform_reports_but_never_gates() {
        let mut bad = good(10);
        bad.idle_cpu_pct = 3.0;
        let g = gate_of(bad, "linux-x86_64", Some(6_000_000));
        assert!(!g.all_pass());
        assert!(!g.gating());
        assert_eq!(g.exit_code(), 0);
        assert!(g.text().contains("reported only"));
    }

    #[test]
    fn gate_json_has_version_platform_git_sha_and_binary_bytes() {
        let g = gate_of(good(10), "macos-aarch64", Some(6_715_312));
        let v = g.to_json();
        assert_eq!(v["all_pass"], true);
        assert_eq!(v["version"], "1.0.0");
        assert_eq!(v["platform"], "macos-aarch64");
        assert_eq!(v["git_sha"], "abc123");
        assert_eq!(v["git_dirty"], false);
        assert_eq!(v["binary_bytes"], 6_715_312);
        assert_eq!(v["profile"], "release");
        assert_eq!(v["tiers"].as_array().map(Vec::len), Some(2));
    }

    /// A runner that answers from a script (then good reports) and records
    /// every `(agents, view)` it is called with.
    fn scripted(
        replies: Vec<RunResult>,
        log: &mut Vec<(u32, bool)>,
    ) -> impl FnMut(u32, bool) -> RunResult + '_ {
        let mut replies = replies.into_iter();
        move |agents, view| {
            log.push((agents, view));
            replies
                .next()
                .unwrap_or_else(|| RunResult::Report(good(agents)))
        }
    }

    #[test]
    fn counted_runs_use_the_view() {
        let mut log = Vec::new();
        let run = run_gate(&mut scripted(Vec::new(), &mut log));
        assert_eq!((run.tiers.len(), run.error), (2, None));
        let expect: Vec<(u32, bool)> = [10, 100]
            .into_iter()
            .flat_map(|n| [(n, true), (n, true), (n, true), (n, false)])
            .collect();
        assert_eq!(log, expect);
    }

    #[test]
    fn discarded_run_is_retried_twice() {
        let mut log = Vec::new();
        let replies = vec![RunResult::Discarded, RunResult::Discarded];
        let run = run_gate(&mut scripted(replies, &mut log));
        assert_eq!(run.error, None);
        // Two discards, then the third try of the first counted run succeeds.
        assert_eq!(log.iter().take(3).collect::<Vec<_>>(), [&(10, true); 3]);
    }

    #[test]
    fn three_discards_end_with_exit_three() {
        let mut log = Vec::new();
        let replies = vec![RunResult::Discarded; 3];
        let err = run_gate(&mut scripted(replies, &mut log)).error;
        assert_eq!(
            err,
            Some(GateError::Discarded {
                agents: 10,
                tries: 3
            })
        );
        assert_eq!(err.map(|e| e.exit_code()), Some(3));
    }

    #[test]
    fn other_failure_ends_with_exit_one_and_its_stderr() {
        let mut log = Vec::new();
        let replies = vec![RunResult::Failed("bench exited 1: tmux missing".to_owned())];
        let err = run_gate(&mut scripted(replies, &mut log)).error;
        let msg = err.as_ref().map(ToString::to_string).unwrap_or_default();
        assert!(msg.contains("tmux missing"), "{msg}");
        assert_eq!(err.map(|e| e.exit_code()), Some(1));
    }

    #[test]
    fn interrupted_run_ends_with_exit_130() {
        let mut log = Vec::new();
        let err = run_gate(&mut scripted(vec![RunResult::Interrupted], &mut log)).error;
        assert_eq!(err, Some(GateError::Interrupted));
        assert_eq!(err.map(|e| e.exit_code()), Some(130));
    }

    #[test]
    fn the_run_without_the_view_is_never_judged() {
        let mut bad = good(10);
        bad.idle_cpu_pct = 9.0;
        let mut log = Vec::new();
        let replies = vec![
            RunResult::Report(good(10)),
            RunResult::Report(good(10)),
            RunResult::Report(good(10)),
            RunResult::Report(bad),
        ];
        let tiers = run_gate(&mut scripted(replies, &mut log)).tiers;
        let first = tiers.first();
        assert!(first.is_some_and(|t| targets_pass(&t.worst)));
        assert!(
            first.is_some_and(|t| t.runs.len() == 3),
            "the view-less run is not counted"
        );
        let seen = first
            .and_then(|t| t.without_view.as_ref())
            .map(|r| r.idle_cpu_pct);
        assert_eq!(seen, Some(9.0), "the view-less run is reported");
        let g = GateReport {
            tiers,
            version: "1.0.0".to_owned(),
            git_sha: None,
            binary_bytes: Some(6_000_000),
            git_dirty: None,
            error: None,
        };
        assert_eq!(g.exit_code(), 0);
    }

    /// 3 good counted runs and the view-less run for the first tier, then a failure in the second.
    fn fails_at_100() -> (GateRun, Vec<(u32, bool)>) {
        let mut log = Vec::new();
        let mut replies: Vec<RunResult> = (0..4).map(|_| RunResult::Report(good(10))).collect();
        replies.push(RunResult::Failed(
            "bench exited 1: timed out waiting for every agent to finish a turn".to_owned(),
        ));
        let run = run_gate(&mut scripted(replies, &mut log));
        (run, log)
    }

    #[test]
    fn a_failure_in_the_second_tier_keeps_the_first() {
        let (run, _) = fails_at_100();
        assert_eq!(run.tiers.len(), 1, "the 10-agent tier is kept");
        assert_eq!(run.tiers[0].agents, 10);
        let msg = run
            .error
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        assert!(
            msg.contains("agents 100") && msg.contains("finish a turn"),
            "{msg}"
        );
    }

    #[test]
    fn an_incomplete_gate_is_never_a_pass_and_says_so() {
        let (run, _) = fails_at_100();
        let g = GateReport {
            tiers: run.tiers,
            version: "1.0.0".to_owned(),
            git_sha: None,
            binary_bytes: Some(6_000_000),
            git_dirty: None,
            error: run.error,
        };
        assert!(!g.all_pass());
        assert_eq!(g.exit_code(), 1);
        let v = g.to_json();
        assert_eq!(v["all_pass"], false);
        assert_eq!(v["tiers"].as_array().map(Vec::len), Some(1));
        assert!(
            v["error"]
                .as_str()
                .is_some_and(|e| e.contains("agents 100")),
            "{v}"
        );
        let text = g.text();
        assert!(
            text.contains("gate stopped early") && text.contains("INCOMPLETE"),
            "{text}"
        );
        assert!(!text.contains("gate: PASS"));
    }

    #[test]
    fn worst_of_loses_the_first_frame_when_any_run_lacks_it() {
        let mut missing = good(10);
        missing.first_frame_ms = None;
        let w = worst_of(&[good(10), missing, good(10)]);
        assert_eq!(w.map(|r| r.first_frame_ms), Some(None));
    }
}
