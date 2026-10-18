//! The Claude Code adapter (spike S1, Claude Code 2.1.288).

use duohive_core::adapter::{
    CliAdapter, CliPresence, HookCoverage, HookSignal, LaunchError, ScreenSignal, ScreenState,
};
use duohive_core::cli_kind::CliKind;
use duohive_core::launch::{Key, LaunchCommand, LaunchFile, LaunchSpec};
use duohive_core::status::WaitingOn;
use serde_json::Value;

use crate::detect::probe_version;
use crate::hooks_json::command_hooks;
use crate::{launch_cwd, utf8};

/// Claude Code hook → DuoHive event (S1.3). `Notification` is left out on
/// purpose: it arrives about 6 s after `PermissionRequest` and would mark an
/// agent waiting again after the user has answered.
const HOOKS: [(&str, &str); 8] = [
    ("SessionStart", "session-start"),
    ("UserPromptSubmit", "busy"),
    ("PreToolUse", "busy"),
    ("PostToolUse", "busy"),
    ("PermissionRequest", "needs-input"),
    ("Stop", "turn-end"),
    ("StopFailure", "turn-end"),
    ("SessionEnd", "session-end"),
];

/// The per-launch hooks file, passed with `--settings` (S1.3).
const SETTINGS_FILE: &str = "settings.json";

/// The per-launch MCP config, passed with `--mcp-config` (TD §5.2).
const MCP_FILE: &str = "mcp.json";

/// The one tool pattern each launch pre-approves: DuoHive's own (P3-D1).
const DUOHIVE_TOOLS: &str = "mcp__duohive__*";

/// The payload field `enrich_hook` fills from the transcript.
const ENRICHED_REPLY: &str = "duohive_last_reply";

/// Claude Code.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClaudeAdapter;

impl CliAdapter for ClaudeAdapter {
    fn kind(&self) -> CliKind {
        CliKind::Claude
    }

    fn detect(&self) -> CliPresence {
        probe_version(self.kind())
    }

