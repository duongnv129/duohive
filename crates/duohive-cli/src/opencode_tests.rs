//! Tests for the OpenCode adapter against spike S2's fixtures (OpenCode 1.18.34).

use std::path::{Path, PathBuf};

use duohive_core::adapter::{CliAdapter, HookCoverage, HookSignal, LaunchError, ScreenState};
use duohive_core::cli_kind::CliKind;
use duohive_core::launch::{Key, LaunchCommand, LaunchFile, LaunchSpec};
use duohive_core::testkit::{example_org, n};
use serde_json::Value;

use super::*;

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/opencode/1.18.34"
);

fn fixture(rel: &str) -> String {
    std::fs::read_to_string(Path::new(FIXTURES).join(rel)).unwrap()
}

/// A reply fixture without the newline the export added; `(null)` is no reply.
fn reply_fixture(rel: &str) -> Option<String> {
    let text = fixture(rel);
    let text = text.strip_suffix('\n').unwrap_or(&text);
    (text != "(null)").then(|| text.to_owned())
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
        if f.path.ends_with("opencode-plugin.js") {
            assert_eq!(f.contents, PLUGIN_SOURCE);
            f.contents = "<src/assets/opencode-plugin.js>".into();
        }
    }
    cmd
}

#[test]
fn launch_snapshot_no_task() {
    let cmd = launch(&spec(None));
    assert_eq!(cmd.program, "opencode");
    assert_eq!(cmd.cwd, PathBuf::from("/home/u/code/payments"));
    assert!(!cmd.args.iter().any(|a| a.starts_with("--prompt")));
    let config = config_json(&cmd);
    // Approvals are composed by the plugin's `config` hook, never a static key (S2.11).
    assert!(config.get("permission").is_none(), "{config}");
    assert!(PLUGIN_SOURCE.contains("config:"));
    insta::assert_yaml_snapshot!(for_snapshot(cmd));
}

