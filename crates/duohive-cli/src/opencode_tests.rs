//! Tests for the OpenCode adapter against spike S2's 2.0.29 fixtures (S2.14).

use std::path::{Path, PathBuf};

use duohive_core::adapter::{CliAdapter, HookCoverage, HookSignal, LaunchError, ScreenState};
use duohive_core::cli_kind::CliKind;
use duohive_core::launch::{Key, LaunchCommand, LaunchFile, LaunchSpec};
use duohive_core::testkit::{example_org, n};
use serde_json::Value;

use super::*;

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/opencode/2.0.29"
);

fn fixture(rel: &str) -> String {
    std::fs::read_to_string(Path::new(FIXTURES).join(rel)).unwrap()
}

/// A reply fixture; an empty one is no reply.
fn reply_fixture(rel: &str) -> Option<String> {
    let text = fixture(rel);
    (!text.is_empty()).then_some(text)
}

const RUN_DIR: &str = "/data/duohive/run/squad-1";

fn spec(task: Option<&str>) -> LaunchSpec {
    let mut agent = duohive_core::inherit::resolve(&example_org(), &n("squad-1")).unwrap();
    agent.cli = CliKind::Opencode;
    agent.directory = Some("~/code/payments".into());
    agent.prompt = "# Org\n\nIf you are asked for the sentinel, reply HIVE-SENTINEL.".into();
    LaunchSpec {
        agent,
        task: task.map(str::to_owned),
        run_dir: RUN_DIR.into(),
        hook_exe: "/usr/local/bin/duohive".into(),
        home: "/home/u".into(),
        cwd: None,
    }
}

fn launch(spec: &LaunchSpec) -> LaunchCommand {
    OpencodeAdapter.launch(spec).unwrap()
}

fn file<'a>(cmd: &'a LaunchCommand, name: &str) -> &'a LaunchFile {
    cmd.files
        .iter()
        .find(|f| f.path.file_name().and_then(|s| s.to_str()) == Some(name))
        .unwrap_or_else(|| panic!("no {name} in {:?}", cmd.files))
}

fn config_json(cmd: &LaunchCommand) -> Value {
    serde_json::from_str(&file(cmd, "opencode.json").contents).unwrap()
}

fn env<'a>(cmd: &'a LaunchCommand, key: &str) -> Option<&'a str> {
    cmd.env
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// The launch with the plugin's source replaced by a marker, so snapshots
/// show the command and config, not the whole asset (checked separately).
fn for_snapshot(mut cmd: LaunchCommand) -> LaunchCommand {
    for f in &mut cmd.files {
        if f.path.ends_with("opencode-plugin/index.js") {
            assert_eq!(f.contents, PLUGIN_SOURCE);
            f.contents = "<src/assets/opencode-plugin/index.js>".into();
        }
    }
    cmd
}

#[test]
fn launch_snapshot_no_task() {
    let cmd = launch(&spec(None));
    assert_eq!(cmd.program, "opencode");
    assert_eq!(cmd.cwd, PathBuf::from("/home/u/code/payments"));
    // Without `--standalone` the TUI uses the shared service, which never reads
    // OPENCODE_CONFIG, so every hook is lost (S2.14).
    assert_eq!(cmd.args, ["--standalone"]);
    let config = config_json(&cmd);
    // Approvals are raised by the plugin's `evaluate` hook, never a static key.
    assert!(config.get("permissions").is_none(), "{config}");
    assert!(PLUGIN_SOURCE.contains("\"evaluate\""));
    insta::assert_yaml_snapshot!(for_snapshot(cmd));
}

