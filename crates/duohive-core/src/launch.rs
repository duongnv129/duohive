//! What it takes to start an agent's CLI: tokens, windows, keys, launch commands.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::inherit::ResolvedAgent;

/// A per-launch secret (D1) that hooks present.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Token(pub String);

impl std::fmt::Debug for Token {
    /// Never prints the secret.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

/// A tmux window id such as `@12`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowId(pub String);

/// A key sent to a pane.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Key {
    /// Enter.
    Enter,
    /// Escape.
    Escape,
    /// Ctrl+C.
    CtrlC,
    /// Ctrl+D.
    CtrlD,
    /// Literal text.
    Text(String),
}

/// Everything an adapter needs to build a launch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchSpec {
    /// The agent, fully resolved.
    pub agent: ResolvedAgent,
    /// The task to start with, if any.
    pub task: Option<String>,
    /// `<data_dir>/run/<agent>/`.
    pub run_dir: PathBuf,
    /// The `duohive` executable that hooks call.
    pub hook_exe: PathBuf,
    /// The user's home directory.
    pub home: PathBuf,
}

/// A file to write before launching; always under the run dir, mode 0600.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchFile {
    /// Where to write it.
    pub path: PathBuf,
    /// What to write.
    pub contents: String,
}

/// The process to run in the agent's window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchCommand {
    /// The executable.
    pub program: String,
    /// Its arguments, passed verbatim.
    pub args: Vec<String>,
    /// Extra environment variables.
    pub env: Vec<(String, String)>,
    /// Working directory.
    pub cwd: PathBuf,
    /// Files to write first.
    pub files: Vec<LaunchFile>,
}

/// Quotes `arg` for a POSIX shell: single quotes, with `'` written `'\''`.
pub fn sh_quote(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn token_debug_is_redacted() {
        let t = super::Token("s3cr3t-value".to_string());
        let shown = format!("{t:?}");
        assert!(!shown.contains("s3cr3t"), "{shown}");
        assert_eq!(shown, "Token(<redacted>)");
    }

    use super::*;

    #[test]
    fn sh_quote_handles_hostile_text() {
        for text in ["$(rm -rf ~)", "`x`", "-x", "a\nb", "é", "it's", "", "''"] {
            let script = format!("printf %s {}", sh_quote(text));
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(&script)
                .output()
                .unwrap();
            assert_eq!(String::from_utf8(out.stdout).unwrap(), text, "{script}");
        }
    }

    #[test]
    fn sh_quote_escapes_single_quotes() {
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
    }
}
