//! `duohive bench` end to end: the built binary, a small run, its tmux server gone.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly on setup errors"
)]

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_duohive");

#[test]
fn bench_runs_two_agents() {
    let out = Command::new(BIN)
        .args([
            "bench",
            "--agents",
            "2",
            "--idle-secs",
            "2",
            "--turns",
            "1",
            "--json",
        ])
        .env_remove("TMUX")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{:?}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["agents"], 2, "{json}");
    assert!(json["idle_cpu_pct"].as_f64().unwrap() >= 0.0, "{json}");
    assert!(json["peak_rss_mb"].as_f64().unwrap() > 0.0, "{json}");
    assert!(
        json["hook_to_screen_ms"]["p50"].as_f64().unwrap() > 0.0,
        "{json}"
    );
    assert!(json["machine"]["cpus"].as_u64().unwrap() > 0, "{json}");

    // The bench's own tmux server is gone, and no socket is left for its pid.
    let list = Command::new("tmux")
        .args(["-f", "/dev/null", "-L"])
        .arg(format!("duohive-bench-{}", json["pid"].as_u64().unwrap()))
        .arg("list-sessions")
        .env_remove("TMUX")
        .output()
        .unwrap();
    assert!(!list.status.success(), "the bench's tmux server still runs");
}
