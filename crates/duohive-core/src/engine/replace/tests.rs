//! A replace from a template (UI §4.8): stop the old tree, then write the new org.

use std::path::PathBuf;
use std::time::Duration;

use super::super::Input;
use super::super::harness::{Harness, Setup, T0};
use crate::card::{Card, CardChange, CardId, CardOutcome, CardSource};
use crate::cli_kind::CliKind;
use crate::command::{Command, OrgEdit};
use crate::event::Event;
use crate::inbox::MessageStatus;
use crate::launch::{Token, WindowId};
use crate::names::{AgentName, NodeName};
use crate::org::{NodeKey, RoleKey};
use crate::ports::RuntimeEvent;
use crate::status::AgentState;
use crate::templates::{ProductInput, SquadInput, TemplateInput};
use crate::testkit::{RuntimeCall, n};
use crate::time::Millis;
use crate::tool::{CallId, ToolCall};
use crate::worktree::{BaseRef, RunIn, TaskId, WorktreeRow, WorktreeState};

const REPLACING: &str = "the org is being replaced";
const RECONNECTING: &str = "still reconnecting · try again";
const BROKEN: &str = "org.toml has an error · fix it first (o)";
const REMOVED: &str = "agent removed";

fn squad(size: u32) -> TemplateInput {
    TemplateInput::AgentSquad(SquadInput {
        size,
        directory: "~/code/app".into(),
        cli: CliKind::Claude,
        worktrees: false,
        prompt: None,
    })
}

fn product() -> TemplateInput {
    TemplateInput::ProductTeam(ProductInput {
        team_name: "Product".into(),
        cli: CliKind::Claude,
        directories: [
            "~/code/backend".into(),
            "~/code/frontend".into(),
            "~/code/qa".into(),
        ],
        worktrees: false,
    })
}

fn replace(h: &mut Harness) -> Vec<Event> {
    replace_with(h, squad(3))
}

fn replace_with(h: &mut Harness, template: TemplateInput) -> Vec<Event> {
    h.send(Command::NewOrgFromTemplate { template })
}

/// `agent` launched and working in `window`; it stops only on an exit.
fn up(h: &mut Harness, agent: &str, window: &str) {
    h.start(agent);
    h.input(Input::Runtime(RuntimeEvent::Started {
        agent: n(agent),
        window: WindowId(window.into()),
    }));
    h.force(agent, AgentState::Working);
}

fn finish(h: &mut Harness, agent: &str, window: &str) -> Vec<Event> {
    h.input(Input::Runtime(RuntimeEvent::WindowGone {
        agent: n(agent),
        window: WindowId(window.into()),
    }))
}

fn stops(h: &Harness) -> Vec<AgentName> {
    h.calls()
        .into_iter()
        .filter_map(|c| match c {
            RuntimeCall::Stop(a, _, _) => Some(a),
            _ => None,
        })
        .collect()
}

fn replaced(h: &Harness) -> Vec<crate::org::Org> {
    h.store.state.lock().expect("lock").replaced_orgs.clone()
}

fn refused(events: &[Event], reason: &str) -> bool {
    events
        .iter()
        .any(|e| matches!(e, Event::CommandRejected { reason: r } if r == reason))
}

fn failed_with(events: &[Event], agent: &str) -> bool {
    events.iter().any(|e| {
        matches!(e, Event::OrgReplaceFailed { agent: Some(a), reason }
            if a == &n(agent) && reason.contains("would not stop"))
    })
}

fn row(agent: &str, task: &str) -> WorktreeRow {
    WorktreeRow {
        agent: n(agent),
        task: TaskId(task.into()),
        repo: PathBuf::from("/home/me/code/app"),
        path: PathBuf::from("/data/worktrees").join(agent).join(task),
        branch: format!("duohive/{agent}/{task}"),
        created_at: Millis(1),
        created_from: "abc123".into(),
        state: WorktreeState::InUse,
        checked_at: None,
        base: Some(BaseRef("origin/main".into())),
    }
}

