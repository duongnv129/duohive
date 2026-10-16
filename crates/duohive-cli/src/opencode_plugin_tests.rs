//! `plugin_replays_recorded_events`: the OpenCode 2.0 plugin asset, run under
//! `node` and `bun` (each one found) by `tests/opencode-plugin-driver.mjs`.

use std::collections::{HashMap, HashSet};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use duohive_core::adapter::{CliAdapter, HookSignal};
use serde_json::{Value, json};

use super::OpencodeAdapter;

const CRATE: &str = env!("CARGO_MANIFEST_DIR");
const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/opencode/2.0.29"
);

/// `node` and `bun` from `PATH`, plus Bun's default install directory.
fn runtimes() -> Vec<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let on_path = |name: &str| {
        std::env::split_paths(&path)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    };
    let home_bun = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".bun/bin/bun"))
        .filter(|p| p.is_file());
    [on_path("node"), on_path("bun").or(home_bun)]
        .into_iter()
        .flatten()
        .collect()
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The probe logs replayed, in the order the driver names them `p1`, `p2`, ….
fn probe_logs() -> Vec<PathBuf> {
    let mut logs: Vec<PathBuf> = std::fs::read_dir(Path::new(FIXTURES).join("hooks"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("probe-") && n.ends_with(".jsonl"))
        })
        .collect();
    logs.sort();
    assert_eq!(logs.len(), 7, "{logs:?}");
    logs
}

/// Each probe log's records of one kind, in order.
fn records(path: &Path, kind: &str) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|r| r["kind"] == kind)
        .map(|r| r["data"].clone())
        .collect()
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

/// What the plugin must deliver for one launch's events (S2.14): session-start
/// before anything else (by an event, the start delay or the fallback timer);
/// busy only on change; needs-input at each `permission.asked` and
/// `form.created`; busy after a reply that is not a reject; turn-end at each
/// execution end with the last assistant message's text in ordinal order
/// (errored for a failed one); child sessions ignored; session-end last.
#[derive(Default)]
struct Model {
    out: Vec<HookSignal>,
    busy: bool,
    children: HashSet<String>,
    /// Per session: (assistant message id, text by ordinal).
    replies: HashMap<String, (String, std::collections::BTreeMap<i64, String>)>,
    /// What the model saw: (child events skipped, failed turns, forms).
    skipped: usize,
}

