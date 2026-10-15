//! Tests for desktop alert triggers: Task 2.9.

use super::super::Input;
use super::super::harness::{Harness, Setup};
use crate::alert::{Alert, AlertKind, alert_for};
use crate::event::Event;
use crate::hook::HookMessage;
use crate::launch::{Token, WindowId};
use crate::ports::{LiveWindow, RuntimeEvent, SessionRow};
use crate::settings::{AlertSettings, Settings};
use crate::status::{AgentState, Status};
use crate::testkit::n;
use crate::time::Millis;

const AGENT: &str = "auth-service";

fn hook_with(event: &str, payload: serde_json::Value) -> Input {
    Input::Hook(HookMessage {
        sent_at_ms: 0,
        token: "tok-1".into(),
        agent: AGENT.into(),
        event: event.into(),
        payload,
    })
}

fn hook(event: &str) -> Input {
    hook_with(event, serde_json::json!({}))
}

/// auth-service working, with no alert yet.
fn working() -> Harness {
    let mut h = Harness::new();
    h.start(AGENT);
    h.input(Input::Runtime(RuntimeEvent::Started {
        agent: n(AGENT),
        window: WindowId("@1".into()),
    }));
    h.input(hook("busy"));
    assert_eq!(h.state(AGENT), AgentState::Working);
    assert!(h.notified().is_empty());
    h
}

fn titles(h: &Harness) -> Vec<String> {
    h.notified().into_iter().map(|a| a.title).collect()
}

fn status(state: AgentState, quiet: bool) -> Status {
    Status {
        state,
        quiet,
        quiet_since: None,
        from_screen: false,
        since: Millis(0),
        error: None,
    }
}

#[test]
fn alerts_on_waiting_errored_quiet_finished() {
    // waiting
    let mut h = working();
    h.input(hook("needs-input"));
    assert_eq!(titles(&h), ["auth-service needs you"]);
    // errored
    h.input(Input::Runtime(RuntimeEvent::PaneDied {
        agent: n(AGENT),
        window: WindowId("@1".into()),
        status: Some(1),
    }));
    assert_eq!(h.state(AGENT), AgentState::Errored);
    assert_eq!(titles(&h).len(), 2);
    assert_eq!(h.notified()[1].kind, AlertKind::NeedsYou);
    // quiet
    let mut h = working();
    h.tick_at(std::time::Duration::from_secs(301));
    assert!(h.rt(AGENT).status.quiet);
    assert_eq!(titles(&h), ["auth-service needs you"]);
    // finished
    let mut h = working();
    h.input(hook("turn-end"));
    assert_eq!(titles(&h), ["auth-service finished"]);
    assert_eq!(h.notified()[0].kind, AlertKind::Finished);
}

#[test]
fn duo_finishing_never_alerts() {
    let before = status(AgentState::Working, false);
    let after = status(AgentState::Finished, false);
    assert_eq!(alert_for(&n("duo"), true, &before, &after, None), None);
    assert!(alert_for(&n(AGENT), false, &before, &after, None).is_some());
}

#[test]
fn alert_for_fires_only_on_the_edge() {
    let a = n(AGENT);
    let working = status(AgentState::Working, false);
    let waiting = status(AgentState::Waiting, false);
    let quiet = status(AgentState::Working, true);
    assert!(alert_for(&a, false, &waiting, &waiting, None).is_none());
    assert!(alert_for(&a, false, &quiet, &quiet, None).is_none());
    assert!(alert_for(&a, false, &quiet, &working, None).is_none());
    assert!(alert_for(&a, false, &working, &quiet, None).is_some());
    let idle = status(AgentState::Idle, false);
    assert!(alert_for(&a, false, &working, &idle, None).is_none());
}

#[test]
fn one_alert_per_state_change() {
    let mut h = working();
    h.input(hook("needs-input"));
    h.input(hook("needs-input"));
    assert_eq!(h.notified().len(), 1);
    h.input(hook("busy"));
    assert_eq!(h.state(AGENT), AgentState::Working);
    h.input(hook("needs-input"));
    assert_eq!(h.notified().len(), 2);
}

