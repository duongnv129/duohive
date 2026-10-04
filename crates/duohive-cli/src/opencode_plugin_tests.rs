//! `plugin_replays_recorded_events`: the OpenCode plugin asset, run under
//! `node` and `bun` (each one found) by `tests/opencode-plugin-driver.mjs`.

use std::collections::VecDeque;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use duohive_core::adapter::{CliAdapter, HookSignal};
use serde_json::{Value, json};

use super::OpencodeAdapter;

const CRATE: &str = env!("CARGO_MANIFEST_DIR");
const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/opencode/1.18.34"
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

fn reply_file(rel: &str) -> Option<String> {
    let text = std::fs::read_to_string(Path::new(FIXTURES).join(rel)).unwrap();
    let text = text.strip_suffix('\n').unwrap_or(&text).to_owned();
    (text != "(null)").then_some(text)
}

/// The replies of the recorded turns, in event-log order (S2.4).
fn recorded_replies() -> VecDeque<Option<String>> {
    let mut replies: VecDeque<_> = (1..=2)
        .map(|n| reply_file(&format!("replies/a1-turn-{n}.from-events.txt")))
        .chain((1..=5).map(|n| {
            reply_file(&format!(
                "replies/session.idle.reply-{n}.duohive_last_reply.txt"
            ))
        }))
        .collect();
    for n in 1..=2 {
        let path = Path::new(FIXTURES).join(format!("hooks/review-session.idle.reply-{n}.json"));
        let payload: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        replies.push_back(payload["duohive_last_reply"].as_str().map(str::to_owned));
    }
    replies
}

/// What each plugin instance must deliver for the events the driver fed it:
/// session-start first (on the first `catalog.updated` or `session.created`,
/// else by the shortened timer), busy only on change, needs-input at each
/// `permission.asked`, busy after each `permission.replied`, turn-end at each
/// `session.idle` with the recorded reply, and session-end last.
fn expected(feed: &[Value]) -> Vec<(String, (String, HookSignal))> {
    #[derive(Default)]
    struct Seen {
        started: bool,
        busy: bool,
    }
    let mut replies = recorded_replies();
    let mut seen: std::collections::HashMap<String, Seen> = Default::default();
    let mut out = Vec::new();
    for item in feed {
        let agent = item["agent"].as_str().unwrap().to_owned();
        let s = seen.entry(agent.clone()).or_default();
        let mut push = |e: &str, sig: HookSignal| out.push((agent.clone(), (e.to_owned(), sig)));
        match item["type"].as_str().unwrap() {
            "catalog.updated" | "session.created" if !s.started => {
                s.started = true;
                push("session-start", HookSignal::Ready);
            }
            "session.status" if item["status"] == "busy" && !s.busy => {
                s.busy = true;
                push("busy", HookSignal::Busy);
            }
            "permission.replied" if !s.busy => {
                s.busy = true;
                push("busy", HookSignal::Busy);
            }
            "permission.asked" => {
                s.busy = false;
                let p = &item["properties"];
                let prompt = format!(
                    "{} {}",
                    p["permission"].as_str().unwrap(),
                    p["metadata"]["filepath"].as_str().unwrap()
                );
                push(
                    "needs-input",
                    HookSignal::NeedsInput {
                        prompt: Some(prompt),
                    },
                );
            }
            "session.idle" => {
                s.busy = false;
                let last_reply = replies.pop_front().unwrap();
                push("turn-end", HookSignal::TurnEnded { last_reply });
            }
            "dispose" => {
                if !s.started {
                    s.started = true;
                    push("session-start", HookSignal::Ready);
                }
                push("session-end", HookSignal::SessionEnded);
            }
            _ => {}
        }
    }
    assert!(replies.is_empty(), "unused replies {replies:?}");
    out
}

