//! Tests for listing sessions to resume (V11-D3, V11-D4; TD §5.6).

use std::path::PathBuf;

use super::super::Input;
use super::super::harness::{Harness, Setup, all_found};
use crate::adapter::CliPresence;
use crate::cli_kind::CliKind;
use crate::command::Command;
use crate::event::Event;
use crate::hook::HookMessage;
use crate::launch::ResumeTarget;
use crate::names::AgentName;
use crate::session_catalog::{CatalogError, SessionInfo};
use crate::status::AgentState;
use crate::templates::{SquadInput, TemplateInput};
use crate::testkit::{RuntimeCall, n};
use crate::tool::{CallId, ToolCall, ToolOk};
use crate::worktree::{BaseRef, TaskId, WorktreeRow, WorktreeState};
use serde_json::json;

fn list(h: &mut Harness, agent: &str) -> Vec<Event> {
    h.send(Command::ListSessions { agent: n(agent) })
}

fn rejected(events: &[Event]) -> Option<&str> {
    events.iter().find_map(|e| match e {
        Event::CommandRejected { reason } => Some(reason.as_str()),
        _ => None,
    })
}

fn info(id: &str) -> SessionInfo {
    SessionInfo {
        id: id.into(),
        title: "t".into(),
        updated_ms: 1,
        branch: None,
        size_bytes: 0,
        dir: PathBuf::from("/home/me/code/pay-qa"),
        worktree: None,
        in_use_by: None,
    }
}

fn answer(h: &mut Harness, agent: &str, ids: &[&str]) -> Vec<SessionInfo> {
    let events = h.input(Input::SessionsListed {
        agent: n(agent),
        result: Ok(ids.iter().map(|id| info(id)).collect()),
    });
    events
        .into_iter()
        .find_map(|e| match e {
            Event::SessionsListed { agent: a, sessions } if a == n(agent) => Some(sessions),
            _ => None,
        })
        .expect("SessionsListed")
}

fn holds(h: &mut Harness, agent: &str, id: &str) {
    h.rt_mut(agent).cli_session_id = Some(id.into());
}

fn in_use(sessions: &[SessionInfo]) -> Vec<Option<AgentName>> {
    sessions.iter().map(|s| s.in_use_by.clone()).collect()
}

fn worktree_row(agent: &str, task: &str) -> WorktreeRow {
    WorktreeRow {
        agent: n(agent),
        task: TaskId(task.into()),
        repo: "/home/me/code/pay-qa".into(),
        path: PathBuf::from("/data/worktrees").join(agent).join(task),
        branch: format!("duohive/{agent}/{task}"),
        created_at: crate::time::Millis(1),
        created_from: "abc".into(),
        state: WorktreeState::Unchecked,
        checked_at: None,
        base: Some(BaseRef("origin/main".into())),
    }
}

#[test]
fn list_asks_for_the_agent_directory_and_each_kept_worktree() {
    let mut h = Harness::new();
    let row = worktree_row("pay-qa", "t1");
    let other = worktree_row("billing-api", "t2");
    {
        let mut st = h.store.state.lock().unwrap();
        st.worktrees
            .insert((row.agent.clone(), row.task.clone()), row.clone());
        st.worktrees
            .insert((other.agent.clone(), other.task.clone()), other);
    }
    let events = list(&mut h, "pay-qa");
    assert!(events.is_empty(), "{events:?}");
    let reqs = h.session_requests.lock().unwrap().clone();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].agent, n("pay-qa"));
    assert_eq!(reqs[0].cli, CliKind::Claude);
    assert_eq!(
        reqs[0].dirs,
        vec![
            (PathBuf::from("/home/me/code/pay-qa"), None),
            (row.path, Some(TaskId("t1".into()))),
        ]
    );
}

