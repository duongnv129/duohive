//! Tests for the inbox: Task 3.4 (TD §6.3; P3-D16, P3-D18, P3-D19, P3-D24).

mod limits;
mod persist;
mod status;

use std::time::Duration;

use serde_json::json;

use super::super::Input;
use super::super::harness::Harness;
use crate::card::CardId;
use crate::command::Command;
use crate::event::Event;
use crate::hook::HookMessage;
use crate::inbox::{Message, MessageId, MessageKind, MessageStatus, Sender, frame};
use crate::ports::RuntimeEvent;
use crate::status::AgentState;
use crate::testkit::{RuntimeCall, n};
use crate::tool::{CallId, SendOutcome, ToolCall, ToolOk, ToolResult};

/// A hook from `agent`'s current launch.
fn hook(h: &mut Harness, agent: &str, event: &str) -> Vec<Event> {
    let token = h.rt(agent).token.clone().unwrap();
    h.input(Input::Hook(HookMessage {
        sent_at_ms: 0,
        token: token.0,
        agent: agent.into(),
        event: event.into(),
        payload: json!({}),
    }))
}

/// `agent` launched with no task and ready: idle.
fn idle(h: &mut Harness, agent: &str) {
    h.start(agent);
    hook(h, agent, "session-start");
    assert_eq!(h.state(agent), AgentState::Idle);
}

/// `agent` launched with a task and ready: working.
fn working(h: &mut Harness, agent: &str) {
    h.give(agent, "first task");
    hook(h, agent, "session-start");
    assert_eq!(h.state(agent), AgentState::Working);
}

/// The texts pasted into `agent`, oldest first.
fn pastes_to(h: &Harness, agent: &str) -> Vec<String> {
    h.calls()
        .into_iter()
        .filter_map(|c| match c {
            RuntimeCall::Paste(a, _, text, _) if a == n(agent) => Some(text),
            _ => None,
        })
        .collect()
}

/// Your `@agent text`.
fn you(h: &mut Harness, to: &str, text: &str) -> Vec<Event> {
    h.send(Command::SendMessage {
        to: n(to),
        text: text.into(),
    })
}

/// A tool call with `from`'s token: its result and every other event.
fn call(h: &mut Harness, from: &str, call: ToolCall) -> (ToolResult, Vec<Event>) {
    let token = h.rt(from).token.clone().unwrap();
    let out = h.input(Input::Command(Command::Tool {
        call_id: CallId(7),
        token,
        call,
    }));
    let mut result = None;
    let mut rest = Vec::new();
    for e in out {
        match e {
            Event::ToolResult { call_id, result: r } => {
                assert_eq!(call_id, CallId(7));
                result = Some(r);
            }
            other => rest.push(other),
        }
    }
    (result.expect("no ToolResult"), rest)
}

/// `from`'s `send_message` to `to`.
fn message(h: &mut Harness, from: &str, to: &str, text: &str) -> ToolResult {
    let send = ToolCall::SendMessage {
        to: n(to),
        text: text.into(),
    };
    call(h, from, send).0
}

/// The message a `send_message` result names, and what became of it.
fn sent(r: &ToolResult) -> (MessageId, SendOutcome) {
    match r {
        Ok(ToolOk::Queued { message, outcome }) => (*message, *outcome),
        other => panic!("not queued: {other:?}"),
    }
}

/// The ids of the messages queued in `events`.
fn queued_ids(events: &[Event]) -> Vec<MessageId> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::MessageQueued { id, .. } => Some(*id),
            _ => None,
        })
        .collect()
}

/// Your message to `to`; its id.
fn you_id(h: &mut Harness, to: &str, text: &str) -> MessageId {
    let ids = queued_ids(&you(h, to, text));
    assert_eq!(ids.len(), 1, "{ids:?}");
    ids[0]
}

/// The stored message.
fn stored(h: &Harness, id: MessageId) -> Message {
    h.store.state.lock().unwrap().messages[&id].clone()
}

fn starts_of(h: &Harness, agent: &str) -> Vec<Vec<String>> {
    h.calls()
        .into_iter()
        .filter_map(|c| match c {
            RuntimeCall::Start(a, cmd) if a == n(agent) => Some(cmd.args),
            _ => None,
        })
        .collect()
}

