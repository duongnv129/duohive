//! The app loop over a counting test backend: when frames are drawn, and what keys send.

use std::io;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, ExitStatus};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::command::Command;
use duohive_core::event::{Event, Snapshot};
use duohive_core::settings::Settings;
use duohive_core::status::AgentState;
use duohive_core::testkit::{FakePaneView, example_org, n};
use ratatui::Terminal;
use ratatui::backend::{Backend, ClearType, TestBackend, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use tokio::sync::{mpsc, oneshot};

use super::{
    AppConfig, InputEvent, QuitSignals, TerminalHost, TuiError, editor_command, quits_blocking,
    run_app_on, run_loop,
};
use crate::fixtures::{mins, model_for, status, view};
use crate::model::UiModel;

/// `TestBackend` that counts flushes: ratatui flushes once per `Terminal::draw`.
pub(super) struct CountingBackend {
    pub(super) inner: TestBackend,
    flushes: Arc<AtomicUsize>,
}

impl Backend for CountingBackend {
    type Error = <TestBackend as Backend>::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.inner.draw(content)
    }
    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.hide_cursor()
    }
    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.show_cursor()
    }
    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        self.inner.get_cursor_position()
    }
    fn set_cursor_position<P: Into<Position>>(&mut self, p: P) -> Result<(), Self::Error> {
        self.inner.set_cursor_position(p)
    }
    fn clear(&mut self) -> Result<(), Self::Error> {
        self.inner.clear()
    }
    fn clear_region(&mut self, t: ClearType) -> Result<(), Self::Error> {
        self.inner.clear_region(t)
    }
    fn size(&self) -> Result<Size, Self::Error> {
        self.inner.size()
    }
    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.inner.window_size()
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        self.flushes.fetch_add(1, Ordering::SeqCst);
        self.inner.flush()
    }
}

/// Channels and counters around one run of the loop.
pub(super) struct Harness {
    pub(super) draws: Arc<AtomicUsize>,
    pub(super) input: mpsc::Sender<InputEvent>,
    pub(super) events: mpsc::UnboundedSender<Event>,
    pub(super) commands: mpsc::Receiver<Command>,
    pub(super) first_frame: oneshot::Receiver<()>,
}

pub(super) struct Parts {
    pub(super) terminal: Terminal<CountingBackend>,
    pub(super) input: mpsc::Receiver<InputEvent>,
    pub(super) events: mpsc::UnboundedReceiver<Event>,
    pub(super) commands: mpsc::Sender<Command>,
    pub(super) first_frame: oneshot::Sender<()>,
}

pub(super) fn harness() -> (Harness, Parts) {
    let draws = Arc::new(AtomicUsize::new(0));
    let backend = CountingBackend {
        inner: TestBackend::new(100, 30),
        flushes: Arc::clone(&draws),
    };
    let (in_tx, in_rx) = mpsc::channel(16);
    let (ev_tx, ev_rx) = mpsc::unbounded_channel();
    let (cmd_tx, cmd_rx) = mpsc::channel(16);
    let (ff_tx, ff_rx) = oneshot::channel();
    (
        Harness {
            draws,
            input: in_tx,
            events: ev_tx,
            commands: cmd_rx,
            first_frame: ff_rx,
        },
        Parts {
            terminal: Terminal::new(backend).unwrap(),
            input: in_rx,
            events: ev_rx,
            commands: cmd_tx,
            first_frame: ff_tx,
        },
    )
}

fn cfg(reduce_motion: bool) -> AppConfig {
    cfg_with_view(reduce_motion, &FakePaneView::default())
}

/// [`cfg`] over `view`, which the test keeps to push chunks and read calls.
pub(super) fn cfg_with_view(reduce_motion: bool, view: &FakePaneView) -> AppConfig {
    AppConfig {
        org_toml: PathBuf::from("/cfg/org.toml"),
        settings: Settings {
            reduce_motion,
            ..Settings::default()
        },
        utc_offset_secs: 0,
        no_color: false,
        editor: Some("nano -w".to_owned()),
        dirs: std::sync::Arc::new(duohive_core::testkit::FakeDirs::new(Default::default())),
        home: PathBuf::from("/home/me"),
        halted: Arc::new(AtomicBool::new(false)),
        running: Arc::new(AtomicUsize::new(0)),
        pane_view: Arc::new(view.clone()),
    }
}

