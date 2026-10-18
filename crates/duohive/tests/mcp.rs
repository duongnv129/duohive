//! `duohive mcp`, the built binary, as an agent's CLI runs it: MCP on
//! stdin/stdout, the token and data dir from the launch's environment.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly on setup errors"
)]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use duohive::system::{SystemOptions, start_system};
use duohive_cli::fake::fake_adapters;
use duohive_core::event::Event;
use duohive_core::inbox::Sender;
use duohive_core::launch::Token;
use duohive_core::ports::{NoGit, NoNotifier, SessionRow, Store};
use duohive_core::status::{AgentState, Status};
use duohive_core::testkit::{example_org, n};
use duohive_core::time::Millis;
use duohive_store::{Paths, SqliteStore, save_org};
use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_duohive");

fn session(agent: &str, token: &str, as_duo: bool) -> SessionRow {
    SessionRow {
        agent: n(agent),
        token: Some(Token(token.into())),
        window: None,
        status: Status {
            state: AgentState::Idle,
            quiet: false,
            quiet_since: None,
            from_screen: false,
            since: Millis(1),
            error: None,
            waiting_on: None,
        },
        last_reply: None,
        task_at_launch: false,
        saw_end_hook: false,
        last_task: None,
        worktree: None,
        launched_as_duo: as_duo,
    }
}

/// A data dir whose `state.db` holds sessions for `pay-qa` and Duo; no `run.sock`.
fn closed_org(dir: &Path) -> Paths {
    let paths = Paths {
        config_dir: dir.join("config"),
        data_dir: dir.join("data"),
        tmux_socket: "duohive-test-unused".into(),
    };
    let mut store = SqliteStore::open(&paths).unwrap();
    store
        .save_session(&session("pay-qa", "tok-1", false))
        .unwrap();
    store
        .save_session(&session("Duo", "tok-duo", true))
        .unwrap();
    paths
}

struct Helper {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Helper {
    fn spawn(paths: &Paths, token: &str) -> Self {
        let mut child = Command::new(BIN)
            .arg("mcp")
            .env_clear()
            .env("HOME", &paths.config_dir)
            .env("DUOHIVE_DATA_DIR", &paths.data_dir)
            .env("DUOHIVE_CONFIG_DIR", &paths.config_dir)
            .env("DUOHIVE_TOKEN", token)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn ask(&mut self, request: &Value) -> Value {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{request}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    fn tools(&mut self) -> Vec<String> {
        self.ask(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                         "params": {"protocolVersion": "2025-06-18"}}));
        let list = self.ask(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
        list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect()
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// With DuoHive closed, the helper lists tools from the session row's rights
/// (P3-D30). Listing through a running DuoHive is Task 3.3's, which builds rights.
#[test]
fn e2e_fake_cli_lists_tools_through_helper() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = closed_org(tmp.path());
    let mut agent = Helper::spawn(&paths, "tok-1");
    assert_eq!(
        agent.tools(),
        ["list_agents", "send_message", "message_status"]
    );
    let mut duo = Helper::spawn(&paths, "tok-duo");
    assert_eq!(duo.tools().len(), 9);
    let call = duo.ask(&json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": {"name": "send_message", "arguments": {"to": "pay-qa", "text": "hi"}}}));
    assert_eq!(
        call["result"]["structuredContent"],
        json!({"status": "queued_until_open", "request": 1})
    );
    let mut store = SqliteStore::open(&paths).unwrap();
    let queued = store.take_queued_tool_calls().unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].token, Token("tok-duo".into()));
}

/// A throwaway tmux server, killed on drop; never `duohive`.
struct Tmux(String);

impl Drop for Tmux {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["-f", "/dev/null", "-L", &self.0, "kill-server"])
            .env_remove("TMUX")
            .output();
    }
}

fn send_message(id: u64) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
           "params": {"name": "send_message",
                      "arguments": {"to": "pay-qa", "text": "run the tests"}}})
}

/// `token`'s helper: initialized, then one `send_message`, off the async executor.
async fn helper_send(paths: &Paths, token: &'static str) -> Value {
    let paths = paths.clone();
    tokio::task::spawn_blocking(move || {
        let mut helper = Helper::spawn(&paths, token);
        helper.tools();
        helper.ask(&send_message(3))
    })
    .await
    .unwrap()
}

/// P3-D24: a call queued while DuoHive was closed runs after `Reconnected`,
/// as its token's agent; once `run.sock` is bound, a call connects instead.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queued_before_open_runs_after() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths {
        config_dir: tmp.path().join("config"),
        data_dir: tmp.path().join("data"),
        tmux_socket: format!("duohive-test-mcp-{}", std::process::id()),
    };
    let _tmux = Tmux(paths.tmux_socket.clone());
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    save_org(&paths, &example_org()).unwrap();
    SqliteStore::open(&paths)
        .unwrap()
        .save_session(&session("squad-1", "tok-squad", false))
        .unwrap();
    let closed = helper_send(&paths, "tok-squad").await;
    assert_eq!(
        closed["result"]["structuredContent"],
        json!({"status": "queued_until_open", "request": 1})
    );

    let opts = SystemOptions {
        tap: None,
        first_frame: None,
        home: tmp.path().to_path_buf(),
        notifier: Some(Box::new(NoNotifier)),
        git: Some(Box::new(NoGit)),
        git_env: Vec::new(),
    };
    let adapters = Arc::new(fake_adapters(Path::new(BIN)));
    let mut h = start_system(&paths, adapters, opts).await.unwrap();
    let mut reconnected = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    let queued = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, h.events.recv()).await {
            Ok(Some(Event::Reconnected)) => reconnected = true,
            Ok(Some(Event::MessageQueued { id, from, to })) => break (id, from, to),
            Ok(Some(_)) => {}
            other => panic!("no message queued: {other:?}"),
        }
    };
    assert!(reconnected, "queued before Reconnected");
    assert_eq!(queued.1, Sender::Agent(n("squad-1")));
    assert_eq!(queued.2, n("pay-qa"));

    let open = helper_send(&paths, "tok-squad").await;
    // squad-1 had no window, so it is stopped and its token is no one's.
    assert_eq!(
        open["result"]["structuredContent"]["error"]["code"], "unknown_token",
        "{open}"
    );
    let mut store = SqliteStore::open(&paths).unwrap();
    assert_eq!(store.take_queued_tool_calls().unwrap(), vec![]);
    h.shutdown().await;
}

#[test]
fn helper_exits_on_stdin_end() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = closed_org(tmp.path());
    let mut helper = Helper::spawn(&paths, "tok-1");
    helper.tools();
    drop(helper.stdin.take());
    let started = Instant::now();
    loop {
        if let Some(status) = helper.child.try_wait().unwrap() {
            assert!(status.success(), "{status}");
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(1), "still running");
        std::thread::sleep(Duration::from_millis(10));
    }
}
