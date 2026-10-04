//! The real terminal: raw mode, the alternate screen and a panic hook that restores them;
//! a reader thread that forwards crossterm input, and pauses while the editor has the tty.

use std::io::{self, stdout};
use std::process::{Command as Process, ExitStatus};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event as TermEvent, KeyEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use duohive_core::command::Command;
use duohive_core::event::Event;
use tokio::sync::{mpsc, oneshot};

use super::{AppConfig, InputEvent, QuitSignals, TerminalHost, TuiError, quits_blocking, run_loop};
use crate::render::{BlockingScreen, render_blocking};

/// How often the reader thread looks for a pause request between reads.
const POLL: Duration = Duration::from_millis(50);
/// How long to wait for the reader to park before handing the tty over anyway.
const PARK_WAIT: Duration = Duration::from_secs(1);
const INPUT_QUEUE: usize = 64;

/// Requests to the reader thread.
enum Gate {
    Pause,
    Resume,
    Stop,
}

/// The app on the real terminal. The terminal is restored on exit and, through ratatui's
/// panic hook, on panic. SIGINT and SIGTERM quit like `q`. `events` is lent, so the
/// caller keeps its handle (and shuts it down) afterwards.
pub async fn run_app(
    cfg: AppConfig,
    events: &mut mpsc::UnboundedReceiver<Event>,
    commands: mpsc::Sender<Command>,
    first_frame: Option<oneshot::Sender<()>>,
) -> Result<(), TuiError> {
    let mut terminal = match ratatui::try_init() {
        Ok(t) => t,
        Err(e) => {
            // Raw mode may be on even though the alternate screen failed.
            let _ = ratatui::try_restore();
            return Err(e.into());
        }
    };
    // Pasted text arrives whole, so a newline in it never reads as enter (forms).
    let _ = execute!(stdout(), EnableBracketedPaste);
    restore_paste_on_panic();
    let (input_tx, input_rx) = mpsc::channel(INPUT_QUEUE);
    let (gate_tx, gate_rx) = std_mpsc::channel();
    let (ack_tx, ack_rx) = std_mpsc::channel();
    let reader = thread::Builder::new()
        .name("duohive-input".to_owned())
        .spawn(move || read_input(&input_tx, &gate_rx, &ack_tx));
    let result = match reader {
        Ok(_) => {
            let host: Arc<dyn TerminalHost> = Arc::new(CrosstermHost {
                gate: Mutex::new(gate_tx.clone()),
                parked: Mutex::new(ack_rx),
            });
            run_loop(
                &mut terminal,
                input_rx,
                cfg,
                events,
                commands,
                first_frame,
                host,
                QuitSignals::install(),
            )
            .await
        }
        Err(e) => Err(e.into()),
    };
    // The reader may already be gone; it ends within one poll either way.
    let _ = gate_tx.send(Gate::Stop);
    let _ = execute!(stdout(), DisableBracketedPaste);
    let restored = ratatui::try_restore();
    result.and(restored.map_err(TuiError::from))
}

/// Puts the terminal back (bracketed paste off, raw mode off, main screen) for a caller
/// that stopped [`run_app`] without letting it finish. Harmless when already restored.
pub fn restore_terminal() {
    let _ = execute!(stdout(), DisableBracketedPaste);
    let _ = ratatui::try_restore();
}

/// Shows a blocking screen (tmux missing, Task 23) until `q` or `ctrl+c`, redrawing on
/// resize; blocking, so run it off the async executor.
///
/// # Errors
/// The terminal cannot be set up, drawn on, read or restored.
pub fn show_blocking(screen: &BlockingScreen) -> Result<(), TuiError> {
    let mut terminal = match ratatui::try_init() {
        Ok(t) => t,
        Err(e) => {
            let _ = ratatui::try_restore();
            return Err(e.into());
        }
    };
    let result = blocking_loop(&mut terminal, screen);
    let restored = ratatui::try_restore();
    result.and(restored.map_err(TuiError::from))
}

