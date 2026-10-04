//! `duohive hook` and the hook listener (Review Focus 1 and 5).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly on setup errors"
)]

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock, mpsc as std_mpsc};
use std::time::{Duration, Instant};

use duohive::hook_cmd::run_hook;
use duohive::hook_listener::{AgentClis, HookListener, ListenerError};
use duohive_core::adapter::Adapters;
use duohive_core::cli_kind::CliKind;
use duohive_core::engine::Input;
use duohive_core::hook::HookMessage;
use duohive_core::ports::Store;
use duohive_core::testkit::StubAdapter;
use duohive_store::{Paths, SqliteStore};
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

const BUDGET: Duration = Duration::from_secs(1);

fn s(items: &[&str]) -> Vec<String> {
    items.iter().map(|i| (*i).to_owned()).collect()
}

fn vars(token: Option<&str>, data: &Path) -> HashMap<String, String> {
    let mut v = HashMap::new();
    v.insert("DUOHIVE_DATA_DIR".to_owned(), data.display().to_string());
    v.insert(
        "DUOHIVE_CONFIG_DIR".to_owned(),
        data.join("config").display().to_string(),
    );
    if let Some(t) = token {
        v.insert("DUOHIVE_TOKEN".to_owned(), t.to_owned());
    }
    v
}

fn pending(data: &Path) -> Vec<HookMessage> {
    let paths = Paths {
        config_dir: data.join("config"),
        data_dir: data.to_path_buf(),
        tmux_socket: "duohive-test-hook".to_owned(),
    };
    SqliteStore::open(&paths)
        .unwrap()
        .take_pending_hooks()
        .unwrap()
}

fn hook(token: &str, agent: &str, event: &str, payload: serde_json::Value) -> HookMessage {
    HookMessage {
        token: token.to_owned(),
        agent: agent.to_owned(),
        event: event.to_owned(),
        payload,
    }
}

struct Served {
    core: mpsc::Receiver<Input>,
    tap: mpsc::UnboundedReceiver<HookMessage>,
}

/// Binds `sock` and serves it on the current runtime with an empty agent map.
fn serve(sock: &Path, agents: AgentClis, adapters: Adapters) -> Served {
    let listener = HookListener::bind(sock).unwrap();
    let (to_core, core) = mpsc::channel(16);
    let (tap_tx, tap) = mpsc::unbounded_channel();
    tokio::spawn(listener.serve(agents, Arc::new(adapters), to_core, Some(tap_tx)));
    Served { core, tap }
}

fn no_agents() -> AgentClis {
    Arc::new(RwLock::new(HashMap::new()))
}

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_duohive"));
    c.env_clear();
    c
}

fn run_bin(mut cmd: Command, stdin_bytes: Option<&[u8]>) -> (Output, Duration) {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let start = Instant::now();
    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    if let Some(b) = stdin_bytes {
        let _ = stdin.write_all(b);
        drop(stdin);
        let out = child.wait_with_output().unwrap();
        (out, start.elapsed())
    } else {
        // Keep stdin open and silent for the whole run.
        let out = child.wait_with_output().unwrap();
        let took = start.elapsed();
        drop(stdin);
        (out, took)
    }
}

fn assert_quiet_success(out: &Output, took: Duration, case: &str) {
    assert!(out.status.success(), "{case}: status {:?}", out.status);
    assert!(out.stdout.is_empty(), "{case}: stdout {:?}", out.stdout);
    assert!(out.stderr.is_empty(), "{case}: stderr {:?}", out.stderr);
    assert!(took < BUDGET, "{case}: took {took:?}");
}

fn hook_cmd(data: &Path, token: Option<&str>) -> Command {
    let mut c = bin();
    c.args(["hook", "pay-qa", "busy"]);
    for (k, v) in vars(token, data) {
        c.env(k, v);
    }
    c
}

#[tokio::test]
async fn hook_forwards_one_line_to_the_socket() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().to_path_buf();
    let mut served = serve(&data.join("run.sock"), no_agents(), Adapters::new(vec![]));
    let env = vars(Some("tok-1"), &data);
    let code = tokio::task::spawn_blocking(move || {
        run_hook(
            &s(&["hook", "pay-qa", "busy"]),
            &b"{\"a\":1}"[..],
            false,
            &env,
        )
    })
    .await
    .unwrap();
    assert_eq!(code, ExitCode::SUCCESS);
    let got = tokio::time::timeout(BUDGET, served.tap.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got, hook("tok-1", "pay-qa", "busy", json!({"a": 1})));
    assert_eq!(
        tokio::time::timeout(BUDGET, served.core.recv())
            .await
            .unwrap(),
        Some(Input::Hook(got))
    );
    assert!(pending(&data).is_empty());
}

