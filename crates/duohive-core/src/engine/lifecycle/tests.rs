//! Tests for the lifecycle: Task 11.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use super::super::harness::{DATA_DIR, HOME, Harness, Setup, T0, all_found};
use super::super::{ExitPending, Input};
use crate::adapter::CliPresence;
use crate::cli_kind::CliKind;
use crate::command::Command;
use crate::event::Event;
use crate::inherit::resolve;
use crate::launch::{Key, Token, WindowId};
use crate::ports::RuntimeEvent;
use crate::status::AgentState;
use crate::testkit::{RuntimeCall, StubAdapter, n};
use crate::time::Millis;
use crate::validate::DirStatus;

fn rejected(events: &[Event], reason: &str) -> bool {
    events
        .iter()
        .any(|e| matches!(e, Event::CommandRejected { reason: r } if r == reason))
}

fn task_args(task: &str) -> Vec<String> {
    vec!["--task".to_owned(), task.to_owned()]
}

#[test]
fn start_stopped_agent_launches_with_no_task() {
    let mut h = Harness::new();
    h.start("pay-qa");
    let cmd = h.only_start();
    assert!(!cmd.args.iter().any(|a| a == "--task"), "{:?}", cmd.args);
    assert!(matches!(&h.calls()[0], RuntimeCall::Start(a, _) if *a == n("pay-qa")));
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    let st = h.store.state.lock().unwrap();
    let row = &st.sessions[&n("pay-qa")];
    assert_eq!(row.token, Some(Token("tok-1".into())));
    assert_eq!(row.status.state, AgentState::Starting);
    assert!(!row.task_at_launch);
}

#[test]
fn launch_env_carries_token_agent_and_data_dir() {
    let mut h = Harness::new();
    h.start("pay-qa");
    let env = h.only_start().env;
    for (k, v) in [
        ("DUOHIVE_TOKEN", "tok-1"),
        ("DUOHIVE_AGENT", "pay-qa"),
        ("DUOHIVE_DATA_DIR", DATA_DIR),
    ] {
        assert!(
            env.contains(&(k.to_owned(), v.to_owned())),
            "{k} in {env:?}"
        );
    }
}

#[test]
fn launch_spec_has_stacked_prompt_and_run_dir() {
    let mut h = Harness::new();
    h.start("pay-qa");
    let expected = resolve(&h.engine.org, &n("pay-qa")).unwrap().prompt;
    let specs = h.claude.launches.lock().unwrap();
    let [spec] = specs.as_slice() else {
        panic!("expected one launch spec, got {specs:?}")
    };
    assert!(expected.contains("Ask before you delete anything."));
    assert_eq!(spec.agent.prompt, expected);
    assert_eq!(spec.run_dir, PathBuf::from(DATA_DIR).join("run/pay-qa"));
    assert_eq!(spec.home, PathBuf::from(HOME));
    assert_eq!(spec.task, None);
}

#[test]
fn give_work_stopped_launches_with_task() {
    let mut h = Harness::new();
    h.give("pay-qa", "Add retry cases");
    assert_eq!(h.only_start().args, task_args("Add retry cases"));
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    assert_eq!(h.rt("pay-qa").last_task.as_deref(), Some("Add retry cases"));
    assert!(h.rt("pay-qa").task_at_launch);
    let st = h.store.state.lock().unwrap();
    assert_eq!(
        st.sessions[&n("pay-qa")].last_task.as_deref(),
        Some("Add retry cases")
    );
}

#[test]
fn give_work_errored_relaunches() {
    let mut h = Harness::new();
    h.force_errored("pay-qa", "exited unexpectedly (code 1)", None);
    h.give("pay-qa", "Retry it");
    let calls = h.calls();
    assert!(matches!(calls.as_slice(), [RuntimeCall::Start(a, _)] if *a == n("pay-qa")));
    assert_eq!(h.only_start().args, task_args("Retry it"));
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    assert_eq!(h.rt("pay-qa").status.error, None);
}

#[test]
fn give_work_finished_types_next_turn() {
    let mut h = Harness::new();
    h.force("squad-2", AgentState::Finished);
    h.give("squad-2", "Now make…");
    assert_eq!(
        h.calls(),
        vec![RuntimeCall::Paste(
            n("squad-2"),
            "Now make…".into(),
            vec![Key::Enter]
        )]
    );
    // P2-D11: the paste moves nothing; a busy hook does.
    assert_eq!(h.state("squad-2"), AgentState::Finished);
    assert!(h.rt("squad-2").delivery.is_some());
}

