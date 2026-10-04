//! Tests for Duo edits, reload and settings: Task 14.

use std::time::Duration;

use super::*;
use crate::settings::Settings;

#[test]
fn rename_running_duo_rejected() {
    let mut h = Harness::new();
    h.force("Duo", AgentState::Idle);
    let org = h.engine.org.clone();
    let ev = edit(&mut h, defaults_edit(&org, "Boss", org.duo.cli));
    assert_eq!(rejected(&ev), vec![RENAME]);
    assert!(saved_orgs(&h).is_empty());
    assert_eq!(h.engine.org.duo.name, n("Duo"));
}

#[test]
fn rename_stopped_duo_moves_its_runtime_state() {
    let mut h = Harness::new();
    put_row(&mut h, "Duo");
    let org = h.engine.org.clone();
    let ev = edit(&mut h, defaults_edit(&org, "Boss", org.duo.cli));
    assert_eq!(org_changed(&ev).0.duo.name, n("Boss"));
    assert!(session_names(&h).contains(&"Boss".to_owned()));
    assert!(!session_names(&h).contains(&"Duo".to_owned()));
    let snap = h.engine.snapshot();
    assert_eq!(snap.agents[&n("Boss")].status.state, AgentState::Stopped);
    assert!(!snap.agents.contains_key(&n("Duo")));
    assert!(!h.engine.agents.contains_key(&n("Duo")));
    h.start("Boss");
    let launched = starts(&h);
    assert_eq!(launched.len(), 1);
    assert_eq!(launched[0].0, n("Boss"));
    assert!(
        launched[0]
            .1
            .env
            .contains(&("DUOHIVE_AGENT".to_owned(), "Boss".to_owned()))
    );

    let mut h = Harness::new();
    h.force_errored("Duo", "claude not found (code 127)", None);
    let org = h.engine.org.clone();
    let ev = edit(&mut h, defaults_edit(&org, "Boss", org.duo.cli));
    assert_eq!(rejected(&ev), vec![RENAME]);
    assert!(saved_orgs(&h).is_empty());
}

#[test]
fn reload_invalid_keeps_last_good_and_reports_line() {
    let mut h = Harness::new();
    let err = OrgFileError {
        line: Some(42),
        message: "unknown CLI \"claud\"".into(),
    };
    h.store.state.lock().unwrap().org_result = Err(err.clone());
    let ev = h.send(Command::ReloadOrg);
    assert_eq!(ev, vec![Event::OrgFileError(Some(err.clone()))]);
    assert_eq!(h.engine.org, test_org());
    assert_eq!(h.engine.snapshot().org_error, Some(err));
}

#[test]
fn reload_valid_clears_banner() {
    let mut h = Harness::new();
    h.engine.org_error = Some(OrgFileError {
        line: Some(1),
        message: "bad".into(),
    });
    h.store.state.lock().unwrap().org_result = Ok(Some(example_org()));
    let ev = h.send(Command::ReloadOrg);
    assert_eq!(org_changed(&ev).0, example_org());
    let pos = |want: fn(&Event) -> bool| ev.iter().position(want).unwrap();
    assert!(
        pos(|e| matches!(e, Event::OrgChanged { .. })) < pos(|e| *e == Event::OrgFileError(None)),
        "{ev:?}"
    );
    assert_eq!(h.engine.snapshot().org_error, None);
    assert_eq!(h.engine.org, example_org());
}

#[test]
fn reload_with_duplicate_names_reports_path() {
    let mut h = Harness::new();
    let mut org = example_org();
    org.roles[0].agents[1].name = n("squad-1");
    h.store.state.lock().unwrap().org_result = Ok(Some(org));
    let ev = h.send(Command::ReloadOrg);
    let err = OrgFileError {
        line: None,
        message: "role[Generalist].agent[squad-1].name: an agent named squad-1 already exists"
            .into(),
    };
    assert_eq!(ev, vec![Event::OrgFileError(Some(err.clone()))]);
    assert_eq!(h.engine.org, test_org());
    assert_eq!(h.engine.snapshot().org_error, Some(err));
}

#[test]
fn change_duo_cli_restarts_running_duo() {
    let mut h = Harness::new();
    h.start("Duo");
    h.input(started("Duo", "@2"));
    let org = h.engine.org.clone();
    let ev = edit(&mut h, defaults_edit(&org, "Duo", CliKind::Opencode));
    assert_eq!(org_changed(&ev).0.duo.cli, CliKind::Opencode);
    assert!(
        h.calls()
            .iter()
            .any(|c| matches!(c, RuntimeCall::Stop(a, _, _) if a == &n("Duo")))
    );
    h.input(gone("Duo", "@2"));
    assert_eq!(h.state("Duo"), AgentState::Starting);
    let launched = starts(&h);
    assert_eq!(launched.len(), 2);
    assert_eq!(launched[1].1.program, "opencode");
}

