//! Tests for worktree checks and clean-up: Task 2.13 (Review Focus 2; P2-D6, P2-D16).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use super::super::Input;
use super::super::harness::{Harness, Setup};
use crate::command::{AgentDraft, Command, OrgEdit};
use crate::event::Event;
use crate::hook::HookMessage;
use crate::launch::{Token, WindowId};
use crate::org::{DuoConfig, Fields, NodeKey};
use crate::ports::{GitEvent, LiveWindow, RemoveWorktree, RuntimeEvent, SessionRow};
use crate::status::{AgentState, Status};
use crate::testkit::{GitCall, n};
use crate::validate::DirStatus;
use crate::worktree::{BaseRef, MergeKind, MergeState, RunIn, TaskId, WorktreeRow, WorktreeState};

const REPO: &str = "/home/me/code/pay-qa";
/// The id `TASK` gets when it runs in a new worktree.
const ID: &str = "add-retry-cases";
const TASK: &str = "Add retry cases";

/// A harness where pay-qa's directory is a git repo.
fn in_repo() -> Harness {
    Setup {
        dirs: HashMap::from([(PathBuf::from(REPO), DirStatus::GitRepo)]),
        ..Setup::default()
    }
    .build()
}

fn row(agent: &str, task: &str, state: WorktreeState) -> WorktreeRow {
    WorktreeRow {
        agent: n(agent),
        task: TaskId(task.into()),
        repo: REPO.into(),
        path: PathBuf::from("/data/worktrees").join(agent).join(task),
        branch: format!("duohive/{agent}/{task}"),
        created_at: crate::time::Millis(1),
        created_from: "abc123".into(),
        state,
        checked_at: None,
        base: Some(BaseRef("origin/main".into())),
    }
}

fn put(h: &Harness, r: WorktreeRow) {
    let mut st = h.store.state.lock().unwrap();
    st.worktrees.insert((r.agent.clone(), r.task.clone()), r);
}

fn stored(h: &Harness, agent: &str, task: &str) -> Option<WorktreeRow> {
    let st = h.store.state.lock().unwrap();
    st.worktrees.get(&(n(agent), TaskId(task.into()))).cloned()
}

fn state(h: &Harness, agent: &str, task: &str) -> WorktreeState {
    stored(h, agent, task).expect("row").state
}

/// The (agent, task) of every check so far, sorted.
fn checks(h: &Harness) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = h
        .git_calls()
        .into_iter()
        .filter_map(|c| match c {
            GitCall::Check(req) => Some((req.agent.to_string(), req.task.0)),
            _ => None,
        })
        .collect();
    v.sort();
    v
}

fn removes(h: &Harness) -> Vec<RemoveWorktree> {
    h.git_calls()
        .into_iter()
        .filter_map(|c| match c {
            GitCall::Remove(req) => Some(req),
            _ => None,
        })
        .collect()
}

fn clear_git(h: &Harness) {
    h.git_log.lock().unwrap().clear();
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, t)| ((*a).to_owned(), (*t).to_owned()))
        .collect()
}

/// An answer to the latest check of (agent, task); one never asked for gets
/// a generation no row has.
fn checked_on(h: &Harness, agent: &str, task: &str, merge: MergeState, dirty: u32) -> Input {
    let generation = h
        .git_calls()
        .into_iter()
        .rev()
        .find_map(|c| match c {
            GitCall::Check(req) if req.agent == n(agent) && req.task.0 == task => {
                Some(req.generation)
            }
            _ => None,
        })
        .unwrap_or(u64::MAX);
    answer(agent, task, merge, dirty, generation)
}

fn answer(agent: &str, task: &str, merge: MergeState, dirty: u32, generation: u64) -> Input {
    Input::Git(GitEvent::Checked {
        agent: n(agent),
        task: TaskId(task.into()),
        merge,
        dirty_files: dirty,
        base: Some(BaseRef("origin/main".into())),
        generation,
    })
}

fn removed(agent: &str, task: &str, result: Result<(), String>) -> Input {
    Input::Git(GitEvent::Removed {
        agent: n(agent),
        task: TaskId(task.into()),
        result,
    })
}

fn delete(h: &mut Harness, agent: &str, task: &str, confirmed: bool) -> Vec<Event> {
    h.send(Command::DeleteWorktree {
        agent: n(agent),
        task: TaskId(task.into()),
        confirmed,
    })
}

