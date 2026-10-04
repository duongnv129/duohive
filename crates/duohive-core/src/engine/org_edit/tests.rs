//! Tests for org edits, reload and settings: Task 14.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use super::super::Input;
use super::super::harness::{Harness, Setup, test_org};
use crate::cli_kind::CliKind;
use crate::command::{AgentDraft, Command, OrgEdit, RoleDraft, TeamDraft};
use crate::event::Event;
use crate::hook::HookMessage;
use crate::launch::WindowId;
use crate::names::{AgentName, NodeName};
use crate::org::{DuoConfig, Fields, NodeKey, Org, RoleKey};
use crate::ports::{OrgFileError, RuntimeEvent, SessionRow};
use crate::status::AgentState;
use crate::testkit::{RuntimeCall, example_org, n};
use crate::validate::DirStatus;

const BROKEN: &str = "org.toml has an error · fix it first (o)";
const RENAME: &str = "rename only while stopped with no worktrees";

fn nn(s: &str) -> NodeName {
    NodeName::parse(s).unwrap()
}

fn qa() -> RoleKey {
    RoleKey {
        team: Some(nn("Payments")),
        role: nn("QA"),
    }
}

fn edit(h: &mut Harness, e: OrgEdit) -> Vec<Event> {
    h.send(Command::EditOrg(e))
}

fn rejected(ev: &[Event]) -> Vec<String> {
    ev.iter()
        .filter_map(|e| match e {
            Event::CommandRejected { reason } => Some(reason.clone()),
            _ => None,
        })
        .collect()
}