pub(super) fn snapshot(m: &UiModel) -> Event {
    Event::Snapshot(Box::new(Snapshot {
        org: m.org.clone(),
        agents: m.agents.clone(),
        needs_you: m.needs_you.clone(),
        org_error: None,
        settings_error: None,
        reconnecting: 0,
        worktrees: Vec::new(),
        cards: Vec::new(),
        chat: Vec::new(),
        your_queued: Vec::new(),
        first_run: None,
    }))
}

/// Every agent stopped for a day, Duo idle: ages show, nothing works, no spinner.
fn idle_model() -> UiModel {
    model_for(example_org())
}

/// Idle, finished, waiting and stopped agents.
pub(super) fn resting_model() -> UiModel {
    let mut m = model_for(example_org());
    m.agents
        .insert(n("auth-service"), view(status(AgentState::Idle, mins(2))));
    m.agents.insert(
        n("billing-api"),
        view(status(AgentState::Finished, mins(3))),
    );
    m.agents
        .insert(n("squad-1"), view(status(AgentState::Waiting, mins(4))));
    m
}

pub(super) fn key(c: char) -> InputEvent {
    InputEvent::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn count(h: &Harness) -> usize {
    h.draws.load(Ordering::SeqCst)
}

#[tokio::test]
async fn first_frame_signal_fires_once_after_drawing() {
    let (mut h, mut p) = harness();
    h.events.send(snapshot(&idle_model())).unwrap();
    let app = run_app_on(
        &mut p.terminal,
        p.input,
        cfg(true),
        &mut p.events,
        p.commands,
        Some(p.first_frame),
    );
    let driver = async {
        (&mut h.first_frame).await.unwrap();
        assert_eq!(count(&h), 1);
        h.events
            .send(Event::CommandRejected {
                reason: "no".into(),
            })
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(count(&h), 2);
        h.input.send(key('q')).await.unwrap();
    };
    let (result, ()) = tokio::join!(app, driver);
    result.unwrap();
    assert_eq!(count(&h), 2);
}

#[tokio::test(start_paused = true)]
async fn running_agents_are_counted_for_the_quit_line() {
    let (mut h, mut p) = harness();
    // Duo and auth-service idle, billing-api finished, squad-1 waiting.
    h.events.send(snapshot(&resting_model())).unwrap();
    let cfg = cfg(true);
    let running = Arc::clone(&cfg.running);
    let app = run_app_on(
        &mut p.terminal,
        p.input,
        cfg,
        &mut p.events,
        p.commands,
        Some(p.first_frame),
    );
    let driver = async {
        (&mut h.first_frame).await.unwrap();
        assert_eq!(running.load(Ordering::SeqCst), 4);
        h.events
            .send(Event::AgentStatusChanged {
                agent: n("auth-service"),
                status: status(AgentState::Stopped, mins(0)),
            })
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        h.input.send(key('q')).await.unwrap();
    };
    let (result, ()) = tokio::join!(app, driver);
    result.unwrap();
    assert_eq!(running.load(Ordering::SeqCst), 3);
}

#[tokio::test(start_paused = true)]
async fn nothing_is_drawn_before_the_snapshot() {
    let (mut h, mut p) = harness();
    let app = run_app_on(
        &mut p.terminal,
        p.input,
        cfg(true),
        &mut p.events,
        p.commands,
        Some(p.first_frame),
    );
    let driver = async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(count(&h), 0);
        h.events.send(snapshot(&idle_model())).unwrap();
        (&mut h.first_frame).await.unwrap();
        assert_eq!(count(&h), 1);
        h.input.send(key('q')).await.unwrap();
    };
    let (result, ()) = tokio::join!(app, driver);
    result.unwrap();
}