    fn tested_versions(&self) -> &'static [&'static str] {
        &["2.1"]
    }

    /// Every event has a hook and the reply comes in `Stop` (S1.3, S1.4).
    fn coverage(&self) -> HookCoverage {
        HookCoverage {
            ready: true,
            busy: true,
            needs_input: true,
            turn_end: true,
            session_end: true,
            last_reply: true,
        }
    }

    /// `claude [--append-system-prompt <prompt>] --settings <run>/settings.json
    /// --mcp-config <run>/mcp.json
    /// [--model=m] [--effort=e] [flags…] [-- <task>]` (S1.2, S1.3, S1.6, S1.8, S1.11).
    fn launch(&self, spec: &LaunchSpec) -> Result<LaunchCommand, LaunchError> {
        let settings_path = spec.run_dir.join(SETTINGS_FILE);
        let mut settings = command_hooks(spec, &HOOKS)?;
        if let Some(obj) = settings.as_object_mut() {
            obj.insert(
                "permissions".to_owned(),
                serde_json::json!({ "allow": [DUOHIVE_TOOLS] }),
            );
        }
        let settings = serde_json::to_string_pretty(&settings)
            .map_err(|e| LaunchError::Invalid(format!("couldn't build hook settings: {e}")))?;
        let mcp_path = spec.run_dir.join(MCP_FILE);
        let mcp = serde_json::json!({ "mcpServers": { "duohive": {
            "type": "stdio",
            "command": utf8(&spec.hook_exe, "DuoHive's path")?,
            "args": ["mcp"],
        }}});
        let mcp = serde_json::to_string_pretty(&mcp)
            .map_err(|e| LaunchError::Invalid(format!("couldn't build the MCP config: {e}")))?;
        let agent = &spec.agent;
        let mut args = Vec::new();
        if !agent.prompt.trim().is_empty() {
            args.extend(["--append-system-prompt".to_owned(), agent.prompt.clone()]);
        }
        args.extend([
            "--settings".to_owned(),
            utf8(&settings_path, "run directory")?.to_owned(),
            // No `--strict-mcp-config`: the user's own servers stay.
            "--mcp-config".to_owned(),
            utf8(&mcp_path, "run directory")?.to_owned(),
        ]);
        // One `--flag=value` argument each: free text starting with `-` stays a value.
        if let Some(model) = &agent.model {
            args.push(format!("--model={model}"));
        }
        if let Some(effort) = &agent.effort {
            args.push(format!("--effort={effort}"));
        }
        args.extend(agent.flags.iter().cloned());
        if let Some(task) = &spec.task {
            // `--` keeps a task that starts with `-` from being read as options (S1.8).
            args.extend(["--".to_owned(), task.clone()]);
        }
        Ok(LaunchCommand {
            program: self.kind().binary().to_owned(),
            args,
            env: Vec::new(),
            cwd: launch_cwd(spec),
            files: vec![
                LaunchFile {
                    path: settings_path,
                    contents: settings,
                },
                LaunchFile {
                    path: mcp_path,
                    contents: mcp,
                },
            ],
        })
    }

    fn parse_hook(&self, event: &str, payload: &Value) -> Option<HookSignal> {
        match event {
            "session-start" => Some(HookSignal::Ready),
            "busy" => Some(HookSignal::Busy),
            "needs-input" => Some(HookSignal::NeedsInput {
                prompt: approval_prompt(payload),
            }),
            "turn-end" => Some(HookSignal::TurnEnded {
                last_reply: text_field(payload, "last_assistant_message")
                    .or_else(|| text_field(payload, ENRICHED_REPLY))
                    .map(str::to_owned),
            }),
            "session-end" => Some(HookSignal::SessionEnded),
            _ => None,
        }
    }

    /// The reply comes from the hook, so only the state is read (S1.7).
    fn read_screen(&self, screen: &str) -> ScreenSignal {
        let read = screen_state(screen);
        ScreenSignal {
            state: read.map(|(state, _)| state),
            last_reply: None,
            waiting_on: read.and_then(|(_, on)| on),
        }
    }

    /// Safe in both editor modes (checked live, TD §5.5). An interrupted turn puts
    /// its prompt back in the input box, where `/exit` would be appended and sent as
    /// a new prompt; under `"editorMode": "vim"` the stop's `Escape` only leaves
    /// INSERT, and `/exit` typed in NORMAL edits the prompt. So `C-c` interrupts the
    /// turn (vim) or clears the box, `i` enters INSERT (vim) or is a stray letter,
    /// and a second `C-c` clears the box. Two `C-c` on a box already empty quit
    /// Claude Code there and then, which is a quit too.
    fn quit_keys(&self) -> Vec<Key> {
        vec![
            Key::CtrlC,
            Key::Text("i".into()),
            Key::CtrlC,
            Key::Text("/exit".into()),
            Key::Enter,
        ]
    }

    fn submit_keys(&self) -> Vec<Key> {
        vec![Key::Enter]
    }

    /// The workspace-trust dialog (S1.8).
    fn startup_prompts(&self) -> bool {
        true
    }

    /// For a `turn-end` whose `Stop` payload lacks `last_assistant_message`,
    /// reads the reply from `transcript_path` (S1.4's fallback). Any failure
    /// leaves the payload as it was.
    fn enrich_hook(&self, event: &str, payload: &mut Value) {
        if event != "turn-end" || text_field(payload, "last_assistant_message").is_some() {
            return;
        }
        let Some(path) = payload.get("transcript_path").and_then(Value::as_str) else {
            return;
        };
        let Ok(transcript) = std::fs::read_to_string(path) else {
            return;
        };
        if let (Some(reply), Some(obj)) = (
            last_reply_from_transcript(&transcript),
            payload.as_object_mut(),
        ) {
            obj.insert(ENRICHED_REPLY.to_owned(), Value::String(reply));
        }
    }
}

/// A non-empty string field of `payload`.
fn text_field<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// `<tool_name> <file_path or command>` from a `PermissionRequest` payload,
/// or just the tool name when its input has neither.
fn approval_prompt(payload: &Value) -> Option<String> {
    let tool = text_field(payload, "tool_name")?;
    let input = payload.get("tool_input").unwrap_or(&Value::Null);
    let detail = text_field(input, "file_path").or_else(|| text_field(input, "command"));
    Some(match detail {
        Some(d) => format!("{tool} {d}"),
        None => tool.to_owned(),
    })
}

