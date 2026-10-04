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

    assert!(
        json["tmux_server"]["rss_mb"].as_f64().unwrap() > 0.0,
        "{json}"
    );
    assert_eq!(json["control_client"], true, "{json}");

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

fn bench_socket_answers(pid: u32) -> bool {
    Command::new("tmux")
        .args([
            "-f",
            "/dev/null",
            "-L",
            &format!("duohive-bench-{pid}"),
            "list-sessions",
        ])
        .env_remove("TMUX")
        .output()
        .unwrap()
        .status
        .success()
}

#[test]
fn sigterm_cleans_up_the_bench() {
    use std::time::{Duration, Instant};
    let mut child = Command::new(BIN)
        .args([
            "bench",
            "--agents",
            "2",
            "--idle-secs",
            "60",
            "--turns",
            "1",
        ])
        .env_remove("TMUX")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !bench_socket_answers(pid) {
        assert!(Instant::now() < deadline, "the bench never started tmux");
        std::thread::sleep(Duration::from_millis(100));
    }
    let sent = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .unwrap();
    assert!(sent.success());
    let status = child.wait().unwrap();
    assert!(!status.success(), "an interrupted bench must not exit 0");
    assert!(!bench_socket_answers(pid), "tmux server left behind");
}
