//! The app loop (TD §3.3): applies events, maps keys, sends commands, and draws only when
//! something changed (capped at 30 fps), plus the spinner and once-a-minute age ticks.

mod terminal;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, ExitStatus};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::cli_kind::CliKind;
use duohive_core::command::Command;
use duohive_core::event::Event;
use duohive_core::org::Org;
use duohive_core::settings::Settings;
use duohive_core::status::AgentState;
use duohive_core::time::Millis;
use duohive_core::validate::DirProbe;
use ratatui::Terminal;
use ratatui::backend::Backend;
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

pub use terminal::{restore_terminal, run_app, show_blocking};

use crate::format::age_of;
use crate::input::{UiEffect, map_key, paste, update};
use crate::model::{SharedDirs, UiModel};
use crate::render::{RenderCtx, render};
use crate::theme::palette_for;

/// At most 30 frames a second.
const FRAME: Duration = Duration::from_micros(33_334);
/// The working spinner's 4 frames a second (UI §2.1).
const SPINNER_TICK: Duration = Duration::from_millis(250);
/// Ages are shown in minutes, so one redraw a minute keeps them current.
const AGE_TICK: Duration = Duration::from_secs(60);
/// The editor when `$EDITOR` is unset or blank.
const DEFAULT_EDITOR: &str = "vi";

/// What the app needs besides its channels.
#[derive(Clone)]
pub struct AppConfig {
    /// `org.toml`, opened by `o`.
    pub org_toml: PathBuf,
    /// The tmux socket in use, for the attach hint (D31).
    pub tmux_socket: String,
    /// The settings at start-up.
    pub settings: Settings,
    /// `NO_COLOR` is set.
    pub no_color: bool,
    /// The value of `$EDITOR`, read by the caller; `None` or blank runs `vi`.
    pub editor: Option<String>,
    /// Probes the directories forms show (`✓ git repo`).
    pub dirs: Arc<dyn DirProbe>,
    /// The home directory `~` expands to.
    pub home: PathBuf,
    /// Set by a panic hook once the terminal has been restored: no frame is drawn
    /// after it, and the app ends with [`TuiError::Halted`].
    pub halted: Arc<AtomicBool>,
}

impl fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppConfig")
            .field("org_toml", &self.org_toml)
            .field("tmux_socket", &self.tmux_socket)
            .field("settings", &self.settings)
            .field("no_color", &self.no_color)
            .field("editor", &self.editor)
            .field("home", &self.home)
            .field("halted", &self.halted)
            .finish_non_exhaustive()
    }
}

/// Terminal input, as the loop sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    /// A key press.
    Key(KeyEvent),
    /// Pasted text (bracketed paste), delivered whole.
    Paste(String),
    /// The terminal's new size.
    Resize(u16, u16),
}

/// Why the app stopped other than by `q`.
#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    /// The terminal could not be drawn on.
    #[error("terminal: {0}")]
    Terminal(String),
    /// The terminal could not be set up or restored.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// The core's channels closed.
    #[error("the core stopped")]
    CoreStopped,
    /// Terminal input stopped.
    #[error("terminal input closed")]
    InputClosed,
    /// A panic restored the terminal: nothing more is drawn.
    #[error("stopped after an internal error")]
    Halted,
}

/// Hands the terminal to a program (the editor) and takes it back; blocking.
pub trait TerminalHost: Send + Sync {
    /// Runs `cmd` with the terminal handed over, and restores the terminal afterwards.
    fn hand_over(&self, cmd: Process) -> io::Result<ExitStatus>;
}

/// For a backend that isn't a real terminal: there is nothing to hand over, so no editor runs.
struct PlainHost;

impl TerminalHost for PlainHost {
    fn hand_over(&self, _cmd: Process) -> io::Result<ExitStatus> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no terminal to hand over",
        ))
    }
}

/// `$EDITOR` trimmed, or `vi` when it is unset or blank.
fn editor_or_default(editor: Option<&str>) -> &str {
    editor
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .unwrap_or(DEFAULT_EDITOR)
}