/// The reply of each turn in a transcript, oldest first (S1.4's rule):
/// sidechain entries are skipped; a turn starts at a `user` entry whose
/// `message.content` is a string; its reply is the `text` blocks of the last
/// `assistant` message (by `message.id`) in the turn that has any, joined by
/// newlines. A turn with no text (a denied tool) has `None`.
pub(crate) fn transcript_turn_replies(transcript: &str) -> Vec<Option<String>> {
    let mut turns: Vec<Option<(String, Vec<String>)>> = Vec::new();
    for entry in transcript
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
    {
        if entry.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let message = entry.get("message").unwrap_or(&Value::Null);
        match entry.get("type").and_then(Value::as_str) {
            Some("user") if message.get("content").is_some_and(Value::is_string) => {
                turns.push(None);
            }
            Some("assistant") => {
                let Some(current) = turns.last_mut() else {
                    continue;
                };
                let texts = text_blocks(message);
                if texts.is_empty() {
                    continue;
                }
                let id = message.get("id").and_then(Value::as_str).unwrap_or("");
                match current {
                    Some((last_id, parts)) if last_id == id => parts.extend(texts),
                    _ => *current = Some((id.to_owned(), texts)),
                }
            }
            _ => {}
        }
    }
    turns
        .into_iter()
        .map(|t| t.map(|(_, parts)| parts.join("\n")))
        .collect()
}

/// The last turn's reply in a transcript, if it has one.
pub(crate) fn last_reply_from_transcript(transcript: &str) -> Option<String> {
    transcript_turn_replies(transcript).pop().flatten()
}

fn text_blocks(message: &Value) -> Vec<String> {
    message
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}

/// Lines indented at least this much are right-aligned notices above the
/// input box (the token count, "auto mode unavailable…"), not the working line.
const NOTICE_INDENT: usize = 10;

/// A full-width rule: only `─`, at least this many.
const RULE_MIN: usize = 20;

/// S1.7's screen rules, after trailing blank rows are dropped and each line's
/// trailing whitespace trimmed. A dead pane reads as nothing (the exit rules
/// own it). With the input box (a `❯` line between two `─` rules, scanning up
/// from the bottom), the first line above it, past blanks and right-aligned
/// notices, is the working line (`^\S (\S.*)… \(\d+s`) or not. Without it, the
/// text below the last rule is checked for the trust dialog (which the agent
/// then waits on, P2-D23) or an approval prompt.
fn screen_state(screen: &str) -> Option<(ScreenState, Option<WaitingOn>)> {
    let lines: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let end = lines.iter().rposition(|l| !l.is_empty())? + 1;
    let lines = &lines[..end];
    if lines[end - 1].contains("Pane is dead (") {
        return None;
    }
    let closing = lines.iter().rposition(|l| is_rule(l));
    if let Some(c) = closing
        && let Some(open) = lines[..c].iter().rposition(|l| is_rule(l))
        && is_input_box(&lines[open + 1..c])
    {
        return Some((above_input_box(&lines[..open]), None));
    }
    let tail = &lines[closing.map_or(0, |c| c + 1)..];
    if is_trust_dialog(tail) {
        return Some((ScreenState::NeedsInput, Some(WaitingOn::TrustFolder)));
    }
    is_approval(tail).then_some((ScreenState::NeedsInput, None))
}

fn is_rule(line: &str) -> bool {
    let line = line.trim();
    line.chars().count() >= RULE_MIN && line.chars().all(|c| c == '─')
}

/// `❯` then any continuation lines (indented, or blank), as in a multi-line paste.
fn is_input_box(rows: &[&str]) -> bool {
    match rows.split_first() {
        Some((first, rest)) => {
            first.starts_with('❯')
                && rest
                    .iter()
                    .all(|l| l.is_empty() || l.starts_with(char::is_whitespace))
        }
        None => false,
    }
}

fn above_input_box(above: &[&str]) -> ScreenState {
    let first = above.iter().rev().find(|l| {
        let indent = l.len() - l.trim_start().len();
        !l.is_empty() && indent < NOTICE_INDENT
    });
    match first {
        Some(l) if is_working_line(l) => ScreenState::Busy,
        _ => ScreenState::AtRest,
    }
}