#[tokio::test(start_paused = true)]
async fn idle_app_does_not_redraw() {
    let (mut h, mut p) = harness();
    h.events.send(snapshot(&idle_model())).unwrap();
    let app = run_app_on(
        &mut p.terminal,
        p.input,
        cfg(false),
        &mut p.events,
        p.commands,
        Some(p.first_frame),
    );
    let driver = async {
        (&mut h.first_frame).await.unwrap();
        let before = count(&h);
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(count(&h), before);
        h.input.send(key('q')).await.unwrap();
    };
    let (result, ()) = tokio::join!(app, driver);
    result.unwrap();
}

#[tokio::test(start_paused = true)]
async fn ages_redraw_once_a_minute() {
    let (mut h, mut p) = harness();
    h.events.send(snapshot(&resting_model())).unwrap();
    let app = run_app_on(
        &mut p.terminal,
        p.input,
        cfg(false),
        &mut p.events,
        p.commands,
        Some(p.first_frame),
    );
    let driver = async {
        (&mut h.first_frame).await.unwrap();
        let before = count(&h);
        tokio::time::sleep(Duration::from_secs(59)).await;
        assert_eq!(count(&h), before);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(count(&h), before + 1);
        h.input.send(key('q')).await.unwrap();
    };
    let (result, ()) = tokio::join!(app, driver);
    result.unwrap();
}

