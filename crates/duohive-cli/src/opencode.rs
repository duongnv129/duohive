//! The OpenCode adapter (spike S2, ported to OpenCode 2.0.29 by its addendum S2.14).
//!
//! Everything goes in per-launch files under the run dir: `opencode.json`,
//! named by `OPENCODE_CONFIG`, which OpenCode merges over the user's global
//! config; `instructions.md`, the stacked prompt; and `opencode-plugin/index.js`,
//! DuoHive's directory plugin, which reports hooks by running `duohive hook`,
//! adds the instructions to the agent's sessions and makes `edit` and `shell`
//! ask (S2.14). `opencode --standalone` keeps the plugin in this launch's own
//! server, which reads `OPENCODE_CONFIG`.

use std::fmt::Write as _;
use std::path::Path;

use duohive_core::adapter::{
    CliAdapter, CliPresence, DialogCheck, HookCoverage, HookSignal, LaunchError, ScreenSignal,
    ScreenState,
};
use duohive_core::cli_kind::CliKind;
use duohive_core::launch::{Key, LaunchCommand, LaunchFile, LaunchSpec};
use serde_json::{Map, Value, json};

use crate::detect::probe_version;
use crate::{launch_cwd, utf8};

/// The plugin asset, written to the run dir on every launch.
pub(crate) const PLUGIN_SOURCE: &str = include_str!("assets/opencode-plugin/index.js");

const CONFIG_FILE: &str = "opencode.json";
/// The plugin directory OpenCode 2.0 loads; it holds `index.js`.
const PLUGIN_DIR: &str = "opencode-plugin";
const PLUGIN_FILE: &str = "index.js";
const INSTRUCTIONS_FILE: &str = "instructions.md";
/// The agent DuoHive's launches start in (`default_agent`).
const DEFAULT_AGENT: &str = "build";

/// The environment the plugin reads: which executable to run and for which agent.
const HOOK_EXE_VAR: &str = "DUOHIVE_HOOK_EXE";
const HOOK_AGENT_VAR: &str = "DUOHIVE_HOOK_AGENT";

/// The payload field the plugin adds to each turn end (S2.4, S2.14).
const REPLY_FIELD: &str = "duohive_last_reply";
/// The turn end of a turn that failed (S2.14, P2-D24).
const FAILED_TURN: &str = "session.execution.failed";

/// OpenCode.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpencodeAdapter;

impl CliAdapter for OpencodeAdapter {
    fn kind(&self) -> CliKind {
        CliKind::Opencode
    }

    fn detect(&self) -> CliPresence {
        probe_version(self.kind())
    }