#[test]
fn list_refused_while_running() {
    let mut h = Harness::new();
    h.start("pay-qa");
    let events = list(&mut h, "pay-qa");
    assert_eq!(
        rejected(&events),
        Some("pay-qa is running \u{b7} stop it first (s)")
    );
    assert!(h.session_requests.lock().unwrap().is_empty());
}

#[test]
fn list_refused_for_unknown_agent() {
    let mut h = Harness::new();
    let events = list(&mut h, "nobody");
    assert_eq!(rejected(&events), Some("no agent named nobody"));
    assert!(h.session_requests.lock().unwrap().is_empty());
}

#[test]
fn list_refused_while_worktree_is_being_created() {
    use crate::command::Command as C;
    use crate::worktree::RunIn;
    let mut h = Setup {
        dirs: std::collections::HashMap::from([(
            PathBuf::from("/home/me/code/pay-qa"),
            crate::validate::DirStatus::GitRepo,
        )]),
        ..Setup::default()
    }
    .build();
    h.send(C::GiveWork {
        agent: n("pay-qa"),
        task: "Add retry cases".into(),
        run: RunIn::NewWorktree,
    });
    let events = list(&mut h, "pay-qa");
    assert_eq!(
        rejected(&events),
        Some("pay-qa is getting a worktree ready")
    );
    assert!(h.session_requests.lock().unwrap().is_empty());
}

#[test]
fn list_for_missing_cli_answers_unreadable_not_found() {
    // The inventory has arrived and lacks the CLI: that is "not found",
    // unlike before any inventory (see `list_waits_for_the_cli_inventory`).
    let mut inventory = all_found();
    for (k, p) in &mut inventory {
        if *k == CliKind::Claude {
            *p = CliPresence::Missing;
        }
    }
    let mut h = Setup {
        inventory: Some(inventory),
        ..Setup::default()
    }
    .build();
    let events = list(&mut h, "pay-qa");
    assert_eq!(
        events,
        vec![Event::SessionsUnreadable {
            agent: n("pay-qa"),
            reason: "claude not found \u{b7} install it or change its CLI (e)".into(),
        }]
    );
    assert!(h.session_requests.lock().unwrap().is_empty());
}

fn before_inventory() -> Harness {
    Setup {
        inventory: None,
        ..Setup::default()
    }
    .build()
}

#[test]
fn list_waits_for_the_cli_inventory_like_a_start() {
    let mut h = before_inventory();
    let events = list(&mut h, "pay-qa");
    assert!(events.is_empty(), "{events:?}");
    assert!(h.session_requests.lock().unwrap().is_empty());
    let events = h.input(Input::CliInventory(all_found()));
    assert!(events.is_empty(), "{events:?}");
    let reqs = h.session_requests.lock().unwrap().clone();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].agent, n("pay-qa"));
    assert_eq!(
        reqs[0].dirs,
        vec![(PathBuf::from("/home/me/code/pay-qa"), None)]
    );
}

#[test]
fn repeated_list_before_the_inventory_queues_once() {
    let mut h = before_inventory();
    list(&mut h, "pay-qa");
    list(&mut h, "pay-qa");
    h.input(Input::CliInventory(all_found()));
    assert_eq!(h.session_requests.lock().unwrap().len(), 1);
}

#[test]
fn list_refused_for_an_agent_without_a_directory() {
    let mut setup = Setup::default();
    setup.org.teams[0].roles[1].agents[0].fields.directory = None;
    let mut h = setup.build();
    let events = list(&mut h, "pay-qa");
    assert_eq!(rejected(&events), Some("no directory set \u{b7} e to fix"));
    assert!(h.session_requests.lock().unwrap().is_empty());
}

#[test]
fn in_use_marks_a_session_held_by_another_running_agent() {
    let mut h = Harness::new();
    h.start("billing-api");
    holds(&mut h, "billing-api", "s1");
    let got = answer(&mut h, "pay-qa", &["s1", "s2"]);
    assert_eq!(in_use(&got), vec![Some(n("billing-api")), None]);
}