#[test]
fn update_settings_saves_and_changes_threshold() {
    let mut h = Harness::new();
    h.start("auth-service");
    h.input(started("auth-service", "@1"));
    h.input(busy("auth-service", "tok-1"));
    let s = Settings {
        quiet_after_minutes: 2,
        ..Settings::default()
    };
    h.send(Command::UpdateSettings(s.clone()));
    assert_eq!(h.store.state.lock().unwrap().saved_settings, vec![s]);
    h.tick_at(Duration::from_secs(120));
    assert!(h.rt("auth-service").status.quiet);
}

#[test]
fn update_settings_not_saved_while_settings_file_invalid() {
    let mut h = Harness::new();
    h.engine.settings_error = Some(OrgFileError {
        line: Some(2),
        message: "invalid type".into(),
    });
    h.start("auth-service");
    h.input(started("auth-service", "@1"));
    h.input(busy("auth-service", "tok-1"));
    h.send(Command::UpdateSettings(Settings {
        quiet_after_minutes: 2,
        ..Settings::default()
    }));
    assert!(h.store.state.lock().unwrap().saved_settings.is_empty());
    assert!(h.engine.snapshot().settings_error.is_some());
    h.tick_at(Duration::from_secs(120));
    assert!(h.rt("auth-service").status.quiet);
}

fn refused(agent: &str, how: &str) -> Vec<Event> {
    vec![Event::OrgFileError(Some(OrgFileError {
        line: None,
        message: format!("{agent} is {how} · stop it, then press o to reload"),
    }))]
}

fn reload(h: &mut Harness, org: &Org) -> Vec<Event> {
    h.store.state.lock().unwrap().org_result = Ok(Some(org.clone()));
    h.send(Command::ReloadOrg)
}

#[test]
fn reload_removing_running_agent_refused() {
    let mut without_auth = test_org();
    without_auth.teams[0].roles[0].agents.remove(0);
    let mut h = Harness::new();
    h.force("auth-service", AgentState::Working);
    let ev = reload(&mut h, &without_auth);
    assert_eq!(ev, refused("auth-service", "running"));
    assert_eq!(h.engine.org, test_org());
    assert_eq!(
        h.engine.snapshot().org_error,
        Some(OrgFileError {
            line: None,
            message: "auth-service is running · stop it, then press o to reload".into(),
        })
    );
    let team = TeamDraft {
        name: nn("Growth"),
        defaults: Fields::default(),
    };
    assert_eq!(
        rejected(&edit(&mut h, OrgEdit::AddTeam(team))),
        vec![BROKEN]
    );
    assert!(h.engine.agents.contains_key(&n("auth-service")));

    // Stopped now: the same file is accepted.
    h.force("auth-service", AgentState::Stopped);
    let ev = reload(&mut h, &without_auth);
    assert_eq!(org_changed(&ev).0, without_auth);
    assert!(ev.contains(&Event::OrgFileError(None)));
    assert_eq!(h.engine.org, without_auth);
    assert!(!h.engine.agents.contains_key(&n("auth-service")));

    let mut without_billing = test_org();
    without_billing.teams[0].roles[0].agents.remove(1);
    let mut h = Harness::new();
    h.force_errored("billing-api", "claude exited at start (code 1)", None);
    // Forcing errored skipped NeedsYouChanged, so this reload carries it too.
    let ev = reload(&mut h, &without_billing);
    assert_eq!(ev[..1], refused("billing-api", "errored"));
    assert_eq!(h.engine.org, test_org());

    let mut boss = test_org();
    boss.duo.name = n("Boss");
    let mut h = Harness::new();
    h.force("Duo", AgentState::Idle);
    assert_eq!(reload(&mut h, &boss), refused("Duo", "running"));
    assert_eq!(h.engine.org.duo.name, n("Duo"));
}

#[test]
fn change_duo_cli_while_stopping_does_not_restart() {
    let mut h = Harness::new();
    h.start("Duo");
    h.input(started("Duo", "@2"));
    h.send(Command::StopAgent { agent: n("Duo") });
    let org = h.engine.org.clone();
    org_changed(&edit(&mut h, defaults_edit(&org, "Duo", CliKind::Opencode)));
    let stops = h
        .calls()
        .iter()
        .filter(|c| matches!(c, RuntimeCall::Stop(..)))
        .count();
    assert_eq!(stops, 1, "the user's stop only");
    h.input(gone("Duo", "@2"));
    assert_eq!(h.state("Duo"), AgentState::Stopped);
    assert_eq!(starts(&h).len(), 1);
}

#[test]
fn settings_save_failure_changes_nothing() {
    let mut h = Harness::new();
    h.store.state.lock().unwrap().fail_next_save = true;
    let ev = h.send(Command::UpdateSettings(Settings {
        quiet_after_minutes: 2,
        ..Settings::default()
    }));
    assert_eq!(
        rejected(&ev),
        vec!["couldn't save settings.toml: disk full"]
    );
    assert_eq!(h.engine.settings, Settings::default());
}