#[test]
fn alert_is_published_as_an_event() {
    let mut h = working();
    let out = h.input(hook("needs-input"));
    let alerts: Vec<&Alert> = out
        .iter()
        .filter_map(|e| match e {
            Event::Alert(a) => Some(a),
            _ => None,
        })
        .collect();
    assert_eq!(alerts, h.notified().iter().collect::<Vec<_>>());
}

#[test]
fn body_is_last_reply_or_empty() {
    let mut h = working();
    h.input(hook("needs-input"));
    assert_eq!(h.notified()[0].body, "");
    let mut h = working();
    h.input(hook_with(
        "turn-end",
        serde_json::json!({"last_reply": "All done."}),
    ));
    assert_eq!(h.notified()[0].body, "All done.");
}

#[test]
fn disabled_kind_not_sent() {
    let settings = Settings {
        alerts: AlertSettings {
            needs_you: false,
            finished: true,
        },
        ..Settings::default()
    };
    let mut h = Setup {
        settings,
        ..Setup::default()
    }
    .build();
    h.start(AGENT);
    h.input(Input::Runtime(RuntimeEvent::Started {
        agent: n(AGENT),
        window: WindowId("@1".into()),
    }));
    h.input(hook("busy"));
    let out = h.input(hook("needs-input"));
    assert!(h.notified().is_empty());
    assert!(!out.iter().any(|e| matches!(e, Event::Alert(_))));
    h.input(hook("busy"));
    h.input(hook("turn-end"));
    assert_eq!(titles(&h), ["auth-service finished"]);
}

#[test]
fn unavailable_published_once() {
    let mut h = Setup {
        notifier_available: false,
        ..Setup::default()
    }
    .build();
    let out = h.engine.start_up();
    assert_eq!(
        out.iter()
            .filter(|e| matches!(e, Event::AlertsUnavailable))
            .count(),
        1
    );
    let out = h.input(Input::Runtime(RuntimeEvent::Inventory(Vec::new())));
    assert!(!out.iter().any(|e| matches!(e, Event::AlertsUnavailable)));
    let mut h = Harness::new();
    assert!(
        !h.engine
            .start_up()
            .iter()
            .any(|e| matches!(e, Event::AlertsUnavailable))
    );
}

#[test]
fn click_becomes_event() {
    let mut h = Harness::new();
    let out = h.input(Input::AlertClicked { agent: n(AGENT) });
    assert_eq!(out, vec![Event::AlertClicked { agent: n(AGENT) }]);
}

fn row(agent: &str, state: AgentState, window: &str) -> SessionRow {
    SessionRow {
        agent: n(agent),
        token: Some(Token("tok-1".into())),
        window: Some(WindowId(window.into())),
        status: status(state, false),
        last_reply: None,
        task_at_launch: true,
        saw_end_hook: false,
        last_task: None,
        worktree: None,
    }
}

#[test]
fn no_alerts_during_reconnect_replay() {
    let mut h = Harness::new();
    {
        let mut st = h.store.state.lock().unwrap();
        st.sessions
            .insert(n(AGENT), row(AGENT, AgentState::Working, "@1"));
        st.pending.push(HookMessage {
            sent_at_ms: 0,
            token: "tok-1".into(),
            agent: AGENT.into(),
            event: "needs-input".into(),
            payload: serde_json::json!({}),
        });
    }
    h.engine.start_up();
    h.input(Input::Runtime(RuntimeEvent::Inventory(vec![LiveWindow {
        agent: n(AGENT),
        window: WindowId("@1".into()),
        dead: None,
        activity_secs: 0,
    }])));
    assert_eq!(h.state(AGENT), AgentState::Waiting);
    assert!(h.notified().is_empty());
    // After the reconnect, changes alert again.
    h.input(hook("busy"));
    h.input(hook("needs-input"));
    assert_eq!(h.notified().len(), 1);
}