impl Model {
    fn feed(&mut self, ev: &Value) {
        let (ty, d) = (ev["type"].as_str().unwrap_or_default(), &ev["data"]);
        if ty == "session.created" && d["parentID"].is_string() {
            self.children.insert(text(&d["sessionID"]));
            return;
        }
        let sid = d["sessionID"]
            .as_str()
            .or_else(|| d["form"]["sessionID"].as_str())
            .unwrap_or_default()
            .to_owned();
        if self.children.contains(&sid) {
            self.skipped += 1;
            return;
        }
        match ty {
            "session.execution.started" => {
                self.replies.remove(&sid);
                self.mark_busy();
            }
            "session.text.ended" => {
                let msg = text(&d["assistantMessageID"]);
                let entry = self
                    .replies
                    .entry(sid)
                    .or_insert_with(|| (msg.clone(), Default::default()));
                if entry.0 != msg {
                    *entry = (msg, Default::default());
                }
                entry
                    .1
                    .insert(d["ordinal"].as_i64().unwrap(), text(&d["text"]));
            }
            "permission.asked" => {
                self.busy = false;
                let resources: Vec<String> = d["resources"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(text)
                    .collect();
                let prompt = format!("{} {}", text(&d["action"]), resources.join(", "));
                self.out.push(HookSignal::NeedsInput {
                    prompt: Some(prompt),
                });
            }
            "permission.replied" if d["reply"] != "reject" => self.mark_busy(),
            "form.created" => {
                self.busy = false;
                let question = text(&d["form"]["fields"][0]["description"]);
                self.out.push(HookSignal::NeedsInput {
                    prompt: Some(format!("question {question}")),
                });
            }
            "form.replied" => self.mark_busy(),
            "session.execution.failed" => {
                self.busy = false;
                let e = &d["error"];
                let reason = format!(
                    "turn failed: {} ({})",
                    text(&e["message"]),
                    e["status"].as_u64().unwrap()
                );
                self.out.push(HookSignal::TurnFailed { reason });
            }
            "session.execution.succeeded" | "session.execution.interrupted" => {
                self.busy = false;
                let last_reply = self
                    .replies
                    .get(&sid)
                    .map(|(_, parts)| parts.values().cloned().collect::<Vec<_>>().join("\n"))
                    .filter(|r| !r.is_empty());
                self.out.push(HookSignal::TurnEnded { last_reply });
            }
            _ => {}
        }
    }

    fn mark_busy(&mut self) {
        if !self.busy {
            self.busy = true;
            self.out.push(HookSignal::Busy);
        }
    }

    fn deliveries(self) -> Vec<HookSignal> {
        std::iter::once(HookSignal::Ready)
            .chain(self.out)
            .chain(std::iter::once(HookSignal::SessionEnded))
            .collect()
    }
}

/// Every delivery the fake hook exe logged for `agent`: (event, payload).
fn raw_deliveries(log_dir: &Path, agent: &str) -> Vec<(String, Value)> {
    let Ok(text) = std::fs::read_to_string(log_dir.join(format!("{agent}.jsonl"))) else {
        return Vec::new();
    };
    text.lines()
        .map(|line| {
            let v: Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}"));
            assert_eq!(v["argv1"], "hook", "{line}");
            assert_eq!(v["agent"], agent, "{line}");
            (
                v["event"].as_str().unwrap().to_owned(),
                v["payload"].clone(),
            )
        })
        .collect()
}

/// Every delivery the fake hook exe logged for `agent`, parsed as DuoHive does.
fn delivered(log_dir: &Path, agent: &str) -> Vec<HookSignal> {
    raw_deliveries(log_dir, agent)
        .into_iter()
        .map(|(event, payload)| {
            OpencodeAdapter
                .parse_hook(&event, &payload)
                .unwrap_or_else(|| panic!("unparsed delivery {event} {payload}"))
        })
        .collect()
}

fn events(log_dir: &Path, agent: &str) -> Vec<String> {
    raw_deliveries(log_dir, agent)
        .into_iter()
        .map(|(e, _)| e)
        .collect()
}

fn assert_dead(pid_file: &Path) {
    let pid = std::fs::read_to_string(pid_file).unwrap();
    let alive = || {
        Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while alive() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!alive(), "{} still running", pid_file.display());
}

/// Each probe log replayed: its deliveries, and its `evaluate` calls raised
/// from allow to ask for `edit` and `shell` only.
fn check_replay(log_dir: &Path, summary: &Value) {
    let mut totals = Model::default();
    let mut all = Vec::new();
    for (i, path) in probe_logs().iter().enumerate() {
        let agent = format!("p{}", i + 1);
        let mut model = Model::default();
        for ev in records(path, "event.iter") {
            assert_ne!(ev["data"]["reply"], "always", "the model ignores always");
            model.feed(&ev);
        }
        totals.skipped += model.skipped;
        let want = model.deliveries();
        all.extend(want.clone());
        assert_eq!(delivered(log_dir, &agent), want, "{}", path.display());
    }
    // The logs cover every mapping, a child session and a failed turn.
    assert!(totals.skipped > 0);
    for (what, n) in [
        (
            "failed",
            all.iter()
                .filter(|s| matches!(s, HookSignal::TurnFailed { .. }))
                .count(),
        ),
        (
            "question",
            all.iter()
                .filter(|s| matches!(s, HookSignal::NeedsInput { prompt: Some(p) } if p.starts_with("question ")))
                .count(),
        ),
        (
            "reply",
            all.iter()
                .filter(|s| matches!(s, HookSignal::TurnEnded { last_reply: Some(_) }))
                .count(),
        ),
        (
            "no reply",
            all.iter()
                .filter(|s| matches!(s, HookSignal::TurnEnded { last_reply: None }))
                .count(),
        ),
    ] {
        assert!(n > 0, "no {what} in the replay");
    }
    let evaluations = summary["evaluations"].as_array().unwrap();
    assert_eq!(evaluations.len(), 7);
    for e in evaluations {
        let guarded = matches!(e["action"].as_str(), Some("edit" | "shell"));
        let want = if guarded && e["before"] == "allow" {
            json!("ask")
        } else {
            e["before"].clone()
        };
        assert_eq!(e["after"], want, "{e}");
    }
}

