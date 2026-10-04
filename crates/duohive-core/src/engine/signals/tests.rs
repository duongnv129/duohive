//! Tests for hook signals, screen reads and exits: Task 12.

use std::time::Duration;

use super::super::Input;
use super::super::harness::{Harness, Setup, T0};
use crate::adapter::HookCoverage;
use crate::cli_kind::CliKind;
use crate::command::Command;
use crate::event::Event;
use crate::hook::HookMessage;
use crate::launch::WindowId;
use crate::ports::{Clock, RuntimeEvent};
use crate::status::AgentState;
use crate::testkit::{RuntimeCall, StubAdapter, n};
use crate::time::Millis;

const FULL: HookCoverage = HookCoverage {
    ready: true,
    busy: true,
    needs_input: true,
    turn_end: true,
    session_end: true,
    last_reply: true,
};

fn hook_with(agent: &str, token: &str, event: &str, payload: serde_json::Value) -> Input {
    Input::Hook(HookMessage {
        token: token.into(),
        agent: agent.into(),
        event: event.into(),
        payload,
    })
}

fn hook(agent: &str, token: &str, event: &str) -> Input {
    hook_with(agent, token, event, serde_json::json!({}))
}

fn screen(agent: &str, text: &str) -> Input {
    Input::Runtime(RuntimeEvent::Screen {
        agent: n(agent),
        text: text.into(),
    })
}

fn started(agent: &str, window: &str) -> Input {
    Input::Runtime(RuntimeEvent::Started {
        agent: n(agent),
        window: WindowId(window.into()),
    })
}

fn died(agent: &str, window: &str, status: Option<i32>) -> Input {
    Input::Runtime(RuntimeEvent::PaneDied {
        agent: n(agent),
        window: WindowId(window.into()),
        status,
    })
}

fn gone(agent: &str, window: &str) -> Input {
    Input::Runtime(RuntimeEvent::WindowGone {
        agent: n(agent),
        window: WindowId(window.into()),
    })
}

/// A harness whose Claude adapter has `coverage`.
fn with_coverage(coverage: HookCoverage) -> Harness {
    Setup {
        claude: StubAdapter {
            coverage,
            ..StubAdapter::new(CliKind::Claude)
        },
        ..Setup::default()
    }
    .build()
}

/// Launches `agent` (no task) in window `window`, then puts it in `state`.
fn running(h: &mut Harness, agent: &str, window: &str, state: AgentState) {
    h.start(agent);
    h.input(started(agent, window));
    h.force(agent, state);
}

/// `(agent, state, from_screen)` of every status change.
fn moves(events: &[Event]) -> Vec<(String, AgentState, bool)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::AgentStatusChanged { agent, status } => {
                Some((agent.to_string(), status.state, status.from_screen))
            }
            _ => None,
        })
        .collect()
}

fn mv(agent: &str, state: AgentState, from_screen: bool) -> Vec<(String, AgentState, bool)> {
    vec![(agent.to_owned(), state, from_screen)]
}

fn replies(events: &[Event]) -> Vec<Option<String>> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::LastReplyChanged { reply, .. } => Some(reply.clone()),
            _ => None,
        })
        .collect()
}

fn error_of(h: &Harness, agent: &str) -> Option<String> {
    h.rt(agent).status.error.clone()
}

fn has_row(h: &Harness, agent: &str) -> bool {
    h.store
        .state
        .lock()
        .unwrap()
        .sessions
        .contains_key(&n(agent))
}

// ---- hooks ----

#[test]
fn hook_with_wrong_token_is_ignored() {
    let mut h = Harness::new();
    h.start("pay-qa");
    assert_eq!(h.input(hook("pay-qa", "nope", "session-start")), vec![]);
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
}

#[test]
fn hook_for_unknown_agent_is_ignored() {
    let mut h = Harness::new();
    h.start("pay-qa");
    let before: Vec<_> = h
        .engine
        .agents
        .iter()
        .map(|(a, rt)| (a.clone(), rt.status.clone()))
        .collect();
    assert_eq!(h.input(hook("ghost", "tok-1", "busy")), vec![]);
    let after: Vec<_> = h
        .engine
        .agents
        .iter()
        .map(|(a, rt)| (a.clone(), rt.status.clone()))
        .collect();
    assert_eq!(before, after);
}

