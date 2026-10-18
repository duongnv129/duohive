//! Tests for Duo's tools and action lines: Task 3.5 (TD §6.2, §6.4; P3-D6,
//! P3-D14, P3-D16, P3-D23).

mod lines;
mod waits;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};

use super::super::Input;
use super::super::harness::{Harness, Setup, test_org};
use crate::card::{CardChange, CardId, CardSource};
use crate::chat::ChatKind;
use crate::command::Command;
use crate::event::Event;
use crate::hook::HookMessage;
use crate::launch::WindowId;
use crate::names::NodeName;
use crate::ports::{GitError, GitEvent, RuntimeEvent};
use crate::status::AgentState;
use crate::testkit::{GitCall, RuntimeCall, n};
use crate::tool::{
    CallId, ErrorCode, Given, SendOutcome, ToolCall, ToolError, ToolOk, ToolResult, parse_call,
};
use crate::validate::DirStatus;
use crate::worktree::{BaseRef, RunIn, TaskId};

const TASK: &str = "Fix the login bug";
const SQUAD_REPO: &str = "/home/me/code/squad";

/// A hook from `agent`'s current launch with `payload`.
fn hook_with(h: &mut Harness, agent: &str, event: &str, payload: Value) -> Vec<Event> {
    let token = h.rt(agent).token.clone().unwrap();
    h.input(Input::Hook(HookMessage {
        sent_at_ms: 0,
        token: token.0,
        agent: agent.into(),
        event: event.into(),
        payload,
    }))
}

fn hook(h: &mut Harness, agent: &str, event: &str) -> Vec<Event> {
    hook_with(h, agent, event, json!({}))
}

fn window_started(h: &mut Harness, agent: &str) {
    h.input(Input::Runtime(RuntimeEvent::Started {
        agent: n(agent),
        window: WindowId(format!("@{agent}")),
    }));
}

/// `agent` launched with no task, in window `@<agent>`, and ready: idle.
fn idle(h: &mut Harness, agent: &str) {
    h.start(agent);
    window_started(h, agent);
    hook(h, agent, "session-start");
    assert_eq!(h.state(agent), AgentState::Idle);
}

/// `agent` launched with a task, in window `@<agent>`, and busy: working.
fn working(h: &mut Harness, agent: &str) {
    h.give(agent, "first task");
    window_started(h, agent);
    hook(h, agent, "session-start");
    assert_eq!(h.state(agent), AgentState::Working);
}

/// `agent` working, then asking: waiting.
fn waiting(h: &mut Harness, agent: &str) {
    working(h, agent);
    hook_with(h, agent, "needs-input", json!({"prompt": "Run tests?"}));
    assert_eq!(h.state(agent), AgentState::Waiting);
}

/// A harness with Duo idle.
fn with_duo() -> Harness {
    let mut h = Harness::new();
    idle(&mut h, "Duo");
    h
}

/// A harness with Duo idle, where the Generalist role works in worktrees in a git repo.
fn squad_worktrees() -> Harness {
    let mut org = test_org();
    org.roles[0].defaults.worktree = Some(true);
    let mut h = Setup {
        org,
        dirs: HashMap::from([(PathBuf::from(SQUAD_REPO), DirStatus::GitRepo)]),
        ..Setup::default()
    }
    .build();
    idle(&mut h, "Duo");
    h
}

/// Duo's call `call` as call `id`: the results and every other event.
fn tool(h: &mut Harness, id: u64, call: ToolCall) -> (Vec<(CallId, ToolResult)>, Vec<Event>) {
    let token = h.rt("Duo").token.clone().unwrap();
    split(h.input(Input::Command(Command::Tool {
        call_id: CallId(id),
        token,
        call,
    })))
}