/// `sh -c '<editor> "$1"' sh <path>`: `$EDITOR` may carry arguments (`code -w`), and the
/// path is passed as `$1`, never interpreted by the shell.
pub fn editor_command(editor: Option<&str>, path: &Path) -> Process {
    let editor = editor_or_default(editor);
    let mut cmd = Process::new("sh");
    cmd.arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(path);
    cmd
}

/// The app over any backend: all drawing, input handling and timing live here.
/// `first_frame` fires once, right after the first frame is drawn. Signals are left
/// alone (no terminal: [`run_app`] handles them).
pub async fn run_app_on<B: Backend>(
    terminal: &mut Terminal<B>,
    input: mpsc::Receiver<InputEvent>,
    cfg: AppConfig,
    events: &mut mpsc::UnboundedReceiver<Event>,
    commands: mpsc::Sender<Command>,
    first_frame: Option<oneshot::Sender<()>>,
) -> Result<(), TuiError> {
    let host: Arc<dyn TerminalHost> = Arc::new(PlainHost);
    let mut signals = QuitSignals::none();
    run_loop(
        terminal,
        input,
        cfg,
        events,
        commands,
        first_frame,
        host,
        &mut signals,
    )
    .await
}

/// SIGINT, SIGTERM and SIGHUP (the terminal window closed): each quits like `q`, and
/// agents keep running. Explicit because once tokio has a SIGINT handler (the editor
/// hand-over installs one), the default action no longer ends the process; and so a
/// closed terminal still gets an orderly shutdown. The bench shares it.
pub struct QuitSignals {
    int: Option<Signal>,
    term: Option<Signal>,
    hup: Option<Signal>,
    hung_up: bool,
}

impl fmt::Debug for QuitSignals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuitSignals")
            .field("int", &self.int.is_some())
            .field("term", &self.term.is_some())
            .field("hup", &self.hup.is_some())
            .field("hung_up", &self.hung_up)
            .finish()
    }
}

impl QuitSignals {
    /// Listens from now on; a handler that can't be installed is left out.
    pub fn install() -> Self {
        Self {
            int: signal(SignalKind::interrupt()).ok(),
            term: signal(SignalKind::terminate()).ok(),
            hup: signal(SignalKind::hangup()).ok(),
            hung_up: false,
        }
    }

    /// Listens for none.
    pub fn none() -> Self {
        Self {
            int: None,
            term: None,
            hup: None,
            hung_up: false,
        }
    }

    /// Resolves at the next SIGINT, SIGTERM or SIGHUP.
    pub async fn recv(&mut self) {
        let Self {
            int,
            term,
            hup,
            hung_up,
        } = self;
        tokio::select! {
            () = next(int) => {}
            () = next(term) => {}
            () = next(hup) => *hung_up = true,
        }
    }

    /// A SIGHUP has arrived: the terminal is gone, and writing to it (or to stderr)
    /// fails.
    pub fn hung_up(&self) -> bool {
        self.hung_up
    }

    /// A ctrl+c typed while the editor had the terminal was meant for the editor:
    /// forget it. A fresh listener sees only signals that arrive from now on.
    fn forget_interrupts(&mut self) {
        if self.int.is_some() {
            self.int = signal(SignalKind::interrupt()).ok();
        }
    }
}

/// The next delivery of `sig`; never, without a listener or once it is closed.
async fn next(sig: &mut Option<Signal>) {
    let delivered = match sig {
        Some(s) => s.recv().await.is_some(),
        None => false,
    };
    if !delivered {
        std::future::pending::<()>().await;
    }
}

/// Keys that leave a blocking screen: `q`, or `ctrl+c` (raw mode makes it a key).
pub fn quits_blocking(k: &KeyEvent) -> bool {
    match k.code {
        KeyCode::Char('q') => true,
        KeyCode::Char('c') => k.modifiers.contains(KeyModifiers::CONTROL),
        _ => false,
    }
}

/// When the next frame, spinner step and age refresh are due.
struct Clock {
    dirty: bool,
    /// The first `Snapshot` has arrived: nothing is drawn before it.
    ready: bool,
    spinner: usize,
    last_draw: Option<Instant>,
    spinner_next: Option<Instant>,
}