/// Every delivery the fake hook exe logged for `agent`, parsed as DuoHive does.
fn delivered(log_dir: &Path, agent: &str) -> Vec<(String, HookSignal)> {
    let Ok(text) = std::fs::read_to_string(log_dir.join(format!("{agent}.jsonl"))) else {
        return Vec::new();
    };
    text.lines()
        .map(|line| {
            let v: Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}"));
            assert_eq!(v["argv1"], "hook", "{line}");
            assert_eq!(v["agent"], agent, "{line}");
            let event = v["event"].as_str().unwrap().to_owned();
            let signal = OpencodeAdapter
                .parse_hook(&event, &v["payload"])
                .unwrap_or_else(|| panic!("unparsed delivery {line}"));
            (event, signal)
        })
        .collect()
}

fn composed_configs() -> Vec<Value> {
    vec![
        json!({"permission": {"edit": "ask", "bash": "ask"}}),
        json!({"permission": {"*": "allow", "edit": "ask", "bash": "ask"}}),
        json!({"permission": "deny"}),
        json!({"permission": {"*": "ask", "edit": "ask"}}),
        json!({
            "permission": {
                "edit": "deny",
                "bash": {"*": "ask", "rm *": "deny", "git push*": "ask"},
                "webfetch": "allow",
            },
            "agent": {
                "build": {"permission": {"edit": "ask", "bash": {"git commit*": "deny"}}},
                "plan": {"permission": {"*": "allow", "edit": "ask", "bash": "ask"}},
                "review": {"model": "x"},
                "docs": {"permission": {"edit": {"*": "ask", "docs/**": "allow"}}},
            },
        }),
    ]
}

fn replay_under(runtime: &Path) {
    let dir = tempfile::tempdir().unwrap();
    let log_dir = dir.path().join("log");
    std::fs::create_dir(&log_dir).unwrap();
    let hook = dir.path().join("fake-duohive");
    script(
        &hook,
        r#"payload=$(cat)
printf '{"argv1":"%s","agent":"%s","event":"%s","payload":%s}\n' "$1" "$2" "$3" "$payload" >> "$FAKE_HOOK_LOG_DIR/$2.jsonl""#,
    );
    let hang = dir.path().join("hanging-duohive");
    script(&hang, "exec sleep 10");
    // The driver imports the plugin as an ES module, which Node decides by extension.
    let plugin = dir.path().join("opencode-plugin.mjs");
    std::fs::copy(
        Path::new(CRATE).join("src/assets/opencode-plugin.js"),
        &plugin,
    )
    .unwrap();

    let out = Command::new(runtime)
        .arg(Path::new(CRATE).join("tests/opencode-plugin-driver.mjs"))
        .arg(&plugin)
        .arg(&hook)
        .arg(&hang)
        .arg(&log_dir)
        .arg(Path::new(FIXTURES).join("hooks/events.jsonl"))
        .arg(Path::new(FIXTURES).join("hooks/review-events.jsonl"))
        .env("FAKE_HOOK_LOG_DIR", &log_dir)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{}: {stderr}", runtime.display());
    let summary: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)));
    assert_eq!(summary["errors"], json!([]), "{}", runtime.display());
    assert_eq!(summary["composed"], json!(composed_configs()));

    let feed = summary["feed"].as_array().unwrap();
    let want = expected(feed);
    let mut agents: Vec<&str> = feed.iter().map(|i| i["agent"].as_str().unwrap()).collect();
    agents.dedup();
    // events.jsonl: a1, a2, the `mcp list` run, a3, a4; review-events.jsonl: a5, a6, a7.
    let mut unique = agents.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 8, "{unique:?}");
    for agent in unique {
        let want: Vec<_> = want
            .iter()
            .filter(|(a, _)| a == agent)
            .map(|(_, d)| d.clone())
            .collect();
        assert_eq!(
            delivered(&log_dir, agent),
            want,
            "{agent} under {}",
            runtime.display()
        );
    }
    // The config instance has no hook exe; the missing and hanging exes log nothing.
    for quiet in ["config", "missing", "hang"] {
        assert!(!log_dir.join(format!("{quiet}.jsonl")).exists(), "{quiet}");
    }
}

#[test]
fn plugin_replays_recorded_events() {
    let found = runtimes();
    if found.is_empty() {
        eprintln!("skipped plugin_replays_recorded_events: neither node nor bun is on PATH");
        return;
    }
    for runtime in found {
        replay_under(&runtime);
    }
}