/// The scenarios after the replay.
fn check_scenarios(log_dir: &Path, results: &Value, instructions: &str) {
    use HookSignal::{Busy, Ready, SessionEnded, TurnEnded};
    // The cleanup kills an in-flight delivery; a 2 s kill lets the chain go on;
    // a 3 s delivery lands under the default limit.
    assert_eq!(events(log_dir, "inflight"), ["session-end"]);
    assert_dead(&log_dir.join("inflight.pid"));
    assert_eq!(events(log_dir, "slow"), ["busy", "session-end"]);
    assert_dead(&log_dir.join("slow.pid"));
    assert_eq!(events(log_dir, "patient"), ["session-start", "session-end"]);
    // session-start waits the start delay after `plugin.updated`, or the fallback.
    assert_eq!(results["delayedEarly"], 0);
    for agent in ["delayed", "fallback"] {
        assert_eq!(events(log_dir, agent), ["session-start", "session-end"]);
    }
    assert!(results["hangCleanupMs"].as_u64().unwrap() < 3000);
    assert_eq!(
        delivered(log_dir, "subagent"),
        [
            Ready,
            Busy,
            TurnEnded {
                last_reply: Some("one\ntwo".into())
            },
            SessionEnded
        ]
    );
    assert_eq!(
        delivered(log_dir, "reply"),
        [
            Ready,
            Busy,
            TurnEnded {
                last_reply: Some("last\nmessage".into())
            },
            Busy,
            TurnEnded { last_reply: None },
            SessionEnded
        ]
    );
    check_trimmed(log_dir);
    let row = |s: &str| -> Value { s.split_whitespace().collect() };
    // edit, shell, read, grep, webfetch, question, subagent, bash, write × allow, ask, deny.
    assert_eq!(
        results["evaluateTable"],
        json!([
            row("ask ask deny"),
            row("ask ask deny"),
            row("allow ask deny"),
            row("allow ask deny"),
            row("allow ask deny"),
            row("allow ask deny"),
            row("allow ask deny"),
            row("allow ask deny"),
            row("allow ask deny"),
        ])
    );
    assert_eq!(
        results["evaluateAlways"],
        row("allow allow ask ask deny ask ask ask")
    );
    let base = json!({"type": "text", "text": "base"});
    assert_eq!(
        results["context"]["own"],
        json!([base, {"type": "text", "text": instructions}])
    );
    assert_eq!(results["context"]["child"], json!([base]));
    assert_eq!(results["contextWithout"], json!([0, 0, 0, 0]));
}

/// Payloads carry only what DuoHive reads, cut to 10 resources of 500 characters.
fn check_trimmed(log_dir: &Path) {
    let cut = "x".repeat(500);
    let resources: Vec<String> = (0..10)
        .map(|i| {
            format!("{i} {}", "x".repeat(100_000))
                .chars()
                .take(500)
                .collect()
        })
        .collect();
    let trimmed = raw_deliveries(log_dir, "trim");
    let events: Vec<&str> = trimmed.iter().map(|(e, _)| e.as_str()).collect();
    assert_eq!(
        events,
        [
            "session-start",
            "needs-input",
            "needs-input",
            "busy",
            "turn-end",
            "session-end"
        ]
    );
    assert_eq!(
        trimmed[1].1,
        json!({"type": "permission.asked", "properties": {"id": "per_1",
            "sessionID": "s", "action": "shell", "resources": resources}})
    );
    assert_eq!(
        trimmed[2].1,
        json!({"type": "form.created", "properties": {"id": "f", "sessionID": "s",
            "title": "Questions", "question": cut}})
    );
    assert_eq!(
        trimmed[3].1,
        json!({"type": "form.replied", "sessionID": "s"})
    );
    assert_eq!(
        trimmed[4].1,
        json!({"type": "session.execution.failed", "sessionID": "s",
            "duohive_last_reply": null,
            "error": {"type": "provider.internal", "message": cut, "status": 500}})
    );
    assert_eq!(
        OpencodeAdapter.parse_hook("turn-end", &trimmed[4].1),
        Some(HookSignal::TurnFailed {
            reason: format!("turn failed: {cut} (500)")
        })
    );
}