#[test]
fn in_use_marks_errored_agent_with_window_up() {
    let mut h = Harness::new();
    h.start("billing-api");
    holds(&mut h, "billing-api", "s1");
    h.force_errored("billing-api", "start timed out", Some("@9"));
    let got = answer(&mut h, "pay-qa", &["s1"]);
    assert_eq!(in_use(&got), vec![Some(n("billing-api"))]);
}

#[test]
fn in_use_ignores_stopped_and_crash_errored_agents() {
    let mut h = Harness::new();
    h.start("billing-api");
    holds(&mut h, "billing-api", "s1");
    h.force("billing-api", AgentState::Stopped);
    h.start("pay-qa");
    holds(&mut h, "pay-qa", "s2");
    h.force_errored("pay-qa", "crashed", None);
    let got = answer(&mut h, "squad-lead", &["s1", "s2"]);
    assert_eq!(in_use(&got), vec![None, None]);
}

#[test]
fn own_session_is_never_in_use_by_itself() {
    let mut h = Harness::new();
    h.start("pay-qa");
    holds(&mut h, "pay-qa", "s1");
    h.force("pay-qa", AgentState::Stopped);
    h.rt_mut("pay-qa").cli_session_id = Some("s1".into());
    h.force_errored("pay-qa", "x", Some("@3"));
    let got = answer(&mut h, "pay-qa", &["s1"]);
    assert_eq!(in_use(&got), vec![None]);
}

#[test]
fn unreadable_answer_becomes_sessions_unreadable() {
    let mut h = Harness::new();
    let events = h.input(Input::SessionsListed {
        agent: n("pay-qa"),
        result: Err(CatalogError::Unreadable("no such store".into())),
    });
    assert_eq!(
        events,
        vec![Event::SessionsUnreadable {
            agent: n("pay-qa"),
            reason: "no such store".into(),
        }]
    );
}

// ---- Resume (Task R.8) ----

fn resume(h: &mut Harness, agent: &str, id: &str) -> Vec<Event> {
    h.send(Command::ResumeSession {
        agent: n(agent),
        session_id: id.into(),
    })
}

/// The picker was opened for `agent` and showed `sessions`.
fn listed(h: &mut Harness, agent: &str, sessions: Vec<SessionInfo>) {
    h.input(Input::SessionsListed {
        agent: n(agent),
        result: Ok(sessions),
    });
}

fn in_dir(id: &str, dir: &str) -> SessionInfo {
    SessionInfo {
        dir: PathBuf::from(dir),
        ..info(id)
    }
}

fn in_worktree(id: &str, agent: &str, task: &str) -> SessionInfo {
    SessionInfo {
        dir: worktree_row(agent, task).path,
        worktree: Some(TaskId(task.into())),
        ..info(id)
    }
}

fn keep_row(h: &mut Harness, row: &WorktreeRow) {
    h.store
        .state
        .lock()
        .unwrap()
        .worktrees
        .insert((row.agent.clone(), row.task.clone()), row.clone());
}

fn launched(h: &Harness) -> Vec<crate::launch::LaunchSpec> {
    h.claude.launches.lock().unwrap().clone()
}

fn hook_with(h: &mut Harness, agent: &str, event: &str, payload: serde_json::Value) {
    let token = h.rt(agent).token.clone().unwrap();
    h.input(Input::Hook(HookMessage {
        sent_at_ms: 0,
        token: token.0,
        agent: agent.into(),
        event: event.into(),
        payload,
    }));
}

#[test]
fn resume_launches_with_the_session_and_no_task() {
    let mut h = Harness::new();
    listed(&mut h, "pay-qa", vec![info("s1")]);
    let events = resume(&mut h, "pay-qa", "s1");
    assert_eq!(rejected(&events), None, "{events:?}");
    let specs = launched(&h);
    let [spec] = specs.as_slice() else {
        panic!("expected one launch, got {specs:?}")
    };
    assert_eq!(spec.task, None);
    assert_eq!(
        spec.resume,
        Some(ResumeTarget {
            session_id: "s1".into(),
            cwd: PathBuf::from("/home/me/code/pay-qa"),
        })
    );
    assert_eq!(spec.cwd, None);
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
}

