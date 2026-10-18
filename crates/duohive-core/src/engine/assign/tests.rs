//! Tests for a task to a role: Task 3.7 (TD §5.4; P3-D3, P3-D15, P3-D23, P3-D28).

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::json;

use super::super::Input;
use super::super::harness::{Harness, Setup, T0, test_org};
use crate::card::{Card, CardChange, CardId, CardOutcome, CardSource};
use crate::command::{AgentDraft, Command, OrgEdit};
use crate::event::{Event, RoleOutcome};
use crate::hook::HookMessage;
use crate::launch::WindowId;
use crate::names::NodeName;
use crate::org::{Fields, NodeKey, RoleKey};
use crate::ports::{GitError, GitEvent, RuntimeEvent};
use crate::status::AgentState;
use crate::testkit::{GitCall, RuntimeCall, n};
use crate::time::Millis;
use crate::tool::{CallId, ErrorCode, Given, ToolCall, ToolError, ToolOk, ToolResult};
use crate::validate::DirStatus;
use crate::worktree::{BaseRef, RunIn, TaskId};

const TASK: &str = "Fix the login bug";
const SQUAD_REPO: &str = "/home/me/code/squad";
const SQUADS: [&str; 3] = ["squad-1", "squad-2", "squad-3"];

fn nn(s: &str) -> NodeName {
    NodeName::parse(s).unwrap()
}

fn generalist() -> RoleKey {
    RoleKey {
        team: None,
        role: nn("Generalist"),
    }
}

fn backend() -> RoleKey {
    RoleKey {
        team: Some(nn("Payments")),
        role: nn("Backend"),
    }
}

/// A harness where the Generalist role works in worktrees, in a git repo.
fn squad_worktrees() -> Harness {
    let mut org = test_org();
    org.roles[0].defaults.worktree = Some(true);
    Setup {
        org,
        dirs: HashMap::from([(PathBuf::from(SQUAD_REPO), DirStatus::GitRepo)]),
        ..Setup::default()
    }
    .build()
}

/// A hook from `agent`'s current launch.
fn hook(h: &mut Harness, agent: &str, event: &str) -> Vec<Event> {
    let token = h.rt(agent).token.clone().unwrap();
    h.input(Input::Hook(HookMessage {
        sent_at_ms: 0,
        token: token.0,
        agent: agent.into(),
        event: event.into(),
        payload: json!({}),
    }))
}

/// `agent` launched with no task, in window `@<agent>`, and ready: idle.
fn idle(h: &mut Harness, agent: &str) {
    h.start(agent);
    h.input(Input::Runtime(RuntimeEvent::Started {
        agent: n(agent),
        window: WindowId(format!("@{agent}")),
    }));
    hook(h, agent, "session-start");
    assert_eq!(h.state(agent), AgentState::Idle);
}

/// Puts `agent` in `state`, entered at `T0 + at_ms`.
fn rest(h: &mut Harness, agent: &str, state: AgentState, at_ms: u64) {
    h.force(agent, state);
    h.rt_mut(agent).status.since = Millis(T0.0 + at_ms);
}

/// Every squad agent working.
fn all_working(h: &mut Harness) {
    for agent in SQUADS {
        h.force(agent, AgentState::Working);
    }
}

/// Your `AssignToRole`.
fn assign(h: &mut Harness, role: RoleKey, pick: Option<&str>) -> Vec<Event> {
    h.send(Command::AssignToRole {
        role,
        task: TASK.into(),
        pick: pick.map(n),
        run: None,
    })
}

/// The outcomes of `RoleAssigned` among `events`.
fn assigned(events: &[Event]) -> Vec<RoleOutcome> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::RoleAssigned {
                role,
                task,
                outcome,
            } => {
                assert_eq!((role, task.as_str()), (&generalist(), TASK));
                Some(outcome.clone())
            }
            _ => None,
        })
        .collect()
}

/// The reasons of `AssignRefused` among `events`, checking each kept the task.
fn refused(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::AssignRefused { task, reason, .. } => {
                assert_eq!(task, TASK);
                Some(reason.clone())
            }
            _ => None,
        })
        .collect()
}

fn raised(events: &[Event]) -> Vec<Card> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::CardRaised(card) => Some((**card).clone()),
            _ => None,
        })
        .collect()
}

fn answered(events: &[Event]) -> Vec<(CardId, CardOutcome)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::CardAnswered { card, outcome } => Some((*card, outcome.clone())),
            _ => None,
        })
        .collect()
}

