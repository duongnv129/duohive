//! The app's start-up and exit (TD §4.1 Start-up, §8):
//! paths → logging → tmux check (missing or older than 3.2: the blocking screen,
//! then exit 1) → the system → the TUI until `q`, SIGINT, SIGTERM or a panic →
//! an orderly shutdown. Agents keep running in tmux.

use std::any::Any;
use std::collections::HashMap;
use std::ffi::OsString;
use std::future::Future;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::task::Poll;

use duohive_cli::real_adapters;
use duohive_store::Paths;
use duohive_tmux::{TmuxError, TmuxVersion, tmux_version};
use duohive_tui::{AppConfig, BlockingScreen, TuiError, restore_terminal, run_app, show_blocking};
use tokio::sync::{Notify, oneshot};

use crate::logging::init_logging;
use crate::real_ports::FsDirProbe;
use crate::system::{StartError, SystemOptions, start_system, tmux_config};

/// The exit status after a panic (Rust's own).
const PANIC_EXIT: u8 = 101;

/// The process environment, dropping entries that aren't UTF-8.
pub fn env_vars() -> HashMap<String, String> {
    utf8_vars(std::env::vars_os())
}

fn utf8_vars(vars: impl Iterator<Item = (OsString, OsString)>) -> HashMap<String, String> {
    vars.filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect()
}

/// A variable's value, with empty counting as unset.
fn var<'a>(env: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    env.get(key).map(String::as_str).filter(|v| !v.is_empty())
}

/// Says `msg` on stderr (the terminal is ours again by then).
fn say(msg: &str) {
    let _ = writeln!(std::io::stderr(), "{msg}");
}

/// Runs the app; see the module docs.
pub async fn run_app_main() -> ExitCode {
    let env = env_vars();
    let paths = match Paths::from_vars(&env) {
        Ok(p) => p,
        Err(e) => {
            say(&format!("duohive: {e}"));
            return ExitCode::FAILURE;
        }
    };
    let _log = match init_logging(&paths) {
        Ok(guard) => guard,
        Err(e) => {
            say(&format!("duohive: {e}"));
            return ExitCode::FAILURE;
        }
    };
    let Some(home) = var(&env, "HOME").map(PathBuf::from) else {
        say(&format!("duohive: {}", StartError::NoHome));
        return ExitCode::FAILURE;
    };
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting");
    if let Some(code) = check_tmux(&paths).await {
        return code;
    }
    let (first_frame_tx, first_frame) = oneshot::channel();
    let opts = SystemOptions {
        tap: None,
        first_frame: Some(first_frame),
        home: home.clone(),
    };
    let mut handle = match start_system(&paths, Arc::new(real_adapters()), opts).await {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(error = %e, "start-up failed");
            say(&match e {
                StartError::AlreadyRunning => e.to_string(),
                e => format!("duohive: {e}"),
            });
            return ExitCode::FAILURE;
        }
    };
    let cfg = AppConfig {
        org_toml: paths.org_toml(),
        tmux_socket: paths.tmux_socket.clone(),
        settings: handle.settings.clone(),
        no_color: var(&env, "NO_COLOR").is_some(),
        editor: env.get("EDITOR").cloned(),
        dirs: Arc::new(FsDirProbe),
        home,
    };
    let panicked = watch_panics();
    let commands = handle.commands.clone();
    let app = run_app(cfg, &mut handle.events, commands, Some(first_frame_tx));
    let ended = until_quit(app, &panicked).await;
    if ended == Ended::Panicked {
        // The app may have been dropped mid-frame; the panic hook restored the
        // terminal already, so this is only belt and braces.
        restore_terminal();
    }
    handle.shutdown().await;
    tracing::info!(?ended, "stopped; agents keep running");
    match ended {
        Ended::Quit => ExitCode::SUCCESS,
        Ended::Failed(e) => {
            say(&format!("duohive: {e}"));
            ExitCode::FAILURE
        }
        Ended::Panicked => {
            say("duohive: an internal error stopped DuoHive; your agents keep running");
            ExitCode::from(PANIC_EXIT)
        }
    }
}

/// `None` when tmux will do; else the exit status, after the blocking screen
/// (tmux missing or too old) or a message (tmux could not be run).
async fn check_tmux(paths: &Paths) -> Option<ExitCode> {
    let cfg = tmux_config(paths);
    let probe = tokio::task::spawn_blocking(move || tmux_version(&cfg))
        .await
        .unwrap_or_else(|e| Err(TmuxError::Setup(e.to_string())));
    match tmux_gate(probe) {
        TmuxGate::Ready => None,
        TmuxGate::Missing { found } => {
            tracing::error!(?found, "tmux missing or older than 3.2");
            let screen = BlockingScreen::TmuxMissing { found };
            let shown = tokio::task::spawn_blocking(move || show_blocking(&screen)).await;
            if let Ok(Err(e)) = &shown {
                say(&format!("duohive: {e}"));
            }
            say("duohive: tmux 3.2 or newer is required");
            Some(ExitCode::FAILURE)
        }
        TmuxGate::Failed(e) => {
            tracing::error!(error = %e, "cannot run tmux");
            say(&format!("duohive: cannot run tmux: {e}"));
            Some(ExitCode::FAILURE)
        }
    }
}

