//! The wheel over a live agent in the app loop (P2-D25): to the agent when its CLI took
//! the mouse, else DuoHive's snapshot, typing too. Over a `FakePaneView` on a paused clock.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use duohive_core::testkit::{FakePaneView, PaneCall};
use duohive_core::view::{PaneChunk, PaneInput, PaneKey, StreamId, Wheel};
use ratatui::layout::Rect;

use super::run_app_on;
use super::tests::{Harness, cfg_with_view, harness, key, resting_model, snapshot};
use crate::InputEvent;
use crate::fixtures::lines_of;
use crate::focus::Pane;
use crate::model::UiModel;
use crate::render::pane_areas;

/// The harness's screen.
const W: u16 = 100;
const H: u16 = 30;
/// Turns mouse reporting on, with SGR encoding.
const MOUSE_ON: &[u8] = b"\x1b[?1000h\x1b[?1006h";

async fn sleep(n: u64) {
    tokio::time::sleep(Duration::from_millis(n)).await;
}

fn code(c: KeyCode) -> InputEvent {
    InputEvent::Key(KeyEvent::new(c, KeyModifiers::NONE))
}

fn ctrl(c: char) -> InputEvent {
    InputEvent::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}

/// Where the live terminal is drawn, zoomed or not, for the resting model.
fn live_area(zoomed: bool) -> Rect {
    let mut m = resting_model();
    m.screen = Rect::new(0, 0, W, H);
    m.zoomed = zoomed;
    m.focus.pane = Pane::Agent;
    pane_areas(&m).unwrap().live
}

/// A wheel tick at the live area's cell (`dx`, `dy`).
fn wheel_at(up: bool, area: Rect, dx: u16, dy: u16) -> InputEvent {
    let kind = if up {
        MouseEventKind::ScrollUp
    } else {
        MouseEventKind::ScrollDown
    };
    InputEvent::Mouse(MouseEvent {
        kind,
        column: area.x + dx,
        row: area.y + dy,
        modifiers: KeyModifiers::NONE,
    })
}

fn wheel(up: bool) -> InputEvent {
    wheel_at(up, live_area(false), 4, 2)
}

fn history() -> Vec<u8> {
    let mut out: Vec<u8> = (1..=60)
        .flat_map(|i| format!("old {i}\n").into_bytes())
        .collect();
    out.extend_from_slice(b"LIVE\n");
    out
}

/// Runs the app on the resting model (Duo selected) while `driver` plays, then quits;
/// returns the last frame.
async fn run_with(view: &FakePaneView, driver: impl AsyncFnOnce(&mut Harness)) -> Vec<String> {
    run_ending(view, false, driver).await
}

/// [`run_with`], but ending by closing the input instead of quitting, so the last frame
/// keeps Typing mode (quitting needs `ctrl+q` first). Returns the agent pane's title row.
async fn title_after(view: &FakePaneView, driver: impl AsyncFnOnce(&mut Harness)) -> String {
    run_ending(view, true, driver).await[1].clone()
}

async fn run_ending(
    view: &FakePaneView,
    close: bool,
    driver: impl AsyncFnOnce(&mut Harness),
) -> Vec<String> {
    let model: UiModel = resting_model();
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
        // Let the history arrive and the next frame (at most 30 a second) be drawn.
        sleep(100).await;
        if close {
            let (closed, _) = tokio::sync::mpsc::channel(1);
            drop(std::mem::replace(&mut h.input, closed));
        } else {
            h.input.send(ctrl('q')).await.unwrap();
            h.input.send(key('q')).await.unwrap();
        }
    };
    let (result, ()) = tokio::join!(app, drive);
    match result {
        Ok(()) => assert!(!close),
        Err(e) => assert!(close && matches!(e, super::TuiError::InputClosed), "{e}"),
    }
    lines_of(p.terminal.backend().inner.buffer())
}

/// Opens Duo's stream with a `Reset` of `prefix` then `LIVE`, and focuses the agent pane.
async fn open_duo(h: &mut Harness, view: &FakePaneView, prefix: &[u8]) -> StreamId {
    sleep(150).await;
    let id = StreamId(1);
    let mut bytes = prefix.to_vec();
    bytes.extend_from_slice(b"LIVE");
    view.push(
        id,
        PaneChunk::Reset {
            bytes,
            cursor: (4, 0),
        },
    );
    sleep(50).await;
    for _ in 0..2 {
        h.input.send(code(KeyCode::Tab)).await.unwrap();
    }
    id
}

