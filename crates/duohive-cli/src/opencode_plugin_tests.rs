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

/// The decision OpenCode reaches (last match wins, built-in agent rules
/// before the user's) for each of the driver's queries, per sample user
/// config, once the plugin's `config` hook has run. Queries, in order:
/// build: edit a.txt, edit src/x.ts, bash ls, bash `rm -rf x`, bash
/// `git push`, bash `git commit -m x`, webfetch; plan: edit a.txt, edit
/// .opencode/plans/p.md, bash `rm -rf x`; docs (a user agent in one sample,
/// unknown otherwise): edit a.txt, edit docs/a.md; explore: edit a.txt, bash
/// ls; general: edit a.txt; title: bash ls; build: edit 5; plan: edit 5.
fn expected_decisions() -> Value {
    fn row(s: &str) -> Vec<&str> {
        s.split_whitespace().collect()
    }
    json!([
        // {}: plan keeps its built-in edit deny (and plan files), explore and
        // title keep theirs; an agent DuoHive does not know (docs) is untouched.
        row(
            "ask ask ask ask ask ask allow  deny allow ask  allow allow  deny ask  ask  deny  ask deny"
        ),
        // "permission": "allow" (overrides the built-ins, so everything asks)
        row("ask ask ask ask ask ask allow  ask ask ask  allow allow  ask ask  ask  ask  ask ask"),
        // "permission": "deny" stays deny everywhere
        row(
            "deny deny deny deny deny deny deny  deny deny deny  deny deny  deny deny  deny  deny  deny deny"
        ),
        // {"*": "ask", "edit": "allow"}
        row("ask ask ask ask ask ask ask  ask ask ask  allow allow  ask ask  ask  ask  ask ask"),
        // the user's deny and patterns kept, their allows made ask
        row(
            "ask ask ask deny ask deny allow  ask ask ask  ask allow  deny ask  deny  ask  ask ask"
        ),
        // {"edit": "allow", "*": "allow"}: "*" last must not win
        row("ask ask ask ask ask ask allow  ask ask ask  allow allow  ask ask  ask  ask  ask ask"),
        // {"bash": {"git *": "allow"}, "*": "allow"}: the later "*" overrode
        // the user's pattern, so it asks too
        row("ask ask ask ask ask ask allow  ask ask ask  allow allow  ask ask  ask  ask  ask ask"),
        // agent build {"bash": "allow", "*": "allow"}
        row(
            "ask ask ask ask ask ask allow  deny allow ask  allow allow  deny ask  ask  deny  ask deny"
        ),
        // {"edit": {"src/**": "deny"}}: the added "*" does not override it
        row(
            "ask deny ask ask ask ask allow  deny allow ask  allow allow  deny ask  ask  deny  ask deny"
        ),
        // {"edit": "ask", "*": "deny"}: the later deny still decides
        row(
            "deny deny deny deny deny deny deny  deny deny deny  deny deny  deny deny  deny  deny  deny deny"
        ),
        // {"bash": "allow", "*": "deny"}
        row(
            "deny deny deny deny deny deny deny  deny deny deny  deny deny  deny deny  deny  deny  deny deny"
        ),
        // agent plan {"edit": "allow", "*": "deny"}
        row(
            "ask ask ask ask ask ask allow  deny deny deny  allow allow  deny ask  ask  deny  ask deny"
        ),
        // {"edit": {"*": "ask"}, "*": "deny"}
        row(
            "deny deny deny deny deny deny deny  deny deny deny  deny deny  deny deny  deny  deny  deny deny"
        ),
        // {"edit": "allow"}: the user's allow overrode plan's deny; now it asks
        row("ask ask ask ask ask ask allow  ask ask ask  allow allow  ask ask  ask  deny  ask ask"),
        // {"edit": {"5": "deny"}}: "5" stays denied; the "*" that would come
        // after it in the JS object is raised to keep it, so build's edits deny
        row(
            "deny deny ask ask ask ask allow  deny allow ask  allow allow  deny ask  deny  deny  deny deny"
        ),
        // top {"edit": {"?": "allow"}}, build {"edit": {"5": "deny"}}: "5" stays
        // denied for build ("?" raised); plan keeps the user's "?" allow
        row(
            "ask ask ask ask ask ask allow  deny allow ask  allow allow  deny ask  ask  deny  deny allow"
        ),
    ])
}

