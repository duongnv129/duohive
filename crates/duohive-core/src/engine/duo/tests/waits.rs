//! `wait_for_agent` (P3-D23), and Duo's stop or relaunch dropping its waits.

use std::time::Duration;

use serde_json::json;

use super::*;
use crate::tool::WaitOutcome;

fn waited(agent: &str, outcome: WaitOutcome, state: AgentState, reply: Option<&str>) -> ToolOk {
    ToolOk::Waited {
        agent: n(agent),
        outcome,
        state,
        last_reply: reply.map(str::to_owned),
    }
}

/// `agent`'s window ends.
fn window_gone(h: &mut Harness, agent: &str) -> Vec<Event> {
    h.input(Input::Runtime(RuntimeEvent::WindowGone {
        agent: n(agent),
        window: WindowId(format!("@{agent}")),
    }))
}

#[test]
fn wait_returns_at_once_when_at_rest() {
    let mut h = with_duo();
    idle(&mut h, "squad-1");
    let (results, events) = tool(&mut h, 5, wait("squad-1"));
    assert_eq!(
        only(&results, 5).unwrap(),
        waited("squad-1", WaitOutcome::AtRest, AgentState::Idle, None)
    );
    assert_eq!(
        actions(&events),
        [("wait for squad-1".into(), "idle".into())]
    );
}

#[test]
fn wait_on_stopped_agent_answers_stopped() {
    let mut h = with_duo();
    let (results, events) = tool(&mut h, 5, wait("pay-qa"));
    assert_eq!(
        only(&results, 5).unwrap(),
        waited("pay-qa", WaitOutcome::Stopped, AgentState::Stopped, None)
    );
    assert_eq!(
        actions(&events),
        [("wait for pay-qa".into(), "stopped".into())]
    );
    h.force_errored("squad-1", "boom", None);
    assert_eq!(
        answer(&mut h, wait("squad-1")).unwrap(),
        waited("squad-1", WaitOutcome::Errored, AgentState::Errored, None)
    );
}

#[test]
fn wait_ends_on_turn_end() {
    let mut h = with_duo();
    working(&mut h, "billing-api");
    let (results, _) = tool(&mut h, 5, wait("billing-api"));
    assert!(results.is_empty(), "{results:?}");
    let (results, events) = split(hook_with(
        &mut h,
        "billing-api",
        "turn-end",
        json!({"last_reply": "fixed it"}),
    ));
    assert_eq!(
        only(&results, 5).unwrap(),
        waited(
            "billing-api",
            WaitOutcome::TurnEnded,
            AgentState::Finished,
            Some("fixed it")
        )
    );
    assert_eq!(
        actions(&events),
        [("wait for billing-api".into(), "finished".into())]
    );
}

#[test]
fn wait_times_out() {
    let mut h = with_duo();
    working(&mut h, "billing-api");
    let (results, _) = tool(&mut h, 5, wait("billing-api"));
    assert!(results.is_empty(), "{results:?}");
    let (results, _) = split(h.tick_at(Duration::from_secs(599)));
    assert!(results.is_empty(), "{results:?}");
    let (results, events) = split(h.tick_at(Duration::from_secs(600)));
    assert_eq!(
        only(&results, 5).unwrap(),
        waited(
            "billing-api",
            WaitOutcome::TimedOut,
            AgentState::Working,
            None
        )
    );
    assert_eq!(
        actions(&events),
        [("wait for billing-api".into(), "timed out after 10m".into())]
    );
}

#[test]
fn wait_ends_when_target_stops() {
    let mut h = with_duo();
    working(&mut h, "billing-api");
    let (results, _) = tool(&mut h, 5, wait("billing-api"));
    assert!(results.is_empty(), "{results:?}");
    h.send(Command::StopAgent {
        agent: n("billing-api"),
    });
    let (results, _) = split(window_gone(&mut h, "billing-api"));
    assert_eq!(h.state("billing-api"), AgentState::Stopped);
    assert_eq!(
        only(&results, 5).unwrap(),
        waited(
            "billing-api",
            WaitOutcome::Stopped,
            AgentState::Stopped,
            None
        )
    );

    // An error ends it too.
    working(&mut h, "squad-1");
    let (results, _) = tool(&mut h, 6, wait("squad-1"));
    assert!(results.is_empty(), "{results:?}");
    let (results, _) = split(hook_with(
        &mut h,
        "squad-1",
        "turn-failed",
        json!({"reason": "rate limited"}),
    ));
    assert_eq!(
        only(&results, 6).unwrap(),
        waited("squad-1", WaitOutcome::Errored, AgentState::Errored, None)
    );
}

#[test]
fn duo_restart_drops_its_waits() {
    let mut h = with_duo();
    working(&mut h, "billing-api");
    working(&mut h, "squad-1");
    let (results, _) = tool(&mut h, 5, wait("billing-api"));
    assert!(results.is_empty(), "{results:?}");
    let (results, _) = tool(&mut h, 6, read("squad-1"));
    assert!(results.is_empty(), "{results:?}");
    // Duo stops, then relaunches.
    h.send(Command::StopAgent { agent: n("Duo") });
    window_gone(&mut h, "Duo");
    assert_eq!(h.state("Duo"), AgentState::Stopped);
    idle(&mut h, "Duo");
    let mut out = hook_with(
        &mut h,
        "billing-api",
        "turn-end",
        json!({"last_reply": "fixed it"}),
    );
    out.extend(h.input(Input::Runtime(RuntimeEvent::Screen {
        agent: n("squad-1"),
        text: "BUSY".into(),
    })));
    out.extend(h.tick_at(Duration::from_secs(600)));
    let (results, events) = split(out);
    assert!(results.is_empty(), "{results:?}");
    assert!(actions(&events).is_empty(), "{events:?}");
}

#[test]
fn wait_ends_when_paste_not_taken() {
    let mut h = with_duo();
    idle(&mut h, "squad-1");
    h.give("squad-1", "yours");
    let (results, _) = tool(&mut h, 5, wait("squad-1"));
    assert!(results.is_empty(), "{results:?}");
    // P2-D11: no sign within 5 s; the agent is at rest with nothing pending.
    let (results, events) = split(h.tick_at(Duration::from_secs(5)));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::GiveWorkNotTaken { .. })),
        "{events:?}"
    );
    assert_eq!(
        only(&results, 5).unwrap(),
        waited("squad-1", WaitOutcome::AtRest, AgentState::Idle, None)
    );
}

#[test]
fn wait_ends_when_worktree_creation_fails() {
    let mut h = squad_worktrees();
    let (results, _) = tool(&mut h, 7, start("squad-1", Some(RunIn::NewWorktree)));
    assert!(results.is_empty(), "{results:?}");
    let (results, _) = tool(&mut h, 8, wait("squad-1"));
    assert!(results.is_empty(), "{results:?}");
    let task = creations(&h, "squad-1")[0].clone();
    let failed = Err(GitError::Other("disk full".into()));
    let (results, _) = split(h.input(created("squad-1", task, failed)));
    let wait_result = results
        .iter()
        .find(|(call, _)| *call == CallId(8))
        .map(|(_, r)| r.clone());
    assert_eq!(
        wait_result,
        Some(Ok(waited(
            "squad-1",
            WaitOutcome::Stopped,
            AgentState::Stopped,
            None
        ))),
        "{results:?}"
    );
}