/// The one card raised among `events`: its new agent, task, run and clone source.
fn clone_of(events: &[Event]) -> (AgentDraft, String, RunIn, Option<String>) {
    let cards = raised(events);
    let [card] = cards.as_slice() else {
        panic!("expected one card, got {events:?}");
    };
    assert_eq!(card.source, CardSource::NoFreeAgent);
    let CardChange::NewAgent {
        agent,
        task,
        run,
        clone_of,
        ..
    } = &card.change
    else {
        panic!("not a new-agent card: {card:?}");
    };
    (
        agent.clone(),
        task.clone(),
        *run,
        clone_of.as_ref().map(ToString::to_string),
    )
}

fn pastes_to(h: &Harness, agent: &str) -> Vec<String> {
    h.calls()
        .into_iter()
        .filter_map(|c| match c {
            RuntimeCall::Paste(a, _, text, _) if a == n(agent) => Some(text),
            _ => None,
        })
        .collect()
}

fn starts_of(h: &Harness, agent: &str) -> usize {
    h.calls()
        .into_iter()
        .filter(|c| matches!(c, RuntimeCall::Start(a, _) if *a == n(agent)))
        .count()
}

/// The task ids of git creations for `agent`, oldest first.
fn creations(h: &Harness, agent: &str) -> Vec<TaskId> {
    h.git_calls()
        .into_iter()
        .filter_map(|c| match c {
            GitCall::Create(req) if req.agent == n(agent) => Some(req.task),
            _ => None,
        })
        .collect()
}

/// Duo's `assign_to_role` as call `id`: the results and every other event.
fn tool_assign(h: &mut Harness, id: u64, role: RoleKey) -> (Vec<(CallId, ToolResult)>, Vec<Event>) {
    let token = h.rt("Duo").token.clone().unwrap();
    let out = h.input(Input::Command(Command::Tool {
        call_id: CallId(id),
        token,
        call: ToolCall::AssignToRole {
            role,
            task: TASK.into(),
        },
    }));
    tool_results(out)
}

fn tool_results(out: Vec<Event>) -> (Vec<(CallId, ToolResult)>, Vec<Event>) {
    let mut found = Vec::new();
    let mut rest = Vec::new();
    for e in out {
        match e {
            Event::ToolResult { call_id, result } => found.push((call_id, result)),
            other => rest.push(other),
        }
    }
    (found, rest)
}

fn created(agent: &str, task: TaskId, result: Result<(BaseRef, String), GitError>) -> Input {
    Input::Git(GitEvent::Created {
        agent: n(agent),
        task,
        result,
    })
}

#[test]
fn assign_skips_agent_you_type_into() {
    let mut h = Harness::new();
    rest(&mut h, "squad-1", AgentState::Idle, 50);
    rest(&mut h, "squad-2", AgentState::Finished, 100);
    h.force("squad-3", AgentState::Working);
    h.send(Command::Typing {
        agent: Some(n("squad-1")),
    });
    assert!(!h.engine.is_free(&n("squad-1")));
    assert_eq!(h.engine.free_agents(&generalist()), [n("squad-2")]);
    let events = assign(&mut h, generalist(), None);
    assert_eq!(
        assigned(&events),
        [RoleOutcome::Given {
            agent: n("squad-2"),
            given: Given::TypedIn
        }]
    );
    assert!(pastes_to(&h, "squad-1").is_empty());
    // Picked by name: refused with the reason.
    let events = assign(&mut h, generalist(), Some("squad-1"));
    assert_eq!(refused(&events), ["you are typing into squad-1"]);
}

#[test]
fn longest_at_rest_wins() {
    let mut h = Harness::new();
    rest(&mut h, "squad-2", AgentState::Finished, 100);
    rest(&mut h, "squad-1", AgentState::Idle, 50);
    h.force("squad-3", AgentState::Working);
    assert_eq!(
        h.engine.free_agents(&generalist()),
        [n("squad-1"), n("squad-2")]
    );
    let events = assign(&mut h, generalist(), None);
    assert_eq!(
        assigned(&events),
        [RoleOutcome::Given {
            agent: n("squad-1"),
            given: Given::TypedIn
        }]
    );
    assert_eq!(pastes_to(&h, "squad-1"), [TASK]);
    assert!(pastes_to(&h, "squad-2").is_empty());
}