#[test]
fn give_work_idle_types_next_turn() {
    let mut h = Harness::new();
    h.force("squad-1", AgentState::Idle);
    h.give("squad-1", "Add a test");
    assert_eq!(
        h.calls(),
        vec![RuntimeCall::Paste(
            n("squad-1"),
            "Add a test".into(),
            vec![Key::Enter]
        )]
    );
    assert_eq!(h.state("squad-1"), AgentState::Idle);
    assert_eq!(h.rt("squad-1").last_task.as_deref(), Some("Add a test"));
}

#[test]
fn give_work_working_is_refused() {
    let mut h = Harness::new();
    h.force("auth-service", AgentState::Working);
    let events = h.give("auth-service", "More");
    assert!(events.contains(&Event::GiveWorkRefused {
        agent: n("auth-service"),
        state: AgentState::Working,
        pending: false,
    }));
    assert!(h.calls().is_empty());
    assert_eq!(h.state("auth-service"), AgentState::Working);
}

#[test]
fn give_work_waiting_is_refused() {
    let mut h = Harness::new();
    h.force("billing-api", AgentState::Waiting);
    let events = h.give("billing-api", "More");
    assert!(events.contains(&Event::GiveWorkRefused {
        agent: n("billing-api"),
        state: AgentState::Waiting,
        pending: false,
    }));
    assert!(h.calls().is_empty());
    assert_eq!(h.state("billing-api"), AgentState::Waiting);
}

#[test]
fn give_work_starting_is_refused() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.call_log.lock().unwrap().clear();
    let events = h.give("pay-qa", "Too early");
    assert_eq!(
        events,
        vec![Event::GiveWorkRefused {
            agent: n("pay-qa"),
            state: AgentState::Starting,
            pending: false,
        }]
    );
    assert!(h.calls().is_empty());
}

#[test]
fn start_running_agent_rejected() {
    let mut h = Harness::new();
    h.start("pay-qa");
    let events = h.start("pay-qa");
    assert!(rejected(&events, "pay-qa is already running"), "{events:?}");
    assert_eq!(h.calls().len(), 1);
}

#[test]
fn unknown_agent_rejected() {
    let mut h = Harness::new();
    let events = h.start("ghost");
    assert!(rejected(&events, "no agent named ghost"), "{events:?}");
    let events = h.send(Command::StopAgent { agent: n("ghost") });
    assert!(rejected(&events, "no agent named ghost"), "{events:?}");
    let events = h.send(Command::ViewAgent { agent: n("ghost") });
    assert!(rejected(&events, "no agent named ghost"), "{events:?}");
    let events = h.give("ghost", "Anything");
    assert!(rejected(&events, "no agent named ghost"), "{events:?}");
    assert!(h.calls().is_empty());
}

#[test]
fn launch_error_errors_through_starting() {
    let mut h = Setup {
        claude: StubAdapter::new(CliKind::Claude).with_launch_error("bad model"),
        ..Setup::default()
    }
    .build();
    let events = h.start("pay-qa");
    let states: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::AgentStatusChanged { status, .. } => Some((status.state, status.error.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        states,
        vec![
            (AgentState::Starting, None),
            (AgentState::Errored, Some("bad model".into())),
        ]
    );
    assert!(h.calls().is_empty());
}

#[test]
fn each_launch_gets_a_fresh_token() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.force_errored("pay-qa", "exited unexpectedly (code 1)", None);
    h.call_log.lock().unwrap().clear();
    h.give("pay-qa", "Retry");
    let env = h.only_start().env;
    assert!(
        env.contains(&("DUOHIVE_TOKEN".to_owned(), "tok-2".to_owned())),
        "{env:?}"
    );
    assert_eq!(h.rt("pay-qa").token, Some(Token("tok-2".into())));
    let st = h.store.state.lock().unwrap();
    assert_eq!(st.sessions[&n("pay-qa")].token, Some(Token("tok-2".into())));
}