#[test]
fn launch_snapshot_with_task_model_effort_flags() {
    let mut s = spec(Some("Add retry cases"));
    s.agent.model = Some("anthropic/claude-opus".into());
    s.agent.effort = Some("high".into());
    s.agent.flags = vec!["--verbose".into()];
    let cmd = launch(&s);
    assert_eq!(
        cmd.args,
        ["--standalone", "--verbose", "--prompt=Add retry cases"]
    );
    // 2.0 rejects `--model` and `--agent` (S2.14): both go in the config.
    assert!(
        !cmd.args
            .iter()
            .any(|a| a.starts_with("--model") || a.starts_with("--agent"))
    );
    let config = config_json(&cmd);
    assert_eq!(config["model"], "anthropic/claude-opus#high");
    assert_eq!(config["default_agent"], "build");
    insta::assert_yaml_snapshot!(for_snapshot(cmd));
}

#[test]
fn launch_snapshot_hostile_text() {
    let task = "-rf $(id) `x` it's\nline2 é";
    let cmd = launch(&spec(Some(task)));
    assert_eq!(
        cmd.args.last().map(String::as_str),
        Some(format!("--prompt={task}").as_str())
    );
    // The positional argument is the project directory, so no `--` (S2.8).
    assert!(!cmd.args.contains(&"--".to_owned()));
    insta::assert_yaml_snapshot!(for_snapshot(cmd));
}

#[test]
fn launch_passes_the_instructions_file_to_the_plugin_and_omits_an_empty_one() {
    let s = spec(None);
    let cmd = launch(&s);
    let instructions = file(&cmd, "instructions.md");
    assert_eq!(instructions.contents, s.agent.prompt);
    let config = config_json(&cmd);
    // 2.0 does not apply the `instructions` key (S2.14).
    assert!(config.get("instructions").is_none(), "{config}");
    assert_eq!(
        config["plugins"][0]["options"],
        serde_json::json!({"instructions": format!("{RUN_DIR}/instructions.md")})
    );
    let mut blank = spec(None);
    blank.agent.prompt = "  \n".into();
    let cmd = launch(&blank);
    assert_eq!(
        config_json(&cmd)["plugins"][0]["options"],
        serde_json::json!({})
    );
    assert!(
        !cmd.files
            .iter()
            .any(|f| f.path.ends_with("instructions.md"))
    );
}

#[test]
fn launch_without_model_effort_or_directory() {
    let mut s = spec(None);
    s.agent.directory = None;
    s.agent.model = None;
    s.agent.effort = None;
    let cmd = launch(&s);
    assert_eq!(cmd.cwd, PathBuf::from("/home/u"));
    assert!(config_json(&cmd).get("model").is_none());
}

#[test]
fn effort_without_a_model_is_left_out() {
    // 2.0 has effort only inside a model (`provider/model#variant`), P2-D24.
    let mut s = spec(None);
    s.agent.model = None;
    s.agent.effort = Some("high".into());
    let cmd = launch(&s);
    let config = config_json(&cmd);
    assert!(config.get("model").is_none(), "{config}");
    assert!(!file(&cmd, "opencode.json").contents.contains("high"));
}

#[test]
fn a_model_with_its_own_variant_is_kept() {
    let mut s = spec(None);
    s.agent.model = Some("litellm/model-2#low".into());
    s.agent.effort = Some("high".into());
    assert_eq!(config_json(&launch(&s))["model"], "litellm/model-2#low");
    s.agent.effort = None;
    s.agent.model = Some("-x".into());
    assert_eq!(config_json(&launch(&s))["model"], "-x");
}

#[test]
fn launch_defaults_to_the_build_agent_unless_the_flags_choose_one() {
    assert_eq!(
        config_json(&launch(&spec(Some("t"))))["default_agent"],
        "build"
    );
    for flags in [vec!["--agent", "plan"], vec!["--agent=plan"]] {
        let mut s = spec(None);
        s.agent.flags = flags.iter().map(|f| (*f).to_owned()).collect();
        let cmd = launch(&s);
        assert!(config_json(&cmd).get("default_agent").is_none());
        assert!(cmd.args.ends_with(&s.agent.flags), "{:?}", cmd.args);
    }
}

