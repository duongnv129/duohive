//! Tests for quiet, settle reads, the start timeout and Needs you: Task 13.

use std::time::Duration;

use super::super::Input;
use super::super::harness::{Harness, Setup};
use crate::adapter::HookCoverage;
use crate::cli_kind::CliKind;
use crate::event::{Event, NeedsYouItem};
use crate::hook::HookMessage;
use crate::launch::WindowId;
use crate::ports::{Clock, RuntimeEvent};
use crate::settings::Settings;
use crate::status::AgentState;
use crate::testkit::{RuntimeCall, StubAdapter, n};
use crate::time::Millis;

fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

fn ms(m: u64) -> Duration {
    Duration::from_millis(m)
}

fn hook(agent: &str, token: &str, event: &str) -> Input {
    hook_with(agent, token, event, serde_json::json!({}))
}

fn hook_with(agent: &str, token: &str, event: &str, payload: serde_json::Value) -> Input {
    Input::Hook(HookMessage {
        sent_at_ms: 0,
        token: token.into(),
        agent: agent.into(),
        event: event.into(),
        payload,
    })
}

fn started(agent: &str, window: &str) -> Input {
    Input::Runtime(RuntimeEvent::Started {
        agent: n(agent),
        window: WindowId(window.into()),
    })
}

/// Activity on `agent` at the clock's current second.
fn activity_now(h: &Harness, agent: &str) -> Input {
    Input::Runtime(RuntimeEvent::Activity {
        agent: n(agent),
        at_secs: h.clock.now().0 / 1000,
    })
}

/// Launches `agent` (window `@<agent>`) and moves it to working with a busy hook.
fn working(h: &mut Harness, agent: &str) {
    h.start(agent);
    let token = h.rt(agent).token.clone().unwrap().0;
    h.input(started(agent, &format!("@{agent}")));
    h.input(hook(agent, &token, "busy"));
    assert_eq!(h.state(agent), AgentState::Working);
}

fn token(h: &Harness, agent: &str) -> String {
    h.rt(agent).token.clone().unwrap().0
}

fn captures(h: &Harness) -> Vec<String> {
    h.calls()
        .into_iter()
        .filter_map(|c| match c {
            RuntimeCall::Capture(a) => Some(a.to_string()),
            _ => None,
        })
        .collect()
}

fn needs_you_events(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, Event::NeedsYouChanged(_)))
        .count()
}

fn quiet_changes(events: &[Event], agent: &str) -> Vec<(bool, Option<Millis>)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::AgentStatusChanged { agent: a, status } if a.as_str() == agent => {
                Some((status.quiet, status.quiet_since))
            }
            _ => None,
        })
        .collect()
}

fn names(items: &[NeedsYouItem]) -> Vec<String> {
    items.iter().map(|i| i.agent.to_string()).collect()
}

fn incomplete() -> HookCoverage {
    HookCoverage {
        ready: true,
        busy: false,
        needs_input: true,
        turn_end: true,
        session_end: true,
        last_reply: true,
    }
}

fn claude(coverage: HookCoverage, startup_prompts: bool) -> StubAdapter {
    StubAdapter {
        coverage,
        startup_prompts,
        ..StubAdapter::new(CliKind::Claude)
    }
}

/// auth-service working since `t0 + 10 s`, then quiet at `t0 + 5m10s`.
fn quiet_auth_service() -> Harness {
    let mut h = Harness::new();
    h.start("auth-service");
    h.input(started("auth-service", "@1"));
    h.at(secs(10));
    h.input(hook("auth-service", "tok-1", "busy"));
    h.tick_at(secs(310));
    assert!(h.rt("auth-service").status.quiet);
    h
}

#[test]
fn quiet_after_threshold_for_working() {
    let mut h = Harness::new();
    h.start("auth-service");
    h.input(started("auth-service", "@1"));
    h.at(secs(10));
    h.input(hook("auth-service", "tok-1", "busy"));
    let ev = h.tick_at(secs(10 + 299));
    assert!(!h.rt("auth-service").status.quiet);
    assert_eq!(quiet_changes(&ev, "auth-service"), vec![]);
    let ev = h.tick_at(secs(10 + 300));
    let since = h.t0 + secs(10);
    assert_eq!(
        quiet_changes(&ev, "auth-service"),
        vec![(true, Some(since))]
    );
    assert_eq!(h.state("auth-service"), AgentState::Working);
    let items = h.engine.needs_you();
    assert_eq!(names(&items), vec!["auth-service"]);
    assert!(items[0].quiet);
    assert_eq!(items[0].since, since);
    assert_eq!(items[0].state, AgentState::Working);
    let row = &h.store.state.lock().unwrap().sessions[&n("auth-service")];
    assert!(row.status.quiet);
}

