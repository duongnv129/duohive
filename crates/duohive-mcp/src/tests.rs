use std::collections::HashMap;

use duohive_core::card::CardId;
use duohive_core::command::Command;
use duohive_core::engine::Input;
use duohive_core::inbox::{MessageId, MessageStatus};
use duohive_core::names::AgentName;
use duohive_core::status::AgentState;
use duohive_core::tool::{
    CallId, ErrorCode, Given, LimitKind, Rights, SendOutcome, ToolCall, ToolError, ToolOk,
    ToolResult, WaitOutcome,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use crate::calls::Calls;
use crate::endpoint::serve_request;
use crate::protocol::{SockAnswer, SockLine, parse_line};
use crate::tools::{render, specs};

fn agent(s: &str) -> AgentName {
    AgentName::parse(s).unwrap()
}

#[test]
fn kindless_line_is_a_hook() {
    let hook = r#"{"token":"t","agent":"pay-qa","event":"Stop","payload":{}}"#;
    assert!(matches!(parse_line(hook).unwrap(), SockLine::Hook(_)));
    let tagged = r#"{"kind":"hook","token":"t","agent":"pay-qa","event":"Stop","payload":{}}"#;
    assert!(matches!(parse_line(tagged).unwrap(), SockLine::Hook(_)));
    let call = r#"{"kind":"call","token":"t","tool":"list_agents","arguments":{}}"#;
    match parse_line(call).unwrap() {
        SockLine::Call {
            token,
            tool,
            arguments,
        } => {
            assert_eq!(token, "t");
            assert_eq!(tool, "list_agents");
            assert_eq!(arguments, json!({}));
        }
        other => panic!("{other:?}"),
    }
    let tools = r#"{"kind":"tools","token":"t"}"#;
    assert!(matches!(parse_line(tools).unwrap(), SockLine::Tools { .. }));
    assert!(parse_line(r#"{"kind":"nope"}"#).is_err());
}

/// The required fields P3-D27 gives each tool.
fn required_fields() -> HashMap<&'static str, Vec<&'static str>> {
    HashMap::from([
        ("list_agents", vec![]),
        ("send_message", vec!["to", "text"]),
        ("message_status", vec!["message_id"]),
        ("read_agent", vec!["agent"]),
        ("wait_for_agent", vec!["agent"]),
        ("start_agent", vec!["agent", "task"]),
        ("stop_agent", vec!["agent"]),
        ("assign_to_role", vec!["role", "task"]),
        ("propose_org_change", vec!["change"]),
    ])
}

fn required(schema: &Value) -> Vec<String> {
    schema["required"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

/// The `oneOf` branch for `change.op`, and whether `change` has its required fields.
fn change_validates(schema: &Value, change: &Value) -> bool {
    let op = change["op"].as_str().unwrap();
    let branches = schema["properties"]["change"]["oneOf"].as_array().unwrap();
    let matching: Vec<&Value> = branches
        .iter()
        .filter(|b| b["properties"]["op"]["const"] == op)
        .collect();
    matching.iter().any(|b| {
        b["type"] == "object"
            && required(b)
                .iter()
                .all(|f| change.get(f).is_some_and(|v| !v.is_null() || f == "team"))
    })
}

#[test]
fn specs_match_rights() {
    let agent_tools: Vec<&str> = specs(Rights::Agent).iter().map(|s| s.name).collect();
    assert_eq!(
        agent_tools,
        ["list_agents", "send_message", "message_status"]
    );
    let duo = specs(Rights::Duo);
    assert_eq!(duo.len(), 9);
    let want = required_fields();
    for spec in &duo {
        assert_eq!(spec.input_schema["type"], "object", "{}", spec.name);
        assert!(!spec.description.is_empty(), "{}", spec.name);
        assert_eq!(
            required(&spec.input_schema),
            want[spec.name],
            "{}",
            spec.name
        );
    }
    let propose = duo.iter().find(|s| s.name == "propose_org_change").unwrap();
    for (change, _) in duohive_core::testkit::change_examples() {
        assert!(
            change_validates(&propose.input_schema, &change),
            "{change} doesn't validate"
        );
    }
}

fn ok_json(r: &ToolResult) -> Value {
    match render(r) {
        SockAnswer::Ok(v) => v,
        SockAnswer::Error { .. } => panic!("not ok: {r:?}"),
    }
}

#[test]
fn render_matches_p3_d16() {
    assert_eq!(
        ok_json(&Ok(ToolOk::Pending { card: CardId(3) })),
        json!({"status": "pending", "card_id": 3})
    );
    assert_eq!(
        ok_json(&Ok(ToolOk::Queued {
            message: MessageId(42),
            outcome: SendOutcome::Queued(AgentState::Stopped),
        })),
        json!({"message_id": 42, "status": "queued", "recipient_state": "stopped"})
    );
    assert_eq!(
        ok_json(&Ok(ToolOk::Queued {
            message: MessageId(42),
            outcome: SendOutcome::TypedIn,
        })),
        json!({"message_id": 42, "status": "typed_in"})
    );
    assert_eq!(
        ok_json(&Ok(ToolOk::Status {
            message: MessageId(42),
            to: agent("pay-qa"),
            status: MessageStatus::Queued,
            reason: Some("pay-qa is stopped".to_owned()),
        })),
        json!({"message_id": 42, "to": "pay-qa", "status": "queued", "reason": "pay-qa is stopped"})
    );
    assert_eq!(
        ok_json(&Ok(ToolOk::Screen {
            agent: agent("pay-qa"),
            state: AgentState::Waiting,
            text: "Allow?".to_owned(),
        })),
        json!({"agent": "pay-qa", "state": "waiting", "screen": "Allow?"})
    );
    assert_eq!(
        ok_json(&Ok(ToolOk::Waited {
            agent: agent("pay-qa"),
            outcome: WaitOutcome::TurnEnded,
            state: AgentState::Finished,
            last_reply: Some("done".to_owned()),
        })),
        json!({"agent": "pay-qa", "outcome": "turn_ended", "state": "finished", "last_reply": "done"})
    );
    assert_eq!(
        ok_json(&Ok(ToolOk::Given {
            agent: agent("pay-qa"),
            given: Given::Started { worktree: true },
        })),
        json!({"agent": "pay-qa", "result": "started", "worktree": true})
    );
    assert_eq!(
        ok_json(&Ok(ToolOk::Given {
            agent: agent("pay-qa"),
            given: Given::TypedIn,
        })),
        json!({"agent": "pay-qa", "result": "typed_in", "worktree": false})
    );
    assert_eq!(
        ok_json(&Ok(ToolOk::Stopped {
            agent: agent("pay-qa")
        })),
        json!({"agent": "pay-qa", "result": "stopped"})
    );
    let codes = [
        (ErrorCode::UnknownToken, "unknown_token"),
        (ErrorCode::NotAllowed, "not_allowed"),
        (ErrorCode::DuohiveClosed, "duohive_closed"),
        (ErrorCode::Interrupted, "interrupted"),
        (ErrorCode::InvalidArguments, "invalid_arguments"),
        (ErrorCode::UnknownAgent, "unknown_agent"),
        (ErrorCode::UnknownRole, "unknown_role"),
        (ErrorCode::UnknownMessage, "unknown_message"),
        (ErrorCode::Refused, "refused"),
        (ErrorCode::Limit(LimitKind::Rate), "limit"),
        (ErrorCode::InvalidChange, "invalid_change"),
        (ErrorCode::NoScreen, "no_screen"),
        (ErrorCode::Unavailable, "unavailable"),
    ];
    for (code, word) in codes {
        let answer = render(&Err(ToolError::new(code, "m")));
        let line: Value = serde_json::from_str(&answer.to_line()).unwrap();
        assert_eq!(line["error"]["code"], word);
        assert_eq!(line["error"]["message"], "m");
        if code == ErrorCode::Limit(LimitKind::Rate) {
            assert_eq!(line["error"]["limit"], "rate");
        }
    }
}

/// A fake core: answers each `Command::Tool` from `answer`, through `calls`.
fn fake_core(
    calls: Calls,
    answer: impl Fn(&ToolCall) -> ToolResult + Send + 'static,
) -> (mpsc::Sender<Input>, mpsc::UnboundedReceiver<ToolCall>) {
    let (tx, mut rx) = mpsc::channel::<Input>(8);
    let (seen_tx, seen_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(input) = rx.recv().await {
            if let Input::Command(Command::Tool { call_id, call, .. }) = input {
                let _ = seen_tx.send(call.clone());
                calls.resolve(call_id, answer(&call));
            }
        }
    });
    (tx, seen_rx)
}

async fn ask(line: &str, to_core: mpsc::Sender<Input>, calls: Calls) -> Value {
    let (client, server) = UnixStream::pair().unwrap();
    let parsed = parse_line(line).unwrap();
    tokio::spawn(serve_request(parsed, server, to_core, calls));
    let mut reader = BufReader::new(client);
    let mut answer = String::new();
    reader.read_line(&mut answer).await.unwrap();
    serde_json::from_str(&answer).unwrap()
}

#[tokio::test]
async fn endpoint_answers_unknown_token_error() {
    let calls = Calls::new();
    let (to_core, _) = fake_core(calls.clone(), |_| {
        Err(ToolError::new(ErrorCode::UnknownToken, "unknown token"))
    });
    let v = ask(
        r#"{"kind":"call","token":"bad","tool":"list_agents","arguments":{}}"#,
        to_core,
        calls,
    )
    .await;
    assert_eq!(v["error"]["code"], "unknown_token");
}

#[tokio::test]
async fn tools_list_for_non_duo_lacks_duo_tools() {
    let calls = Calls::new();
    let (to_core, _) = fake_core(calls.clone(), |_| Ok(ToolOk::Rights(Rights::Agent)));
    let v = ask(r#"{"kind":"tools","token":"t"}"#, to_core, calls).await;
    let names: Vec<&str> = v["ok"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["list_agents", "send_message", "message_status"]);
}

#[tokio::test]
async fn bad_arguments_answered_without_core() {
    let calls = Calls::new();
    let (to_core, mut seen) = fake_core(calls.clone(), |_| Ok(ToolOk::Rights(Rights::Duo)));
    let v = ask(
        r#"{"kind":"call","token":"t","tool":"send_message","arguments":{"text":"hi"}}"#,
        to_core,
        calls,
    )
    .await;
    assert_eq!(v["error"]["code"], "invalid_arguments");
    assert!(seen.try_recv().is_err(), "the core saw a command");
}

#[tokio::test]
async fn result_reaches_only_its_call() {
    let calls = Calls::new();
    let (to_core, mut rx) = mpsc::channel::<Input>(8);
    let first = tokio::spawn(ask(
        r#"{"kind":"call","token":"t","tool":"read_agent","arguments":{"agent":"one"}}"#,
        to_core.clone(),
        calls.clone(),
    ));
    let second = tokio::spawn(ask(
        r#"{"kind":"call","token":"t","tool":"read_agent","arguments":{"agent":"two"}}"#,
        to_core,
        calls.clone(),
    ));
    let mut ids = Vec::new();
    for _ in 0..2 {
        if let Some(Input::Command(Command::Tool { call_id, call, .. })) = rx.recv().await {
            ids.push((call_id, call));
        }
    }
    // Answer in reverse order; each answer names the agent its call asked about.
    for (id, call) in ids.into_iter().rev() {
        let ToolCall::ReadAgent { agent } = call else {
            panic!("{call:?}")
        };
        calls.resolve(
            id,
            Ok(ToolOk::Screen {
                agent: agent.clone(),
                state: AgentState::Idle,
                text: agent.as_str().to_owned(),
            }),
        );
    }
    assert_eq!(first.await.unwrap()["ok"]["screen"], "one");
    assert_eq!(second.await.unwrap()["ok"]["screen"], "two");
}

#[tokio::test]
async fn early_close_forgets_call() {
    let calls = Calls::new();
    let (to_core, mut rx) = mpsc::channel::<Input>(8);
    let (client, server) = UnixStream::pair().unwrap();
    let line =
        parse_line(r#"{"kind":"call","token":"t","tool":"list_agents","arguments":{}}"#).unwrap();
    let served = tokio::spawn(serve_request(line, server, to_core, calls.clone()));
    let Some(Input::Command(Command::Tool { call_id, .. })) = rx.recv().await else {
        panic!("no call")
    };
    let mut client = client;
    client.shutdown().await.unwrap();
    drop(client);
    served.await.unwrap();
    assert!(!calls.is_open(call_id));
    calls.resolve(call_id, Ok(ToolOk::Rights(Rights::Duo)));
}

#[test]
fn replay_id_never_issued() {
    let calls = Calls::new();
    for _ in 0..100 {
        let (id, _rx) = calls.open();
        assert_ne!(id, CallId::REPLAY);
    }
    calls.resolve(CallId::REPLAY, Ok(ToolOk::Rights(Rights::Duo)));
    assert!(!calls.is_open(CallId::REPLAY));
}