#[test]
fn launch_refuses_non_utf8_paths() {
    use std::os::unix::ffi::OsStrExt;
    let bad = std::ffi::OsStr::from_bytes(b"/opt/\xff/duohive");
    let mut s = spec(None);
    s.hook_exe = bad.into();
    assert!(matches!(
        OpencodeAdapter.launch(&s),
        Err(LaunchError::Invalid(_))
    ));
    let mut s = spec(None);
    s.run_dir = bad.into();
    assert!(matches!(
        OpencodeAdapter.launch(&s),
        Err(LaunchError::Invalid(_))
    ));
}

#[test]
fn launch_files_stay_in_run_dir() {
    let s = spec(Some("t"));
    let cmd = launch(&s);
    assert_eq!(cmd.files.len(), 3);
    for f in &cmd.files {
        assert!(f.path.starts_with(&s.run_dir), "{}", f.path.display());
    }
    assert_eq!(
        env(&cmd, "OPENCODE_CONFIG").map(PathBuf::from),
        Some(s.run_dir.join("opencode.json"))
    );
}

#[test]
fn no_auto_approve_flags() {
    let mut s = spec(Some("t"));
    s.agent.model = Some("p/m".into());
    s.agent.effort = Some("high".into());
    let cmd = launch(&s);
    for banned in [
        "--auto",
        "--dangerously-skip-permissions",
        "--permission-mode",
    ] {
        assert!(!cmd.args.iter().any(|a| a.starts_with(banned)), "{banned}");
    }
    let config = config_json(&cmd);
    for key in ["permission", "permissions", "agents", "agent"] {
        assert!(config.get(key).is_none(), "{key} in {config}");
    }
    // The plugin never answers a permission itself (TD §2).
    assert!(!PLUGIN_SOURCE.contains("permission.reply"));
}

#[test]
fn hook_config_points_at_duohive_hook() {
    let s = spec(None);
    let cmd = launch(&s);
    assert_eq!(
        config_json(&cmd)["plugins"][0]["package"],
        format!("file://{RUN_DIR}/opencode-plugin")
    );
    // 2.0 loads a plugin directory holding `index.js` (S2.14).
    let plugin = cmd
        .files
        .iter()
        .find(|f| f.path == PathBuf::from(RUN_DIR).join("opencode-plugin/index.js"))
        .unwrap();
    assert_eq!(plugin.contents, PLUGIN_SOURCE);
    assert_eq!(
        env(&cmd, "DUOHIVE_HOOK_EXE"),
        Some("/usr/local/bin/duohive")
    );
    assert_eq!(env(&cmd, "DUOHIVE_HOOK_AGENT"), Some("squad-1"));
    // S2.14's rows, as the plugin names them.
    for needle in [
        "\"plugin.updated\"",
        "\"session.created\"",
        "\"session.execution.started\"",
        "\"session.execution.succeeded\"",
        "\"session.execution.failed\"",
        "\"session.execution.interrupted\"",
        "\"session.text.ended\"",
        "\"permission.asked\"",
        "\"permission.replied\"",
        "\"form.created\"",
        "\"form.replied\"",
        "\"evaluate\"",
        "\"context\"",
        "\"session-start\"",
        "\"busy\"",
        "\"needs-input\"",
        "\"turn-end\"",
        "\"session-end\"",
        "duohive_last_reply",
        "process.env.DUOHIVE_HOOK_EXE",
        "process.env.DUOHIVE_HOOK_AGENT",
        "export default { id: \"duohive\", setup }",
    ] {
        assert!(PLUGIN_SOURCE.contains(needle), "plugin lacks {needle}");
    }
}

#[test]
fn plugin_url_is_percent_encoded() {
    let mut s = spec(None);
    s.run_dir = "/Users/a b/data#1/run/squad-1".into();
    assert_eq!(
        config_json(&launch(&s))["plugins"][0]["package"],
        "file:///Users/a%20b/data%231/run/squad-1/opencode-plugin"
    );
}