fn put(h: &Harness, r: WorktreeRow) {
    let mut st = h.store.state.lock().expect("lock");
    st.worktrees.insert((r.agent.clone(), r.task.clone()), r);
}

#[test]
fn replace_stops_agents_then_writes() {
    let mut h = Harness::new();
    up(&mut h, "squad-1", "@1");
    up(&mut h, "squad-2", "@2");
    let events = replace(&mut h);
    assert_eq!(stops(&h), vec![n("squad-1"), n("squad-2")]);
    assert!(
        replaced(&h).is_empty(),
        "nothing is written before the stops"
    );
    assert!(!events.iter().any(|e| matches!(e, Event::OrgReplaced)));

    let events = finish(&mut h, "squad-1", "@1");
    assert!(replaced(&h).is_empty(), "one agent still stopping");
    assert!(!events.iter().any(|e| matches!(e, Event::OrgReplaced)));

    let events = finish(&mut h, "squad-2", "@2");
    assert_eq!(replaced(&h).len(), 1);
    assert!(events.iter().any(|e| matches!(e, Event::OrgReplaced)));
}

#[test]
fn replace_keeps_duo_and_org_defaults() {
    let mut h = Harness::new();
    let before = h.engine.snapshot().org;
    up(&mut h, "squad-1", "@1");
    replace(&mut h);
    finish(&mut h, "squad-1", "@1");
    let after = h.engine.snapshot().org;
    assert_eq!(after.defaults, before.defaults);
    assert_eq!(after.duo, before.duo);
    assert_eq!(after.agents().len(), 3);
    assert!(
        after.teams.is_empty(),
        "the squad template makes a new tree"
    );
}

#[test]
fn replace_keeps_duo_running() {
    let mut h = Harness::new();
    h.force("Duo", AgentState::Working);
    up(&mut h, "squad-1", "@1");
    replace(&mut h);
    assert!(!stops(&h).contains(&AgentName::duo()));
    finish(&mut h, "squad-1", "@1");
    assert_eq!(h.state("Duo"), AgentState::Working);
}

#[test]
fn replace_aborts_when_a_stop_fails() {
    let mut h = Harness::new();
    up(&mut h, "squad-1", "@1");
    replace(&mut h);
    let events = h.tick_at(Duration::from_secs(6));
    assert!(failed_with(&events, "squad-1"), "{events:?}");
    assert!(replaced(&h).is_empty());
    assert!(h.engine.replace.is_none());
    assert_eq!(h.engine.snapshot().org.teams.len(), 1, "the old org stands");
}

#[test]
fn next_deadline_includes_pending_replace() {
    let mut h = Harness::new();
    up(&mut h, "squad-1", "@1");
    replace(&mut h);
    let deadline = T0 + Duration::from_secs(6);
    assert_eq!(h.engine.next_deadline(), Some(deadline));
}

#[test]
fn replace_refused_while_a_worktree_is_being_created() {
    let mut h = Setup {
        dirs: std::collections::HashMap::from([(
            PathBuf::from("/home/me/code/squad"),
            crate::validate::DirStatus::GitRepo,
        )]),
        ..Setup::default()
    }
    .build();
    h.give_in("squad-1", "build it", RunIn::NewWorktree);
    assert!(
        h.engine.creating_worktree(&n("squad-1")),
        "a worktree is being made"
    );
    let events = replace(&mut h);
    assert!(
        refused(&events, "squad-1 is getting a worktree ready"),
        "{events:?}"
    );
    assert!(stops(&h).is_empty());
    assert!(replaced(&h).is_empty());
}

#[test]
fn replace_excludes_duo_from_the_stop_set() {
    let mut h = Harness::new();
    h.force("Duo", AgentState::Working);
    up(&mut h, "squad-1", "@1");
    replace(&mut h);
    assert_eq!(stops(&h), vec![n("squad-1")]);
}