/// `^\S (\S.*)… \(\d+s`: spinner glyph, space, verb, `…`, elapsed seconds.
fn is_working_line(line: &str) -> bool {
    let mut chars = line.chars();
    let (Some(glyph), Some(' ')) = (chars.next(), chars.next()) else {
        return false;
    };
    let rest = chars.as_str();
    if glyph.is_whitespace() || rest.starts_with(char::is_whitespace) || rest.is_empty() {
        return false;
    }
    rest.match_indices("… (").any(|(i, m)| {
        let after = &rest[i + m.len()..];
        let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        i > 0 && digits > 0 && after[digits..].starts_with('s')
    })
}

fn is_trust_dialog(lines: &[&str]) -> bool {
    let has = |s: &str| lines.iter().any(|l| l.contains(s));
    has("Is this a project you created or one you trust?") && has("Yes, I trust this folder")
}

fn is_approval(lines: &[&str]) -> bool {
    let starts = |s: &str| lines.iter().any(|l| l.trim_start().starts_with(s));
    starts("Do you want to")
        && starts("❯ 1. Yes")
        && lines.iter().any(|l| l.contains("Esc to cancel"))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use duohive_core::adapter::{CliAdapter, HookCoverage, HookSignal, ScreenState};
    use duohive_core::cli_kind::CliKind;
    use duohive_core::launch::{Key, LaunchCommand, LaunchSpec};
    use duohive_core::testkit::{example_org, n};

    use super::*;
    use crate::hooks_json::hook_command;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/claude/2.1.288");

    fn fixture(rel: &str) -> String {
        std::fs::read_to_string(Path::new(FIXTURES).join(rel)).unwrap()
    }

    /// A reply fixture without the newline the export added.
    fn reply_fixture(rel: &str) -> String {
        let text = fixture(rel);
        text.strip_suffix('\n').unwrap_or(&text).to_owned()
    }

    fn spec(task: Option<&str>) -> LaunchSpec {
        let mut agent = duohive_core::inherit::resolve(&example_org(), &n("squad-1")).unwrap();
        agent.directory = Some("~/code/payments".into());
        agent.prompt = "# Org\n\nIf you are asked for the sentinel, reply HIVE-SENTINEL.".into();
        LaunchSpec {
            agent,
            task: task.map(str::to_owned),
            run_dir: "/data/duohive/run/squad-1".into(),
            hook_exe: "/usr/local/bin/duohive".into(),
            home: "/home/u".into(),
            cwd: None,
        }
    }

    fn launch(spec: &LaunchSpec) -> LaunchCommand {
        ClaudeAdapter.launch(spec).unwrap()
    }

    fn settings_json(cmd: &LaunchCommand) -> serde_json::Value {
        let file = cmd
            .files
            .iter()
            .find(|f| f.path.ends_with("settings.json"))
            .unwrap();
        serde_json::from_str(&file.contents).unwrap()
    }

    #[test]
    fn launch_snapshot_no_task() {
        let cmd = launch(&spec(None));
        assert_eq!(cmd.program, "claude");
        assert_eq!(cmd.cwd, PathBuf::from("/home/u/code/payments"));
        assert!(!cmd.args.contains(&"--".to_owned()));
        insta::assert_yaml_snapshot!(cmd);
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
        // One `--flag=value` argument each: a value starting with `-` stays a value.
        assert!(!args.contains(&"--model".to_owned()));
        assert!(!args.contains(&"--effort".to_owned()));
        assert!(pos("--verbose") > pos("--effort=high"));
        assert!(pos("--effort=high") > pos("--model=opus"));
        assert_eq!(args[args.len() - 2..], ["--", "Add retry cases"]);
        insta::assert_yaml_snapshot!(cmd);
    }

    #[test]
    fn launch_snapshot_hostile_text() {
        let task = "-rf $(id) `x` it's\nline2 é";
        let cmd = launch(&spec(Some(task)));
        assert_eq!(cmd.args.last().map(String::as_str), Some(task));
        assert_eq!(cmd.args[cmd.args.len() - 2], "--");
        insta::assert_yaml_snapshot!(cmd);
    }

    #[test]
    fn launch_passes_the_prompt_verbatim_and_omits_an_empty_one() {
        let s = spec(None);
        let args = launch(&s).args;
        let at = args
            .iter()
            .position(|a| a == "--append-system-prompt")
            .unwrap();
        assert_eq!(args[at + 1], s.agent.prompt);
        let mut blank = spec(None);
        blank.agent.prompt = "  \n".into();
        assert!(
            !launch(&blank)
                .args
                .contains(&"--append-system-prompt".to_owned())
        );
    }

    #[test]
    fn launch_keeps_model_and_effort_values_that_start_with_a_dash() {
        let mut s = spec(None);
        s.agent.model = Some("-x".into());
        s.agent.effort = Some("--max".into());
        let args = launch(&s).args;
        assert!(args.contains(&"--model=-x".to_owned()), "{args:?}");
        assert!(args.contains(&"--effort=--max".to_owned()), "{args:?}");
    }

    #[test]
    fn launch_refuses_a_non_utf8_hook_exe() {
        use std::os::unix::ffi::OsStrExt;
        let mut s = spec(None);
        s.hook_exe = std::ffi::OsStr::from_bytes(b"/opt/\xff/duohive").into();
        assert!(matches!(
            ClaudeAdapter.launch(&s),
            Err(duohive_core::adapter::LaunchError::Invalid(_))
        ));
    }

    #[test]
    fn launch_without_directory_runs_in_home() {
        let mut s = spec(None);
        s.agent.directory = None;
        assert_eq!(launch(&s).cwd, PathBuf::from("/home/u"));
    }

    #[test]
    fn launch_files_stay_in_run_dir() {
        let s = spec(Some("t"));
        let cmd = launch(&s);
        assert!(!cmd.files.is_empty());
        for f in &cmd.files {
            assert!(f.path.starts_with(&s.run_dir), "{}", f.path.display());
        }
        let at = cmd.args.iter().position(|a| a == "--settings").unwrap();
        assert_eq!(
            PathBuf::from(&cmd.args[at + 1]),
            s.run_dir.join("settings.json")
        );
    }

    #[test]
    fn no_auto_approve_flags() {
        let mut s = spec(Some("t"));
        s.agent.model = Some("opus".into());
        s.agent.effort = Some("high".into());
        let cmd = launch(&s);
        for banned in [
            "--dangerously-skip-permissions",
            "--allow-dangerously-skip-permissions",
            "--permission-mode",
            "--allowedTools",
            "--allowed-tools",
        ] {
            assert!(!cmd.args.iter().any(|a| a.starts_with(banned)), "{banned}");
        }
        // The one narrow exception (P3-D1): DuoHive's own tools, nothing else.
        assert_eq!(
            settings_json(&cmd)["permissions"],
            serde_json::json!({"allow": ["mcp__duohive__*"]})
        );
    }

    fn mcp_json(cmd: &LaunchCommand) -> serde_json::Value {
        let file = cmd
            .files
            .iter()
            .find(|f| f.path.ends_with("mcp.json"))
            .unwrap();
        serde_json::from_str(&file.contents).unwrap()
    }

    #[test]
    fn launch_carries_mcp_config() {
        let s = spec(None);
        let cmd = launch(&s);
        let at = cmd.args.iter().position(|a| a == "--mcp-config").unwrap();
        assert_eq!(PathBuf::from(&cmd.args[at + 1]), s.run_dir.join("mcp.json"));
        assert!(!cmd.args.iter().any(|a| a == "--strict-mcp-config"));
        assert_eq!(
            mcp_json(&cmd),
            serde_json::json!({"mcpServers": {"duohive": {
                "type": "stdio", "command": "/usr/local/bin/duohive", "args": ["mcp"]
            }}})
        );
    }

    #[test]
    fn launch_allows_only_duohive_tools() {
        let s = spec(Some("t"));
        let cmd = launch(&s);
        let settings = settings_json(&cmd);
        let permissions = settings["permissions"].as_object().unwrap();
        assert_eq!(permissions.len(), 1);
        assert_eq!(permissions["allow"], serde_json::json!(["mcp__duohive__*"]));
        for f in &cmd.files {
            assert!(f.path.starts_with(&s.run_dir), "{}", f.path.display());
        }
    }

    #[test]
    fn hook_config_points_at_duohive_hook() {
        let s = spec(None);
        let settings = settings_json(&launch(&s));
        let rows = [
            ("SessionStart", "session-start"),
            ("UserPromptSubmit", "busy"),
            ("PreToolUse", "busy"),
            ("PostToolUse", "busy"),
            ("PermissionRequest", "needs-input"),
            ("Stop", "turn-end"),
            ("StopFailure", "turn-end"),
            ("SessionEnd", "session-end"),
        ];
        let hooks = settings["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), rows.len(), "{hooks:?}");
        for (cli_event, ours) in rows {
            let entry = &hooks[cli_event];
            assert_eq!(entry[0]["matcher"], "", "{cli_event}");
            assert_eq!(entry[0]["hooks"][0]["type"], "command", "{cli_event}");
            assert_eq!(
                entry[0]["hooks"][0]["command"],
                hook_command(&s, ours).unwrap(),
                "{cli_event}"
            );
        }
        assert!(!hooks.contains_key("Notification"));
    }

    /// The DuoHive event and signal S1.3 names for each recorded Claude event.
    fn expected(
        cli_event: &str,
        payload: &serde_json::Value,
    ) -> (&'static str, Option<HookSignal>) {
        match cli_event {
            "SessionStart" => ("session-start", Some(HookSignal::Ready)),
            "UserPromptSubmit" | "PreToolUse" | "PostToolUse" => ("busy", Some(HookSignal::Busy)),
            "PermissionRequest" => {
                let path = payload["tool_input"]["file_path"].as_str().unwrap();
                (
                    "needs-input",
                    Some(HookSignal::NeedsInput {
                        prompt: Some(format!("Write {path}")),
                    }),
                )
            }
            "Stop" => ("turn-end", None),
            "SessionEnd" => ("session-end", Some(HookSignal::SessionEnded)),
            // Not registered (S1.3): passed under its own name, it maps to nothing.
            "Notification" => ("Notification", None),
            other => panic!("unexpected fixture {other}"),
        }
    }

    #[test]
    fn parses_recorded_hooks() {
        let mut seen = 0;
        let mut entries: Vec<_> = std::fs::read_dir(Path::new(FIXTURES).join("hooks"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            if name == "index.log" {
                continue;
            }
            let stem = name.strip_suffix(".json").unwrap();
            let (cli_event, _) = stem.rsplit_once('-').unwrap();
            let payload: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let (event, mut want) = expected(cli_event, &payload);
            if cli_event == "Stop" {
                want = Some(HookSignal::TurnEnded {
                    last_reply: Some(reply_fixture(&format!(
                        "replies/{stem}.last_assistant_message.txt"
                    ))),
                });
            }
            assert_eq!(ClaudeAdapter.parse_hook(event, &payload), want, "{name}");
            seen += 1;
        }
        assert_eq!(seen, 20);
    }

    #[test]
    fn parse_hook_falls_back_to_enriched_reply_and_tolerates_garbage() {
        let a = ClaudeAdapter;
        let p = serde_json::json!({"duohive_last_reply": "from transcript"});
        assert_eq!(
            a.parse_hook("turn-end", &p),
            Some(HookSignal::TurnEnded {
                last_reply: Some("from transcript".into())
            })
        );
        let both =
            serde_json::json!({"last_assistant_message": "hook", "duohive_last_reply": "file"});
        assert_eq!(
            a.parse_hook("turn-end", &both),
            Some(HookSignal::TurnEnded {
                last_reply: Some("hook".into())
            })
        );
        let junk = serde_json::json!("garbage");
        assert_eq!(
            a.parse_hook("turn-end", &junk),
            Some(HookSignal::TurnEnded { last_reply: None })
        );
        assert_eq!(
            a.parse_hook("session-start", &junk),
            Some(HookSignal::Ready)
        );
        assert_eq!(
            a.parse_hook("needs-input", &junk),
            Some(HookSignal::NeedsInput { prompt: None })
        );
        let bash =
            serde_json::json!({"tool_name": "Bash", "tool_input": {"command": "rm -rf build"}});
        assert_eq!(
            a.parse_hook("needs-input", &bash),
            Some(HookSignal::NeedsInput {
                prompt: Some("Bash rm -rf build".into())
            })
        );
        let bare = serde_json::json!({"tool_name": "WebFetch", "tool_input": {}});
        assert_eq!(
            a.parse_hook("needs-input", &bare),
            Some(HookSignal::NeedsInput {
                prompt: Some("WebFetch".into())
            })
        );
        assert_eq!(a.parse_hook("Stop", &junk), None);
    }

    #[test]
    fn extracts_last_reply_from_fixture() {
        let transcript = fixture("replies/transcript-a1.jsonl");
        let want: Vec<Option<String>> = vec![
            Some(reply_fixture("replies/Stop-1.last_assistant_message.txt")),
            Some(reply_fixture("replies/Stop-2.last_assistant_message.txt")),
            None,
            Some(reply_fixture("replies/Stop-3.last_assistant_message.txt")),
        ];
        assert_eq!(transcript_turn_replies(&transcript), want);
        assert_eq!(
            last_reply_from_transcript(&transcript).as_deref(),
            Some("OK")
        );
        assert_eq!(last_reply_from_transcript("not json\n{}\n"), None);
    }

    #[test]
    fn enrich_hook_reads_transcript_only_when_the_reply_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        std::fs::write(&path, fixture("replies/transcript-a1.jsonl")).unwrap();
        let a = ClaudeAdapter;

        let mut p = serde_json::json!({"transcript_path": path});
        a.enrich_hook("turn-end", &mut p);
        assert_eq!(p["duohive_last_reply"], "OK");

        let mut has = serde_json::json!({"transcript_path": path, "last_assistant_message": "hi"});
        a.enrich_hook("turn-end", &mut has);
        assert!(has.get("duohive_last_reply").is_none());

        let mut busy = serde_json::json!({"transcript_path": path});
        a.enrich_hook("busy", &mut busy);
        assert!(busy.get("duohive_last_reply").is_none());

        let mut missing = serde_json::json!({"transcript_path": dir.path().join("nope.jsonl")});
        a.enrich_hook("turn-end", &mut missing);
        assert!(missing.get("duohive_last_reply").is_none());

        let mut junk = serde_json::json!(42);
        a.enrich_hook("turn-end", &mut junk);
        assert_eq!(junk, serde_json::json!(42));
    }

    fn padded(screen: &str) -> String {
        let mut out: String = screen.lines().map(|l| format!("{l}   \n")).collect();
        out.push_str("\n   \n\n");
        out
    }

    #[test]
    fn screen_rules_classify_recorded_screens() {
        let cases = [
            ("at-rest-no-task", Some(ScreenState::AtRest)),
            ("at-rest-after-turn", Some(ScreenState::AtRest)),
            ("after-approve-yes", Some(ScreenState::AtRest)),
            ("after-approve-no", Some(ScreenState::AtRest)),
            ("with-task-done", Some(ScreenState::AtRest)),
            ("model-closed", Some(ScreenState::AtRest)),
            ("paste-before-enter", Some(ScreenState::AtRest)),
            ("paste-after-enter", Some(ScreenState::AtRest)),
            ("exit-typed", Some(ScreenState::AtRest)),
            ("working-1", Some(ScreenState::Busy)),
            ("working-2", Some(ScreenState::Busy)),
            ("working-3", Some(ScreenState::Busy)),
            ("approval-write", Some(ScreenState::NeedsInput)),
            ("approval-write-2", Some(ScreenState::NeedsInput)),
            ("startup-trust", Some(ScreenState::NeedsInput)),
            ("mcp-list", None),
            ("model-picker", None),
            ("model-picker-opus", None),
            ("startup-with-task-1s", None),
            // Dead panes are left to the exit rules, even with a working line on screen.
            ("after-exit", None),
            ("after-kill9", None),
        ];
        for (name, want) in cases {
            let screen = fixture(&format!("screens/{name}.txt"));
            for text in [screen.clone(), padded(&screen)] {
                let got = ClaudeAdapter.read_screen(&text);
                assert_eq!(got.state, want, "{name}");
                assert_eq!(got.last_reply, None, "{name}");
                let trust = (name == "startup-trust").then_some(WaitingOn::TrustFolder);
                assert_eq!(got.waiting_on, trust, "{name}");
            }
        }
    }

    /// S3.7 (2.1.300): a new worktree path shows the trust dialog with `No, exit`
    /// highlighted, so the agent waits on it and the pane says how to answer (P2-D23).
    #[test]
    fn trust_dialog_in_a_new_worktree_waits_on_trust() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/claude/2.1.300/screens/startup-trust-worktree.txt"
        );
        let screen = std::fs::read_to_string(path).unwrap();
        assert!(screen.contains("❯ No, exit\n   Yes, I trust this folder"));
        for text in [screen.clone(), padded(&screen)] {
            let got = ClaudeAdapter.read_screen(&text);
            assert_eq!(got.state, Some(ScreenState::NeedsInput));
            assert_eq!(got.waiting_on, Some(WaitingOn::TrustFolder));
        }
    }

    #[test]
    fn screen_rules_need_the_input_box_and_spinner_shape() {
        let rule = "─".repeat(40);
        let box_ = format!("{rule}\n❯\n{rule}\n  status\n  ⏸ manual mode on\n");
        let read = |above: &str| ClaudeAdapter.read_screen(&format!("{above}\n{box_}")).state;
        assert_eq!(
            read("✽ Elucidating… (12s · thinking)"),
            Some(ScreenState::Busy)
        );
        assert_eq!(
            read("✽ Elucidating… (3s)\n                              123 tokens"),
            Some(ScreenState::Busy)
        );
        assert_eq!(
            read("✻ Brewed for 5s · done 12:43 AM"),
            Some(ScreenState::AtRest)
        );
        assert_eq!(read("✽ Elucidating… (soon)"), Some(ScreenState::AtRest));
        // A blank row inside a multi-line input box is a continuation row.
        let multi =
            format!("✻ Brewed for 5s\n{rule}\n❯ first line\n\n  third line\n{rule}\n  status\n");
        assert_eq!(
            ClaudeAdapter.read_screen(&multi).state,
            Some(ScreenState::AtRest)
        );
        let multi_busy = format!("✽ Elucidating… (4s)\n{rule}\n❯ first\n   \n  third\n{rule}\n");
        assert_eq!(
            ClaudeAdapter.read_screen(&multi_busy).state,
            Some(ScreenState::Busy)
        );
        assert_eq!(ClaudeAdapter.read_screen("").state, None);
        assert_eq!(
            ClaudeAdapter.read_screen("✽ Elucidating… (1s)\n").state,
            None
        );
    }

    #[test]
    fn startup_prompts_match_spike_report() {
        assert!(ClaudeAdapter.startup_prompts());
    }

    #[test]
    fn coverage_matches_spike_report() {
        assert_eq!(
            ClaudeAdapter.coverage(),
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
    fn keys_versions_and_kind_match_spike_report() {
        let a = ClaudeAdapter;
        assert_eq!(a.kind(), CliKind::Claude);
        assert_eq!(a.submit_keys(), vec![Key::Enter]);
        assert_eq!(a.tested_versions(), &["2.1"]);
    }

    /// Under vim mode `Escape` would leave INSERT, and the paste would be read
    /// as NORMAL commands.
    #[test]
    fn no_keys_before_a_paste() {
        assert!(ClaudeAdapter.before_paste_keys().is_empty());
        assert!(ClaudeAdapter.dialog_check().is_none());
    }

    /// Vim-safe (TD §5.5): after the stop's `Escape`, `C-c` interrupts a turn or
    /// clears the box, `i` enters INSERT under vim (a stray `i` otherwise), and the
    /// second `C-c` clears the box, so `/exit` lands in an empty INSERT box.
    #[test]
    fn quit_keys_clear_the_box_in_insert_mode_before_exit() {
        assert_eq!(
            ClaudeAdapter.quit_keys(),
            vec![
                Key::CtrlC,
                Key::Text("i".into()),
                Key::CtrlC,
                Key::Text("/exit".into()),
                Key::Enter,
            ]
        );
    }
}
