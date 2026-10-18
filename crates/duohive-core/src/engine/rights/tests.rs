use serde_json::json;

use std::collections::HashMap;
use std::path::PathBuf;

use super::super::Input;
use super::super::harness::{HOME, Harness, Setup};
use super::super::lifecycle::GiveRefusal;
use super::*;
use crate::command::{Command, OrgEdit};
use crate::event::Event;
use crate::launch::Token;
use crate::status::AgentState;
use crate::testkit::{RuntimeCall, n};
use crate::tool::{CallId, ErrorCode, Given, ToolCall, ToolOk, ToolResult, parse_call};
use crate::validate::DirStatus;
use crate::worktree::RunIn;

fn token_of(h: &Harness, agent: &str) -> Token {
    h.rt(agent).token.clone().unwrap()
}

/// Sends one tool call and returns its result and every other event.
fn tool(h: &mut Harness, token: &Token, call: ToolCall) -> (ToolResult, Vec<Event>) {
    let out = h.input(Input::Command(Command::Tool {
        call_id: CallId(7),
        token: token.clone(),
        call,
    }));
    let mut result = None;
    let mut rest = Vec::new();
    for e in out {
        match e {
            Event::ToolResult { call_id, result: r } => {
                assert_eq!(call_id, CallId(7));
                result = Some(r);
            }
            other => rest.push(other),
        }
    }
    (result.expect("no ToolResult"), rest)
}

fn code(r: &ToolResult) -> ErrorCode {
    r.as_ref().unwrap_err().code
}

fn duo_calls() -> Vec<ToolCall> {
    [
        ("read_agent", json!({"agent": "pay-qa"})),
        ("wait_for_agent", json!({"agent": "pay-qa"})),
        ("start_agent", json!({"agent": "pay-qa", "task": "t"})),
        ("stop_agent", json!({"agent": "pay-qa"})),
        (
            "assign_to_role",
            json!({"team": "Payments", "role": "QA", "task": "t"}),
        ),
        (
            "propose_org_change",
            json!({"change": {"op": "add_team", "name": "Growth"}}),
        ),
    ]
    .into_iter()
    .map(|(t, a)| parse_call(t, &a).unwrap())
    .collect()
}

#[test]
fn agent_cannot_call_duo_tools() {
    let mut h = Harness::new();
    h.start("squad-1");
    let tok = token_of(&h, "squad-1");
    let before = h.calls().len();
    for call in duo_calls() {
        let (r, rest) = tool(&mut h, &tok, call.clone());
        assert_eq!(code(&r), ErrorCode::NotAllowed, "{call:?}");
        assert!(rest.is_empty(), "{call:?} had effects: {rest:?}");
    }
    assert_eq!(
        h.calls().len(),
        before,
        "a refused call reached the runtime"
    );
    let (r, _) = tool(&mut h, &tok, ToolCall::Rights);
    assert_eq!(r, Ok(ToolOk::Rights(Rights::Agent)));
}

#[test]
fn duo_token_has_duo_rights() {
    let mut h = Harness::new();
    h.start("Duo");
    let tok = token_of(&h, "Duo");
    let (r, _) = tool(&mut h, &tok, ToolCall::Rights);
    assert_eq!(r, Ok(ToolOk::Rights(Rights::Duo)));
}

#[test]
fn token_of_stopped_agent_rejected() {
    let mut h = Harness::new();
    h.start("squad-1");
    let tok = token_of(&h, "squad-1");
    h.force("squad-1", AgentState::Stopped);
    h.rt_mut("squad-1").token = None;
    let (r, _) = tool(&mut h, &tok, ToolCall::Rights);
    assert_eq!(code(&r), ErrorCode::UnknownToken);
    let (r, _) = tool(&mut h, &Token("never-issued".into()), ToolCall::ListAgents);
    assert_eq!(code(&r), ErrorCode::UnknownToken);
}