#[test]
fn hook_for_a_stopped_agent_is_ignored() {
    let mut h = Harness::new();
    assert_eq!(h.input(hook("pay-qa", "tok-1", "busy")), vec![]);
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
}

#[test]
fn ready_without_task_goes_idle() {
    let mut h = Harness::new();
    h.start("pay-qa");
    let ev = h.input(hook("pay-qa", "tok-1", "session-start"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Idle, false));
}

#[test]
fn ready_with_task_goes_working() {
    let mut h = Harness::new();
    h.give("pay-qa", "Add retry cases");
    let ev = h.input(hook("pay-qa", "tok-1", "session-start"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Working, false));
}

#[test]
fn busy_while_starting_goes_working() {
    let mut h = Harness::new();
    h.start("pay-qa");
    let ev = h.input(hook("pay-qa", "tok-1", "busy"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Working, false));
}

#[test]
fn busy_while_working_does_not_move() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    assert_eq!(moves(&h.input(hook("pay-qa", "tok-1", "busy"))), vec![]);
    assert_eq!(h.state("pay-qa"), AgentState::Working);
}

#[test]
fn accepted_hook_records_the_signal_time() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    h.clock.advance(Duration::from_secs(30));
    h.input(hook("pay-qa", "tok-1", "busy"));
    assert_eq!(h.rt("pay-qa").last_signal_at, T0 + Duration::from_secs(30));
    h.clock.advance(Duration::from_secs(30));
    h.input(hook("pay-qa", "nope", "busy"));
    assert_eq!(h.rt("pay-qa").last_signal_at, T0 + Duration::from_secs(30));
}

#[test]
fn hook_clears_quiet() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    {
        let rt = h.rt_mut("pay-qa");
        rt.status.quiet = true;
        rt.status.quiet_since = Some(T0);
    }
    let ev = h.input(hook("pay-qa", "tok-1", "busy"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Working, false));
    assert!(!h.rt("pay-qa").status.quiet);
    assert_eq!(h.rt("pay-qa").status.quiet_since, None);
}

#[test]
fn needs_input_then_busy_returns_to_working() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    let ev = h.input(hook("pay-qa", "tok-1", "needs-input"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Waiting, false));
    let ev = h.input(hook("pay-qa", "tok-1", "busy"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Working, false));
}

#[test]
fn needs_input_keeps_the_prompt() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    h.input(hook_with(
        "pay-qa",
        "tok-1",
        "needs-input",
        serde_json::json!({"prompt": "Run tests?"}),
    ));
    assert_eq!(h.state("pay-qa"), AgentState::Waiting);
    assert_eq!(h.rt("pay-qa").waiting_prompt.as_deref(), Some("Run tests?"));
}

#[test]
fn needs_input_while_starting_goes_waiting() {
    let mut h = Harness::new();
    h.start("pay-qa");
    let ev = h.input(hook("pay-qa", "tok-1", "needs-input"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Waiting, false));
}

#[test]
fn turn_end_after_declined_prompt_finishes() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Waiting);
    let ev = h.input(hook("pay-qa", "tok-1", "turn-end"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Finished, false));
    let duo = h.engine.org.duo.name.to_string();
    running(&mut h, &duo, "@2", AgentState::Waiting);
    let ev = h.input(hook(&duo, "tok-2", "turn-end"));
    assert_eq!(moves(&ev), mv(&duo, AgentState::Idle, false));
}

#[test]
fn hook_busy_at_rest_goes_working() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Finished);
    let ev = h.input(hook("pay-qa", "tok-1", "busy"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Working, false));
    h.force("pay-qa", AgentState::Idle);
    let ev = h.input(hook("pay-qa", "tok-1", "busy"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Working, false));
}