#[test]
fn launch_snapshot_with_task_model_effort_flags() {
    let mut s = spec(Some("Add retry cases"));
    s.agent.model = Some("opus".into());
    s.agent.effort = Some("high".into());
    s.agent.flags = vec!["--verbose".into()];
    let cmd = launch(&s);
    let args = &cmd.args;
    let pos = |a: &str| args.iter().position(|x| x == a).unwrap();
    assert!(pos("--verbose") > pos("--model=opus"));
    assert_eq!(
        args.last().map(String::as_str),
        Some("--prompt=Add retry cases")
    );
    assert!(
        !args
            .iter()
            .any(|a| a.contains("effort") || a.contains("variant"))
    );
    assert_eq!(config_json(&cmd)["agent"]["build"]["variant"], "high");
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
fn launch_writes_the_prompt_to_an_instructions_file_and_omits_an_empty_one() {
    let s = spec(None);
    let cmd = launch(&s);
    let instructions = file(&cmd, "instructions.md");
    assert_eq!(instructions.contents, s.agent.prompt);
    assert_eq!(
        config_json(&cmd)["instructions"],
        serde_json::json!([format!("{RUN_DIR}/instructions.md")])
    );
    let mut blank = spec(None);
    blank.agent.prompt = "  \n".into();
    let cmd = launch(&blank);
    assert!(config_json(&cmd).get("instructions").is_none());
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
    assert!(!cmd.args.iter().any(|a| a.starts_with("--model")));
    assert!(config_json(&cmd).get("agent").is_none());
}

#[test]
fn launch_pins_the_build_agent_unless_the_flags_choose_one() {
    // The plugin composes approvals for the agents it knows; `build` is the
    // one DuoHive launches (the TUI otherwise starts the last agent used).
    let args = launch(&spec(Some("t"))).args;
    assert!(args.contains(&"--agent=build".to_owned()), "{args:?}");
    for flags in [vec!["--agent", "plan"], vec!["--agent=plan"]] {
        let mut s = spec(None);
        s.agent.flags = flags.iter().map(|f| (*f).to_owned()).collect();
        let args = launch(&s).args;
        assert!(!args.contains(&"--agent=build".to_owned()), "{args:?}");
        assert!(args.ends_with(&s.agent.flags), "{args:?}");
    }
}

#[test]
fn launch_keeps_a_model_that_starts_with_a_dash() {
    let mut s = spec(None);
    s.agent.model = Some("-x".into());
    assert!(launch(&s).args.contains(&"--model=-x".to_owned()));
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
    s.agent.model = Some("opus".into());
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
    assert!(config.get("permission").is_none(), "{config}");
    assert!(
        config["agent"]["build"].get("permission").is_none(),
        "{config}"
    );
    // The plugin never decides a permission itself (TD §2).
    assert!(!PLUGIN_SOURCE.contains("permission.ask\""));
}

#[test]
fn hook_config_points_at_duohive_hook() {
    let s = spec(None);
    let cmd = launch(&s);
    assert_eq!(
        config_json(&cmd)["plugin"],
        serde_json::json!([format!("file://{RUN_DIR}/opencode-plugin.js")])
    );
    assert_eq!(file(&cmd, "opencode-plugin.js").contents, PLUGIN_SOURCE);
    assert_eq!(
        env(&cmd, "DUOHIVE_HOOK_EXE"),
        Some("/usr/local/bin/duohive")
    );
    assert_eq!(env(&cmd, "DUOHIVE_HOOK_AGENT"), Some("squad-1"));
    // S2.3's rows, as the plugin names them.
    for needle in [
        "\"catalog.updated\"",
        "\"session.created\"",
        "\"chat.message\"",
        "\"session.status\"",
        "\"tool.execute.before\"",
        "\"tool.execute.after\"",
        "\"permission.asked\"",
        "\"permission.replied\"",
        "\"session.idle\"",
        "dispose:",
        "\"session-start\"",
        "\"busy\"",
        "\"needs-input\"",
        "\"turn-end\"",
        "\"session-end\"",
        "duohive_last_reply",
        "process.env.DUOHIVE_HOOK_EXE",
        "process.env.DUOHIVE_HOOK_AGENT",
    ] {
        assert!(PLUGIN_SOURCE.contains(needle), "plugin lacks {needle}");
    }
}

#[test]
fn plugin_url_is_percent_encoded() {
    let mut s = spec(None);
    s.run_dir = "/Users/a b/data#1/run/squad-1".into();
    assert_eq!(
        config_json(&launch(&s))["plugin"],
        serde_json::json!(["file:///Users/a%20b/data%231/run/squad-1/opencode-plugin.js"])
    );
}

/// The DuoHive event and signal S2.3 names for each recorded OpenCode fixture.
fn expected(stem: &str, payload: &Value) -> (String, Option<HookSignal>) {
    let base = stem.strip_prefix("review-").unwrap_or(stem);
    let (name, _) = base.rsplit_once('-').unwrap();
    let ours = |e: &str, s: HookSignal| (e.to_owned(), Some(s));
    match name {
        "plugin.init" => ours("session-start", HookSignal::Ready),
        "session.status" if payload["properties"]["status"]["type"] == "busy" => {
            ours("busy", HookSignal::Busy)
        }
        "hook.chat.message"
        | "hook.tool.execute.before"
        | "hook.tool.execute.after"
        | "permission.replied" => ours("busy", HookSignal::Busy),
        "permission.asked" => {
            let p = &payload["properties"];
            let prompt = format!(
                "{} {}",
                p["permission"].as_str().unwrap(),
                p["metadata"]["filepath"].as_str().unwrap()
            );
            ours(
                "needs-input",
                HookSignal::NeedsInput {
                    prompt: Some(prompt),
                },
            )
        }
        "session.idle.reply" => {
            let last_reply = if stem.starts_with("review-") {
                payload["duohive_last_reply"].as_str().map(str::to_owned)
            } else {
                reply_fixture(&format!("replies/{stem}.duohive_last_reply.txt"))
            };
            ours("turn-end", HookSignal::TurnEnded { last_reply })
        }
        "session.idle" => ours("turn-end", HookSignal::TurnEnded { last_reply: None }),
        "hook.dispose" => ours("session-end", HookSignal::SessionEnded),
        // Not mapped (S2.3): passed under its OpenCode name, it maps to nothing.
        "session.created" | "session.updated" | "message.updated.completed" | "session.status" => {
            (name.to_owned(), None)
        }
        other => panic!("unexpected fixture {other}"),
    }
}

#[test]
fn parses_recorded_hooks() {
    let mut entries: Vec<_> = std::fs::read_dir(Path::new(FIXTURES).join("hooks"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    entries.sort();
    let mut mapped = 0;
    for path in &entries {
        let stem = path.file_stem().unwrap().to_str().unwrap();
        let payload: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let (event, want) = expected(stem, &payload);
        mapped += usize::from(want.is_some());
        assert_eq!(OpencodeAdapter.parse_hook(&event, &payload), want, "{stem}");
    }
    assert_eq!(entries.len(), 134);
    assert!(mapped > 40, "{mapped}");
}

#[test]
fn needs_input_prompt_falls_back_to_patterns_and_tolerates_garbage() {
    let a = OpencodeAdapter;
    let p = serde_json::json!({"properties": {"permission": "bash",
        "patterns": ["git push*", "rm -rf build"], "metadata": {}}});
    assert_eq!(
        a.parse_hook("needs-input", &p),
        Some(HookSignal::NeedsInput {
            prompt: Some("bash git push*, rm -rf build".into())
        })
    );
    let bare = serde_json::json!({"properties": {"permission": "webfetch"}});
    assert_eq!(
        a.parse_hook("needs-input", &bare),
        Some(HookSignal::NeedsInput {
            prompt: Some("webfetch".into())
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
    assert_eq!(a.parse_hook("session.idle", &junk), None);
}

#[test]
fn extracts_last_reply_from_fixture() {
    // S2.4: the plugin adds the reply to the `session.idle` payload, so the
    // reply parse_hook reads is the recorded `duohive_last_reply`.
    for k in 1..=5 {
        let payload: Value =
            serde_json::from_str(&fixture(&format!("hooks/session.idle.reply-{k}.json"))).unwrap();
        let want = reply_fixture(&format!(
            "replies/session.idle.reply-{k}.duohive_last_reply.txt"
        ));
        assert_eq!(
            OpencodeAdapter.parse_hook("turn-end", &payload),
            Some(HookSignal::TurnEnded { last_reply: want }),
            "reply {k}"
        );
    }
    // The reply lives in the payload: enrich_hook is the no-op.
    let mut p = serde_json::json!({"type": "session.idle"});
    OpencodeAdapter.enrich_hook("turn-end", &mut p);
    assert_eq!(p, serde_json::json!({"type": "session.idle"}));
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
        ("approval-write-2", Some(NeedsInput)),
        ("approval-reject-selected", Some(NeedsInput)),
        ("review-approval-write-composed", Some(NeedsInput)),
        ("working-1", Some(Busy)),
        ("working-2", Some(Busy)),
        ("write-default-3s", Some(Busy)),
        ("after-approve-yes-1s", Some(Busy)),
        ("startup-with-task-5s", Some(Busy)),
        ("startup-with-task-6", Some(Busy)),
        ("review-paste-after-catalog-1.5s", Some(Busy)),
        ("at-rest-no-task", Some(AtRest)),
        ("at-rest-after-turn", Some(AtRest)),
        ("after-approve-yes", Some(AtRest)),
        ("after-approve-no", Some(AtRest)),
        ("after-paste", Some(AtRest)),
        ("with-task-done", Some(AtRest)),
        ("write-default-done", Some(AtRest)),
        ("exit-typed", Some(AtRest)),
        ("mcp-tool-name", Some(AtRest)),
        ("paste-before-enter", Some(AtRest)),
        ("typed-before-enter", Some(AtRest)),
        ("status-dialog", Some(AtRest)),
        ("startup-a1-6s", Some(AtRest)),
        ("startup-with-task-5", Some(AtRest)),
        ("review-a5-first-box", Some(AtRest)),
        ("review-early-paste-1.5s", Some(AtRest)),
        ("startup-with-task-4", None),
        // Dead panes are left to the exit rules, even with a busy frame on screen.
        ("after-exit", None),
        ("after-ctrl-c", None),
        ("after-kill9", None),
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
    assert_eq!(
        read(&format!(
            "  ┃  Build\n{border}\n   ■■⬝⬝  esc interrupt   ctrl+p commands\n"
        )),
        Some(ScreenState::Busy)
    );
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
    // Approval text above a later input box is history, not a prompt.
    assert_eq!(
        read(&format!(
            "  ┃  △ Permission required\n  ┃   Allow once   Allow always   Reject\n{border}\n   /tmp/w\n"
        )),
        Some(ScreenState::AtRest)
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
        crate::detect::parse_cli_version(CliKind::Opencode, "1.18.34\n").as_deref(),
        Some("1.18.34")
    );
    for garbage in ["", "garbage", "1.18", "v1.18.34", "1.18.34 (OpenCode)"] {
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
    assert_eq!(a.tested_versions(), &["1.18"]);
}