/// The composition only ever tightens: for every sample and every edit or
/// bash query, the decision after is at least as strict as before.
fn assert_only_tightens(summary: &Value) {
    let rank = |d: &Value| match d.as_str() {
        Some("allow") => 0,
        Some("ask") => 1,
        Some("deny") => 2,
        other => panic!("decision {other:?}"),
    };
    let queries = summary["queries"].as_array().unwrap();
    let before = summary["before"].as_array().unwrap();
    let after = summary["decisions"].as_array().unwrap();
    assert_eq!(before.len(), after.len());
    for (sample, (b, a)) in before.iter().zip(after).enumerate() {
        for (q, query) in queries.iter().enumerate() {
            if !matches!(query[1].as_str(), Some("edit" | "bash")) {
                assert_eq!(a[q], b[q], "sample {sample} {query} changed");
                continue;
            }
            assert!(
                rank(&a[q]) >= rank(&b[q]),
                "sample {sample} {query}: {} -> {}",
                b[q],
                a[q]
            );
        }
    }
}

/// Each delivery the fake hook exe logged for `agent`: (event, payload).
fn raw_deliveries(log_dir: &Path, agent: &str) -> Vec<(String, Value)> {
    let Ok(text) = std::fs::read_to_string(log_dir.join(format!("{agent}.jsonl"))) else {
        return Vec::new();
    };
    text.lines()
        .map(|line| {
            let v: Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}"));
            (
                v["event"].as_str().unwrap().to_owned(),
                v["payload"].clone(),
            )
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

/// The scenarios after the replay: dispose kills an in-flight delivery,
/// the chain goes on after a 2 s kill, subagent sessions are ignored, and
/// payloads are trimmed.
fn check_scenarios(log_dir: &Path) {
    assert_eq!(events(log_dir, "inflight"), ["session-end"]);
    assert_dead(&log_dir.join("inflight.pid"));
    assert_eq!(events(log_dir, "slow"), ["busy", "session-end"]);
    assert_dead(&log_dir.join("slow.pid"));
    assert_eq!(
        events(log_dir, "subagent"),
        ["session-start", "busy", "turn-end", "session-end"]
    );
    let trim = raw_deliveries(log_dir, "trim");
    let trimmed: Vec<_> = trim.iter().map(|(e, p)| (e.as_str(), p.clone())).collect();
    assert_eq!(
        trimmed,
        [
            (
                "busy",
                json!({"input": {"tool": "write", "sessionID": "s"}})
            ),
            (
                "needs-input",
                json!({"type": "permission.asked", "properties": {"id": "per_1",
                    "sessionID": "s", "permission": "edit", "patterns": ["a.txt"],
                    "metadata": {"filepath": "/w/a.txt"}}})
            ),
            ("session-start", json!({})),
            ("busy", json!({"input": {"sessionID": "s"}})),
            ("session-end", json!({})),
        ]
    );
    assert_eq!(
        OpencodeAdapter.parse_hook("needs-input", &trim[1].1),
        Some(HookSignal::NeedsInput {
            prompt: Some("edit /w/a.txt".into())
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
    // would still log) or hangs.
    script(
        &hook,
        r#"IFS= read -r payload || true
if [ "$3" = session-start ]; then
  case "$2" in
    inflight) echo $$ > "$FAKE_HOOK_LOG_DIR/inflight.pid"; sleep 1.5 ;;
    slow) echo $$ > "$FAKE_HOOK_LOG_DIR/slow.pid"; exec sleep 10 ;;
  esac
fi
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
    assert_only_tightens(&summary);
    assert_eq!(
        summary["decisions"],
        expected_decisions(),
        "{}",
        runtime.display()
    );

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
    check_scenarios(&log_dir);
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
