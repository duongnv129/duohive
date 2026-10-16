//! The real terminal: raw mode, the alternate screen and a panic hook that restores them;
//! a reader thread that forwards crossterm input, and pauses while the editor has the tty.

use std::io::{self, Write, stdout};
use std::mem::ManuallyDrop;
use std::process::{Command as Process, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event as TermEvent, KeyEventKind, MouseEventKind,
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
/// panic hook, on panic (which also sets `cfg.halted`, so nothing is drawn after it).
/// `signals` quit like `q`. `events` and `signals` are lent, so the caller keeps its
/// handle (and shuts it down) and its signal listeners afterwards.
pub async fn run_app(
    cfg: AppConfig,
    events: &mut mpsc::UnboundedReceiver<Event>,
    commands: mpsc::Sender<Command>,
    first_frame: Option<oneshot::Sender<()>>,
    signals: &mut QuitSignals,
) -> Result<(), TuiError> {
    // Never dropped, on any path (a return, a panic unwinding through here, or the
    // caller dropping this future): `Terminal`'s Drop shows a hidden cursor and reports
    // a failure with `eprintln!`, which panics when the terminal (and stderr) is gone,
    // as after a SIGHUP, and inside the panic hook aborts before the orderly shutdown.
    let mut terminal = match ratatui::try_init() {
        Ok(t) => ManuallyDrop::new(t),
        Err(e) => {
            // Raw mode may be on even though the alternate screen failed.
            let _ = ratatui::try_restore();
            return Err(e.into());
        }
    };
    // Pasted text arrives whole, so a newline in it never reads as enter (forms); the
    // wheel and clicks arrive as mouse events (P2-D25).
    let _ = input_modes_on(&mut stdout());
    restore_paste_on_panic(Arc::clone(&cfg.halted));
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
                signals,
            )
            .await
        }
        Err(e) => Err(e.into()),
    };
    // The reader may already be gone; it ends within one poll either way.
    let _ = gate_tx.send(Gate::Stop);
    // What the `Terminal`'s Drop would do, with errors ignored rather than printed.
    let _ = input_modes_off(&mut stdout());
    let _ = execute!(stdout(), Show);
    let restored = ratatui::try_restore();
    if signals.hung_up() {
        // A closed terminal can't be restored (the attempt only returns errors); a
        // `kill -HUP` on a live one just was. Either way this is a quit.
        return result;
    }
    result.and(restored.map_err(TuiError::from))
}

/// Puts the terminal back (bracketed paste and the mouse off, raw mode off, main screen)
/// for a caller that stopped [`run_app`] without letting it finish. Harmless when already
/// restored.
pub fn restore_terminal() {
    let _ = input_modes_off(&mut stdout());
    let _ = ratatui::try_restore();
}

/// Shows a blocking screen (tmux missing, Task 23) until `q` or `ctrl+c`, redrawing on
/// resize; blocking, so run it off the async executor.
///
/// # Errors
/// The terminal cannot be set up, drawn on, read or restored.
pub fn show_blocking(screen: &BlockingScreen) -> Result<(), TuiError> {
    // Never dropped, as in `run_app`.
    let mut terminal = match ratatui::try_init() {
        Ok(t) => ManuallyDrop::new(t),
        Err(e) => {
            let _ = ratatui::try_restore();
            return Err(e.into());
        }
    };
    let result = blocking_loop(&mut terminal, screen);
    let _ = execute!(stdout(), Show);
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
/// first stops drawing (`halted`), and also turns bracketed paste and the mouse off;
/// otherwise the shell would get pastes wrapped in escapes, and mouse reports.
fn restore_paste_on_panic(halted: Arc<AtomicBool>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        halted.store(true, Ordering::SeqCst);
        let _ = input_modes_off(&mut stdout());
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
            Ok(ev) => match to_input(ev) {
                Some(input) => input,
                None => continue,
            },
            Err(_) => return,
        };
        if tx.blocking_send(input).is_err() {
            return;
        }
    }
}

