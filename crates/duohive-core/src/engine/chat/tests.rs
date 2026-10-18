//! Tests for the Duo chat transcript: Task 3.8 (TD §6.5; P3-D14, P3-D26, P3-Q5).

use serde_json::json;

use super::super::Input;
use super::super::harness::{Harness, Setup, T0};
use crate::chat::{ChatEntry, ChatId, ChatKind};
use crate::command::Command;
use crate::event::Event;
use crate::hook::HookMessage;
use crate::inbox::MessageStatus;
use crate::ports::RuntimeEvent;
use crate::status::AgentState;
use crate::testkit::{RuntimeCall, n};

/// A hook from `agent`'s current launch with `payload`.
fn hook_with(h: &mut Harness, agent: &str, event: &str, payload: serde_json::Value) -> Vec<Event> {
    let token = h.rt(agent).token.clone().unwrap();
    h.input(Input::Hook(HookMessage {
        sent_at_ms: 0,
        token: token.0,
        agent: agent.into(),
        event: event.into(),
        payload,
    }))
}

fn hook(h: &mut Harness, agent: &str, event: &str) -> Vec<Event> {
    hook_with(h, agent, event, json!({}))
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

fn chat(h: &mut Harness, text: &str) -> Vec<Event> {
    h.send(Command::ChatToDuo { text: text.into() })
}

/// The chat lines in `events`.
fn lines(events: &[Event]) -> Vec<ChatEntry> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::ChatLine(entry) => Some(entry.clone()),
            _ => None,
        })
        .collect()
}