struct PanicReader(Arc<AtomicBool>);

impl Read for PanicReader {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        self.0.store(true, Ordering::SeqCst);
        panic!("a terminal stdin must not be read");
    }
}

#[test]
fn terminal_stdin_is_not_read() {
    let tmp = tempfile::tempdir().unwrap();
    let touched = Arc::new(AtomicBool::new(false));
    let code = run_hook(
        &s(&["hook", "pay-qa", "busy"]),
        PanicReader(touched.clone()),
        true,
        &vars(Some("tok-1"), tmp.path()),
    );
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(!touched.load(Ordering::SeqCst));
    assert_eq!(
        pending(tmp.path()),
        vec![hook("tok-1", "pay-qa", "busy", serde_json::Value::Null)]
    );
}

#[test]
fn hook_falls_back_to_the_database_when_no_listener() {
    let tmp = tempfile::tempdir().unwrap();
    let code = run_hook(
        &s(&["hook", "pay-qa", "turn-end"]),
        &b"{\"x\":2}"[..],
        false,
        &vars(Some("tok-2"), tmp.path()),
    );
    assert_eq!(code, ExitCode::SUCCESS);
    assert_eq!(
        pending(tmp.path()),
        vec![hook("tok-2", "pay-qa", "turn-end", json!({"x": 2}))]
    );
}

fn multi_thread_rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn hook_survives_silent_open_stdin() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().to_path_buf();
    let rt = multi_thread_rt();
    let mut served =
        rt.block_on(async { serve(&data.join("run.sock"), no_agents(), Adapters::new(vec![])) });
    let (out, took) = run_bin(hook_cmd(&data, Some("tok-3")), None);
    assert_quiet_success(&out, took, "silent open stdin");
    let got = rt
        .block_on(async { tokio::time::timeout(BUDGET, served.tap.recv()).await })
        .unwrap()
        .unwrap();
    assert_eq!(
        got,
        hook("tok-3", "pay-qa", "busy", serde_json::Value::Null)
    );
    drop(served.core);
}

/// Fills `listener`'s backlog: connects until one would block or is refused.
fn fill_backlog(sock: &Path) -> Vec<UnixStream> {
    let mut held = Vec::new();
    for _ in 0..4096 {
        let (tx, rx) = std_mpsc::channel();
        let path = sock.to_path_buf();
        std::thread::spawn(move || {
            let _ = tx.send(UnixStream::connect(path));
        });
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(stream)) => held.push(stream),
            Ok(Err(_)) | Err(_) => break, // refused at once, or blocked: the backlog is full
        }
    }
    held
}

#[test]
fn hook_survives_an_unreachable_socket() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().to_path_buf();
    let sock = data.join("run.sock");
    let _never_accepts = UnixListener::bind(&sock).unwrap();
    let _held = fill_backlog(&sock);
    let (out, took) = run_bin(hook_cmd(&data, Some("tok-4")), Some(b"{\"b\":2}"));
    assert_quiet_success(&out, took, "unreachable socket");
    assert_eq!(
        pending(&data),
        vec![hook("tok-4", "pay-qa", "busy", json!({"b": 2}))]
    );
}