#[test]
fn activity_clears_quiet() {
    let mut h = quiet_auth_service();
    h.at(secs(400));
    let act = activity_now(&h, "auth-service");
    let ev = h.input(act);
    assert_eq!(quiet_changes(&ev, "auth-service"), vec![(false, None)]);
    assert_eq!(h.engine.needs_you(), vec![]);
    assert_eq!(needs_you_events(&ev), 1);
    assert_eq!(h.rt("auth-service").last_signal_at, h.t0 + secs(400));
}

#[test]
fn hook_clears_quiet() {
    let mut h = quiet_auth_service();
    let ev = h.input(hook("auth-service", "tok-1", "busy"));
    assert_eq!(quiet_changes(&ev, "auth-service"), vec![(false, None)]);
    assert_eq!(h.engine.needs_you(), vec![]);
}

#[test]
fn waiting_never_goes_quiet() {
    let mut h = Harness::new();
    working(&mut h, "billing-api");
    h.input(hook("billing-api", "tok-1", "needs-input"));
    h.tick_at(secs(600));
    assert!(!h.rt("billing-api").status.quiet);
    let items = h.engine.needs_you();
    assert_eq!(names(&items), vec!["billing-api"]);
    assert_eq!(items[0].state, AgentState::Waiting);
    assert!(!items[0].quiet);
}

#[test]
fn starting_goes_quiet_with_one_minute_threshold() {
    let mut h = Setup {
        settings: Settings {
            quiet_after_minutes: 1,
            ..Settings::default()
        },
        ..Setup::default()
    }
    .build();
    h.start("pay-qa");
    h.tick_at(secs(59));
    assert!(!h.rt("pay-qa").status.quiet);
    h.tick_at(secs(60));
    assert!(h.rt("pay-qa").status.quiet);
    assert_eq!(h.rt("pay-qa").status.quiet_since, Some(h.t0));
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    h.tick_at(secs(119));
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
}

#[test]
fn settle_reads_screen_once_after_activity_stops() {
    let mut h = Setup {
        claude: claude(incomplete(), false),
        ..Setup::default()
    }
    .build();
    h.give("auth-service", "Fix the login bug");
    h.input(hook("auth-service", "tok-1", "session-start"));
    assert_eq!(h.state("auth-service"), AgentState::Working);
    h.at(secs(30));
    let act = activity_now(&h, "auth-service");
    h.input(act);
    h.tick_at(ms(31_400));
    assert_eq!(captures(&h), Vec::<String>::new());
    h.tick_at(ms(31_500));
    assert_eq!(captures(&h), vec!["auth-service"]);
    h.tick_at(secs(40));
    assert_eq!(captures(&h), vec!["auth-service"]);
}

#[test]
fn complete_coverage_never_reads_screen() {
    let mut h = Harness::new();
    working(&mut h, "auth-service");
    h.at(secs(30));
    let act = activity_now(&h, "auth-service");
    h.input(act);
    h.tick_at(ms(31_500));
    h.tick_at(secs(40));
    assert_eq!(captures(&h), Vec::<String>::new());
}

#[test]
fn starting_agent_screen_read_once_before_timeout() {
    let full = StubAdapter::new(CliKind::Claude).coverage;
    let mut h = Setup {
        claude: claude(full, true),
        ..Setup::default()
    }
    .build();
    h.start("pay-qa");
    h.tick_at(secs(114));
    assert_eq!(captures(&h), Vec::<String>::new());
    h.tick_at(secs(115));
    assert_eq!(captures(&h), vec!["pay-qa"]);
    h.tick_at(secs(118));
    assert_eq!(captures(&h), vec!["pay-qa"]);
    h.input(Input::Runtime(RuntimeEvent::Screen {
        agent: n("pay-qa"),
        text: "Trust this folder?\nASK".into(),
    }));
    assert_eq!(h.state("pay-qa"), AgentState::Waiting);
    assert!(h.rt("pay-qa").status.from_screen);
    h.tick_at(secs(120));
    assert_eq!(h.state("pay-qa"), AgentState::Waiting);
    assert_eq!(h.rt("pay-qa").status.error, None);
}