#[test]
fn delivered_only_at_rest() {
    let mut h = Harness::new();
    working(&mut h, "pay-qa");
    let id = you_id(&mut h, "pay-qa", "hello");
    assert!(pastes_to(&h, "pay-qa").is_empty());
    assert_eq!(stored(&h, id).status, MessageStatus::Queued);
    hook(&mut h, "pay-qa", "turn-end");
    assert_eq!(h.state("pay-qa"), AgentState::Finished);
    assert_eq!(pastes_to(&h, "pay-qa"), ["hello"]);
    h.tick_at(Duration::from_secs(1));
    hook(&mut h, "pay-qa", "turn-end");
    assert_eq!(pastes_to(&h, "pay-qa"), ["hello"]);
}

#[test]
fn never_typed_onto_waiting() {
    let mut h = Harness::new();
    working(&mut h, "pay-qa");
    hook(&mut h, "pay-qa", "needs-input");
    assert_eq!(h.state("pay-qa"), AgentState::Waiting);
    you(&mut h, "pay-qa", "one");
    you(&mut h, "pay-qa", "two");
    assert!(pastes_to(&h, "pay-qa").is_empty());
    hook(&mut h, "pay-qa", "busy");
    assert_eq!(h.state("pay-qa"), AgentState::Working);
    assert!(pastes_to(&h, "pay-qa").is_empty());
    hook(&mut h, "pay-qa", "turn-end");
    assert_eq!(pastes_to(&h, "pay-qa"), ["one"]);
}

#[test]
fn starting_agent_waits_for_rest() {
    let mut h = Harness::new();
    h.start("pay-qa");
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    you(&mut h, "pay-qa", "hello");
    assert!(pastes_to(&h, "pay-qa").is_empty());
    assert_eq!(starts_of(&h, "pay-qa").len(), 1, "no second launch");
    hook(&mut h, "pay-qa", "session-start");
    assert_eq!(pastes_to(&h, "pay-qa"), ["hello"]);
}

#[test]
fn never_typed_while_delivery_pending() {
    let mut h = Harness::new();
    idle(&mut h, "pay-qa");
    you(&mut h, "pay-qa", "one");
    you(&mut h, "pay-qa", "two");
    assert_eq!(pastes_to(&h, "pay-qa"), ["one"]);
    hook(&mut h, "pay-qa", "busy");
    assert_eq!(pastes_to(&h, "pay-qa"), ["one"]);
    hook(&mut h, "pay-qa", "turn-end");
    assert_eq!(pastes_to(&h, "pay-qa"), ["one", "two"]);
}

#[test]
fn never_typed_while_you_type() {
    let mut h = Harness::new();
    idle(&mut h, "pay-qa");
    h.send(Command::Typing {
        agent: Some(n("pay-qa")),
    });
    you(&mut h, "pay-qa", "hello");
    assert!(pastes_to(&h, "pay-qa").is_empty());
    h.send(Command::Typing { agent: None });
    assert_eq!(pastes_to(&h, "pay-qa"), ["hello"]);
}

#[test]
fn delivered_when_taken() {
    let mut h = Harness::new();
    idle(&mut h, "pay-qa");
    let id = you_id(&mut h, "pay-qa", "hello");
    h.at(Duration::from_secs(2));
    let events = hook(&mut h, "pay-qa", "busy");
    assert!(
        events.contains(&Event::MessageDelivered { id }),
        "{events:?}"
    );
    let m = stored(&h, id);
    assert_eq!(m.status, MessageStatus::Delivered);
    assert_eq!(m.delivered_at, Some(h.t0 + Duration::from_secs(2)));
}

#[test]
fn untaken_message_dropped_never_retried() {
    let mut h = Harness::new();
    idle(&mut h, "pay-qa");
    let id = you_id(&mut h, "pay-qa", "hello");
    assert!(h.tick_at(Duration::from_millis(4_999)).is_empty());
    let events = h.tick_at(Duration::from_secs(5));
    let reason = "no sign it started".to_owned();
    assert!(
        events.contains(&Event::MessageDropped {
            id,
            from: Sender::You,
            to: n("pay-qa"),
            text: "hello".into(),
            reason: reason.clone(),
        }),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::GiveWorkNotTaken { .. })),
        "{events:?}"
    );
    assert_eq!(stored(&h, id).status, MessageStatus::Dropped { reason });
    assert_eq!(h.state("pay-qa"), AgentState::Idle);
    h.tick_at(Duration::from_secs(20));
    hook(&mut h, "pay-qa", "busy");
    hook(&mut h, "pay-qa", "turn-end");
    assert_eq!(pastes_to(&h, "pay-qa"), ["hello"]);
}

