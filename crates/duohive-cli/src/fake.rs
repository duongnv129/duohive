//! The fake CLI: `duohive bench-agent`, so the bench, the e2e tests and CI
//! need no real CLI installed.

use std::path::{Path, PathBuf};

use duohive_core::adapter::{
    Adapters, CliAdapter, CliPresence, HookCoverage, HookSignal, LaunchError, ModelList,
    ModelListError, ScreenSignal, ScreenState,
};
use duohive_core::cli_kind::CliKind;
use duohive_core::launch::{Key, LaunchCommand, LaunchSpec};
use serde_json::Value;

use crate::{launch_cwd, utf8};

/// The version every fake CLI reports.
const FAKE_VERSION: &str = "0.0.0-fake";
/// What the screen shows at the prompt, during a turn, and when asking.
const PROMPT: &str = "❯";
const WORKING: &str = "working…";
const ASKING: &str = "allow? (y/n)";

/// A CLI kind played by `<exe> bench-agent`; `hooks: false` plays a CLI with
/// no hooks at all, read from the screen only. With `duo_script`, Duo's
/// launches run that script of tool calls (`--mcp-script`): a scripted fake Duo
/// with no model behind it. Duo has no `flags` field to carry the path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeCliAdapter {
    /// The CLI kind it stands in for.
    pub kind: CliKind,
    /// The `duohive` executable.
    pub exe: PathBuf,
    /// Whether the fake agent fires hooks.
    pub hooks: bool,
    /// The script Duo runs, if any.
    pub duo_script: Option<PathBuf>,
    /// A directory the fakes stamp their prompt times into
    /// (`--stamp-file <dir>/<agent>`), for the bench's screen-read timing.
    pub stamp_dir: Option<PathBuf>,
}

/// Fake adapters for every kind: Claude Code with hooks, OpenCode without.
pub fn fake_adapters(exe: &Path) -> Adapters {
    scripted_fake_adapters(exe, None)
}

/// [`fake_adapters`], with every fake appending its prompt times to
/// `<dir>/<agent>`: the bench times the screen-read path from those stamps.
pub fn fake_adapters_with_stamps(exe: &Path, dir: &Path) -> Adapters {
    let fake = |kind, hooks| -> Box<dyn CliAdapter> {
        Box::new(FakeCliAdapter {
            kind,
            exe: exe.to_path_buf(),
            hooks,
            duo_script: None,
            stamp_dir: Some(dir.to_path_buf()),
        })
    };
    Adapters::new(vec![
        fake(CliKind::Claude, true),
        fake(CliKind::Opencode, false),
    ])
}

/// [`fake_adapters`], with Duo running `duo_script` on either kind.
pub fn scripted_fake_adapters(exe: &Path, duo_script: Option<&Path>) -> Adapters {
    let fake = |kind, hooks| -> Box<dyn CliAdapter> {
        Box::new(FakeCliAdapter {
            kind,
            exe: exe.to_path_buf(),
            hooks,
            duo_script: duo_script.map(Path::to_path_buf),
            stamp_dir: None,
        })
    };
    Adapters::new(vec![
        fake(CliKind::Claude, true),
        fake(CliKind::Opencode, false),
    ])
}

impl CliAdapter for FakeCliAdapter {
    fn kind(&self) -> CliKind {
        self.kind
    }

    /// Always found: nothing depends on which CLIs the machine has.
    fn detect(&self) -> CliPresence {
        CliPresence::Found {
            version: FAKE_VERSION.to_owned(),
        }
    }

