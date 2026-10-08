//! The live stream's lifecycle in the app loop (P2-D1, P2-D4, P2-D5, P2-D12), over a
//! `FakePaneView` and tokio's paused clock.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::event::Event;
use duohive_core::status::AgentState;
use duohive_core::testkit::{FakePaneView, PaneCall, n};
use duohive_core::view::{PaneChunk, PaneInput, PaneKey, PaneSize, StreamId};
use ratatui::layout::Rect;

use super::run_app_on;
use super::tests::{Harness, cfg_with_view, harness, key, resting_model, snapshot};
use crate::InputEvent;
use crate::fixtures::{lines_of, mins, status};
use crate::model::UiModel;
use crate::render::live_size;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

async fn sleep(n: u64) {
    tokio::time::sleep(ms(n)).await;
}

fn code(c: KeyCode) -> InputEvent {
    InputEvent::Key(KeyEvent::new(c, KeyModifiers::NONE))
}

fn ctrl(c: char) -> InputEvent {
    InputEvent::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}

/// The live terminal's size for `m` on a `w`×`h` screen.
fn size_at(m: &UiModel, w: u16, h: u16) -> PaneSize {
    let mut m = m.clone();
    m.screen = Rect::new(0, 0, w, h);
    live_size(&m).unwrap()
}

fn opens(view: &FakePaneView) -> Vec<PaneCall> {
    view.calls()
        .into_iter()
        .filter(|c| matches!(c, PaneCall::Open(..)))
        .collect()
}

/// A screen of one line, the cursor after it.
fn reset(text: &str) -> PaneChunk {
    PaneChunk::Reset {
        bytes: text.as_bytes().to_vec(),
        cursor: (u16::try_from(text.len()).unwrap(), 0),
    }
}

/// Runs the app on `model` (Duo selected, idle, so running) at 100×30 while `driver`
/// plays; returns the last frame drawn.
async fn run_with(
    model: UiModel,
    view: &FakePaneView,
    driver: impl AsyncFnOnce(&mut Harness),
) -> Vec<String> {
    let (mut h, mut p) = harness();
    h.events.send(snapshot(&model)).unwrap();
    let app = run_app_on(
        &mut p.terminal,
        p.input,
        cfg_with_view(true, view),
        &mut p.events,
        p.commands,
        Some(p.first_frame),
    );
    let drive = async {
        (&mut h.first_frame).await.unwrap();
        driver(&mut h).await;
    };
    let (result, ()) = tokio::join!(app, drive);
    result.unwrap();
    lines_of(p.terminal.backend().inner.buffer())
}

/// Opens Duo's stream (stream 1) with a `Reset` showing `screen`.
async fn open_duo(view: &FakePaneView, screen: &str) -> StreamId {
    sleep(150).await;
    let id = StreamId(1);
    view.push(id, reset(screen));
    sleep(50).await;
    id
}

