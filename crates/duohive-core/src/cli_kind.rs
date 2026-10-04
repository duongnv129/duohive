//! The agent CLIs DuoHive can drive.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A supported agent CLI. TOML values: `claude`, `opencode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CliKind {
    /// Claude Code.
    Claude,
    /// OpenCode.
    Opencode,
}

impl CliKind {
    /// Every supported CLI.
    pub const ALL: [CliKind; 2] = [CliKind::Claude, CliKind::Opencode];

    /// The executable name.
    pub fn binary(self) -> &'static str {
        match self {
            CliKind::Claude => "claude",
            CliKind::Opencode => "opencode",
        }
    }

    /// The name shown to the user.
    pub fn display(self) -> &'static str {
        match self {
            CliKind::Claude => "Claude Code",
            CliKind::Opencode => "OpenCode",
        }
    }
}

/// A CLI name that is not supported.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown CLI \"{0}\"")]
pub struct UnknownCli(pub String);

impl FromStr for CliKind {
    type Err = UnknownCli;

    fn from_str(s: &str) -> Result<Self, UnknownCli> {
        Self::ALL
            .into_iter()
            .find(|k| k.binary() == s)
            .ok_or_else(|| UnknownCli(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_cli_message() {
        assert_eq!(
            "claud".parse::<CliKind>().unwrap_err().to_string(),
            "unknown CLI \"claud\""
        );
    }

    #[test]
    fn cli_display_and_binary() {
        assert_eq!(CliKind::Opencode.display(), "OpenCode");
        assert_eq!(CliKind::Claude.binary(), "claude");
    }

    #[test]
    fn parses_toml_values() {
        assert_eq!("claude".parse(), Ok(CliKind::Claude));
        assert_eq!("opencode".parse(), Ok(CliKind::Opencode));
    }
}