    fn tested_versions(&self) -> &'static [&'static str] {
        &["0.0"]
    }

    fn coverage(&self) -> HookCoverage {
        let h = self.hooks;
        HookCoverage {
            ready: h,
            busy: h,
            needs_input: h,
            turn_end: h,
            session_end: h,
            last_reply: h,
        }
    }

    /// `<exe> bench-agent [--no-hooks] [--mcp-script <path>] [--task <task>]`,
    /// the task as one argument; the script only for Duo.
    fn launch(&self, spec: &LaunchSpec) -> Result<LaunchCommand, LaunchError> {
        let mut args = vec!["bench-agent".to_owned()];
        if !self.hooks {
            args.push("--no-hooks".to_owned());
        }
        if let Some(script) = self.duo_script.as_deref().filter(|_| spec.agent.is_duo) {
            args.extend([
                "--mcp-script".to_owned(),
                utf8(script, "fake Duo script")?.to_owned(),
            ]);
        }
        if let Some(dir) = &self.stamp_dir {
            args.extend([
                "--stamp-file".to_owned(),
                utf8(&dir.join(spec.agent.name.as_str()), "fake stamp file")?.to_owned(),
            ]);
        }
        if let Some(task) = &spec.task {
            args.extend(["--task".to_owned(), task.clone()]);
        }
        Ok(LaunchCommand {
            program: utf8(&self.exe, "fake CLI executable")?.to_owned(),
            args,
            env: Vec::new(),
            cwd: launch_cwd(spec),
            files: Vec::new(),
        })
    }

    fn parse_hook(&self, event: &str, payload: &Value) -> Option<HookSignal> {
        let text = |key: &str| payload[key].as_str().map(str::to_owned);
        match event {
            "session-start" => Some(HookSignal::Ready),
            "busy" => Some(HookSignal::Busy),
            "needs-input" => Some(HookSignal::NeedsInput {
                prompt: text("prompt"),
            }),
            "turn-end" => Some(HookSignal::TurnEnded {
                last_reply: text("last_reply"),
            }),
            "session-end" => Some(HookSignal::SessionEnded),
            _ => None,
        }
    }

    /// The last non-empty line, trailing whitespace trimmed (capture-pane pads
    /// rows and lines); at the prompt, the line above it is the reply.
    fn read_screen(&self, screen: &str) -> ScreenSignal {
        let mut lines = screen
            .lines()
            .rev()
            .map(str::trim_end)
            .filter(|l| !l.is_empty());
        let state = match lines.next() {
            Some(PROMPT) => Some(ScreenState::AtRest),
            Some(WORKING) => Some(ScreenState::Busy),
            Some(l) if l.ends_with(ASKING) => Some(ScreenState::NeedsInput),
            _ => None,
        };
        let last_reply = match state {
            Some(ScreenState::AtRest) => lines.next().map(str::to_owned),
            _ => None,
        };
        ScreenSignal {
            state,
            last_reply,
            waiting_on: None,
        }
    }

    fn quit_keys(&self) -> Vec<Key> {
        vec![Key::Text("/exit".to_owned()), Key::Enter]
    }

    fn submit_keys(&self) -> Vec<Key> {
        vec![Key::Enter]
    }

    fn list_models(&self) -> Result<ModelList, ModelListError> {
        Ok(ModelList {
            models: vec!["fake-small".to_owned(), "fake-large".to_owned()],
            efforts: vec!["low".to_owned(), "high".to_owned()],
            variants: Default::default(),
        })
    }

    fn startup_prompts(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use duohive_core::adapter::untested_notice;
    use duohive_core::inherit::resolve;
    use duohive_core::testkit::{example_org, n};

    use super::*;

    fn spec(task: Option<&str>) -> LaunchSpec {
        LaunchSpec {
            agent: resolve(&example_org(), &n("squad-1")).unwrap(),
            task: task.map(str::to_owned),
            run_dir: "/run/squad-1".into(),
            hook_exe: "/bin/duohive".into(),
            home: "/h".into(),
            cwd: None,
        }
    }

    #[test]
    fn fake_adapter_screen_rules() {
        let a = FakeCliAdapter {
            kind: CliKind::Opencode,
            exe: "duohive".into(),
            hooks: false,
            duo_script: None,
            stamp_dir: None,
        };
        // Trailing blank rows, as capture-pane gives.
        assert_eq!(
            a.read_screen("hello\n❯ \n\n\n").state,
            Some(ScreenState::AtRest)
        );
        assert_eq!(
            a.read_screen("working…  \n\n").state,
            Some(ScreenState::Busy)
        );
        assert_eq!(
            a.read_screen("Write file? allow? (y/n)\n\n").state,
            Some(ScreenState::NeedsInput)
        );
        assert_eq!(a.read_screen("something else\n").state, None);
        assert!(!a.coverage().complete());
    }

    #[test]
    fn fake_adapter_detects_without_a_real_cli() {
        let a = FakeCliAdapter {
            kind: CliKind::Claude,
            exe: "duohive".into(),
            hooks: true,
            duo_script: None,
            stamp_dir: None,
        };
        assert_eq!(
            a.detect(),
            CliPresence::Found {
                version: "0.0.0-fake".into()
            }
        );
        assert_eq!(
            untested_notice("bench-agent", "0.0.0-fake", a.tested_versions()),
            None
        );
        assert!(a.coverage().complete());
        assert!(!a.startup_prompts());
        assert_eq!(a.quit_keys(), vec![Key::Text("/exit".into()), Key::Enter]);
        assert_eq!(a.submit_keys(), vec![Key::Enter]);
    }

    #[test]
    fn fake_adapter_passes_the_task() {
        let a = FakeCliAdapter {
            kind: CliKind::Claude,
            exe: "/bin/duohive".into(),
            hooks: true,
            duo_script: None,
            stamp_dir: None,
        };
        let cmd = a.launch(&spec(Some("it's -rf $(id)"))).unwrap();
        assert_eq!(cmd.program, "/bin/duohive");
        assert_eq!(cmd.args, vec!["bench-agent", "--task", "it's -rf $(id)"]);
        let cmd = a.launch(&spec(None)).unwrap();
        assert_eq!(cmd.args, vec!["bench-agent"]);
        let quiet = FakeCliAdapter {
            hooks: false,
            ..a.clone()
        };
        assert_eq!(
            quiet.launch(&spec(Some("t"))).unwrap().args,
            vec!["bench-agent", "--no-hooks", "--task", "t"]
        );
    }

    #[test]
    fn fake_adapter_passes_the_script_to_duo_only() {
        let a = FakeCliAdapter {
            kind: CliKind::Claude,
            exe: "/bin/duohive".into(),
            hooks: true,
            duo_script: Some("/s/duo.json".into()),
            stamp_dir: None,
        };
        let mut duo = spec(Some("hi"));
        duo.agent = resolve(&example_org(), &n("Duo")).unwrap();
        assert_eq!(
            a.launch(&duo).unwrap().args,
            vec!["bench-agent", "--mcp-script", "/s/duo.json", "--task", "hi"]
        );
        assert_eq!(
            a.launch(&spec(Some("hi"))).unwrap().args,
            vec!["bench-agent", "--task", "hi"]
        );
    }

    #[test]
    fn fake_adapter_maps_the_five_events() {
        use duohive_core::adapter::HookSignal;
        let a = FakeCliAdapter {
            kind: CliKind::Claude,
            exe: "duohive".into(),
            hooks: true,
            duo_script: None,
            stamp_dir: None,
        };
        let p = serde_json::json!({"last_reply": "done", "sent_at_ms": 1});
        assert_eq!(a.parse_hook("session-start", &p), Some(HookSignal::Ready));
        assert_eq!(a.parse_hook("busy", &p), Some(HookSignal::Busy));
        assert_eq!(
            a.parse_hook("turn-end", &p),
            Some(HookSignal::TurnEnded {
                last_reply: Some("done".into())
            })
        );
        assert_eq!(
            a.parse_hook("needs-input", &serde_json::json!({"prompt": "ok?"})),
            Some(HookSignal::NeedsInput {
                prompt: Some("ok?".into())
            })
        );
        assert_eq!(
            a.parse_hook("session-end", &p),
            Some(HookSignal::SessionEnded)
        );
        assert_eq!(a.parse_hook("other", &p), None);
    }

    #[test]
    fn fake_adapters_hook_claude_only() {
        let adapters = fake_adapters(std::path::Path::new("/bin/duohive"));
        let claude = adapters.get(CliKind::Claude).unwrap();
        let opencode = adapters.get(CliKind::Opencode).unwrap();
        assert!(claude.coverage().complete());
        assert!(!opencode.coverage().session_end);
        assert_eq!(opencode.kind(), CliKind::Opencode);
    }
}
