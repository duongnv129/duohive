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

/// The agent's directory with `~` expanded, or the home directory.
pub(crate) fn launch_cwd(spec: &LaunchSpec) -> PathBuf {
    spec.agent
        .directory
        .as_deref()
        .map_or_else(|| spec.home.clone(), |d| expand_home(d, &spec.home))
}

#[cfg(test)]
mod tests {
    use duohive_core::cli_kind::CliKind;

    use super::*;

    #[test]
    fn real_adapters_include_opencode() {
        let adapters = real_adapters();
        assert_eq!(
            adapters.get(CliKind::Opencode).map(|a| a.kind()),
            Some(CliKind::Opencode)
        );
    }

    #[test]
    fn real_adapters_include_claude() {
        let adapters = real_adapters();
        assert_eq!(
            adapters.get(CliKind::Claude).map(|a| a.kind()),
            Some(CliKind::Claude)
        );
    }
}