fn rejected(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::CommandRejected { reason } => Some(reason.clone()),
            _ => None,
        })
        .collect()
}

/// The last `Worktrees` event's rows.
fn listed(events: &[Event]) -> Option<Vec<WorktreeRow>> {
    events.iter().rev().find_map(|e| match e {
        Event::Worktrees(rows) => Some(rows.clone()),
        _ => None,
    })
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

fn started(agent: &str, window: &str) -> Input {
    Input::Runtime(RuntimeEvent::Started {
        agent: n(agent),
        window: WindowId(window.into()),
    })
}

/// Stops a running agent: the stop request, then its window going.
fn stop(h: &mut Harness, agent: &str, window: &str) {
    h.send(Command::StopAgent { agent: n(agent) });
    h.input(Input::Runtime(RuntimeEvent::WindowGone {
        agent: n(agent),
        window: WindowId(window.into()),
    }));
    assert_eq!(h.state(agent), AgentState::Stopped);
}

/// pay-qa working in a new worktree `ID`, on window `@1`.
fn working_in_worktree() -> Harness {
    let mut h = in_repo();
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    h.input(Input::Git(GitEvent::Created {
        agent: n("pay-qa"),
        task: TaskId(ID.into()),
        result: Ok((BaseRef("origin/main".into()), "abc123".into())),
    }));
    h.input(started("pay-qa", "@1"));
    h.input(hook("pay-qa", "tok-1", "session-start"));
    assert_eq!(h.state("pay-qa"), AgentState::Working);
    assert_eq!(state(&h, "pay-qa", ID), WorktreeState::InUse);
    clear_git(&h);
    h
}

#[test]
fn checks_on_finish_startup_open_and_every_10_minutes() {
    let mut h = in_repo();
    put(&h, row("pay-qa", "a", WorktreeState::Unchecked));
    put(&h, row("pay-qa", "b", WorktreeState::Removing));
    put(
        &h,
        row(
            "squad-1",
            "c",
            WorktreeState::NotMerged { files_changed: 1 },
        ),
    );
    let all = pairs(&[("pay-qa", "a"), ("squad-1", "c")]);

    // Its agent enters finished: that agent's rows.
    h.give("pay-qa", "run the tests");
    h.input(started("pay-qa", "@1"));
    h.input(hook("pay-qa", "tok-1", "session-start"));
    assert!(checks(&h).is_empty(), "{:?}", checks(&h));
    h.input(hook("pay-qa", "tok-1", "turn-end"));
    assert_eq!(h.state("pay-qa"), AgentState::Finished);
    assert_eq!(checks(&h), pairs(&[("pay-qa", "a")]));
    clear_git(&h);

    // Stopping checks nothing.
    stop(&mut h, "pay-qa", "@1");
    assert!(checks(&h).is_empty(), "{:?}", checks(&h));

    // The first directory inventory (after the first frame); a later one doesn't.
    h.input(Input::DirInventory(Vec::new()));
    assert_eq!(checks(&h), all);
    clear_git(&h);
    h.input(Input::DirInventory(Vec::new()));
    assert!(checks(&h).is_empty());

    // The list opens.
    h.send(Command::CheckWorktrees);
    assert_eq!(checks(&h), all);
    clear_git(&h);

    // Every 10 minutes.
    h.tick_at(Duration::from_secs(599));
    assert!(checks(&h).is_empty());
    h.tick_at(Duration::from_secs(600));
    assert_eq!(checks(&h), all);
    clear_git(&h);
    h.tick_at(Duration::from_secs(601));
    assert!(checks(&h).is_empty());
    h.tick_at(Duration::from_secs(1_200));
    assert_eq!(checks(&h), all);
}

#[test]
fn ten_minute_check_is_a_deadline_only_with_worktrees() {
    let mut h = Harness::new();
    assert_eq!(h.engine.next_deadline(), None);
    put(&h, row("squad-1", "c", WorktreeState::Unchecked));
    h.send(Command::CheckWorktrees);
    assert_eq!(
        h.engine.next_deadline(),
        Some(h.t0 + Duration::from_secs(600))
    );
}

#[test]
fn merged_clean_unused_is_removed() {
    let mut h = Harness::new();
    let r = row("squad-1", "c", WorktreeState::Unchecked);
    put(&h, r.clone());
    h.send(Command::CheckWorktrees);
    h.input(checked_on(
        &h,
        "squad-1",
        "c",
        MergeState::Merged(MergeKind::Squash),
        0,
    ));
    assert_eq!(
        removes(&h),
        vec![RemoveWorktree {
            agent: r.agent.clone(),
            task: r.task.clone(),
            repo: r.repo.clone(),
            path: r.path.clone(),
            branch: r.branch.clone(),
            delete_branch: true,
            force: false,
        }]
    );
    assert_eq!(state(&h, "squad-1", "c"), WorktreeState::Removing);
    let events = h.input(removed("squad-1", "c", Ok(())));
    assert_eq!(listed(&events), Some(Vec::new()), "{events:?}");
    assert!(stored(&h, "squad-1", "c").is_none());
    assert!(h.engine.snapshot().worktrees.is_empty());
}

#[test]
fn never_removes_dirty_unmerged_or_in_use() {
    let mut h = working_in_worktree();
    put(&h, row("squad-1", "x", WorktreeState::Unchecked));
    put(&h, row("squad-1", "y", WorktreeState::Unchecked));
    h.send(Command::CheckWorktrees);
    assert_eq!(
        checks(&h),
        pairs(&[("pay-qa", ID), ("squad-1", "x"), ("squad-1", "y")])
    );
    h.input(checked_on(
        &h,
        "squad-1",
        "x",
        MergeState::NotMerged { files_changed: 3 },
        0,
    ));
    h.input(checked_on(
        &h,
        "squad-1",
        "y",
        MergeState::Merged(MergeKind::Normal),
        2,
    ));
    let events = h.input(checked_on(
        &h,
        "pay-qa",
        ID,
        MergeState::Merged(MergeKind::Rebase),
        0,
    ));
    assert!(removes(&h).is_empty(), "{:?}", removes(&h));
    assert_eq!(
        state(&h, "squad-1", "x"),
        WorktreeState::NotMerged { files_changed: 3 }
    );
    assert_eq!(state(&h, "squad-1", "y"), WorktreeState::MergedDirty);
    assert_eq!(state(&h, "pay-qa", ID), WorktreeState::InUse);
    let rows = listed(&events).unwrap_or_else(|| h.engine.snapshot().worktrees);
    assert_eq!(rows.len(), 3);
    let checked_row = stored(&h, "squad-1", "x").unwrap();
    assert_eq!(checked_row.checked_at, Some(h.t0));
}

#[test]
fn in_use_survives_restart() {
    let mut h = Harness::new();
    put(
        &h,
        row("pay-qa", "t", WorktreeState::NotMerged { files_changed: 1 }),
    );
    h.store.state.lock().unwrap().sessions.insert(
        n("pay-qa"),
        SessionRow {
            agent: n("pay-qa"),
            token: Some(Token("tok-pay-qa".into())),
            window: Some(WindowId("@7".into())),
            status: Status {
                state: AgentState::Working,
                quiet: false,
                quiet_since: None,
                from_screen: false,
                since: h.t0,
                error: None,
            },
            last_reply: None,
            task_at_launch: true,
            saw_end_hook: false,
            last_task: Some("t".into()),
            worktree: Some(TaskId("t".into())),
        },
    );
    let events = h.engine.start_up();
    let snapshot = events
        .iter()
        .find_map(|e| match e {
            Event::Snapshot(s) => Some(s.clone()),
            _ => None,
        })
        .expect("snapshot");
    assert_eq!(snapshot.worktrees.len(), 1);
    assert_eq!(snapshot.worktrees[0].state, WorktreeState::InUse);
    h.input(Input::Runtime(RuntimeEvent::Inventory(vec![LiveWindow {
        agent: n("pay-qa"),
        window: WindowId("@7".into()),
        dead: None,
        activity_secs: 0,
    }])));
    assert_eq!(state(&h, "pay-qa", "t"), WorktreeState::InUse);
    h.send(Command::CheckWorktrees);
    h.input(checked_on(
        &h,
        "pay-qa",
        "t",
        MergeState::Merged(MergeKind::Normal),
        0,
    ));
    assert!(removes(&h).is_empty(), "{:?}", removes(&h));
    assert_eq!(state(&h, "pay-qa", "t"), WorktreeState::InUse);
}

#[test]
fn restart_without_its_window_is_not_in_use() {
    let mut h = Harness::new();
    put(&h, row("pay-qa", "t", WorktreeState::InUse));
    put(&h, row("squad-1", "r", WorktreeState::Removing));
    h.store.state.lock().unwrap().sessions.insert(
        n("pay-qa"),
        SessionRow {
            agent: n("pay-qa"),
            token: Some(Token("tok-pay-qa".into())),
            window: Some(WindowId("@7".into())),
            status: Status {
                state: AgentState::Working,
                quiet: false,
                quiet_since: None,
                from_screen: false,
                since: h.t0,
                error: None,
            },
            last_reply: None,
            task_at_launch: true,
            saw_end_hook: false,
            last_task: None,
            worktree: Some(TaskId("t".into())),
        },
    );
    h.engine.start_up();
    // A removal cut short by the restart is checked again, never assumed done.
    assert_eq!(state(&h, "squad-1", "r"), WorktreeState::Unchecked);
    let events = h.input(Input::Runtime(RuntimeEvent::Inventory(Vec::new())));
    assert_eq!(h.state("pay-qa"), AgentState::Stopped);
    assert_eq!(state(&h, "pay-qa", "t"), WorktreeState::Unchecked);
    assert!(listed(&events).is_some(), "{events:?}");
}

#[test]
fn removed_after_session_stops() {
    let mut h = working_in_worktree();
    stop(&mut h, "pay-qa", "@1");
    assert_ne!(state(&h, "pay-qa", ID), WorktreeState::InUse);
    assert!(checks(&h).is_empty());
    h.send(Command::CheckWorktrees);
    assert_eq!(checks(&h), pairs(&[("pay-qa", ID)]));
    h.input(checked_on(
        &h,
        "pay-qa",
        ID,
        MergeState::Merged(MergeKind::Squash),
        0,
    ));
    let rm = removes(&h);
    assert_eq!(rm.len(), 1, "{rm:?}");
    assert!(rm[0].delete_branch && !rm[0].force);
    assert_eq!(rm[0].task, TaskId(ID.into()));
}

#[test]
fn manual_remove_needs_confirm_unless_merged_clean() {
    let mut h = Harness::new();
    put(&h, row("squad-1", "m", WorktreeState::Merged));
    put(
        &h,
        row(
            "squad-1",
            "u",
            WorktreeState::NotMerged { files_changed: 2 },
        ),
    );
    put(&h, row("squad-1", "d", WorktreeState::MergedDirty));
    put(
        &h,
        row("squad-1", "f", WorktreeState::CheckFailed("x".into())),
    );
    put(&h, row("squad-1", "z", WorktreeState::Unchecked));

    let events = delete(&mut h, "squad-1", "m", false);
    assert!(rejected(&events).is_empty(), "{events:?}");
    let rm = removes(&h);
    assert_eq!(rm.len(), 1);
    assert!(rm[0].delete_branch && !rm[0].force);
    assert_eq!(state(&h, "squad-1", "m"), WorktreeState::Removing);
    clear_git(&h);

    for task in ["u", "d", "f", "z"] {
        let before = state(&h, "squad-1", task);
        let events = delete(&mut h, "squad-1", task, false);
        assert_eq!(
            rejected(&events),
            vec![format!(
                "duohive/squad-1/{task} isn't merged and clean · confirm to remove it"
            )]
        );
        assert_eq!(state(&h, "squad-1", task), before);
    }
    assert!(h.git_calls().is_empty(), "{:?}", h.git_calls());

    let events = delete(&mut h, "squad-1", "nope", true);
    assert_eq!(rejected(&events), vec!["no worktree nope for squad-1"]);
}

#[test]
fn confirmed_remove_forces_and_keeps_branch() {
    let mut h = Harness::new();
    let r = row(
        "squad-1",
        "u",
        WorktreeState::NotMerged { files_changed: 2 },
    );
    put(&h, r.clone());
    put(&h, row("squad-1", "d", WorktreeState::MergedDirty));
    let events = delete(&mut h, "squad-1", "u", true);
    assert!(rejected(&events).is_empty(), "{events:?}");
    assert_eq!(
        removes(&h),
        vec![RemoveWorktree {
            agent: r.agent.clone(),
            task: r.task.clone(),
            repo: r.repo.clone(),
            path: r.path.clone(),
            branch: r.branch.clone(),
            delete_branch: false,
            force: true,
        }]
    );
    assert_eq!(state(&h, "squad-1", "u"), WorktreeState::Removing);
    delete(&mut h, "squad-1", "d", true);
    assert!(removes(&h).iter().all(|r| r.force && !r.delete_branch));
    assert_eq!(removes(&h).len(), 2);
    // Removing: neither checked nor removed again.
    clear_git(&h);
    h.send(Command::CheckWorktrees);
    let events = delete(&mut h, "squad-1", "u", true);
    assert_eq!(
        rejected(&events),
        vec!["duohive/squad-1/u is being removed"]
    );
    assert!(h.git_calls().is_empty(), "{:?}", h.git_calls());
    let events = h.input(removed("squad-1", "u", Ok(())));
    assert!(stored(&h, "squad-1", "u").is_none());
    let rows = listed(&events).expect("worktrees published");
    assert_eq!(rows.len(), 1);
}

#[test]
fn failed_removal_is_kept_and_checked_again() {
    let mut h = Harness::new();
    put(&h, row("squad-1", "u", WorktreeState::MergedDirty));
    delete(&mut h, "squad-1", "u", true);
    h.input(removed("squad-1", "u", Err("locked".into())));
    assert_eq!(
        state(&h, "squad-1", "u"),
        WorktreeState::CheckFailed("couldn't remove: locked".into())
    );
    clear_git(&h);
    h.send(Command::CheckWorktrees);
    assert_eq!(checks(&h), pairs(&[("squad-1", "u")]));
}

#[test]
fn in_use_remove_refused() {
    let mut h = working_in_worktree();
    for confirmed in [false, true] {
        let events = delete(&mut h, "pay-qa", ID, confirmed);
        assert_eq!(
            rejected(&events),
            vec!["pay-qa is running in it · stop it first"]
        );
    }
    assert!(h.git_calls().is_empty(), "{:?}", h.git_calls());
    assert_eq!(state(&h, "pay-qa", ID), WorktreeState::InUse);
}

#[test]
fn unknown_merge_never_removes() {
    let mut h = Harness::new();
    put(&h, row("squad-1", "c", WorktreeState::Unchecked));
    h.send(Command::CheckWorktrees);
    h.input(checked_on(
        &h,
        "squad-1",
        "c",
        MergeState::Unknown("no default branch".into()),
        0,
    ));
    assert!(removes(&h).is_empty());
    assert_eq!(
        state(&h, "squad-1", "c"),
        WorktreeState::CheckFailed("no default branch".into())
    );
    // A later answer for a row that's gone, or isn't being removed, moves nothing.
    h.input(removed("squad-1", "c", Ok(())));
    assert!(stored(&h, "squad-1", "c").is_some());
    h.input(checked_on(
        &h,
        "squad-1",
        "gone",
        MergeState::Merged(MergeKind::Normal),
        0,
    ));
    assert!(removes(&h).is_empty());
}

#[test]
fn row_being_created_is_never_checked_or_removed() {
    let mut h = in_repo();
    put(&h, row("squad-1", "c", WorktreeState::Unchecked));
    h.give_in("pay-qa", TASK, RunIn::NewWorktree);
    assert_eq!(state(&h, "pay-qa", ID), WorktreeState::Unchecked);
    clear_git(&h);
    h.send(Command::CheckWorktrees);
    assert_eq!(checks(&h), pairs(&[("squad-1", "c")]));
    let events = delete(&mut h, "pay-qa", ID, true);
    assert_eq!(
        rejected(&events),
        vec!["pay-qa is getting a worktree ready"]
    );
    // A stray check answer for it moves nothing.
    h.input(checked_on(
        &h,
        "pay-qa",
        ID,
        MergeState::Merged(MergeKind::Normal),
        0,
    ));
    assert!(removes(&h).is_empty());
    assert_eq!(state(&h, "pay-qa", ID), WorktreeState::Unchecked);
}

#[test]
fn rename_refused_with_worktrees() {
    const RENAME: &str = "rename only while stopped with no worktrees";
    let mut h = Harness::new();
    put(&h, row("pay-qa", "u", WorktreeState::MergedDirty));
    let fields = h.engine.org.teams[0].roles[1].agents[0].fields.clone();
    let events = h.send(Command::EditOrg(OrgEdit::EditAgent {
        key: n("pay-qa"),
        draft: AgentDraft {
            name: n("pay-qa-2"),
            fields: fields.clone(),
        },
    }));
    assert_eq!(rejected(&events), vec![RENAME]);
    assert!(h.engine.org.find_agent(&n("pay-qa")).is_some());
    // Other edits to the agent still go ahead.
    let events = h.send(Command::EditOrg(OrgEdit::EditAgent {
        key: n("pay-qa"),
        draft: AgentDraft {
            name: n("pay-qa"),
            fields: Fields {
                model: Some("opus".into()),
                ..fields
            },
        },
    }));
    assert!(rejected(&events).is_empty(), "{events:?}");
    // Duo too.
    put(&h, row("Duo", "v", WorktreeState::Unchecked));
    let org = h.engine.org.clone();
    let events = h.send(Command::EditOrg(OrgEdit::EditOrgDefaults {
        defaults: org.defaults.clone(),
        duo: DuoConfig {
            name: n("Duo-2"),
            cli: org.duo.cli,
        },
    }));
    assert_eq!(rejected(&events), vec![RENAME]);
}

#[test]
fn delete_agent_keeps_worktrees() {
    let mut h = Harness::new();
    put(
        &h,
        row("pay-qa", "u", WorktreeState::NotMerged { files_changed: 4 }),
    );
    let events = h.send(Command::EditOrg(OrgEdit::Delete(NodeKey::Agent(n(
        "pay-qa",
    )))));
    assert!(rejected(&events).is_empty(), "{events:?}");
    assert!(h.engine.org.find_agent(&n("pay-qa")).is_none());
    assert_eq!(
        state(&h, "pay-qa", "u"),
        WorktreeState::NotMerged { files_changed: 4 }
    );
    h.send(Command::CheckWorktrees);
    assert_eq!(checks(&h), pairs(&[("pay-qa", "u")]));
    assert_eq!(h.engine.snapshot().worktrees.len(), 1);
}

#[test]
fn held_agent_worktree_is_in_use() {
    let mut h = Harness::new();
    put(&h, row("ghost", "t", WorktreeState::Unchecked));
    h.store.state.lock().unwrap().sessions.insert(
        n("ghost"),
        SessionRow {
            agent: n("ghost"),
            token: Some(Token("tok-ghost".into())),
            window: Some(WindowId("@9".into())),
            status: Status {
                state: AgentState::Working,
                quiet: false,
                quiet_since: None,
                from_screen: false,
                since: h.t0,
                error: None,
            },
            last_reply: None,
            task_at_launch: true,
            saw_end_hook: false,
            last_task: None,
            worktree: Some(TaskId("t".into())),
        },
    );
    h.engine.start_up();
    h.input(Input::Runtime(RuntimeEvent::Inventory(vec![LiveWindow {
        agent: n("ghost"),
        window: WindowId("@9".into()),
        dead: None,
        activity_secs: 0,
    }])));
    assert!(h.engine.holds_rows());
    assert_eq!(state(&h, "ghost", "t"), WorktreeState::InUse);
    h.send(Command::CheckWorktrees);
    h.input(checked_on(
        &h,
        "ghost",
        "t",
        MergeState::Merged(MergeKind::Normal),
        0,
    ));
    let events = delete(&mut h, "ghost", "t", true);
    assert_eq!(
        rejected(&events),
        vec!["ghost is running in it · stop it first"]
    );
    assert!(removes(&h).is_empty(), "{:?}", removes(&h));
    assert_eq!(state(&h, "ghost", "t"), WorktreeState::InUse);
}

/// A session row for `agent` working in worktree `task` on `window`.
fn session_in(h: &Harness, agent: &str, task: &str, window: &str) {
    h.store.state.lock().unwrap().sessions.insert(
        n(agent),
        SessionRow {
            agent: n(agent),
            token: Some(Token(format!("tok-{agent}"))),
            window: Some(WindowId(window.into())),
            status: Status {
                state: AgentState::Working,
                quiet: false,
                quiet_since: None,
                from_screen: false,
                since: h.t0,
                error: None,
            },
            last_reply: None,
            task_at_launch: true,
            saw_end_hook: false,
            last_task: None,
            worktree: Some(TaskId(task.into())),
        },
    );
}

#[test]
fn no_removal_while_reconnecting() {
    let mut h = Harness::new();
    // org.toml lost `ghost` while DuoHive was closed; its session may still run.
    put(&h, row("ghost", "t", WorktreeState::Unchecked));
    put(&h, row("squad-1", "c", WorktreeState::Unchecked));
    session_in(&h, "ghost", "t", "@9");
    h.engine.start_up();
    h.send(Command::CheckWorktrees);
    assert_eq!(checks(&h), pairs(&[("ghost", "t"), ("squad-1", "c")]));
    h.input(checked_on(
        &h,
        "ghost",
        "t",
        MergeState::Merged(MergeKind::Normal),
        0,
    ));
    h.input(checked_on(
        &h,
        "squad-1",
        "c",
        MergeState::Merged(MergeKind::Squash),
        0,
    ));
    assert!(removes(&h).is_empty(), "{:?}", removes(&h));
    // The orphan session's worktree is in use until the inventory settles it;
    // a merged, clean row stays `Merged` meanwhile.
    assert_eq!(state(&h, "ghost", "t"), WorktreeState::InUse);
    assert_eq!(state(&h, "squad-1", "c"), WorktreeState::Merged);
    let events = delete(&mut h, "ghost", "t", true);
    assert_eq!(
        rejected(&events),
        vec!["ghost is running in it · stop it first"]
    );
    assert!(removes(&h).is_empty(), "{:?}", removes(&h));

    // The inventory finds no window: the session has gone, so the next check removes.
    h.input(Input::Runtime(RuntimeEvent::Inventory(Vec::new())));
    assert_eq!(state(&h, "ghost", "t"), WorktreeState::Unchecked);
    clear_git(&h);
    h.send(Command::CheckWorktrees);
    h.input(checked_on(
        &h,
        "ghost",
        "t",
        MergeState::Merged(MergeKind::Normal),
        0,
    ));
    h.input(checked_on(
        &h,
        "squad-1",
        "c",
        MergeState::Merged(MergeKind::Squash),
        0,
    ));
    let mut removed: Vec<(String, bool, bool)> = removes(&h)
        .into_iter()
        .map(|r| (r.task.0, r.delete_branch, r.force))
        .collect();
    removed.sort();
    assert_eq!(
        removed,
        vec![("c".into(), true, false), ("t".into(), true, false)]
    );
}

#[test]
fn late_check_answer_ignored_for_recreated_row() {
    let mut h = Harness::new();
    put(&h, row("squad-1", "c", WorktreeState::MergedDirty));
    h.send(Command::CheckWorktrees);
    let old = match h.git_calls().pop() {
        Some(GitCall::Check(req)) => req.generation,
        other => panic!("expected a check, got {other:?}"),
    };
    delete(&mut h, "squad-1", "c", true);
    h.input(removed("squad-1", "c", Ok(())));
    assert!(stored(&h, "squad-1", "c").is_none());
    // A new row with the same agent and task, then the old check's answer.
    put(&h, row("squad-1", "c", WorktreeState::Unchecked));
    clear_git(&h);
    h.input(answer(
        "squad-1",
        "c",
        MergeState::Merged(MergeKind::Normal),
        0,
        old,
    ));
    assert!(removes(&h).is_empty(), "{:?}", removes(&h));
    assert_eq!(state(&h, "squad-1", "c"), WorktreeState::Unchecked);
    // Its own check's answer applies.
    h.send(Command::CheckWorktrees);
    h.input(checked_on(
        &h,
        "squad-1",
        "c",
        MergeState::Merged(MergeKind::Normal),
        0,
    ));
    assert_eq!(removes(&h).len(), 1);
}

#[test]
fn ten_minute_timer_rearms_when_first_worktree_appears() {
    let mut h = Harness::new();
    // No worktrees: the timer is no deadline, so no tick re-armed it.
    h.at(Duration::from_secs(1_800));
    assert_eq!(h.engine.next_deadline(), None);
    put(&h, row("squad-1", "c", WorktreeState::Unchecked));
    h.send(Command::CheckWorktrees);
    assert_eq!(
        h.engine.next_deadline(),
        Some(h.t0 + Duration::from_secs(2_400))
    );
}
