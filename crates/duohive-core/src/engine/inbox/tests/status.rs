//! Statuses, outcomes, `message_status`, refusals and the `✉ N` count.

use serde_json::json;

use super::*;
use crate::tool::{ErrorCode, parse_call};

fn status_of(h: &mut Harness, from: &str, id: MessageId) -> ToolResult {
    call(h, from, ToolCall::MessageStatus { id }).0
}

#[test]
fn answered_when_recipient_writes_back() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    idle(&mut h, "pay-qa");
    let (id, outcome) = sent(&message(&mut h, "squad-1", "pay-qa", "which tests fail?"));
    assert_eq!(outcome, SendOutcome::TypedIn);
    hook(&mut h, "pay-qa", "busy");
    assert_eq!(stored(&h, id).status, MessageStatus::Delivered);
    message(&mut h, "pay-qa", "squad-1", "two of them").unwrap();
    assert_eq!(stored(&h, id).status, MessageStatus::Answered);
}

#[test]
fn message_status_only_for_sender() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    idle(&mut h, "squad-2");
    let (id, _) = sent(&message(&mut h, "squad-1", "pay-qa", "hi"));
    let r = status_of(&mut h, "squad-2", id);
    assert_eq!(r.unwrap_err().code, ErrorCode::UnknownMessage);
    assert!(status_of(&mut h, "squad-1", id).is_ok());
    let r = status_of(&mut h, "squad-1", MessageId(99));
    assert_eq!(r.unwrap_err().code, ErrorCode::UnknownMessage);
}

#[test]
fn message_status_says_what_it_waits_on() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    let (id, _) = sent(&message(&mut h, "squad-1", "pay-qa", "hi"));
    let expect = |reason: &str| {
        Ok(ToolOk::Status {
            message: id,
            to: n("pay-qa"),
            status: MessageStatus::Queued,
            reason: Some(reason.to_owned()),
        })
    };
    assert_eq!(
        status_of(&mut h, "squad-1", id),
        expect("pay-qa is stopped")
    );
    h.force_errored("pay-qa", "exited unexpectedly", None);
    assert_eq!(
        status_of(&mut h, "squad-1", id),
        expect("pay-qa is errored")
    );
    h.force("pay-qa", AgentState::Working);
    assert_eq!(
        status_of(&mut h, "squad-1", id),
        expect("pay-qa is working")
    );
}

#[test]
fn send_outcome_by_recipient_state() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    idle(&mut h, "pay-qa");
    let (_, outcome) = sent(&message(&mut h, "squad-1", "pay-qa", "a"));
    assert_eq!(outcome, SendOutcome::TypedIn);
    working(&mut h, "squad-2");
    let (_, outcome) = sent(&message(&mut h, "squad-1", "squad-2", "b"));
    assert_eq!(outcome, SendOutcome::Queued(AgentState::Working));
    let mut out = Vec::new();
    let r = h.engine.send(
        Sender::You,
        &n("squad-3"),
        MessageKind::Text,
        "c".into(),
        &mut out,
    );
    assert!(matches!(r, Ok((_, SendOutcome::Started))), "{r:?}");
    let (_, outcome) = sent(&message(&mut h, "squad-1", "auth-service", "d"));
    assert_eq!(outcome, SendOutcome::Queued(AgentState::Stopped));
}

#[test]
fn send_message_sender_is_token_not_payload() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    idle(&mut h, "pay-qa");
    let forged = parse_call(
        "send_message",
        &json!({"to": "pay-qa", "text": "hi", "from": "Duo"}),
    )
    .unwrap();
    let (r, _) = call(&mut h, "squad-1", forged);
    let (id, _) = sent(&r);
    assert_eq!(stored(&h, id).from, Sender::Agent(n("squad-1")));
    assert!(
        pastes_to(&h, "pay-qa")[0].starts_with("[message from squad-1 "),
        "{:?}",
        pastes_to(&h, "pay-qa")
    );
}

#[test]
fn your_send_refused_for_unknown_agent() {
    let mut h = Harness::new();
    let events = you(&mut h, "nobody", "hi");
    assert_eq!(
        events,
        [Event::MessageRefused {
            to: n("nobody"),
            text: "hi".into(),
            reason: "no agent named nobody".into(),
        }]
    );
    let events = you(&mut h, "pay-qa", "  ");
    assert_eq!(
        events,
        [Event::MessageRefused {
            to: n("pay-qa"),
            text: "  ".into(),
            reason: "the message is empty".into(),
        }]
    );
    assert!(h.store.state.lock().unwrap().messages.is_empty());
}

#[test]
fn snapshot_lists_your_queued() {
    let mut h = Harness::new();
    working(&mut h, "pay-qa");
    idle(&mut h, "squad-1");
    let first = you_id(&mut h, "pay-qa", "one");
    let second = you_id(&mut h, "pay-qa", "two");
    message(&mut h, "squad-1", "pay-qa", "three").unwrap();
    let snap = h.engine.snapshot();
    assert_eq!(snap.your_queued, [first, second]);
    assert_eq!(snap.agents[&n("pay-qa")].inbox, 3);
    hook(&mut h, "pay-qa", "turn-end");
    hook(&mut h, "pay-qa", "busy");
    assert_eq!(h.engine.snapshot().your_queued, [second]);
}

#[test]
fn inbox_count_published() {
    let mut h = Harness::new();
    working(&mut h, "pay-qa");
    let mut events = you(&mut h, "pay-qa", "one");
    events.extend(you(&mut h, "pay-qa", "two"));
    for event in ["turn-end", "busy", "turn-end", "busy"] {
        events.extend(hook(&mut h, "pay-qa", event));
    }
    let counts: Vec<u32> = events
        .iter()
        .filter_map(|e| match e {
            Event::InboxChanged { agent, waiting } if *agent == n("pay-qa") => Some(*waiting),
            _ => None,
        })
        .collect();
    assert_eq!(counts, [1, 2, 1, 0]);
}
