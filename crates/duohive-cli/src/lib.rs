//! The CLI adapters: Claude Code and OpenCode.

pub mod claude;
pub mod detect;
pub mod fake;
pub mod hooks_json;
pub mod opencode;

use std::path::{Path, PathBuf};

use duohive_core::adapter::{Adapters, LaunchError};
use duohive_core::inherit::expand_home;
use duohive_core::launch::LaunchSpec;

pub use claude::ClaudeAdapter;
pub use detect::{parse_cli_version, probe_version};
pub use hooks_json::hook_command;
pub use opencode::OpencodeAdapter;

/// The adapters DuoHive runs agents with.
pub fn real_adapters() -> Adapters {
    Adapters::new(vec![Box::new(ClaudeAdapter), Box::new(OpencodeAdapter)])
}

/// `path` as UTF-8, or `LaunchError::Invalid` naming `what` it is: a CLI
/// argument, config value or environment value must not be a lossy path.
pub(crate) fn utf8<'a>(path: &'a Path, what: &str) -> Result<&'a str, LaunchError> {
    path.to_str()
        .ok_or_else(|| LaunchError::Invalid(format!("{what} is not UTF-8: {}", path.display())))
}

/// Where the CLI runs: the spec's own `cwd` (a new worktree), else the agent's
/// directory with `~` expanded, else the home directory.
pub(crate) fn launch_cwd(spec: &LaunchSpec) -> PathBuf {
    if let Some(cwd) = &spec.cwd {
        return cwd.clone();
    }
    spec.agent
        .directory
        .as_deref()
        .map_or_else(|| spec.home.clone(), |d| expand_home(d, &spec.home))
}

#[cfg(test)]
mod tests {
    use duohive_core::cli_kind::CliKind;

    use super::*;

    fn spec(cwd: Option<&str>) -> LaunchSpec {
        let org = duohive_core::testkit::example_org();
        let mut agent =
            duohive_core::inherit::resolve(&org, &duohive_core::testkit::n("squad-1")).unwrap();
        agent.directory = Some("~/code/payments".into());
        LaunchSpec {
            agent,
            task: None,
            run_dir: "/run/squad-1".into(),
            hook_exe: "/bin/duohive".into(),
            home: "/home/u".into(),
            cwd: cwd.map(PathBuf::from),
        }
    }

    #[test]
    fn launch_cwd_prefers_spec_cwd() {
        assert_eq!(
            launch_cwd(&spec(None)),
            PathBuf::from("/home/u/code/payments")
        );
        assert_eq!(
            launch_cwd(&spec(Some("/data/worktrees/squad-1/fix"))),
            PathBuf::from("/data/worktrees/squad-1/fix")
        );
    }

    #[test]
    fn real_adapters_include_opencode() {
        let adapters = real_adapters();
        assert_eq!(
            adapters
                .get(CliKind::Opencode)
                .map(duohive_core::adapter::CliAdapter::kind),
            Some(CliKind::Opencode)
        );
    }

    #[test]
    fn real_adapters_include_claude() {
        let adapters = real_adapters();
        assert_eq!(
            adapters
                .get(CliKind::Claude)
                .map(duohive_core::adapter::CliAdapter::kind),
            Some(CliKind::Claude)
        );
    }
}
