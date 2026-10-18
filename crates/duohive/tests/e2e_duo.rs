//! Duo end to end (TD §9): the headless system on a real tmux server (a
//! throwaway socket, never `duohive`), temp dirs, the fake CLIs, a
//! [`FakeNotifier`] and [`NoGit`]. Duo is the fake agent running a script of
//! tool calls through the real `duohive mcp` helper, as an agent's CLI runs it
//! (`bench-agent --mcp-script`); no model is called. Every step shows as a chat
//! entry: nothing attaches to a pane, and no key reaches a pane but by the inbox.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly on setup errors"
)]

use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command as StdCommand, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use duohive::system::{SystemHandle, SystemOptions, start_system};
use duohive_cli::fake::scripted_fake_adapters;
use duohive_core::card::{Card, CardChange};
use duohive_core::chat::{ChatEntry, ChatKind};
use duohive_core::cli_kind::CliKind;
use duohive_core::command::Command;
use duohive_core::event::{Event, RoleOutcome};
use duohive_core::names::{AgentName, NodeName};
use duohive_core::org::{AgentDef, Fields, Org, Role, RoleKey, Team};
use duohive_core::ports::{NoGit, Store};
use duohive_core::status::AgentState;
use duohive_core::testkit::{FakeNotifier, n};
use duohive_core::worktree::RunIn;
use duohive_store::{Paths, SqliteStore, save_org};
use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_duohive");
/// Long enough for a fake agent to start on a loaded machine.
const START: Duration = Duration::from_secs(20);
/// The story, from your line to Duo's report (Task 3.12).
const STORY: Duration = Duration::from_secs(30);
/// A free agent takes a role's task: its busy hook within this.
const TAKEN: Duration = Duration::from_secs(6);

fn random_hex() -> String {
    let mut h = RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    );
    h.write_u32(std::process::id());
    format!("{:016x}", h.finish())
}

/// Temp config, data and home dirs plus a throwaway tmux socket, whose server
/// (and every fake agent in it) is killed on drop.
struct Env {
    _tmp: tempfile::TempDir,
    paths: Paths,
    home: PathBuf,
    work: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths {
            config_dir: tmp.path().join("config"),
            data_dir: tmp.path().join("data"),
            tmux_socket: format!("duohive-test-{}", random_hex()),
        };
        let home = tmp.path().join("home");
        let work = tmp.path().join("work");
        for dir in [&paths.config_dir, &home, &work] {
            std::fs::create_dir_all(dir).unwrap();
        }
        Self {
            _tmp: tmp,
            paths,
            home,
            work,
        }
    }

    /// The system on the fake CLIs, Duo running `fixtures/duo/<script>` if any.
    async fn start(&self, script: Option<&str>) -> SystemHandle {
        save_org(&self.paths, &duo_org(&self.work)).unwrap();
        let script = script.map(|s| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/duo")
                .join(s)
        });
        let adapters = Arc::new(scripted_fake_adapters(Path::new(BIN), script.as_deref()));
        let opts = SystemOptions {
            tap: None,
            first_frame: None,
            home: self.home.clone(),
            notifier: Some(Box::new(FakeNotifier::new(true))),
            git: Some(Box::new(NoGit)),
            git_env: Vec::new(),
        };
        start_system(&self.paths, adapters, opts).await.unwrap()
    }
}

impl Drop for Env {
    /// Kills this test's own tmux server (its throwaway socket only).
    fn drop(&mut self) {
        let _ = StdCommand::new("tmux")
            .args(["-f", "/dev/null", "-L", &self.paths.tmux_socket])
            .arg("kill-server")
            .env_remove("TMUX")
            .output();
    }
}

fn fields(work: &Path) -> Fields {
    Fields {
        cli: Some(CliKind::Claude),
        directory: Some(work.display().to_string()),
        ..Fields::default()
    }
}

fn role(name: &str, agents: &[&str], defaults: Fields) -> Role {
    Role {
        name: NodeName::parse(name).unwrap(),
        defaults,
        agents: agents
            .iter()
            .map(|a| AgentDef {
                name: n(a),
                fields: Fields::default(),
            })
            .collect(),
    }
}

