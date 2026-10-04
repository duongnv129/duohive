//! Tests for hooks that arrive out of order: an older hook never moves the agent.

use serde_json::json;

use super::super::Input;
use super::super::harness::Harness;
use crate::event::Event;
use crate::hook::HookMessage;
use crate::launch::WindowId;
use crate::ports::RuntimeEvent;
use crate::status::AgentState;
use crate::testkit::n;

fn hook_at(token: &str, event: &str, sent_at_ms: u64, payload: serde_json::Value) -> Input {
    Input::Hook(HookMessage {
        sent_at_ms,
        token: token.into(),
        agent: "pay-qa".into(),
        event: event.into(),
        payload,
    })
}

/// `pay-qa` launched (token `tok-1`) in window `@1`, then put in `state`.
fn running(state: AgentState) -> Harness {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.input(Input::Runtime(RuntimeEvent::Started {
        agent: n("pay-qa"),
        window: WindowId("@1".into()),
    }));
    h.force("pay-qa", state);
    h
}

#[test]
fn older_hook_does_not_move_the_agent() {
    let mut h = running(AgentState::Idle);
    h.input(hook_at("tok-1", "busy", 2, json!({})));
    assert_eq!(h.state("pay-qa"), AgentState::Working);
    h.input(hook_at(
        "tok-1",
        "needs-input",
        1,
        json!({"prompt": "edit a.txt"}),
    ));
    assert_eq!(h.state("pay-qa"), AgentState::Working);
}

#[test]
fn older_turn_end_still_records_its_reply() {
    let mut h = running(AgentState::Working);
    h.input(hook_at("tok-1", "needs-input", 3, json!({})));
    assert_eq!(h.state("pay-qa"), AgentState::Waiting);
    let events = h.input(hook_at(
        "tok-1",
        "turn-end",
        2,
        json!({"last_reply": "done"}),
    ));
    assert_eq!(h.state("pay-qa"), AgentState::Waiting);
    assert_eq!(h.rt("pay-qa").last_reply.as_deref(), Some("done"));
    assert!(events.iter().any(|e| matches!(e,
        Event::LastReplyChanged { agent, reply } if *agent == n("pay-qa")
            && reply.as_deref() == Some("done"))));
}

#[test]
fn hook_with_unknown_send_time_applies() {
    let mut h = running(AgentState::Idle);
    h.input(hook_at("tok-1", "busy", 2, json!({})));
    h.input(hook_at("tok-1", "needs-input", 0, json!({})));
    assert_eq!(h.state("pay-qa"), AgentState::Waiting);
}

#[test]
fn relaunch_forgets_the_last_send_time() {
    let mut h = running(AgentState::Idle);
    h.input(hook_at("tok-1", "busy", 5, json!({})));
    h.input(Input::Runtime(RuntimeEvent::PaneDied {
        agent: n("pay-qa"),
        window: WindowId("@1".into()),
        status: Some(1),
    }));
    assert_eq!(h.state("pay-qa"), AgentState::Errored);
    h.give("pay-qa", "again");
    h.input(Input::Runtime(RuntimeEvent::Started {
        agent: n("pay-qa"),
        window: WindowId("@2".into()),
    }));
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    h.input(hook_at("tok-2", "busy", 1, json!({})));
    assert_eq!(h.state("pay-qa"), AgentState::Working);
}
