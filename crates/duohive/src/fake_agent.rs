//! `duohive bench-agent [--no-hooks] [--ignore-input] [--mcp-script <path>]
//! [--task <text>]`: a fake agent CLI for the bench, the e2e tests and CI
//! (`duohive_cli::fake::FakeCliAdapter` launches it).
//!
//! It fires `session-start` and shows its prompt `❯ `; a task runs as the first
//! turn; then each stdin line is a turn: `busy`, `working…` every 100 ms for 1 s
//! (3 s when the line holds `slow`, 30 s when it holds `long`), a reply,
//! `turn-end` with the reply, and the prompt again. With `--mcp-script`, a turn
//! the script answers makes its tool calls instead and replies its reply
//! ([`script`]). A turn whose text starts with `ask ` first asks: it shows
//! `allow? (y/n)` and fires `needs-input`, and the next line answers it.
//! `/exit` (or the end of stdin) fires `session-end` and exits 0.
//! With `--ignore-input`, every other stdin line is dropped: a typed-in task is
//! never taken (P2-D11).
//! Hooks run `duohive hook` in order on a helper thread, so a slow hook never
//! stretches a turn; every payload carries `sent_at_ms`, the agent's clock.

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use self::script::{Script, Step};

mod script;

const PROMPT: &str = "❯ ";
const WORKING: &str = "working…";
const EXIT: &str = "/exit";
const SLOW_WORD: &str = "slow";
const LONG_WORD: &str = "long";
const ASK_PREFIX: &str = "ask ";
const ASKING: &str = "allow? (y/n)";
const TICK: Duration = Duration::from_millis(100);
const TURN: Duration = Duration::from_secs(1);
const SLOW_TURN: Duration = Duration::from_secs(3);
/// Holds an agent busy for a whole test; a stop meanwhile ends by the grace's kill.
const LONG_TURN: Duration = Duration::from_secs(30);

/// How the fake agent runs: `duohive bench-agent`'s flags.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BenchAgentArgs {
    /// Fire hooks (unless `--no-hooks`).
    pub hooks: bool,
    /// Drop every stdin line but `/exit` (`--ignore-input`).
    pub ignore_input: bool,
    /// The first turn's task, verbatim.
    pub task: Option<String>,
    /// A script of tool calls to make through `duohive mcp` (`--mcp-script`).
    pub mcp_script: Option<PathBuf>,
}

