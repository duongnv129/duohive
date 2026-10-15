//! Tests for Give work in a new worktree: Task 2.12 (P2-D7, P2-D20, P2-D21).

use std::collections::HashMap;
use std::path::PathBuf;

use super::super::Input;
use super::super::harness::{Harness, Setup, all_found};
use crate::event::Event;
use crate::hook::HookMessage;
use crate::ports::{CreateWorktree, GitError, GitEvent};
use crate::status::AgentState;
use crate::testkit::{GitCall, RuntimeCall, n};
use crate::validate::DirStatus;
use crate::worktree::{BaseRef, RunIn, TaskId, WorktreeState};

const TASK: &str = "Add retry cases";
const ID: &str = "add-retry-cases";
const REPO: &str = "/home/me/code/pay-qa";

/// A harness where pay-qa's directory is a git repo.
fn in_repo() -> Harness {
    Setup {
        dirs: HashMap::from([(PathBuf::from(REPO), DirStatus::GitRepo)]),
        ..Setup::default()
    }
    .build()
}

fn rejected(events: &[Event], reason: &str) -> bool {
    events
        .iter()
        .any(|e| matches!(e, Event::CommandRejected { reason: r } if r == reason))
}

fn creates(h: &Harness) -> Vec<CreateWorktree> {
    h.git_calls()
        .into_iter()
        .filter_map(|c| match c {
            GitCall::Create(req) => Some(req),
            _ => None,
        })
        .collect()
}

fn starts(h: &Harness) -> usize {
    h.calls()
        .iter()
        .filter(|c| matches!(c, RuntimeCall::Start(..)))
        .count()
}

fn created(agent: &str, id: &str, result: Result<(BaseRef, String), GitError>) -> Input {
    Input::Git(GitEvent::Created {
        agent: n(agent),
        task: TaskId(id.into()),
        result,
    })
}

fn ok() -> Result<(BaseRef, String), GitError> {
    Ok((BaseRef("origin/main".into()), "abc123".into()))
}

fn hook(agent: &str, token: &str, event: &str) -> Input {
    Input::Hook(HookMessage {
        sent_at_ms: 0,
        token: token.into(),
        agent: agent.into(),
        event: event.into(),
        payload: serde_json::json!({}),
    })
}

fn run_changes(events: &[Event]) -> Vec<(Option<TaskId>, bool)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::AgentRunChanged {
                worktree,
                delivery_pending,
                ..
            } => Some((worktree.clone(), *delivery_pending)),
            _ => None,
        })
        .collect()
}

#[test]
fn worktree_give_work_creates_then_launches_in_it() {
    let mut h = in_repo();
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    let path = PathBuf::from("/data/worktrees/pay-qa").join(ID);
    assert_eq!(
        creates(&h),
        vec![CreateWorktree {
            agent: n("pay-qa"),
            task: TaskId(ID.into()),
            repo: REPO.into(),
            path: path.clone(),
            branch: format!("duohive/pay-qa/{ID}"),
        }]
    );
    assert!(h.calls().is_empty(), "{:?}", h.calls());
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
    {
        let st = h.store.state.lock().unwrap();
        let row = &st.worktrees[&(n("pay-qa"), TaskId(ID.into()))];
        assert_eq!(row.state, WorktreeState::Unchecked);
        assert_eq!(row.path, path);
    }

    h.input(created("pay-qa", ID, ok()));
    let cmd = h.only_start();
    assert_eq!(cmd.cwd, path);
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
    assert_eq!(h.rt("pay-qa").last_task.as_deref(), Some(TASK));
    let st = h.store.state.lock().unwrap();
    assert_eq!(st.sessions[&n("pay-qa")].worktree, Some(TaskId(ID.into())));
    let row = &st.worktrees[&(n("pay-qa"), TaskId(ID.into()))];
    assert_eq!(row.state, WorktreeState::InUse);
    assert_eq!(row.base, Some(BaseRef("origin/main".into())));
    assert_eq!(row.created_from, "abc123");
    drop(st);
    let view = &h.engine.snapshot().agents[&n("pay-qa")];
    assert_eq!(view.worktree, Some(TaskId(ID.into())));
    let specs = h.claude.launches.lock().unwrap();
    assert_eq!(specs.last().and_then(|s| s.cwd.clone()), Some(path));
}