#[test]
fn replace_refused_while_reconnecting() {
    let mut h = Harness::new();
    h.engine.start_up();
    let events = replace(&mut h);
    assert!(refused(&events, RECONNECTING), "{events:?}");
    assert!(stops(&h).is_empty());
}

#[test]
fn replace_store_failure_keeps_the_old_org() {
    let mut h = Harness::new();
    up(&mut h, "squad-1", "@1");
    replace(&mut h);
    h.store.state.lock().expect("lock").fail_replace = true;
    let events = finish(&mut h, "squad-1", "@1");
    assert!(events.iter().any(|e| matches!(
        e,
        Event::OrgReplaceFailed { agent: None, reason } if reason.contains("disk full")
    )));
    assert!(!events.iter().any(|e| matches!(e, Event::OrgReplaced)));
    assert!(h.engine.replace.is_none());
    assert_eq!(h.engine.snapshot().org.teams.len(), 1);
}

#[test]
fn commands_rejected_while_replacing() {
    let mut h = Harness::new();
    up(&mut h, "squad-1", "@1");
    up(&mut h, "squad-2", "@2");
    h.force("squad-2", AgentState::Idle);
    h.force("squad-3", AgentState::Stopped);
    replace(&mut h);

    let busy = h.send(Command::GiveWork {
        agent: n("squad-2"),
        task: "do it".into(),
        run: RunIn::Directory,
    });
    assert!(refused(&busy, REPLACING), "{busy:?}");
    let free = h.send(Command::GiveWork {
        agent: n("squad-3"),
        task: "do it".into(),
        run: RunIn::Directory,
    });
    assert!(refused(&free, REPLACING), "{free:?}");
    let assign = h.send(Command::AssignToRole {
        role: RoleKey {
            team: Some(NodeName::parse("Payments").expect("a name")),
            role: NodeName::parse("QA").expect("a name"),
        },
        task: "do it".into(),
        pick: None,
        run: Some(RunIn::Directory),
    });
    assert!(refused(&assign, REPLACING), "{assign:?}");
    let card = h.send(Command::ApproveCard {
        card: CardId(1),
        edited: None,
    });
    assert!(refused(&card, REPLACING), "{card:?}");
    let tool = h.send(Command::Tool {
        call_id: CallId(1),
        token: Token("tok".into()),
        call: ToolCall::StartAgent {
            agent: n("squad-2"),
            task: "do it".into(),
            run: None,
        },
    });
    assert!(refused(&tool, REPLACING), "{tool:?}");
    let start = h.send(Command::StartAgent {
        agent: n("squad-2"),
    });
    assert!(refused(&start, REPLACING), "{start:?}");
    let edit = h.send(Command::EditOrg(OrgEdit::Delete(NodeKey::Agent(n(
        "squad-2",
    )))));
    assert!(refused(&edit, REPLACING), "{edit:?}");
    let reload = h.send(Command::ReloadOrg);
    assert!(refused(&reload, REPLACING), "{reload:?}");

    finish(&mut h, "squad-1", "@1");
    finish(&mut h, "squad-2", "@2");
    assert_eq!(replaced(&h).len(), 1);
}

#[test]
fn restart_cleared_for_the_stop_set() {
    let mut h = Harness::new();
    up(&mut h, "squad-1", "@1");
    h.rt_mut("squad-1").restart_after_stop = true;
    replace(&mut h);
    assert!(!h.rt("squad-1").restart_after_stop);
    h.call_log.lock().expect("lock").clear();
    finish(&mut h, "squad-1", "@1");
    assert!(
        !h.calls()
            .iter()
            .any(|c| matches!(c, RuntimeCall::Start(..))),
        "no relaunch after the stop"
    );
}