/// Runs the fake agent; `env` holds the process environment (`DUOHIVE_AGENT`
/// names the agent its hooks are for).
pub fn run_bench_agent(args: &BenchAgentArgs, env: &HashMap<String, String>) -> ExitCode {
    let agent = env.get("DUOHIVE_AGENT").filter(|a| !a.is_empty());
    let exe = std::env::current_exe().ok();
    let script = match &args.mcp_script {
        Some(path) => match Script::load(path, exe.as_deref(), env) {
            Ok(script) => Some(script),
            Err(e) => {
                show(&format!("bad --mcp-script: {e}\n"));
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    let hooks = match (args.hooks, agent, exe) {
        (true, Some(agent), Some(exe)) => Hooks::start(exe, agent.clone()),
        _ => Hooks::off(),
    };
    let mut agent = Agent {
        hooks,
        ignore_input: args.ignore_input,
        asked: None,
        script,
    };
    agent.hooks.fire("session-start", json!({}));
    show(PROMPT);
    let mut going = args.task.as_deref().is_none_or(|t| agent.task(t));
    let stdin = io::stdin();
    let mut lines = stdin.lock().split(b'\n');
    while going {
        let Some(Ok(bytes)) = lines.next() else { break };
        going = agent.line(&String::from_utf8_lossy(&bytes));
    }
    agent.hooks.fire("session-end", json!({}));
    agent.hooks.finish();
    ExitCode::SUCCESS
}

struct Agent {
    hooks: Hooks,
    ignore_input: bool,
    /// The turn waiting for an answer to its question.
    asked: Option<String>,
    /// The `--mcp-script`, if any.
    script: Option<Script>,
}

impl Agent {
    /// The launch task: always taken, even with `--ignore-input`.
    fn task(&mut self, text: &str) -> bool {
        self.input(text, false)
    }

    /// One stdin line: `false` once it is `/exit`.
    fn line(&mut self, text: &str) -> bool {
        self.input(text, self.ignore_input)
    }

    /// One input: `false` once it is `/exit`. Control bytes (the Escape a stop
    /// sends first) are dropped, like a CLI's input box.
    fn input(&mut self, text: &str, ignore: bool) -> bool {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        let text = text.trim();
        if text == EXIT {
            return false;
        }
        if ignore {
            return true;
        }
        if let Some(asked) = self.asked.take() {
            self.hooks.fire("busy", json!({}));
            self.work(&asked);
        } else if text.is_empty() {
            show(PROMPT);
        } else {
            self.turn(text);
        }
        true
    }

    /// One turn: `busy`, then a question when it starts with `ask `, else the work.
    fn turn(&mut self, text: &str) {
        show("\n");
        self.hooks.fire("busy", json!({}));
        if let Some(steps) = self.script.as_ref().and_then(|s| s.steps_for(text)) {
            self.scripted(text, &steps);
            return;
        }
        if text.starts_with(ASK_PREFIX) {
            show(&format!("{ASKING}\n"));
            self.hooks.fire("needs-input", json!({ "prompt": ASKING }));
            self.asked = Some(text.to_owned());
            return;
        }
        self.work(text);
    }

    /// A scripted turn: its calls in order, then its reply (or `done: <text>`).
    fn scripted(&mut self, text: &str, steps: &[Step]) {
        let mut reply = format!("done: {text}");
        for step in steps {
            match step {
                Step::Call { tool, args } => {
                    show(&format!("{WORKING}\n"));
                    if let Some(script) = &mut self.script {
                        script.call(tool, args);
                    }
                }
                Step::Reply(text) => reply.clone_from(text),
            }
        }
        self.end_turn(&reply);
    }

    /// The rest of a turn: `working…` lines, a reply, `turn-end`, the prompt.
    fn work(&mut self, text: &str) {
        let length = if text.contains(LONG_WORD) {
            LONG_TURN
        } else if text.contains(SLOW_WORD) {
            SLOW_TURN
        } else {
            TURN
        };
        let start = Instant::now();
        while let Some(left) = length.checked_sub(start.elapsed()).filter(|l| !l.is_zero()) {
            show(&format!("{WORKING}\n"));
            thread::sleep(left.min(TICK));
        }
        self.end_turn(&format!("done: {text}"));
    }

    /// The reply, `turn-end` with it, and the prompt.
    fn end_turn(&self, reply: &str) {
        show(&format!("{reply}\n"));
        self.hooks.fire("turn-end", json!({ "last_reply": reply }));
        show(PROMPT);
    }
}

/// Writes to the terminal; a closed stdout is ignored (the agent keeps going).
fn show(text: &str) {
    let mut out = io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

/// Hook delivery, in order, on a helper thread.
struct Hooks {
    tx: Option<mpsc::Sender<(&'static str, Value)>>,
    thread: Option<JoinHandle<()>>,
}

impl Hooks {
    fn off() -> Self {
        Self {
            tx: None,
            thread: None,
        }
    }

    fn start(exe: PathBuf, agent: String) -> Self {
        let (tx, rx) = mpsc::channel::<(&'static str, Value)>();
        let spawned = thread::Builder::new()
            .name("bench-agent-hooks".to_owned())
            .spawn(move || {
                for (event, payload) in rx {
                    run_hook(&exe, &agent, event, &payload);
                }
            });
        match spawned {
            Ok(thread) => Self {
                tx: Some(tx),
                thread: Some(thread),
            },
            Err(_) => Self::off(),
        }
    }

    /// Queues `event`, stamping the payload with `sent_at_ms` now.
    fn fire(&self, event: &'static str, mut payload: Value) {
        let Some(tx) = &self.tx else { return };
        payload["sent_at_ms"] = json!(now_ms());
        let _ = tx.send((event, payload));
    }

    /// Delivers every queued hook, then returns.
    fn finish(mut self) {
        self.tx = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// `<exe> hook <agent> <event>` with the payload on stdin; the environment
/// (`DUOHIVE_TOKEN`, `DUOHIVE_DATA_DIR`) is inherited.
fn run_hook(exe: &Path, agent: &str, event: &str, payload: &Value) {
    let child = Command::new(exe)
        .args(["hook", agent, event])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(payload.to_string().as_bytes());
    }
    let _ = child.wait();
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}
