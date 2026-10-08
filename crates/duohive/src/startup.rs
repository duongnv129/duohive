//! The app's start-up and exit (TD §4.1 Start-up, §8):
//! paths → logging → tmux check (missing or older than 3.2: the blocking screen on a
//! terminal, then exit 1) → quit signals → the system → the TUI → an orderly
//! shutdown, bounded by [`SHUTDOWN_LIMIT`]. Agents keep running in tmux.
//!
//! How the app ends:
//! - `q`, `ctrl+c`, SIGINT, SIGTERM or SIGHUP: quit, exit 0, with one stderr line
//!   when agents are still running (how to list them). A signal during start-up
//!   lets the start finish, then shuts down what started. A second signal during
//!   shutdown exits at once.
//! - Panics (ruling): any panic, on any thread or task, quits the app with exit 101
//!   through the same orderly shutdown. Ratatui's hook has restored the terminal by
//!   then, so the TUI draws nothing more (`AppConfig::halted`). A panicking tmux
//!   runtime task is also reported to the engine as `ConnectionLost`; that matters
//!   headless (the bench), where nothing else would notice.

use std::any::Any;
use std::collections::HashMap;
use std::ffi::OsString;
use std::future::Future;
use std::io::{IsTerminal, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::Poll;
use std::time::Duration;

use duohive_cli::real_adapters;
use duohive_store::Paths;
use duohive_tmux::{TmuxError, TmuxPaneView, TmuxVersion, tmux_version};
use duohive_tui::{
    AppConfig, BlockingScreen, QuitSignals, TuiError, restore_terminal, run_app, show_blocking,
};
use tokio::sync::{Notify, oneshot};

use crate::local_time::utc_offset_secs;
use crate::logging::init_logging;
use crate::real_ports::{FsDirProbe, LatePaneView};
use crate::system::{StartError, SystemOptions, start_system, tmux_config};

/// The exit status after a panic (Rust's own).
const PANIC_EXIT: u8 = 101;
/// The longest an orderly shutdown may take before DuoHive just exits.
const SHUTDOWN_LIMIT: Duration = Duration::from_secs(10);

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

/// The line said after a quit while `n` agents keep running in tmux, if any.
fn still_running(n: usize, socket: &str) -> Option<String> {
    let agents = if n == 1 { "agent" } else { "agents" };
    (n > 0).then(|| format!("{n} {agents} still running · tmux -L {socket} ls"))
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
    // From here a quit signal shuts down in order instead of killing the process.
    let mut signals = QuitSignals::install();
    let (first_frame_tx, first_frame) = oneshot::channel();
    let opts = SystemOptions {
        tap: None,
        first_frame: Some(first_frame),
        home: home.clone(),
    };
    let start = start_system(&paths, Arc::new(real_adapters()), opts);
    let (started, quit_early) = finish_start(start, signals.recv()).await;
    let mut handle = match started {
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
    let running = Arc::new(AtomicUsize::new(0));
    let ended = if quit_early {
        tracing::info!("quit during start-up");
        Ended::Quit
    } else {
        let cfg = AppConfig {
            pane_view: attach_pane_view(&paths),
            org_toml: paths.org_toml(),
            settings: handle.settings.clone(),
            utc_offset_secs: utc_offset_secs(var(&env, "TZ")),
            no_color: var(&env, "NO_COLOR").is_some(),
            editor: env.get("EDITOR").cloned(),
            dirs: Arc::new(FsDirProbe),
            home,
            halted: Arc::new(AtomicBool::new(false)),
            running: Arc::clone(&running),
        };
        let panicked = watch_panics();
        let commands = handle.commands.clone();
        let app = run_app(
            cfg,
            &mut handle.events,
            commands,
            Some(first_frame_tx),
            &mut signals,
        );
        until_quit(app, &panicked).await
    };
    if ended == Ended::Panicked {
        // The app may have been dropped mid-frame; the panic hook restored the
        // terminal already, so this is only belt and braces.
        restore_terminal();
    }
    match bounded(handle.shutdown(), SHUTDOWN_LIMIT, signals.recv()).await {
        Bounded::Done => tracing::info!(?ended, "stopped; agents keep running"),
        Bounded::TimedOut => {
            tracing::error!(?ended, "shutdown took over {SHUTDOWN_LIMIT:?}; exiting");
            say("duohive: shutdown took too long; exiting (your agents keep running)");
            return ExitCode::FAILURE;
        }
        Bounded::Quit => {
            tracing::warn!(?ended, "quit again during shutdown; exiting at once");
            return ExitCode::FAILURE;
        }
    }
    match ended {
        Ended::Quit => {
            if let Some(line) = still_running(running.load(Ordering::Relaxed), &paths.tmux_socket) {
                say(&line);
            }
            ExitCode::SUCCESS
        }
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

/// The live pane view: a [`LatePaneView`] that the tmux view client joins once it has
/// attached. Attaching blocks for up to a few seconds and needs the session, which
/// the started system has made, so it runs off the async loop and the first frame
/// never waits for it. A failed attach is logged; the agent pane then keeps its
/// summaries.
fn attach_pane_view(paths: &Paths) -> Arc<LatePaneView> {
    let view = Arc::new(LatePaneView::default());
    let late = Arc::clone(&view);
    let cfg = tmux_config(paths);
    // Detached: the result lands in `late`, and the runtime's exit doesn't wait on it.
    drop(tokio::task::spawn_blocking(
        move || match TmuxPaneView::new(&cfg) {
            Ok(v) => {
                tracing::info!("live pane view attached");
                late.set(Arc::new(v));
            }
            Err(e) => tracing::error!(error = %e, "live pane view unavailable"),
        },
    ));
    view
}

/// Runs `f` on `rt`, then shuts the runtime down waiting at most `grace` for blocking
/// work. A plain drop would wait without limit for every `spawn_blocking` job, such as
/// a core thread join left behind by a shutdown that hit [`SHUTDOWN_LIMIT`], and so
/// undo the bound. Anything `f` owned (the log guard included) is dropped, and so
/// flushed, before this returns.
pub fn block_on_then_exit<T>(
    rt: tokio::runtime::Runtime,
    f: impl Future<Output = T>,
    grace: Duration,
) -> T {
    let out = rt.block_on(f);
    rt.shutdown_timeout(grace);
    out
}

/// How long the runtime may wait for blocking work once the app has returned.
pub const EXIT_GRACE: Duration = Duration::from_millis(500);

/// `start`'s result, and whether `quit` fired meanwhile: a quit never abandons a
/// half-started system (the listener may already hold hooks), it waits for it.
pub(crate) async fn finish_start<T>(
    start: impl Future<Output = T>,
    quit: impl Future<Output = ()>,
) -> (T, bool) {
    let mut start = std::pin::pin!(start);
    tokio::select! {
        biased;
        started = &mut start => (started, false),
        () = quit => (start.await, true),
    }
}

/// How a bounded shutdown went.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Bounded {
    Done,
    /// It ran past the limit.
    TimedOut,
    /// Another quit signal: the user wants out now.
    Quit,
}

/// Runs `f` for at most `limit`, or until `quit`.
pub(crate) async fn bounded(
    f: impl Future<Output = ()>,
    limit: Duration,
    quit: impl Future<Output = ()>,
) -> Bounded {
    tokio::select! {
        biased;
        () = f => Bounded::Done,
        () = quit => Bounded::Quit,
        () = tokio::time::sleep(limit) => Bounded::TimedOut,
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
            // The blocking screen needs a terminal; without one the message says it.
            if std::io::stdout().is_terminal() {
                let screen = BlockingScreen::TmuxMissing { found };
                let shown = tokio::task::spawn_blocking(move || show_blocking(&screen)).await;
                if let Ok(Err(e)) = &shown {
                    say(&format!("duohive: {e}"));
                }
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
    /// `q`, `ctrl+c`, SIGINT, SIGTERM or SIGHUP.
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
            Ok(Err(TuiError::Halted)) => Ended::Panicked,
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
    fn still_running_line_names_the_count_and_socket() {
        assert_eq!(still_running(0, "duohive"), None);
        assert_eq!(
            still_running(1, "duohive").as_deref(),
            Some("1 agent still running · tmux -L duohive ls")
        );
        assert_eq!(
            still_running(3, "duohive-test-x").as_deref(),
            Some("3 agents still running · tmux -L duohive-test-x ls")
        );
    }

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

    #[tokio::test]
    async fn a_quit_during_start_up_waits_for_the_start_to_finish() {
        let start = async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            "handle"
        };
        assert_eq!(finish_start(start, async {}).await, ("handle", true));
        let quick = async { "handle" };
        let never = std::future::pending::<()>();
        assert_eq!(finish_start(quick, never).await, ("handle", false));
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_is_bounded_and_a_second_quit_cuts_it_short() {
        let never = || std::future::pending::<()>();
        let limit = Duration::from_secs(10);
        assert_eq!(bounded(async {}, limit, never()).await, Bounded::Done);
        assert_eq!(bounded(never(), limit, never()).await, Bounded::TimedOut);
        assert_eq!(bounded(never(), limit, async {}).await, Bounded::Quit);
    }

    #[test]
    fn stalled_blocking_work_does_not_hold_the_exit() {
        // A core thread join that never ends, as a timed-out shutdown leaves behind.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let started = std::time::Instant::now();
        let out = block_on_then_exit(
            rt,
            async {
                drop(tokio::task::spawn_blocking(|| {
                    std::thread::sleep(Duration::from_secs(5));
                }));
                7
            },
            Duration::from_millis(100),
        );
        assert_eq!(out, 7);
        let took = started.elapsed();
        assert!(took < Duration::from_secs(2), "exit waited {took:?}");
    }
}