#[test]
fn errored_agent_is_stopped_and_leaves_waiting() {
    let mut h = Harness::new();
    up(&mut h, "squad-1", "@1");
    h.force_errored("squad-2", "tmux server exited", Some("@2"));
    replace(&mut h);
    assert!(
        h.calls()
            .iter()
            .any(|c| matches!(c, RuntimeCall::Kill(a, w) if *a == n("squad-2") && w.0 == "@2"))
    );
    assert!(replaced(&h).is_empty(), "squad-1 is still stopping");
    let events = finish(&mut h, "squad-1", "@1");
    assert_eq!(replaced(&h).len(), 1);
    assert!(events.iter().any(|e| matches!(e, Event::OrgReplaced)));
}

#[test]
fn replace_refused_while_org_file_is_broken() {
    let mut h = Setup {
        org_error: Some(crate::ports::OrgFileError {
            line: Some(3),
            message: "unknown field".into(),
        }),
        ..Setup::default()
    }
    .build();
    up(&mut h, "squad-1", "@1");
    let events = replace(&mut h);
    assert!(refused(&events, BROKEN), "{events:?}");
    assert!(stops(&h).is_empty());
}

#[test]
fn replace_apply_happens_after_dispatch() {
    let mut h = Harness::new();
    up(&mut h, "squad-1", "@1");
    replace_with(&mut h, product());
    let events = finish(&mut h, "squad-1", "@1");
    assert!(events.iter().any(|e| matches!(e, Event::OrgReplaced)));
    assert!(
        !h.engine.agents.contains_key(&n("squad-1")),
        "the removed agent has no runtime entry"
    );
}

#[test]
fn agents_map_matches_the_new_org() {
    let mut h = Harness::new();
    up(&mut h, "squad-1", "@1");
    replace_with(&mut h, TemplateInput::Blank);
    finish(&mut h, "squad-1", "@1");
    let names: Vec<AgentName> = h.engine.agents.keys().cloned().collect();
    assert_eq!(names, vec![AgentName::duo()]);
    assert_eq!(h.engine.snapshot().org.agents().len(), 0);
}

#[test]
fn replace_keeps_worktree_rows() {
    let mut h = Harness::new();
    put(&h, row("squad-1", "first-task"));
    up(&mut h, "squad-1", "@1");
    replace_with(&mut h, TemplateInput::Blank);
    finish(&mut h, "squad-1", "@1");
    let st = h.store.state.lock().expect("lock");
    assert_eq!(st.worktrees.len(), 1);
}

#[test]
fn replace_drops_cards_and_messages_for_removed_agents() {
    let mut h = Harness::new();
    up(&mut h, "squad-1", "@1");
    h.send(Command::SendMessage {
        to: n("squad-1"),
        text: "a note".into(),
    });
    h.engine.cards.push(Card {
        id: CardId(1),
        source: CardSource::Duo,
        change: CardChange::Stop {
            agent: n("squad-1"),
        },
        raised_at: T0,
        answer: None,
    });
    replace_with(&mut h, TemplateInput::Blank);
    finish(&mut h, "squad-1", "@1");

    let status = h.engine.inbox[0].status.clone();
    assert_eq!(
        status,
        MessageStatus::Dropped {
            reason: REMOVED.into()
        }
    );
    assert_eq!(
        h.engine.cards[0].answer.as_ref().map(|(o, _)| o.clone()),
        Some(CardOutcome::NotApplied(REMOVED.into()))
    );
}

#[test]
fn replace_template_names_skip_kept_worktree_rows() {
    let mut h = Harness::new();
    put(&h, row("squad-1", "a"));
    put(&h, row("squad-2", "b"));
    put(&h, row("squad-3", "c"));
    replace(&mut h);
    let names: Vec<AgentName> = h
        .engine
        .snapshot()
        .org
        .agents()
        .into_iter()
        .map(|a| a.agent.name.clone())
        .collect();
    assert_eq!(names, vec![n("squad-4"), n("squad-5"), n("squad-6")]);
}