#[test]
fn resume_runs_in_the_sessions_own_directory() {
    let mut h = Harness::new();
    listed(
        &mut h,
        "pay-qa",
        vec![in_dir("s1", "/home/me/code/elsewhere")],
    );
    resume(&mut h, "pay-qa", "s1");
    let specs = launched(&h);
    assert_eq!(
        specs[0].resume.as_ref().map(|r| r.cwd.clone()),
        Some(PathBuf::from("/home/me/code/elsewhere"))
    );
}

#[test]
fn resume_in_a_kept_worktree_marks_it_in_use_again() {
    let mut h = Harness::new();
    let row = worktree_row("pay-qa", "t1");
    keep_row(&mut h, &row);
    listed(&mut h, "pay-qa", vec![in_worktree("s1", "pay-qa", "t1")]);
    let events = resume(&mut h, "pay-qa", "s1");
    assert_eq!(rejected(&events), None, "{events:?}");
    assert_eq!(h.rt("pay-qa").worktree, Some(TaskId("t1".into())));
    let st = h.store.state.lock().unwrap();
    assert_eq!(
        st.sessions[&n("pay-qa")].worktree,
        Some(TaskId("t1".into()))
    );
    assert_eq!(
        st.worktrees[&(n("pay-qa"), TaskId("t1".into()))].state,
        WorktreeState::InUse
    );
    drop(st);
    let specs = launched(&h);
    assert_eq!(specs[0].cwd, Some(row.path.clone()));
    assert_eq!(
        specs[0].resume.as_ref().map(|r| r.cwd.clone()),
        Some(row.path)
    );
}

#[test]
fn resume_on_errored_agent_kills_its_window_first() {
    let mut h = Harness::new();
    h.force_errored("pay-qa", "start timed out", Some("@9"));
    listed(&mut h, "pay-qa", vec![info("s1")]);
    resume(&mut h, "pay-qa", "s1");
    let calls = h.calls();
    let kill = calls
        .iter()
        .position(|c| matches!(c, RuntimeCall::Kill(..)));
    let start = calls
        .iter()
        .position(|c| matches!(c, RuntimeCall::Start(..)));
    assert!(
        kill.is_some() && start.is_some() && kill < start,
        "{calls:?}"
    );
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
}

#[test]
fn resume_refused_while_running() {
    let mut h = Harness::new();
    listed(&mut h, "pay-qa", vec![info("s1")]);
    h.start("pay-qa");
    let events = resume(&mut h, "pay-qa", "s1");
    assert_eq!(
        rejected(&events),
        Some("pay-qa is running \u{b7} stop it first (s)")
    );
    assert_eq!(launched(&h).len(), 1);
}

#[test]
fn resume_refused_when_session_in_use_by_another_agent() {
    let mut h = Harness::new();
    listed(&mut h, "pay-qa", vec![info("s1")]);
    h.start("billing-api");
    holds(&mut h, "billing-api", "s1");
    let events = resume(&mut h, "pay-qa", "s1");
    assert_eq!(rejected(&events), Some("billing-api is using this session"));
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
}

#[test]
fn second_resume_of_same_session_is_refused() {
    let mut h = Harness::new();
    listed(&mut h, "pay-qa", vec![info("s1")]);
    listed(&mut h, "billing-api", vec![info("s1")]);
    let first = resume(&mut h, "pay-qa", "s1");
    assert_eq!(rejected(&first), None, "{first:?}");
    let second = resume(&mut h, "billing-api", "s1");
    assert_eq!(rejected(&second), Some("pay-qa is using this session"));
    assert_eq!(launched(&h).len(), 1);
}

