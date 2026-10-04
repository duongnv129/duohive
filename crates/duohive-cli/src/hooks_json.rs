//! Shared hook wiring: the shell command every hook runs, and the
//! command-hook settings shape (`{"hooks": {"<Event>": [...]}}`, spike S1.3).

use duohive_core::launch::{LaunchSpec, sh_quote};
use serde_json::{Map, Value, json};

/// The shell command a CLI hook runs: `'<hook_exe>' hook '<agent>' <event>`,
/// with the executable and agent name sh-quoted. `event` is one of DuoHive's
/// fixed event names, so it is left bare.
pub fn hook_command(spec: &LaunchSpec, event: &str) -> String {
    format!(
        "{} hook {} {event}",
        sh_quote(&spec.hook_exe.to_string_lossy()),
        sh_quote(spec.agent.name.as_str()),
    )
}

/// `{"hooks": {...}}` with one command hook (matcher `""`) per
/// `(cli_event, duohive_event)` row, each running [`hook_command`].
pub fn command_hooks(spec: &LaunchSpec, rows: &[(&str, &str)]) -> Value {
    let hooks: Map<String, Value> = rows
        .iter()
        .map(|(cli_event, ours)| {
            let entry = json!([{
                "matcher": "",
                "hooks": [{"type": "command", "command": hook_command(spec, ours)}],
            }]);
            ((*cli_event).to_owned(), entry)
        })
        .collect();
    json!({ "hooks": hooks })
}

#[cfg(test)]
mod tests {
    use duohive_core::launch::LaunchSpec;
    use duohive_core::testkit::{example_org, n};

    use super::*;

    fn spec(hook_exe: &str) -> LaunchSpec {
        LaunchSpec {
            agent: duohive_core::inherit::resolve(&example_org(), &n("squad-1")).unwrap(),
            task: None,
            run_dir: "/run/squad-1".into(),
            hook_exe: hook_exe.into(),
            home: "/h".into(),
        }
    }

    #[test]
    fn hook_command_quotes_paths_with_spaces() {
        let s = spec("/Applications/My Tools/duohive");
        let cmd = hook_command(&s, "turn-end");
        assert_eq!(
            cmd,
            "'/Applications/My Tools/duohive' hook 'squad-1' turn-end"
        );
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("set -- {cmd}; printf '%s|' \"$@\""))
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            "/Applications/My Tools/duohive|hook|squad-1|turn-end|"
        );
    }

    #[test]
    fn hook_command_escapes_single_quotes() {
        let s = spec("/opt/it's/duohive");
        assert_eq!(
            hook_command(&s, "busy"),
            r"'/opt/it'\''s/duohive' hook 'squad-1' busy"
        );
    }

    #[test]
    fn command_hooks_use_the_standard_shape() {
        let s = spec("/bin/duohive");
        let v = command_hooks(&s, &[("Stop", "turn-end"), ("SessionEnd", "session-end")]);
        assert_eq!(
            v,
            serde_json::json!({"hooks": {
                "Stop": [{"matcher": "", "hooks": [{"type": "command",
                    "command": "'/bin/duohive' hook 'squad-1' turn-end"}]}],
                "SessionEnd": [{"matcher": "", "hooks": [{"type": "command",
                    "command": "'/bin/duohive' hook 'squad-1' session-end"}]}],
            }})
        );
    }
}