/// The results among `out`, and every other event.
fn split(out: Vec<Event>) -> (Vec<(CallId, ToolResult)>, Vec<Event>) {
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

/// The one result among `results`, for call `id`.
fn only(results: &[(CallId, ToolResult)], id: u64) -> ToolResult {
    match results {
        [(call, result)] if *call == CallId(id) => result.clone(),
        other => panic!("expected one result for call {id}, got {other:?}"),
    }
}

/// Duo's call answered at once: its result.
fn answer(h: &mut Harness, call: ToolCall) -> ToolResult {
    only(&tool(h, 1, call).0, 1)
}

/// The action lines among `events`: (text, result).
fn actions(events: &[Event]) -> Vec<(String, String)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::ChatLine(entry) => match &entry.kind {
                ChatKind::Action { result } => Some((entry.text.clone(), result.clone())),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn refused(message: &str) -> ToolResult {
    Err(ToolError::new(ErrorCode::Refused, message))
}

fn read(agent: &str) -> ToolCall {
    ToolCall::ReadAgent { agent: n(agent) }
}

fn wait(agent: &str) -> ToolCall {
    parse_call("wait_for_agent", &json!({ "agent": agent })).unwrap()
}

fn start(agent: &str, run: Option<RunIn>) -> ToolCall {
    ToolCall::StartAgent {
        agent: n(agent),
        task: TASK.into(),
        run,
    }
}

fn stop(agent: &str) -> ToolCall {
    ToolCall::StopAgent { agent: n(agent) }
}

/// Runtime calls that touch `agent`'s keys or window: pastes, stops and kills.
fn keys_to(h: &Harness, agent: &str) -> Vec<RuntimeCall> {
    h.calls()
        .into_iter()
        .filter(|c| match c {
            RuntimeCall::Paste(a, ..) | RuntimeCall::Stop(a, ..) | RuntimeCall::Kill(a, _) => {
                *a == n(agent)
            }
            _ => false,
        })
        .collect()
}

fn creations(h: &Harness, agent: &str) -> Vec<TaskId> {
    h.git_calls()
        .into_iter()
        .filter_map(|c| match c {
            GitCall::Create(req) if req.agent == n(agent) => Some(req.task),
            _ => None,
        })
        .collect()
}

fn created(agent: &str, task: TaskId, result: Result<(BaseRef, String), GitError>) -> Input {
    Input::Git(GitEvent::Created {
        agent: n(agent),
        task,
        result,
    })
}

/// The stop cards raised among `events`.
fn stop_cards(events: &[Event]) -> Vec<(CardId, CardSource, String)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::CardRaised(card) => match &card.change {
                CardChange::Stop { agent } => Some((card.id, card.source, agent.to_string())),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[test]
fn list_agents_shows_org_state_and_replies() {
    let mut h = with_duo();
    working(&mut h, "squad-2");
    hook_with(&mut h, "squad-2", "turn-end", json!({"last_reply": "done"}));
    working(&mut h, "auth-service");
    h.send(Command::SendMessage {
        to: n("auth-service"),
        text: "when you're free".into(),
    });
    let (results, events) = tool(&mut h, 1, ToolCall::ListAgents);
    let Ok(ToolOk::Agents { you, agents }) = only(&results, 1) else {
        panic!("not agents");
    };
    assert_eq!(you, n("Duo"));
    let names: Vec<String> = agents.iter().map(|a| a.name.to_string()).collect();
    assert_eq!(
        names,
        [
            "Duo",
            "auth-service",
            "billing-api",
            "pay-qa",
            "squad-1",
            "squad-2",
            "squad-3"
        ]
    );
    let duo = &agents[0];
    assert_eq!((duo.team.as_ref(), duo.role.as_ref()), (None, None));
    assert_eq!(duo.state, AgentState::Idle);
    assert!(!duo.free);
    let auth = &agents[1];
    assert_eq!(auth.team, Some(NodeName::parse("Payments").unwrap()));
    assert_eq!(auth.role, Some(NodeName::parse("Backend").unwrap()));
    assert_eq!(auth.state, AgentState::Working);
    assert_eq!(auth.waiting_messages, 1);
    assert!(!auth.free);
    let squad2 = &agents[5];
    assert_eq!(squad2.team, None);
    assert_eq!(squad2.role, Some(NodeName::parse("Generalist").unwrap()));
    assert_eq!(squad2.state, AgentState::Finished);
    assert_eq!(squad2.last_reply.as_deref(), Some("done"));
    assert!(squad2.free);
    assert_eq!(agents[4].state, AgentState::Stopped);
    assert!(agents[4].free);
    assert_eq!(
        actions(&events),
        [("list agents".into(), "7 agents".into())]
    );
}

#[test]
fn read_agent_returns_screen() {
    let mut h = with_duo();
    working(&mut h, "billing-api");
    let (results, _) = tool(&mut h, 3, read("billing-api"));
    assert!(results.is_empty(), "{results:?}");
    assert_eq!(
        h.calls().last(),
        Some(&RuntimeCall::Capture(n("billing-api")))
    );
    let screen = "compiling\nBUSY";
    let (results, events) = split(h.input(Input::Runtime(RuntimeEvent::Screen {
        agent: n("billing-api"),
        text: screen.into(),
    })));
    assert_eq!(
        only(&results, 3),
        Ok(ToolOk::Screen {
            agent: n("billing-api"),
            state: AgentState::Working,
            text: screen.into(),
        })
    );
    assert_eq!(
        actions(&events),
        [("read billing-api".into(), "read".into())]
    );
    // Answered once: the next screen answers nothing.
    let (results, _) = split(h.input(Input::Runtime(RuntimeEvent::Screen {
        agent: n("billing-api"),
        text: screen.into(),
    })));
    assert!(results.is_empty(), "{results:?}");
}

#[test]
fn read_agent_times_out_no_screen() {
    let mut h = with_duo();
    working(&mut h, "billing-api");
    let (results, _) = tool(&mut h, 3, read("billing-api"));
    assert!(results.is_empty(), "{results:?}");
    assert_eq!(h.engine.next_deadline(), Some(h.t0 + super::READ_TIMEOUT));
    let (results, _) = split(h.tick_at(Duration::from_millis(4_999)));
    assert!(results.is_empty(), "{results:?}");
    let (results, events) = split(h.tick_at(Duration::from_secs(5)));
    let Err(ToolError { code, .. }) = only(&results, 3) else {
        panic!("answered");
    };
    assert_eq!(code, ErrorCode::NoScreen);
    assert_eq!(actions(&events).len(), 1);
}

#[test]
fn read_stopped_agent_refused() {
    let mut h = with_duo();
    let before = h.calls().len();
    let (results, events) = tool(&mut h, 3, read("pay-qa"));
    assert_eq!(only(&results, 3), refused("pay-qa is stopped"));
    assert_eq!(h.calls().len(), before, "no capture");
    assert_eq!(
        actions(&events),
        [("read pay-qa".into(), "refused (pay-qa is stopped)".into())]
    );
}

#[test]
fn start_in_worktree_answers_after_created() {
    let mut h = squad_worktrees();
    let (results, events) = tool(&mut h, 7, start("squad-1", Some(RunIn::NewWorktree)));
    assert!(results.is_empty(), "{results:?}");
    assert!(actions(&events).is_empty(), "no line until answered");
    let task = creations(&h, "squad-1")[0].clone();
    let ok = Ok((BaseRef("origin/main".into()), "abc123".into()));
    let (results, events) = split(h.input(created("squad-1", task, ok)));
    assert_eq!(
        only(&results, 7),
        Ok(ToolOk::Given {
            agent: n("squad-1"),
            given: Given::Started { worktree: true },
        })
    );
    assert_eq!(
        actions(&events),
        [(format!("give work squad-1 \"{TASK}\""), "started".into())]
    );
}

#[test]
fn start_in_worktree_creation_fails_refused() {
    let mut h = squad_worktrees();
    // The role's default is a worktree, so `run` may be left out.
    let (results, _) = tool(&mut h, 7, start("squad-1", None));
    assert!(results.is_empty(), "{results:?}");
    let task = creations(&h, "squad-1")[0].clone();
    let failed = Err(GitError::Other("disk full".into()));
    let (results, events) = split(h.input(created("squad-1", task, failed)));
    assert_eq!(
        only(&results, 7),
        refused("couldn't create worktree: disk full")
    );
    assert_eq!(
        actions(&events),
        [(
            format!("give work squad-1 \"{TASK}\""),
            "refused (couldn't create worktree: disk full)".into()
        )]
    );

    // P2-D22: no answer from git within 120 s.
    let (results, _) = tool(&mut h, 8, start("squad-2", Some(RunIn::NewWorktree)));
    assert!(results.is_empty(), "{results:?}");
    let (results, _) = split(h.tick_at(Duration::from_secs(119)));
    assert!(results.is_empty(), "{results:?}");
    let (results, _) = split(h.tick_at(Duration::from_secs(120)));
    assert_eq!(
        only(&results, 8),
        refused("couldn't create worktree: git didn't answer")
    );
}

#[test]
fn start_busy_agent_refused() {
    let mut h = with_duo();
    working(&mut h, "billing-api");
    assert_eq!(
        answer(&mut h, start("billing-api", None)),
        refused("billing-api is working; send_message instead")
    );
    // A task typed in and not yet taken: still pending.
    idle(&mut h, "squad-1");
    h.give("squad-1", "first");
    assert_eq!(
        answer(&mut h, start("squad-1", None)),
        refused("squad-1 is still taking the last task")
    );
}

#[test]
fn start_agent_refused_while_you_type() {
    let mut h = with_duo();
    idle(&mut h, "squad-1");
    h.send(Command::Typing {
        agent: Some(n("squad-1")),
    });
    assert_eq!(
        answer(&mut h, start("squad-1", None)),
        refused("you are typing into squad-1")
    );
    assert!(
        !h.calls()
            .iter()
            .any(|c| matches!(c, RuntimeCall::Paste(a, ..) if *a == n("squad-1")))
    );
    // Your own Give work still types in (P3-D19 holds back deliveries only).
    h.give("squad-1", "yours");
    assert!(h.calls().iter().any(
        |c| matches!(c, RuntimeCall::Paste(a, _, text, _) if *a == n("squad-1") && text == "yours")
    ));
}

#[test]
fn start_idle_agent_types_in() {
    let mut h = with_duo();
    idle(&mut h, "squad-1");
    let (results, events) = tool(&mut h, 4, start("squad-1", None));
    assert_eq!(
        only(&results, 4),
        Ok(ToolOk::Given {
            agent: n("squad-1"),
            given: Given::TypedIn,
        })
    );
    assert!(h.calls().iter().any(
        |c| matches!(c, RuntimeCall::Paste(a, _, text, _) if *a == n("squad-1") && text == TASK)
    ));
    assert_eq!(
        actions(&events),
        [(format!("give work squad-1 \"{TASK}\""), "typed in".into())]
    );
}

#[test]
fn stop_idle_agent_direct() {
    let mut h = with_duo();
    idle(&mut h, "squad-1");
    let (results, events) = tool(&mut h, 2, stop("squad-1"));
    assert_eq!(
        only(&results, 2),
        Ok(ToolOk::Stopped {
            agent: n("squad-1")
        })
    );
    assert!(matches!(
        keys_to(&h, "squad-1").as_slice(),
        [RuntimeCall::Stop(..)]
    ));
    assert!(stop_cards(&events).is_empty());
    assert_eq!(
        actions(&events),
        [("stop squad-1".into(), "stopped".into())]
    );
}

#[test]
fn stop_errored_agent_direct() {
    let mut h = with_duo();
    h.force_errored("squad-1", "boom", Some("@squad-1"));
    assert_eq!(
        answer(&mut h, stop("squad-1")),
        Ok(ToolOk::Stopped {
            agent: n("squad-1")
        })
    );
    assert_eq!(h.state("squad-1"), AgentState::Stopped);
}

/// `agent`'s stop through Duo: a pending card, and nothing sent to the agent.
fn assert_stop_raises_card(h: &mut Harness, agent: &str) {
    let before = keys_to(h, agent);
    let (results, events) = tool(h, 2, stop(agent));
    assert_eq!(only(&results, 2), Ok(ToolOk::Pending { card: CardId(1) }));
    assert_eq!(
        stop_cards(&events),
        [(CardId(1), CardSource::Duo, agent.to_owned())]
    );
    assert_eq!(keys_to(h, agent), before, "nothing sent to {agent}");
    assert_eq!(
        actions(&events),
        [(format!("stop {agent}"), "card raised".into())]
    );
}

#[test]
fn stop_busy_agent_raises_card() {
    let mut h = with_duo();
    working(&mut h, "squad-1");
    assert_stop_raises_card(&mut h, "squad-1");
    assert_eq!(h.state("squad-1"), AgentState::Working);
}

#[test]
fn stop_starting_agent_raises_card() {
    let mut h = with_duo();
    h.start("squad-1");
    assert_eq!(h.state("squad-1"), AgentState::Starting);
    assert_stop_raises_card(&mut h, "squad-1");
}

#[test]
fn stop_waiting_agent_raises_card() {
    let mut h = with_duo();
    waiting(&mut h, "squad-1");
    assert_stop_raises_card(&mut h, "squad-1");
}

#[test]
fn stop_stopped_agent_refused() {
    let mut h = with_duo();
    assert_eq!(
        answer(&mut h, stop("squad-1")),
        refused("squad-1 is already stopped")
    );
}

#[test]
fn no_tool_answers_a_prompt() {
    let mut h = with_duo();
    waiting(&mut h, "billing-api");
    let before = keys_to(&h, "billing-api");
    assert!(matches!(
        answer(&mut h, stop("billing-api")),
        Ok(ToolOk::Pending { .. })
    ));
    let send = ToolCall::SendMessage {
        to: n("billing-api"),
        text: "yes, run them".into(),
    };
    assert!(matches!(
        answer(&mut h, send),
        Ok(ToolOk::Queued {
            outcome: SendOutcome::Queued(AgentState::Waiting),
            ..
        })
    ));
    assert!(answer(&mut h, start("billing-api", None)).is_err());
    let (results, _) = tool(&mut h, 9, read("billing-api"));
    assert!(results.is_empty());
    let (results, _) = tool(&mut h, 10, wait("billing-api"));
    assert!(results.is_empty());
    let assign = ToolCall::AssignToRole {
        role: crate::org::RoleKey {
            team: Some(NodeName::parse("Payments").unwrap()),
            role: NodeName::parse("Backend").unwrap(),
        },
        task: TASK.into(),
    };
    tool(&mut h, 11, assign);
    assert_eq!(keys_to(&h, "billing-api"), before, "nothing typed in");
    assert_eq!(h.state("billing-api"), AgentState::Waiting);
}