#[test]
fn tie_goes_to_tree_order() {
    let mut h = Harness::new();
    rest(&mut h, "squad-3", AgentState::Idle, 50);
    rest(&mut h, "squad-2", AgentState::Idle, 50);
    h.force("squad-1", AgentState::Working);
    assert_eq!(
        h.engine.free_agents(&generalist()),
        [n("squad-2"), n("squad-3")]
    );
    let events = assign(&mut h, generalist(), None);
    assert_eq!(
        assigned(&events),
        [RoleOutcome::Given {
            agent: n("squad-2"),
            given: Given::TypedIn
        }]
    );
}

#[test]
fn stopped_counts_as_free() {
    let mut h = Harness::new();
    rest(&mut h, "squad-1", AgentState::Stopped, 300);
    rest(&mut h, "squad-2", AgentState::Stopped, 100);
    rest(&mut h, "squad-3", AgentState::Stopped, 200);
    let events = assign(&mut h, generalist(), None);
    assert_eq!(
        assigned(&events),
        [RoleOutcome::Given {
            agent: n("squad-2"),
            given: Given::Started { worktree: false }
        }]
    );
    assert_eq!(starts_of(&h, "squad-2"), 1);
    assert_eq!(h.state("squad-2"), AgentState::Starting);
    assert_eq!(h.rt("squad-2").last_task.as_deref(), Some(TASK));
}

#[test]
fn pending_or_creating_not_free() {
    let mut h = Setup {
        dirs: HashMap::from([(PathBuf::from(SQUAD_REPO), DirStatus::GitRepo)]),
        ..Setup::default()
    }
    .build();
    rest(&mut h, "squad-1", AgentState::Idle, 0);
    h.give("squad-1", "an earlier task");
    assert!(h.rt("squad-1").delivery.is_some());
    h.give_in("squad-2", "an earlier task", RunIn::NewWorktree);
    assert!(h.rt("squad-2").creating.is_some());
    assert_eq!(h.engine.free_agents(&generalist()), [n("squad-3")]);
    let events = assign(&mut h, generalist(), None);
    assert_eq!(
        assigned(&events),
        [RoleOutcome::Given {
            agent: n("squad-3"),
            given: Given::Started { worktree: false }
        }]
    );
}

#[test]
fn errored_is_not_free() {
    let mut h = Harness::new();
    h.force_errored("squad-1", "exited unexpectedly", None);
    assert_eq!(
        h.engine.free_agents(&generalist()),
        [n("squad-2"), n("squad-3")]
    );
}

#[test]
fn none_free_raises_card_with_task() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    all_working(&mut h);
    let (results, events) = tool_assign(&mut h, 7, generalist());
    assert_eq!(
        results,
        [(CallId(7), Ok(ToolOk::Pending { card: CardId(1) }))]
    );
    let (agent, task, run, from) = clone_of(&events);
    assert_eq!(agent.name, n("generalist-1"));
    assert_eq!(task, TASK);
    assert_eq!(run, RunIn::Directory);
    assert_eq!(from.as_deref(), Some("squad-1"));
    let card = &raised(&events)[0];
    assert_eq!(
        card.change,
        CardChange::NewAgent {
            role: generalist(),
            agent,
            task: TASK.into(),
            run: RunIn::Directory,
            clone_of: Some(n("squad-1")),
        }
    );
    // Nothing is created until you approve.
    assert!(h.engine.org.find_agent(&n("generalist-1")).is_none());
}

#[test]
fn clone_named_by_d11_skipping_taken() {
    let mut org = test_org();
    org.roles[0].agents.push(crate::org::AgentDef {
        name: n("generalist-1"),
        fields: Fields::default(),
    });
    let mut h = Setup {
        org,
        ..Setup::default()
    }
    .build();
    all_working(&mut h);
    h.force("generalist-1", AgentState::Working);
    let (agent, ..) = clone_of(&assign(&mut h, generalist(), None));
    assert_eq!(agent.name, n("generalist-2"));

    let mut h = Harness::new();
    h.force("auth-service", AgentState::Working);
    h.force("billing-api", AgentState::Waiting);
    let events = h.send(Command::AssignToRole {
        role: backend(),
        task: TASK.into(),
        pick: None,
        run: None,
    });
    let (agent, _, _, from) = clone_of(&events);
    assert_eq!(agent.name, n("payments-backend-1"));
    assert_eq!(from.as_deref(), Some("auth-service"));
}

