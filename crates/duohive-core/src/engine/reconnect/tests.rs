//! Tests for reconnect at start-up: Task 15 (Review Focus 3; D9, D22, D26).

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::super::Input;
use super::super::harness::{Harness, Setup, T0, test_org};
use crate::event::{Event, Snapshot};
use crate::hook::HookMessage;
use crate::launch::{Token, WindowId};
use crate::org::Org;
use crate::ports::{LiveWindow, OrgFileError, RuntimeEvent, SessionRow};
use crate::status::{AgentState, Status};
use crate::testkit::{RuntimeCall, n};
use crate::time::Millis;
use crate::validate::DirStatus;

/// The token restored rows carry.
fn token(agent: &str) -> String {
    format!("tok-{agent}")
}

/// A session row for `agent` in `state` since `since`, on `window`.
fn row(agent: &str, state: AgentState, window: Option<&str>, since: Millis) -> SessionRow {
    SessionRow {
        agent: n(agent),
        token: Some(Token(token(agent))),
        window: window.map(|w| WindowId(w.into())),
        status: Status {
            state,
            quiet: false,
            quiet_since: None,
            from_screen: false,
            since,
            error: None,
            waiting_on: None,
        },
        last_reply: Some("earlier reply".into()),
        task_at_launch: true,
        saw_end_hook: false,
        last_task: Some("fix the login".into()),
        worktree: None,
    }
}

fn put(h: &Harness, row: SessionRow) {
    let mut st = h.store.state.lock().unwrap();
    st.sessions.insert(row.agent.clone(), row);
}

fn queue(h: &Harness, agent: &str, event: &str, payload: serde_json::Value) {
    h.store
        .state
        .lock()
        .unwrap()
        .pending
        .push(message(agent, event, payload));
}

fn message(agent: &str, event: &str, payload: serde_json::Value) -> HookMessage {
    HookMessage {
        sent_at_ms: 0,
        token: token(agent),
        agent: agent.into(),
        event: event.into(),
        payload,
    }
}

fn live(agent: &str, window: &str, activity_secs: u64) -> LiveWindow {
    LiveWindow {
        agent: n(agent),
        window: WindowId(window.into()),
        dead: None,
        activity_secs,
    }
}

fn dead(agent: &str, window: &str, status: Option<i32>) -> LiveWindow {
    LiveWindow {
        agent: n(agent),
        window: WindowId(window.into()),
        dead: Some(status),
        activity_secs: 0,
    }
}

fn inventory(h: &mut Harness, windows: Vec<LiveWindow>) -> Vec<Event> {
    h.input(Input::Runtime(RuntimeEvent::Inventory(windows)))
}