#[test]
fn turn_end_finishes_and_sets_last_reply() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    let ev = h.input(hook_with(
        "pay-qa",
        "tok-1",
        "turn-end",
        serde_json::json!({"last_reply": "Ready for review"}),
    ));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Finished, false));
    assert_eq!(replies(&ev), vec![Some("Ready for review".to_owned())]);
    assert_eq!(
        h.rt("pay-qa").last_reply.as_deref(),
        Some("Ready for review")
    );
    let st = h.store.state.lock().unwrap();
    assert_eq!(
        st.sessions[&n("pay-qa")].last_reply.as_deref(),
        Some("Ready for review")
    );
}

#[test]
fn same_reply_is_not_reported_again() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    h.rt_mut("pay-qa").last_reply = Some("Same".into());
    let ev = h.input(hook_with(
        "pay-qa",
        "tok-1",
        "turn-end",
        serde_json::json!({"last_reply": "Same"}),
    ));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Finished, false));
    assert_eq!(replies(&ev), Vec::<Option<String>>::new());
}

#[test]
fn duo_turn_end_goes_idle() {
    let mut h = Harness::new();
    let duo = h.engine.org.duo.name.to_string();
    running(&mut h, &duo, "@1", AgentState::Working);
    let ev = h.input(hook(&duo, "tok-1", "turn-end"));
    assert_eq!(moves(&ev), mv(&duo, AgentState::Idle, false));
}

// ---- screen reads ----

#[test]
fn screen_busy_at_rest_goes_working_without_busy_hook() {
    let mut h = with_coverage(HookCoverage {
        busy: false,
        ..FULL
    });
    running(&mut h, "squad-2", "@1", AgentState::Finished);
    let ev = h.input(screen("squad-2", "thinking…\nBUSY"));
    assert_eq!(moves(&ev), mv("squad-2", AgentState::Working, true));
    assert!(h.rt("squad-2").status.from_screen);

    let mut h = with_coverage(FULL);
    running(&mut h, "squad-2", "@1", AgentState::Finished);
    assert_eq!(h.input(screen("squad-2", "thinking…\nBUSY")), vec![]);
    assert_eq!(h.state("squad-2"), AgentState::Finished);
}

#[test]
fn screen_needs_input_while_starting_goes_waiting_without_hook() {
    let mut h = with_coverage(HookCoverage {
        needs_input: false,
        ..FULL
    });
    h.start("pay-qa");
    let ev = h.input(screen("pay-qa", "Trust this folder?\nASK"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Waiting, true));
}

#[test]
fn screen_needs_input_while_starting_goes_waiting_even_with_hook() {
    let mut h = with_coverage(FULL);
    h.start("pay-qa");
    let ev = h.input(screen("pay-qa", "Trust this folder?\nASK"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Waiting, true));

    let mut h = with_coverage(FULL);
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    assert_eq!(h.input(screen("pay-qa", "Run tests?\nASK")), vec![]);
    assert_eq!(h.state("pay-qa"), AgentState::Working);
}

#[test]
fn screen_at_rest_while_starting_needs_no_ready_hook() {
    let mut h = with_coverage(HookCoverage {
        ready: false,
        ..FULL
    });
    h.start("pay-qa");
    let ev = h.input(screen("pay-qa", "REST"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Idle, true));

    let mut h = with_coverage(FULL);
    h.start("pay-qa");
    assert_eq!(h.input(screen("pay-qa", "REST")), vec![]);
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
}

#[test]
fn screen_busy_while_starting_needs_no_busy_hook() {
    let mut h = with_coverage(HookCoverage {
        busy: false,
        ..FULL
    });
    h.start("pay-qa");
    let ev = h.input(screen("pay-qa", "BUSY"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Working, true));
}

#[test]
fn screen_read_moves_only_unhooked_transitions_and_marks_tilde() {
    let mut h = with_coverage(HookCoverage {
        needs_input: false,
        ..FULL
    });
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    let ev = h.input(screen("pay-qa", "Run tests?\nASK"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Waiting, true));

    let mut h = with_coverage(FULL);
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    assert_eq!(h.input(screen("pay-qa", "REST")), vec![]);
    assert_eq!(h.state("pay-qa"), AgentState::Working);

    // D2's waiting exception: complete coverage, and the screen still moves it.
    h.force("pay-qa", AgentState::Waiting);
    let ev = h.input(screen("pay-qa", "REST"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Finished, true));
    let duo = h.engine.org.duo.name.to_string();
    running(&mut h, &duo, "@2", AgentState::Waiting);
    let ev = h.input(screen(&duo, "REST"));
    assert_eq!(moves(&ev), mv(&duo, AgentState::Idle, true));

    h.force("pay-qa", AgentState::Waiting);
    let ev = h.input(screen("pay-qa", "BUSY"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Working, true));
}