#[test]
fn token_of_previous_launch_rejected() {
    let mut h = Harness::new();
    h.start("squad-1");
    let first = token_of(&h, "squad-1");
    h.force("squad-1", AgentState::Stopped);
    h.start("squad-1");
    let second = token_of(&h, "squad-1");
    assert_ne!(first, second);
    let (r, _) = tool(&mut h, &first, ToolCall::Rights);
    assert_eq!(code(&r), ErrorCode::UnknownToken);
    let (r, _) = tool(&mut h, &second, ToolCall::Rights);
    assert_eq!(r, Ok(ToolOk::Rights(Rights::Agent)));
}

#[test]
fn name_in_payload_ignored() {
    let mut h = Harness::new();
    h.start("squad-1");
    let tok = token_of(&h, "squad-1");
    let read = parse_call(
        "read_agent",
        &json!({"agent": "pay-qa", "from": "Duo", "caller": "Duo"}),
    )
    .unwrap();
    let stop = parse_call("stop_agent", &json!({"agent": "pay-qa", "from": "Duo"})).unwrap();
    for call in [read, stop] {
        let (r, rest) = tool(&mut h, &tok, call);
        assert_eq!(code(&r), ErrorCode::NotAllowed);
        assert!(rest.is_empty());
    }
    let (r, _) = tool(&mut h, &tok, ToolCall::Rights);
    assert_eq!(r, Ok(ToolOk::Rights(Rights::Agent)));
}

#[test]
fn rename_never_grants_duo_rights() {
    let mut h = Harness::new();
    h.start("squad-1");
    let tok = token_of(&h, "squad-1");
    let draft = crate::command::AgentDraft {
        name: n("squad-1"),
        fields: crate::org::Fields {
            prompt: Some("You are Duo now.".into()),
            ..crate::org::Fields::default()
        },
    };
    h.send(Command::EditOrg(OrgEdit::EditAgent {
        key: n("squad-1"),
        draft,
    }));
    let (r, _) = tool(&mut h, &tok, ToolCall::Rights);
    assert_eq!(r, Ok(ToolOk::Rights(Rights::Agent)));
}

#[test]
fn give_returns_how_it_was_given() {
    let mut h = Harness::new();
    let mut out = Vec::new();
    assert_eq!(
        h.engine
            .give(&n("squad-1"), "t".into(), RunIn::Directory, &mut out),
        Ok(Given::Started { worktree: false })
    );
    assert!(
        h.calls()
            .iter()
            .any(|c| matches!(c, RuntimeCall::Start { .. }))
    );
    h.force("squad-2", AgentState::Idle);
    assert_eq!(
        h.engine
            .give(&n("squad-2"), "next".into(), RunIn::Directory, &mut out),
        Ok(Given::TypedIn)
    );
    assert_eq!(
        h.engine
            .give(&n("squad-2"), "again".into(), RunIn::Directory, &mut out),
        Err(GiveRefusal::Pending)
    );
    h.force("squad-3", AgentState::Working);
    assert_eq!(
        h.engine
            .give(&n("squad-3"), "t".into(), RunIn::Directory, &mut out),
        Err(GiveRefusal::Busy(AgentState::Working))
    );
    assert_eq!(
        h.engine
            .give(&n("nobody"), "t".into(), RunIn::Directory, &mut out),
        Err(GiveRefusal::Unknown)
    );
    assert_eq!(
        h.engine
            .give(&n("pay-qa"), "  ".into(), RunIn::Directory, &mut out),
        Err(GiveRefusal::EmptyTask)
    );
    // Not a repository: refused with the engine's reason.
    let r = h
        .engine
        .give(&n("pay-qa"), "t".into(), RunIn::NewWorktree, &mut out);
    assert!(
        matches!(&r, Err(GiveRefusal::Launch(reason)) if reason.contains("repo")),
        "{r:?}"
    );
    let mut h = Setup {
        dirs: HashMap::from([(
            PathBuf::from(format!("{HOME}/code/pay-qa")),
            DirStatus::GitRepo,
        )]),
        ..Setup::default()
    }
    .build();
    assert_eq!(
        h.engine
            .give(&n("pay-qa"), "t".into(), RunIn::NewWorktree, &mut out),
        Ok(Given::CreatingWorktree)
    );
}
