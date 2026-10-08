//! The bench's numbers and how they print (PRD Success Metrics).

use std::fmt;

use serde_json::{Value, json};

use super::LiveViewUse;

/// p50, p95 and max of a set of samples.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Percentiles {
    /// The median.
    pub p50: f64,
    /// The 95th percentile.
    pub p95: f64,
    /// The largest sample.
    pub max: f64,
}

/// What one bench run measured.
#[derive(Clone, Debug, PartialEq)]
pub struct BenchReport {
    /// How many agents ran.
    pub agents: u32,
    /// The tmux version, without the `tmux ` prefix.
    pub tmux: String,
    /// `<os>-<arch>`.
    pub platform: String,
    /// DuoHive's CPU while every agent is at rest, percent of one core: this
    /// process plus its `tmux -C` control client, never the tmux server or agents.
    pub idle_cpu_pct: f64,
    /// DuoHive's peak resident memory in MB: this process's peak plus the
    /// control client's largest sampled size.
    pub peak_rss_mb: f64,
    /// From a hook's `sent_at_ms` (stamped when `duohive hook` has started, so
    /// the hook process's own start-up is not included) to the end of the frame
    /// that shows its state, in ms.
    pub hook_to_screen_ms: Percentiles,
}

/// The PRD targets for a run's agent count: at 10 agents or fewer the 10-agent
/// targets, above that the 100-agent ones.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Targets {
    /// Idle CPU, percent.
    pub cpu_pct: f64,
    /// Memory, MB.
    pub rss_mb: f64,
    /// Hook to screen, ms.
    pub hook_ms: f64,
}

impl Targets {
    /// The targets that apply at `agents`.
    pub fn for_agents(agents: u32) -> Self {
        if agents <= 10 {
            Self {
                cpu_pct: 1.0,
                rss_mb: 20.0,
                hook_ms: 1000.0,
            }
        } else {
            Self {
                cpu_pct: 3.0,
                rss_mb: 60.0,
                hook_ms: 2000.0,
            }
        }
    }
}

/// How loaded the machine was, so a number can be read in context.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Machine {
    /// Logical CPUs.
    pub cpus: usize,
    /// 1, 5 and 15 minute load averages when the bench began.
    pub load_start: Option<[f64; 3]>,
    /// The same, when the idle measurement ended.
    pub load_end: Option<[f64; 3]>,
}

/// DuoHive's own tmux server: reported, never judged (it is not DuoHive's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ServerUse {
    /// CPU over the idle window, percent of one core.
    pub cpu_pct: f64,
    /// Largest resident size sampled, MB.
    pub rss_mb: f64,
}

/// A run: the report plus the context around it.
#[derive(Clone, Debug, PartialEq)]
pub struct BenchOutcome {
    /// The measurements.
    pub report: BenchReport,
    /// The machine they were taken on.
    pub machine: Machine,
    /// The tmux server on the bench's socket, when it could be sampled.
    pub tmux_server: Option<ServerUse>,
    /// Whether the control client's CPU and memory are inside the report's figures.
    pub control_client: bool,
    /// `--view`: the live stream held open over the run.
    pub live_view: Option<LiveViewUse>,
    /// How many hooks were timed (the percentiles rest on these).
    pub hook_samples: usize,
    /// The bench's pid (its tmux socket is `duohive-bench-<pid>`).
    pub pid: u32,
}

const LABEL_WIDTH: usize = 16;
const VALUE_WIDTH: usize = 9;

/// The two-space `MISSED` marker when `over`: a missed target is never left unsaid.
fn verdict(over: bool) -> &'static str {
    if over { "  MISSED" } else { "" }
}

/// One decimal place, as the report prints CPU and memory.
fn one(v: f64) -> String {
    format!("{v:.1}")
}

impl fmt::Display for BenchReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let t = Targets::for_agents(self.agents);
        let h = &self.hook_to_screen_ms;
        writeln!(
            f,
            "duohive bench · {} agents · tmux {} · {}",
            self.agents, self.tmux, self.platform
        )?;
        let cpu = format!("{} %", one(self.idle_cpu_pct));
        let mem = format!("{} MB", one(self.peak_rss_mb));
        writeln!(
            f,
            "{:<LABEL_WIDTH$}{cpu:<VALUE_WIDTH$}target < {} %{}",
            "idle CPU",
            t.cpu_pct,
            verdict(self.idle_cpu_pct >= t.cpu_pct)
        )?;
        writeln!(
            f,
            "{:<LABEL_WIDTH$}{mem:<VALUE_WIDTH$}target < {} MB{}",
            "memory (peak)",
            t.rss_mb,
            verdict(self.peak_rss_mb >= t.rss_mb)
        )?;
        if h.max <= 0.0 {
            // No hook was timed: that is a failure to measure, not a pass.
            return writeln!(
                f,
                "{:<LABEL_WIDTH$}no samples   target < {} ms{}",
                "hook → screen",
                t.hook_ms,
                verdict(true)
            );
        }
        writeln!(
            f,
            "{:<LABEL_WIDTH$}p50 {:.0} ms · p95 {:.0} ms · max {:.0} ms   target < {} ms{}",
            "hook → screen",
            h.p50,
            h.p95,
            h.max,
            t.hook_ms,
            verdict(h.max >= t.hook_ms)
        )
    }
}