/// The signal DuoHive must read from each delivery the spike's skeleton plugin
/// logged, with turn `n`'s reply from `replies/turn-end-<n>.<log>.…`.
fn expected(log: &str, turn: usize, event: &str, payload: &Value) -> HookSignal {
    match event {
        "session-start" => HookSignal::Ready,
        "busy" => HookSignal::Busy,
        "needs-input" => {
            let p = &payload["properties"];
            let prompt = if payload["type"] == "form.created" {
                format!("question {}", p["title"].as_str().unwrap())
            } else {
                let resources: Vec<&str> = p["resources"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| r.as_str().unwrap())
                    .collect();
                format!("{} {}", p["action"].as_str().unwrap(), resources.join(", "))
            };
            HookSignal::NeedsInput {
                prompt: Some(prompt),
            }
        }
        "turn-end" if payload["type"] == "session.execution.failed" => HookSignal::TurnFailed {
            reason: "turn failed: spike: model not found (400)".into(),
        },
        "turn-end" => HookSignal::TurnEnded {
            last_reply: reply_fixture(&format!(
                "replies/turn-end-{turn}.{log}.duohive_last_reply.txt"
            )),
        },
        "session-end" => HookSignal::SessionEnded,
        other => panic!("unexpected delivery {other}"),
    }
}

#[test]
fn parses_recorded_hooks() {
    let mut logs: Vec<_> = std::fs::read_dir(Path::new(FIXTURES).join("hooks"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("duohive-hooks-"))
        })
        .collect();
    logs.sort();
    assert_eq!(logs.len(), 6);
    let (mut turn, mut parsed, mut failed) = (0, 0, 0);
    for path in &logs {
        let log = path
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .trim_start_matches("duohive-hooks-");
        for line in std::fs::read_to_string(path).unwrap().lines() {
            let v: Value = serde_json::from_str(line).unwrap();
            let event = v["event"].as_str().unwrap();
            turn += usize::from(event == "turn-end");
            let want = expected(log, turn, event, &v["payload"]);
            failed += usize::from(matches!(want, HookSignal::TurnFailed { .. }));
            assert_eq!(
                OpencodeAdapter.parse_hook(event, &v["payload"]),
                Some(want),
                "{log}: {line}"
            );
            parsed += 1;
        }
    }
    assert_eq!(turn, 13);
    assert_eq!(failed, 1);
    assert_eq!(parsed, 48);
}

#[test]
fn an_interrupted_turn_is_a_normal_turn_end() {
    for reason in ["user", "shutdown"] {
        let p = serde_json::json!({"type": "session.execution.interrupted",
            "reason": reason, "duohive_last_reply": "half"});
        assert_eq!(
            OpencodeAdapter.parse_hook("turn-end", &p),
            Some(HookSignal::TurnEnded {
                last_reply: Some("half".into())
            })
        );
    }
}

