//! The adapter port: what each agent CLI contributes (TD §5).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::cli_kind::CliKind;
use crate::launch::{Key, LaunchCommand, LaunchSpec};

/// What a hook tells the engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookSignal {
    /// The CLI is up and waiting for input.
    Ready,
    /// A turn began.
    Busy,
    /// The CLI is asking the user something.
    NeedsInput {
        /// The question, if the hook carries it.
        prompt: Option<String>,
    },
    /// A turn ended.
    TurnEnded {
        /// The agent's last reply, if the hook carries it.
        last_reply: Option<String>,
    },
    /// The CLI session ended.
    SessionEnded,
}

/// What a screen read says the agent is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScreenState {
    /// Working.
    Busy,
    /// Asking the user.
    NeedsInput,
    /// Waiting at its prompt.
    AtRest,
}

/// What a screen read found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenSignal {
    /// The state shown, if recognisable.
    pub state: Option<ScreenState>,
    /// The last reply shown, if recognisable.
    pub last_reply: Option<String>,
}

/// Which of the five Sn.3 events a CLI hooks, and where the reply comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookCoverage {
    /// Ready hook.
    pub ready: bool,
    /// Busy hook.
    pub busy: bool,
    /// Needs-input hook.
    pub needs_input: bool,
    /// Turn-end hook.
    pub turn_end: bool,
    /// Session-end hook.
    pub session_end: bool,
    /// The last reply arrives in a hook (not read from the screen).
    pub last_reply: bool,
}

impl HookCoverage {
    /// All five events are hooked and the reply comes from a hook.
    pub fn complete(&self) -> bool {
        self.ready
            && self.busy
            && self.needs_input
            && self.turn_end
            && self.session_end
            && self.last_reply
    }
}

/// Whether a CLI is installed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CliPresence {
    /// Not found.
    Missing,
    /// Found.
    Found {
        /// The version it reported.
        version: String,
    },
}

/// A launch that cannot be built.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LaunchError {
    /// The spec is unusable.
    #[error("{0}")]
    Invalid(String),
}

/// One agent CLI.
pub trait CliAdapter: Send + Sync {
    /// Which CLI this is.
    fn kind(&self) -> CliKind;
    /// Looks for the CLI; may run `<binary> --version`. Called off the core task, never by the engine.
    fn detect(&self) -> CliPresence;
    /// Tested versions as major.minor, oldest first.
    fn tested_versions(&self) -> &'static [&'static str];
    /// The hooks this CLI offers.
    fn coverage(&self) -> HookCoverage;
    /// Builds the launch command.
    fn launch(&self, spec: &LaunchSpec) -> Result<LaunchCommand, LaunchError>;
    /// Maps a hook to a signal; `None` for events DuoHive ignores.
    fn parse_hook(&self, event: &str, payload: &serde_json::Value) -> Option<HookSignal>;
    /// Reads what the screen shows.
    fn read_screen(&self, screen: &str) -> ScreenSignal;
    /// Keys that quit the CLI.
    fn quit_keys(&self) -> Vec<Key>;
    /// Keys that submit after a paste.
    fn submit_keys(&self) -> Vec<Key>;
    /// Sn.8 lists a start-up prompt (trust, login, update notice): D2, Task 13.
    fn startup_prompts(&self) -> bool;
    /// Adds to a hook payload before parsing (may read files). Called by the hook listener, never the engine.
    fn enrich_hook(&self, _event: &str, _payload: &mut serde_json::Value) {}
}

/// The adapters in use; shared as `Arc<Adapters>`.
pub struct Adapters(HashMap<CliKind, Box<dyn CliAdapter>>);

impl Adapters {
    /// Indexes the adapters by kind; a later one replaces an earlier one of the same kind.
    pub fn new(list: Vec<Box<dyn CliAdapter>>) -> Self {
        Self(list.into_iter().map(|a| (a.kind(), a)).collect())
    }

    /// The adapter for `k`.
    pub fn get(&self, k: CliKind) -> Option<&dyn CliAdapter> {
        self.0.get(&k).map(|a| &**a)
    }

    /// Each adapter's `detect()`, in `CliKind::ALL` order.
    pub fn detect_all(&self) -> Vec<(CliKind, CliPresence)> {
        CliKind::ALL
            .into_iter()
            .filter_map(|k| self.get(k).map(|a| (k, a.detect())))
            .collect()
    }
}

