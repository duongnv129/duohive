//! The OpenCode adapter (spike S2, OpenCode 1.18.34).
//!
//! Everything goes in per-launch files under the run dir (S2.2, S2.3, S2.6,
//! S2.13): `opencode.json`, named by `OPENCODE_CONFIG`, which OpenCode merges
//! over the user's global config; `instructions.md`, the stacked prompt; and
//! `opencode-plugin.js`, DuoHive's plugin, which reports hooks by running
//! `duohive hook` and makes `edit` and `bash` ask (S2.11).

use std::fmt::Write as _;
use std::path::Path;

use duohive_core::adapter::{
    CliAdapter, CliPresence, HookCoverage, HookSignal, LaunchError, ScreenSignal, ScreenState,
};
use duohive_core::cli_kind::CliKind;
use duohive_core::launch::{Key, LaunchCommand, LaunchFile, LaunchSpec};
use serde_json::{Map, Value, json};

use crate::detect::probe_version;
use crate::{launch_cwd, utf8};

/// The plugin asset, written to the run dir on every launch.
pub(crate) const PLUGIN_SOURCE: &str = include_str!("assets/opencode-plugin.js");

const CONFIG_FILE: &str = "opencode.json";
const PLUGIN_FILE: &str = "opencode-plugin.js";
const INSTRUCTIONS_FILE: &str = "instructions.md";

/// The environment the plugin reads: which executable to run and for which agent.
const HOOK_EXE_VAR: &str = "DUOHIVE_HOOK_EXE";
const HOOK_AGENT_VAR: &str = "DUOHIVE_HOOK_AGENT";

/// The payload field the plugin adds to `session.idle` (S2.4).
const REPLY_FIELD: &str = "duohive_last_reply";

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
        &["1.18"]
    }

    /// Every event has a source in the plugin, which also adds the reply (S2.3, S2.4).
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

    /// `OPENCODE_CONFIG=<run>/opencode.json opencode [--model=m]
    /// [--agent=build] [flags…] [--prompt=<task>]` (S2.6, S2.8, S2.11). Effort is the build agent's
    /// `variant` in the config; there is no `--` (the positional argument is
    /// the project directory), and `--prompt=` keeps a task starting with `-`.
    fn launch(&self, spec: &LaunchSpec) -> Result<LaunchCommand, LaunchError> {
        let hook_exe = utf8(&spec.hook_exe, "hook executable")?;
        let agent = &spec.agent;
        let config_path = spec.run_dir.join(CONFIG_FILE);
        let plugin_path = spec.run_dir.join(PLUGIN_FILE);
        let instructions_path = spec.run_dir.join(INSTRUCTIONS_FILE);
        let config_env = utf8(&config_path, "run directory")?.to_owned();

        let mut config = Map::new();
        let mut files = Vec::new();
        if !agent.prompt.trim().is_empty() {
            config.insert(
                "instructions".into(),
                json!([utf8(&instructions_path, "run directory")?]),
            );
            files.push(LaunchFile {
                path: instructions_path,
                contents: agent.prompt.clone(),
            });
        }
        config.insert("plugin".into(), json!([file_url(&plugin_path)?]));
        if let Some(effort) = &agent.effort {
            config.insert("agent".into(), json!({"build": {"variant": effort}}));
        }
        let config = serde_json::to_string_pretty(&Value::Object(config)).map_err(|e| {
            LaunchError::Invalid(format!("couldn't build the OpenCode config: {e}"))
        })?;
        files.push(LaunchFile {
            path: config_path,
            contents: config,
        });
        files.push(LaunchFile {
            path: plugin_path,
            contents: PLUGIN_SOURCE.to_owned(),
        });

        let mut args = Vec::new();
        if let Some(model) = &agent.model {
            args.push(format!("--model={model}"));
        }
        // The plugin composes approvals for the agents it knows; pin `build`
        // unless the user's flags choose an agent themselves.
        if !agent
            .flags
            .iter()
            .any(|f| f == "--agent" || f.starts_with("--agent="))
        {
            args.push("--agent=build".to_owned());
        }
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
        }
    }

    fn quit_keys(&self) -> Vec<Key> {
        vec![Key::Text("/exit".into()), Key::Enter]
    }

    fn submit_keys(&self) -> Vec<Key> {
        vec![Key::Enter]
    }

    /// No trust, login or update prompt was seen (S2.8).
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

/// `<permission> <metadata.filepath>` from a `permission.asked` event, else
/// `<permission> <patterns joined by ", ">`, else the permission alone.
fn approval_prompt(payload: &Value) -> Option<String> {
    let p = payload.get("properties")?;
    let permission = p.get("permission").and_then(Value::as_str)?;
    let filepath = p
        .pointer("/metadata/filepath")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let patterns = p
        .get("patterns")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|s| !s.is_empty());
    Some(match filepath.or(patterns) {
        Some(detail) => format!("{permission} {detail}"),
        None => permission.to_owned(),
    })
}

/// S2.7's screen rules, after trailing blank rows are dropped and each line's
/// trailing whitespace trimmed. A dead pane reads as nothing (the exit rules
/// own it). An approval prompt (`△ Permission required` with a later
/// `Allow once  Allow always  Reject` line, and no input box below them)
/// needs input. Otherwise the last input-box bottom border (`╹` then only
/// `▀`) decides: the footer line just below it shows `esc interrupt` while
/// working, and anything else at rest. No border, no state.
fn screen_state(screen: &str) -> Option<ScreenState> {
    let lines: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let end = lines.iter().rposition(|l| !l.is_empty())? + 1;
    let lines = &lines[..end];
    if lines[end - 1].contains("Pane is dead (") {
        return None;
    }
    let border = lines.iter().rposition(|l| is_border(l));
    let asked = lines
        .iter()
        .rposition(|l| l.contains("△ Permission required"));
    if let Some(asked) = asked
        && border.is_none_or(|b| b < asked)
        && lines[asked..].iter().any(|l| is_approval_choices(l))
    {
        return Some(ScreenState::NeedsInput);
    }
    let footer = lines.get(border? + 1).copied().unwrap_or("");
    Some(if footer.contains("esc interrupt") {
        ScreenState::Busy
    } else {
        ScreenState::AtRest
    })
}

fn is_border(line: &str) -> bool {
    line.trim_start()
        .strip_prefix('╹')
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c == '▀'))
}

fn is_approval_choices(line: &str) -> bool {
    ["Allow once", "Allow always", "Reject"]
        .iter()
        .all(|s| line.contains(s))
}

#[cfg(test)]
#[path = "opencode_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "opencode_plugin_tests.rs"]
mod plugin_tests;
