//! Tests for the CLI session id a launch's hooks report (V11-D4).

use serde_json::json;

use super::super::Input;
use super::super::harness::Harness;
use crate::hook::HookMessage;
use crate::testkit::n;

fn hook_with(h: &mut Harness, event: &str, payload: serde_json::Value) {
    let token = h.rt("pay-qa").token.clone().unwrap();
    h.input(Input::Hook(HookMessage {
        sent_at_ms: 0,
        token: token.0,
        agent: "pay-qa".into(),
        event: event.into(),
        payload,
    }));
}

fn stored(h: &Harness) -> Option<String> {
    h.store.state.lock().unwrap().sessions[&n("pay-qa")]
        .cli_session_id
        .clone()
}

#[test]
fn first_hook_records_cli_session_id() {
    let mut h = Harness::new();
    h.start("pay-qa");
    hook_with(&mut h, "session-start", json!({"session_id": "abc-123"}));
    assert_eq!(h.rt("pay-qa").cli_session_id.as_deref(), Some("abc-123"));
    assert_eq!(stored(&h).as_deref(), Some("abc-123"));
}

#[test]
fn later_hooks_do_not_overwrite_it() {
    let mut h = Harness::new();
    h.start("pay-qa");
    hook_with(&mut h, "session-start", json!({"session_id": "first"}));
    hook_with(&mut h, "busy", json!({"session_id": "second"}));
    assert_eq!(h.rt("pay-qa").cli_session_id.as_deref(), Some("first"));
    assert_eq!(stored(&h).as_deref(), Some("first"));
}

#[test]
fn id_with_odd_characters_is_ignored() {
    let mut h = Harness::new();
    h.start("pay-qa");
    for bad in ["", "a b", "../x", "a;b", &"x".repeat(129)] {
        hook_with(&mut h, "busy", json!({"session_id": bad}));
        assert_eq!(h.rt("pay-qa").cli_session_id, None, "{bad:?}");
    }
    hook_with(&mut h, "busy", json!({"session_id": "ok_1-A"}));
    assert_eq!(h.rt("pay-qa").cli_session_id.as_deref(), Some("ok_1-A"));
}

#[test]
fn save_row_keeps_cli_session_id() {
    let mut h = Harness::new();
    h.start("pay-qa");
    hook_with(&mut h, "session-start", json!({"session_id": "keep"}));
    hook_with(&mut h, "turn-end", json!({}));
    hook_with(&mut h, "busy", json!({}));
    assert_eq!(stored(&h).as_deref(), Some("keep"));
}

#[test]
fn new_launch_clears_the_id() {
    let mut h = Harness::new();
    h.start("pay-qa");
    hook_with(&mut h, "session-start", json!({"session_id": "old"}));
    h.force("pay-qa", crate::status::AgentState::Stopped);
    h.start("pay-qa");
    assert_eq!(h.rt("pay-qa").cli_session_id, None);
}

#[test]
fn stale_token_hook_cannot_record_a_session_id() {
    let mut h = Harness::new();
    h.start("pay-qa");
    let old = h.rt("pay-qa").token.clone().unwrap();
    h.force("pay-qa", crate::status::AgentState::Stopped);
    h.start("pay-qa");
    assert_ne!(h.rt("pay-qa").token.clone().unwrap(), old);
    h.input(Input::Hook(HookMessage {
        sent_at_ms: 0,
        token: old.0,
        agent: "pay-qa".into(),
        event: "busy".into(),
        payload: json!({"session_id": "stale"}),
    }));
    assert_eq!(h.rt("pay-qa").cli_session_id, None);
    assert_eq!(stored(&h), None);
}
