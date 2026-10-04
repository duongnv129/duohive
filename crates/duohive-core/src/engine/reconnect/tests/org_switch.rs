//! Start-up and the last good org: which org is accepted, and sessions for
//! agents `org.toml` no longer names (controller rulings A and B, Task 15).

use super::*;
use crate::command::{Command, OrgEdit, TeamDraft};
use crate::names::NodeName;
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

fn missing_refusal(agent: &str) -> OrgFileError {
    OrgFileError {
        line: None,
        message: format!(
            "{agent} is running but missing from org.toml · add it back, or stop it, then press o to reload"
        ),
    }
}

fn reload(h: &mut Harness, org: &Org) -> Vec<Event> {
    h.store.state.lock().unwrap().org_result = Ok(Some(org.clone()));
    h.send(Command::ReloadOrg)
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
    // The running agent's row is held, never silently dropped; the file's own error stays.
    assert!(deleted(&h).is_empty());
    assert!(has_row(&h, "gone-agent"));
    assert!(kills(&h).is_empty());
    assert_eq!(h.engine.org, test_org());
    assert_eq!(h.engine.org_error.as_ref().map(|e| e.line), Some(Some(3)));
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
fn missing_running_agent_with_no_org_naming_it_is_held() {
    let mut h = Harness::new();
    put(&h, row("gone-agent", AgentState::Working, Some("@5"), T0));
    h.engine.start_up();
    let ev = inventory(&mut h, vec![live("gone-agent", "@5", 900)]);
    let err = missing_refusal("gone-agent");
    assert!(
        ev.contains(&Event::OrgFileError(Some(err.clone()))),
        "{ev:?}"
    );
    assert_eq!(h.engine.org, test_org());
    assert_eq!(h.engine.org_error, Some(err));
    assert!(marked(&h).is_empty(), "the loaded org is not accepted");
    assert!(has_row(&h, "gone-agent"));
    assert!(deleted(&h).is_empty());
    assert!(kills(&h).is_empty());
    // In-app edits stay refused.
    let team = TeamDraft {
        name: NodeName::parse("Growth").unwrap(),
        defaults: Fields::default(),
    };
    let refused = h.send(Command::EditOrg(OrgEdit::AddTeam(team)));
    assert!(
        matches!(&refused[..], [Event::CommandRejected { .. }]),
        "{refused:?}"
    );
    // A reload that names it again adopts it, row and token kept.
    let ev = reload(&mut h, &org_naming_gone_agent());
    assert!(ev.contains(&Event::OrgFileError(None)), "{ev:?}");
    assert_eq!(h.engine.org, org_naming_gone_agent());
    let rt = h.rt("gone-agent");
    assert_eq!(rt.status.state, AgentState::Working);
    assert_eq!(rt.token, Some(Token(token("gone-agent"))));
    assert_eq!(rt.window, Some(WindowId("@5".into())));
    assert!(has_row(&h, "gone-agent"));
    assert_eq!(marked(&h), vec![org_naming_gone_agent()]);
    let ev = h.input(Input::Hook(message(
        "gone-agent",
        "turn-end",
        serde_json::json!({}),
    )));
    assert_eq!(moves(&ev, "gone-agent"), vec![AgentState::Finished]);
}

#[test]
fn held_row_is_dropped_once_its_window_is_gone() {
    let mut h = Harness::new();
    put(&h, row("gone-agent", AgentState::Working, Some("@5"), T0));
    h.engine.start_up();
    inventory(&mut h, vec![live("gone-agent", "@5", 900)]);
    // A reload still missing it keeps the banner and re-checks the windows.
    let inventories = |h: &Harness| {
        h.calls()
            .iter()
            .filter(|c| **c == RuntimeCall::Inventory)
            .count()
    };
    let before = inventories(&h);
    let ev = reload(&mut h, &test_org());
    assert!(ev.contains(&Event::OrgFileError(Some(missing_refusal("gone-agent")))));
    assert!(marked(&h).is_empty());
    assert_eq!(inventories(&h), before + 1);
    // Still running: nothing changes.
    assert_eq!(
        inventory(&mut h, vec![live("gone-agent", "@5", 900)]),
        vec![]
    );
    assert!(has_row(&h, "gone-agent"));
    // Gone: the row goes, the banner clears and the org is accepted.
    let ev = inventory(&mut h, vec![]);
    assert_eq!(ev, vec![Event::OrgFileError(None)]);
    assert_eq!(deleted(&h), vec!["gone-agent".to_owned()]);
    assert_eq!(h.engine.org_error, None);
    assert_eq!(marked(&h), vec![test_org()]);
}

#[test]
fn last_good_org_must_name_live_agents_of_the_loaded_org() {
    // The file renamed auth-service to new-agent while both ran: neither org
    // names both, so no live agent may be silently orphaned.
    let mut loaded = test_org();
    loaded.teams[0].roles[0].agents[0].name = n("new-agent");
    let mut h = Setup {
        org: loaded.clone(),
        ..Setup::default()
    }
    .build();
    h.store.state.lock().unwrap().last_good = Some(test_org());
    put(&h, row("auth-service", AgentState::Working, Some("@1"), T0));
    put(&h, row("new-agent", AgentState::Working, Some("@2"), T0));
    h.engine.start_up();
    inventory(
        &mut h,
        vec![
            live("auth-service", "@1", 900),
            live("new-agent", "@2", 900),
        ],
    );
    assert_eq!(h.engine.org, loaded);
    assert_eq!(h.engine.org_error, Some(missing_refusal("auth-service")));
    assert_eq!(h.state("new-agent"), AgentState::Working);
    assert!(has_row(&h, "auth-service"));
    assert!(has_row(&h, "new-agent"));
    assert!(marked(&h).is_empty());
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

/// Holds gone-agent (Working on @5): start-up with a live window no org names.
fn held_gone_agent() -> Harness {
    let mut h = Harness::new();
    put(&h, row("gone-agent", AgentState::Working, Some("@5"), T0));
    h.engine.start_up();
    inventory(&mut h, vec![live("gone-agent", "@5", 900)]);
    assert_eq!(h.engine.org_error, Some(missing_refusal("gone-agent")));
    h
}

#[test]
fn held_agent_whose_pane_died_gets_the_exit_rules_on_adoption() {
    let mut h = held_gone_agent();
    let ev = h.input(Input::Runtime(RuntimeEvent::PaneDied {
        agent: n("gone-agent"),
        window: WindowId("@5".into()),
        status: Some(1),
    }));
    assert_eq!(ev, vec![]);
    reload(&mut h, &org_naming_gone_agent());
    let status = &h.rt("gone-agent").status;
    assert_eq!(status.state, AgentState::Errored);
    assert_eq!(
        status.error.as_deref(),
        Some("exited unexpectedly (code 1)")
    );
    assert!(kills(&h).contains(&RuntimeCall::Kill(n("gone-agent"), WindowId("@5".into()))));
}

#[test]
fn held_agent_whose_window_went_is_dropped() {
    let mut h = held_gone_agent();
    let ev = h.input(Input::Runtime(RuntimeEvent::WindowGone {
        agent: n("gone-agent"),
        window: WindowId("@5".into()),
    }));
    assert!(ev.contains(&Event::OrgFileError(None)), "{ev:?}");
    assert!(!has_row(&h, "gone-agent"));
    assert_eq!(h.engine.org_error, None);
    assert_eq!(marked(&h), vec![test_org()]);
}

#[test]
fn adopted_agent_gets_its_window_checked() {
    let mut h = held_gone_agent();
    let inventories = |h: &Harness| {
        h.calls()
            .iter()
            .filter(|c| **c == RuntimeCall::Inventory)
            .count()
    };
    let before = inventories(&h);
    reload(&mut h, &org_naming_gone_agent());
    assert_eq!(h.state("gone-agent"), AgentState::Working);
    assert_eq!(inventories(&h), before + 1);
    // Its window vanished meanwhile (D9): stopped, no error, row deleted.
    let ev = inventory(&mut h, vec![]);
    assert_eq!(moves(&ev, "gone-agent"), vec![AgentState::Stopped]);
    assert_eq!(h.rt("gone-agent").status.error, None);
    assert!(!has_row(&h, "gone-agent"));
    // Checked once: a later inventory changes nothing.
    h.start("gone-agent");
    assert_eq!(inventory(&mut h, vec![]), vec![]);
    assert_eq!(h.state("gone-agent"), AgentState::Starting);
}
