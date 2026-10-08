//! Tests for confirmed delivery of a next-turn task: Task 2.12 (P2-D11).

use std::time::Duration;

use super::super::Input;
use super::super::harness::{Harness, Setup};
use crate::adapter::HookCoverage;
use crate::cli_kind::CliKind;
use crate::event::Event;
use crate::hook::HookMessage;
use crate::launch::WindowId;
use crate::ports::RuntimeEvent;
use crate::status::AgentState;
use crate::testkit::{RuntimeCall, StubAdapter, n};
use crate::worktree::TaskId;

fn hook(agent: &str, event: &str) -> Input {
    Input::Hook(HookMessage {
        sent_at_ms: 0,
        token: "tok-1".into(),
        agent: agent.into(),
        event: event.into(),
        payload: serde_json::json!({}),
    })
}

/// `agent` launched with no task and ready: idle, with token `tok-1`.
fn idle(h: &mut Harness, agent: &str) {
    h.start(agent);
    h.input(hook(agent, "session-start"));
    assert_eq!(h.state(agent), AgentState::Idle);
    h.call_log.lock().unwrap().clear();
}

fn pastes(h: &Harness) -> usize {
    h.calls()
        .iter()
        .filter(|c| matches!(c, RuntimeCall::Paste(..)))
        .count()
}

fn not_taken(events: &[Event]) -> Vec<(String, String)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::GiveWorkNotTaken {
                task: TaskId(task),
                reason,
                ..
            } => Some((task.clone(), reason.clone())),
            _ => None,
        })
        .collect()
}

fn pending(h: &Harness, agent: &str) -> bool {
    h.engine.snapshot().agents[&n(agent)].delivery_pending
}

#[test]
fn next_turn_stays_at_rest_until_taken() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    let events = h.give("squad-1", "Add a test");
    assert_eq!(pastes(&h), 1);
    assert_eq!(h.state("squad-1"), AgentState::Idle);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::AgentStatusChanged { .. })),
        "{events:?}"
    );
    assert_eq!(h.rt("squad-1").last_task.as_deref(), Some("Add a test"));
    assert!(pending(&h, "squad-1"));
    h.input(hook("squad-1", "busy"));
    assert_eq!(h.state("squad-1"), AgentState::Working);
    assert!(!h.rt("squad-1").status.from_screen);
    assert!(!pending(&h, "squad-1"));
}

#[test]
fn second_give_work_refused_while_pending() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    h.give("squad-1", "Add a test");
    let events = h.give("squad-1", "And another");
    assert_eq!(
        events,
        vec![Event::GiveWorkRefused {
            agent: n("squad-1"),
            state: AgentState::Idle,
            pending: true,
        }]
    );
    assert_eq!(pastes(&h), 1);
    assert_eq!(h.rt("squad-1").last_task.as_deref(), Some("Add a test"));
}

#[test]
fn untaken_paste_reported_after_5s() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    h.give("squad-1", "Add a test");
    let due = h.engine.next_deadline().unwrap();
    assert!(due <= h.t0 + Duration::from_secs(5), "{due:?}");
    assert!(not_taken(&h.tick_at(Duration::from_millis(4_999))).is_empty());
    assert!(pending(&h, "squad-1"));
    let events = h.tick_at(Duration::from_secs(5));
    assert_eq!(
        not_taken(&events),
        vec![("Add a test".to_owned(), "no sign it started".to_owned())]
    );
    assert!(events.iter().any(|e| matches!(
        e,
        Event::AgentRunChanged {
            delivery_pending: false,
            ..
        }
    )));
    assert_eq!(h.state("squad-1"), AgentState::Idle);
    assert!(!pending(&h, "squad-1"));
    // Reported once; another Give work goes ahead.
    assert!(not_taken(&h.tick_at(Duration::from_secs(10))).is_empty());
    h.give("squad-1", "Try again");
    assert_eq!(pastes(&h), 2);
}

#[test]
fn tilde_busy_read_confirms_for_hookless() {
    let mut h = hookless_busy();
    idle(&mut h, "squad-1");
    h.give("squad-1", "Add a test");
    assert_eq!(h.state("squad-1"), AgentState::Idle);
    h.input(Input::Runtime(RuntimeEvent::Screen {
        agent: n("squad-1"),
        text: "BUSY".into(),
    }));
    assert_eq!(h.state("squad-1"), AgentState::Working);
    assert!(h.rt("squad-1").status.from_screen);
    assert!(!pending(&h, "squad-1"));
    assert!(not_taken(&h.tick_at(Duration::from_secs(6))).is_empty());
}

/// A harness whose Claude adapter has no busy hook.
fn hookless_busy() -> Harness {
    Setup {
        claude: StubAdapter {
            coverage: HookCoverage {
                busy: false,
                ..StubAdapter::new(CliKind::Claude).coverage
            },
            ..StubAdapter::new(CliKind::Claude)
        },
        ..Setup::default()
    }
    .build()
}

fn activity(agent: &str, at_secs: u64) -> Input {
    Input::Runtime(RuntimeEvent::Activity {
        agent: n(agent),
        at_secs,
    })
}

fn captures(h: &Harness) -> usize {
    h.calls()
        .iter()
        .filter(|c| matches!(c, RuntimeCall::Capture(..)))
        .count()
}

#[test]
fn pending_delivery_reads_the_screen_on_activity_for_hookless() {
    let mut h = hookless_busy();
    idle(&mut h, "squad-1");
    h.input(activity("squad-1", 1));
    assert_eq!(captures(&h), 0, "no delivery pending: wait to settle");
    h.give("squad-1", "Add a test");
    h.input(activity("squad-1", 2));
    assert_eq!(
        captures(&h),
        1,
        "a turn's output rarely settles: read at once"
    );
    // With a busy hook, the hook confirms: no extra read.
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    h.give("squad-1", "Add a test");
    h.input(activity("squad-1", 2));
    assert_eq!(captures(&h), 0);
}

#[test]
fn paste_failed_reported() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    h.give("squad-1", "Add a test");
    let events = h.input(Input::Runtime(RuntimeEvent::PasteFailed {
        agent: n("squad-1"),
        reason: "no such window".into(),
    }));
    assert_eq!(
        not_taken(&events),
        vec![("Add a test".to_owned(), "no such window".to_owned())]
    );
    assert!(!pending(&h, "squad-1"));
    assert_eq!(h.state("squad-1"), AgentState::Idle);
    // Nothing is left to time out.
    assert!(not_taken(&h.tick_at(Duration::from_secs(6))).is_empty());
}

#[test]
fn a_crash_ends_the_wait() {
    let mut h = Harness::new();
    h.start("squad-1");
    h.input(Input::Runtime(RuntimeEvent::Started {
        agent: n("squad-1"),
        window: WindowId("@1".into()),
    }));
    h.input(hook("squad-1", "session-start"));
    h.give("squad-1", "Add a test");
    let events = h.input(Input::Runtime(RuntimeEvent::PaneDied {
        agent: n("squad-1"),
        window: WindowId("@1".into()),
        status: Some(1),
    }));
    assert_eq!(h.state("squad-1"), AgentState::Errored);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::AgentRunChanged {
            delivery_pending: false,
            ..
        }
    )));
    assert!(!pending(&h, "squad-1"));
    assert!(not_taken(&h.tick_at(Duration::from_secs(6))).is_empty());
}