/// What the loop gets for a terminal event: key presses, resizes, pastes, and the
/// wheel and button presses and releases. Movement and drags are dropped here, so a
/// moving pointer can't fill the input queue (crossterm reports every motion).
fn to_input(ev: TermEvent) -> Option<InputEvent> {
    match ev {
        TermEvent::Key(k) if k.kind != KeyEventKind::Release => Some(InputEvent::Key(k)),
        TermEvent::Resize(w, h) => Some(InputEvent::Resize(w, h)),
        TermEvent::Paste(text) => Some(InputEvent::Paste(text)),
        TermEvent::Mouse(m) => match m.kind {
            MouseEventKind::Moved | MouseEventKind::Drag(_) => None,
            _ => Some(InputEvent::Mouse(m)),
        },
        _ => None,
    }
}

/// Turns on bracketed paste and mouse reporting.
fn input_modes_on(w: &mut impl Write) -> io::Result<()> {
    execute!(w, EnableBracketedPaste, EnableMouseCapture)
}

/// Turns off bracketed paste and mouse reporting.
fn input_modes_off(w: &mut impl Write) -> io::Result<()> {
    execute!(w, DisableBracketedPaste, DisableMouseCapture)
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
    input_modes_off(&mut stdout())?;
    execute!(stdout(), LeaveAlternateScreen, Show)
}

fn resume() -> io::Result<()> {
    execute!(stdout(), EnterAlternateScreen)?;
    input_modes_on(&mut stdout())?;
    execute!(stdout(), Hide)?;
    enable_raw_mode()
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    use super::*;

    fn mouse(kind: MouseEventKind) -> TermEvent {
        TermEvent::Mouse(MouseEvent {
            kind,
            column: 3,
            row: 4,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn motion_events_dropped() {
        assert_eq!(to_input(mouse(MouseEventKind::Moved)), None);
        assert_eq!(
            to_input(mouse(MouseEventKind::Drag(MouseButton::Left))),
            None
        );
        let TermEvent::Mouse(up) = mouse(MouseEventKind::ScrollUp) else {
            unreachable!()
        };
        assert_eq!(to_input(TermEvent::Mouse(up)), Some(InputEvent::Mouse(up)));
        let TermEvent::Mouse(down) = mouse(MouseEventKind::Down(MouseButton::Left)) else {
            unreachable!()
        };
        assert_eq!(
            to_input(TermEvent::Mouse(down)),
            Some(InputEvent::Mouse(down))
        );
    }

    #[test]
    fn mouse_capture_off_on_exit_restore_panic_and_editor_and_back_on_after_editor() {
        let mut on = Vec::new();
        input_modes_on(&mut on).unwrap();
        let on = String::from_utf8(on).unwrap();
        assert!(
            on.contains("\x1b[?2004h") && on.contains("\x1b[?1000h"),
            "{on:?}"
        );
        let mut off = Vec::new();
        input_modes_off(&mut off).unwrap();
        let off = String::from_utf8(off).unwrap();
        assert!(
            off.contains("\x1b[?2004l") && off.contains("\x1b[?1000l"),
            "{off:?}"
        );
        // Every path that turns paste on or off (start-up, exit, restore_terminal, the
        // panic hook, suspend and resume around the editor) goes through these two.
        let source = include_str!("terminal.rs");
        let body = &source[..source.find("#[cfg(test)]").unwrap()];
        // The two imports, then once in each helper.
        assert_eq!(
            body.matches("BracketedPaste").count(),
            4,
            "paste named outside the helpers"
        );
        assert_eq!(
            body.matches("input_modes_off(").count(),
            5,
            "exit, restore, panic, suspend + fn"
        );
        assert_eq!(
            body.matches("input_modes_on(").count(),
            3,
            "start-up, resume + fn"
        );
    }
}
