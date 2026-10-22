//! `scripts/bench-gate.sh` against a stub `duohive`: the worst run decides, the
//! counted runs use `--view`, and a discarded run is retried twice.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly on setup errors"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/bench-gate.sh");

/// A bench `--json` fixture: every target passes unless `fail_memory` is set.
fn fixture(agents: u32, fail_memory: bool) -> String {
    let pass = !fail_memory;
    serde_json::json!({
        "agents": agents,
        "tmux": "3.7c",
        "platform": "macos-aarch64",
        "idle_cpu_pct": 0.4,
        "peak_rss_mb": 12.0,
        "rss_end_mb": 11.0,
        "hook_to_screen_ms": { "p50": 10.0, "p95": 20.0, "max": 30.0 },
        "tilde_ms": { "p50": 20.0, "p95": 40.0, "max": 60.0 },
        "first_frame_ms": 80.0,
        "targets": {
            "idle_cpu": { "value": 0.4, "target": 1.0, "pass": true },
            "memory": { "value": if fail_memory { 25.0 } else { 11.0 }, "target": 20.0, "pass": pass },
            "hook": { "value": 30.0, "target": 1000.0, "pass": true },
            "tilde": { "value": 40.0, "target": 2000.0, "pass": true },
            "first_frame": { "value": 80.0, "target": 100.0, "pass": true },
        },
        "all_pass": pass,
    })
    .to_string()
}

/// Writes a stub binary that answers the n-th call from `replies`, each a
/// `(exit code, output)`, and records every invocation's arguments.
fn stub(dir: &Path, replies: &[(i32, String)]) -> PathBuf {
    let path = dir.join("stub.sh");
    let log = dir.join("args.log");
    let counter = dir.join("counter");
    let mut body = String::from("#!/bin/sh\n");
    body.push_str(&format!("printf '%s\\n' \"$*\" >> '{}'\n", log.display()));
    body.push_str(&format!(
        "n=$(cat '{}' 2>/dev/null || echo 0)\n",
        counter.display()
    ));
    body.push_str("n=$((n + 1))\n");
    body.push_str(&format!("echo \"$n\" > '{}'\n", counter.display()));
    body.push_str("case $n in\n");
    for (i, (code, out)) in replies.iter().enumerate() {
        body.push_str(&format!(
            "  {}) printf '%s\\n' '{}'; exit {code} ;;\n",
            i + 1,
            out
        ));
    }
    body.push_str("  *) exit 0 ;;\nesac\n");
    std::fs::write(&path, body).unwrap();
    let chmod = Command::new("chmod").arg("+x").arg(&path).status().unwrap();
    assert!(chmod.success());
    path
}

fn gate(stub: &Path) -> std::process::Output {
    Command::new("sh")
        .arg(SCRIPT)
        .env("DUOHIVE_BIN", stub)
        .env_remove("TMUX")
        .output()
        .unwrap()
}

fn args_log(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("args.log"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn worst_of_three_decides() {
    let all_pass = vec![
        (0, fixture(10, false)),
        (0, fixture(10, false)),
        (0, fixture(10, false)),
        (0, fixture(10, false)),
        (0, fixture(100, false)),
        (0, fixture(100, false)),
        (0, fixture(100, false)),
        (0, fixture(100, false)),
    ];
    let dir = tempfile::tempdir().unwrap();
    let bin = stub(dir.path(), &all_pass);
    let out = gate(&bin);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );

    let mut worst = all_pass.clone();
    // The third counted run (the last at 10 agents) misses memory.
    worst[2] = (0, fixture(10, true));
    let dir = tempfile::tempdir().unwrap();
    let bin = stub(dir.path(), &worst);
    let out = gate(&bin);
    assert_ne!(out.status.code(), Some(0));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("memory"), "{text}");
    assert!(text.contains("FAIL"), "{text}");
}

#[test]
fn gate_runs_with_the_view_flag() {
    let replies: Vec<(i32, String)> = (1..=8)
        .map(|i| {
            let agents = if i <= 4 { 10 } else { 100 };
            (0, fixture(agents, false))
        })
        .collect();
    let dir = tempfile::tempdir().unwrap();
    let bin = stub(dir.path(), &replies);
    let out = gate(&bin);
    assert_eq!(out.status.code(), Some(0));
    let calls = args_log(dir.path());
    assert_eq!(calls.len(), 8, "{calls:?}");
    // Three counted runs per tier, then one informational run.
    for (i, call) in calls.iter().enumerate() {
        let counted = matches!(i, 0..=2 | 4..=6);
        assert_eq!(call.contains("--view"), counted, "call {i}: {call}");
    }
}

#[test]
fn discarded_run_is_retried_twice() {
    let mut replies = vec![
        (3, String::from("{\"discarded\":\"load\",\"load\":2.5}")),
        (3, String::from("{\"discarded\":\"load\",\"load\":2.5}")),
        (0, fixture(10, false)),
    ];
    replies.extend((4..=9).map(|i| {
        let agents = if i <= 7 { 10 } else { 100 };
        (0, fixture(agents, false))
    }));
    let dir = tempfile::tempdir().unwrap();
    let bin = stub(dir.path(), &replies);
    let out = gate(&bin);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(args_log(dir.path()).len(), 10, "two retries, then the run");

    let always_discarded: Vec<(i32, String)> = (0..3)
        .map(|_| (3, String::from("{\"discarded\":\"load\",\"load\":2.5}")))
        .collect();
    let dir = tempfile::tempdir().unwrap();
    let bin = stub(dir.path(), &always_discarded);
    let out = gate(&bin);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(args_log(dir.path()).len(), 3, "retried at most twice");
}