#[tokio::test(start_paused = true)]
async fn spinner_ticks_four_times_a_second_while_working() {
    let mut working = idle_model();
    working.agents.insert(
        n("auth-service"),
        view(status(AgentState::Working, mins(2))),
    );
    for (reduce_motion, ticks) in [(false, 4), (true, 0)] {
        let (mut h, mut p) = harness();
        h.events.send(snapshot(&working)).unwrap();
        let app = run_app_on(
            &mut p.terminal,
            p.input,
            cfg(reduce_motion),
            &mut p.events,
            p.commands,
            Some(p.first_frame),
        );
        let driver = async {
            (&mut h.first_frame).await.unwrap();
            let before = count(&h);
            tokio::time::sleep(Duration::from_millis(1_010)).await;
            assert_eq!(count(&h), before + ticks, "reduce motion {reduce_motion}");
            h.input.send(key('q')).await.unwrap();
        };
        let (result, ()) = tokio::join!(app, driver);
        result.unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn keys_send_commands_and_redraw() {
    let (mut h, mut p) = harness();
    h.events.send(snapshot(&idle_model())).unwrap();
    let app = run_app_on(
        &mut p.terminal,
        p.input,
        cfg(true),
        &mut p.events,
        p.commands,
        Some(p.first_frame),
    );
    let driver = async {
        (&mut h.first_frame).await.unwrap();
        let before = count(&h);
        // A snapshot selects Duo; move down to Payments, Backend, auth-service.
        for _ in 0..3 {
            h.input
                .send(InputEvent::Key(KeyEvent::new(
                    KeyCode::Down,
                    KeyModifiers::NONE,
                )))
                .await
                .unwrap();
        }
        h.input.send(key('s')).await.unwrap();
        assert_eq!(
            h.commands.recv().await,
            Some(Command::StartAgent {
                agent: n("auth-service")
            })
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(count(&h) > before);
        let after_keys = count(&h);
        h.input.send(InputEvent::Resize(120, 40)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(count(&h), after_keys + 1);
        h.input.send(key('q')).await.unwrap();
    };
    let (result, ()) = tokio::join!(app, driver);
    result.unwrap();
}

/// Records what it was asked to run instead of running it, and "exits" with `code`.
#[derive(Default)]
struct FakeHost {
    runs: Mutex<Vec<(String, Vec<String>)>>,
    code: i32,
    /// Every `set_mouse` call, in order.
    mouse: Mutex<Vec<bool>>,
    /// Every text put on the clipboard, in order.
    copied: Mutex<Vec<String>>,
}

impl TerminalHost for FakeHost {
    fn hand_over(&self, cmd: Process) -> io::Result<ExitStatus> {
        let program = cmd.get_program().to_string_lossy().into_owned();
        let args = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        self.runs.lock().unwrap().push((program, args));
        // A wait status: the exit code sits in the second byte.
        Ok(ExitStatus::from_raw(self.code << 8))
    }

    fn set_mouse(&self, on: bool) {
        self.mouse.lock().unwrap().push(on);
    }

    fn copy(&self, text: &str) {
        self.copied.lock().unwrap().push(text.to_owned());
    }
}

fn mouse_event(kind: crossterm::event::MouseEventKind, column: u16, row: u16) -> InputEvent {
    InputEvent::Mouse(crossterm::event::MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

#[tokio::test(start_paused = true)]
async fn dragging_across_the_agent_pane_copies_what_it_shows() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut h, mut p) = harness();
    h.events.send(snapshot(&idle_model())).unwrap();
    let host = Arc::new(FakeHost::default());
    let mut signals = QuitSignals::none();
    // Where the agent pane is on the harness's 100x30 screen.
    let mut probe = idle_model();
    probe.screen = ratatui::layout::Rect::new(0, 0, 100, 30);
    let agent = crate::render::pane_areas(&probe).unwrap().agent;
    let app = run_loop(
        &mut p.terminal,
        p.input,
        cfg(true),
        &mut p.events,
        p.commands,
        Some(p.first_frame),
        Arc::clone(&host) as Arc<dyn TerminalHost>,
        &mut signals,
    );
    let driver = async {
        (&mut h.first_frame).await.unwrap();
        // The pane's third row reads `    ◦ idle`.
        let row = agent.y + 2;
        let down = MouseEventKind::Down(MouseButton::Left);
        let drag = MouseEventKind::Drag(MouseButton::Left);
        let up = MouseEventKind::Up(MouseButton::Left);
        h.input
            .send(mouse_event(down, agent.x + 5, row))
            .await
            .unwrap();
        h.input
            .send(mouse_event(drag, agent.x + 12, row))
            .await
            .unwrap();
        h.input
            .send(mouse_event(up, agent.x + 12, row))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        h.input.send(key('q')).await.unwrap();
    };
    let (result, ()) = tokio::join!(app, driver);
    result.unwrap();
    assert_eq!(*host.copied.lock().unwrap(), ["◦ idle"]);
}

fn special(code: KeyCode) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

#[tokio::test(start_paused = true)]
async fn mouse_capture_follows_the_setting() {
    let (mut h, mut p) = harness();
    h.events.send(snapshot(&idle_model())).unwrap();
    let host = Arc::new(FakeHost::default());
    let mut signals = QuitSignals::none();
    let app = run_loop(
        &mut p.terminal,
        p.input,
        cfg(true),
        &mut p.events,
        p.commands,
        Some(p.first_frame),
        Arc::clone(&host) as Arc<dyn TerminalHost>,
        &mut signals,
    );
    let driver = async {
        (&mut h.first_frame).await.unwrap();
        // Settings: Name, Quiet, Needs you, Finished, Theme, Last reply, Reduce motion, Mouse.
        h.input.send(key(',')).await.unwrap();
        for _ in 0..7 {
            h.input.send(special(KeyCode::Down)).await.unwrap();
        }
        h.input.send(special(KeyCode::Enter)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        h.input.send(special(KeyCode::Enter)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        h.input.send(special(KeyCode::Esc)).await.unwrap();
        h.input.send(key('q')).await.unwrap();
    };
    let (result, ()) = tokio::join!(app, driver);
    result.unwrap();
    // Off, then on again: one call per change, none at start-up (the setting began on).
    assert_eq!(*host.mouse.lock().unwrap(), [false, true]);
}

#[tokio::test(start_paused = true)]
async fn o_hands_the_terminal_to_the_editor_then_reloads() {
    let (mut h, mut p) = harness();
    h.events.send(snapshot(&idle_model())).unwrap();
    let host = Arc::new(FakeHost::default());
    let mut signals = QuitSignals::none();
    let app = run_loop(
        &mut p.terminal,
        p.input,
        cfg(true),
        &mut p.events,
        p.commands,
        Some(p.first_frame),
        Arc::clone(&host) as Arc<dyn TerminalHost>,
        &mut signals,
    );
    let driver = async {
        (&mut h.first_frame).await.unwrap();
        let before = count(&h);
        h.input.send(key('o')).await.unwrap();
        assert_eq!(h.commands.recv().await, Some(Command::ReloadOrg));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(count(&h), before + 1);
        h.input.send(key('q')).await.unwrap();
    };
    let (result, ()) = tokio::join!(app, driver);
    result.unwrap();
    let runs = host.runs.lock().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].0, "sh");
    assert_eq!(runs[0].1.last().map(String::as_str), Some("/cfg/org.toml"));
}

#[test]
fn editor_command_uses_editor_else_vi() {
    let args = |c: &Process| -> Vec<String> {
        c.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    };
    let path = Path::new("/cfg/my org.toml");
    let c = editor_command(Some("code -w"), path);
    assert_eq!(c.get_program(), "sh");
    assert_eq!(args(&c), ["-c", "code -w \"$1\"", "sh", "/cfg/my org.toml"]);
    for none in [None, Some(""), Some("  ")] {
        let c = editor_command(none, path);
        assert_eq!(args(&c), ["-c", "vi \"$1\"", "sh", "/cfg/my org.toml"]);
    }
}

#[tokio::test]
async fn closed_event_channel_ends_the_app() {
    let (h, mut p) = harness();
    drop(h.events);
    let result = run_app_on(
        &mut p.terminal,
        p.input,
        cfg(true),
        &mut p.events,
        p.commands,
        None,
    )
    .await;
    assert!(matches!(result, Err(TuiError::CoreStopped)), "{result:?}");
    drop(h.input);
}

/// The footer line of the last frame drawn.
fn footer(t: &Terminal<CountingBackend>) -> String {
    let buf = t.backend().inner.buffer();
    let y = buf.area.bottom() - 1;
    (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// Presses `o` with `host` handing over, then `q`; returns the commands sent.
async fn press_o(
    p: &mut Parts,
    mut h: Harness,
    host: Option<Arc<dyn TerminalHost>>,
) -> Vec<Command> {
    h.events.send(snapshot(&idle_model())).unwrap();
    let input = std::mem::replace(&mut p.input, mpsc::channel(1).1);
    let mut events = std::mem::replace(&mut p.events, mpsc::unbounded_channel().1);
    let first_frame = std::mem::replace(&mut p.first_frame, oneshot::channel().0);
    let commands = p.commands.clone();
    let app = async {
        match host {
            Some(host) => {
                run_loop(
                    &mut p.terminal,
                    input,
                    cfg(true),
                    &mut events,
                    commands,
                    Some(first_frame),
                    host,
                    &mut QuitSignals::none(),
                )
                .await
            }
            None => {
                run_app_on(
                    &mut p.terminal,
                    input,
                    cfg(true),
                    &mut events,
                    commands,
                    Some(first_frame),
                )
                .await
            }
        }
    };
    let driver = async {
        (&mut h.first_frame).await.unwrap();
        h.input.send(key('o')).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        h.input.send(key('q')).await.unwrap();
    };
    let (result, ()) = tokio::join!(app, driver);
    result.unwrap();
    let mut sent = Vec::new();
    while let Ok(c) = h.commands.try_recv() {
        sent.push(c);
    }
    sent
}

#[tokio::test(start_paused = true)]
async fn an_editor_that_is_not_found_reloads_nothing() {
    for code in [126, 127] {
        let (h, mut p) = harness();
        let host = Arc::new(FakeHost {
            code,
            ..FakeHost::default()
        });
        let sent = press_o(&mut p, h, Some(host)).await;
        assert_eq!(sent, [], "exit {code}");
        assert_eq!(footer(&p.terminal), " nano not found");
    }
}

#[tokio::test(start_paused = true)]
async fn an_editor_that_fails_reloads_nothing() {
    let (h, mut p) = harness();
    let host = Arc::new(FakeHost {
        code: 3,
        ..FakeHost::default()
    });
    let sent = press_o(&mut p, h, Some(host)).await;
    assert_eq!(sent, []);
    assert_eq!(footer(&p.terminal), " the editor exited with code 3");
}

#[tokio::test(start_paused = true)]
async fn run_app_on_has_no_terminal_to_hand_over() {
    let (h, mut p) = harness();
    let sent = press_o(&mut p, h, None).await;
    assert_eq!(sent, []);
    assert_eq!(
        footer(&p.terminal),
        " couldn't run the editor: no terminal to hand over"
    );
}

/// Signal tests send real signals to this process: one at a time.
static SIGNALS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// `kill -<sig>` this process.
fn raise(sig: &str) {
    let pid = std::process::id().to_string();
    let sent = Process::new("kill").args([sig, &pid]).status().unwrap();
    assert!(sent.success(), "kill {sig}");
}

#[tokio::test]
async fn sigint_sigterm_and_sighup_quit() {
    let _one = SIGNALS.lock().await;
    for sig in ["-INT", "-TERM", "-HUP"] {
        let (mut h, mut p) = harness();
        h.events.send(snapshot(&idle_model())).unwrap();
        let host: Arc<dyn TerminalHost> = Arc::new(FakeHost::default());
        let mut signals = QuitSignals::install();
        let app = run_loop(
            &mut p.terminal,
            p.input,
            cfg(true),
            &mut p.events,
            p.commands,
            Some(p.first_frame),
            host,
            &mut signals,
        );
        let driver = async {
            (&mut h.first_frame).await.unwrap();
            raise(sig);
        };
        let run = async { tokio::join!(app, driver) };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .unwrap_or_else(|_| panic!("{sig} did not quit"));
        result.unwrap();
        // Nothing was sent: agents keep running.
        assert_eq!(h.commands.try_recv().ok(), None);
        // After SIGHUP the terminal is gone: run_app must not touch it again.
        assert_eq!(signals.hung_up(), sig == "-HUP", "{sig}");
    }
}

/// Hands over like an editor that the user pressed ctrl+c in: the terminal sends
/// SIGINT to DuoHive too.
struct CtrlCInEditor;

impl TerminalHost for CtrlCInEditor {
    fn hand_over(&self, _cmd: Process) -> io::Result<ExitStatus> {
        raise("-INT");
        std::thread::sleep(Duration::from_millis(300));
        Ok(ExitStatus::from_raw(0))
    }
}

#[tokio::test]
async fn ctrl_c_in_the_editor_does_not_quit() {
    let _one = SIGNALS.lock().await;
    let mut signals = QuitSignals::install();
    let (mut h, mut p) = harness();
    h.events.send(snapshot(&idle_model())).unwrap();
    let app = run_loop(
        &mut p.terminal,
        p.input,
        cfg(true),
        &mut p.events,
        p.commands,
        Some(p.first_frame),
        Arc::new(CtrlCInEditor),
        &mut signals,
    );
    let driver = async {
        (&mut h.first_frame).await.unwrap();
        h.input.send(key('o')).await.unwrap();
        assert_eq!(h.commands.recv().await, Some(Command::ReloadOrg));
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!h.input.is_closed(), "the editor's ctrl+c quit DuoHive");
        // A later SIGINT still quits.
        raise("-INT");
    };
    let run = async { tokio::join!(app, driver) };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .expect("SIGINT after the editor did not quit");
    result.unwrap();
}

#[test]
fn blocking_screen_quits_on_q_or_ctrl_c() {
    let k = |code, mods| KeyEvent::new(code, mods);
    assert!(quits_blocking(&k(KeyCode::Char('q'), KeyModifiers::NONE)));
    assert!(quits_blocking(&k(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL
    )));
    assert!(!quits_blocking(&k(KeyCode::Char('c'), KeyModifiers::NONE)));
    assert!(!quits_blocking(&k(KeyCode::Enter, KeyModifiers::NONE)));
}

#[tokio::test]
async fn a_halted_app_draws_nothing() {
    // Set by the panic hook once it has restored the terminal.
    let (h, mut p) = harness();
    h.events.send(snapshot(&idle_model())).unwrap();
    let cfg = cfg(true);
    cfg.halted.store(true, Ordering::SeqCst);
    let result = run_app_on(
        &mut p.terminal,
        p.input,
        cfg,
        &mut p.events,
        p.commands,
        Some(p.first_frame),
    )
    .await;
    assert!(matches!(result, Err(TuiError::Halted)), "{result:?}");
    assert_eq!(count(&h), 0);
}
