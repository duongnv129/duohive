//! The app loop (TD §3.3): applies events, maps keys, sends commands, and draws only when
//! something changed (capped at 30 fps), plus the spinner and once-a-minute age ticks.

mod terminal;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, ExitStatus};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use duohive_core::cli_kind::CliKind;
use duohive_core::command::Command;
use duohive_core::event::Event;
use duohive_core::org::Org;
use duohive_core::ports::PaneView;
use duohive_core::settings::Settings;
use duohive_core::status::AgentState;
use duohive_core::time::Millis;
use duohive_core::validate::DirProbe;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::layout::Rect;
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

pub use terminal::{restore_terminal, run_app, show_blocking};

use crate::format::age_of;
use crate::input::mouse::on_mouse;
use crate::input::{UiEffect, map_key, paste, selected_text, update};
use crate::live::{Live, next_chunk, next_scrollback};
use crate::model::{SharedDirs, UiModel};
use crate::render::{RenderCtx, chat_extra_limit, render_live};
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
    /// The settings at start-up.
    pub settings: Settings,
    /// The local time zone's offset from UTC, in seconds, for chat times.
    pub utc_offset_secs: i32,
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
    /// How many agents are running, Duo included, kept current by the app so the
    /// caller can say so after a quit (they keep running in tmux).
    pub running: Arc<AtomicUsize>,
    /// The live view of the selected agent's pane (P2-D1).
    pub pane_view: Arc<dyn PaneView>,
}

impl fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppConfig")
            .field("org_toml", &self.org_toml)
            .field("settings", &self.settings)
            .field("utc_offset_secs", &self.utc_offset_secs)
            .field("no_color", &self.no_color)
            .field("editor", &self.editor)
            .field("home", &self.home)
            .field("halted", &self.halted)
            .field("running", &self.running)
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
    /// The wheel, or a button press or release (P2-D25); movement is dropped earlier.
    Mouse(MouseEvent),
    /// Quit now, as `q` does: the bench's reopen procedure ends the first app
    /// this way, leaving the agents in tmux (no terminal to type into).
    Quit,
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

    /// Puts `text` on the system clipboard (a selection copied with the mouse). A host with
    /// no real terminal has nowhere to put it.
    fn copy(&self, _text: &str) {}
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

/// The agents whose CLI is running, Duo included.
fn running_agents(m: &UiModel) -> usize {
    m.agents
        .values()
        .filter(|v| v.status.state.is_running())
        .count()
}