async fn send(h: &mut Harness, e: InputEvent) {
    h.input.send(e).await.unwrap();
    sleep(20).await;
}

fn sent(view: &FakePaneView) -> Vec<PaneInput> {
    view.calls()
        .into_iter()
        .filter_map(|c| match c {
            PaneCall::Send(_, input) => Some(input),
            _ => None,
        })
        .collect()
}

fn asked_history(view: &FakePaneView) -> bool {
    view.calls()
        .iter()
        .any(|c| matches!(c, PaneCall::Scrollback(..)))
}

fn tick(wheel: Wheel) -> PaneInput {
    PaneInput::Mouse {
        wheel,
        col: 5,
        row: 3,
        sgr: true,
    }
}

#[tokio::test(start_paused = true)]
async fn wheel_goes_to_agent_that_wants_mouse_in_selected_and_typing() {
    let view = FakePaneView::default();
    run_with(&view, async |h: &mut Harness| {
        open_duo(h, &view, MOUSE_ON).await;
        send(h, wheel(true)).await;
        send(h, code(KeyCode::Enter)).await;
        send(h, wheel(false)).await;
        assert_eq!(sent(&view), [tick(Wheel::Up), tick(Wheel::Down)]);
        assert!(!asked_history(&view));
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn pgup_goes_to_agent_that_wants_mouse_in_selected_and_typing() {
    let view = FakePaneView::default();
    run_with(&view, async |h: &mut Harness| {
        open_duo(h, &view, MOUSE_ON).await;
        send(h, code(KeyCode::PageUp)).await;
        send(h, code(KeyCode::Enter)).await;
        send(h, code(KeyCode::PageDown)).await;
        assert_eq!(
            sent(&view),
            [
                PaneInput::Key(PaneKey::PageUp),
                PaneInput::Key(PaneKey::PageDown)
            ]
        );
        assert!(!asked_history(&view));
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn wheel_to_agent_in_zoom() {
    let view = FakePaneView::default();
    run_with(&view, async |h: &mut Harness| {
        open_duo(h, &view, MOUSE_ON).await;
        send(h, key('z')).await;
        sleep(200).await;
        send(h, wheel_at(true, live_area(true), 4, 2)).await;
        assert_eq!(sent(&view).last(), Some(&tick(Wheel::Up)));
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn mouse_mode_change_in_live_output_switches_route() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    run_with(&view, async |h: &mut Harness| {
        let id = open_duo(h, &view, MOUSE_ON).await;
        view.push(id, PaneChunk::Bytes(b"\x1b[?1000l".to_vec()));
        sleep(20).await;
        send(h, wheel(true)).await;
        assert!(asked_history(&view));
        assert_eq!(sent(&view), []);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn wheel_up_freezes_snapshot_three_lines_per_tick() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    let lines = run_with(&view, async |h: &mut Harness| {
        open_duo(h, &view, b"").await;
        send(h, wheel(true)).await;
        assert!(asked_history(&view));
    })
    .await;
    let text = lines.join("\n");
    assert!(lines[1].contains("· scroll"), "{}", lines[1]);
    assert!(text.contains("old 58"), "{text}");
    assert!(!text.contains("old 59") && !text.contains("LIVE"), "{text}");
}

#[tokio::test(start_paused = true)]
async fn wheel_down_past_bottom_returns_live() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    let lines = run_with(&view, async |h: &mut Harness| {
        open_duo(h, &view, b"").await;
        send(h, wheel(true)).await;
        send(h, wheel(false)).await;
    })
    .await;
    assert!(!lines[1].contains("· scroll"), "{}", lines[1]);
    assert!(lines.join("\n").contains("LIVE"));
}

#[tokio::test(start_paused = true)]
async fn snapshot_fallback_in_zoom() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    run_with(&view, async |h: &mut Harness| {
        open_duo(h, &view, b"").await;
        send(h, key('z')).await;
        sleep(200).await;
        send(h, wheel_at(true, live_area(true), 4, 2)).await;
        assert!(asked_history(&view));
    })
    .await;
}

/// Typing, then one wheel tick up into the snapshot.
async fn typing_scrolled(h: &mut Harness, view: &FakePaneView) {
    open_duo(h, view, b"").await;
    send(h, code(KeyCode::Enter)).await;
    send(h, wheel(true)).await;
    assert!(asked_history(view));
}

#[tokio::test(start_paused = true)]
async fn typing_scrolled_title_keeps_both_words() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    let title = title_after(&view, async |h: &mut Harness| {
        typing_scrolled(h, &view).await;
    })
    .await;
    assert!(
        title.contains("TYPING") && title.contains("scroll"),
        "{title}"
    );
}

#[tokio::test(start_paused = true)]
async fn typing_wheel_past_bottom_returns_live_and_stays_typing() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    let title = title_after(&view, async |h: &mut Harness| {
        typing_scrolled(h, &view).await;
        send(h, wheel(false)).await;
    })
    .await;
    assert!(
        title.contains("TYPING") && !title.contains("scroll"),
        "{title}"
    );
}

#[tokio::test(start_paused = true)]
async fn typing_scrolled_pgup_pages_snapshot() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    let lines = run_with(&view, async |h: &mut Harness| {
        typing_scrolled(h, &view).await;
        send(h, code(KeyCode::PageUp)).await;
        assert_eq!(sent(&view), []);
    })
    .await;
    let text = lines.join("\n");
    assert!(!text.contains("old 58"), "{text}");
}

#[tokio::test(start_paused = true)]
async fn typing_scrolled_pgdn_past_bottom_returns_live_and_stays_typing() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    let title = title_after(&view, async |h: &mut Harness| {
        typing_scrolled(h, &view).await;
        send(h, code(KeyCode::PageDown)).await;
        assert_eq!(sent(&view), []);
    })
    .await;
    assert!(
        title.contains("TYPING") && !title.contains("scroll"),
        "{title}"
    );
}

#[tokio::test(start_paused = true)]
async fn typing_scrolled_ctrl_q_leaves_typing_and_stays_scrolled() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    let title = title_after(&view, async |h: &mut Harness| {
        typing_scrolled(h, &view).await;
        send(h, ctrl('q')).await;
    })
    .await;
    assert!(
        !title.contains("TYPING") && title.contains("scroll"),
        "{title}"
    );
}

#[tokio::test(start_paused = true)]
async fn typing_scrolled_key_returns_live_and_is_delivered() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    let title = title_after(&view, async |h: &mut Harness| {
        typing_scrolled(h, &view).await;
        send(h, key('x')).await;
        assert_eq!(sent(&view), [PaneInput::Text("x".into())]);
    })
    .await;
    assert!(
        title.contains("TYPING") && !title.contains("scroll"),
        "{title}"
    );
}

#[tokio::test(start_paused = true)]
async fn typing_scrolled_paste_returns_live_and_is_delivered() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    let title = title_after(&view, async |h: &mut Harness| {
        typing_scrolled(h, &view).await;
        send(h, InputEvent::Paste("hi".into())).await;
        assert_eq!(sent(&view), [PaneInput::Paste("hi".into())]);
    })
    .await;
    assert!(!title.contains("scroll"), "{title}");
}

#[tokio::test(start_paused = true)]
async fn typing_unscrolled_pgup_goes_to_agent() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    run_with(&view, async |h: &mut Harness| {
        open_duo(h, &view, b"").await;
        send(h, code(KeyCode::Enter)).await;
        send(h, code(KeyCode::PageUp)).await;
        assert_eq!(sent(&view), [PaneInput::Key(PaneKey::PageUp)]);
        assert!(!asked_history(&view));
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn mouse_reporting_turned_on_while_scrolled_keeps_snapshot() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    let lines = run_with(&view, async |h: &mut Harness| {
        let id = open_duo(h, &view, b"").await;
        send(h, wheel(true)).await;
        view.push(id, PaneChunk::Bytes(MOUSE_ON.to_vec()));
        sleep(20).await;
        send(h, wheel(true)).await;
        assert_eq!(sent(&view), []);
    })
    .await;
    assert!(lines[1].contains("· scroll"), "{}", lines[1]);
}

#[tokio::test(start_paused = true)]
async fn wheel_with_no_live_stream_does_nothing() {
    let view = FakePaneView::default();
    run_with(&view, async |h: &mut Harness| {
        // Before the settle: no stream yet.
        send(h, wheel(true)).await;
        assert!(
            view.calls()
                .iter()
                .all(|c| !matches!(c, PaneCall::Send(..) | PaneCall::Scrollback(..)))
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn wheel_over_stopped_agent_does_nothing() {
    let view = FakePaneView::default();
    view.set_scrollback(history());
    run_with(&view, async |h: &mut Harness| {
        let id = open_duo(h, &view, MOUSE_ON).await;
        view.close(id, "the window closed");
        sleep(20).await;
        send(h, wheel(true)).await;
        assert_eq!(sent(&view), []);
        assert!(!asked_history(&view));
    })
    .await;
}