/// Duo; Payments › Backend (`auth-service`, `billing-api`), Payments › QA
/// (`pay-qa`) and Generalist (`squad-1`, `squad-2`), all working in `work`.
fn duo_org(work: &Path) -> Org {
    let mut org = Org::blank(CliKind::Claude);
    org.teams = vec![Team {
        name: NodeName::parse("Payments").unwrap(),
        defaults: fields(work),
        roles: vec![
            role(
                "Backend",
                &["auth-service", "billing-api"],
                Fields::default(),
            ),
            role("QA", &["pay-qa"], Fields::default()),
        ],
    }];
    org.roles = vec![role("Generalist", &["squad-1", "squad-2"], fields(work))];
    org
}

fn generalist() -> RoleKey {
    RoleKey {
        team: None,
        role: NodeName::parse("Generalist").unwrap(),
    }
}

/// What the events showed so far.
#[derive(Default)]
struct Seen {
    chat: Vec<ChatEntry>,
    states: HashMap<AgentName, AgentState>,
    cards: Vec<Card>,
    roles: Vec<Event>,
}

impl Seen {
    fn apply(&mut self, ev: Event) {
        match ev {
            Event::ChatLine(entry) => self.chat.push(entry),
            Event::AgentStatusChanged { agent, status } => {
                self.states.insert(agent, status.state);
            }
            Event::CardRaised(card) => self.cards.push(*card),
            ev @ (Event::RoleAssigned { .. } | Event::AssignRefused { .. }) => self.roles.push(ev),
            _ => {}
        }
    }

    /// The chat as the pane reads it: `you: …`, `duo: …`, `→ text   result`.
    fn lines(&self) -> Vec<String> {
        self.chat
            .iter()
            .map(|e| match &e.kind {
                ChatKind::You { .. } => format!("you: {}", e.text),
                ChatKind::Duo => format!("duo: {}", e.text),
                ChatKind::Action { result } => format!("→ {}   {result}", e.text),
                ChatKind::CardResult { .. } => format!("card: {}", e.text),
                ChatKind::Note => format!("note: {}", e.text),
            })
            .collect()
    }

    fn has(&self, line: &str) -> bool {
        self.lines().iter().any(|l| l == line)
    }

    fn is(&self, agent: &str, state: AgentState) -> bool {
        self.states.get(&n(agent)) == Some(&state)
    }

    /// Reads events until `cond` holds; panics after `within` with the chat so far.
    async fn until(
        &mut self,
        h: &mut SystemHandle,
        within: Duration,
        what: &str,
        cond: impl Fn(&Self) -> bool,
    ) {
        let deadline = Instant::now() + within;
        while !cond(self) {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                panic!(
                    "no {what} within {within:?}: chat {:#?}, states {:?}",
                    self.lines(),
                    self.states
                );
            };
            match tokio::time::timeout(left, h.events.recv()).await {
                Ok(Some(ev)) => self.apply(ev),
                Ok(None) => panic!("the core stopped before {what}: {:#?}", self.lines()),
                Err(_) => {}
            }
        }
    }
}

async fn send(h: &SystemHandle, c: Command) {
    h.commands.send(c).await.unwrap();
}

async fn give(h: &SystemHandle, agent: &str, task: &str) {
    let c = Command::GiveWork {
        agent: n(agent),
        task: task.into(),
        run: RunIn::Directory,
    };
    send(h, c).await;
}

