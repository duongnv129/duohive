//! `CreateOrg` (UI §4.3).

use std::sync::atomic::Ordering;

use crate::adapter::CliPresence;
use crate::cli_kind::CliKind;
use crate::command::Command;
use crate::engine::Input;
use crate::event::Event;
use crate::templates::{SquadInput, TemplateInput};

use super::super::harness::{Harness, Setup, all_found};

fn template() -> TemplateInput {
    TemplateInput::AgentSquad(SquadInput {
        size: 3,
        directory: "~/code/app".into(),
        cli: CliKind::Claude,
        worktrees: false,
        prompt: None,
    })
}

fn first_run() -> Harness {
    Setup {
        first_run: Some(all_found()),
        ..Setup::default()
    }
    .build()
}

fn create(h: &mut Harness, template: &TemplateInput, your_name: Option<&str>) {
    h.send(Command::CreateOrg {
        template: template.clone(),
        your_name: your_name.map(ToOwned::to_owned),
        duo_cli: CliKind::Claude,
    });
}

#[test]
fn create_org_saves_the_org_and_starts_no_agent() {
    let mut h = first_run();
    let events = h.send(Command::CreateOrg {
        template: template(),
        your_name: None,
        duo_cli: CliKind::Claude,
    });
    assert!(events.contains(&Event::OrgCreated));
    assert!(h.calls().is_empty(), "no agent starts");
    let saved = h.store.state.lock().expect("lock").saved_orgs.clone();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].agents().len(), 3);
    assert_eq!(
        h.store.state.lock().expect("lock").marked_last_good.len(),
        1
    );
}

#[test]
fn create_org_sets_your_name_and_duo_cli() {
    let mut h = first_run();
    h.send(Command::CreateOrg {
        template: TemplateInput::Blank,
        your_name: Some(" Duong ".into()),
        duo_cli: CliKind::Opencode,
    });
    let settings = h.store.state.lock().expect("lock").saved_settings.clone();
    assert_eq!(
        settings.last().expect("saved").your_name.as_deref(),
        Some("Duong")
    );
    let org = &h.engine.snapshot().org;
    assert_eq!(org.duo.cli, CliKind::Opencode);
    assert_eq!(org.defaults.cli, CliKind::Opencode);
}

#[test]
fn create_org_refused_when_an_org_exists() {
    let mut h = Harness::new();
    let events = h.send(Command::CreateOrg {
        template: template(),
        your_name: None,
        duo_cli: CliKind::Claude,
    });
    assert!(
        matches!(events.as_slice(), [Event::CommandRejected { .. }]),
        "{events:?}"
    );
    assert!(h.store.state.lock().expect("lock").saved_orgs.is_empty());
}

#[test]
fn first_run_flag_clears_after_create() {
    let mut h = first_run();
    assert!(h.engine.snapshot().first_run.is_some());
    create(&mut h, &template(), None);
    assert!(h.engine.snapshot().first_run.is_none());
}

#[test]
fn create_org_agents_map_matches() {
    let mut h = first_run();
    create(&mut h, &template(), None);
    let names: Vec<String> = h.engine.agents.keys().map(ToString::to_string).collect();
    assert_eq!(names, ["Duo", "squad-1", "squad-2", "squad-3"]);
}

#[test]
fn create_org_updates_the_tracker_org() {
    let mut h = first_run();
    let events = h.send(Command::CreateOrg {
        template: template(),
        your_name: None,
        duo_cli: CliKind::Claude,
    });
    let org = events
        .iter()
        .find_map(|e| match e {
            Event::OrgChanged { org, .. } => Some(org.clone()),
            _ => None,
        })
        .expect("an org-changed event");
    assert_eq!(org.agents().len(), 3);
}

#[test]
fn create_org_probes_directories() {
    let mut h = first_run();
    h.input(Input::DirInventory(Vec::new()));
    let before = h.dir_probes.load(Ordering::SeqCst);
    create(&mut h, &template(), None);
    assert!(
        h.dir_probes.load(Ordering::SeqCst) > before,
        "the new agent's directory is probed"
    );
}

#[test]
fn first_run_snapshot_from_setter() {
    let h = first_run();
    let want: Option<Vec<(CliKind, CliPresence)>> = Some(all_found());
    assert_eq!(h.engine.snapshot().first_run, want);
}
