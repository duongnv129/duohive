//! `bench-agent --mcp-script <path>`: a scripted fake Duo for the end-to-end
//! tests (Task 3.12). No model is called.
//!
//! The script is a JSON list of `{"on": "<text>", "steps": [...]}`. A turn
//! runs the first entry whose `on` is a substring of its text; each step is
//! `{"call": "<tool>", "args": {…}}`, made through a `duohive mcp` child (the
//! helper an agent's CLI runs, with this launch's `DUOHIVE_TOKEN`), or
//! `{"reply": "<text>"}`, the turn's reply. Each call's answer is appended to
//! `<data dir>/run/<agent>/calls.jsonl`; an argument string
//! `$result.<n>.<field>` is the `field` of the n-th answer (from 1).

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

/// What a `$result.<n>.<field>` argument starts with.
const RESULT_REF: &str = "$result.";
/// The MCP version the helper is asked for.
const PROTOCOL: &str = "2025-06-18";

/// One step of a scripted turn.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Step {
    /// A tool call.
    Call {
        /// The tool.
        tool: String,
        /// Its arguments.
        args: Value,
    },
    /// The turn's reply.
    Reply(String),
}

/// One entry: the turn text it answers and its steps.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    on: String,
    steps: Vec<Step>,
}

/// A loaded script with its helper and the answers so far.
pub(super) struct Script {
    entries: Vec<Entry>,
    mcp: Result<Mcp, String>,
    answers: Vec<Value>,
    log: Option<PathBuf>,
}

impl Script {
    /// Loads `path` and starts `<exe> mcp`; `env` names the agent and data dir
    /// for the log. A helper that can't start makes every call answer an error.
    pub(super) fn load(
        path: &Path,
        exe: Option<&Path>,
        env: &HashMap<String, String>,
    ) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let entries = parse(&text)?;
        let mcp = exe
            .ok_or_else(|| "no executable".to_owned())
            .and_then(Mcp::start);
        let log = match (env.get("DUOHIVE_DATA_DIR"), env.get("DUOHIVE_AGENT")) {
            (Some(data), Some(agent)) => {
                Some(Path::new(data).join("run").join(agent).join("calls.jsonl"))
            }
            _ => None,
        };
        Ok(Self {
            entries,
            mcp,
            answers: Vec::new(),
            log,
        })
    }

    /// The steps of the first entry whose `on` is in `text`.
    pub(super) fn steps_for(&self, text: &str) -> Option<Vec<Step>> {
        self.entries
            .iter()
            .find(|e| text.contains(&e.on))
            .map(|e| e.steps.clone())
    }

    /// Makes one call; its answer is kept and logged.
    pub(super) fn call(&mut self, tool: &str, args: &Value) -> Value {
        let args = resolve(args, &self.answers);
        let answer = match &mut self.mcp {
            Ok(mcp) => mcp.call(tool, &args),
            Err(e) => json!({ "error": e }),
        };
        if let Some(log) = &self.log {
            append(log, &answer);
        }
        self.answers.push(answer.clone());
        answer
    }
}

/// The script's entries; every entry needs `on` and `steps`.
fn parse(text: &str) -> Result<Vec<Entry>, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let entries = value.as_array().ok_or("the script is not a list")?;
    entries
        .iter()
        .map(|e| {
            let on = e["on"].as_str().ok_or("an entry has no \"on\"")?.to_owned();
            let steps = e["steps"]
                .as_array()
                .ok_or("an entry has no \"steps\"")?
                .iter()
                .map(step)
                .collect::<Result<_, _>>()?;
            Ok(Entry { on, steps })
        })
        .collect()
}

fn step(v: &Value) -> Result<Step, String> {
    if let Some(reply) = v["reply"].as_str() {
        return Ok(Step::Reply(reply.to_owned()));
    }
    let tool = v["call"]
        .as_str()
        .ok_or("a step is neither call nor reply")?;
    let args = match v.get("args") {
        Some(args) => args.clone(),
        None => json!({}),
    };
    Ok(Step::Call {
        tool: tool.to_owned(),
        args,
    })
}

/// `args` with every `$result.<n>.<field>` string replaced by that answer's
/// value; one that names nothing is left as written.
fn resolve(args: &Value, answers: &[Value]) -> Value {
    match args {
        Value::String(s) => lookup(s, answers).unwrap_or_else(|| args.clone()),
        Value::Array(items) => Value::Array(items.iter().map(|v| resolve(v, answers)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), resolve(v, answers)))
                .collect(),
        ),
        _ => args.clone(),
    }
}