#[test]
fn failed_relaunch_clears_the_old_token() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.force_errored("pay-qa", "exited unexpectedly (code 1)", None);
    h.input(Input::CliInventory(vec![(
        CliKind::Claude,
        CliPresence::Missing,
    )]));
    h.give("pay-qa", "Retry");
    assert_eq!(h.state("pay-qa"), AgentState::Errored);
    assert_eq!(h.rt("pay-qa").token, None);
    let st = h.store.state.lock().unwrap();
    assert_eq!(st.sessions[&n("pay-qa")].token, None);
}

#[test]
fn give_work_with_no_adapter_is_rejected() {
    let mut setup = Setup {
        without_adapter: Some(CliKind::Opencode),
        ..Setup::default()
    };
    setup.org.roles[0].agents[1].fields.cli = Some(CliKind::Opencode);
    let mut h = setup.build();
    h.force("squad-2", AgentState::Finished);
    let events = h.give("squad-2", "Next");
    assert!(
        rejected(
            &events,
            "opencode not found · install it or change its CLI (e)"
        ),
        "{events:?}"
    );
    assert!(h.calls().is_empty());
    assert_eq!(h.state("squad-2"), AgentState::Finished);
}

#[test]
fn blank_task_is_rejected() {
    let mut h = Harness::new();
    h.force("squad-1", AgentState::Idle);
    for task in ["", "   ", "\n\t"] {
        let events = h.give("squad-1", task);
        assert!(rejected(&events, "the task is empty"), "{events:?}");
        let events = h.give("pay-qa", task);
        assert!(rejected(&events, "the task is empty"), "{events:?}");
    }
    assert!(h.calls().is_empty());
    assert_eq!(h.state("squad-1"), AgentState::Idle);
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
}

#[test]
fn missing_directory_refuses_start() {
    let mut h = Setup {
        dirs: HashMap::from([(PathBuf::from(HOME).join("code/pay-qa"), DirStatus::Missing)]),
        ..Setup::default()
    }
    .build();
    let events = h.start("pay-qa");
    assert!(
        rejected(&events, "✕ ~/code/pay-qa not found · e to fix"),
        "{events:?}"
    );
    assert!(h.calls().is_empty());
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
}