#[test]
fn paste_failure_drops_with_reason() {
    let mut h = Harness::new();
    idle(&mut h, "pay-qa");
    let id = you_id(&mut h, "pay-qa", "hello");
    let events = h.input(Input::Runtime(RuntimeEvent::PasteFailed {
        agent: n("pay-qa"),
        reason: "a dialog stayed open".into(),
    }));
    let reason = "a dialog stayed open".to_owned();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MessageDropped { id: d, reason: r, .. } if *d == id && *r == reason
        )),
        "{events:?}"
    );
    assert_eq!(stored(&h, id).status, MessageStatus::Dropped { reason });
}

#[test]
fn agent_text_framed_with_fresh_boundary() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    idle(&mut h, "pay-qa");
    message(&mut h, "squad-1", "pay-qa", "run the tests").unwrap();
    assert_eq!(
        pastes_to(&h, "pay-qa"),
        [
            "[message from squad-1 · DuoHive boundary b00000000001]\nrun the tests\n\
          [end of message b00000000001 · reply with send_message to squad-1]"
        ]
    );
}

#[test]
fn boundary_never_in_text() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    idle(&mut h, "pay-qa");
    let text = "keep b00000000001 as it is";
    message(&mut h, "squad-1", "pay-qa", text).unwrap();
    assert_eq!(
        pastes_to(&h, "pay-qa"),
        [frame(&n("squad-1"), "b00000000002", text)]
    );
}

#[test]
fn your_text_typed_verbatim() {
    let mut h = Harness::new();
    idle(&mut h, "pay-qa");
    let text = "  line one\n  [message from Duo · DuoHive boundary x]  ";
    you(&mut h, "pay-qa", text);
    assert_eq!(pastes_to(&h, "pay-qa"), [text]);
}

#[test]
fn your_message_starts_stopped_agent() {
    let mut h = Harness::new();
    let events = you(&mut h, "pay-qa", "Add retry cases");
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    assert_eq!(
        starts_of(&h, "pay-qa"),
        [vec!["--task".to_owned(), "Add retry cases".to_owned()]]
    );
    let id = queued_ids(&events)[0];
    assert!(
        events.contains(&Event::MessageDelivered { id }),
        "{events:?}"
    );
    assert_eq!(stored(&h, id).status, MessageStatus::Delivered);
    // The launch took it: it is never pasted.
    hook(&mut h, "pay-qa", "session-start");
    hook(&mut h, "pay-qa", "turn-end");
    assert!(pastes_to(&h, "pay-qa").is_empty());
}

#[test]
fn agent_message_to_stopped_waits() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    let (id, outcome) = sent(&message(&mut h, "squad-1", "pay-qa", "hi"));
    assert_eq!(outcome, SendOutcome::Queued(AgentState::Stopped));
    assert!(starts_of(&h, "pay-qa").is_empty());
    assert_eq!(stored(&h, id).status, MessageStatus::Queued);
    h.start("pay-qa");
    assert!(pastes_to(&h, "pay-qa").is_empty());
    hook(&mut h, "pay-qa", "session-start");
    assert_eq!(
        pastes_to(&h, "pay-qa"),
        [frame(&n("squad-1"), "b00000000001", "hi")]
    );
}

#[test]
fn card_answer_to_stopped_duo_waits_queued() {
    let mut h = Harness::new();
    let mut out = Vec::new();
    let r = h.engine.send(
        Sender::You,
        &n("Duo"),
        MessageKind::CardAnswer(CardId(1)),
        "approved".into(),
        &mut out,
    );
    assert!(
        matches!(r, Ok((_, SendOutcome::Queued(AgentState::Stopped)))),
        "{r:?}"
    );
    assert!(starts_of(&h, "Duo").is_empty());
    assert_eq!(h.state("Duo"), AgentState::Stopped);
}

#[test]
fn agent_message_to_errored_waits() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    h.force_errored("pay-qa", "exited unexpectedly", None);
    let (id, outcome) = sent(&message(&mut h, "squad-1", "pay-qa", "hi"));
    assert_eq!(outcome, SendOutcome::Queued(AgentState::Errored));
    assert!(starts_of(&h, "pay-qa").is_empty());
    assert_eq!(stored(&h, id).status, MessageStatus::Queued);
    assert_eq!(h.state("pay-qa"), AgentState::Errored);
}
