//! Messages across a restart, and calls queued while DuoHive was closed (P3-D10, P3-D24).

use super::super::super::harness::{Setup, T0};
use super::*;
use crate::launch::{Token, WindowId};
use crate::ports::{LiveWindow, QueuedToolCall, SessionRow};
use crate::status::Status;

/// A restored idle session for `agent` on `window`, holding `token`.
fn idle_row(agent: &str, token: &str, window: &str) -> SessionRow {
    SessionRow {
        agent: n(agent),
        token: Some(Token(token.into())),
        window: Some(WindowId(window.into())),
        status: Status {
            state: AgentState::Idle,
            quiet: false,
            quiet_since: None,
            from_screen: false,
            since: T0,
            error: None,
            waiting_on: None,
        },
        last_reply: None,
        task_at_launch: false,
        saw_end_hook: false,
        last_task: None,
        worktree: None,
        launched_as_duo: false,
    }
}

fn live(agent: &str, window: &str) -> Input {
    Input::Runtime(RuntimeEvent::Inventory(vec![LiveWindow {
        agent: n(agent),
        window: WindowId(window.into()),
        dead: None,
        activity_secs: 0,
    }]))
}

/// A harness whose store holds `squad-1`'s idle session and one queued call.
fn closed_call(token: &str, tool: &str, arguments: serde_json::Value) -> Harness {
    let h = Harness::new();
    {
        let mut st = h.store.state.lock().unwrap();
        st.sessions
            .insert(n("squad-1"), idle_row("squad-1", "tok-squad", "@2"));
        st.queued_tool_calls.push(QueuedToolCall {
            id: 1,
            token: Token(token.into()),
            tool: tool.into(),
            arguments,
            queued_at: T0,
        });
    }
    h
}

fn send_hi() -> serde_json::Value {
    json!({"to": "pay-qa", "text": "hi"})
}

fn inbox_events(events: &[Event]) -> Vec<&Event> {
    events
        .iter()
        .filter(|e| {
            matches!(
                e,
                Event::MessageQueued { .. }
                    | Event::InboxChanged { .. }
                    | Event::ToolResult { .. }
                    | Event::CommandRejected { .. }
            )
        })
        .collect()
}

#[test]
fn persisted_and_replayed_after_restart() {
    let mut first = Harness::new();
    working(&mut first, "pay-qa");
    let one = you_id(&mut first, "pay-qa", "one");
    let two = you_id(&mut first, "pay-qa", "two");
    {
        let mut st = first.store.state.lock().unwrap();
        st.sessions
            .insert(n("pay-qa"), idle_row("pay-qa", "tok-pay", "@1"));
    }
    let mut h = Setup {
        store: first.store.clone(),
        ..Setup::default()
    }
    .build();
    let snap = h.engine.start_up();
    let Some(Event::Snapshot(snap)) = snap.first() else {
        panic!("no snapshot: {snap:?}");
    };
    assert_eq!(snap.agents[&n("pay-qa")].inbox, 2);
    assert_eq!(snap.your_queued, [one, two]);
    assert!(pastes_to(&h, "pay-qa").is_empty());
    let events = h.input(live("pay-qa", "@1"));
    assert!(events.contains(&Event::Reconnected), "{events:?}");
    assert_eq!(pastes_to(&h, "pay-qa"), ["one"]);
    // Ids continue from the highest.
    let three = you_id(&mut h, "pay-qa", "three");
    assert_eq!(three, MessageId(two.0 + 1));
}

#[test]
fn pending_paste_at_shutdown_not_pasted_again() {
    let mut first = Harness::new();
    idle(&mut first, "pay-qa");
    let one = you_id(&mut first, "pay-qa", "one");
    assert_eq!(pastes_to(&first, "pay-qa"), ["one"]);
    // DuoHive closes before the agent is seen to take it.
    {
        let mut st = first.store.state.lock().unwrap();
        st.sessions
            .insert(n("pay-qa"), idle_row("pay-qa", "tok-pay", "@1"));
    }
    let mut h = Setup {
        store: first.store.clone(),
        ..Setup::default()
    }
    .build();
    let snap = h.engine.start_up();
    let Some(Event::Snapshot(snap)) = snap.first() else {
        panic!("no snapshot: {snap:?}");
    };
    assert_eq!(snap.agents[&n("pay-qa")].inbox, 0);
    assert!(snap.your_queued.is_empty(), "{:?}", snap.your_queued);
    let events = h.input(live("pay-qa", "@1"));
    assert!(events.contains(&Event::Reconnected), "{events:?}");
    assert!(pastes_to(&h, "pay-qa").is_empty());
    let reason = "DuoHive closed during delivery".to_owned();
    assert_eq!(stored(&h, one).status, MessageStatus::Dropped { reason });
}

#[test]
fn closed_changes_queue_and_run_on_open() {
    let mut h = closed_call("tok-squad", "send_message", send_hi());
    h.engine.start_up();
    assert!(h.store.state.lock().unwrap().queued_tool_calls.is_empty());
    let events = h.input(live("squad-1", "@2"));
    let reconnected = events
        .iter()
        .position(|e| *e == Event::Reconnected)
        .unwrap();
    let queued = events
        .iter()
        .position(|e| {
            matches!(e, Event::MessageQueued { from: Sender::Agent(a), to, .. }
                if *a == n("squad-1") && *to == n("pay-qa"))
        })
        .unwrap_or_else(|| panic!("no message queued: {events:?}"));
    assert!(reconnected < queued, "{events:?}");
    let id = queued_ids(&events)[0];
    let m = stored(&h, id);
    assert_eq!(m.from, Sender::Agent(n("squad-1")));
    assert_eq!(m.status, MessageStatus::Queued);
    assert_eq!(m.text, "hi");
}

#[test]
fn queued_call_with_unknown_token_dropped_on_open() {
    for (token, tool, args) in [
        ("nope", "send_message", send_hi()),
        ("tok-squad", "send_message", json!({"to": "pay-qa"})),
    ] {
        let mut h = closed_call(token, tool, args);
        h.engine.start_up();
        let events = h.input(live("squad-1", "@2"));
        assert!(events.contains(&Event::Reconnected), "{events:?}");
        assert!(inbox_events(&events).is_empty(), "{events:?}");
        assert!(h.store.state.lock().unwrap().messages.is_empty());
    }
}

#[test]
fn replayed_call_publishes_no_tool_result() {
    let mut h = closed_call("tok-squad", "send_message", send_hi());
    let mut events = h.engine.start_up();
    events.extend(h.input(live("squad-1", "@2")));
    assert_eq!(queued_ids(&events).len(), 1, "{events:?}");
    assert!(
        !events.iter().any(|e| matches!(e, Event::ToolResult { .. })),
        "{events:?}"
    );
}