#[test]
fn worktree_survives_later_saves_and_a_restore() {
    let mut h = in_repo();
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    h.input(created("pay-qa", ID, ok()));
    h.input(hook("pay-qa", "tok-1", "session-start"));
    assert_eq!(h.state("pay-qa"), AgentState::Working);
    let row = h.store.state.lock().unwrap().sessions[&n("pay-qa")].clone();
    assert_eq!(row.worktree, Some(TaskId(ID.into())));
    h.rt_mut("pay-qa").worktree = None;
    h.engine.restore(row);
    assert_eq!(h.rt("pay-qa").worktree, Some(TaskId(ID.into())));
}

#[test]
fn existing_branch_takes_next_suffix_then_errors() {
    let mut h = in_repo();
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    let mut ids = Vec::new();
    for _ in 0..9 {
        let req = creates(&h).pop().unwrap();
        ids.push(req.task.0.clone());
        assert_eq!(req.branch, format!("duohive/pay-qa/{}", req.task.0));
        h.input(created("pay-qa", &req.task.0, Err(GitError::BranchExists)));
    }
    let expected: Vec<String> = std::iter::once(ID.to_owned())
        .chain((2..=9).map(|k| format!("{ID}-{k}")))
        .collect();
    assert_eq!(ids, expected);
    assert_eq!(creates(&h).len(), 9, "no tenth attempt");
    assert_eq!(starts(&h), 0);
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
    assert_eq!(
        h.rt("pay-qa").status.error.as_deref(),
        Some("couldn't create worktree: branch exists · space")
    );
    assert!(h.store.state.lock().unwrap().worktrees.is_empty());
}

#[test]
fn worktree_creation_failure_keeps_task_and_stays_stopped() {
    let mut h = in_repo();
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    let events = h.input(created(
        "pay-qa",
        ID,
        Err(GitError::Other("disk full".into())),
    ));
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
    assert_eq!(h.rt("pay-qa").last_task.as_deref(), Some(TASK));
    let reason = "couldn't create worktree: disk full · space";
    assert_eq!(h.rt("pay-qa").status.error.as_deref(), Some(reason));
    assert!(
        events.iter().any(|e| matches!(e,
            Event::AgentStatusChanged { agent, status }
                if *agent == n("pay-qa") && status.error.as_deref() == Some(reason))),
        "{events:?}"
    );
    assert!(h.store.state.lock().unwrap().worktrees.is_empty());
    assert_eq!(starts(&h), 0);
    // Creation is over: a start goes ahead.
    h.start("pay-qa");
    assert_eq!(starts(&h), 1);
    assert_eq!(h.rt("pay-qa").status.error, None);
}

#[test]
fn errored_agent_stays_errored_on_creation_failure() {
    let mut h = in_repo();
    h.force_errored("pay-qa", "exited unexpectedly (code 1)", None);
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    assert_eq!(h.state("pay-qa"), AgentState::Errored);
    h.input(created("pay-qa", ID, Err(GitError::Other("boom".into()))));
    assert_eq!(h.state("pay-qa"), AgentState::Errored);
    let reason = "couldn't create worktree: boom · space";
    assert_eq!(h.rt("pay-qa").status.error.as_deref(), Some(reason));
    let st = h.store.state.lock().unwrap();
    let row = &st.sessions[&n("pay-qa")];
    assert_eq!(row.status.error.as_deref(), Some(reason));
    assert_eq!(row.last_task.as_deref(), Some(TASK));
    assert!(st.worktrees.is_empty());
}

#[test]
fn start_during_worktree_creation_refused() {
    let mut h = in_repo();
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    let reason = "pay-qa is getting a worktree ready";
    let events = h.start("pay-qa");
    assert!(rejected(&events, reason), "{events:?}");
    let events = h.give_in("pay-qa", "Another", RunIn::NewWorktree);
    assert!(rejected(&events, reason), "{events:?}");
    let events = h.give("pay-qa", "Another");
    assert!(rejected(&events, reason), "{events:?}");
    assert_eq!(creates(&h).len(), 1);
    assert_eq!(starts(&h), 0);
    // A stale answer for another id moves nothing.
    h.input(created("pay-qa", "other", ok()));
    assert_eq!(starts(&h), 0);
}