/// `tab tab` from the org pane: Needs you, then the agent pane.
async fn focus_agent_pane(h: &mut Harness) {
    for _ in 0..2 {
        h.input.send(code(KeyCode::Tab)).await.unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn stream_opens_after_100ms_settle_and_only_one() {
    let view = FakePaneView::default();
    let m = resting_model();
    let size = size_at(&m, 100, 30);
    run_with(m, &view, async |h: &mut Harness| {
        sleep(50).await;
        assert_eq!(view.calls(), []);
        // Duo → Payments → Backend → auth-service (idle): each move restarts the wait.
        for _ in 0..3 {
            h.input.send(code(KeyCode::Down)).await.unwrap();
        }
        sleep(60).await;
        assert_eq!(view.calls(), [], "Duo's open was cancelled");
        // → billing-api (finished).
        h.input.send(code(KeyCode::Down)).await.unwrap();
        sleep(60).await;
        assert_eq!(view.calls(), []);
        sleep(60).await;
        assert_eq!(opens(&view), [PaneCall::Open(n("billing-api"), size)]);
        assert_eq!(
            view.calls(),
            [
                PaneCall::Open(n("billing-api"), size),
                PaneCall::Resize(StreamId(1), size),
            ]
        );
        sleep(1_000).await;
        assert_eq!(opens(&view).len(), 1);
        h.input.send(key('q')).await.unwrap();
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn stream_closes_on_selection_change() {
    let view = FakePaneView::default();
    let lines = run_with(resting_model(), &view, async |h: &mut Harness| {
        let id = open_duo(&view, "DUO-SCREEN").await;
        assert!(view.receiving(id));
        // → Payments, a team: the stream closes at once, and nothing opens.
        h.input.send(code(KeyCode::Down)).await.unwrap();
        sleep(10).await;
        assert!(!view.receiving(id));
        sleep(500).await;
        assert_eq!(opens(&view).len(), 1);
        // Back on Duo: a fresh stream after the settle.
        h.input.send(code(KeyCode::Up)).await.unwrap();
        sleep(150).await;
        assert_eq!(opens(&view).len(), 2);
        assert!(view.receiving(StreamId(2)));
        h.input.send(code(KeyCode::Down)).await.unwrap();
        sleep(50).await;
        h.input.send(key('q')).await.unwrap();
    })
    .await;
    assert!(!lines.join("\n").contains("DUO-SCREEN"));
}

#[tokio::test(start_paused = true)]
async fn live_terminal_is_drawn_after_reset() {
    let view = FakePaneView::default();
    let lines = run_with(resting_model(), &view, async |h: &mut Harness| {
        let id = open_duo(&view, "hello from duo").await;
        view.push(id, PaneChunk::Bytes(b" and more".to_vec()));
        sleep(50).await;
        h.input.send(key('q')).await.unwrap();
    })
    .await;
    let text = lines.join("\n");
    assert!(text.contains("hello from duo and more"), "{text}");
    assert!(
        text.contains("⬢ Duo · Claude · your manager agent"),
        "{text}"
    );
}

#[tokio::test(start_paused = true)]
async fn resize_follows_pane_size() {
    let view = FakePaneView::default();
    let m = resting_model();
    let (small, big) = (size_at(&m, 100, 30), size_at(&m, 160, 48));
    assert_ne!(small, big);
    run_with(m, &view, async |h: &mut Harness| {
        open_duo(&view, "x").await;
        h.input.send(InputEvent::Resize(160, 48)).await.unwrap();
        sleep(10).await;
        assert_eq!(
            view.calls(),
            [
                PaneCall::Open(n("Duo"), small),
                PaneCall::Resize(StreamId(1), small),
                PaneCall::Resize(StreamId(1), big),
            ]
        );
        // After the settle the stream is opened again, so a fresh `Reset` follows.
        sleep(150).await;
        assert_eq!(
            view.calls()[3..],
            [
                PaneCall::Open(n("Duo"), big),
                PaneCall::Resize(StreamId(2), big),
            ]
        );
        assert!(!view.receiving(StreamId(1)));
        assert!(view.receiving(StreamId(2)));
        h.input.send(key('q')).await.unwrap();
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn typing_ends_when_window_dies() {
    let view = FakePaneView::default();
    let lines = run_with(resting_model(), &view, async |h: &mut Harness| {
        let id = open_duo(&view, "$ ").await;
        focus_agent_pane(h).await;
        h.input.send(code(KeyCode::Enter)).await.unwrap();
        h.input.send(key('x')).await.unwrap();
        sleep(20).await;
        assert_eq!(
            view.calls().last(),
            Some(&PaneCall::Send(id, PaneInput::Text("x".into())))
        );
        view.close(id, "the window closed");
        sleep(50).await;
        // Selected mode again: `q` quits.
        h.input.send(key('q')).await.unwrap();
    })
    .await;
    let footer = lines.last().unwrap();
    assert_eq!(footer.trim_end(), " Duo's session ended");
    assert!(!lines.join("\n").contains("TYPING"));
}

#[tokio::test(start_paused = true)]
async fn stream_closed_before_reset_is_not_running() {
    let view = FakePaneView::default();
    let lines = run_with(resting_model(), &view, async |h: &mut Harness| {
        sleep(150).await;
        view.close(StreamId(1), "Duo is not running");
        sleep(500).await;
        // No retry while nothing changes.
        assert_eq!(opens(&view).len(), 1);
        h.events
            .send(Event::AgentStatusChanged {
                agent: n("Duo"),
                status: status(AgentState::Working, mins(0)),
            })
            .unwrap();
        sleep(150).await;
        assert_eq!(opens(&view).len(), 2);
        view.close(StreamId(2), "Duo is not running");
        sleep(50).await;
        h.input.send(key('q')).await.unwrap();
    })
    .await;
    let text = lines.join("\n");
    assert!(!text.contains("session ended"), "{text}");
    assert!(text.contains("● working"), "{text}");
}

#[tokio::test(start_paused = true)]
async fn typed_keys_and_pastes_reach_the_open_stream() {
    let view = FakePaneView::default();
    run_with(resting_model(), &view, async |h: &mut Harness| {
        let id = open_duo(&view, "$ ").await;
        focus_agent_pane(h).await;
        h.input.send(code(KeyCode::Enter)).await.unwrap();
        h.input.send(key('q')).await.unwrap();
        h.input.send(ctrl('c')).await.unwrap();
        h.input
            .send(InputEvent::Paste("a\nb".to_owned()))
            .await
            .unwrap();
        h.input.send(ctrl('q')).await.unwrap();
        sleep(20).await;
        let sent: Vec<PaneCall> = view
            .calls()
            .into_iter()
            .filter(|c| matches!(c, PaneCall::Send(..)))
            .collect();
        assert_eq!(
            sent,
            [
                PaneCall::Send(id, PaneInput::Text("q".into())),
                PaneCall::Send(id, PaneInput::Key(PaneKey::Ctrl('c'))),
                PaneCall::Send(id, PaneInput::Paste("a\nb".into())),
            ]
        );
        h.input.send(key('q')).await.unwrap();
    })
    .await;
}

fn history() -> Vec<u8> {
    let mut out: Vec<u8> = (1..=60)
        .flat_map(|i| format!("old {i}\n").into_bytes())
        .collect();
    out.extend_from_slice(b"LIVE\n");
    out
}

#[tokio::test(start_paused = true)]
async fn pgup_enters_scrolled_snapshot_and_pgdn_returns_live() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    let lines = run_with(resting_model(), &view, async |h: &mut Harness| {
        let id = open_duo(&view, "LIVE").await;
        focus_agent_pane(h).await;
        h.input.send(code(KeyCode::PageUp)).await.unwrap();
        sleep(50).await;
        assert_eq!(view.calls().last(), Some(&PaneCall::Scrollback(id, 2_000)));
        h.input.send(key('q')).await.unwrap();
    })
    .await;
    let text = lines.join("\n");
    // At 100 columns the title is cut: `· scroll…`.
    assert!(lines[1].contains("· scroll"), "{}", lines[1]);
    assert!(text.contains("old 30"), "{text}");
    assert!(!text.contains("LIVE"), "{text}");

    let view = FakePaneView::default();
    view.set_scrollback(history());
    let lines = run_with(resting_model(), &view, async |h: &mut Harness| {
        open_duo(&view, "LIVE").await;
        focus_agent_pane(h).await;
        for k in [KeyCode::PageUp, KeyCode::PageUp] {
            h.input.send(code(k)).await.unwrap();
            sleep(20).await;
        }
        for _ in 0..2 {
            h.input.send(code(KeyCode::PageDown)).await.unwrap();
        }
        sleep(50).await;
        h.input.send(key('q')).await.unwrap();
    })
    .await;
    let text = lines.join("\n");
    assert!(!lines[1].contains("· scroll"), "{}", lines[1]);
    assert!(text.contains("LIVE"), "{text}");
    assert!(!text.contains("old 30"), "{text}");
}

#[tokio::test(start_paused = true)]
async fn stream_opens_once_the_view_is_ready() {
    let view = FakePaneView::default();
    view.set_ready(false);
    run_with(resting_model(), &view, async |h: &mut Harness| {
        sleep(2_000).await;
        // Nothing is opened against a view that isn't there yet.
        assert_eq!(opens(&view), []);
        view.set_ready(true);
        // No event or key needed: the view joining is noticed on its own.
        sleep(500).await;
        assert_eq!(opens(&view).len(), 1);
        view.push(StreamId(1), reset("hello"));
        sleep(50).await;
        h.input.send(key('q')).await.unwrap();
    })
    .await;
}

fn reconnecting(m: &UiModel, n: u32) -> Event {
    let Event::Snapshot(mut s) = snapshot(m) else {
        unreachable!()
    };
    s.reconnecting = n;
    Event::Snapshot(s)
}

#[tokio::test(start_paused = true)]
async fn reconnecting_closes_quietly_and_reopens_after() {
    let view = FakePaneView::default();
    let m = resting_model();
    let lines = run_with(m.clone(), &view, async |h: &mut Harness| {
        let id = open_duo(&view, "DUO").await;
        h.events.send(reconnecting(&m, 2)).unwrap();
        sleep(10).await;
        assert!(!view.receiving(id));
        h.events.send(Event::Reconnected).unwrap();
        sleep(150).await;
        assert_eq!(opens(&view).len(), 2);
        view.push(StreamId(2), reset("DUO AGAIN"));
        sleep(50).await;
        h.input.send(key('q')).await.unwrap();
    })
    .await;
    let text = lines.join("\n");
    assert!(!text.contains("session ended"), "{text}");
    assert!(text.contains("DUO AGAIN"), "{text}");
}

#[tokio::test(start_paused = true)]
async fn hidden_pane_closes_quietly_and_reopens_when_shown() {
    let view = FakePaneView::default();
    let lines = run_with(resting_model(), &view, async |h: &mut Harness| {
        let id = open_duo(&view, "DUO").await;
        focus_agent_pane(h).await;
        h.input.send(code(KeyCode::Enter)).await.unwrap();
        // Too small: the main screen (and the pane) is gone.
        h.input.send(InputEvent::Resize(60, 20)).await.unwrap();
        sleep(10).await;
        assert!(!view.receiving(id));
        h.input.send(InputEvent::Resize(100, 30)).await.unwrap();
        sleep(150).await;
        assert_eq!(opens(&view).len(), 2);
        view.push(StreamId(2), reset("BACK"));
        sleep(50).await;
        // Typing ended with the stream: `q` quits.
        h.input.send(key('q')).await.unwrap();
    })
    .await;
    let text = lines.join("\n");
    assert!(!text.contains("session ended"), "{text}");
    assert!(text.contains("BACK"), "{text}");
}

#[tokio::test(start_paused = true)]
async fn resize_while_typing_keeps_the_stream_until_typing_ends() {
    let view = FakePaneView::default();
    let m = resting_model();
    let big = size_at(&m, 160, 48);
    run_with(m, &view, async |h: &mut Harness| {
        let id = open_duo(&view, "$ ").await;
        focus_agent_pane(h).await;
        h.input.send(code(KeyCode::Enter)).await.unwrap();
        h.input.send(InputEvent::Resize(160, 48)).await.unwrap();
        sleep(500).await;
        assert_eq!(opens(&view).len(), 1, "no re-open while typing");
        assert!(view.calls().contains(&PaneCall::Resize(id, big)));
        h.input.send(key('x')).await.unwrap();
        sleep(10).await;
        assert_eq!(
            view.calls().last(),
            Some(&PaneCall::Send(id, PaneInput::Text("x".into())))
        );
        // Typing ends: now the stream is opened again.
        h.input.send(ctrl('q')).await.unwrap();
        sleep(150).await;
        assert_eq!(
            opens(&view),
            [
                PaneCall::Open(n("Duo"), size_at(&resting_model(), 100, 30)),
                PaneCall::Open(n("Duo"), big)
            ]
        );
        h.input.send(key('q')).await.unwrap();
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn closed_around_a_resize_still_ends_typing_with_the_note() {
    let view = FakePaneView::default();
    let lines = run_with(resting_model(), &view, async |h: &mut Harness| {
        let id = open_duo(&view, "$ ").await;
        focus_agent_pane(h).await;
        h.input.send(code(KeyCode::Enter)).await.unwrap();
        sleep(10).await;
        view.close(id, "the window closed");
        h.input.send(InputEvent::Resize(160, 48)).await.unwrap();
        sleep(200).await;
        assert_eq!(opens(&view).len(), 1);
        h.input.send(key('q')).await.unwrap();
    })
    .await;
    assert_eq!(lines.last().unwrap().trim_end(), " Duo's session ended");
}