#[test]
fn missing_cli_errors_through_starting() {
    let mut setup = Setup {
        inventory: Some(vec![
            (
                CliKind::Claude,
                CliPresence::Found {
                    version: "2.1.0".into(),
                },
            ),
            (CliKind::Opencode, CliPresence::Missing),
        ]),
        ..Setup::default()
    };
    setup.org.roles[0].agents[2].fields.cli = Some(CliKind::Opencode);
    let mut h = setup.build();
    let events = h.start("squad-3");
    let states: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::AgentStatusChanged { agent, status } if *agent == n("squad-3") => {
                Some((status.state, status.error.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        states,
        vec![
            (AgentState::Starting, None),
            (
                AgentState::Errored,
                Some("opencode not found · install it or change its CLI (e)".into())
            ),
        ]
    );
    assert!(h.calls().is_empty());
    assert!(h.opencode.launches.lock().unwrap().is_empty());
}

#[test]
fn launch_before_cli_inventory_is_queued() {
    let mut h = Setup {
        inventory: None,
        ..Setup::default()
    }
    .build();
    assert!(h.start("pay-qa").is_empty());
    assert!(h.calls().is_empty());
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
    h.input(Input::CliInventory(all_found()));
    assert!(matches!(h.calls().as_slice(), [RuntimeCall::Start(a, _)] if *a == n("pay-qa")));
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
}

#[test]
fn queued_launches_run_in_order() {
    let mut h = Setup {
        inventory: None,
        ..Setup::default()
    }
    .build();
    h.give("squad-1", "First");
    h.start("pay-qa");
    h.start("pay-qa");
    let events = h.input(Input::CliInventory(all_found()));
    let started: Vec<_> = h
        .calls()
        .into_iter()
        .filter_map(|c| match c {
            RuntimeCall::Start(a, cmd) => Some((a, cmd.args)),
            _ => None,
        })
        .collect();
    assert_eq!(
        started,
        vec![(n("squad-1"), task_args("First")), (n("pay-qa"), vec![])]
    );
    assert!(rejected(&events, "pay-qa is already running"), "{events:?}");
}

#[test]
fn no_directory_refuses_start() {
    let mut setup = Setup::default();
    setup.org.teams[0].roles[1].agents[0].fields.directory = None;
    let mut h = setup.build();
    let events = h.start("pay-qa");
    assert!(
        rejected(&events, "no directory set · e to fix"),
        "{events:?}"
    );
    assert!(h.calls().is_empty());
    let events = h.give("pay-qa", "Anything");
    assert!(
        rejected(&events, "no directory set · e to fix"),
        "{events:?}"
    );
    assert!(h.calls().is_empty());
}

#[test]
fn relaunch_resets_per_launch_flags() {
    let mut h = Harness::new();
    h.force_errored("pay-qa", "exited unexpectedly (code 0)", None);
    {
        let rt = h.rt_mut("pay-qa");
        rt.saw_end_hook = true;
        rt.waiting_prompt = Some("Allow edit?".into());
        rt.status.quiet = true;
        rt.status.quiet_since = Some(Millis(5));
        rt.pre_read_done = true;
        rt.last_signal_at = Millis(5);
        rt.exit_pending = Some(ExitPending {
            window: WindowId("@1".into()),
            status: Some(0),
            due: Millis(10),
        });
    }
    h.clock.advance(Duration::from_secs(30));
    let t = T0 + Duration::from_secs(30);
    h.give("pay-qa", "Retry");
    let rt = h.rt("pay-qa");
    assert!(!rt.saw_end_hook);
    assert_eq!(rt.waiting_prompt, None);
    assert!(!rt.status.quiet);
    assert_eq!(rt.status.quiet_since, None);
    assert!(!rt.pre_read_done);
    assert_eq!(rt.exit_pending, None);
    assert!(!rt.stopping);
    assert_eq!(rt.last_signal_at, t);
    assert_eq!(rt.status.since, t);
    assert_eq!(rt.start_deadline, Some(t + Duration::from_secs(120)));
    assert_eq!(h.engine.next_deadline(), Some(t + Duration::from_secs(120)));
}

#[test]
fn relaunch_kills_the_old_window_first() {
    let mut h = Harness::new();
    h.force_errored("pay-qa", "tmux server exited", Some("@7"));
    h.give("pay-qa", "Retry");
    let calls = h.calls();
    assert!(
        matches!(
            calls.as_slice(),
            [RuntimeCall::Kill(a, w), RuntimeCall::Start(b, _)]
                if *a == n("pay-qa") && *w == WindowId("@7".into()) && *b == n("pay-qa")
        ),
        "{calls:?}"
    );
    assert_eq!(h.rt("pay-qa").window, None);
}

#[test]
fn untested_version_sets_notice() {
    let mut h = Setup {
        inventory: Some(vec![(
            CliKind::Claude,
            CliPresence::Found {
                version: "2.3.0".into(),
            },
        )]),
        ..Setup::default()
    }
    .build();
    let events = h.start("pay-qa");
    assert!(
        events.contains(&Event::AgentNoticeChanged {
            agent: n("pay-qa"),
            notice: Some("notice: claude 2.3 untested (tested up to 2.1)".into()),
        }),
        "{events:?}"
    );
    assert_eq!(h.calls().len(), 1);
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    assert_eq!(
        h.engine.snapshot().agents[&n("pay-qa")].notice.as_deref(),
        Some("notice: claude 2.3 untested (tested up to 2.1)")
    );
}

#[test]
fn tested_version_sets_no_notice() {
    let mut h = Harness::new();
    let events = h.start("pay-qa");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::AgentNoticeChanged { .. }))
    );
    assert_eq!(h.rt("pay-qa").notice, None);
}

#[test]
fn start_failed_errors_and_keeps_task() {
    let mut h = Harness::new();
    h.give("pay-qa", "Add retry cases");
    let events = h.input(Input::Runtime(RuntimeEvent::StartFailed {
        agent: n("pay-qa"),
        reason: "branch exists".into(),
    }));
    assert!(events.iter().any(|e| matches!(e,
        Event::AgentStatusChanged { agent, status }
            if *agent == n("pay-qa") && status.state == AgentState::Errored
                && status.error.as_deref() == Some("branch exists"))));
    assert_eq!(h.state("pay-qa"), AgentState::Errored);
    assert_eq!(h.rt("pay-qa").last_task.as_deref(), Some("Add retry cases"));
    let st = h.store.state.lock().unwrap();
    assert_eq!(st.sessions[&n("pay-qa")].status.state, AgentState::Errored);
}

