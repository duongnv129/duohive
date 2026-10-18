//! The chat pane: history, action lines, queued lines and suggestions (UI §4.6, §4.7).

use std::collections::BTreeSet;

use duohive_core::chat::ChatKind;
use duohive_core::inbox::MessageId;
use duohive_core::names::NodeName;
use duohive_core::org::{NodeKey, RoleKey};
use duohive_core::status::AgentState;
use duohive_core::testkit::n;

use crate::fixtures::{
    action_entry, chat_entry, clock, ctx, mins, render_lines, spec_model, status,
};
use crate::focus::Pane;
use crate::forms::LineEdit;
use crate::model::UiModel;
use crate::suggest::suggestions;

fn nn(s: &str) -> NodeName {
    NodeName::parse(s).unwrap()
}

/// UI §4.1's chat: your question, Duo's answer and an action line, a role selected.
fn history() -> UiModel {
    let mut m = spec_model();
    m.selected = NodeKey::Role(RoleKey {
        team: Some(nn("Payments")),
        role: nn("Backend"),
    });
    let at = clock(10, 5);
    m.chat.entries = vec![
        chat_entry(
            1,
            at,
            ChatKind::You { message: None },
            "what's blocking the Payments team?",
        ),
        chat_entry(
            2,
            at,
            ChatKind::Duo,
            "billing-api is waiting on a file-edit approval; auth-service is mid-test.",
        ),
        action_entry(
            3,
            at,
            "@auth-service \"run the tests again\"",
            "queued (auth-service is working)",
        ),
    ];
    m
}

fn chat_rows(lines: &[String]) -> Vec<String> {
    let top = lines
        .iter()
        .position(|l| l.starts_with("┌─ ⬢") || l.starts_with("┏━ ⬢"))
        .unwrap();
    lines[top..lines.len() - 1].to_vec()
}

#[test]
fn chat_history_snapshots() {
    let m = history();
    for (name, w, h) in [
        ("chat_history_80x24", 80, 24),
        ("chat_history_160x48", 160, 48),
    ] {
        let lines = render_lines(&m, &ctx(), w, h);
        insta::assert_snapshot!(name, chat_rows(&lines).join("\n"));
    }
    let wide = render_lines(&m, &ctx(), 160, 48).join("\n");
    assert!(
        wide.contains("│ 10:05 you   what's blocking the Payments team?"),
        "{wide}"
    );
    assert!(
        wide.contains(
            "│             → @auth-service \"run the tests again\"   queued (auth-service is working)"
        ),
        "{wide}"
    );
}

#[test]
fn chat_suggest_snapshots() {
    for (name, input) in [
        ("chat_suggest_agents_160x48", "@"),
        ("chat_suggest_commands_160x48", "/"),
    ] {
        let mut m = spec_model();
        m.focus.pane = Pane::Chat;
        m.chat_input = LineEdit::new(input);
        m.suggest = suggestions(input, 1, &m);
        let lines = render_lines(&m, &ctx(), 160, 48);
        insta::assert_snapshot!(name, chat_rows(&lines).join("\n"));
    }
    let mut m = spec_model();
    m.focus.pane = Pane::Chat;
    m.chat_input = LineEdit::new("/");
    m.suggest = suggestions("/", 1, &m);
    let text = render_lines(&m, &ctx(), 160, 48).join("\n");
    assert!(text.contains("│▌/help     commands and keys"), "{text}");
    assert!(
        text.contains("│ /agents   every agent and its state"),
        "{text}"
    );
}

#[test]
fn chat_queued_you_line_snapshot() {
    let mut m = history();
    m.chat.entries.truncate(1);
    m.chat.entries[0].kind = ChatKind::You {
        message: Some(MessageId(5)),
    };
    m.queued = BTreeSet::from([MessageId(5)]);
    m.agents.get_mut(&n("Duo")).unwrap().status = status(AgentState::Working, mins(1));
    let lines = render_lines(&m, &ctx(), 160, 48);
    insta::assert_snapshot!("chat_queued_you_line_160x48", chat_rows(&lines).join("\n"));
    let row = lines
        .iter()
        .find(|l| l.contains("10:05 you"))
        .unwrap()
        .trim_end();
    assert!(row.ends_with("queued (Duo is working) │"), "{row}");
    // Delivered: the tag goes.
    m.apply(duohive_core::event::Event::MessageDelivered { id: MessageId(5) });
    let text = render_lines(&m, &ctx(), 160, 48).join("\n");
    assert!(!text.contains("queued (Duo"), "{text}");
}

#[test]
fn chat_80x24_shows_message_start() {
    let mut m = spec_model();
    m.chat.entries = vec![
        chat_entry(1, clock(10, 0), ChatKind::You { message: None }, "status?"),
        chat_entry(
            2,
            clock(10, 1),
            ChatKind::Duo,
            "line one\nline two\nline three\nline four\nline five",
        ),
    ];
    let lines = render_lines(&m, &ctx(), 80, 24);
    let chat = chat_rows(&lines).join("\n");
    assert!(chat.contains("10:01 Duo  line one"), "{chat}");
    assert!(!chat.contains("line five"), "{chat}");
}

#[test]
fn hanging_indent_wraps_under_text() {
    let mut m = spec_model();
    m.chat.entries = vec![
        chat_entry(1, clock(10, 0), ChatKind::Duo, &"word ".repeat(30)),
        action_entry(
            2,
            clock(10, 0),
            &format!("give work pay-qa \"{}\"", "x ".repeat(80)),
            "started",
        ),
    ];
    m.focus.pane = Pane::Chat;
    let lines = render_lines(&m, &ctx(), 120, 36);
    let chat = chat_rows(&lines);
    let at = chat
        .iter()
        .position(|l| l.starts_with("┃ 10:00 Duo   word"))
        .unwrap();
    assert!(chat[at + 1].starts_with("┃             word"), "{chat:#?}");
    let arrow = chat
        .iter()
        .position(|l| l.starts_with("┃             → give work"))
        .unwrap();
    assert!(
        chat[arrow + 1].starts_with("┃               x x"),
        "{chat:#?}"
    );
    assert!(chat.iter().any(|l| l.contains("   started")), "{chat:#?}");
}
