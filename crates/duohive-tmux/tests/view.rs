//! Integration tests: the live pane view against a real tmux server on a throwaway socket.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test-only helpers")]

mod support;

use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use duohive_core::launch::WindowId;
use duohive_core::names::AgentName;
use duohive_core::ports::{PaneView, Runtime};
use duohive_core::view::{PaneChunk, PaneInput, PaneKey, PaneSize, PaneStream, ViewError};
use duohive_tmux::TmuxPaneView;
use duohive_tmux::runtime::TmuxRuntime;
use support::{Events, TestServer, agent};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc::Receiver;

const SECS: fn(u64) -> Duration = Duration::from_secs;
const MS: fn(u64) -> Duration = Duration::from_millis;
const SIZE: PaneSize = PaneSize { cols: 80, rows: 24 };

/// A server, its runtime (which creates and tags the agents' windows) and a view.
struct Fixture {
    srv: TestServer,
    rt: TmuxRuntime,
    ev: Events,
    view: TmuxPaneView,
}

async fn fixture() -> Fixture {
    let srv = TestServer::new();
    let (rt, ev) = srv.connect().await;
    let view = TmuxPaneView::new(&srv.config()).unwrap();
    Fixture { srv, rt, ev, view }
}

impl Fixture {
    async fn start(&mut self, name: &str, script: &str) -> (AgentName, WindowId) {
        let a = agent(name);
        self.rt.start(&a, self.srv.sh(script));
        let w = self.ev.started(&a).await;
        (a, w)
    }
}

async fn next(rx: &mut Receiver<PaneChunk>, within: Duration) -> Option<PaneChunk> {
    tokio::time::timeout(within, rx.recv()).await.ok().flatten()
}

/// Reads chunks until the text seen so far (a `Reset`'s bytes, then every `Bytes`)
/// contains `needle`; panics with what was seen after `within`.
async fn wait_text(rx: &mut Receiver<PaneChunk>, needle: &str, within: Duration) -> String {
    let deadline = Instant::now() + within;
    let mut seen = Vec::new();
    loop {
        let text = String::from_utf8_lossy(&seen).into_owned();
        if text.contains(needle) {
            return text;
        }
        let left = deadline
            .checked_duration_since(Instant::now())
            .unwrap_or_else(|| panic!("{needle:?} never arrived; saw {text:?}"));
        match next(rx, left).await {
            Some(PaneChunk::Reset { bytes, .. } | PaneChunk::Bytes(bytes)) => {
                seen.extend_from_slice(&bytes);
            }
            Some(PaneChunk::Closed { reason }) => panic!("closed ({reason}); saw {text:?}"),
            None => panic!("{needle:?} never arrived; saw {text:?}"),
        }
    }
}

/// Every chunk until the stream ends; panics if it doesn't end within `within`.
async fn drain_to_end(rx: &mut Receiver<PaneChunk>, within: Duration) -> Vec<PaneChunk> {
    let deadline = Instant::now() + within;
    let mut got = Vec::new();
    loop {
        let left = deadline
            .checked_duration_since(Instant::now())
            .unwrap_or_else(|| panic!("the stream never ended: {got:?}"));
        match tokio::time::timeout(left, rx.recv()).await {
            Ok(Some(chunk)) => got.push(chunk),
            Ok(None) => return got,
            Err(_) => panic!("the stream never ended: {got:?}"),
        }
    }
}

async fn opened(view: &TmuxPaneView, a: &AgentName, size: PaneSize) -> PaneStream {
    let mut s = view.open(a, size).unwrap();
    match next(&mut s.rx, SECS(5)).await {
        Some(PaneChunk::Reset { .. }) => s,
        other => panic!("expected Reset first, got {other:?}"),
    }
}

#[tokio::test]
async fn open_gives_reset_then_bytes() {
    let mut f = fixture().await;
    let (a, w) = f.start("a", "read x; printf 'hi\\n'; exec cat").await;
    let mut s = opened(&f.view, &a, SIZE).await;
    assert!(
        f.srv
            .tmux(&["send-keys", "-t", &w.0, "go", "Enter"])
            .status
            .success()
    );
    let sent = Instant::now();
    let mut seen = Vec::new();
    while !String::from_utf8_lossy(&seen).contains("hi\r\n") {
        let left = MS(200)
            .checked_sub(sent.elapsed())
            .unwrap_or_else(|| panic!("no hi within 200 ms: {seen:?}"));
        match next(&mut s.rx, left).await {
            Some(PaneChunk::Bytes(b)) => seen.extend_from_slice(&b),
            other => panic!("expected Bytes, got {other:?} after {seen:?}"),
        }
    }
}