fn stored_chat(h: &Harness) -> Vec<ChatEntry> {
    h.store.state.lock().unwrap().chat.clone()
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

fn starts_of(h: &Harness, agent: &str) -> Vec<Vec<String>> {
    h.calls()
        .into_iter()
        .filter_map(|c| match c {
            RuntimeCall::Start(a, cmd) if a == n(agent) => Some(cmd.args),
            _ => None,
        })
        .collect()
}

fn entry(id: u64, kind: ChatKind, text: &str) -> ChatEntry {
    ChatEntry {
        id: ChatId(id),
        at: T0,
        kind,
        text: text.into(),
    }
}

#[test]
fn queued_until_duo_at_rest() {
    let mut h = Harness::new();
    working(&mut h, "Duo");
    let events = chat(&mut h, "what's blocked?");
    let id = events
        .iter()
        .find_map(|e| match e {
            Event::MessageQueued { id, to, .. } if *to == n("Duo") => Some(*id),
            _ => None,
        })
        .expect("no message queued");
    let you = entry(1, ChatKind::You { message: Some(id) }, "what's blocked?");
    assert_eq!(lines(&events), std::slice::from_ref(&you));
    assert_eq!(stored_chat(&h), [you]);
    assert_eq!(
        h.store.state.lock().unwrap().messages[&id].status,
        MessageStatus::Queued
    );
    assert!(pastes_to(&h, "Duo").is_empty());
    hook(&mut h, "Duo", "turn-end");
    assert_eq!(h.state("Duo"), AgentState::Idle);
    assert_eq!(pastes_to(&h, "Duo"), ["what's blocked?"]);
}

#[test]
fn chat_to_stopped_duo_starts_it() {
    let mut h = Harness::new();
    let events = chat(&mut h, "Story: let users retry");
    assert_eq!(h.state("Duo"), AgentState::Starting);
    let starts = starts_of(&h, "Duo");
    assert_eq!(starts.len(), 1, "{starts:?}");
    assert!(
        starts[0]
            .windows(2)
            .any(|w| w == ["--task", "Story: let users retry"]),
        "{starts:?}"
    );
    let lines = lines(&events);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(matches!(lines[0].kind, ChatKind::You { message: Some(_) }));
    assert_eq!(lines[0].text, "Story: let users retry");
    // An errored Duo is started the same way.
    let mut h = Harness::new();
    h.force_errored("Duo", "exited unexpectedly", None);
    chat(&mut h, "again");
    assert_eq!(h.state("Duo"), AgentState::Starting);
    assert_eq!(starts_of(&h, "Duo").len(), 1);
}

#[test]
fn reply_becomes_chat_line() {
    let mut h = Harness::new();
    working(&mut h, "Duo");
    let done = json!({"last_reply": "done"});
    let events = hook_with(&mut h, "Duo", "turn-end", done.clone());
    assert_eq!(lines(&events), [entry(1, ChatKind::Duo, "done")]);
    hook(&mut h, "Duo", "busy");
    let events = hook_with(&mut h, "Duo", "turn-end", done);
    assert_eq!(lines(&events), [entry(2, ChatKind::Duo, "done")]);
    assert_eq!(
        stored_chat(&h),
        [
            entry(1, ChatKind::Duo, "done"),
            entry(2, ChatKind::Duo, "done")
        ]
    );
}

#[test]
fn no_reply_no_line() {
    let mut h = Harness::new();
    working(&mut h, "Duo");
    let events = hook(&mut h, "Duo", "turn-end");
    assert_eq!(h.state("Duo"), AgentState::Idle);
    assert!(lines(&events).is_empty(), "{events:?}");
    assert!(stored_chat(&h).is_empty());
    // Another agent's reply is not Duo's.
    working(&mut h, "pay-qa");
    let events = hook_with(&mut h, "pay-qa", "turn-end", json!({"last_reply": "ok"}));
    assert!(lines(&events).is_empty(), "{events:?}");
}

#[test]
fn duo_goes_straight_to_idle() {
    let mut h = Harness::new();
    working(&mut h, "Duo");
    let events = hook_with(&mut h, "Duo", "turn-end", json!({"last_reply": "ok"}));
    assert_eq!(h.state("Duo"), AgentState::Idle);
    assert!(
        !events.iter().any(|e| matches!(
            e,
            Event::AgentStatusChanged { status, .. } if status.state == AgentState::Finished
        )),
        "{events:?}"
    );
}

#[test]
fn duo_waiting_joins_needs_you() {
    let mut h = Harness::new();
    working(&mut h, "Duo");
    hook_with(
        &mut h,
        "Duo",
        "needs-input",
        json!({"prompt": "Allow Bash?"}),
    );
    assert_eq!(h.state("Duo"), AgentState::Waiting);
    let needs_you = h.engine.needs_you();
    assert!(
        needs_you
            .iter()
            .any(|i| i.agent == n("Duo") && i.state == AgentState::Waiting),
        "{needs_you:?}"
    );
    assert!(
        h.notified().iter().any(|a| a.title == "Duo needs you"),
        "{:?}",
        h.notified()
    );
}

#[test]
fn transcript_survives_restart() {
    let mut first = Harness::new();
    working(&mut first, "Duo");
    chat(&mut first, "one");
    hook_with(&mut first, "Duo", "turn-end", json!({"last_reply": "two"}));
    first.send(Command::SendMessage {
        to: n("pay-qa"),
        text: "three".into(),
    });
    let saved = stored_chat(&first);
    assert_eq!(saved.len(), 3, "{saved:?}");
    let mut h = Setup {
        store: first.store.clone(),
        ..Setup::default()
    }
    .build();
    let events = h.engine.start_up();
    let Some(Event::Snapshot(snap)) = events.first() else {
        panic!("no snapshot: {events:?}");
    };
    assert_eq!(snap.chat, saved);
    assert_eq!(h.engine.snapshot().chat, saved);
    // Ids continue from the highest.
    let events = chat(&mut h, "four");
    assert_eq!(lines(&events)[0].id, ChatId(4));

    // With 600 stored, the snapshot holds the last 500.
    let many = Harness::new();
    {
        let mut st = many.store.state.lock().unwrap();
        st.chat = (1..=600)
            .map(|i| entry(i, ChatKind::Duo, &format!("reply {i}")))
            .collect();
    }
    let mut h = Setup {
        store: many.store.clone(),
        ..Setup::default()
    }
    .build();
    h.engine.start_up();
    let chat_now = h.engine.snapshot().chat;
    assert_eq!(chat_now.len(), 500);
    assert_eq!(chat_now[0].id, ChatId(101));
    assert_eq!(chat_now[499].id, ChatId(600));
    // A new entry keeps the snapshot at 500, newest last.
    h.input(Input::Runtime(RuntimeEvent::Inventory(Vec::new())));
    working(&mut h, "Duo");
    hook_with(&mut h, "Duo", "turn-end", json!({"last_reply": "new"}));
    let chat_now = h.engine.snapshot().chat;
    assert_eq!(chat_now.len(), 500);
    assert_eq!(chat_now[0].id, ChatId(102));
    assert_eq!(chat_now[499].id, ChatId(601));
    assert_eq!(stored_chat(&h).len(), 601);
}

/// Your `@to text`: the chat lines it wrote.
fn you(h: &mut Harness, to: &str, text: &str) -> Vec<ChatEntry> {
    lines(&h.send(Command::SendMessage {
        to: n(to),
        text: text.into(),
    }))
}

fn action(id: u64, result: &str, text: &str) -> ChatEntry {
    entry(
        id,
        ChatKind::Action {
            result: result.into(),
        },
        text,
    )
}

#[test]
fn your_message_writes_action_line() {
    let mut h = Harness::new();
    working(&mut h, "pay-qa");
    assert_eq!(
        you(&mut h, "pay-qa", "rerun"),
        [action(1, "queued (pay-qa is working)", "@pay-qa \"rerun\"")]
    );
    idle(&mut h, "squad-1");
    assert_eq!(
        you(&mut h, "squad-1", "hi"),
        [action(2, "typed in", "@squad-1 \"hi\"")]
    );
    assert_eq!(
        you(&mut h, "billing-api", "start on it"),
        [action(3, "started", "@billing-api \"start on it\"")]
    );
    assert!(you(&mut h, "nobody", "hello").is_empty());
    assert!(you(&mut h, "pay-qa", "   ").is_empty());
    assert_eq!(stored_chat(&h).len(), 3);
}

#[test]
fn long_text_cut_to_40() {
    let mut h = Harness::new();
    working(&mut h, "pay-qa");
    let text = "rerun the whole payments suite\nand then tell me what failed";
    let lines = you(&mut h, "pay-qa", text);
    assert_eq!(
        lines[0].text,
        "@pay-qa \"rerun the whole payments suite and then…\""
    );
    assert_eq!(
        lines[0].text.chars().count(),
        "@pay-qa \"\"".chars().count() + 40
    );
}
