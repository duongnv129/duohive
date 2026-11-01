//! Tests for listing sessions to resume (V11-D3, V11-D4; TD §5.6).

use std::path::PathBuf;

use super::super::Input;
use super::super::harness::{Harness, Setup, all_found};
use crate::adapter::CliPresence;
use crate::cli_kind::CliKind;
use crate::command::Command;
use crate::event::Event;
use crate::names::AgentName;
use crate::session_catalog::{CatalogError, SessionInfo};
use crate::status::AgentState;
use crate::testkit::n;
use crate::worktree::{BaseRef, TaskId, WorktreeRow, WorktreeState};

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