#[test]
fn clone_copies_first_agents_fields() {
    let mut org = test_org();
    let own = Fields {
        cli: Some(crate::cli_kind::CliKind::Opencode),
        flags: Some("--verbose".into()),
        directory: Some("~/code/squad-one".into()),
        model: Some("small".into()),
        effort: Some("high".into()),
        worktree: Some(false),
        prompt: Some("Keep diffs small.".into()),
    };
    org.roles[0].agents[0].fields = own.clone();
    let mut h = Setup {
        org,
        ..Setup::default()
    }
    .build();
    all_working(&mut h);
    let (agent, ..) = clone_of(&assign(&mut h, generalist(), None));
    assert_eq!(agent.fields, own);
    assert_eq!(
        (
            agent.fields.directory.as_deref(),
            agent.fields.cli,
            agent.fields.flags.as_deref()
        ),
        (
            Some("~/code/squad-one"),
            Some(crate::cli_kind::CliKind::Opencode),
            Some("--verbose")
        )
    );
}

#[test]
fn none_free_card_raised_while_duo_stopped() {
    let mut h = Harness::new();
    all_working(&mut h);
    let before = h.calls().len();
    let events = assign(&mut h, generalist(), None);
    assert_eq!(assigned(&events), [RoleOutcome::Card(CardId(1))]);
    assert_eq!(raised(&events).len(), 1);
    assert_eq!(h.state("Duo"), AgentState::Stopped);
    assert_eq!(h.calls().len(), before, "{:?}", h.calls());
    assert_eq!(starts_of(&h, "Duo"), 0);
}

#[test]
fn none_free_card_raised_while_duo_busy() {
    let mut h = Harness::new();
    h.give("Duo", "plan the week");
    hook(&mut h, "Duo", "session-start");
    assert_eq!(h.state("Duo"), AgentState::Working);
    all_working(&mut h);
    let before = h.calls().len();
    let events = assign(&mut h, generalist(), None);
    assert_eq!(assigned(&events), [RoleOutcome::Card(CardId(1))]);
    assert_eq!(raised(&events).len(), 1);
    assert_eq!(h.state("Duo"), AgentState::Working);
    assert_eq!(h.calls().len(), before, "{:?}", h.calls());
}

#[test]
fn assign_none_free_returns_pending() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    all_working(&mut h);
    let (results, events) = tool_assign(&mut h, 9, generalist());
    assert_eq!(
        results,
        [(CallId(9), Ok(ToolOk::Pending { card: CardId(1) }))]
    );
    assert_eq!(raised(&events).len(), 1);
    // The card is the tool's answer: no RoleAssigned for Duo's call.
    assert!(assigned(&events).is_empty(), "{events:?}");
    assert_eq!(h.state("Duo"), AgentState::Idle);
}

#[test]
fn approve_creates_and_gives() {
    let mut h = Harness::new();
    all_working(&mut h);
    assign(&mut h, generalist(), None);
    let events = h.send(Command::ApproveCard {
        card: CardId(1),
        edited: None,
    });
    assert_eq!(answered(&events), [(CardId(1), CardOutcome::Approved)]);
    let loc = h.engine.org.find_agent(&n("generalist-1")).unwrap();
    assert_eq!(loc.role.name, nn("Generalist"));
    assert_eq!(starts_of(&h, "generalist-1"), 1);
    assert_eq!(h.state("generalist-1"), AgentState::Starting);
    assert_eq!(h.rt("generalist-1").last_task.as_deref(), Some(TASK));
}

#[test]
fn approve_clone_after_role_deleted_not_applied() {
    let mut h = Harness::new();
    all_working(&mut h);
    assign(&mut h, generalist(), None);
    for agent in SQUADS {
        h.force(agent, AgentState::Stopped);
    }
    h.send(Command::EditOrg(OrgEdit::Delete(NodeKey::Role(
        generalist(),
    ))));
    assert!(h.engine.org.roles.is_empty());
    let events = h.send(Command::ApproveCard {
        card: CardId(1),
        edited: None,
    });
    let outcome = answered(&events);
    assert!(
        matches!(
            outcome.as_slice(),
            [(CardId(1), CardOutcome::NotApplied(_))]
        ),
        "{outcome:?}"
    );
    assert!(h.engine.org.find_agent(&n("generalist-1")).is_none());
    assert_eq!(starts_of(&h, "generalist-1"), 0);
}

