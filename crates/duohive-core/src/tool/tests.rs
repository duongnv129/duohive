use serde_json::json;

use super::*;
use crate::card::{Card, CardChange, CardId, CardOutcome, CardSource};
use crate::chat::{ChatEntry, ChatId, ChatKind};
use crate::command::{Command, MoveTarget, OrgEdit, TeamDraft};
use crate::event::{Event, RoleOutcome};
use crate::inbox::{MessageId, MessageStatus, Sender};
use crate::launch::Token;
use crate::names::{AgentName, NodeName};
use crate::org::{Fields, NodeKey, RoleKey};
use crate::status::AgentState;
use crate::testkit::change_examples;
use crate::time::Millis;
use crate::worktree::RunIn;

fn agent(s: &str) -> AgentName {
    AgentName::parse(s).unwrap()
}

fn node(s: &str) -> NodeName {
    NodeName::parse(s).unwrap()
}

fn role_key(team: Option<&str>, role: &str) -> RoleKey {
    RoleKey {
        team: team.map(node),
        role: node(role),
    }
}

fn code(r: Result<ToolCall, ToolError>) -> (ErrorCode, String) {
    let e = r.unwrap_err();
    (e.code, e.message)
}

#[test]
fn tool_names_round_trip() {
    for t in ToolName::ALL {
        assert_eq!(ToolName::parse(t.as_str()), Some(t));
    }
    let duo_only: Vec<&str> = ToolName::ALL
        .into_iter()
        .filter(|t| t.duo_only())
        .map(ToolName::as_str)
        .collect();
    assert_eq!(
        duo_only,
        [
            "read_agent",
            "wait_for_agent",
            "start_agent",
            "stop_agent",
            "assign_to_role",
            "propose_org_change"
        ]
    );
    let changes: Vec<&str> = ToolName::ALL
        .into_iter()
        .filter(|t| t.changes())
        .map(ToolName::as_str)
        .collect();
    assert_eq!(
        changes,
        [
            "send_message",
            "start_agent",
            "stop_agent",
            "assign_to_role",
            "propose_org_change"
        ]
    );
    assert_eq!(ToolName::parse("rm_rf"), None);
}

#[test]
fn tools_for_agent_lacks_duo_tools() {
    assert_eq!(
        tools_for(Rights::Agent),
        [
            ToolName::ListAgents,
            ToolName::SendMessage,
            ToolName::MessageStatus
        ]
    );
    assert_eq!(tools_for(Rights::Duo), ToolName::ALL);
}

#[test]
#[expect(clippy::too_many_lines, reason = "one case per tool")]
fn parse_call_accepts_each_tool() {
    assert_eq!(
        parse_call("list_agents", &json!({})).unwrap(),
        ToolCall::ListAgents
    );
    assert_eq!(
        parse_call("send_message", &json!({"to": "pay-qa", "text": "hi"})).unwrap(),
        ToolCall::SendMessage {
            to: agent("pay-qa"),
            text: "hi".to_owned()
        }
    );
    assert_eq!(
        parse_call("message_status", &json!({"message_id": 42})).unwrap(),
        ToolCall::MessageStatus { id: MessageId(42) }
    );
    assert_eq!(
        parse_call("read_agent", &json!({"agent": "pay-qa"})).unwrap(),
        ToolCall::ReadAgent {
            agent: agent("pay-qa")
        }
    );
    assert_eq!(
        parse_call("wait_for_agent", &json!({"agent": "pay-qa"})).unwrap(),
        ToolCall::WaitForAgent {
            agent: agent("pay-qa"),
            timeout_minutes: 10
        }
    );
    assert_eq!(
        parse_call(
            "wait_for_agent",
            &json!({"agent": "pay-qa", "timeout_minutes": 60})
        )
        .unwrap(),
        ToolCall::WaitForAgent {
            agent: agent("pay-qa"),
            timeout_minutes: 60
        }
    );
    assert_eq!(
        parse_call(
            "start_agent",
            &json!({"agent": "pay-qa", "task": "run the tests", "run": "worktree"})
        )
        .unwrap(),
        ToolCall::StartAgent {
            agent: agent("pay-qa"),
            task: "run the tests".to_owned(),
            run: Some(RunIn::NewWorktree)
        }
    );
    assert_eq!(
        parse_call(
            "start_agent",
            &json!({"agent": "pay-qa", "task": "t", "run": "directory"})
        )
        .unwrap(),
        ToolCall::StartAgent {
            agent: agent("pay-qa"),
            task: "t".to_owned(),
            run: Some(RunIn::Directory)
        }
    );
    assert_eq!(
        parse_call("start_agent", &json!({"agent": "pay-qa", "task": "t"})).unwrap(),
        ToolCall::StartAgent {
            agent: agent("pay-qa"),
            task: "t".to_owned(),
            run: None
        }
    );
    assert_eq!(
        parse_call("stop_agent", &json!({"agent": "pay-qa"})).unwrap(),
        ToolCall::StopAgent {
            agent: agent("pay-qa")
        }
    );
    assert_eq!(
        parse_call(
            "assign_to_role",
            &json!({"team": "Payments", "role": "QA", "task": "check it"})
        )
        .unwrap(),
        ToolCall::AssignToRole {
            role: role_key(Some("Payments"), "QA"),
            task: "check it".to_owned()
        }
    );
    assert_eq!(
        parse_call(
            "assign_to_role",
            &json!({"role": "Generalist", "task": "t"})
        )
        .unwrap(),
        ToolCall::AssignToRole {
            role: role_key(None, "Generalist"),
            task: "t".to_owned()
        }
    );
    assert_eq!(
        parse_call(
            "propose_org_change",
            &json!({"change": {"op": "add_team", "name": "Growth", "fields": {}}})
        )
        .unwrap(),
        ToolCall::ProposeOrgChange {
            change: Box::new(OrgEdit::AddTeam(TeamDraft {
                name: node("Growth"),
                defaults: Fields::default()
            }))
        }
    );
}