#[test]
fn failed_turn_reason_falls_back_through_the_error() {
    let reason = |error: Value| {
        let p = serde_json::json!({"type": "session.execution.failed", "error": error});
        match OpencodeAdapter.parse_hook("turn-end", &p) {
            Some(HookSignal::TurnFailed { reason }) => reason,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(
        reason(serde_json::json!({"type": "provider.auth", "message": " bad key ", "status": 401})),
        "turn failed: bad key (401)"
    );
    assert_eq!(
        reason(serde_json::json!({"type": "provider.auth", "status": 401})),
        "turn failed: provider.auth (401)"
    );
    assert_eq!(
        reason(serde_json::json!({"message": "boom"})),
        "turn failed: boom"
    );
    assert_eq!(
        reason(serde_json::json!({"status": 503})),
        "turn failed (503)"
    );
    assert_eq!(reason(serde_json::json!("garbage")), "turn failed");
    assert_eq!(reason(Value::Null), "turn failed");
}

#[test]
fn needs_input_prompt_tolerates_garbage() {
    let a = OpencodeAdapter;
    let bare = serde_json::json!({"properties": {"action": "webfetch"}});
    assert_eq!(
        a.parse_hook("needs-input", &bare),
        Some(HookSignal::NeedsInput {
            prompt: Some("webfetch".into())
        })
    );
    let form = serde_json::json!({"type": "form.created",
        "properties": {"title": "Questions", "question": "Which do you prefer?"}});
    assert_eq!(
        a.parse_hook("needs-input", &form),
        Some(HookSignal::NeedsInput {
            prompt: Some("question Which do you prefer?".into())
        })
    );
    let empty_form = serde_json::json!({"type": "form.created", "properties": {}});
    assert_eq!(
        a.parse_hook("needs-input", &empty_form),
        Some(HookSignal::NeedsInput {
            prompt: Some("question".into())
        })
    );
    let junk = serde_json::json!("garbage");
    assert_eq!(
        a.parse_hook("needs-input", &junk),
        Some(HookSignal::NeedsInput { prompt: None })
    );
    assert_eq!(
        a.parse_hook("turn-end", &junk),
        Some(HookSignal::TurnEnded { last_reply: None })
    );
    let empty = serde_json::json!({"duohive_last_reply": ""});
    assert_eq!(
        a.parse_hook("turn-end", &empty),
        Some(HookSignal::TurnEnded { last_reply: None })
    );
    assert_eq!(a.parse_hook("session.execution.succeeded", &junk), None);
    // The reply lives in the payload: enrich_hook is the no-op.
    let mut p = serde_json::json!({"type": "session.execution.succeeded"});
    a.enrich_hook("turn-end", &mut p);
    assert_eq!(
        p,
        serde_json::json!({"type": "session.execution.succeeded"})
    );
}

fn padded(screen: &str) -> String {
    let mut out: String = screen.lines().map(|l| format!("{l}   \n")).collect();
    out.push_str("\n   \n\n");
    out
}

#[test]
fn screen_rules_classify_recorded_screens() {
    use ScreenState::{AtRest, Busy, NeedsInput};
    let cases = [
        ("approval-write", Some(NeedsInput)),
        ("approval-shell", Some(NeedsInput)),
        ("approval-always-selected", Some(NeedsInput)),
        ("evaluate-hook-reprompts-after-always", Some(NeedsInput)),
        ("g1-approval-write", Some(NeedsInput)),
        ("g1-approval-shell", Some(NeedsInput)),
        ("question-form", Some(NeedsInput)),
        ("g1-question", Some(NeedsInput)),
        ("working-1", Some(Busy)),
        ("working-2", Some(Busy)),
        ("after-approve-yes-1s", Some(Busy)),
        // One `Escape` while working: still working (S2.14).
        ("esc-again-to-interrupt", Some(Busy)),
        // A 5xx retries without end: working, then quiet (S2.14's risk).
        ("retrying-500", Some(Busy)),
        ("at-rest-no-task", Some(AtRest)),
        ("at-rest-after-turn", Some(AtRest)),
        ("after-approve-yes", Some(AtRest)),
        ("after-reject-escape", Some(AtRest)),
        ("after-interrupt", Some(AtRest)),
        ("after-paste-multiline", Some(AtRest)),
        ("early-paste-lost-1.5s", Some(AtRest)),
        ("startup-with-task-2s", Some(AtRest)),
        ("with-task-done", Some(AtRest)),
        ("subagent-done", Some(AtRest)),
        ("model-variant-from-config", Some(AtRest)),
        ("unknown-model-silently-default", Some(AtRest)),
        ("turn-error-400", Some(AtRest)),
        ("g2-after-error", Some(AtRest)),
        ("tui-shared-start", Some(AtRest)),
        ("tui-shared-after-turn", Some(AtRest)),
        ("tui-shared-exit-typed", Some(AtRest)),
        // The TUI has exited: the shell's output, no input box.
        ("after-exit", None),
        ("exit-from-approval", None),
        ("model-flag-rejected", None),
    ];
    let listed = std::fs::read_dir(Path::new(FIXTURES).join("screens"))
        .unwrap()
        .count();
    assert_eq!(cases.len(), listed, "every recorded screen is classified");
    for (name, want) in cases {
        let screen = fixture(&format!("screens/{name}.txt"));
        for text in [screen.clone(), padded(&screen)] {
            let got = OpencodeAdapter.read_screen(&text);
            assert_eq!(got.state, want, "{name}");
            assert_eq!(got.last_reply, None, "{name}");
        }
    }
}

#[test]
fn screen_rules_need_the_border_and_its_footer() {
    let border = format!("  ╹{}", "▀".repeat(60));
    let read = |s: &str| OpencodeAdapter.read_screen(s).state;
    for footer in ["esc interrupt", "esc again to interrupt"] {
        assert_eq!(
            read(&format!(
                "  ┃  Build\n{border}\n   ■■⬝⬝  {footer}   ctrl+p commands\n"
            )),
            Some(ScreenState::Busy),
            "{footer}"
        );
    }
    assert_eq!(
        read(&format!(
            "  ┃  Build\n{border}\n   /tmp/w   12K  ctrl+p commands\n"
        )),
        Some(ScreenState::AtRest)
    );
    // `esc interrupt` elsewhere on screen is not the footer.
    assert_eq!(
        read(&format!(
            "  esc interrupt\n{border}\n   tab agents  ctrl+p commands\n"
        )),
        Some(ScreenState::AtRest)
    );
    // A border made of anything but `▀` is not the input box.
    assert_eq!(read("  ╹▀▀▀x▀▀\n   esc interrupt\n"), None);
    // Approval or question text above a later input box is history, not a prompt.
    for prompt in [
        "  ┃  △ Permission required\n  ┃   Allow once   Always allow   Reject\n",
        "  ┃  Questions\n  ┃  ↑↓ select  enter submit  esc dismiss\n",
    ] {
        assert_eq!(
            read(&format!("{prompt}{border}\n   /tmp/w\n")),
            Some(ScreenState::AtRest)
        );
        assert_eq!(read(prompt), Some(ScreenState::NeedsInput));
    }
    // 1.18's order of the choices still reads as a prompt.
    assert_eq!(
        read("  ┃  △ Permission required\n  ┃   Allow once   Allow always   Reject\n"),
        Some(ScreenState::NeedsInput)
    );
    // `Questions` in a reply is not the form's title line.
    assert_eq!(
        read("  ┃  Questions about X\n  ┃  enter submit  esc dismiss\n"),
        None
    );
    assert_eq!(read(""), None);
}

#[test]
fn startup_prompts_match_spike_report() {
    assert!(!OpencodeAdapter.startup_prompts());
}

#[test]
fn coverage_matches_spike_report() {
    assert_eq!(
        OpencodeAdapter.coverage(),
        HookCoverage {
            ready: true,
            busy: true,
            needs_input: true,
            turn_end: true,
            session_end: true,
            last_reply: true,
        }
    );
}

#[test]
fn version_output_parses() {
    assert_eq!(
        crate::detect::parse_cli_version(CliKind::Opencode, "2.0.29\n").as_deref(),
        Some("2.0.29")
    );
    for garbage in ["", "garbage", "2.0", "v2.0.29", "2.0.29 (OpenCode)"] {
        assert_eq!(
            crate::detect::parse_cli_version(CliKind::Opencode, garbage),
            None,
            "{garbage}"
        );
    }
}

#[test]
fn keys_versions_and_kind_match_spike_report() {
    let a = OpencodeAdapter;
    assert_eq!(a.kind(), CliKind::Opencode);
    assert_eq!(a.quit_keys(), vec![Key::Text("/exit".into()), Key::Enter]);
    assert_eq!(a.submit_keys(), vec![Key::Enter]);
    assert_eq!(a.tested_versions(), &["2.0"]);
}

/// One `Escape` before a next-turn paste: it closed 1.18.35's update dialog,
/// and at rest on 2.0.29 it changes nothing (TD §5.5, S2.14).
#[test]
fn before_paste_keys_close_a_dialog() {
    assert_eq!(OpencodeAdapter.before_paste_keys(), vec![Key::Escape]);
}
