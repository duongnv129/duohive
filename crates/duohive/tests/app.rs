//! The built binary in app mode, where it ends before the TUI: `--help`, tmux
//! missing, and a second instance. The environment is set per process with
//! `Command::env`, never `set_var`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::match_wild_err_arm,
    reason = "test helpers fail loudly on setup errors"
)]

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use duohive::args::USAGE;
use duohive::system::{SystemHandle, SystemOptions, start_system};
use duohive_cli::fake::fake_adapters;
use duohive_core::cli_kind::CliKind;
use duohive_core::command::Command as CoreCommand;
use duohive_core::event::Event;
use duohive_core::ports::{NoGit, NoNotifier};
use duohive_core::templates::TemplateInput;
use duohive_store::Paths;

/// How long a test waits for the engine to answer.
const WAIT: Duration = Duration::from_secs(10);

const BIN: &str = env!("CARGO_BIN_EXE_duohive");

/// Makes every `paths` call's tmux socket name different within this process.
static NEXT_SOCKET: AtomicUsize = AtomicUsize::new(0);

/// Temp config, data and home dirs and a throwaway tmux socket name, unique per call.
fn paths(tmp: &Path) -> Paths {
    let paths = Paths {
        config_dir: tmp.join("config"),
        data_dir: tmp.join("data"),
        tmux_socket: format!(
            "duohive-test-app-{}-{}",
            std::process::id(),
            NEXT_SOCKET.fetch_add(1, Ordering::SeqCst)
        ),
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

/// Tests in one binary run in parallel. A socket shared between them lets one test's
/// `KillServer` kill the server another is still using ("no server running").
#[test]
fn each_test_gets_its_own_tmux_socket() {
    let tmp = tempfile::tempdir().unwrap();
    let a = paths(tmp.path());
    let b = paths(tmp.path());
    assert_ne!(a.tmux_socket, b.tmux_socket);
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

/// The next event, or a failure after [`WAIT`].
async fn next_event(h: &mut SystemHandle) -> Event {
    tokio::time::timeout(WAIT, h.events.recv())
        .await
        .expect("an event within ten seconds")
        .expect("the events channel stays open")
}

/// The windows on DuoHive's throwaway tmux server; none when it isn't running.
fn windows(socket: &str) -> Vec<String> {
    let out = Command::new("tmux")
        .args([
            "-f",
            "/dev/null",
            "-L",
            socket,
            "list-windows",
            "-a",
            "-F",
            "#{window_name}",
        ])
        .env_remove("TMUX")
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_run_shows_wizard_and_create_org_starts_no_agent() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = paths(tmp.path());
    let server = KillServer(paths.tmux_socket.clone());
    let options = || SystemOptions {
        tap: None,
        first_frame: None,
        home: tmp.path().join("home"),
        notifier: Some(Box::new(NoNotifier)),
        git: Some(Box::new(NoGit)),
        git_env: Vec::new(),
    };
    let mut first = start_system(&paths, Arc::new(fake_adapters(Path::new(BIN))), options())
        .await
        .unwrap();
    let Event::Snapshot(snap) = next_event(&mut first).await else {
        panic!("the first event is the start-up snapshot");
    };
    assert!(
        snap.first_run.as_ref().is_some_and(|list| !list.is_empty()),
        "no org yet: the wizard's inventory comes with the snapshot"
    );
    assert!(!paths.org_toml().exists(), "the first run writes no org");

    first
        .commands
        .send(CoreCommand::CreateOrg {
            template: TemplateInput::Blank,
            your_name: Some("Duong".to_owned()),
            duo_cli: CliKind::Claude,
        })
        .await
        .unwrap();
    let mut created = false;
    for _ in 0..20 {
        if let Event::OrgCreated = next_event(&mut first).await {
            created = true;
            break;
        }
    }
    assert!(created, "Create org answers with OrgCreated");
    assert!(
        paths.org_toml().exists(),
        "the wizard's answers are on disk"
    );
    // The start-up home window is the only one; no agent gets one.
    let open: Vec<String> = windows(&paths.tmux_socket)
        .into_iter()
        .filter(|w| w != "duohive-home")
        .collect();
    assert!(open.is_empty(), "no agent starts, found {open:?}");
    first.shutdown().await;

    // The org exists now, so the next start is no first run.
    let mut second = start_system(&paths, Arc::new(fake_adapters(Path::new(BIN))), options())
        .await
        .unwrap();
    let Event::Snapshot(snap) = next_event(&mut second).await else {
        panic!("the first event is the start-up snapshot");
    };
    assert!(snap.first_run.is_none(), "an org exists: no wizard");
    second.shutdown().await;
    drop(server);
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
