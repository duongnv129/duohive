//! The real terminal: raw mode, the alternate screen and a panic hook that restores them;
//! a reader thread that forwards crossterm input, and pauses while the editor has the tty.

use std::io::{self, stdout};
use std::process::Command as Process;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event as TermEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use duohive_core::command::Command;
use duohive_core::event::Event;
use tokio::sync::{mpsc, oneshot};

use super::{AppConfig, InputEvent, TerminalHost, TuiError, run_loop};

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
/// panic hook, on panic.
pub async fn run_app(
    cfg: AppConfig,
    events: mpsc::UnboundedReceiver<Event>,
    commands: mpsc::Sender<Command>,
    first_frame: Option<oneshot::Sender<()>>,
) -> Result<(), TuiError> {
    let mut terminal = ratatui::try_init()?;
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
            )
            .await
        }
        Err(e) => Err(e.into()),
    };
    // The reader may already be gone; it ends within one poll either way.
    let _ = gate_tx.send(Gate::Stop);
    let restored = ratatui::try_restore();
    result.and(restored.map_err(TuiError::from))
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
    fn hand_over(&self, mut cmd: Process) -> io::Result<()> {
        self.tell(Gate::Pause);
        if let Ok(parked) = self.parked.lock() {
            // On a timeout the editor still runs; the reader stops at its next poll.
            let _ = parked.recv_timeout(PARK_WAIT);
        }
        let ran = suspend().and_then(|()| cmd.status().map(|_| ()));
        let back = resume();
        self.tell(Gate::Resume);
        ran.and(back)
    }
}

fn suspend() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen, Show)
}

fn resume() -> io::Result<()> {
    execute!(stdout(), EnterAlternateScreen, Hide)?;
    enable_raw_mode()
}