#[test]
fn unrecognised_screen_does_nothing() {
    let mut h = with_coverage(HookCoverage {
        needs_input: false,
        turn_end: false,
        ..FULL
    });
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    assert_eq!(h.input(screen("pay-qa", "hello")), vec![]);
    assert_eq!(h.input(screen("squad-1", "ASK")), vec![]);
    assert_eq!(h.state("squad-1"), AgentState::Stopped);
}

#[test]
fn hook_move_clears_tilde() {
    let mut h = with_coverage(HookCoverage {
        needs_input: false,
        ..FULL
    });
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    let ev = h.input(screen("pay-qa", "ASK"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Waiting, true));
    let ev = h.input(hook("pay-qa", "tok-1", "busy"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Working, false));
    assert!(!h.rt("pay-qa").status.from_screen);
}

#[test]
fn screen_last_reply_used_only_without_hook_reply() {
    let mut h = with_coverage(HookCoverage {
        last_reply: false,
        turn_end: false,
        ..FULL
    });
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    let ev = h.input(screen("pay-qa", "Done: 3 files\nREST"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Finished, true));
    assert_eq!(replies(&ev), vec![Some("Done: 3 files".to_owned())]);

    let mut h = with_coverage(HookCoverage {
        turn_end: false,
        ..FULL
    });
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    let ev = h.input(screen("pay-qa", "Done: 3 files\nREST"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Finished, true));
    assert_eq!(replies(&ev), Vec::<Option<String>>::new());
}

// ---- exits ----

#[test]
fn normal_exit_with_end_hook_and_zero_is_stopped() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Finished);
    h.input(hook("pay-qa", "tok-1", "session-end"));
    assert_eq!(h.state("pay-qa"), AgentState::Finished);
    let ev = h.input(died("pay-qa", "@1", Some(0)));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Stopped, false));
    assert_eq!(error_of(&h, "pay-qa"), None);
    assert!(!has_row(&h, "pay-qa"));
    assert!(
        h.calls()
            .contains(&RuntimeCall::Kill(n("pay-qa"), WindowId("@1".into())))
    );
}

#[test]
fn exit_without_end_hook_is_errored() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    assert_eq!(h.input(died("pay-qa", "@1", Some(0))), vec![]);
    h.clock.advance(Duration::from_millis(500));
    let ev = h.input(Input::Tick);
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Errored, false));
    assert_eq!(
        error_of(&h, "pay-qa").as_deref(),
        Some("exited unexpectedly (code 0)")
    );
    assert!(has_row(&h, "pay-qa"));
    assert_eq!(h.rt("pay-qa").window, None);
    assert!(
        h.calls()
            .contains(&RuntimeCall::Kill(n("pay-qa"), WindowId("@1".into())))
    );
}

#[test]
fn zero_exit_waits_briefly_for_the_end_hook() {
    let mut h = Harness::new();
    running(&mut h, "auth-service", "@1", AgentState::Working);
    let t: Millis = h.clock.now();
    assert_eq!(h.input(died("auth-service", "@1", Some(0))), vec![]);
    assert_eq!(
        h.engine.next_deadline(),
        Some(t + Duration::from_millis(500))
    );
    h.clock.advance(Duration::from_millis(200));
    let ev = h.input(hook("auth-service", "tok-1", "session-end"));
    assert_eq!(moves(&ev), mv("auth-service", AgentState::Stopped, false));
    assert_eq!(error_of(&h, "auth-service"), None);
    assert!(!has_row(&h, "auth-service"));
    assert_eq!(h.engine.next_deadline(), None);
    h.clock.advance(Duration::from_millis(300));
    assert_eq!(h.input(Input::Tick), vec![]);
}

