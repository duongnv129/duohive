//! Start-up and the last good org: which org is accepted, and sessions for
//! agents `org.toml` no longer names (controller rulings A and B, Task 15).

use super::*;
use crate::org::AgentDef;
use crate::org::Fields;

/// The test org with `gone-agent` added under Generalist, as it was before a hand edit removed it.
fn org_naming_gone_agent() -> Org {
    let mut org = test_org();
    org.roles[0].agents.push(AgentDef {
        name: n("gone-agent"),
        fields: Fields::default(),
    });
    org
}

fn running_refusal(agent: &str, how: &str) -> OrgFileError {
    OrgFileError {
        line: None,
        message: format!("{agent} is {how} · stop it, then press o to reload"),
    }
}

#[test]
fn start_up_marks_the_accepted_org_last_good() {
    let mut h = Harness::new();
    put(&h, row("auth-service", AgentState::Working, Some("@1"), T0));
    h.engine.start_up();
    assert_eq!(marked(&h), vec![test_org()]);
    inventory(&mut h, vec![live("auth-service", "@1", 900)]);
    assert_eq!(marked(&h), vec![test_org()], "marked once");
}

#[test]
fn start_up_on_a_broken_org_file_marks_nothing() {
    let mut h = Harness::new();
    h.engine.org_error = Some(OrgFileError {
        line: Some(3),
        message: "unknown CLI \"claud\"".into(),
    });
    put(&h, row("gone-agent", AgentState::Working, Some("@5"), T0));
    h.engine.start_up();
    inventory(&mut h, vec![live("gone-agent", "@5", 900)]);
    assert!(marked(&h).is_empty());
    // D9: the row goes, the window is left running.
    assert_eq!(deleted(&h), vec!["gone-agent".to_owned()]);
    assert!(kills(&h).is_empty());
    assert_eq!(h.engine.org, test_org());
}

#[test]
fn a_failed_last_good_write_is_reported_not_fatal() {
    let mut h = Harness::new();
    h.store.state.lock().unwrap().fail_next_save = true;
    let ev = h.engine.start_up();
    assert!(matches!(ev[0], Event::Snapshot(_)), "{ev:?}");
    assert!(ev.contains(&Event::CommandRejected {
        reason: "couldn't save org.last-good.toml: disk full".into(),
    }));
    assert!(marked(&h).is_empty());
    assert_eq!(inventory(&mut h, vec![]).last(), Some(&Event::Reconnected));
}

#[test]
fn running_agent_missing_from_org_file_keeps_last_good_org() {
    let mut h = Harness::new();
    h.store.state.lock().unwrap().last_good = Some(org_naming_gone_agent());
    put(&h, row("gone-agent", AgentState::Working, Some("@5"), T0));
    put(&h, row("auth-service", AgentState::Working, Some("@1"), T0));
    let snap = snapshot_of(&h.engine.start_up());
    assert!(marked(&h).is_empty(), "not accepted before the inventory");
    assert_eq!(snap.reconnecting, 1);
    let ev = inventory(
        &mut h,
        vec![
            live("gone-agent", "@5", 900),
            live("auth-service", "@1", 900),
        ],
    );
    let err = running_refusal("gone-agent", "running");
    assert!(
        ev.contains(&Event::OrgFileError(Some(err.clone()))),
        "{ev:?}"
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::OrgChanged { org, .. } if **org == org_naming_gone_agent()
    )));
    assert_eq!(h.engine.org, org_naming_gone_agent());
    assert_eq!(h.engine.snapshot().org_error, Some(err));
    assert!(marked(&h).is_empty());
    // The running agent stays manageable: restored, on its window, with its row.
    let rt = h.rt("gone-agent");
    assert_eq!(rt.status.state, AgentState::Working);
    assert_eq!(rt.window, Some(WindowId("@5".into())));
    assert!(has_row(&h, "gone-agent"));
    assert!(deleted(&h).is_empty());
    assert_eq!(h.state("auth-service"), AgentState::Working);
    // It can be stopped.
    let stop = h.send(crate::command::Command::StopAgent {
        agent: n("gone-agent"),
    });
    assert_eq!(stop, vec![]);
    assert!(
        h.calls()
            .iter()
            .any(|c| matches!(c, RuntimeCall::Stop(a, ..) if *a == n("gone-agent")))
    );
}

#[test]
fn hooks_replay_for_an_agent_kept_by_the_last_good_org() {
    let mut h = Harness::new();
    h.store.state.lock().unwrap().last_good = Some(org_naming_gone_agent());
    put(&h, row("gone-agent", AgentState::Working, Some("@5"), T0));
    queue(&h, "gone-agent", "turn-end", serde_json::json!({}));
    h.engine.start_up();
    let ev = inventory(&mut h, vec![live("gone-agent", "@5", 900)]);
    assert_eq!(moves(&ev, "gone-agent"), vec![AgentState::Finished]);
}

#[test]
fn missing_agent_whose_window_is_gone_is_dropped_and_org_accepted() {
    let mut h = Harness::new();
    h.store.state.lock().unwrap().last_good = Some(org_naming_gone_agent());
    put(&h, row("gone-agent", AgentState::Working, Some("@5"), T0));
    h.engine.start_up();
    let ev = inventory(&mut h, vec![dead("gone-agent", "@5", Some(0))]);
    assert!(!ev.iter().any(|e| matches!(e, Event::OrgFileError(_))));
    assert_eq!(h.engine.org, test_org());
    assert_eq!(h.engine.org_error, None);
    assert_eq!(deleted(&h), vec!["gone-agent".to_owned()]);
    assert_eq!(marked(&h), vec![test_org()]);
    assert!(
        kills(&h).is_empty(),
        "D9: windows with no row are left alone"
    );
}

#[test]
fn missing_running_agent_with_no_org_naming_it_is_left_running() {
    let mut h = Harness::new();
    put(&h, row("gone-agent", AgentState::Working, Some("@5"), T0));
    h.engine.start_up();
    inventory(&mut h, vec![live("gone-agent", "@5", 900)]);
    assert_eq!(h.engine.org, test_org());
    assert_eq!(h.engine.org_error, None);
    assert_eq!(deleted(&h), vec!["gone-agent".to_owned()]);
    assert_eq!(marked(&h), vec![test_org()]);
    assert!(kills(&h).is_empty());
}

#[test]
fn missing_errored_agent_with_a_live_window_says_errored() {
    let mut h = Harness::new();
    h.store.state.lock().unwrap().last_good = Some(org_naming_gone_agent());
    let mut errored = row("gone-agent", AgentState::Errored, Some("@5"), T0);
    errored.status.error = Some("exited unexpectedly (code 1)".into());
    put(&h, errored);
    h.engine.start_up();
    inventory(&mut h, vec![live("gone-agent", "@5", 900)]);
    assert_eq!(
        h.engine.org_error,
        Some(running_refusal("gone-agent", "errored"))
    );
    assert_eq!(h.state("gone-agent"), AgentState::Errored);
}