#[test]
fn squad_worktrees_on() {
    let mut h = squad_worktrees();
    let events = assign(&mut h, generalist(), None);
    assert_eq!(
        assigned(&events),
        [RoleOutcome::Given {
            agent: n("squad-1"),
            given: Given::CreatingWorktree
        }]
    );
    assert_eq!(creations(&h, "squad-1").len(), 1);
    assert_eq!(starts_of(&h, "squad-1"), 0);

    let mut h = squad_worktrees();
    all_working(&mut h);
    let (_, _, run, _) = clone_of(&assign(&mut h, generalist(), None));
    assert_eq!(run, RunIn::NewWorktree);
}

#[test]
fn pick_must_still_be_free() {
    let mut h = Setup {
        dirs: HashMap::from([(PathBuf::from(SQUAD_REPO), DirStatus::GitRepo)]),
        ..Setup::default()
    }
    .build();
    h.force("squad-2", AgentState::Working);
    let events = assign(&mut h, generalist(), Some("squad-2"));
    assert_eq!(refused(&events), ["squad-2 isn't free any more"]);
    assert!(assigned(&events).is_empty());
    assert!(raised(&events).is_empty());

    h.give_in("squad-3", "an earlier task", RunIn::NewWorktree);
    assert!(h.rt("squad-3").creating.is_some());
    let events = assign(&mut h, generalist(), Some("squad-3"));
    assert_eq!(refused(&events), ["squad-3 isn't free any more"]);

    // A free pick wins over the one at rest longest.
    rest(&mut h, "squad-1", AgentState::Stopped, 0);
    let events = assign(&mut h, generalist(), Some("squad-1"));
    assert_eq!(
        assigned(&events),
        [RoleOutcome::Given {
            agent: n("squad-1"),
            given: Given::Started { worktree: false }
        }]
    );
}

#[test]
fn your_assign_refusals_keep_the_task() {
    let mut h = Harness::new();
    let ghost = RoleKey {
        team: None,
        role: nn("Ghost"),
    };
    let events = assign(&mut h, ghost, None);
    assert_eq!(refused(&events), ["no role Ghost"]);
    let events = h.send(Command::AssignToRole {
        role: generalist(),
        task: "  ".into(),
        pick: None,
        run: None,
    });
    let reasons: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::AssignRefused { reason, .. } => Some(reason.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(reasons, ["the task is empty"]);
}

#[test]
fn your_assign_publishes_who_took_it() {
    let mut h = Harness::new();
    rest(&mut h, "squad-1", AgentState::Idle, 0);
    h.force("squad-2", AgentState::Working);
    h.force("squad-3", AgentState::Working);
    let events = assign(&mut h, generalist(), None);
    assert_eq!(
        assigned(&events),
        [RoleOutcome::Given {
            agent: n("squad-1"),
            given: Given::TypedIn
        }]
    );
    h.force("squad-1", AgentState::Working);
    let events = assign(&mut h, generalist(), None);
    assert_eq!(assigned(&events), [RoleOutcome::Card(CardId(1))]);
}

#[test]
fn tool_unknown_role() {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    let ghost = RoleKey {
        team: Some(nn("Payments")),
        role: nn("Ghost"),
    };
    let (results, _) = tool_assign(&mut h, 3, ghost);
    let [(CallId(3), Err(ToolError { code, .. }))] = results.as_slice() else {
        panic!("{results:?}");
    };
    assert_eq!(*code, ErrorCode::UnknownRole);
}

#[test]
fn tool_assign_in_worktree_waits_for_created() {
    let mut h = squad_worktrees();
    idle(&mut h, "Duo");
    let (results, _) = tool_assign(&mut h, 7, generalist());
    assert!(results.is_empty(), "{results:?}");
    let task = creations(&h, "squad-1")[0].clone();
    let ok = Ok((BaseRef("origin/main".into()), "abc123".into()));
    let (results, _) = tool_results(h.input(created("squad-1", task, ok)));
    assert_eq!(
        results,
        [(
            CallId(7),
            Ok(ToolOk::Given {
                agent: n("squad-1"),
                given: Given::Started { worktree: true }
            })
        )]
    );
    assert_eq!(starts_of(&h, "squad-1"), 1);

    // squad-1 is starting now, so squad-2 takes the next one; git fails.
    let (results, _) = tool_assign(&mut h, 8, generalist());
    assert!(results.is_empty(), "{results:?}");
    let task = creations(&h, "squad-2")[0].clone();
    let failed = Err(GitError::Other("disk full".into()));
    let (results, _) = tool_results(h.input(created("squad-2", task, failed)));
    assert_eq!(
        results,
        [(
            CallId(8),
            Err(ToolError::new(
                ErrorCode::Refused,
                "couldn't create worktree: disk full"
            ))
        )]
    );
}