#[tokio::test]
async fn send_text_and_keys_reach_the_pane() {
    let mut f = fixture().await;
    let (a, _) = f
        .start("a", "stty -icanon -isig -echo; echo ready; exec cat -v")
        .await;
    let mut s = f.view.open(&a, SIZE).unwrap();
    wait_text(&mut s.rx, "ready", SECS(5)).await;
    for input in [
        PaneInput::Text("a".into()),
        PaneInput::Key(PaneKey::Ctrl('c')),
        PaneInput::Key(PaneKey::Up),
        PaneInput::Key(PaneKey::Enter),
    ] {
        f.view.send(s.id, input).unwrap();
    }
    wait_text(&mut s.rx, "a^C^[[A", SECS(5)).await;
}

#[tokio::test]
async fn paste_is_bracketed() {
    let mut f = fixture().await;
    let (a, _) = f
        .start(
            "a",
            "stty -icanon -isig -echo; printf '\\033[?2004hready\\n'; exec cat -v",
        )
        .await;
    let mut s = f.view.open(&a, SIZE).unwrap();
    wait_text(&mut s.rx, "ready", SECS(5)).await;
    f.view
        .send(s.id, PaneInput::Paste("it's $x".into()))
        .unwrap();
    f.view.send(s.id, PaneInput::Key(PaneKey::Enter)).unwrap();
    wait_text(&mut s.rx, "^[[200~it's $x^[[201~", SECS(5)).await;
}

#[tokio::test]
async fn one_stream_at_a_time() {
    let mut f = fixture().await;
    let (a, _) = f.start("a", "exec cat").await;
    let (b, _) = f.start("b", "exec cat").await;
    let mut first = opened(&f.view, &a, SIZE).await;
    let _second = opened(&f.view, &b, SIZE).await;
    let rest = drain_to_end(&mut first.rx, SECS(5)).await;
    assert!(
        matches!(rest.last(), Some(PaneChunk::Closed { .. })),
        "{rest:?}"
    );
    assert_eq!(
        f.view.send(first.id, PaneInput::Text("x".into())),
        Err(ViewError::Closed)
    );
}