fn loads(l: Option<[f64; 3]>) -> String {
    l.map_or_else(
        || "unknown".to_owned(),
        |[a, b, c]| format!("{a:.2} {b:.2} {c:.2}"),
    )
}

impl fmt::Display for BenchOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.report)?;
        let set = if self.report.agents <= 10 { 10 } else { 100 };
        let viewing = self.live_view.as_ref().is_some_and(|v| v.client_counted);
        let client = match (self.control_client, viewing) {
            (true, true) => "this process + its tmux control and view clients",
            (true, false) => "this process + its tmux control client",
            (false, _) => "this process only (control client not found)",
        };
        writeln!(f, "{:<LABEL_WIDTH$}DuoHive = {client}", "counted")?;
        if self.report.platform.starts_with("linux") {
            writeln!(
                f,
                "{:<LABEL_WIDTH$}Linux ps counts CPU in whole seconds: tmux processes' CPU is coarse",
                "note"
            )?;
        }
        if let Some(v) = &self.live_view {
            writeln!(f, "{:<LABEL_WIDTH$}{}", "live view", describe_view(v))?;
        }
        if let Some(s) = &self.tmux_server {
            writeln!(
                f,
                "{:<LABEL_WIDTH$}{:.1} % CPU · {:.1} MB (not DuoHive: no target)",
                "tmux server", s.cpu_pct, s.rss_mb
            )?;
        }
        writeln!(
            f,
            "{:<LABEL_WIDTH$}{} timed hooks · {set}-agent targets apply",
            "samples", self.hook_samples
        )?;
        writeln!(
            f,
            "{:<LABEL_WIDTH$}screen-read (~) freshness (< 2 s), start-up first frame (< 100 ms at 10)",
            "not measured"
        )?;
        writeln!(
            f,
            "{:<LABEL_WIDTH$}{} cpus · load (1/5/15 min) {} at start, {} at the end",
            "machine",
            self.machine.cpus,
            loads(self.machine.load_start),
            loads(self.machine.load_end)
        )
    }
}

/// One line on the live stream: what it streamed, whether it stayed open, and
/// whether the other panes stayed silent on its client.
fn describe_view(v: &LiveViewUse) -> String {
    let open = match &v.closed {
        None => "open to the end".to_owned(),
        Some(reason) => format!("CLOSED ({reason})"),
    };
    let client = if v.client_counted {
        "its client counted"
    } else {
        "its client not found"
    };
    format!(
        "{} stream on {} · {open} · {:.1} KB in {} chunks · {} lines from other panes · {client}",
        v.streams_opened,
        v.agent,
        v.bytes as f64 / 1024.0,
        v.chunks,
        v.foreign_output_lines
    )
}

impl BenchOutcome {
    /// The run as JSON (`--json`).
    pub fn to_json(&self) -> Value {
        let r = &self.report;
        let h = &r.hook_to_screen_ms;
        json!({
            "agents": r.agents,
            "tmux": r.tmux,
            "platform": r.platform,
            "idle_cpu_pct": r.idle_cpu_pct,
            "peak_rss_mb": r.peak_rss_mb,
            "hook_to_screen_ms": { "p50": h.p50, "p95": h.p95, "max": h.max },
            "machine": {
                "cpus": self.machine.cpus,
                "load_start": self.machine.load_start,
                "load_end": self.machine.load_end,
            },
            "hook_samples": self.hook_samples,
            "control_client": self.control_client,
            "live_view": self.live_view.as_ref().map(|v| json!({
                "agent": v.agent,
                "streams_opened": v.streams_opened,
                "open_at_end": v.open_at_end,
                "closed": v.closed,
                "chunks": v.chunks,
                "bytes": v.bytes,
                "foreign_output_lines": v.foreign_output_lines,
                "client_counted": v.client_counted,
            })),
            "tmux_server": self.tmux_server.map(|s| json!({ "cpu_pct": s.cpu_pct, "rss_mb": s.rss_mb })),
            "pid": self.pid,
        })
    }
}