#[test]
fn resume_rejects_odd_session_id() {
    let mut h = Harness::new();
    for bad in [
        "",
        "-x",
        "--help",
        "-",
        "a b",
        "../x",
        "a;b",
        &"x".repeat(129),
    ] {
        let events = resume(&mut h, "pay-qa", bad);
        assert_eq!(
            rejected(&events),
            Some("that isn't a session id"),
            "{bad:?}"
        );
    }
    assert!(launched(&h).is_empty());
}

#[test]
fn resume_accepts_real_session_id_shapes() {
    let mut h = Harness::new();
    for ok in ["3f2a9c1e-0b7d-4e55-9a11-6c2d8f0e1b34", "ses_abc123"] {
        listed(&mut h, "pay-qa", vec![info(ok)]);
        let events = resume(&mut h, "pay-qa", ok);
        assert_eq!(rejected(&events), None, "{ok}");
        h.force("pay-qa", AgentState::Stopped);
    }
}

#[test]
fn resume_records_the_launch_session_id() {
    let mut h = Harness::new();
    listed(&mut h, "pay-qa", vec![info("s1")]);
    resume(&mut h, "pay-qa", "s1");
    assert_eq!(h.rt("pay-qa").cli_session_id.as_deref(), Some("s1"));
    let stored = |h: &Harness| {
        h.store.state.lock().unwrap().sessions[&n("pay-qa")]
            .cli_session_id
            .clone()
    };
    assert_eq!(stored(&h).as_deref(), Some("s1"));
    hook_with(&mut h, "pay-qa", "busy", json!({"session_id": "other"}));
    assert_eq!(h.rt("pay-qa").cli_session_id.as_deref(), Some("s1"));
    assert_eq!(stored(&h).as_deref(), Some("s1"));
}

#[test]
fn queued_messages_arrive_at_the_first_rest_after_resume() {
    let mut h = Harness::new();
    h.start("squad-1");
    hook_with(&mut h, "squad-1", "session-start", json!({}));
    let token = h.rt("squad-1").token.clone().unwrap();
    let out = h.send(Command::Tool {
        call_id: CallId(7),
        token,
        call: ToolCall::SendMessage {
            to: n("pay-qa"),
            text: "hi".into(),
        },
    });
    assert!(
        out.iter().any(|e| matches!(
            e,
            Event::ToolResult {
                result: Ok(ToolOk::Queued { .. }),
                ..
            }
        )),
        "{out:?}"
    );
    listed(&mut h, "pay-qa", vec![info("s1")]);
    let pastes = |h: &Harness| {
        h.calls()
            .into_iter()
            .filter(|c| matches!(c, RuntimeCall::Paste(a, ..) if *a == n("pay-qa")))
            .count()
    };
    resume(&mut h, "pay-qa", "s1");
    assert_eq!(pastes(&h), 0);
    hook_with(&mut h, "pay-qa", "session-start", json!({}));
    assert_eq!(pastes(&h), 1);
}

#[test]
fn resume_while_cli_not_found_errors_like_any_start() {
    let mut inventory = all_found();
    for (k, p) in &mut inventory {
        if *k == CliKind::Claude {
            *p = CliPresence::Missing;
        }
    }
    let mut h = Setup {
        inventory: Some(inventory),
        ..Setup::default()
    }
    .build();
    listed(&mut h, "pay-qa", vec![info("s1")]);
    resume(&mut h, "pay-qa", "s1");
    assert_eq!(h.state("pay-qa"), AgentState::Errored);
    assert_eq!(
        h.rt("pay-qa").status.error.as_deref(),
        Some("claude not found \u{b7} install it or change its CLI (e)")
    );
    assert!(launched(&h).is_empty());
}