#[test]
fn hook_never_fails_the_cli() {
    // No token.
    let tmp = tempfile::tempdir().unwrap();
    let (out, took) = run_bin(hook_cmd(tmp.path(), None), Some(b"{}"));
    assert_quiet_success(&out, took, "no token");

    // Garbage stdin (no listener: lands in the database, wrapped).
    let tmp = tempfile::tempdir().unwrap();
    let (out, took) = run_bin(
        hook_cmd(tmp.path(), Some("tok-5")),
        Some(b"\xff\xfe not json {"),
    );
    assert_quiet_success(&out, took, "garbage stdin");
    let rows = pending(tmp.path());
    assert_eq!(rows.len(), 1);
    assert!(rows[0].payload["raw"].is_string(), "{:?}", rows[0].payload);

    // Data dir unwritable: its parent is read-only, so neither the database nor the log can be created.
    let tmp = tempfile::tempdir().unwrap();
    let ro: PathBuf = tmp.path().join("ro");
    fs::create_dir(&ro).unwrap();
    fs::set_permissions(&ro, fs::Permissions::from_mode(0o500)).unwrap();
    let (out, took) = run_bin(hook_cmd(&ro.join("data"), Some("tok-6")), Some(b"{}"));
    fs::set_permissions(&ro, fs::Permissions::from_mode(0o700)).unwrap();
    assert_quiet_success(&out, took, "unwritable data dir");

    // The socket path is a regular file.
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("run.sock"), b"not a socket").unwrap();
    let (out, took) = run_bin(hook_cmd(tmp.path(), Some("tok-7")), Some(b"{}"));
    assert_quiet_success(&out, took, "socket is a regular file");
    assert_eq!(pending(tmp.path()).len(), 1);
}

#[test]
fn second_instance_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("run.sock");
    let _first = HookListener::bind(&sock).unwrap();
    assert!(matches!(
        HookListener::bind(&sock),
        Err(ListenerError::AlreadyRunning)
    ));
    assert_eq!(
        ListenerError::AlreadyRunning.to_string(),
        "another DuoHive is already running (run.sock answers)"
    );
}

#[test]
fn stale_socket_replaced() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("run.sock");
    drop(UnixListener::bind(&sock).unwrap()); // a crash leaves the file behind
    assert!(sock.exists());
    // On macOS a child spawned by a parallel test can inherit the listener in the
    // window before std marks it close-on-exec; wait until nothing answers.
    let deadline = Instant::now() + Duration::from_secs(5);
    while UnixStream::connect(&sock).is_ok() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let _listener = HookListener::bind(&sock).unwrap();
    UnixStream::connect(&sock).unwrap();
}

#[test]
fn socket_is_owner_only() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("run.sock");
    let _listener = HookListener::bind(&sock).unwrap();
    let mode = fs::metadata(&sock).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

fn line(m: &HookMessage) -> Vec<u8> {
    let mut l = serde_json::to_vec(m).unwrap();
    l.push(b'\n');
    l
}

#[tokio::test]
async fn oversized_line_dropped() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("run.sock");
    let mut served = serve(&sock, no_agents(), Adapters::new(vec![]));
    let valid = hook("tok-8", "pay-qa", "busy", json!({"ok": true}));
    let mut stream = tokio::net::UnixStream::connect(&sock).await.unwrap();
    let mut big = vec![b'a'; 2 * 1024 * 1024];
    big.push(b'\n');
    stream.write_all(&big).await.unwrap();
    stream.write_all(&line(&valid)).await.unwrap();
    stream.shutdown().await.unwrap();
    let first = tokio::time::timeout(BUDGET, served.core.recv())
        .await
        .unwrap();
    assert_eq!(first, Some(Input::Hook(valid)));
    let more = tokio::time::timeout(Duration::from_millis(200), served.core.recv()).await;
    assert!(!matches!(more, Ok(Some(_))), "{more:?}");
    drop(served.tap);
}

fn add_reply(_: &str, p: &mut serde_json::Value) {
    p["duohive_last_reply"] = json!("done");
}

#[tokio::test]
async fn listener_enriches_before_forwarding() {
    let tmp = tempfile::tempdir().unwrap();
    let sock = tmp.path().join("run.sock");
    let agents: AgentClis = Arc::new(RwLock::new(HashMap::from([(
        "pay-qa".to_owned(),
        CliKind::Claude,
    )])));
    let adapters = Adapters::new(vec![Box::new(
        StubAdapter::new(CliKind::Claude).with_enrich(add_reply),
    )]);
    let mut served = serve(&sock, agents, adapters);
    let mut stream = tokio::net::UnixStream::connect(&sock).await.unwrap();
    stream
        .write_all(&line(&hook("tok-9", "pay-qa", "turn-end", json!({"a": 1}))))
        .await
        .unwrap();
    let got = tokio::time::timeout(BUDGET, served.core.recv())
        .await
        .unwrap();
    assert_eq!(
        got,
        Some(Input::Hook(hook(
            "tok-9",
            "pay-qa",
            "turn-end",
            json!({"a": 1, "duohive_last_reply": "done"})
        )))
    );
    drop(served.tap);
}
