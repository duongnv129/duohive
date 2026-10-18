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
use std::time::{Duration, Instant};

use duohive_core::launch::Token;
use duohive_core::ports::{SessionRow, Store};
use duohive_core::status::{AgentState, Status};
use duohive_core::testkit::n;
use duohive_core::time::Millis;
use duohive_store::{Paths, SqliteStore};
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