#[test]
fn resume_blocked_while_replacing() {
    let mut h = Harness::new();
    listed(&mut h, "pay-qa", vec![info("s1")]);
    h.start("billing-api");
    h.force("billing-api", AgentState::Working);
    h.send(Command::NewOrgFromTemplate {
        template: TemplateInput::AgentSquad(SquadInput {
            size: 3,
            directory: "~/code/app".into(),
            cli: CliKind::Claude,
            worktrees: false,
            prompt: None,
        }),
    });
    let events = resume(&mut h, "pay-qa", "s1");
    assert_eq!(rejected(&events), Some("the org is being replaced"));
    // billing-api's own start is the one launch; the resume adds none.
    assert_eq!(launched(&h).len(), 1);
}

#[test]
fn unknown_agent_answer_is_not_cached() {
    let mut h = Harness::new();
    listed(&mut h, "ghost", vec![info("s1")]);
    assert!(!h.engine.last_listed.contains_key(&n("ghost")));
    listed(&mut h, "pay-qa", vec![info("s1")]);
    assert!(h.engine.last_listed.contains_key(&n("pay-qa")));
}

#[test]
fn resume_waits_for_the_cli_inventory_like_a_start() {
    let mut h = Setup {
        inventory: None,
        ..Setup::default()
    }
    .build();
    listed(&mut h, "pay-qa", vec![info("s1")]);
    let events = resume(&mut h, "pay-qa", "s1");
    assert_eq!(rejected(&events), None);
    assert!(launched(&h).is_empty());
    h.input(Input::CliInventory(all_found()));
    assert_eq!(launched(&h).len(), 1);
}

#[test]
fn resume_rejects_unlisted_session() {
    let mut h = Harness::new();
    let events = resume(&mut h, "pay-qa", "s1");
    assert_eq!(
        rejected(&events),
        Some("that session isn't in the list \u{b7} open the picker again")
    );
    listed(&mut h, "pay-qa", vec![info("s2")]);
    let events = resume(&mut h, "pay-qa", "s1");
    assert_eq!(
        rejected(&events),
        Some("that session isn't in the list \u{b7} open the picker again")
    );
    assert!(launched(&h).is_empty());
}

#[test]
fn resume_refused_for_worktree_being_removed() {
    let mut h = Harness::new();
    let mut row = worktree_row("pay-qa", "t1");
    row.state = WorktreeState::Removing;
    keep_row(&mut h, &row);
    listed(&mut h, "pay-qa", vec![in_worktree("s1", "pay-qa", "t1")]);
    let events = resume(&mut h, "pay-qa", "s1");
    assert_eq!(
        rejected(&events),
        Some("pay-qa's worktree is being removed")
    );
    // A row that is gone altogether is a stale list.
    h.store.state.lock().unwrap().worktrees.clear();
    let events = resume(&mut h, "pay-qa", "s1");
    assert_eq!(
        rejected(&events),
        Some("that session isn't in the list \u{b7} open the picker again")
    );
    assert!(launched(&h).is_empty());
}

fn requests(h: &Harness) -> usize {
    h.session_requests.lock().unwrap().len()
}

fn edit_fields(h: &mut Harness, agent: &str, change: impl FnOnce(&mut crate::org::Fields)) {
    let mut fields = h
        .engine
        .org
        .find_agent(&n(agent))
        .expect("agent")
        .agent
        .fields
        .clone();
    change(&mut fields);
    let events = h.send(Command::EditOrg(crate::command::OrgEdit::EditAgent {
        key: n(agent),
        draft: crate::command::AgentDraft {
            name: n(agent),
            fields,
        },
    }));
    assert_eq!(rejected(&events), None, "{events:?}");
}

fn unreadable(events: &[Event]) -> Option<&str> {
    events.iter().find_map(|e| match e {
        Event::SessionsUnreadable { reason, .. } => Some(reason.as_str()),
        _ => None,
    })
}

#[test]
fn second_list_while_one_is_pending_sends_no_second_request() {
    let mut h = Harness::new();
    assert!(list(&mut h, "pay-qa").is_empty());
    let again = list(&mut h, "pay-qa");
    assert!(again.is_empty(), "{again:?}");
    assert_eq!(requests(&h), 1);
}

