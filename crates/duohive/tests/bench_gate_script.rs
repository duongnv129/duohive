//! `scripts/bench-gate.sh` is a thin wrapper: it runs `$DUOHIVE_BIN bench --gate`
//! with its own arguments and returns that exit code. The gate's method is
//! tested in `bench/gate.rs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly on setup errors"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/bench-gate.sh");

/// A stub binary that records its arguments and exits with `code`.
fn stub(dir: &Path, code: i32) -> PathBuf {
    let path = dir.join("stub.sh");
    let log = dir.join("args.log");
    let body = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit {code}\n",
        log.display()
    );
    std::fs::write(&path, body).unwrap();
    let chmod = Command::new("chmod").arg("+x").arg(&path).status().unwrap();
    assert!(chmod.success());
    path
}

fn run(stub: &Path, extra: &[&str]) -> std::process::Output {
    Command::new("sh")
        .arg(SCRIPT)
        .args(extra)
        .env("DUOHIVE_BIN", stub)
        .output()
        .unwrap()
}

#[test]
fn bench_gate_script_delegates() {
    let dir = tempfile::tempdir().unwrap();
    let bin = stub(dir.path(), 0);
    let out = run(&bin, &["--json"]);
    assert_eq!(out.status.code(), Some(0));
    let log = std::fs::read_to_string(dir.path().join("args.log")).unwrap();
    assert_eq!(log.trim(), "bench --gate --json");
}

#[test]
fn bench_gate_script_returns_the_gates_exit_code() {
    for code in [1, 3, 130] {
        let dir = tempfile::tempdir().unwrap();
        let bin = stub(dir.path(), code);
        assert_eq!(run(&bin, &[]).status.code(), Some(code));
    }
}