/// The loop, with the terminal hand-over for `o` and the quit signals injected.
#[expect(
    clippy::too_many_arguments,
    reason = "the loop's seams, each injected for tests"
)]
#[expect(
    clippy::too_many_lines,
    reason = "one select! over every wake source; split its arms out when it next grows"
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
    // Boxed: the model (and the copy each input compares against) would make the loop's
    // future too large to keep on the stack (`clippy::large_futures`).
    let mut m = Box::new(UiModel::new(Org::blank(CliKind::Claude)));
    m.set_settings(cfg.settings.clone());
    m.org_toml = cfg.org_toml.clone();
    m.dirs = SharedDirs(Arc::clone(&cfg.dirs));
    m.home = cfg.home.clone();
    if let Ok(size) = terminal.size() {
        set_screen(&mut m, size.width, size.height);
    }
    let mut clock = Clock {
        dirty: true,
        ready: false,
        spinner: 0,
        last_draw: None,
        spinner_next: None,
    };
    let mut live = Live::new(Arc::clone(&cfg.pane_view), cfg.no_color);
    loop {
        clock.dirty |= sync_live(&mut live, &mut m, &commands).await?;
        let wake = clock.next_wake(&m);
        let open_at = live.open_at();
        let (streaming, scrolling) = (live.streaming(), live.scrolling_back());
        let (stream, scrollback) = live.waits();
        tokio::select! {
            () = signals.recv() => return Ok(()),
            event = events.recv() => {
                let event = event.ok_or(TuiError::CoreStopped)?;
                let snapshot = matches!(event, Event::Snapshot(_));
                clock.dirty |= m.apply(event);
                clock.ready |= snapshot;
                cfg.running.store(running_agents(&m), Ordering::Relaxed);
            }
            got = input.recv() => match got.ok_or(TuiError::InputClosed)? {
                InputEvent::Quit => return Ok(()),
                InputEvent::Resize(w, h) => {
                    set_screen(&mut m, w, h);
                    clock.dirty = true;
                }
                InputEvent::Mouse(ev) => {
                    let before = m.clone();
                    for effect in on_mouse(&mut m, ev) {
                        match effect {
                            UiEffect::Wheel { up, col, row } => {
                                live.wheel(up, col, row, &mut m);
                                clock.dirty = true;
                            }
                            UiEffect::CopySelection => {
                                copy_selection(terminal, &mut m, &cfg, &live, clock.spinner, &host)?;
                                clock.dirty = true;
                            }
                            _ => {}
                        }
                    }
                    clock.dirty |= m != before;
                }
                InputEvent::Paste(text) => {
                    let before = m.clone();
                    for effect in paste(&mut m, &text) {
                        if let UiEffect::Pane(input) = effect {
                            live.send(input);
                        }
                    }
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
                            UiEffect::Pane(input) => live.send(input),
                            UiEffect::Scroll { up } => {
                                live.scroll(up, &mut m);
                                clock.dirty = true;
                            }
                            UiEffect::Wheel { up, col, row } => {
                                live.wheel(up, col, row, &mut m);
                                clock.dirty = true;
                            }
                            UiEffect::CopySelection => {}
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
            chunk = next_chunk(stream), if streaming => {
                clock.dirty |= live.on_chunk(chunk, &mut m);
            }
            bytes = next_scrollback(scrollback), if scrolling => {
                clock.dirty |= live.on_scrollback(bytes, &mut m);
            }
            () = sleep_until(open_at), if open_at.is_some() => {
                clock.dirty |= live.open_due(&mut m, Instant::now());
            }
            () = sleep_until(wake), if wake.is_some() => {
                if clock.on_wake(&m) {
                    draw(terminal, &m, &cfg, &live, clock.spinner)?;
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

/// Follows the pane's stream (`Live::sync`), then tells the engine when Typing mode
/// starts or ends (P3-D19); `true` when the screen needs a redraw.
async fn sync_live(
    live: &mut Live,
    m: &mut UiModel,
    commands: &mpsc::Sender<Command>,
) -> Result<bool, TuiError> {
    let dirty = live.sync(m, Instant::now());
    if let Some(c) = live.typing_change(m) {
        send(commands, c).await?;
    }
    Ok(dirty)
}

/// Records the terminal's size, which bounds the chat's resizing (P2-D3).
fn set_screen(m: &mut UiModel, w: u16, h: u16) {
    m.screen = Rect::new(0, 0, w, h);
    m.chat_rows_extra = m.chat_rows_extra.min(chat_extra_limit(m));
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

/// Copies the selected text: draws the screen as it is now and reads the text from that
/// frame, so the copy is what the user sees. Says how much was copied.
fn copy_selection<B: Backend>(
    terminal: &mut Terminal<B>,
    m: &mut UiModel,
    cfg: &AppConfig,
    live: &Live,
    spinner: usize,
    host: &Arc<dyn TerminalHost>,
) -> Result<(), TuiError> {
    let frame = draw_frame(terminal, m, cfg, live, spinner, true)?;
    let text = frame.map(|b| selected_text(&b, m)).unwrap_or_default();
    if !text.is_empty() {
        host.copy(&text);
        m.footer_note = Some(format!("copied {} characters", text.chars().count()));
    }
    Ok(())
}

fn draw<B: Backend>(
    terminal: &mut Terminal<B>,
    m: &UiModel,
    cfg: &AppConfig,
    live: &Live,
    spinner: usize,
) -> Result<(), TuiError> {
    draw_frame(terminal, m, cfg, live, spinner, false).map(|_| ())
}

/// Draws a frame; with `keep`, returns a copy of what was drawn.
fn draw_frame<B: Backend>(
    terminal: &mut Terminal<B>,
    m: &UiModel,
    cfg: &AppConfig,
    live: &Live,
    spinner: usize,
    keep: bool,
) -> Result<Option<ratatui::buffer::Buffer>, TuiError> {
    if cfg.halted.load(Ordering::SeqCst) {
        return Err(TuiError::Halted);
    }
    let ctx = RenderCtx {
        now: wall_now(),
        spinner,
        palette: palette_for(m.settings.theme, cfg.no_color),
        settings: m.settings.clone(),
        utc_offset_secs: cfg.utc_offset_secs,
    };
    terminal
        .draw(|f| render_live(f, m, &ctx, live.screen()))
        .map(|done| keep.then(|| done.buffer.clone()))
        .map_err(|e| TuiError::Terminal(e.to_string()))
}

#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod mouse_tests;
#[cfg(test)]
mod tests;