#[test]
fn tick_before_the_grace_does_nothing() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    h.input(died("pay-qa", "@1", Some(0)));
    h.clock.advance(Duration::from_millis(499));
    assert_eq!(h.input(Input::Tick), vec![]);
    assert_eq!(h.state("pay-qa"), AgentState::Working);
}

#[test]
fn zero_exit_is_stopped_when_cli_has_no_end_hook() {
    let mut h = with_coverage(HookCoverage {
        session_end: false,
        ..FULL
    });
    running(&mut h, "pay-qa", "@1", AgentState::Finished);
    let ev = h.input(died("pay-qa", "@1", Some(0)));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Stopped, false));
    assert!(!has_row(&h, "pay-qa"));

    running(&mut h, "pay-qa", "@2", AgentState::Working);
    let ev = h.input(died("pay-qa", "@2", Some(1)));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Errored, false));
    assert_eq!(
        error_of(&h, "pay-qa").as_deref(),
        Some("exited unexpectedly (code 1)")
    );
}

#[test]
fn nonzero_exit_is_errored_with_code() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    let ev = h.input(died("pay-qa", "@1", Some(1)));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Errored, false));
    assert_eq!(
        error_of(&h, "pay-qa").as_deref(),
        Some("exited unexpectedly (code 1)")
    );
    assert!(
        h.calls()
            .contains(&RuntimeCall::Kill(n("pay-qa"), WindowId("@1".into())))
    );
    let st = h.store.state.lock().unwrap();
    let row = &st.sessions[&n("pay-qa")];
    assert_eq!(row.status.state, AgentState::Errored);
    assert_eq!(row.window, None);
}

#[test]
fn signal_death_is_errored_without_code() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    let ev = h.input(died("pay-qa", "@1", None));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Errored, false));
    assert_eq!(
        error_of(&h, "pay-qa").as_deref(),
        Some("exited unexpectedly")
    );
}

#[test]
fn window_gone_while_running_is_errored() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Idle);
    let ev = h.input(gone("pay-qa", "@1"));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Errored, false));
    assert_eq!(
        error_of(&h, "pay-qa").as_deref(),
        Some("exited unexpectedly")
    );
    assert!(!h.calls().iter().any(|c| matches!(c, RuntimeCall::Kill(..))));
}

#[test]
fn exit_127_while_starting_says_not_found() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.input(started("pay-qa", "@1"));
    let ev = h.input(died("pay-qa", "@1", Some(127)));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Errored, false));
    assert_eq!(
        error_of(&h, "pay-qa").as_deref(),
        Some("claude not found (code 127)")
    );
}

#[test]
fn exit_while_starting_says_exited_at_start() {
    let mut setup = Setup::default();
    setup.org.duo.cli = CliKind::Opencode;
    let mut h = setup.build();
    let duo = h.engine.org.duo.name.to_string();
    h.start(&duo);
    h.input(started(&duo, "@1"));
    let ev = h.input(died(&duo, "@1", Some(2)));
    assert_eq!(moves(&ev), mv(&duo, AgentState::Errored, false));
    assert_eq!(
        error_of(&h, &duo).as_deref(),
        Some("opencode exited at start (code 2)")
    );
}

#[test]
fn signal_death_while_starting_says_exited_at_start() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.input(started("pay-qa", "@1"));
    h.input(died("pay-qa", "@1", None));
    assert_eq!(
        error_of(&h, "pay-qa").as_deref(),
        Some("claude exited at start (killed by a signal)")
    );
}

#[test]
fn zero_exit_while_starting_says_exited_at_start() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.input(started("pay-qa", "@1"));
    h.input(died("pay-qa", "@1", Some(0)));
    assert_eq!(
        error_of(&h, "pay-qa").as_deref(),
        Some("claude exited at start (code 0)")
    );
}