#[test]
fn pending_is_cleared_by_the_answer_so_the_next_list_runs() {
    let mut h = Harness::new();
    list(&mut h, "pay-qa");
    answer(&mut h, "pay-qa", &["s1"]);
    list(&mut h, "pay-qa");
    assert_eq!(requests(&h), 2);
}

#[test]
fn pending_is_cleared_by_an_error_answer() {
    let mut h = Harness::new();
    list(&mut h, "pay-qa");
    let events = h.input(Input::SessionsListed {
        agent: n("pay-qa"),
        result: Err(CatalogError::Unreadable("boom".into())),
    });
    assert_eq!(unreadable(&events), Some("boom"));
    list(&mut h, "pay-qa");
    assert_eq!(requests(&h), 2);
}

const CHANGED: &str = "pay-qa changed while its sessions were being read \u{b7} press r again";

#[test]
fn answer_after_the_agents_cli_changed_is_dropped_with_a_note() {
    let mut h = Harness::new();
    list(&mut h, "pay-qa");
    edit_fields(&mut h, "pay-qa", |f| f.cli = Some(CliKind::Opencode));
    let events = h.input(Input::SessionsListed {
        agent: n("pay-qa"),
        result: Ok(vec![info("s1")]),
    });
    assert_eq!(unreadable(&events), Some(CHANGED), "{events:?}");
    assert!(!h.engine.last_listed.contains_key(&n("pay-qa")));
    list(&mut h, "pay-qa");
    assert_eq!(requests(&h), 2);
}

#[test]
fn answer_after_the_agents_directory_changed_is_dropped_with_a_note() {
    let mut h = Harness::new();
    list(&mut h, "pay-qa");
    edit_fields(&mut h, "pay-qa", |f| {
        f.directory = Some("~/code/other".into());
    });
    let events = h.input(Input::SessionsListed {
        agent: n("pay-qa"),
        result: Ok(vec![info("s1")]),
    });
    assert_eq!(unreadable(&events), Some(CHANGED), "{events:?}");
    assert!(!h.engine.last_listed.contains_key(&n("pay-qa")));
}

#[test]
fn resume_refuses_a_cached_list_from_before_a_cli_change() {
    let mut h = Harness::new();
    list(&mut h, "pay-qa");
    answer(&mut h, "pay-qa", &["s1"]);
    edit_fields(&mut h, "pay-qa", |f| f.cli = Some(CliKind::Opencode));
    let events = resume(&mut h, "pay-qa", "s1");
    assert_eq!(
        rejected(&events),
        Some("that session isn't in the list \u{b7} open the picker again")
    );
    assert!(launched(&h).is_empty());
    assert!(!h.engine.last_listed.contains_key(&n("pay-qa")));
}

#[test]
fn removed_agent_loses_its_pending_and_cached_state() {
    let mut h = Harness::new();
    list(&mut h, "pay-qa");
    answer(&mut h, "pay-qa", &["s1"]);
    list(&mut h, "billing-api");
    list(&mut h, "pay-qa");
    assert!(h.engine.pending_lists.contains_key(&n("billing-api")));
    let events = h.send(Command::EditOrg(crate::command::OrgEdit::Delete(
        crate::org::NodeKey::Agent(n("billing-api")),
    )));
    assert_eq!(rejected(&events), None, "{events:?}");
    assert!(!h.engine.pending_lists.contains_key(&n("billing-api")));
    let events = h.send(Command::EditOrg(crate::command::OrgEdit::Delete(
        crate::org::NodeKey::Agent(n("pay-qa")),
    )));
    assert_eq!(rejected(&events), None, "{events:?}");
    assert!(!h.engine.last_listed.contains_key(&n("pay-qa")));
    assert!(!h.engine.pending_lists.contains_key(&n("pay-qa")));
}