fn lookup(text: &str, answers: &[Value]) -> Option<Value> {
    let mut path = text.strip_prefix(RESULT_REF)?.split('.');
    let n: usize = path.next()?.parse().ok()?;
    let mut value = answers.get(n.checked_sub(1)?)?;
    for field in path {
        value = value.get(field)?;
    }
    Some(value.clone())
}

/// Appends one line; a log that can't be written is skipped.
fn append(log: &Path, answer: &Value) {
    if let Some(dir) = log.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log) {
        let _ = writeln!(file, "{answer}");
    }
}

/// The `duohive mcp` child, spoken to one JSON-RPC line at a time.
struct Mcp {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Mcp {
    /// Starts the helper (the environment is inherited) and initializes it.
    fn start(exe: &Path) -> Result<Self, String> {
        let mut child = Command::new(exe)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("duohive mcp: {e}"))?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("duohive mcp: no pipes".to_owned());
        };
        let mut mcp = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        };
        mcp.ask("initialize", &json!({ "protocolVersion": PROTOCOL }))?;
        Ok(mcp)
    }

    /// `tools/call`: the structured answer, or `{"error": …}`.
    fn call(&mut self, tool: &str, args: &Value) -> Value {
        let params = json!({ "name": tool, "arguments": args });
        match self.ask("tools/call", &params) {
            Ok(answer) => match answer["result"].get("structuredContent") {
                Some(content) => content.clone(),
                None => answer,
            },
            Err(e) => json!({ "error": e }),
        }
    }

    fn ask(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        writeln!(self.stdin, "{request}").map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        match self.stdout.read_line(&mut line) {
            Ok(0) => Err("duohive mcp ended".to_owned()),
            Ok(_) => serde_json::from_str(&line).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_parses_calls_and_replies() {
        let text = r#"[{"on": "Story:", "steps": [
            {"call": "list_agents"},
            {"call": "stop_agent", "args": {"agent": "squad-1"}},
            {"reply": "done"}]}]"#;
        let entries = parse(text).unwrap();
        assert_eq!(entries[0].on, "Story:");
        assert_eq!(
            entries[0].steps,
            vec![
                Step::Call {
                    tool: "list_agents".into(),
                    args: json!({})
                },
                Step::Call {
                    tool: "stop_agent".into(),
                    args: json!({"agent": "squad-1"})
                },
                Step::Reply("done".into()),
            ]
        );
        assert!(parse(r#"[{"steps": []}]"#).is_err());
        assert!(parse(r#"[{"on": "x", "steps": [{"wait": 1}]}]"#).is_err());
        assert!(parse("{}").is_err());
    }

    #[test]
    fn first_matching_entry_runs() {
        let script = Script {
            entries: parse(
                r#"[{"on": "fix it", "steps": [{"reply": "a"}]},
                    {"on": "it", "steps": [{"reply": "b"}]}]"#,
            )
            .unwrap(),
            mcp: Err("none".into()),
            answers: Vec::new(),
            log: None,
        };
        assert_eq!(
            script.steps_for("please fix it now"),
            Some(vec![Step::Reply("a".into())])
        );
        assert_eq!(
            script.steps_for("is it?"),
            Some(vec![Step::Reply("b".into())])
        );
        assert_eq!(script.steps_for("nothing"), None);
    }

    #[test]
    fn result_refs_take_earlier_answers() {
        let answers = [
            json!({"status": "pending", "card_id": 3}),
            json!({"agent": "squad-1"}),
        ];
        let args = json!({"card": "$result.1.card_id", "agent": "$result.2.agent",
                          "keep": "$result.9.agent", "text": "say $result.1.card_id"});
        assert_eq!(
            resolve(&args, &answers),
            json!({"card": 3, "agent": "squad-1", "keep": "$result.9.agent",
                   "text": "say $result.1.card_id"})
        );
        assert_eq!(
            resolve(&json!("$result.0.x"), &answers),
            json!("$result.0.x")
        );
    }

    #[test]
    fn calls_without_a_helper_answer_an_error_and_are_logged() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("run/Duo/calls.jsonl");
        let mut script = Script {
            entries: Vec::new(),
            mcp: Err("no helper".into()),
            answers: Vec::new(),
            log: Some(log.clone()),
        };
        assert_eq!(
            script.call("list_agents", &json!({})),
            json!({"error": "no helper"})
        );
        assert_eq!(
            fs::read_to_string(log).unwrap(),
            "{\"error\":\"no helper\"}\n"
        );
    }
}