#[test]
fn parse_call_ignores_unknown_fields() {
    let call = parse_call(
        "send_message",
        &json!({"to": "pay-qa", "text": "hi", "from": "Duo", "agent": "Duo"}),
    )
    .unwrap();
    assert_eq!(
        call,
        ToolCall::SendMessage {
            to: agent("pay-qa"),
            text: "hi".to_owned()
        }
    );
}

#[test]
fn propose_change_shapes_parse() {
    for (change, want) in change_examples() {
        let got = parse_call("propose_org_change", &json!({ "change": change })).unwrap();
        assert_eq!(
            got,
            ToolCall::ProposeOrgChange {
                change: Box::new(want)
            },
            "{change}"
        );
    }
    // The root is only a move target.
    let (c, _) = code(parse_call(
        "propose_org_change",
        &json!({"change": {"op": "remove", "node": {"duo": true}}}),
    ));
    assert_eq!(c, ErrorCode::InvalidArguments);
    // Positions are for teams only.
    for to in [json!({"before_team": "Payments"}), json!("end")] {
        let (c, _) = code(parse_call(
            "propose_org_change",
            &json!({"change": {"op": "move", "node": {"agent": "pay-qa"}, "to": to}}),
        ));
        assert_eq!(c, ErrorCode::InvalidArguments);
    }
}

#[test]
fn parse_call_refuses_bad_arguments() {
    let cases = [
        ("send_message", json!({"text": "hi"}), "to"),
        ("send_message", json!({"to": "../x", "text": "hi"}), "to"),
        (
            "send_message",
            json!({"to": "pay-qa", "text": "a".repeat(MAX_TEXT + 1)}),
            "text",
        ),
        (
            "wait_for_agent",
            json!({"agent": "pay-qa", "timeout_minutes": 61}),
            "timeout_minutes",
        ),
        (
            "propose_org_change",
            json!({"change": {"op": "rename_everything"}}),
            "change.op",
        ),
    ];
    for (tool, args, field) in cases {
        let (c, message) = code(parse_call(tool, &args));
        assert_eq!(c, ErrorCode::InvalidArguments, "{tool} {args}");
        assert!(message.contains(field), "{message} should name {field}");
    }
    let (c, message) = code(parse_call("rm_rf", &json!({})));
    assert_eq!(c, ErrorCode::InvalidArguments);
    assert_eq!(message, "unknown tool rm_rf");
}

fn round_trip<T>(v: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let text = serde_json::to_string(v).unwrap();
    let back: T = serde_json::from_str(&text).unwrap();
    assert_eq!(&back, v);
}