fn blocking_loop(
    terminal: &mut ratatui::DefaultTerminal,
    screen: &BlockingScreen,
) -> Result<(), TuiError> {
    loop {
        terminal
            .draw(|f| render_blocking(f, screen))
            .map_err(|e| TuiError::Terminal(e.to_string()))?;
        loop {
            match event::read()? {
                TermEvent::Key(k) if k.kind != KeyEventKind::Release && quits_blocking(&k) => {
                    return Ok(());
                }
                TermEvent::Resize(..) => break,
                _ => {}
            }
        }
    }
}

/// Wraps the panic hook (ratatui's, which restores raw mode and the screen) so a panic
/// also turns bracketed paste off; otherwise the shell would get pastes wrapped in escapes.
fn restore_paste_on_panic() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(stdout(), DisableBracketedPaste);
        previous(info);
    }));
}

/// Forwards key presses and resizes until stopped, the app is gone or the terminal fails.
fn read_input(
    tx: &mpsc::Sender<InputEvent>,
    gate: &std_mpsc::Receiver<Gate>,
    parked: &std_mpsc::Sender<()>,
) {
    loop {
        match gate.try_recv() {
            Ok(Gate::Pause) => {
                if !park(gate, parked) {
                    return;
                }
                continue;
            }
            Ok(Gate::Resume) | Err(std_mpsc::TryRecvError::Empty) => {}
            Ok(Gate::Stop) | Err(std_mpsc::TryRecvError::Disconnected) => return,
        }
        match event::poll(POLL) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(_) => return,
        }
        let input = match event::read() {
            Ok(TermEvent::Key(k)) if k.kind != KeyEventKind::Release => InputEvent::Key(k),
            Ok(TermEvent::Resize(w, h)) => InputEvent::Resize(w, h),
            Ok(TermEvent::Paste(text)) => InputEvent::Paste(text),
            Ok(_) => continue,
            Err(_) => return,
        };
        if tx.blocking_send(input).is_err() {
            return;
        }
    }
}

/// Says it is parked, then waits for `Resume`; `false` when it should stop instead.
fn park(gate: &std_mpsc::Receiver<Gate>, parked: &std_mpsc::Sender<()>) -> bool {
    // The host may have given up waiting; it resumes us all the same.
    let _ = parked.send(());
    loop {
        match gate.recv() {
            Ok(Gate::Resume) => return true,
            Ok(Gate::Pause) => {
                let _ = parked.send(());
            }
            Ok(Gate::Stop) | Err(_) => return false,
        }
    }
}

/// Parks the reader, leaves raw mode and the alternate screen, runs the program, and
/// restores both before letting the reader go on.
struct CrosstermHost {
    gate: Mutex<std_mpsc::Sender<Gate>>,
    parked: Mutex<std_mpsc::Receiver<()>>,
}

impl CrosstermHost {
    fn tell(&self, g: Gate) {
        if let Ok(gate) = self.gate.lock() {
            // A reader that is gone has nothing to pause.
            let _ = gate.send(g);
        }
    }
}

impl TerminalHost for CrosstermHost {
    fn hand_over(&self, mut cmd: Process) -> io::Result<ExitStatus> {
        let parked = self.parked.lock();
        if let Ok(parked) = &parked {
            // A late answer from an earlier hand-over must not count as this one's.
            while parked.try_recv().is_ok() {}
        }
        self.tell(Gate::Pause);
        if let Ok(parked) = &parked {
            // On a timeout the editor still runs; the reader stops at its next poll.
            let _ = parked.recv_timeout(PARK_WAIT);
        }
        let ran = suspend().and_then(|()| cmd.status());
        let back = resume();
        self.tell(Gate::Resume);
        back.and(ran)
    }
}

fn suspend() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(stdout(), DisableBracketedPaste, LeaveAlternateScreen, Show)
}

fn resume() -> io::Result<()> {
    execute!(stdout(), EnterAlternateScreen, EnableBracketedPaste, Hide)?;
    enable_raw_mode()
}