impl Clock {
    /// Updates the spinner's schedule and returns when the loop should next wake to draw.
    fn next_wake(&mut self, m: &UiModel) -> Option<Instant> {
        let now = Instant::now();
        let spinning = self.ready && !m.settings.reduce_motion && any_working(m);
        self.spinner_next = match (spinning, self.spinner_next) {
            (false, _) => None,
            (true, Some(at)) => Some(at),
            (true, None) => Some(now + SPINNER_TICK),
        };
        let frame = self.last_draw.map_or(now, |t| (t + FRAME).max(now));
        let draw = (self.dirty && self.ready).then_some(frame);
        let ages = (self.ready && ages_shown(m))
            .then(|| self.last_draw.map(|t| t + AGE_TICK))
            .flatten();
        [draw, self.spinner_next, ages].into_iter().flatten().min()
    }

    /// At a wake: steps the spinner when due and marks the frame dirty when a tick is due.
    fn on_wake(&mut self, m: &UiModel) -> bool {
        let now = Instant::now();
        if let Some(at) = self.spinner_next.filter(|at| *at <= now) {
            self.spinner = self.spinner.wrapping_add(1);
            self.spinner_next = Some((at + SPINNER_TICK).max(now));
            self.dirty = true;
        }
        if ages_shown(m) && self.last_draw.is_some_and(|t| t + AGE_TICK <= now) {
            self.dirty = true;
        }
        let frame_ok = self.last_draw.is_none_or(|t| t + FRAME <= now);
        self.dirty && self.ready && frame_ok
    }
}

/// A working (not quiet) agent: its row animates.
fn any_working(m: &UiModel) -> bool {
    m.agent_names().into_iter().any(|a| {
        let s = &m.view(a).status;
        s.state == AgentState::Working && !s.quiet
    })
}

/// Some age is on screen: a state cell or a Needs you row. Whether an agent shows an age
/// doesn't depend on the time it is measured to (only an unknown start hides it), so any
/// `now` does here.
fn ages_shown(m: &UiModel) -> bool {
    !m.needs_you.is_empty()
        || m.agent_names()
            .into_iter()
            .any(|a| age_of(&m.view(a).status, Millis(1)).is_some())
}

/// The wall clock, in the engine's units.
fn wall_now() -> Millis {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    Millis(u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
}

/// The loop, with the terminal hand-over for `o` and the quit signals injected.
#[expect(
    clippy::too_many_arguments,
    reason = "the loop's seams, each injected for tests"
)]
pub(crate) async fn run_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    mut input: mpsc::Receiver<InputEvent>,
    cfg: AppConfig,
    events: &mut mpsc::UnboundedReceiver<Event>,
    commands: mpsc::Sender<Command>,
    mut first_frame: Option<oneshot::Sender<()>>,
    host: Arc<dyn TerminalHost>,
    signals: &mut QuitSignals,
) -> Result<(), TuiError> {
    let mut m = UiModel::new(Org::blank(CliKind::Claude));
    m.settings = cfg.settings.clone();
    m.org_toml = cfg.org_toml.clone();
    m.dirs = SharedDirs(Arc::clone(&cfg.dirs));
    m.home = cfg.home.clone();
    let mut clock = Clock {
        dirty: true,
        ready: false,
        spinner: 0,
        last_draw: None,
        spinner_next: None,
    };
    loop {
        let wake = clock.next_wake(&m);
        tokio::select! {
            () = signals.recv() => return Ok(()),
            event = events.recv() => {
                let event = event.ok_or(TuiError::CoreStopped)?;
                let snapshot = matches!(event, Event::Snapshot(_));
                clock.dirty |= m.apply(event);
                clock.ready |= snapshot;
            }
            got = input.recv() => match got.ok_or(TuiError::InputClosed)? {
                InputEvent::Resize(..) => clock.dirty = true,
                InputEvent::Paste(text) => {
                    let before = m.clone();
                    paste(&mut m, &text);
                    clock.dirty |= m != before;
                }
                InputEvent::Key(k) => {
                    m.now = wall_now();
                    let before = m.clone();
                    let effects = match map_key(&m, k) {
                        Some(a) => update(&mut m, a),
                        None => {
                            m.footer_note = None;
                            Vec::new()
                        }
                    };
                    clock.dirty |= m != before;
                    for effect in effects {
                        match effect {
                            UiEffect::Quit => return Ok(()),
                            UiEffect::Send(c) => send(&commands, c).await?,
                            UiEffect::OpenEditor(path) => {
                                let editor = cfg.editor.as_deref();
                                let cmd = editor_command(editor, &path);
                                let ran = hand_over(cmd, &host).await;
                                signals.forget_interrupts();
                                terminal
                                    .clear()
                                    .map_err(|e| TuiError::Terminal(e.to_string()))?;
                                match after_editor(ran, editor) {
                                    Ok(()) => send(&commands, Command::ReloadOrg).await?,
                                    Err(note) => m.footer_note = Some(note),
                                }
                                clock.dirty = true;
                            }
                        }
                    }
                }
            },
            () = sleep_until(wake), if wake.is_some() => {
                if clock.on_wake(&m) {
                    draw(terminal, &m, &cfg, clock.spinner)?;
                    clock.dirty = false;
                    clock.last_draw = Some(Instant::now());
                    if let Some(tx) = first_frame.take() {
                        // The receiver may be gone; the app runs on regardless.
                        let _ = tx.send(());
                    }
                }
            }
        }
    }
}

