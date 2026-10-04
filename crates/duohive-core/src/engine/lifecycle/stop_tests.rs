//! Tests for stopping by state: a waiting agent's prompt is never answered.

use std::time::Duration;

use super::super::Input;
use super::super::harness::Harness;
use crate::command::Command;
use crate::launch::{Key, WindowId};
use crate::ports::RuntimeEvent;
use crate::status::AgentState;
use crate::testkit::{RuntimeCall, n};

/// Launches `agent` in window `window`, puts it in `state` and clears the call log.
fn running(h: &mut Harness, agent: &str, window: &str, state: AgentState) {
    h.start(agent);
    h.input(Input::Runtime(RuntimeEvent::Started {
        agent: n(agent),
        window: WindowId(window.into()),
    }));
    h.force(agent, state);
    h.call_log.lock().unwrap().clear();
}

fn stop(h: &mut Harness, agent: &str) {
    h.send(Command::StopAgent { agent: n(agent) });
}

fn sends_keys(call: &RuntimeCall) -> bool {
    matches!(call, RuntimeCall::Stop(..) | RuntimeCall::Paste(..))
}

#[test]
fn stop_waiting_kills_without_keys() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@4", AgentState::Waiting);
    stop(&mut h, "pay-qa");
    let calls = h.calls();
    assert!(!calls.iter().any(sends_keys), "{calls:?}");
    assert_eq!(
        calls,
        vec![RuntimeCall::Kill(n("pay-qa"), WindowId("@4".into()))]
    );
    assert!(h.rt("pay-qa").stopping);

    h.input(Input::Runtime(RuntimeEvent::WindowGone {
        agent: n("pay-qa"),
        window: WindowId("@4".into()),
    }));
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
    assert_eq!(h.rt("pay-qa").status.error, None);
}

#[test]
fn stop_waiting_with_a_zero_exit_ends_stopped() {
    // D21: the adapter has an end hook, yet a stop's exit never errors.
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@4", AgentState::Waiting);
    stop(&mut h, "pay-qa");
    h.input(Input::Runtime(RuntimeEvent::PaneDied {
        agent: n("pay-qa"),
        window: WindowId("@4".into()),
        status: Some(0),
    }));
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
    assert_eq!(h.rt("pay-qa").status.error, None);
}

#[test]
fn stop_waiting_honours_a_restart() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@4", AgentState::Waiting);
    stop(&mut h, "pay-qa");
    h.rt_mut("pay-qa").restart_after_stop = true;
    h.input(Input::Runtime(RuntimeEvent::WindowGone {
        agent: n("pay-qa"),
        window: WindowId("@4".into()),
    }));
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    assert!(
        h.calls()
            .iter()
            .any(|c| matches!(c, RuntimeCall::Start(a, _) if *a == n("pay-qa")))
    );
}

#[test]
fn stop_working_sends_escape_then_quit_keys() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@4", AgentState::Working);
    stop(&mut h, "pay-qa");
    assert_eq!(
        h.calls(),
        vec![RuntimeCall::Stop(
            n("pay-qa"),
            vec![Key::Escape, Key::Text("/exit".into()), Key::Enter],
            Duration::from_secs(5)
        )]
    );
    h.input(Input::Runtime(RuntimeEvent::PaneDied {
        agent: n("pay-qa"),
        window: WindowId("@4".into()),
        status: Some(0),
    }));
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
    assert_eq!(h.rt("pay-qa").status.error, None);
}