#[test]
#[expect(clippy::too_many_lines, reason = "one value per new variant")]
fn new_types_round_trip_json() {
    let card = Card {
        id: CardId(3),
        source: CardSource::NoFreeAgent,
        change: CardChange::Stop {
            agent: agent("pay-qa"),
        },
        raised_at: Millis(5),
        answer: Some((CardOutcome::NotApplied("gone".to_owned()), Millis(9))),
    };
    let chat = ChatEntry {
        id: ChatId(1),
        at: Millis(2),
        kind: ChatKind::CardResult {
            card: CardId(3),
            outcome: CardOutcome::Approved,
        },
        text: "→ approved".to_owned(),
    };
    let commands = [
        Command::Tool {
            call_id: CallId(7),
            token: Token("t".to_owned()),
            call: ToolCall::ListAgents,
        },
        Command::SendMessage {
            to: agent("pay-qa"),
            text: "hi".to_owned(),
        },
        Command::ChatToDuo {
            text: "what's blocked?".to_owned(),
        },
        Command::AssignToRole {
            role: role_key(Some("Payments"), "QA"),
            task: "t".to_owned(),
            pick: Some(agent("pay-qa")),
            run: Some(RunIn::Directory),
        },
        Command::ApproveCard {
            card: CardId(3),
            edited: Some(Box::new(CardChange::Org(OrgEdit::Move {
                node: NodeKey::Team(node("Growth")),
                to: MoveTarget::End,
            }))),
        },
        Command::RejectCard { card: CardId(3) },
        Command::Typing {
            agent: Some(agent("pay-qa")),
        },
    ];
    for c in &commands {
        round_trip(c);
    }
    let events = [
        Event::ToolResult {
            call_id: CallId(7),
            result: Ok(ToolOk::Pending { card: CardId(3) }),
        },
        Event::MessageQueued {
            id: MessageId(1),
            from: Sender::Agent(agent("Duo")),
            to: agent("pay-qa"),
        },
        Event::MessageDelivered { id: MessageId(1) },
        Event::MessageDropped {
            id: MessageId(1),
            from: Sender::You,
            to: agent("pay-qa"),
            text: "hi".to_owned(),
            reason: "inbox full".to_owned(),
        },
        Event::InboxChanged {
            agent: agent("pay-qa"),
            waiting: 2,
        },
        Event::CardRaised(Box::new(card)),
        Event::CardAnswered {
            card: CardId(3),
            outcome: CardOutcome::EditedApproved,
        },
        Event::ChatLine(chat),
        Event::MessageRefused {
            to: agent("pay-qa"),
            text: "hi".to_owned(),
            reason: "no agent named pay-qa".to_owned(),
        },
        Event::RoleAssigned {
            role: role_key(None, "Generalist"),
            task: "t".to_owned(),
            outcome: RoleOutcome::Given {
                agent: agent("squad-1"),
                given: Given::TypedIn,
            },
        },
        Event::AssignRefused {
            role: role_key(None, "Generalist"),
            task: "t".to_owned(),
            reason: "squad-1 isn't free any more".to_owned(),
        },
    ];
    for e in &events {
        round_trip(e);
    }
    let oks = [
        ToolOk::Rights(Rights::Duo),
        ToolOk::Agents {
            you: agent("Duo"),
            agents: vec![AgentEntry {
                name: agent("pay-qa"),
                team: Some(node("Payments")),
                role: Some(node("QA")),
                state: AgentState::Idle,
                quiet: false,
                from_screen: false,
                for_secs: 12,
                last_reply: Some("done".to_owned()),
                waiting_messages: 1,
                cli: crate::cli_kind::CliKind::Opencode,
                worktree: None,
                free: true,
            }],
        },
        ToolOk::Queued {
            message: MessageId(4),
            outcome: SendOutcome::Queued(AgentState::Stopped),
        },
        ToolOk::Status {
            message: MessageId(4),
            to: agent("pay-qa"),
            status: MessageStatus::Dropped {
                reason: "x".to_owned(),
            },
            reason: Some("x".to_owned()),
        },
        ToolOk::Screen {
            agent: agent("pay-qa"),
            state: AgentState::Waiting,
            text: "Allow?".to_owned(),
        },
        ToolOk::Waited {
            agent: agent("pay-qa"),
            outcome: WaitOutcome::TimedOut,
            state: AgentState::Working,
            last_reply: None,
        },
        ToolOk::Given {
            agent: agent("pay-qa"),
            given: Given::Started { worktree: true },
        },
        ToolOk::Stopped {
            agent: agent("pay-qa"),
        },
        ToolOk::Pending { card: CardId(3) },
    ];
    for ok in oks {
        round_trip(&Ok::<ToolOk, ToolError>(ok));
    }
    let codes = [
        ErrorCode::UnknownToken,
        ErrorCode::NotAllowed,
        ErrorCode::DuohiveClosed,
        ErrorCode::Interrupted,
        ErrorCode::InvalidArguments,
        ErrorCode::UnknownAgent,
        ErrorCode::UnknownRole,
        ErrorCode::UnknownMessage,
        ErrorCode::Refused,
        ErrorCode::Limit(LimitKind::Duplicate),
        ErrorCode::Limit(LimitKind::Rate),
        ErrorCode::Limit(LimitKind::InboxFull),
        ErrorCode::InvalidChange,
        ErrorCode::NoScreen,
        ErrorCode::Unavailable,
    ];
    for code in codes {
        round_trip(&Err::<ToolOk, ToolError>(ToolError {
            code,
            message: "m".to_owned(),
        }));
    }
}