/// What `tmux -V` means for start-up.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TmuxGate {
    /// 3.2 or newer, or a version that doesn't parse (logged, accepted).
    Ready,
    /// Not installed, or too old: the blocking screen (`found` without `tmux `).
    Missing { found: Option<String> },
    /// tmux could not be run.
    Failed(String),
}

pub(crate) fn tmux_gate(probe: Result<Option<TmuxVersion>, TmuxError>) -> TmuxGate {
    match probe {
        Ok(None) => TmuxGate::Missing { found: None },
        Ok(Some(v)) if !v.supported() => TmuxGate::Missing {
            found: Some(v.raw.strip_prefix("tmux ").unwrap_or(&v.raw).to_owned()),
        },
        Ok(Some(_)) => TmuxGate::Ready,
        Err(TmuxError::UnknownVersion { output }) => {
            tracing::warn!(output, "unknown tmux version; proceeding");
            TmuxGate::Ready
        }
        Err(e) => TmuxGate::Failed(e.to_string()),
    }
}

/// How the app ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Ended {
    /// `q`, `ctrl+c`, SIGINT or SIGTERM.
    Quit,
    /// The terminal or the core failed.
    Failed(String),
    /// A panic, on any thread: ratatui's hook has restored the terminal, so the
    /// app must not draw again.
    Panicked,
}

/// Runs `app` until it ends or anything panics.
pub(crate) async fn until_quit(
    app: impl Future<Output = Result<(), TuiError>>,
    panicked: &Notify,
) -> Ended {
    tokio::select! {
        ran = catch_panic(app) => match ran {
            Ok(Ok(())) => Ended::Quit,
            Ok(Err(e)) => Ended::Failed(e.to_string()),
            Err(_) => Ended::Panicked,
        },
        () = panicked.notified() => Ended::Panicked,
    }
}

/// `f`'s output, or the payload of a panic while polling it.
async fn catch_panic<T>(f: impl Future<Output = T>) -> Result<T, Box<dyn Any + Send>> {
    let mut f = std::pin::pin!(f);
    std::future::poll_fn(
        move |cx| match catch_unwind(AssertUnwindSafe(|| f.as_mut().poll(cx))) {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(v)) => Poll::Ready(Ok(v)),
            Err(payload) => Poll::Ready(Err(payload)),
        },
    )
    .await
}

/// Wraps the panic hook so a panic on any thread or task is logged and ends the app
/// (`until_quit`); the default hook still prints it, after ratatui's hook (installed
/// later, around this one) has restored the terminal.
fn watch_panics() -> Arc<Notify> {
    let notify = Arc::new(Notify::new());
    let on_panic = Arc::clone(&notify);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic = %info, "panic: shutting down");
        on_panic.notify_one();
        previous(info);
    }));
    notify
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use duohive_tmux::{TmuxError, parse_version};
    use duohive_tui::TuiError;
    use tokio::sync::Notify;

    use super::*;

    #[test]
    fn tmux_gate_blocks_missing_and_old_tmux() {
        assert_eq!(tmux_gate(Ok(None)), TmuxGate::Missing { found: None });
        assert_eq!(
            tmux_gate(Ok(parse_version("tmux 3.1c"))),
            TmuxGate::Missing {
                found: Some("3.1c".to_owned())
            }
        );
        assert_eq!(tmux_gate(Ok(parse_version("tmux 3.2a"))), TmuxGate::Ready);
        let unknown = TmuxError::UnknownVersion {
            output: "tmux master".to_owned(),
        };
        assert_eq!(tmux_gate(Err(unknown)), TmuxGate::Ready);
        let hung = TmuxError::Io(std::io::Error::other("tmux -V hung"));
        assert!(matches!(tmux_gate(Err(hung)), TmuxGate::Failed(ref e) if e.contains("hung")),);
    }

    #[tokio::test]
    async fn the_app_quitting_or_failing_ends_the_run() {
        let none = Notify::new();
        assert_eq!(until_quit(async { Ok(()) }, &none).await, Ended::Quit);
        assert_eq!(
            until_quit(async { Err(TuiError::CoreStopped) }, &none).await,
            Ended::Failed("the core stopped".to_owned())
        );
    }

    #[tokio::test]
    async fn a_panic_anywhere_ends_the_run() {
        let panicked = Arc::new(Notify::new());
        panicked.notify_one();
        let app = std::future::pending::<Result<(), TuiError>>();
        assert_eq!(until_quit(app, &panicked).await, Ended::Panicked);
        // A panic inside the app itself is caught too (`resume_unwind`: no hook noise).
        let app = async { std::panic::resume_unwind(Box::new("boom")) };
        assert_eq!(until_quit(app, &Notify::new()).await, Ended::Panicked);
    }
}