#[test]
fn dir_inventory_reports_missing_directories() {
    let mut h = Harness::new();
    assert_eq!(h.engine.snapshot().agents[&n("billing-api")].problem, None);
    let billing = PathBuf::from(HOME).join("code/billing");
    let events = h.input(Input::DirInventory(vec![(billing, DirStatus::Missing)]));
    let [Event::OrgChanged { org, problems }] = events.as_slice() else {
        panic!("expected one OrgChanged, got {events:?}")
    };
    assert_eq!(**org, h.engine.org);
    assert_eq!(
        problems.get(&n("billing-api")).map(String::as_str),
        Some("✕ ~/code/billing not found · e to fix")
    );
    assert_eq!(problems.get(&n("auth-service")), None);
    assert_eq!(
        h.engine.snapshot().agents[&n("billing-api")]
            .problem
            .as_deref(),
        Some("✕ ~/code/billing not found · e to fix")
    );
}

#[test]
fn stop_running_sends_quit_keys_with_grace() {
    let mut h = Harness::new();
    h.force("auth-service", AgentState::Working);
    let events = h.send(Command::StopAgent {
        agent: n("auth-service"),
    });
    assert!(events.is_empty(), "{events:?}");
    assert_eq!(
        h.calls(),
        vec![RuntimeCall::Stop(
            n("auth-service"),
            vec![Key::Escape, Key::Text("/exit".into()), Key::Enter],
            Duration::from_secs(5)
        )]
    );
    assert_eq!(h.state("auth-service"), AgentState::Working);
    assert!(h.rt("auth-service").stopping);
}

#[test]
fn stop_errored_goes_stopped() {
    let mut h = Harness::new();
    h.start("pay-qa");
    h.call_log.lock().unwrap().clear();
    assert!(
        h.store
            .state
            .lock()
            .unwrap()
            .sessions
            .contains_key(&n("pay-qa"))
    );
    h.force_errored("pay-qa", "exited unexpectedly (code 1)", Some("@3"));
    let events = h.send(Command::StopAgent { agent: n("pay-qa") });
    assert!(events.iter().any(|e| matches!(e,
        Event::AgentStatusChanged { agent, status }
            if *agent == n("pay-qa") && status.state == AgentState::Stopped
                && status.error.is_none())));
    assert_eq!(
        h.calls(),
        vec![RuntimeCall::Kill(n("pay-qa"), WindowId("@3".into()))]
    );
    assert_eq!(h.rt("pay-qa").window, None);
    assert_eq!(h.rt("pay-qa").status.error, None);
    assert_eq!(h.rt("pay-qa").token, None);
    {
        let st = h.store.state.lock().unwrap();
        assert!(st.deleted_sessions.contains(&n("pay-qa")));
        assert!(!st.sessions.contains_key(&n("pay-qa")));
    }

    h.force_errored("squad-1", "tmux server exited", None);
    h.send(Command::StopAgent {
        agent: n("squad-1"),
    });
    assert_eq!(h.state("squad-1"), AgentState::Stopped);
    assert_eq!(h.calls().len(), 1, "no Kill without a window");

    let events = h.send(Command::StopAgent { agent: n("pay-qa") });
    assert!(events.is_empty(), "stopped → no-op: {events:?}");
    assert_eq!(h.calls().len(), 1);
}

#[test]
fn view_finished_goes_idle() {
    let mut h = Harness::new();
    h.force("squad-2", AgentState::Finished);
    let events = h.send(Command::ViewAgent {
        agent: n("squad-2"),
    });
    assert!(matches!(events.as_slice(),
        [Event::AgentStatusChanged { agent, status }]
            if *agent == n("squad-2") && status.state == AgentState::Idle));
    h.force("squad-1", AgentState::Idle);
    let events = h.send(Command::ViewAgent {
        agent: n("squad-1"),
    });
    assert!(events.is_empty(), "{events:?}");
    assert!(h.calls().is_empty());
}

#[test]
fn store_failure_is_reported_not_fatal() {
    let mut h = Harness::new();
    h.store.state.lock().unwrap().fail_next_save = true;
    let events = h.start("pay-qa");
    assert!(
        rejected(&events, "couldn't save state: disk full"),
        "{events:?}"
    );
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    assert_eq!(h.calls().len(), 1);
}
