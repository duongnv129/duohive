//! The built binary in app mode, where it ends before the TUI: `--help`, tmux
//! missing, and a second instance. The environment is set per process with
//! `Command::env`, never `set_var`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly on setup errors"
)]

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;

use duohive::args::USAGE;
use duohive::system::{SystemOptions, start_system};
use duohive_cli::fake::fake_adapters;
use duohive_core::ports::{NoGit, NoNotifier};
use duohive_store::Paths;

const BIN: &str = env!("CARGO_BIN_EXE_duohive");

/// Temp config, data and home dirs and a throwaway tmux socket name.
fn paths(tmp: &Path) -> Paths {
    let paths = Paths {
        config_dir: tmp.join("config"),
        data_dir: tmp.join("data"),
        tmux_socket: format!("duohive-test-app-{}", std::process::id()),
    };
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::create_dir_all(tmp.join("home")).unwrap();
    paths
}

/// Kills the test's own tmux server (its throwaway socket only) on drop, also
/// when the test panics.
struct KillServer(String);

impl Drop for KillServer {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["-f", "/dev/null", "-L", &self.0, "kill-server"])
            .env_remove("TMUX")
            .output();
    }
}

/// Runs the app with `paths`, no terminal, and `path` as `PATH`.
fn run_app(tmp: &Path, paths: &Paths, path: &str) -> Output {
    Command::new(BIN)
        .env_clear()
        .env("PATH", path)
        .env("HOME", tmp.join("home"))
        .env("DUOHIVE_CONFIG_DIR", &paths.config_dir)
        .env("DUOHIVE_DATA_DIR", &paths.data_dir)
        .env("DUOHIVE_TMUX_SOCKET", &paths.tmux_socket)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn help_prints_usage_and_exits_0() {
    let out = Command::new(BIN).arg("--help").output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), USAGE);
}

#[test]
fn missing_tmux_exits_1_with_a_message() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = paths(tmp.path());
    let out = run_app(tmp.path(), &paths, "/nonexistent");
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    // No terminal: nothing is drawn to stdout.
    assert!(out.stdout.is_empty(), "{out:?}");
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("tmux 3.2 or newer is required"), "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_instance_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = paths(tmp.path());
    let server = KillServer(paths.tmux_socket.clone());
    let first = start_system(
        &paths,
        Arc::new(fake_adapters(Path::new(BIN))),
        SystemOptions {
            tap: None,
            first_frame: None,
            home: tmp.path().join("home"),
            notifier: Some(Box::new(NoNotifier)),
            git: Some(Box::new(NoGit)),
            git_env: Vec::new(),
        },
    )
    .await
    .unwrap();
    let path = std::env::var("PATH").unwrap();
    let out = {
        let (tmp, paths) = (tmp.path().to_path_buf(), paths.clone());
        tokio::task::spawn_blocking(move || run_app(&tmp, &paths, &path))
            .await
            .unwrap()
    };
    first.shutdown().await;
    drop(server);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stderr.clone()).unwrap(),
        "DuoHive is already running\n"
    );
}
