//! Tests for a failed turn (P2-D24): the agent is errored with the reason,
//! while its CLI session is still up in the pane.

use std::collections::HashMap;
use std::path::PathBuf;

use super::super::Input;
use super::super::harness::{Harness, Setup};
use crate::alert::AlertKind;
use crate::command::Command;
use crate::hook::HookMessage;
use crate::launch::WindowId;
use crate::ports::{GitEvent, RuntimeEvent};
use crate::status::AgentState;
use crate::testkit::{RuntimeCall, n};
use crate::validate::DirStatus;
use crate::worktree::{BaseRef, RunIn, TaskId, WorktreeState};

const REASON: &str = "turn failed: model not found (400)";

fn hook_at(agent: &str, token: &str, event: &str, sent_at_ms: u64) -> Input {
    Input::Hook(HookMessage {
        sent_at_ms,
        token: token.into(),
        agent: agent.into(),
        event: event.into(),
        payload: serde_json::json!({"reason": REASON}),
    })
}

fn hook(agent: &str, token: &str, event: &str) -> Input {
    hook_at(agent, token, event, 0)
}

fn started(agent: &str, window: &str) -> Input {
    Input::Runtime(RuntimeEvent::Started {
        agent: n(agent),
        window: WindowId(window.into()),
    })
}

fn window_event(agent: &str, window: &str, dead: bool) -> Input {
    let (agent, window) = (n(agent), WindowId(window.into()));
    Input::Runtime(if dead {
        RuntimeEvent::PaneDied {
            agent,
            window,
            status: Some(0),
        }
    } else {
        RuntimeEvent::WindowGone { agent, window }
    })
}

/// pay-qa working on a task in window `@1`.
fn working() -> Harness {
    let mut h = Harness::new();
    h.give("pay-qa", "Add retry cases");
    h.input(started("pay-qa", "@1"));
    h.input(hook("pay-qa", "tok-1", "session-start"));
    assert_eq!(h.state("pay-qa"), AgentState::Working);
    h
}

/// pay-qa errored by a failed turn, its pane still up in `@1`.
fn failed() -> Harness {
    let mut h = working();
    h.input(hook("pay-qa", "tok-1", "turn-failed"));
    assert_eq!(h.state("pay-qa"), AgentState::Errored);
    h
}

fn kills(h: &Harness) -> Vec<WindowId> {
    h.calls()
        .into_iter()
        .filter_map(|c| match c {
            RuntimeCall::Kill(_, w) => Some(w),
            _ => None,
        })
        .collect()
}

fn starts(h: &Harness) -> usize {
    h.calls()
        .iter()
        .filter(|c| matches!(c, RuntimeCall::Start(..)))
        .count()
}

#[test]
fn failed_turn_errors_with_the_reason_and_keeps_the_window() {
    let h = failed();
    let rt = h.rt("pay-qa");
    assert_eq!(rt.status.error.as_deref(), Some(REASON));
    assert_eq!(rt.window, Some(WindowId("@1".into())));
    assert!(kills(&h).is_empty(), "{:?}", h.calls());
    let st = h.store.state.lock().unwrap();
    let row = &st.sessions[&n("pay-qa")];
    assert_eq!(row.status.state, AgentState::Errored);
    assert_eq!(row.window, Some(WindowId("@1".into())));
}

#[test]
fn failed_turn_alerts_needs_you() {
    let h = failed();
    let kinds: Vec<_> = h.notified().iter().map(|a| a.kind).collect();
    assert_eq!(kinds, [AlertKind::NeedsYou]);
}

#[test]
fn failed_turn_errors_every_running_state() {
    for from in [
        AgentState::Starting,
        AgentState::Waiting,
        AgentState::Finished,
        AgentState::Idle,
    ] {
        let mut h = working();
        h.force("pay-qa", from);
        h.input(hook("pay-qa", "tok-1", "turn-failed"));
        assert_eq!(h.state("pay-qa"), AgentState::Errored, "{from:?}");
        assert_eq!(h.rt("pay-qa").status.error.as_deref(), Some(REASON));
    }
}

#[test]
fn failed_turn_for_a_stopped_agent_is_ignored() {
    let mut h = Harness::new();
    assert!(h.input(hook("pay-qa", "tok-1", "turn-failed")).is_empty());
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
}

#[test]
fn a_stale_failed_turn_moves_nothing() {
    let mut h = working();
    h.input(hook_at("pay-qa", "tok-1", "busy", 20));
    h.input(hook_at("pay-qa", "tok-1", "turn-failed", 10));
    assert_eq!(h.state("pay-qa"), AgentState::Working);
}