/// The lines in `got` that are in `want`, in `got`'s order.
fn in_order<'a>(got: &[String], want: &[&'a str]) -> Vec<&'a str> {
    got.iter()
        .filter_map(|l| want.iter().find(|w| *w == l).copied())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_story_through_the_factory() {
    let env = Env::new();
    let mut h = env.start(Some("story.json")).await;
    let mut seen = Seen::default();
    send(
        &h,
        Command::ChatToDuo {
            text: "Story: let users retry a failed payment".into(),
        },
    )
    .await;
    let report = "duo: PAY-88: all three subtasks finished";
    seen.until(&mut h, STORY, "Duo's report", |s| s.has(report))
        .await;
    let want = [
        "you: Story: let users retry a failed payment",
        "→ give work auth-service \"PAY-88a backend: retry endpoint\"   started",
        "→ give work pay-qa \"PAY-88c QA: retry cases\"   started",
        "→ give work squad-1 \"PAY-88b frontend: retry button\"   started",
        "→ wait for auth-service   finished",
        "→ wait for pay-qa   finished",
        "→ wait for squad-1   finished",
        report,
    ];
    assert_eq!(in_order(&seen.lines(), &want), want, "{:#?}", seen.lines());
    for agent in ["auth-service", "pay-qa", "squad-1"] {
        assert!(
            seen.is(agent, AgentState::Finished),
            "{agent}: {:?}",
            seen.states
        );
    }
    assert!(seen.is("squad-2", AgentState::Stopped) || !seen.states.contains_key(&n("squad-2")));
    // Each answer is logged, and `$result.3.agent` named the role's taker.
    let log = std::fs::read_to_string(env.paths.data_dir.join("run/Duo/calls.jsonl")).unwrap();
    let answers: Vec<Value> = log
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(answers.len(), 6, "{log}");
    assert_eq!(answers[2]["agent"], "squad-1", "{log}");
    assert_eq!(answers[5]["agent"], "squad-1", "{log}");
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_whats_blocked_then_fix_it() {
    let env = Env::new();
    let mut h = env.start(Some("blocked.json")).await;
    let mut seen = Seen::default();
    give(&h, "billing-api", "ask to deploy the retry fix").await;
    give(&h, "squad-1", "a long refactor").await;
    seen.until(&mut h, START, "billing-api waiting, squad-1 working", |s| {
        s.is("billing-api", AgentState::Waiting) && s.is("squad-1", AgentState::Working)
    })
    .await;

    let asked = "duo: billing-api is waiting on an approval; squad-1 is busy";
    send(
        &h,
        Command::ChatToDuo {
            text: "what's blocked?".into(),
        },
    )
    .await;
    seen.until(&mut h, STORY, "Duo's answer", |s| s.has(asked))
        .await;

    let fixing = "duo: asked you to stop squad-1; told pay-qa to rerun the tests";
    send(
        &h,
        Command::ChatToDuo {
            text: "fix it: stop squad-1 and tell pay-qa to rerun the tests".into(),
        },
    )
    .await;
    seen.until(&mut h, STORY, "the stop card", |s| {
        s.has(fixing) && !s.cards.is_empty()
    })
    .await;
    let card = seen.cards[0].clone();
    assert_eq!(
        card.change,
        CardChange::Stop {
            agent: n("squad-1")
        }
    );
    assert!(seen.is("squad-1", AgentState::Working), "{:?}", seen.states);

    send(
        &h,
        Command::ApproveCard {
            card: card.id,
            edited: None,
        },
    )
    .await;
    let done = "duo: squad-1 stopped";
    seen.until(&mut h, STORY, "squad-1 stopped and Duo told", |s| {
        s.is("squad-1", AgentState::Stopped) && s.has(done)
    })
    .await;

    let want = [
        "you: what's blocked?",
        "→ list agents   6 agents",
        "→ read billing-api   read",
        asked,
        "you: fix it: stop squad-1 and tell pay-qa to rerun the tests",
        "→ stop squad-1   card raised",
        "→ @pay-qa \"rerun the tests\"   queued (pay-qa is stopped)",
        fixing,
        "card: approved: stop squad-1",
        done,
    ];
    assert_eq!(in_order(&seen.lines(), &want), want, "{:#?}", seen.lines());
    assert!(
        seen.is("billing-api", AgentState::Waiting),
        "{:?}",
        seen.states
    );
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_role_task_reaches_free_agent() {
    let env = Env::new();
    let mut h = env.start(None).await;
    let mut seen = Seen::default();
    give(&h, "squad-2", "a short task").await;
    give(&h, "squad-1", "a long refactor").await;
    seen.until(&mut h, START, "squad-2 finished, squad-1 working", |s| {
        s.is("squad-2", AgentState::Finished) && s.is("squad-1", AgentState::Working)
    })
    .await;

    let assign = |task: &str| Command::AssignToRole {
        role: generalist(),
        task: task.into(),
        pick: None,
        run: None,
    };
    send(&h, assign("PAY-90 a long migration")).await;
    seen.until(&mut h, TAKEN, "squad-2 busy with the role's task", |s| {
        s.is("squad-2", AgentState::Working)
    })
    .await;
    match &seen.roles[..] {
        [
            Event::RoleAssigned {
                outcome: RoleOutcome::Given { agent, .. },
                ..
            },
        ] => assert_eq!(agent, &n("squad-2")),
        other => panic!("not given to squad-2: {other:#?}"),
    }

    // Both working: a card proposes a new agent holding the task.
    send(&h, assign("PAY-91 retry docs")).await;
    seen.until(&mut h, START, "the no-free-agent card", |s| {
        !s.cards.is_empty() && s.roles.len() == 2
    })
    .await;
    match &seen.cards[0].change {
        CardChange::NewAgent { role, task, .. } => {
            assert_eq!(role, &generalist());
            assert_eq!(task, "PAY-91 retry docs");
        }
        other => panic!("not a new-agent card: {other:?}"),
    }
    match &seen.roles[1] {
        Event::RoleAssigned {
            outcome: RoleOutcome::Card(card),
            ..
        } => assert_eq!(*card, seen.cards[0].id),
        other => panic!("no card for the role: {other:?}"),
    }
    h.shutdown().await;
}

/// `duohive mcp` as an agent's CLI runs it, with a launch's token.
struct Helper {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Helper {
    fn spawn(paths: &Paths, token: &str) -> Self {
        let mut child = StdCommand::new(BIN)
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
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn ask(&mut self, request: &Value) -> Value {
        writeln!(self.stdin, "{request}").unwrap();
        self.stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    /// Initializes, lists the tools, then calls `list_agents`.
    fn tools_and_agents(&mut self) -> (Vec<String>, Value) {
        self.ask(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                         "params": {"protocolVersion": "2025-06-18"}}));
        let list = self.ask(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
        let tools = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect();
        let call = self.ask(&json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "list_agents", "arguments": {}}}));
        (tools, call["result"]["structuredContent"].clone())
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Carried from Task 3.2: through a RUNNING DuoHive, an agent's token lists
/// the 3 every-agent tools and Duo's the 9; `list_agents` is answered live.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_mcp_lists_tools_through_running_duohive() {
    let env = Env::new();
    let mut h = env.start(None).await;
    let mut seen = Seen::default();
    for agent in ["pay-qa", "Duo"] {
        send(&h, Command::StartAgent { agent: n(agent) }).await;
    }
    seen.until(&mut h, START, "pay-qa and Duo idle", |s| {
        s.is("pay-qa", AgentState::Idle) && s.is("Duo", AgentState::Idle)
    })
    .await;
    let rows = SqliteStore::open(&env.paths)
        .unwrap()
        .load_sessions()
        .unwrap();
    let token = |agent: &str| {
        let row = rows.iter().find(|r| r.agent == n(agent)).unwrap();
        row.token.clone().unwrap().0
    };
    let (agent_token, duo_token) = (token("pay-qa"), token("Duo"));
    let paths = env.paths.clone();
    let (agent, duo) = tokio::task::spawn_blocking(move || {
        let agent = Helper::spawn(&paths, &agent_token).tools_and_agents();
        let duo = Helper::spawn(&paths, &duo_token).tools_and_agents();
        (agent, duo)
    })
    .await
    .unwrap();
    assert_eq!(agent.0, ["list_agents", "send_message", "message_status"]);
    assert_eq!(duo.0.len(), 9, "{:?}", duo.0);
    // Live answers: closed, `list_agents` has no engine to ask.
    for (tools, listed) in [&agent, &duo] {
        let names: Vec<_> = listed["agents"]
            .as_array()
            .unwrap_or_else(|| panic!("{tools:?}: {listed}"))
            .iter()
            .map(|a| a["name"].as_str().unwrap().to_owned())
            .collect();
        assert!(names.contains(&"squad-2".to_owned()), "{names:?}");
    }
    h.shutdown().await;
}