async fn sleep_until(at: Option<Instant>) {
    if let Some(at) = at {
        tokio::time::sleep_until(at).await;
    }
}

async fn send(commands: &mpsc::Sender<Command>, c: Command) -> Result<(), TuiError> {
    commands.send(c).await.map_err(|_| TuiError::CoreStopped)
}

/// Runs `cmd` with the terminal handed over. A `ctrl+c` typed while the editor has the
/// terminal reaches DuoHive as SIGINT too; it is swallowed here so DuoHive keeps running.
async fn hand_over(cmd: Process, host: &Arc<dyn TerminalHost>) -> io::Result<ExitStatus> {
    let host = Arc::clone(host);
    let mut job = tokio::task::spawn_blocking(move || host.hand_over(cmd));
    let mut swallowing = true;
    loop {
        tokio::select! {
            ran = &mut job => return ran.map_err(io::Error::other)?,
            got = tokio::signal::ctrl_c(), if swallowing => {
                // Without a signal handler there is nothing to swallow; just wait.
                swallowing = got.is_ok();
            }
        }
    }
}

/// `Ok` when the org should be reloaded; else the footer note saying why not.
fn after_editor(ran: io::Result<ExitStatus>, editor: Option<&str>) -> Result<(), String> {
    let status = ran.map_err(|e| format!("couldn't run the editor: {e}"))?;
    match status.code() {
        Some(0) => Ok(()),
        // The shell's "not executable" and "command not found".
        Some(126 | 127) => {
            let name = editor_or_default(editor)
                .split_whitespace()
                .next()
                .unwrap_or(DEFAULT_EDITOR);
            Err(format!("{name} not found"))
        }
        Some(code) => Err(format!("the editor exited with code {code}")),
        None => Err("the editor was stopped by a signal".to_owned()),
    }
}

fn draw<B: Backend>(
    terminal: &mut Terminal<B>,
    m: &UiModel,
    cfg: &AppConfig,
    spinner: usize,
) -> Result<(), TuiError> {
    if cfg.halted.load(Ordering::SeqCst) {
        return Err(TuiError::Halted);
    }
    let ctx = RenderCtx {
        now: wall_now(),
        spinner,
        palette: palette_for(m.settings.theme, cfg.no_color),
        settings: m.settings.clone(),
        tmux_socket: cfg.tmux_socket.clone(),
    };
    terminal
        .draw(|f| render(f, m, &ctx))
        .map(|_| ())
        .map_err(|e| TuiError::Terminal(e.to_string()))
}

#[cfg(test)]
mod tests;