#[test]
fn worktree_needs_git_repo() {
    let mut h = Harness::new();
    let events = h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    assert!(
        rejected(
            &events,
            "new worktree needs a git repo; ~/code/pay-qa isn't one"
        ),
        "{events:?}"
    );
    assert!(h.git_calls().is_empty());
    assert!(h.calls().is_empty());
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
    // Nothing is in progress: a start goes ahead.
    h.start("pay-qa");
    assert_eq!(starts(&h), 1);
}

#[test]
fn queued_worktree_give_work_keeps_run_choice() {
    let mut h = Setup {
        dirs: HashMap::from([(PathBuf::from(REPO), DirStatus::GitRepo)]),
        inventory: None,
        ..Setup::default()
    }
    .build();
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    assert!(h.git_calls().is_empty());
    h.input(Input::CliInventory(all_found()));
    assert_eq!(creates(&h).len(), 1);
    assert_eq!(starts(&h), 0);
}

#[test]
fn next_turn_ignores_run_choice() {
    let mut h = in_repo();
    h.force("squad-1", AgentState::Idle);
    h.give_in("squad-1", "Add a test", RunIn::NewWorktree);
    assert!(h.git_calls().is_empty());
    assert!(matches!(h.calls().as_slice(), [RuntimeCall::Paste(a, t, _)]
        if *a == n("squad-1") && t == "Add a test"));
}

#[test]
fn run_changed_published() {
    let mut h = in_repo();
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    let id = Some(TaskId(ID.into()));
    let events = h.input(created("pay-qa", ID, ok()));
    assert_eq!(run_changes(&events), vec![(id.clone(), false)]);
    h.input(hook("pay-qa", "tok-1", "session-start"));
    h.input(hook("pay-qa", "tok-1", "turn-end"));
    assert_eq!(h.state("pay-qa"), AgentState::Finished);
    let events = h.give("pay-qa", "Next");
    assert_eq!(run_changes(&events), vec![(id.clone(), true)]);
    let view = &h.engine.snapshot().agents[&n("pay-qa")];
    assert!(view.delivery_pending);
    let events = h.input(hook("pay-qa", "tok-1", "busy"));
    assert_eq!(run_changes(&events), vec![(id, false)]);
    assert!(!h.engine.snapshot().agents[&n("pay-qa")].delivery_pending);
}

#[test]
fn directory_launch_clears_the_worktree() {
    let mut h = in_repo();
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    h.input(created("pay-qa", ID, ok()));
    h.force_errored("pay-qa", "exited unexpectedly (code 1)", None);
    let events = h.give("pay-qa", "Again");
    assert_eq!(run_changes(&events), vec![(None, false)]);
    assert_eq!(
        h.store.state.lock().unwrap().sessions[&n("pay-qa")].worktree,
        None
    );
}

const READY: &str = "pay-qa is getting a worktree ready";

fn edit(h: &mut Harness, e: crate::command::OrgEdit) -> Vec<Event> {
    h.send(crate::command::Command::EditOrg(e))
}

#[test]
fn rename_and_delete_refused_during_creation() {
    use crate::command::{AgentDraft, OrgEdit};
    use crate::names::NodeName;
    use crate::org::{NodeKey, RoleKey};
    let mut h = in_repo();
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    let fields = h
        .engine
        .org
        .find_agent(&n("pay-qa"))
        .unwrap()
        .agent
        .fields
        .clone();
    let rename = OrgEdit::EditAgent {
        key: n("pay-qa"),
        draft: AgentDraft {
            name: n("pay-qa-2"),
            fields,
        },
    };
    let nn = |s: &str| NodeName::parse(s).unwrap();
    let qa = RoleKey {
        team: Some(nn("Payments")),
        role: nn("QA"),
    };
    for e in [
        rename,
        OrgEdit::Delete(NodeKey::Agent(n("pay-qa"))),
        OrgEdit::Delete(NodeKey::Role(qa)),
        OrgEdit::Delete(NodeKey::Team(nn("Payments"))),
    ] {
        let events = edit(&mut h, e.clone());
        assert!(rejected(&events, READY), "{e:?}: {events:?}");
    }
    assert!(h.engine.org.find_agent(&n("pay-qa")).is_some());
    assert!(h.store.state.lock().unwrap().saved_orgs.is_empty());
    // The answer still finds its agent and launches.
    h.input(created("pay-qa", ID, ok()));
    assert_eq!(starts(&h), 1);
}

