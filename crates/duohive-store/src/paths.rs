//! Where DuoHive keeps its files (D18, D31).

use std::collections::HashMap;
use std::path::PathBuf;

use duohive_core::names::AgentName;

const APP_DIR: &str = "duohive";
const DEFAULT_TMUX_SOCKET: &str = "duohive";

/// Paths could not be worked out.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PathsError {
    /// No `HOME` and no override that would make it unnecessary.
    #[error("cannot find your home directory: HOME is not set")]
    NoHome,
}

/// Where DuoHive's files live, and which tmux socket it uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    /// Holds `org.toml` and `settings.toml`.
    pub config_dir: PathBuf,
    /// Holds `state.db`, `run.sock`, `run/<agent>/` and `logs/`.
    pub data_dir: PathBuf,
    /// The tmux server socket name (`tmux -L <name>`).
    pub tmux_socket: String,
}

/// A variable's value, with empty counting as unset.
fn var<'a>(vars: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    vars.get(key).map(String::as_str).filter(|v| !v.is_empty())
}

fn resolve(
    vars: &HashMap<String, String>,
    override_key: &str,
    xdg_key: &str,
    home_rel: &[&str],
) -> Result<PathBuf, PathsError> {
    if let Some(dir) = var(vars, override_key) {
        return Ok(PathBuf::from(dir));
    }
    if let Some(base) = var(vars, xdg_key) {
        return Ok(PathBuf::from(base).join(APP_DIR));
    }
    let home = var(vars, "HOME").ok_or(PathsError::NoHome)?;
    let mut path = PathBuf::from(home);
    path.extend(home_rel);
    Ok(path.join(APP_DIR))
}

impl Paths {
    /// Works the paths out from environment-style variables (no process state read).
    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Paths, PathsError> {
        Ok(Paths {
            config_dir: resolve(vars, "DUOHIVE_CONFIG_DIR", "XDG_CONFIG_HOME", &[".config"])?,
            data_dir: resolve(
                vars,
                "DUOHIVE_DATA_DIR",
                "XDG_DATA_HOME",
                &[".local", "share"],
            )?,
            tmux_socket: var(vars, "DUOHIVE_TMUX_SOCKET")
                .unwrap_or(DEFAULT_TMUX_SOCKET)
                .to_owned(),
        })
    }

    /// The org file.
    pub fn org_toml(&self) -> PathBuf {
        self.config_dir.join("org.toml")
    }

    /// The previous version of the org file.
    pub fn org_backup(&self) -> PathBuf {
        self.config_dir.join("org.toml.bak")
    }

    /// The last org the engine accepted.
    pub fn org_last_good(&self) -> PathBuf {
        self.data_dir.join("org.last-good.toml")
    }

    /// The personal settings file.
    pub fn settings_toml(&self) -> PathBuf {
        self.config_dir.join("settings.toml")
    }

    /// The live-state database.
    pub fn state_db(&self) -> PathBuf {
        self.data_dir.join("state.db")
    }

    /// The hook socket.
    pub fn run_sock(&self) -> PathBuf {
        self.data_dir.join("run.sock")
    }

    /// One agent's per-launch files.
    pub fn run_dir(&self, agent: &AgentName) -> PathBuf {
        self.data_dir.join("run").join(agent.as_str())
    }

    /// Task worktrees, one directory per agent.
    pub fn worktrees_dir(&self) -> PathBuf {
        self.data_dir.join("worktrees")
    }

    /// Log files.
    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use duohive_core::testkit::n;

    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn paths_follow_xdg_then_home() {
        let p = Paths::from_vars(&vars(&[
            ("HOME", "/h"),
            ("XDG_CONFIG_HOME", "/x"),
            ("XDG_DATA_HOME", "/d"),
        ]))
        .unwrap();
        assert_eq!(p.config_dir, PathBuf::from("/x/duohive"));
        assert_eq!(p.data_dir, PathBuf::from("/d/duohive"));
        let p = Paths::from_vars(&vars(&[("HOME", "/h")])).unwrap();
        assert_eq!(p.config_dir, PathBuf::from("/h/.config/duohive"));
        assert_eq!(p.data_dir, PathBuf::from("/h/.local/share/duohive"));
    }

    #[test]
    fn duohive_dirs_win_and_need_no_home() {
        let p = Paths::from_vars(&vars(&[
            ("XDG_CONFIG_HOME", "/x"),
            ("DUOHIVE_CONFIG_DIR", "/c"),
            ("DUOHIVE_DATA_DIR", "/dd"),
        ]))
        .unwrap();
        assert_eq!(p.config_dir, PathBuf::from("/c"));
        assert_eq!(p.data_dir, PathBuf::from("/dd"));
    }

    #[test]
    fn empty_values_count_as_unset() {
        let p = Paths::from_vars(&vars(&[("HOME", "/h"), ("XDG_CONFIG_HOME", "")])).unwrap();
        assert_eq!(p.config_dir, PathBuf::from("/h/.config/duohive"));
    }

    #[test]
    fn no_home_is_an_error() {
        assert_eq!(Paths::from_vars(&vars(&[])), Err(PathsError::NoHome));
    }

    #[test]
    fn tmux_socket_default_and_override() {
        let base = [("HOME", "/h")];
        assert_eq!(
            Paths::from_vars(&vars(&base)).unwrap().tmux_socket,
            "duohive"
        );
        let p = Paths::from_vars(&vars(&[
            ("HOME", "/h"),
            ("DUOHIVE_TMUX_SOCKET", "duohive-test-1"),
        ]))
        .unwrap();
        assert_eq!(p.tmux_socket, "duohive-test-1");
    }

    #[test]
    fn file_locations() {
        let p = Paths {
            config_dir: "/c".into(),
            data_dir: "/d".into(),
            tmux_socket: "duohive".into(),
        };
        assert_eq!(p.org_toml(), PathBuf::from("/c/org.toml"));
        assert_eq!(p.org_backup(), PathBuf::from("/c/org.toml.bak"));
        assert_eq!(p.settings_toml(), PathBuf::from("/c/settings.toml"));
        assert_eq!(p.org_last_good(), PathBuf::from("/d/org.last-good.toml"));
        assert_eq!(p.state_db(), PathBuf::from("/d/state.db"));
        assert_eq!(p.run_sock(), PathBuf::from("/d/run.sock"));
        assert_eq!(
            p.run_dir(&n("auth-service")),
            PathBuf::from("/d/run/auth-service")
        );
        assert_eq!(p.logs_dir(), PathBuf::from("/d/logs"));
        assert_eq!(p.worktrees_dir(), PathBuf::from("/d/worktrees"));
    }
}
