//! The tmux server: configuration, version check, idempotent set-up, one-shot commands.

use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::version::{TmuxVersion, parse_version};

/// Where and how to reach DuoHive's tmux server.
#[derive(Debug, Clone)]
pub struct TmuxConfig {
    /// The tmux executable.
    pub tmux_bin: PathBuf,
    /// The `-L` socket name (D31).
    pub socket: String,
    /// The session, `duohive`.
    pub session: String,
    /// The paste buffer, `duohive-<pid>`.
    pub buffer_name: String,
    /// `<data_dir>/run`: `start` creates `run_root/<agent>/` and its `launch.sh`.
    pub run_root: PathBuf,
}

/// A tmux failure.
#[derive(Debug, thiserror::Error)]
pub enum TmuxError {
    /// No tmux executable.
    #[error("tmux not found")]
    NotFound,
    /// tmux older than 3.2.
    #[error("tmux 3.2 or newer is required (found {found})")]
    Unsupported {
        /// What `tmux -V` printed.
        found: String,
    },
    /// `tmux -V` printed something unparsable: callers warn and proceed.
    #[error("unknown tmux version {output:?}")]
    UnknownVersion {
        /// What `tmux -V` printed.
        output: String,
    },
    /// Setting up the server or session failed.
    #[error("tmux set-up failed: {0}")]
    Setup(String),
    /// The control client could not attach.
    #[error("tmux control client failed: {0}")]
    Attach(String),
    /// An I/O error talking to tmux.
    #[error("tmux i/o: {0}")]
    Io(#[from] std::io::Error),
}

/// Runs `tmux -V`. `Ok(None)` when tmux isn't found.
///
/// # Errors
/// [`TmuxError::UnknownVersion`] when the output doesn't parse (callers warn and
/// proceed), [`TmuxError::Io`] when tmux can't be run for another reason.
pub fn tmux_version(cfg: &TmuxConfig) -> Result<Option<TmuxVersion>, TmuxError> {
    let out = match std::process::Command::new(&cfg.tmux_bin)
        .arg("-V")
        .stdin(Stdio::null())
        .output()
    {
        Ok(out) => out,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    match parse_version(&text) {
        Some(v) => Ok(Some(v)),
        None => Err(TmuxError::UnknownVersion { output: text }),
    }
}

/// Fails unless tmux is found and at least 3.2; an unknown version is logged and accepted.
pub(crate) async fn check_version(cfg: &TmuxConfig) -> Result<(), TmuxError> {
    let probe = cfg.clone();
    let found = tokio::task::spawn_blocking(move || tmux_version(&probe))
        .await
        .map_err(|e| TmuxError::Setup(e.to_string()))?;
    match found {
        Ok(None) => Err(TmuxError::NotFound),
        Ok(Some(v)) if !v.supported() => Err(TmuxError::Unsupported { found: v.raw }),
        Ok(Some(_)) => Ok(()),
        Err(TmuxError::UnknownVersion { output }) => {
            tracing::warn!(output, "unknown tmux version; proceeding");
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// `tmux -L <socket>`, without an inherited `$TMUX`.
pub(crate) fn tmux(cfg: &TmuxConfig) -> Command {
    let mut c = Command::new(&cfg.tmux_bin);
    c.arg("-L")
        .arg(&cfg.socket)
        .env_remove("TMUX")
        .stdin(Stdio::null());
    c
}

/// `=<session>`: an exact-match target.
pub(crate) fn exact(cfg: &TmuxConfig) -> String {
    format!("={}", cfg.session)
}

/// Runs a one-shot tmux command; its stderr on failure.
async fn run(cfg: &TmuxConfig, args: &[&str]) -> Result<(), String> {
    let out = tmux(cfg)
        .args(args)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// True when the server runs and has the session.
pub(crate) async fn has_session(cfg: &TmuxConfig) -> bool {
    run(cfg, &["has-session", "-t", &exact(cfg)]).await.is_ok()
}

/// Starts the server and session if needed (the keeper window keeps the session
/// alive with no agent), then sets the global options. Idempotent.
pub(crate) async fn ensure_server(cfg: &TmuxConfig) -> Result<(), TmuxError> {
    if !has_session(cfg).await {
        let created = run(
            cfg,
            &[
                "new-session",
                "-d",
                "-s",
                &cfg.session,
                "-n",
                "duohive-home",
                "-x",
                "160",
                "-y",
                "48",
                "tail -f /dev/null",
            ],
        )
        .await;
        // Another instance may have created it meanwhile.
        if let Err(e) = created
            && !has_session(cfg).await
        {
            return Err(TmuxError::Setup(e));
        }
    }
    run(cfg, &["set-option", "-g", "remain-on-exit", "on"])
        .await
        .map_err(TmuxError::Setup)?;
    run(cfg, &["set-option", "-g", "default-size", "160x48"])
        .await
        .map_err(TmuxError::Setup)
}

/// `load-buffer -b <buffer> -` with `text` on stdin: no length or newline limits.
pub(crate) async fn load_buffer(cfg: &TmuxConfig, text: &str) -> Result<(), String> {
    let mut child = tmux(cfg)
        .args(["load-buffer", "-b", &cfg.buffer_name, "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().await.map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}
