//! Action lines (P3-D14): one `→` entry per answered call of Duo's.

use super::super::super::harness::T0;
use super::*;
use crate::chat::cut;
use crate::inbox::MessageId;
use crate::launch::Token;
use crate::org::RoleKey;
use crate::ports::{LiveWindow, QueuedToolCall, SessionRow};
use crate::status::Status;

/// Duo's call: its result and its action lines, checking there is at most one.
fn line_of(h: &mut Harness, call: ToolCall) -> (ToolResult, (String, String)) {
    let (results, events) = tool(h, 1, call);
    let lines = actions(&events);
    let [line] = lines.as_slice() else {
        panic!("expected one action line, got {lines:?}");
    };
    (only(&results, 1), line.clone())
}

fn pair(text: &str, result: &str) -> (String, String) {
    (text.to_owned(), result.to_owned())
}

#[test]
fn every_call_is_an_action_line() {
    let mut h = with_duo();
    working(&mut h, "billing-api");
    idle(&mut h, "squad-1");

    let (_, line) = line_of(&mut h, ToolCall::ListAgents);
    assert_eq!(line, pair("list agents", "7 agents"));

    let text = "please rerun the integration tests and report back";
    let send = ToolCall::SendMessage {
        to: n("billing-api"),
        text: text.into(),
    };
    let (result, line) = line_of(&mut h, send);
    let Ok(ToolOk::Queued { message, .. }) = result else {
        panic!("{result:?}");
    };
    let quoted = format!("@billing-api \"{}\"", cut(text, 40));
    assert!(quoted.ends_with("…\""), "{quoted}");
    assert_eq!(line, (quoted, "queued (billing-api is working)".into()));

    let status = ToolCall::MessageStatus { id: message };
    let (_, line) = line_of(&mut h, status);
    assert_eq!(
        line,
        (format!("message {} status", message.0), "queued".into())
    );
    let unknown = ToolCall::MessageStatus { id: MessageId(99) };
    let (_, line) = line_of(&mut h, unknown);
    assert_eq!(
        line,
        pair("message 99 status", "refused (you sent no message 99)")
    );

    let (_, line) = line_of(&mut h, read("pay-qa"));
    assert_eq!(line, pair("read pay-qa", "refused (pay-qa is stopped)"));

    let (_, line) = line_of(&mut h, wait("squad-1"));
    assert_eq!(line, pair("wait for squad-1", "idle"));

    let (_, line) = line_of(&mut h, start("billing-api", None));
    assert_eq!(
        line,
        (
            format!("give work billing-api \"{TASK}\""),
            "refused (billing-api is working; send_message instead)".into()
        )
    );

    let (_, line) = line_of(&mut h, stop("billing-api"));
    assert_eq!(line, pair("stop billing-api", "card raised"));

    let assign = ToolCall::AssignToRole {
        role: RoleKey {
            team: None,
            role: NodeName::parse("Generalist").unwrap(),
        },
        task: TASK.into(),
    };
    let (_, line) = line_of(&mut h, assign);
    assert_eq!(
        line,
        (format!("give work squad-1 \"{TASK}\""), "typed in".into())
    );

    let propose = parse_call(
        "propose_org_change",
        &json!({"change": {"op": "add_team", "name": "Growth", "fields": {}}}),
    )
    .unwrap();
    let (_, line) = line_of(&mut h, propose);
    assert_eq!(line, pair("propose org change", "card raised"));

    // `Rights` isn't a tool: no line.
    let (results, events) = tool(&mut h, 1, ToolCall::Rights);
    assert_eq!(results.len(), 1);
    assert!(actions(&events).is_empty(), "{events:?}");
}

#[test]
fn other_agents_calls_write_no_line() {
    let mut h = with_duo();
    idle(&mut h, "squad-1");
    let token = h.rt("squad-1").token.clone().unwrap();
    let out = h.input(Input::Command(Command::Tool {
        call_id: CallId(4),
        token,
        call: ToolCall::ListAgents,
    }));
    let (results, events) = split(out);
    assert!(matches!(only(&results, 4), Ok(ToolOk::Agents { .. })));
    assert!(actions(&events).is_empty(), "{events:?}");
}

#[test]
fn assign_lines_name_the_role_until_an_agent_takes_it() {
    let mut h = with_duo();
    for agent in ["squad-1", "squad-2", "squad-3"] {
        working(&mut h, agent);
    }
    let assign = ToolCall::AssignToRole {
        role: RoleKey {
            team: None,
            role: NodeName::parse("Generalist").unwrap(),
        },
        task: TASK.into(),
    };
    let (_, line) = line_of(&mut h, assign);
    assert_eq!(
        line,
        (
            format!("give work Generalist \"{TASK}\""),
            "card raised".into()
        )
    );
}

/// A restored idle session for `agent` on window `@<agent>`, holding `token`.
fn idle_row(agent: &str, token: &str, launched_as_duo: bool) -> SessionRow {
    SessionRow {
        agent: n(agent),
        token: Some(Token(token.into())),
        window: Some(WindowId(format!("@{agent}"))),
        status: Status {
            state: AgentState::Idle,
            quiet: false,
            quiet_since: None,
            from_screen: false,
            since: T0,
            error: None,
            waiting_on: None,
        },
        last_reply: None,
        task_at_launch: false,
        saw_end_hook: false,
        last_task: None,
        worktree: None,
        launched_as_duo,
    }
}

#[test]
fn replayed_duo_calls_write_their_action_lines() {
    // P3-D24: calls queued while closed run after `Reconnected`, each with
    // its action line when Duo made it, and no result.
    let mut h = Harness::new();
    {
        let mut st = h.store.state.lock().unwrap();
        st.sessions
            .insert(n("Duo"), idle_row("Duo", "tok-duo", true));
        st.sessions
            .insert(n("squad-1"), idle_row("squad-1", "tok-squad", false));
        let calls = [
            (
                "tok-duo",
                "propose_org_change",
                json!({"change": {"op": "add_team", "name": "Growth", "fields": {}}}),
            ),
            (
                "tok-squad",
                "send_message",
                json!({"to": "pay-qa", "text": "hi"}),
            ),
        ];
        for (id, (token, tool, arguments)) in (1..).zip(calls) {
            st.queued_tool_calls.push(QueuedToolCall {
                id,
                token: Token(token.into()),
                tool: tool.into(),
                arguments,
                queued_at: T0,
            });
        }
    }
    let mut events = h.engine.start_up();
    let windows = ["Duo", "squad-1"].map(|agent| LiveWindow {
        agent: n(agent),
        window: WindowId(format!("@{agent}")),
        dead: None,
        activity_secs: 0,
    });
    events.extend(h.input(Input::Runtime(RuntimeEvent::Inventory(windows.to_vec()))));
    assert!(events.contains(&Event::Reconnected), "{events:?}");
    let (results, events) = split(events);
    assert!(results.is_empty(), "{results:?}");
    assert_eq!(
        actions(&events),
        [pair("propose org change", "card raised")]
    );
}