/// A raw control client like the runtime's main connection; counts its `%output` lines.
fn main_connection(srv: &TestServer) -> (tokio::process::Child, Arc<AtomicU64>) {
    let mut child = tokio::process::Command::new(&srv.tmux_bin)
        .args(["-f", "/dev/null", "-L", &srv.socket])
        .args([
            "-C",
            "attach-session",
            "-f",
            "no-output,ignore-size",
            "-t",
            "=duohive",
        ])
        .env_remove("TMUX")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let count = Arc::new(AtomicU64::new(0));
    let mut lines = BufReader::new(child.stdout.take().unwrap()).split(b'\n');
    let counter = Arc::clone(&count);
    tokio::spawn(async move {
        while let Ok(Some(line)) = lines.next_segment().await {
            if line.starts_with(b"%output ") {
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    (child, count)
}

#[tokio::test]
async fn other_panes_stay_silenced() {
    let mut f = fixture().await;
    let noisy = "while :; do echo NOISE; sleep 0.05; done";
    let (a, _) = f
        .start("a", "while :; do echo MINE; sleep 0.05; done")
        .await;
    f.start("b", noisy).await;
    f.start("c", noisy).await;
    let (_main, main_lines) = main_connection(&f.srv);
    let mut s = f.view.open(&a, SIZE).unwrap();
    let mut seen = Vec::new();
    let until = Instant::now() + SECS(1);
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        match next(&mut s.rx, left).await {
            Some(PaneChunk::Reset { bytes, .. } | PaneChunk::Bytes(bytes)) => {
                seen.extend_from_slice(&bytes);
            }
            Some(PaneChunk::Closed { reason }) => panic!("closed: {reason}"),
            None => {}
        }
    }
    let text = String::from_utf8_lossy(&seen);
    assert!(text.contains("MINE") && !text.contains("NOISE"), "{text:?}");
    assert_eq!(
        f.view.foreign_output_lines(),
        0,
        "no other pane streamed to the viewer"
    );
    // A window added while viewing streams only until it is turned off.
    f.start("d", noisy).await;
    tokio::time::sleep(MS(500)).await;
    let after_add = f.view.foreign_output_lines();
    tokio::time::sleep(SECS(1)).await;
    assert_eq!(f.view.foreign_output_lines(), after_add);
    assert_eq!(main_lines.load(Ordering::Relaxed), 0);
    let tail = wait_text(&mut s.rx, "MINE", SECS(2)).await;
    assert!(!tail.contains("NOISE"), "{tail:?}");
}

fn window_width(srv: &TestServer, w: &WindowId) -> String {
    srv.tmux_out(&[
        "display",
        "-p",
        "-t",
        &w.0,
        "#{window_width}x#{window_height}",
    ])
    .trim()
    .to_owned()
}

#[tokio::test]
async fn resize_changes_window_size() {
    let mut f = fixture().await;
    let (a, w) = f.start("a", "exec cat").await;
    let s = opened(
        &f.view,
        &a,
        PaneSize {
            cols: 100,
            rows: 30,
        },
    )
    .await;
    assert_eq!(window_width(&f.srv, &w), "100x30");
    f.view
        .resize(s.id, PaneSize { cols: 90, rows: 25 })
        .unwrap();
    let deadline = Instant::now() + SECS(2);
    while window_width(&f.srv, &w) != "90x25" {
        assert!(Instant::now() < deadline, "{}", window_width(&f.srv, &w));
        tokio::time::sleep(MS(20)).await;
    }
}

#[tokio::test]
async fn scrollback_returns_history() {
    let mut f = fixture().await;
    let (a, _) = f.start("a", "seq 1 2000; echo done; exec cat").await;
    let mut s = f.view.open(&a, SIZE).unwrap();
    wait_text(&mut s.rx, "done", SECS(5)).await;
    let rx = f.view.scrollback(s.id, 2000).unwrap();
    let history = tokio::time::timeout(SECS(5), rx).await.unwrap().unwrap();
    let text = String::from_utf8_lossy(&history);
    assert!(text.lines().any(|l| l.trim_end() == "1"), "{text:?}");
    assert!(text.lines().any(|l| l.trim_end() == "2000"));
}

#[tokio::test]
async fn closed_when_window_dies() {
    let mut f = fixture().await;
    let (a, w) = f.start("a", "exec cat").await;
    let mut s = opened(&f.view, &a, SIZE).await;
    assert!(f.srv.tmux(&["kill-window", "-t", &w.0]).status.success());
    let rest = drain_to_end(&mut s.rx, SECS(5)).await;
    assert!(
        matches!(rest.last(), Some(PaneChunk::Closed { .. })),
        "{rest:?}"
    );
}

#[tokio::test]
async fn send_to_stale_stream_fails() {
    let mut f = fixture().await;
    let (a, w) = f.start("a", "exec cat").await;
    let mut s = opened(&f.view, &a, SIZE).await;
    assert!(f.srv.tmux(&["kill-window", "-t", &w.0]).status.success());
    drain_to_end(&mut s.rx, SECS(5)).await;
    let out = f.srv.path("out");
    let (_, w2) = f
        .start("a", &format!("exec cat > '{}'", out.display()))
        .await;
    assert_ne!(w, w2);
    assert_eq!(
        f.view.send(s.id, PaneInput::Text("x".into())),
        Err(ViewError::Closed)
    );
    assert_eq!(
        f.view.send(s.id, PaneInput::Key(PaneKey::Enter)),
        Err(ViewError::Closed)
    );
    assert_eq!(f.view.resize(s.id, SIZE), Err(ViewError::Closed));
    assert!(f.view.scrollback(s.id, 10).is_err());
    tokio::time::sleep(MS(300)).await;
    assert_eq!(std::fs::read_to_string(&out).unwrap_or_default(), "");
    let screen = f.srv.tmux_out(&["capture-pane", "-p", "-t", &w2.0]);
    assert!(!screen.contains('x'), "{screen:?}");
}

#[tokio::test]
async fn open_of_an_agent_with_no_window_closes() {
    let f = fixture().await;
    let mut s = f.view.open(&agent("ghost"), SIZE).unwrap();
    match next(&mut s.rx, SECS(5)).await {
        Some(PaneChunk::Closed { reason }) => assert_eq!(reason, "ghost is not running"),
        other => panic!("expected Closed, got {other:?}"),
    }
    assert_eq!(
        f.view.send(s.id, PaneInput::Text("x".into())),
        Err(ViewError::Closed)
    );
}