/// The `Snapshot` `start_up` emits; panics when there is none.
fn snapshot_of(ev: &[Event]) -> Snapshot {
    ev.iter()
        .find_map(|e| match e {
            Event::Snapshot(s) => Some((**s).clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no Snapshot in {ev:?}"))
}

fn deleted(h: &Harness) -> Vec<String> {
    let st = h.store.state.lock().unwrap();
    st.deleted_sessions
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn has_row(h: &Harness, agent: &str) -> bool {
    h.store
        .state
        .lock()
        .unwrap()
        .sessions
        .contains_key(&n(agent))
}

fn marked(h: &Harness) -> Vec<Org> {
    h.store.state.lock().unwrap().marked_last_good.clone()
}

fn kills(h: &Harness) -> Vec<RuntimeCall> {
    h.calls()
        .into_iter()
        .filter(|c| matches!(c, RuntimeCall::Kill(..)))
        .collect()
}

/// The states `agent` moved through in `ev`.
fn moves(ev: &[Event], agent: &str) -> Vec<AgentState> {
    ev.iter()
        .filter_map(|e| match e {
            Event::AgentStatusChanged { agent: a, status } if *a == n(agent) => Some(status.state),
            _ => None,
        })
        .collect()
}

const TWO_MIN: Duration = Duration::from_secs(120);

#[test]
fn reconnect_restores_live_sessions() {
    let mut h = Harness::new();
    let since = Millis(T0.0 - 120_000);
    put(
        &h,
        row("auth-service", AgentState::Working, Some("@1"), since),
    );
    let snap = snapshot_of(&h.engine.start_up());
    assert_eq!(
        snap.agents[&n("auth-service")].status.state,
        AgentState::Working
    );
    assert_eq!(
        snap.agents[&n("auth-service")].last_reply.as_deref(),
        Some("earlier reply")
    );
    assert!(h.calls().contains(&RuntimeCall::Inventory));
    h.at(TWO_MIN);
    let ev = inventory(&mut h, vec![live("auth-service", "@1", 900)]);
    assert_eq!(moves(&ev, "auth-service"), vec![]);
    let rt = h.rt("auth-service");
    assert_eq!(rt.status.state, AgentState::Working);
    assert_eq!(rt.status.since, since);
    assert_eq!(rt.last_activity_secs, Some(900));
    assert_eq!(rt.window, Some(WindowId("@1".into())));
    assert_eq!(rt.token, Some(Token(token("auth-service"))));
    assert_eq!(rt.last_task.as_deref(), Some("fix the login"));
    assert!(kills(&h).is_empty());
}

#[test]
fn reconnect_dead_pane_applies_exit_rules() {
    let mut h = Harness::new();
    put(&h, row("auth-service", AgentState::Working, Some("@1"), T0));
    h.engine.start_up();
    inventory(&mut h, vec![dead("auth-service", "@1", Some(1))]);
    let status = &h.rt("auth-service").status;
    assert_eq!(status.state, AgentState::Errored);
    assert_eq!(
        status.error.as_deref(),
        Some("exited unexpectedly (code 1)")
    );
    assert_eq!(
        kills(&h),
        vec![RuntimeCall::Kill(n("auth-service"), WindowId("@1".into()))]
    );
}

#[test]
fn reconnect_missing_window_marks_stopped() {
    let mut h = Harness::new();
    put(&h, row("auth-service", AgentState::Working, Some("@1"), T0));
    h.engine.start_up();
    let ev = inventory(&mut h, vec![]);
    assert_eq!(moves(&ev, "auth-service"), vec![AgentState::Stopped]);
    assert_eq!(h.rt("auth-service").status.error, None);
    assert!(!has_row(&h, "auth-service"));
    assert!(deleted(&h).contains(&"auth-service".to_owned()));
    assert!(h.engine.needs_you().is_empty());
}

#[test]
fn reconnect_replays_pending_hooks_in_order() {
    let mut h = Harness::new();
    put(&h, row("auth-service", AgentState::Idle, Some("@1"), T0));
    queue(&h, "auth-service", "busy", serde_json::json!({}));
    queue(
        &h,
        "auth-service",
        "turn-end",
        serde_json::json!({"last_reply": "all green"}),
    );
    h.engine.start_up();
    let ev = inventory(&mut h, vec![live("auth-service", "@1", 900)]);
    assert_eq!(
        moves(&ev, "auth-service"),
        vec![AgentState::Working, AgentState::Finished]
    );
    assert_eq!(
        h.rt("auth-service").last_reply.as_deref(),
        Some("all green")
    );
    assert!(h.store.state.lock().unwrap().pending.is_empty());
}

#[test]
fn live_hooks_wait_for_the_replay() {
    let mut h = Harness::new();
    put(&h, row("squad-1", AgentState::Idle, Some("@3"), T0));
    queue(&h, "squad-1", "busy", serde_json::json!({}));
    h.engine.start_up();
    let early = h.input(Input::Hook(message(
        "squad-1",
        "turn-end",
        serde_json::json!({}),
    )));
    assert_eq!(early, vec![]);
    assert_eq!(h.state("squad-1"), AgentState::Idle);
    let ev = inventory(&mut h, vec![live("squad-1", "@3", 900)]);
    assert_eq!(
        moves(&ev, "squad-1"),
        vec![AgentState::Working, AgentState::Finished]
    );
    assert_eq!(h.state("squad-1"), AgentState::Finished);
    // After Reconnected, hooks apply as they arrive.
    let later = h.input(Input::Hook(message(
        "squad-1",
        "busy",
        serde_json::json!({}),
    )));
    assert_eq!(moves(&later, "squad-1"), vec![AgentState::Working]);
}

#[test]
fn reconnect_clean_exit_while_closed_is_stopped() {
    let mut h = Harness::new();
    assert!(h.claude.coverage.session_end);
    put(&h, row("auth-service", AgentState::Working, Some("@1"), T0));
    queue(&h, "auth-service", "turn-end", serde_json::json!({}));
    queue(&h, "auth-service", "session-end", serde_json::json!({}));
    h.engine.start_up();
    let ev = inventory(&mut h, vec![dead("auth-service", "@1", Some(0))]);
    assert_eq!(
        moves(&ev, "auth-service"),
        vec![AgentState::Finished, AgentState::Stopped]
    );
    assert_eq!(h.rt("auth-service").status.error, None);
    assert!(!has_row(&h, "auth-service"));
}

#[test]
fn reconnect_does_not_mark_quiet_immediately() {
    let mut h = Harness::new();
    let ten_min_ago = Millis(T0.0 - 600_000);
    put(
        &h,
        row("auth-service", AgentState::Working, Some("@1"), ten_min_ago),
    );
    h.engine.start_up();
    inventory(
        &mut h,
        vec![live("auth-service", "@1", ten_min_ago.0 / 1000)],
    );
    h.tick_at(Duration::from_secs(1));
    assert!(!h.rt("auth-service").status.quiet);
    h.tick_at(Duration::from_secs(300));
    let status = &h.rt("auth-service").status;
    assert!(status.quiet);
    assert_eq!(status.quiet_since, Some(T0));
}

#[test]
fn start_up_does_not_probe_directories() {
    let mut h = Setup {
        dirs: HashMap::from([("/home/me/code/auth".into(), DirStatus::Missing)]),
        ..Setup::default()
    }
    .build();
    put(&h, row("auth-service", AgentState::Working, Some("@1"), T0));
    let snap = snapshot_of(&h.engine.start_up());
    inventory(&mut h, vec![live("auth-service", "@1", 900)]);
    assert_eq!(h.dir_probes.load(Ordering::SeqCst), 0);
    assert_eq!(snap.agents[&n("auth-service")].problem, None);
    assert_eq!(h.engine.snapshot().agents[&n("auth-service")].problem, None);
}

#[test]
fn reconnect_keeps_errored_agents_errored() {
    let mut h = Harness::new();
    let mut errored = row("auth-service", AgentState::Errored, Some("@1"), T0);
    errored.status.error = Some("exited unexpectedly (code 1)".into());
    put(&h, errored);
    h.engine.start_up();
    let ev = inventory(&mut h, vec![]);
    let status = &h.rt("auth-service").status;
    assert_eq!(status.state, AgentState::Errored);
    assert_eq!(
        status.error.as_deref(),
        Some("exited unexpectedly (code 1)")
    );
    assert!(has_row(&h, "auth-service"));
    let needs: Vec<_> = h.engine.needs_you().into_iter().map(|i| i.agent).collect();
    assert_eq!(needs, vec![n("auth-service")]);
    assert!(ev.contains(&Event::NeedsYouChanged(h.engine.needs_you())));
}

#[test]
fn reconnect_takes_the_window_the_inventory_reports() {
    // The row was saved before `Started` arrived: no window recorded.
    let mut h = Harness::new();
    put(&h, row("auth-service", AgentState::Starting, None, T0));
    h.engine.start_up();
    inventory(&mut h, vec![live("auth-service", "@4", 900)]);
    assert_eq!(h.rt("auth-service").window, Some(WindowId("@4".into())));
    let saved = h.store.state.lock().unwrap().sessions[&n("auth-service")].clone();
    assert_eq!(saved.window, Some(WindowId("@4".into())));
}

#[test]
fn restored_starting_agent_still_times_out() {
    let mut h = Harness::new();
    put(
        &h,
        row("auth-service", AgentState::Starting, Some("@1"), T0),
    );
    h.engine.start_up();
    inventory(&mut h, vec![live("auth-service", "@1", 900)]);
    h.tick_at(Duration::from_secs(120));
    let status = &h.rt("auth-service").status;
    assert_eq!(status.state, AgentState::Errored);
    assert_eq!(status.error.as_deref(), Some("not ready within 120s"));
}

#[test]
fn reconnecting_count_then_reconnected() {
    let mut h = Harness::new();
    put(&h, row("auth-service", AgentState::Working, Some("@1"), T0));
    put(&h, row("squad-2", AgentState::Finished, Some("@2"), T0));
    put(&h, row("pay-qa", AgentState::Stopped, None, T0));
    let snap = snapshot_of(&h.engine.start_up());
    assert_eq!(snap.reconnecting, 2);
    assert_eq!(h.engine.snapshot().reconnecting, 2);
    let ev = inventory(
        &mut h,
        vec![live("auth-service", "@1", 900), live("squad-2", "@2", 900)],
    );
    assert_eq!(ev.last(), Some(&Event::Reconnected));
    assert!(matches!(ev[ev.len() - 2], Event::NeedsYouChanged(_)));
    assert_eq!(h.engine.snapshot().reconnecting, 0);
    // A later inventory is not a reconnect.
    assert_eq!(inventory(&mut h, vec![]), vec![]);
    assert_eq!(h.state("auth-service"), AgentState::Working);
}

#[test]
fn unknown_windows_are_left_alone() {
    let mut h = Harness::new();
    h.engine.start_up();
    let ev = inventory(
        &mut h,
        vec![
            live("squad-3", "@7", 900),
            live("stranger", "@8", 900),
            dead("squad-2", "@9", Some(1)),
        ],
    );
    assert!(kills(&h).is_empty(), "{:?}", h.calls());
    assert!(!h.calls().iter().any(|c| matches!(c, RuntimeCall::Stop(..))));
    assert_eq!(h.state("squad-3"), AgentState::Stopped);
    assert_eq!(h.state("squad-2"), AgentState::Stopped);
    assert_eq!(ev.last(), Some(&Event::Reconnected));
}

#[test]
fn session_for_deleted_agent_is_dropped() {
    let mut h = Harness::new();
    put(&h, row("gone-agent", AgentState::Working, Some("@5"), T0));
    let snap = snapshot_of(&h.engine.start_up());
    assert!(!snap.agents.contains_key(&n("gone-agent")));
    inventory(&mut h, vec![]);
    assert_eq!(deleted(&h), vec!["gone-agent".to_owned()]);
    assert!(!h.engine.snapshot().agents.contains_key(&n("gone-agent")));
    assert_eq!(h.engine.org, test_org());
    assert_eq!(marked(&h), vec![test_org()]);
}

#[test]
fn hooks_for_a_dropped_agent_are_dropped() {
    let mut h = Harness::new();
    put(&h, row("auth-service", AgentState::Working, Some("@1"), T0));
    queue(&h, "auth-service", "turn-end", serde_json::json!({}));
    h.engine.start_up();
    let ev = inventory(&mut h, vec![]);
    assert_eq!(moves(&ev, "auth-service"), vec![AgentState::Stopped]);
}

#[test]
fn restored_stopped_rows_are_deleted() {
    // Stopped means no row: a stale one goes at start-up, token and all.
    let mut h = Harness::new();
    put(&h, row("pay-qa", AgentState::Stopped, None, T0));
    put(&h, row("gone-agent", AgentState::Stopped, None, T0));
    h.engine.start_up();
    assert!(!has_row(&h, "pay-qa"));
    assert!(!has_row(&h, "gone-agent"));
    assert_eq!(h.rt("pay-qa").token, None);
    inventory(&mut h, vec![live("pay-qa", "@6", 900)]);
    assert!(
        !has_row(&h, "pay-qa"),
        "a stopped agent's row is never saved"
    );
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
}

#[test]
fn connection_lost_before_the_inventory_ends_the_reconnect() {
    let mut h = Harness::new();
    put(&h, row("auth-service", AgentState::Working, Some("@1"), T0));
    put(&h, row("gone-agent", AgentState::Working, Some("@5"), T0));
    queue(&h, "auth-service", "turn-end", serde_json::json!({}));
    h.engine.start_up();
    let early = h.input(Input::Hook(message(
        "auth-service",
        "busy",
        serde_json::json!({}),
    )));
    assert_eq!(early, vec![]);
    let ev = h.input(Input::Runtime(RuntimeEvent::ConnectionLost {
        reason: "server exited".into(),
    }));
    // Queued hooks, then the buffered live one, then the loss.
    assert_eq!(
        moves(&ev, "auth-service"),
        vec![
            AgentState::Finished,
            AgentState::Working,
            AgentState::Errored
        ]
    );
    assert_eq!(
        h.rt("auth-service").status.error.as_deref(),
        Some("tmux server exited")
    );
    assert_eq!(ev.last(), Some(&Event::Reconnected));
    assert_eq!(h.engine.snapshot().reconnecting, 0);
    // The server took every window with it: the orphan's row goes.
    assert_eq!(deleted(&h), vec!["gone-agent".to_owned()]);
    assert_eq!(marked(&h), vec![test_org()]);
    // Reconnect is over: a late inventory is ignored.
    assert_eq!(
        inventory(&mut h, vec![live("auth-service", "@1", 900)]),
        vec![]
    );
    assert_eq!(h.state("auth-service"), AgentState::Errored);
}

mod org_switch;
