//! Timer edge cases from the Task 13 review: D2's waiting settle read, stops at
//! the start timeout, quiet hand-offs, D20 and relaunch resets.

use std::time::Duration;

use super::super::Input;
use super::super::harness::{Harness, Setup};
use crate::adapter::HookCoverage;
use crate::cli_kind::CliKind;
use crate::command::Command;
use crate::hook::HookMessage;
use crate::launch::WindowId;
use crate::ports::{Clock, RuntimeEvent};
use crate::settings::Settings;
use crate::status::AgentState;
use crate::testkit::{RuntimeCall, StubAdapter, n};

fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

fn hook(agent: &str, token: &str, event: &str) -> Input {
    Input::Hook(HookMessage {
        token: token.into(),
        agent: agent.into(),
        event: event.into(),
        payload: serde_json::json!({}),
    })
}

fn started(agent: &str, window: &str) -> Input {
    Input::Runtime(RuntimeEvent::Started {
        agent: n(agent),
        window: WindowId(window.into()),
    })
}

fn activity(agent: &str, at_secs: u64) -> Input {
    Input::Runtime(RuntimeEvent::Activity {
        agent: n(agent),
        at_secs,
    })
}

fn now_secs(h: &Harness) -> u64 {
    h.clock.now().0 / 1000
}

fn count(h: &Harness, f: impl Fn(&RuntimeCall) -> bool) -> usize {
    h.calls().iter().filter(|c| f(c)).count()
}

fn captures(h: &Harness) -> usize {
    count(h, |c| matches!(c, RuntimeCall::Capture(_)))
}

fn kills(h: &Harness, window: &str) -> usize {
    let w = WindowId(window.into());
    count(h, |c| matches!(c, RuntimeCall::Kill(_, k) if *k == w))
}

fn with_claude(coverage: HookCoverage, startup_prompts: bool) -> Harness {
    Setup {
        claude: StubAdapter {
            coverage,
            startup_prompts,
            ..StubAdapter::new(CliKind::Claude)
        },
        ..Setup::default()
    }
    .build()
}

fn full() -> HookCoverage {
    StubAdapter::new(CliKind::Claude).coverage
}

#[test]
fn settle_reads_a_waiting_agent_even_with_complete_coverage() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.input(started("pay-qa", "@1"));
    h.input(hook("pay-qa", "tok-1", "needs-input"));
    assert_eq!(h.state("pay-qa"), AgentState::Waiting);
    h.at(secs(30));
    let at = now_secs(&h);
    h.input(activity("pay-qa", at));
    h.tick_at(Duration::from_millis(31_500));
    assert_eq!(captures(&h), 1);
    h.input(Input::Runtime(RuntimeEvent::Screen {
        agent: n("pay-qa"),
        text: "No, skipped\nREST".into(),
    }));
    assert_eq!(h.state("pay-qa"), AgentState::Finished);
    assert!(h.rt("pay-qa").status.from_screen);
}

#[test]
fn stop_in_progress_at_the_start_timeout_ends_stopped() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.input(started("pay-qa", "@1"));
    h.send(Command::StopAgent { agent: n("pay-qa") });
    assert_eq!(
        h.engine.next_deadline(),
        Some(h.t0 + secs(120)),
        "the start timeout stays a deadline while stopping"
    );
    h.tick_at(secs(120));
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
    assert_eq!(kills(&h, "@1"), 1);
}

#[test]
fn stop_with_restart_at_the_start_timeout_relaunches() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.send(Command::StopAgent { agent: n("pay-qa") });
    h.rt_mut("pay-qa").restart_after_stop = true;
    h.tick_at(secs(120));
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    assert_eq!(count(&h, |c| matches!(c, RuntimeCall::Start(..))), 2);
    assert!(!h.rt("pay-qa").stopping);
}

#[test]
fn state_change_clears_quiet_and_needs_you_uses_new_since() {
    let mut h = Harness::new();
    h.start("auth-service");
    h.input(hook("auth-service", "tok-1", "busy"));
    h.tick_at(secs(300));
    assert!(h.rt("auth-service").status.quiet);
    h.at(secs(330));
    h.input(hook("auth-service", "tok-1", "needs-input"));
    let rt = h.rt("auth-service");
    assert_eq!(rt.status.state, AgentState::Waiting);
    assert!(!rt.status.quiet);
    assert_eq!(rt.status.quiet_since, None);
    let items = h.engine.needs_you();
    assert_eq!(items.len(), 1);
    assert!(!items[0].quiet);
    assert_eq!(items[0].since, h.t0 + secs(330));
}

#[test]
fn start_timeout_wins_over_a_two_minute_threshold() {
    let mut h = Setup {
        settings: Settings {
            quiet_after_minutes: 2,
            ..Settings::default()
        },
        ..Setup::default()
    }
    .build();
    h.start("pay-qa");
    h.tick_at(secs(119));
    assert!(!h.rt("pay-qa").status.quiet);
    h.tick_at(secs(120));
    let rt = h.rt("pay-qa");
    assert_eq!(rt.status.state, AgentState::Errored);
    assert!(!rt.status.quiet);
    let items = h.engine.needs_you();
    assert_eq!(items.len(), 1);
    assert!(!items[0].quiet);
}

#[test]
fn pre_read_runs_again_after_a_relaunch() {
    let mut h = with_claude(full(), true);
    h.start("pay-qa");
    h.tick_at(secs(115));
    assert_eq!(captures(&h), 1);
    h.tick_at(secs(120));
    assert_eq!(h.state("pay-qa"), AgentState::Errored);
    h.start("pay-qa");
    h.tick_at(secs(234));
    assert_eq!(captures(&h), 1);
    h.tick_at(secs(235));
    assert_eq!(captures(&h), 2);
}

#[test]
fn relaunch_resets_settle_and_activity() {
    let incomplete = HookCoverage {
        busy: false,
        ..full()
    };
    let mut h = with_claude(incomplete, false);
    h.give("auth-service", "Fix it");
    h.input(hook("auth-service", "tok-1", "session-start"));
    h.at(secs(30));
    let at = now_secs(&h);
    h.input(activity("auth-service", at));
    h.force_errored("auth-service", "exited unexpectedly (code 1)", None);
    h.start("auth-service");
    h.tick_at(Duration::from_millis(31_500));
    assert_eq!(captures(&h), 0, "the old launch's settle read is dropped");
    h.input(activity("auth-service", at));
    assert!(h.rt("auth-service").settle_due.is_some());
}

#[test]
fn started_for_a_window_the_timeout_killed_is_ignored() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.input(started("pay-qa", "@1"));
    h.tick_at(secs(120));
    assert_eq!(kills(&h, "@1"), 1);
    h.input(started("pay-qa", "@1"));
    assert_eq!(h.rt("pay-qa").window, None);
    assert_eq!(kills(&h, "@1"), 1);
}