#[test]
fn stop_refused_during_creation() {
    let mut h = in_repo();
    h.force_errored("pay-qa", "exited unexpectedly (code 1)", None);
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    let events = h.send(crate::command::Command::StopAgent { agent: n("pay-qa") });
    assert!(rejected(&events, READY), "{events:?}");
    assert_eq!(h.state("pay-qa"), AgentState::Errored);
    h.input(created("pay-qa", ID, ok()));
    assert_eq!(h.state("pay-qa"), AgentState::Starting);
}

#[test]
fn stale_created_keeps_the_guard() {
    let mut h = in_repo();
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    h.input(created("pay-qa", "other", ok()));
    h.input(created("pay-qa", "other", Err(GitError::Other("x".into()))));
    assert!(rejected(&h.start("pay-qa"), READY));
    assert_eq!(starts(&h), 0);
    assert_eq!(h.rt("pay-qa").status.error, None);
    assert_eq!(creates(&h).len(), 1);
    h.input(created("pay-qa", ID, ok()));
    assert_eq!(starts(&h), 1);
}

#[test]
fn creation_with_no_answer_times_out() {
    use super::super::worktrees::CREATE_WAIT;
    use std::time::Duration;

    let mut h = in_repo();
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    assert_eq!(h.engine.next_deadline(), Some(h.t0 + CREATE_WAIT));
    h.tick_at(CREATE_WAIT - Duration::from_secs(1));
    assert!(rejected(&h.start("pay-qa"), READY));
    let events = h.tick_at(CREATE_WAIT);
    let reason = "couldn't create worktree: git didn't answer · space";
    assert!(
        events.iter().any(|e| matches!(e,
            Event::AgentStatusChanged { agent, status }
                if *agent == n("pay-qa") && status.error.as_deref() == Some(reason))),
        "{events:?}"
    );
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
    assert_eq!(h.rt("pay-qa").last_task.as_deref(), Some(TASK));
    // The row stays listed under w: git may still have made the worktree.
    assert_eq!(h.store.state.lock().unwrap().worktrees.len(), 1);
    // A late answer launches nothing; the guard is gone.
    h.input(created("pay-qa", ID, ok()));
    assert_eq!(starts(&h), 0);
    h.start("pay-qa");
    assert_eq!(starts(&h), 1);
}

#[test]
fn reload_dropping_an_agent_mid_creation_is_refused() {
    use super::super::harness::test_org;
    use crate::command::Command;
    use crate::ports::OrgFileError;
    let mut removed = test_org();
    removed.teams[0].roles.remove(1);
    let mut renamed = test_org();
    renamed.teams[0].roles[1].agents[0].name = n("pay-qa-2");
    for org in [removed, renamed] {
        let mut h = in_repo();
        h.give_in("pay-qa", TASK, RunIn::NewWorktree);
        h.store.state.lock().unwrap().org_result = Ok(Some(org));
        let events = h.send(Command::ReloadOrg);
        let err = OrgFileError {
            line: None,
            message: READY.into(),
        };
        assert_eq!(events, vec![Event::OrgFileError(Some(err))]);
        assert_eq!(h.engine.org, test_org());
        // The late answer still finds its agent and launches.
        h.input(created("pay-qa", ID, ok()));
        assert_eq!(starts(&h), 1);
    }
}

#[test]
fn duo_cli_change_mid_creation_sets_no_restart() {
    use crate::cli_kind::CliKind;
    use crate::command::OrgEdit;
    use crate::org::DuoConfig;
    let mut h = Setup {
        dirs: HashMap::from([(PathBuf::from("/home/me"), DirStatus::GitRepo)]),
        ..Setup::default()
    }
    .build();
    h.give_in("Duo", TASK, RunIn::NewWorktree);
    assert!(h.engine.creating_worktree(&n("Duo")));
    let defaults = h.engine.org.defaults.clone();
    let events = edit(
        &mut h,
        OrgEdit::EditOrgDefaults {
            defaults,
            duo: DuoConfig {
                name: n("Duo"),
                cli: CliKind::Opencode,
            },
        },
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::OrgChanged { .. })),
        "{events:?}"
    );
    assert!(!h.rt("Duo").restart_after_stop);
    h.input(created("Duo", ID, ok()));
    assert_eq!(starts(&h), 1);
    assert_eq!(h.rt("Duo").cli, Some(CliKind::Opencode));
}