    fn tested_versions(&self) -> &'static [&'static str] {
        &["2.0"]
    }

    /// Every event has a source in the plugin, which also adds the reply (S2.14).
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

    /// `OPENCODE_CONFIG=<run>/opencode.json opencode --standalone [flags…]
    /// [--prompt=<task>]` (S2.14). OpenCode 2.0 rejects `--model` and `--agent`,
    /// so the model (with the effort as its `#variant`) and `default_agent` go
    /// in the config; an effort with no model is left out, since 2.0 has effort
    /// only inside a model. There is no `--` (the positional argument is the
    /// project directory), and `--prompt=` keeps a task starting with `-`.
    fn launch(&self, spec: &LaunchSpec) -> Result<LaunchCommand, LaunchError> {
        let hook_exe = utf8(&spec.hook_exe, "hook executable")?;
        let agent = &spec.agent;
        let config_path = spec.run_dir.join(CONFIG_FILE);
        let plugin_dir = spec.run_dir.join(PLUGIN_DIR);
        let instructions_path = spec.run_dir.join(INSTRUCTIONS_FILE);
        let config_env = utf8(&config_path, "run directory")?.to_owned();

        let mut files = Vec::new();
        let mut options = Map::new();
        if !agent.prompt.trim().is_empty() {
            options.insert(
                "instructions".into(),
                json!(utf8(&instructions_path, "run directory")?),
            );
            files.push(LaunchFile {
                path: instructions_path,
                contents: agent.prompt.clone(),
            });
        }
        let mut config = Map::new();
        config.insert(
            "plugins".into(),
            json!([{"package": file_url(&plugin_dir)?, "options": options}]),
        );
        // DuoHive's tools (P3-D1). Shape from S2.5 (1.18), re-checked on 2.0 in Task 3.2.
        config.insert(
            "mcp".into(),
            json!({"duohive": {"type": "local", "command": [hook_exe, "mcp"], "enabled": true}}),
        );
        if let Some(model) = &agent.model {
            let model = match &agent.effort {
                Some(effort) if !model.contains('#') => format!("{model}#{effort}"),
                _ => model.clone(),
            };
            config.insert("model".into(), json!(model));
        }
        // The TUI otherwise starts in the agent last used; the user's own
        // `--agent` flag, if any, is passed through below (and refused by 2.0).
        if !agent
            .flags
            .iter()
            .any(|f| f == "--agent" || f.starts_with("--agent="))
        {
            config.insert("default_agent".into(), json!(DEFAULT_AGENT));
        }
        let config = serde_json::to_string_pretty(&Value::Object(config)).map_err(|e| {
            LaunchError::Invalid(format!("couldn't build the OpenCode config: {e}"))
        })?;
        files.push(LaunchFile {
            path: config_path,
            contents: config,
        });
        files.push(LaunchFile {
            path: plugin_dir.join(PLUGIN_FILE),
            contents: PLUGIN_SOURCE.to_owned(),
        });

        let mut args = vec!["--standalone".to_owned()];
        args.extend(agent.flags.iter().cloned());
        if let Some(task) = &spec.task {
            args.push(format!("--prompt={task}"));
        }
        Ok(LaunchCommand {
            program: self.kind().binary().to_owned(),
            args,
            env: vec![
                ("OPENCODE_CONFIG".into(), config_env),
                (HOOK_EXE_VAR.into(), hook_exe.to_owned()),
                (HOOK_AGENT_VAR.into(), agent.name.as_str().to_owned()),
            ],
            cwd: launch_cwd(spec),
            files,
        })
    }

    fn parse_hook(&self, event: &str, payload: &Value) -> Option<HookSignal> {
        match event {
            "session-start" => Some(HookSignal::Ready),
            "busy" => Some(HookSignal::Busy),
            "needs-input" => Some(HookSignal::NeedsInput {
                prompt: approval_prompt(payload),
            }),
            "turn-end" if payload.get("type").and_then(Value::as_str) == Some(FAILED_TURN) => {
                Some(HookSignal::TurnFailed {
                    reason: failure_reason(payload),
                })
            }
            "turn-end" => Some(HookSignal::TurnEnded {
                last_reply: payload
                    .get(REPLY_FIELD)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
            }),
            "session-end" => Some(HookSignal::SessionEnded),
            _ => None,
        }
    }

    /// The reply comes from the plugin, so only the state is read (S2.7).
    fn read_screen(&self, screen: &str) -> ScreenSignal {
        ScreenSignal {
            state: screen_state(screen),
            last_reply: None,
            waiting_on: None,
        }
    }

    fn quit_keys(&self) -> Vec<Key> {
        vec![Key::Text("/exit".into()), Key::Enter]
    }

    fn submit_keys(&self) -> Vec<Key> {
        vec![Key::Enter]
    }

    /// `Escape` closed the "Update Available" dialog OpenCode 1.18.35 opened
    /// over the session, which swallowed typed text with `Confirm` highlighted.
    /// OpenCode 2.0.29 showed no such dialog, but one `Escape` at rest changes
    /// nothing on screen (S2.14: the footer's `esc again to interrupt` shows only
    /// while working), and deliveries happen only at rest (TD §5.5), so it stays
    /// as a guard.
    fn before_paste_keys(&self) -> Vec<Key> {
        vec![Key::Escape]
    }

    /// The update dialog can open in the key pause after `Escape` (friction
    /// 6), so the screen is read again before the paste and before `Enter`.
    fn dialog_check(&self) -> Option<DialogCheck> {
        Some(dialog_open)
    }

    /// No trust or login prompt was seen (S2.8, S2.14).
    fn startup_prompts(&self) -> bool {
        false
    }
}

/// `file://<path>`, percent-encoding everything but unreserved characters and `/`.
fn file_url(path: &Path) -> Result<String, LaunchError> {
    let path = utf8(path, "run directory")?;
    let mut url = String::from("file://");
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            url.push(char::from(b));
        } else {
            // Writing to a String cannot fail.
            let _ = write!(url, "%{b:02X}");
        }
    }
    Ok(url)
}