/// The single `OrgChanged` in `ev`; panics otherwise.
fn org_changed(ev: &[Event]) -> (Org, Vec<(String, String)>) {
    let found: Vec<_> = ev
        .iter()
        .filter_map(|e| match e {
            Event::OrgChanged { org, problems } => Some((
                (**org).clone(),
                problems
                    .iter()
                    .map(|(a, p)| (a.to_string(), p.clone()))
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1, "expected one OrgChanged in {ev:?}");
    found.into_iter().next().unwrap()
}

fn saved_orgs(h: &Harness) -> Vec<Org> {
    h.store.state.lock().unwrap().saved_orgs.clone()
}

fn session_names(h: &Harness) -> Vec<String> {
    let st = h.store.state.lock().unwrap();
    st.sessions.keys().map(ToString::to_string).collect()
}

/// Puts a session row for `agent`, as a stopped agent can have after a restart.
fn put_row(h: &mut Harness, agent: &str) {
    let row = SessionRow {
        agent: n(agent),
        token: None,
        window: None,
        status: h.rt(agent).status.clone(),
        last_reply: Some("done".into()),
        task_at_launch: false,
        saw_end_hook: false,
        last_task: None,
    };
    let mut st = h.store.state.lock().unwrap();
    st.sessions.insert(n(agent), row);
}

fn agent_edit(key: &str, name: &str, fields: Fields) -> OrgEdit {
    OrgEdit::EditAgent {
        key: n(key),
        draft: AgentDraft {
            name: n(name),
            fields,
        },
    }
}

fn defaults_edit(org: &Org, name: &str, cli: CliKind) -> OrgEdit {
    OrgEdit::EditOrgDefaults {
        defaults: org.defaults.clone(),
        duo: DuoConfig { name: n(name), cli },
    }
}

fn started(agent: &str, window: &str) -> Input {
    Input::Runtime(RuntimeEvent::Started {
        agent: n(agent),
        window: WindowId(window.into()),
    })
}

fn gone(agent: &str, window: &str) -> Input {
    Input::Runtime(RuntimeEvent::WindowGone {
        agent: n(agent),
        window: WindowId(window.into()),
    })
}

fn busy(agent: &str, token: &str) -> Input {
    Input::Hook(HookMessage {
        token: token.into(),
        agent: agent.into(),
        event: "busy".into(),
        payload: serde_json::json!({}),
    })
}

fn starts(h: &Harness) -> Vec<(AgentName, crate::launch::LaunchCommand)> {
    h.calls()
        .into_iter()
        .filter_map(|c| match c {
            RuntimeCall::Start(a, cmd) => Some((a, cmd)),
            _ => None,
        })
        .collect()
}

#[test]
fn add_agent_saves_org_and_emits_org_changed() {
    let mut h = Harness::new();
    h.input(Input::DirInventory(Vec::new()));
    let fields = Fields {
        directory: Some("~/code/pay-qa-2".into()),
        ..Fields::default()
    };
    let ev = edit(
        &mut h,
        OrgEdit::AddAgent {
            role: qa(),
            agent: AgentDraft {
                name: n("pay-qa-2"),
                fields,
            },
        },
    );
    let saved = saved_orgs(&h);
    assert_eq!(saved.len(), 1);
    let loc = saved[0].find_agent(&n("pay-qa-2")).unwrap();
    assert_eq!(loc.role.name.as_str(), "QA");
    let (org, problems) = org_changed(&ev);
    assert!(org.find_agent(&n("pay-qa-2")).is_some());
    assert!(
        problems.iter().all(|(a, _)| a != "pay-qa-2"),
        "{problems:?}"
    );
    assert_eq!(h.engine.snapshot().org, org);
    assert_eq!(
        h.engine.snapshot().agents[&n("pay-qa-2")].status.state,
        AgentState::Stopped
    );
}

#[test]
fn org_edit_probes_the_directories_it_changes() {
    let gone = PathBuf::from("/home/me/code/gone");
    let mut h = Setup {
        dirs: [(gone, DirStatus::Missing)].into(),
        ..Setup::default()
    }
    .build();
    let to_gone = Fields {
        directory: Some("~/code/gone".into()),
        ..Fields::default()
    };
    // Before the inventory, directories are pending (D26): no probe, no problem.
    let ev = edit(&mut h, agent_edit("pay-qa", "pay-qa", to_gone.clone()));
    assert_eq!(h.dir_probes.load(Ordering::SeqCst), 0);
    assert!(org_changed(&ev).1.is_empty());
    let inventory = ["auth", "billing", "squad", "gone"]
        .iter()
        .map(|d| {
            (
                PathBuf::from(format!("/home/me/code/{d}")),
                DirStatus::Found,
            )
        })
        .chain([(PathBuf::from("/home/me"), DirStatus::Found)])
        .collect();
    h.input(Input::DirInventory(inventory));
    // An edit that changes no directory probes nothing.
    let model = Fields {
        model: Some("opus".into()),
        ..to_gone.clone()
    };
    edit(&mut h, agent_edit("pay-qa", "pay-qa", model));
    assert_eq!(h.dir_probes.load(Ordering::SeqCst), 0);
    // billing-api's directory changes: probed, and the fresh status reaches every row using it.
    let ev = edit(&mut h, agent_edit("billing-api", "billing-api", to_gone));
    assert_eq!(h.dir_probes.load(Ordering::SeqCst), 1);
    let (_, problems) = org_changed(&ev);
    let msg = "✕ ~/code/gone not found · e to fix".to_owned();
    assert_eq!(
        problems,
        vec![("billing-api".into(), msg.clone()), ("pay-qa".into(), msg)]
    );
}

#[test]
fn duplicate_agent_name_rejected() {
    let mut h = Harness::new();
    let ev = edit(
        &mut h,
        OrgEdit::AddAgent {
            role: qa(),
            agent: AgentDraft {
                name: n("auth-service"),
                fields: Fields::default(),
            },
        },
    );
    assert_eq!(
        rejected(&ev),
        vec!["an agent named auth-service already exists"]
    );
    assert!(saved_orgs(&h).is_empty());
    assert_eq!(h.engine.org, test_org());
}

#[test]
fn rename_running_agent_rejected() {
    let mut h = Harness::new();
    h.force("auth-service", AgentState::Working);
    let ev = edit(
        &mut h,
        agent_edit("auth-service", "auth-svc", Fields::default()),
    );
    assert_eq!(rejected(&ev), vec![RENAME]);
    assert_eq!(h.engine.org, test_org());
    assert!(saved_orgs(&h).is_empty());
}

#[test]
fn rename_stopped_agent_moves_session_row() {
    let mut h = Harness::new();
    put_row(&mut h, "pay-qa");
    let fields = h.engine.org.teams[0].roles[1].agents[0].fields.clone();
    let ev = edit(&mut h, agent_edit("pay-qa", "pay-qa-old", fields));
    let (org, _) = org_changed(&ev);
    assert!(org.find_agent(&n("pay-qa-old")).is_some());
    assert!(org.find_agent(&n("pay-qa")).is_none());
    assert_eq!(session_names(&h), vec!["pay-qa-old"]);
    let st = h.store.state.lock().unwrap();
    assert_eq!(st.sessions[&n("pay-qa-old")].agent, n("pay-qa-old"));
    assert_eq!(
        st.sessions[&n("pay-qa-old")].last_reply.as_deref(),
        Some("done")
    );
}

#[test]
fn delete_running_agent_rejected() {
    let mut h = Harness::new();
    h.force("auth-service", AgentState::Working);
    let ev = edit(&mut h, OrgEdit::Delete(NodeKey::Agent(n("auth-service"))));
    assert_eq!(rejected(&ev), vec!["stop auth-service first"]);
    assert_eq!(h.engine.org, test_org());
}

#[test]
fn delete_role_with_running_agent_rejected() {
    let mut h = Harness::new();
    h.force("billing-api", AgentState::Waiting);
    let backend = RoleKey {
        team: Some(nn("Payments")),
        role: nn("Backend"),
    };
    let ev = edit(&mut h, OrgEdit::Delete(NodeKey::Role(backend)));
    assert_eq!(rejected(&ev), vec!["stop billing-api first"]);
    let ev = edit(&mut h, OrgEdit::Delete(NodeKey::Team(nn("Payments"))));
    assert_eq!(rejected(&ev), vec!["stop billing-api first"]);
    assert_eq!(h.engine.org, test_org());
    assert!(saved_orgs(&h).is_empty());
}

#[test]
fn delete_stopped_agent_removes_session() {
    let mut h = Harness::new();
    put_row(&mut h, "pay-qa");
    let ev = edit(&mut h, OrgEdit::Delete(NodeKey::Agent(n("pay-qa"))));
    let (org, _) = org_changed(&ev);
    assert!(org.find_agent(&n("pay-qa")).is_none());
    assert_eq!(org.teams[0].roles[1].agents.len(), 0);
    let st = h.store.state.lock().unwrap();
    assert!(st.deleted_sessions.contains(&n("pay-qa")));
    assert!(!st.sessions.contains_key(&n("pay-qa")));
    drop(st);
    assert!(!h.engine.agents.contains_key(&n("pay-qa")));
}

#[test]
fn delete_stopped_team_removes_its_agents() {
    let mut h = Harness::new();
    let ev = edit(&mut h, OrgEdit::Delete(NodeKey::Team(nn("Payments"))));
    let (org, _) = org_changed(&ev);
    assert!(org.teams.is_empty());
    let st = h.store.state.lock().unwrap();
    for a in ["auth-service", "billing-api", "pay-qa"] {
        assert!(st.deleted_sessions.contains(&n(a)), "{a}");
    }
}

#[test]
fn cannot_delete_duo() {
    let mut h = Harness::new();
    for key in [NodeKey::Agent(n("Duo")), NodeKey::Duo] {
        let ev = edit(&mut h, OrgEdit::Delete(key));
        assert_eq!(rejected(&ev), vec!["Duo can't be deleted"]);
    }
    assert!(saved_orgs(&h).is_empty());
}

#[test]
fn edit_running_agent_applies_next_start() {
    let mut h = Harness::new();
    h.start("auth-service");
    h.input(started("auth-service", "@1"));
    h.force("auth-service", AgentState::Working);
    let calls_before = h.calls().len();
    let fields = Fields {
        model: Some("opus".into()),
        ..h.engine.org.teams[0].roles[0].agents[0].fields.clone()
    };
    let ev = edit(&mut h, agent_edit("auth-service", "auth-service", fields));
    org_changed(&ev);
    assert_eq!(h.calls().len(), calls_before, "no runtime call");
    assert_eq!(h.state("auth-service"), AgentState::Working);
    h.send(Command::StopAgent {
        agent: n("auth-service"),
    });
    h.input(gone("auth-service", "@1"));
    assert_eq!(h.state("auth-service"), AgentState::Stopped);
    h.start("auth-service");
    let launches = h.claude.launches.lock().unwrap();
    assert_eq!(launches.len(), 2);
    assert_eq!(launches[0].agent.model, None);
    assert_eq!(launches[1].agent.model.as_deref(), Some("opus"));
}

#[test]
fn add_role_with_agent_count_creates_named_agents() {
    let mut h = Harness::new();
    let role = RoleDraft {
        name: nn("Frontend"),
        defaults: Fields::default(),
    };
    let ev = edit(
        &mut h,
        OrgEdit::AddRole {
            team: Some(nn("Payments")),
            role: role.clone(),
            agents: 2,
        },
    );
    let (org, _) = org_changed(&ev);
    let added = &org.teams[0].roles[2];
    assert_eq!(added.name.as_str(), "Frontend");
    let names: Vec<_> = added.agents.iter().map(|a| a.name.to_string()).collect();
    assert_eq!(names, ["payments-frontend-1", "payments-frontend-2"]);
    assert!(added.agents.iter().all(|a| a.fields == Fields::default()));
    let ev = edit(
        &mut h,
        OrgEdit::AddRole {
            team: None,
            role,
            agents: 1,
        },
    );
    let (org, _) = org_changed(&ev);
    let root = org.roles.last().unwrap();
    assert_eq!(root.name.as_str(), "Frontend");
    assert_eq!(root.agents[0].name, n("frontend-1"));
}

#[test]
fn add_team_and_edit_team_and_role() {
    let mut h = Harness::new();
    let team = TeamDraft {
        name: nn("Growth"),
        defaults: Fields::default(),
    };
    org_changed(&edit(&mut h, OrgEdit::AddTeam(team)));
    let renamed = TeamDraft {
        name: nn("Growth Ops"),
        defaults: Fields {
            model: Some("sonnet".into()),
            ..Fields::default()
        },
    };
    let ev = edit(
        &mut h,
        OrgEdit::EditTeam {
            key: nn("Growth"),
            draft: renamed.clone(),
        },
    );
    let (org, _) = org_changed(&ev);
    assert_eq!(org.teams[1].name, renamed.name);
    assert_eq!(org.teams[1].defaults, renamed.defaults);
    let draft = RoleDraft {
        name: nn("Testing"),
        defaults: Fields::default(),
    };
    let ev = edit(&mut h, OrgEdit::EditRole { key: qa(), draft });
    let (org, _) = org_changed(&ev);
    assert_eq!(org.teams[0].roles[1].name.as_str(), "Testing");
    let ev = edit(
        &mut h,
        OrgEdit::EditTeam {
            key: nn("Nobody"),
            draft: renamed,
        },
    );
    assert_eq!(rejected(&ev), vec!["no team named Nobody"]);
}

#[test]
fn edits_refused_while_org_file_invalid() {
    let mut h = Harness::new();
    h.engine.org_error = Some(OrgFileError {
        line: Some(3),
        message: "expected `=`".into(),
    });
    let team = TeamDraft {
        name: nn("Growth"),
        defaults: Fields::default(),
    };
    let ev = edit(&mut h, OrgEdit::AddTeam(team));
    assert_eq!(rejected(&ev), vec![BROKEN]);
    let ev = edit(&mut h, OrgEdit::Delete(NodeKey::Agent(n("pay-qa"))));
    assert_eq!(rejected(&ev), vec![BROKEN]);
    assert!(saved_orgs(&h).is_empty());
    assert_eq!(h.engine.org, test_org());
}

#[test]
fn org_save_failure_changes_nothing() {
    let mut h = Harness::new();
    put_row(&mut h, "pay-qa");
    h.store.state.lock().unwrap().fail_next_save = true;
    let fields = h.engine.org.teams[0].roles[1].agents[0].fields.clone();
    let ev = edit(&mut h, agent_edit("pay-qa", "pay-qa-old", fields));
    assert_eq!(rejected(&ev), vec!["couldn't save org.toml: disk full"]);
    assert_eq!(h.engine.org, test_org());
    assert!(h.engine.agents.contains_key(&n("pay-qa")));
    assert_eq!(session_names(&h), vec!["pay-qa"]);
}

mod duo_reload_settings;

#[test]
fn preview_edit_applies_and_validates() {
    let org = example_org();
    let growth = OrgEdit::AddTeam(TeamDraft {
        name: nn("Growth"),
        defaults: Fields::default(),
    });
    let after = super::super::preview_edit(&org, &growth).unwrap();
    assert_eq!(after.teams.len(), 2);
    assert_eq!(org.teams.len(), 1, "the org itself is untouched");
    let dup = OrgEdit::AddTeam(TeamDraft {
        name: nn("Payments"),
        defaults: Fields::default(),
    });
    let errs = super::super::preview_edit(&org, &dup).unwrap_err();
    assert_eq!(errs[0].path, "team[Payments].name");
    assert_eq!(errs[0].message, "a team named Payments already exists");
    let missing = OrgEdit::EditTeam {
        key: nn("Nope"),
        draft: TeamDraft {
            name: nn("Nope"),
            defaults: Fields::default(),
        },
    };
    let errs = super::super::preview_edit(&org, &missing).unwrap_err();
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].path, "");
    assert!(errs[0].message.contains("Nope"), "{}", errs[0].message);
}