/// The UI §4.12 notice when `version`'s major.minor is not in `tested`.
pub fn untested_notice(binary: &str, version: &str, tested: &[&str]) -> Option<String> {
    let version = version.trim();
    let mut parts = version.split('.');
    let major_minor = match (parts.next(), parts.next()) {
        (Some(""), _) | (None, _) => return None,
        (Some(major), Some(minor)) => format!("{major}.{minor}"),
        (Some(major), None) => major.to_owned(),
    };
    // With no tested versions there is nothing to compare against.
    let up_to = tested.last()?;
    if tested.contains(&major_minor.as_str()) {
        return None;
    }
    Some(format!(
        "notice: {binary} {major_minor} untested (tested up to {up_to})"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::StubAdapter;

    #[test]
    fn untested_notice_matches_ui_4_12() {
        assert_eq!(
            untested_notice("claude", "2.3.0", &["2.0", "2.1"]).as_deref(),
            Some("notice: claude 2.3 untested (tested up to 2.1)")
        );
        assert_eq!(untested_notice("claude", "2.1.288", &["2.1"]), None);
    }

    #[test]
    fn coverage_complete_needs_all_five_events_and_reply() {
        let all = HookCoverage {
            ready: true,
            busy: true,
            needs_input: true,
            turn_end: true,
            session_end: true,
            last_reply: true,
        };
        assert!(all.complete());
        assert!(
            !HookCoverage {
                session_end: false,
                ..all
            }
            .complete()
        );
        assert!(!HookCoverage { busy: false, ..all }.complete());
        assert!(
            !HookCoverage {
                last_reply: false,
                ..all
            }
            .complete()
        );
    }

    #[test]
    fn stub_adapter_detect_reports_its_presence() {
        let a = StubAdapter {
            presence: CliPresence::Missing,
            ..StubAdapter::new(CliKind::Opencode)
        };
        assert_eq!(
            Adapters::new(vec![Box::new(a)]).detect_all(),
            vec![(CliKind::Opencode, CliPresence::Missing)]
        );
    }

    #[test]
    fn detect_all_follows_cli_kind_order() {
        let list: Vec<Box<dyn CliAdapter>> = vec![
            Box::new(StubAdapter::new(CliKind::Opencode)),
            Box::new(StubAdapter::new(CliKind::Claude)),
        ];
        let kinds: Vec<CliKind> = Adapters::new(list)
            .detect_all()
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        assert_eq!(kinds, CliKind::ALL.to_vec());
    }

    #[test]
    fn stub_parses_hooks_and_screens() {
        let a = StubAdapter::new(CliKind::Claude);
        let p = serde_json::json!({"last_reply": "done"});
        assert_eq!(
            a.parse_hook("turn-end", &p),
            Some(HookSignal::TurnEnded {
                last_reply: Some("done".into())
            })
        );
        assert_eq!(a.parse_hook("nope", &p), None);
        assert_eq!(
            a.read_screen("x\nASK  \n\n").state,
            Some(ScreenState::NeedsInput)
        );
        assert_eq!(a.read_screen("hello").state, None);
    }

    #[test]
    fn untested_notice_edge_cases() {
        assert_eq!(untested_notice("claude", "", &["2.1"]), None);
        assert_eq!(untested_notice("claude", "  ", &["2.1"]), None);
        assert_eq!(untested_notice("claude", "2.3.0", &[]), None);
    }

    #[test]
    fn adapter_types_round_trip_json() {
        let p = CliPresence::Found {
            version: "2.1.0".into(),
        };
        let back: CliPresence = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back, p);
        let c = StubAdapter::new(CliKind::Claude).coverage;
        let back: HookCoverage = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back, c);
    }

    fn spec(dir: Option<&str>, task: Option<&str>) -> LaunchSpec {
        let org = crate::testkit::example_org();
        let mut agent = crate::inherit::resolve(&org, &crate::testkit::n("squad-1")).unwrap();
        agent.directory = dir.map(str::to_owned);
        LaunchSpec {
            agent,
            task: task.map(str::to_owned),
            run_dir: "/run/a".into(),
            hook_exe: "/bin/duohive".into(),
            home: "/h".into(),
            cwd: None,
        }
    }

    #[test]
    fn stub_records_launch_specs_and_uses_agent_directory() {
        let a = StubAdapter::new(CliKind::Claude);
        let seen = a.launches.clone();
        let boxed: Box<dyn CliAdapter> = Box::new(a);
        let cmd = boxed.launch(&spec(Some("~/code"), Some("t"))).unwrap();
        assert_eq!(cmd.cwd, std::path::PathBuf::from("/h/code"));
        assert_eq!(cmd.args, vec!["--task".to_owned(), "t".to_owned()]);
        let cmd = boxed.launch(&spec(None, None)).unwrap();
        assert_eq!(cmd.cwd, std::path::PathBuf::from("/h"));
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[test]
    fn stub_enrich_hook_runs_the_configured_behaviour() {
        fn add_reply(_: &str, p: &mut serde_json::Value) {
            p["duohive_last_reply"] = serde_json::json!("hi");
        }
        let plain = StubAdapter::new(CliKind::Claude);
        let mut p = serde_json::json!({});
        plain.enrich_hook("turn-end", &mut p);
        assert_eq!(p, serde_json::json!({}));
        StubAdapter::new(CliKind::Claude)
            .with_enrich(add_reply)
            .enrich_hook("turn-end", &mut p);
        assert_eq!(p["duohive_last_reply"], "hi");
    }
}