#[test]
fn no_pre_read_without_startup_prompts() {
    let mut h = Setup {
        claude: claude(incomplete(), false),
        ..Setup::default()
    }
    .build();
    h.start("pay-qa");
    h.tick_at(secs(115));
    h.tick_at(secs(118));
    assert_eq!(captures(&h), Vec::<String>::new());
}

#[test]
fn start_timeout_errors_and_kills() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.input(started("pay-qa", "@7"));
    h.tick_at(ms(119_999));
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    h.tick_at(secs(120));
    assert_eq!(h.state("pay-qa"), AgentState::Errored);
    assert_eq!(
        h.rt("pay-qa").status.error.as_deref(),
        Some("not ready within 120s")
    );
    assert!(
        h.calls()
            .contains(&RuntimeCall::Kill(n("pay-qa"), WindowId("@7".into())))
    );
    assert_eq!(h.rt("pay-qa").window, None);
}

#[test]
fn needs_you_holds_waiting_quiet_errored_oldest_first() {
    let mut h = Harness::new();
    working(&mut h, "squad-3");
    h.start("billing-api");
    h.at(secs(120));
    h.input(hook_with(
        "billing-api",
        &token(&h, "billing-api"),
        "needs-input",
        serde_json::json!({"prompt": "Run tests?"}),
    ));
    h.at(secs(180));
    h.start("pay-qa");
    h.tick_at(secs(300));
    h.tick_at(secs(360));
    let items = h.engine.needs_you();
    assert_eq!(names(&items), vec!["squad-3", "billing-api", "pay-qa"]);
    assert_eq!(
        items[0],
        NeedsYouItem {
            agent: n("squad-3"),
            state: AgentState::Working,
            quiet: true,
            since: h.t0,
            excerpt: None,
            card: None,
        }
    );
    assert_eq!(items[1].since, h.t0 + secs(120));
    assert_eq!(items[1].excerpt.as_deref(), Some("Run tests?"));
    assert_eq!(items[2].state, AgentState::Errored);
    assert_eq!(items[2].excerpt.as_deref(), Some("not ready within 120s"));
    assert_eq!(h.engine.snapshot().needs_you, items);
}

#[test]
fn needs_you_ties_break_by_name() {
    let mut h = Harness::new();
    working(&mut h, "squad-2");
    working(&mut h, "squad-1");
    h.input(hook("squad-2", &token(&h, "squad-2"), "needs-input"));
    h.input(hook("squad-1", &token(&h, "squad-1"), "needs-input"));
    assert_eq!(names(&h.engine.needs_you()), vec!["squad-1", "squad-2"]);
}

#[test]
fn needs_you_changed_only_on_change() {
    let mut h = Harness::new();
    working(&mut h, "billing-api");
    working(&mut h, "auth-service");
    let ev = h.input(hook(
        "billing-api",
        &token(&h, "billing-api"),
        "needs-input",
    ));
    assert_eq!(needs_you_events(&ev), 1);
    assert!(ev.contains(&Event::NeedsYouChanged(h.engine.needs_you())));
    assert_eq!(needs_you_events(&h.tick_at(secs(1))), 0);
    let act = activity_now(&h, "auth-service");
    assert_eq!(needs_you_events(&h.input(act)), 0);
}

#[test]
fn next_deadline_is_earliest_timer() {
    let full = StubAdapter::new(CliKind::Claude).coverage;
    let mut h = Setup {
        claude: claude(full, true),
        start: Millis(0),
        ..Setup::default()
    }
    .build();
    working(&mut h, "auth-service");
    assert_eq!(h.rt("auth-service").last_signal_at, Millis(0));
    assert_eq!(h.engine.next_deadline(), Some(Millis(300_000)));
    h.at(secs(10));
    h.start("pay-qa");
    assert_eq!(h.engine.next_deadline(), Some(Millis(125_000)));
    h.at(secs(20));
    working(&mut h, "squad-1");
    let act = activity_now(&h, "squad-1");
    h.input(act);
    assert_eq!(h.engine.next_deadline(), Some(Millis(21_500)));
    h.tick_at(ms(21_500));
    assert_eq!(h.engine.next_deadline(), Some(Millis(125_000)));
    h.tick_at(secs(125));
    assert_eq!(h.engine.next_deadline(), Some(Millis(130_000)));
}