fn replay_under(runtime: &Path) {
    let dir = tempfile::tempdir().unwrap();
    let log_dir = dir.path().join("log");
    std::fs::create_dir(&log_dir).unwrap();
    let hook = dir.path().join("fake-duohive");
    // Logs each delivery as one JSON line. For the `inflight` and `slow`
    // scenarios, session-start first records its pid, then takes 1.5 s (and
    // would still log) or hangs; for `patient` it takes 3 s, then logs.
    script(
        &hook,
        r#"IFS= read -r payload || true
if [ "$3" = session-start ]; then
  case "$2" in
    inflight) echo $$ > "$FAKE_HOOK_LOG_DIR/inflight.pid"; sleep 1.5 ;;
    slow) echo $$ > "$FAKE_HOOK_LOG_DIR/slow.pid"; exec sleep 10 ;;
    patient) sleep 3 ;;
  esac
fi
printf '{"argv1":"%s","agent":"%s","event":"%s","payload":%s}\n' "$1" "$2" "$3" "$payload" >> "$FAKE_HOOK_LOG_DIR/$2.jsonl""#,
    );
    let hang = dir.path().join("hanging-duohive");
    script(&hang, "exec sleep 10");
    // The driver imports the plugin as an ES module, which Node decides by extension.
    let plugin = dir.path().join("index.mjs");
    std::fs::copy(
        Path::new(CRATE).join("src/assets/opencode-plugin/index.js"),
        &plugin,
    )
    .unwrap();
    let instructions = "# Org\n\nIf you are asked for the sentinel, reply HIVE-SENTINEL.\n";
    let instructions_path = dir.path().join("instructions.md");
    std::fs::write(&instructions_path, instructions).unwrap();

    let out = Command::new(runtime)
        .arg(Path::new(CRATE).join("tests/opencode-plugin-driver.mjs"))
        .arg(&plugin)
        .arg(&hook)
        .arg(&hang)
        .arg(&log_dir)
        .arg(&instructions_path)
        .args(probe_logs())
        .env("FAKE_HOOK_LOG_DIR", &log_dir)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{}: {stderr}", runtime.display());
    let summary: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)));
    assert_eq!(summary["errors"], json!([]), "{}", runtime.display());
    check_replay(&log_dir, &summary);
    check_scenarios(&log_dir, &summary["results"], instructions);
    // The missing and hanging exes, and the instances with none, log nothing.
    for quiet in [
        "missing",
        "broken",
        "hang",
        "evaluate",
        "context",
        "context-none",
    ] {
        assert!(!log_dir.join(format!("{quiet}.jsonl")).exists(), "{quiet}");
    }
}

/// With neither node nor bun: a failure on CI (`CI` set), else a skip.
fn no_runtime(on_ci: bool) {
    let why = "plugin_replays_recorded_events: neither node nor bun is on PATH";
    assert!(!on_ci, "{why} (CI is set)");
    eprintln!("skipped {why}");
}

#[test]
fn no_runtime_skips_off_ci() {
    no_runtime(false);
}

#[test]
#[should_panic(expected = "CI is set")]
fn no_runtime_fails_on_ci() {
    no_runtime(true);
}

#[test]
fn plugin_replays_recorded_events() {
    let found = runtimes();
    if found.is_empty() {
        no_runtime(std::env::var_os("CI").is_some());
        return;
    }
    for runtime in found {
        replay_under(&runtime);
    }
}