/// `<action> <resources joined by ", ">` from a `permission.asked` payload,
/// else the action alone; `question <question>` (else its title) from a
/// `form.created` one.
fn approval_prompt(payload: &Value) -> Option<String> {
    let p = payload.get("properties")?;
    let text = |key: &str| p.get(key).and_then(Value::as_str).filter(|s| !s.is_empty());
    if payload.get("type").and_then(Value::as_str) == Some("form.created") {
        return Some(match text("question").or_else(|| text("title")) {
            Some(q) => format!("question {q}"),
            None => "question".to_owned(),
        });
    }
    let action = text("action")?;
    let resources = p
        .get("resources")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|s| !s.is_empty());
    Some(match resources {
        Some(detail) => format!("{action} {detail}"),
        None => action.to_owned(),
    })
}

/// The row text for a failed turn: `turn failed: <message> (<status>)`, with
/// the error's type where it has no message (P2-D24).
fn failure_reason(payload: &Value) -> String {
    let error = payload.get("error");
    let field = |key: &str| {
        error
            .and_then(|e| e.get(key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let status = error.and_then(|e| e.get("status")).and_then(Value::as_u64);
    match (field("message").or_else(|| field("type")), status) {
        (Some(what), Some(code)) => format!("turn failed: {what} ({code})"),
        (Some(what), None) => format!("turn failed: {what}"),
        (None, Some(code)) => format!("turn failed ({code})"),
        (None, None) => "turn failed".to_owned(),
    }
}

/// S2.7's screen rules as S2.14 updated them for 2.0.29, after trailing blank
/// rows are dropped and each line's trailing whitespace trimmed. A dead pane
/// reads as nothing (the exit rules own it). An approval prompt
/// (`△ Permission required` with a later `Allow once  Always allow  Reject`
/// line) or a question form (a `Questions` line with a later `enter submit`
/// `esc dismiss` line), with no input box below it, needs input. Otherwise
/// the last input-box bottom border (`╹` then only `▀`) decides: the footer
/// line just below it shows `esc interrupt` (or, after one `Escape`,
/// `esc again to interrupt`) while working, and anything else at rest. No
/// border, no state.
fn screen_state(screen: &str) -> Option<ScreenState> {
    let lines: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let end = lines.iter().rposition(|l| !l.is_empty())? + 1;
    let lines = &lines[..end];
    if lines[end - 1].contains("Pane is dead (") {
        return None;
    }
    let border = lines.iter().rposition(|l| is_border(l));
    let open = |start: Option<usize>, choices: fn(&str) -> bool| {
        start.is_some_and(|at| {
            border.is_none_or(|b| b < at) && lines[at..].iter().any(|l| choices(l))
        })
    };
    let asked = lines
        .iter()
        .rposition(|l| l.contains("△ Permission required"));
    let form = lines.iter().rposition(|l| gutter_text(l) == "Questions");
    if open(asked, is_approval_choices) || open(form, is_form_keys) {
        return Some(ScreenState::NeedsInput);
    }
    let footer = lines.get(border? + 1).copied().unwrap_or("");
    Some(
        if footer.contains("esc interrupt") || footer.contains("esc again to interrupt") {
            ScreenState::Busy
        } else {
            ScreenState::AtRest
        },
    )
}

/// A dialog is drawn over the session: OpenCode's dialogs put their title and
/// `esc` on their top line (1.18.34's `/status`), so a line ending in `esc`
/// after a gap, with text before it, is one. "Update Available" anywhere also
/// counts: that dialog (1.18.35) was seen but not recorded. A false reading
/// only refuses the delivery, which the TUI offers again (P2-D11).
fn dialog_open(screen: &str) -> bool {
    screen.lines().any(|line| {
        let line = line.trim();
        line.contains("Update Available")
            || line
                .strip_suffix("esc")
                .is_some_and(|head| head.ends_with("  ") && !head.trim().is_empty())
    })
}

fn is_border(line: &str) -> bool {
    line.trim_start()
        .strip_prefix('╹')
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c == '▀'))
}

/// A line's text after the session's `┃` gutter.
fn gutter_text(line: &str) -> &str {
    let line = line.trim();
    line.strip_prefix('┃').map_or(line, str::trim)
}

fn is_approval_choices(line: &str) -> bool {
    line.contains("Allow once")
        && line.contains("Reject")
        && (line.contains("Always allow") || line.contains("Allow always"))
}

fn is_form_keys(line: &str) -> bool {
    line.contains("enter submit") && line.contains("esc dismiss")
}

#[cfg(test)]
#[path = "opencode_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "opencode_plugin_tests.rs"]
mod plugin_tests;