#[test]
fn window_gone_after_stop_is_stopped() {
    let mut h = Harness::new();
    running(&mut h, "auth-service", "@1", AgentState::Working);
    h.send(Command::StopAgent {
        agent: n("auth-service"),
    });
    let ev = h.input(gone("auth-service", "@1"));
    assert_eq!(moves(&ev), mv("auth-service", AgentState::Stopped, false));
    assert_eq!(error_of(&h, "auth-service"), None);
    assert!(!has_row(&h, "auth-service"));
    assert!(!h.rt("auth-service").stopping);
}

#[test]
fn nonzero_exit_after_stop_is_stopped() {
    let mut h = Harness::new();
    running(&mut h, "auth-service", "@1", AgentState::Working);
    h.send(Command::StopAgent {
        agent: n("auth-service"),
    });
    let ev = h.input(died("auth-service", "@1", Some(130)));
    assert_eq!(moves(&ev), mv("auth-service", AgentState::Stopped, false));
}

#[test]
fn stop_with_restart_launches_again_without_task() {
    let mut h = Harness::new();
    h.give("pay-qa", "First task");
    h.input(started("pay-qa", "@1"));
    h.force("pay-qa", AgentState::Working);
    h.send(Command::StopAgent { agent: n("pay-qa") });
    h.rt_mut("pay-qa").restart_after_stop = true;
    let ev = h.input(gone("pay-qa", "@1"));
    assert_eq!(
        moves(&ev),
        vec![
            ("pay-qa".to_owned(), AgentState::Stopped, false),
            ("pay-qa".to_owned(), AgentState::Starting, false),
        ]
    );
    let starts: Vec<_> = h
        .calls()
        .into_iter()
        .filter_map(|c| match c {
            RuntimeCall::Start(_, cmd) => Some(cmd.args),
            _ => None,
        })
        .collect();
    assert_eq!(starts.len(), 2);
    assert!(starts[1].is_empty(), "{:?}", starts[1]);
    assert!(!h.rt("pay-qa").restart_after_stop);
    assert!(!h.rt("pay-qa").task_at_launch);
}

#[test]
fn exits_of_an_old_window_are_ignored() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.input(started("pay-qa", "@3"));
    let ev = h.input(died("pay-qa", "@3", Some(1)));
    assert_eq!(moves(&ev), mv("pay-qa", AgentState::Errored, false));
    assert!(
        h.calls()
            .contains(&RuntimeCall::Kill(n("pay-qa"), WindowId("@3".into())))
    );
    h.give("pay-qa", "Retry");
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    assert_eq!(h.rt("pay-qa").window, None);
    assert_eq!(h.input(gone("pay-qa", "@3")), vec![]);
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    h.input(started("pay-qa", "@4"));
    assert_eq!(h.rt("pay-qa").window, Some(WindowId("@4".into())));
    assert_eq!(h.input(gone("pay-qa", "@3")), vec![]);
    assert_eq!(h.input(died("pay-qa", "@3", Some(1))), vec![]);
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
}

#[test]
fn started_records_the_window_in_the_row() {
    let mut h = Harness::new();
    h.start("pay-qa");
    assert_eq!(h.input(started("pay-qa", "@9")), vec![]);
    let st = h.store.state.lock().unwrap();
    assert_eq!(
        st.sessions[&n("pay-qa")].window,
        Some(WindowId("@9".into()))
    );
}

#[test]
fn connection_lost_errors_running_agents() {
    let mut h = Harness::new();
    running(&mut h, "pay-qa", "@1", AgentState::Working);
    running(&mut h, "squad-1", "@2", AgentState::Waiting);
    let ev = h.input(Input::Runtime(RuntimeEvent::ConnectionLost {
        reason: "server exited unexpectedly".into(),
    }));
    assert_eq!(
        moves(&ev),
        vec![
            ("pay-qa".to_owned(), AgentState::Errored, false),
            ("squad-1".to_owned(), AgentState::Errored, false),
        ]
    );
    for a in ["pay-qa", "squad-1"] {
        assert_eq!(error_of(&h, a).as_deref(), Some("tmux server exited"));
    }
    assert_eq!(h.rt("pay-qa").window, Some(WindowId("@1".into())));
    assert_eq!(h.state("auth-service"), AgentState::Stopped);
    assert_eq!(error_of(&h, "auth-service"), None);
}
