//! The CLI adapters: Claude Code (and OpenCode, Task 21).

pub mod claude;
pub mod detect;
pub mod hooks_json;

use duohive_core::adapter::Adapters;

pub use claude::ClaudeAdapter;
pub use detect::{parse_cli_version, probe_version};
pub use hooks_json::hook_command;

/// The adapters DuoHive runs agents with.
pub fn real_adapters() -> Adapters {
    Adapters::new(vec![Box::new(ClaudeAdapter)])
}

#[cfg(test)]
mod tests {
    use duohive_core::cli_kind::CliKind;

    use super::*;

    #[test]
    fn real_adapters_include_claude() {
        let adapters = real_adapters();
        assert_eq!(
            adapters.get(CliKind::Claude).map(|a| a.kind()),
            Some(CliKind::Claude)
        );
    }
}
