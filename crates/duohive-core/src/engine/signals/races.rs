//! Races between stops, relaunches, exits and late reports (Task 12 review carry-overs).

use std::time::Duration;

use super::super::Input;
use super::super::harness::Harness;
use crate::command::Command;
use crate::hook::HookMessage;
use crate::launch::WindowId;
use crate::ports::RuntimeEvent;
use crate::status::AgentState;
use crate::testkit::{RuntimeCall, n};

fn hook(agent: &str, token: &str, event: &str) -> Input {
    Input::Hook(HookMessage {
        token: token.into(),
        agent: agent.into(),
        event: event.into(),
        payload: serde_json::json!({}),
    })
}

fn started(agent: &str, window: &str) -> Input {
    Input::Runtime(RuntimeEvent::Started {
        agent: n(agent),
        window: WindowId(window.into()),
    })
}

fn died(agent: &str, window: &str, status: Option<i32>) -> Input {
    Input::Runtime(RuntimeEvent::PaneDied {
        agent: n(agent),
        window: WindowId(window.into()),
        status,
    })
}

fn stop(agent: &str) -> Command {
    Command::StopAgent { agent: n(agent) }
}

/// pay-qa working in `@1`, its pane dead with 0 and waiting for the end hook,
/// then a stop arrives during that grace.
fn stopped_during_grace() -> Harness {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.input(started("pay-qa", "@1"));
    h.input(hook("pay-qa", "tok-1", "busy"));
    assert_eq!(h.input(died("pay-qa", "@1", Some(0))), vec![]);
    h.send(stop("pay-qa"));
    assert!(h.rt("pay-qa").stopping);
    h
}

#[test]
fn stop_during_end_hook_grace_ends_stopped_on_tick() {
    let mut h = stopped_during_grace();
    h.clock.advance(Duration::from_millis(500));
    h.input(Input::Tick);
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
    assert_eq!(h.rt("pay-qa").status.error, None);
}

#[test]
fn stop_during_end_hook_grace_ends_stopped_on_late_end_hook() {
    let mut h = stopped_during_grace();
    h.clock.advance(Duration::from_millis(600));
    h.input(hook("pay-qa", "tok-1", "session-end"));
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
    assert_eq!(h.rt("pay-qa").status.error, None);
}

#[test]
fn stop_with_restart_during_end_hook_grace_relaunches() {
    let mut h = stopped_during_grace();
    h.rt_mut("pay-qa").restart_after_stop = true;
    h.clock.advance(Duration::from_millis(500));
    h.input(Input::Tick);
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    let starts = h
        .calls()
        .iter()
        .filter(|c| matches!(c, RuntimeCall::Start(..)))
        .count();
    assert_eq!(starts, 2);
}

#[test]
fn old_token_hook_after_relaunch_is_ignored() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.input(started("pay-qa", "@1"));
    h.force_errored("pay-qa", "exited unexpectedly (code 1)", Some("@1"));
    h.start("pay-qa");
    assert_eq!(
        h.rt("pay-qa").token.as_ref().map(|t| t.0.as_str()),
        Some("tok-2")
    );
    let before = h.rt("pay-qa").status.clone();
    assert_eq!(h.input(hook("pay-qa", "tok-1", "busy")), vec![]);
    assert_eq!(h.rt("pay-qa").status, before);
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
}

#[test]
fn window_reported_after_start_timeout_is_recorded_then_killed_by_stop() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.clock.advance(Duration::from_secs(120));
    h.input(Input::Tick);
    assert_eq!(h.state("pay-qa"), AgentState::Errored);
    h.input(started("pay-qa", "@9"));
    assert_eq!(h.rt("pay-qa").window, Some(WindowId("@9".into())));
    h.send(stop("pay-qa"));
    assert!(
        h.calls()
            .contains(&RuntimeCall::Kill(n("pay-qa"), WindowId("@9".into())))
    );
}

#[test]
fn window_reported_after_start_timeout_is_killed_by_relaunch() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.clock.advance(Duration::from_secs(120));
    h.input(Input::Tick);
    h.input(started("pay-qa", "@9"));
    h.start("pay-qa");
    assert!(
        h.calls()
            .contains(&RuntimeCall::Kill(n("pay-qa"), WindowId("@9".into())))
    );
}

#[test]
fn window_reported_for_a_stopped_agent_is_killed() {
    let mut h = Harness::new();
    h.input(started("pay-qa", "@9"));
    assert_eq!(
        h.calls(),
        vec![RuntimeCall::Kill(n("pay-qa"), WindowId("@9".into()))]
    );
    assert_eq!(h.rt("pay-qa").window, None);
}