#[test]
fn hooks_after_the_failure_leave_it_errored() {
    // The session is still up: a turn typed into the pane is not taken as
    // recovery; `s` or a fresh launch clears the error.
    let mut h = failed();
    for event in ["busy", "needs-input", "turn-end", "session-start"] {
        h.input(hook("pay-qa", "tok-1", event));
        assert_eq!(h.state("pay-qa"), AgentState::Errored, "{event}");
    }
    assert_eq!(h.rt("pay-qa").status.error.as_deref(), Some(REASON));
}

#[test]
fn screen_reads_leave_it_errored() {
    let mut h = failed();
    for text in ["BUSY", "ASK", "REST"] {
        h.input(Input::Runtime(RuntimeEvent::Screen {
            agent: n("pay-qa"),
            text: text.into(),
        }));
        assert_eq!(h.state("pay-qa"), AgentState::Errored, "{text}");
    }
}

#[test]
fn stop_kills_the_live_pane_and_its_late_hooks_and_exit_are_ignored() {
    let mut h = failed();
    h.send(Command::StopAgent { agent: n("pay-qa") });
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
    assert_eq!(kills(&h), [WindowId("@1".into())]);
    assert_eq!(h.rt("pay-qa").window, None);
    // The plugin's cleanup still sends session-end, and the window then goes.
    assert!(h.input(hook("pay-qa", "tok-1", "session-end")).is_empty());
    assert!(h.input(window_event("pay-qa", "@1", false)).is_empty());
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
}

#[test]
fn give_work_kills_the_live_pane_then_launches_fresh() {
    let mut h = failed();
    h.give("pay-qa", "Try again");
    assert_eq!(kills(&h), [WindowId("@1".into())]);
    assert_eq!(starts(&h), 2);
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    assert_eq!(h.rt("pay-qa").status.error, None);
    h.input(started("pay-qa", "@2"));
    // The old session's end hook carries the old token; its window's end is
    // not the current window's.
    h.input(hook("pay-qa", "tok-1", "session-end"));
    h.input(window_event("pay-qa", "@1", true));
    h.input(window_event("pay-qa", "@1", false));
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    assert_eq!(h.rt("pay-qa").window, Some(WindowId("@2".into())));
    h.input(hook("pay-qa", "tok-2", "session-start"));
    assert_eq!(h.state("pay-qa"), AgentState::Working);
}

#[test]
fn start_kills_the_live_pane_then_launches_fresh() {
    let mut h = failed();
    h.start("pay-qa");
    assert_eq!(kills(&h), [WindowId("@1".into())]);
    assert_eq!(starts(&h), 2);
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
}

#[test]
fn the_pane_ending_on_its_own_keeps_it_errored_until_stopped() {
    // `/exit` typed into the errored agent's pane: its exit is not the
    // running session's, so the error and the recorded window stay.
    let mut h = failed();
    assert!(h.input(window_event("pay-qa", "@1", true)).is_empty());
    assert_eq!(h.state("pay-qa"), AgentState::Errored);
    assert_eq!(h.rt("pay-qa").status.error.as_deref(), Some(REASON));
    h.send(Command::StopAgent { agent: n("pay-qa") });
    assert_eq!(kills(&h), [WindowId("@1".into())]);
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
}

#[test]
fn its_worktree_stays_in_use_while_the_pane_is_up() {
    const REPO: &str = "/home/me/code/pay-qa";
    const ID: &str = "add-retry-cases";
    let mut h = Setup {
        dirs: HashMap::from([(PathBuf::from(REPO), DirStatus::GitRepo)]),
        ..Setup::default()
    }
    .build();
    h.give_in("pay-qa", "Add retry cases", RunIn::NewWorktree);
    h.input(Input::Git(GitEvent::Created {
        agent: n("pay-qa"),
        task: TaskId(ID.into()),
        result: Ok((BaseRef("origin/main".into()), "abc123".into())),
    }));
    h.input(started("pay-qa", "@1"));
    h.input(hook("pay-qa", "tok-1", "session-start"));
    h.input(hook("pay-qa", "tok-1", "turn-failed"));
    assert_eq!(h.state("pay-qa"), AgentState::Errored);
    h.send(Command::CheckWorktrees);
    let st = h.store.state.lock().unwrap();
    let row = &st.worktrees[&(n("pay-qa"), TaskId(ID.into()))];
    assert_eq!(row.state, WorktreeState::InUse);
}
