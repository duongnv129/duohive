//! The Duo chat's routing, suggestions and message events (UI §4.7, P3-D26).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::chat::ChatKind;
use duohive_core::command::Command;
use duohive_core::event::Event;
use duohive_core::inbox::{MessageId, Sender};
use duohive_core::status::AgentState;
use duohive_core::testkit::n;

use super::{UiEffect, map_key, update};
use crate::chat::{Route, route};
use crate::fixtures::{NOW, action_entry, ctx, mins, render_lines, spec_model, status};
use crate::focus::Pane;
use crate::model::UiModel;
use crate::suggest::{SuggestKind, suggestions};

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn code(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::NONE)
}

fn press(m: &mut UiModel, k: KeyEvent) -> Vec<UiEffect> {
    match map_key(m, k) {
        Some(a) => update(m, a),
        None => Vec::new(),
    }
}

fn type_text(m: &mut UiModel, text: &str) {
    for c in text.chars() {
        assert_eq!(press(m, key(c)), []);
    }
}

/// Types `text`, closes any suggestion list, and presses `enter`.
fn send_line(m: &mut UiModel, text: &str) -> Vec<UiEffect> {
    type_text(m, text);
    m.suggest = None;
    press(m, code(KeyCode::Enter))
}

fn chat_focused() -> UiModel {
    let mut m = spec_model();
    m.focus.pane = Pane::Chat;
    m
}

fn set_state(m: &mut UiModel, agent: &str, state: AgentState) {
    m.agents.get_mut(&n(agent)).unwrap().status = status(state, mins(1));
}

fn screen(m: &UiModel) -> String {
    render_lines(m, &ctx(), 160, 48).join("\n")
}

fn items(m: &UiModel) -> Vec<String> {
    m.suggest
        .as_ref()
        .map(|s| s.items.iter().map(|(i, _)| i.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn at_agent_routes_by_state() {
    let cases = [
        (AgentState::Working, "queued (pay-qa is working)", 1),
        (AgentState::Idle, "typed in", 0),
        (AgentState::Stopped, "started", 1),
    ];
    for (state, result, inbox) in cases {
        let mut m = chat_focused();
        set_state(&mut m, "pay-qa", state);
        assert_eq!(
            send_line(&mut m, "@pay-qa hi"),
            [UiEffect::Send(Command::SendMessage {
                to: n("pay-qa"),
                text: "hi".to_owned(),
            })],
            "{state:?}"
        );
        assert_eq!(m.chat_input.text(), "");
        // The core's line (Task 3.8's `your_message_writes_action_line`).
        m.apply(Event::ChatLine(action_entry(
            1,
            NOW,
            "@pay-qa \"hi\"",
            result,
        )));
        m.apply(Event::InboxChanged {
            agent: n("pay-qa"),
            waiting: inbox,
        });
        let text = screen(&m);
        assert!(
            text.contains(&format!("→ @pay-qa \"hi\"   {result}")),
            "{text}"
        );
        let row = text.lines().find(|l| l.contains("─ pay-qa")).unwrap();
        assert_eq!(row.contains("pay-qa ✉ 1"), inbox == 1, "{row}");
    }
    // The compact tree: `✉1`.
    let mut m = spec_model();
    m.apply(Event::InboxChanged {
        agent: n("auth-service"),
        waiting: 1,
    });
    let compact = render_lines(&m, &ctx(), 80, 24).join("\n");
    assert!(compact.contains("auth-service ✉1"), "{compact}");
}

#[test]
fn plain_text_goes_to_duo() {
    let mut m = chat_focused();
    // Even while Duo is busy: the core queues it (UI §4.7).
    set_state(&mut m, "Duo", AgentState::Working);
    assert_eq!(
        send_line(&mut m, "what's blocking Payments?"),
        [UiEffect::Send(Command::ChatToDuo {
            text: "what's blocking Payments?".to_owned(),
        })]
    );
    assert_eq!(m.chat_input.text(), "");
    assert_eq!(
        send_line(&mut m, "@Duo hi"),
        [UiEffect::Send(Command::ChatToDuo {
            text: "hi".to_owned(),
        })]
    );
    // An empty input sends nothing; alt+enter adds a line.
    assert_eq!(press(&mut m, code(KeyCode::Enter)), []);
    type_text(&mut m, "a");
    press(&mut m, KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
    type_text(&mut m, "b");
    assert_eq!(
        press(&mut m, code(KeyCode::Enter)),
        [UiEffect::Send(Command::ChatToDuo {
            text: "a\nb".to_owned(),
        })]
    );
    assert_eq!(route("  hello ", &m), Route::Duo("hello".to_owned()));
}

#[test]
fn unknown_agent_keeps_text() {
    let mut m = chat_focused();
    assert_eq!(send_line(&mut m, "@nobody hi"), []);
    assert_eq!(m.footer_note.as_deref(), Some("no agent named nobody"));
    assert_eq!(m.chat_input.text(), "@nobody hi");
    assert!(m.chat.entries.is_empty());
}

#[test]
fn message_refused_keeps_text() {
    let mut m = chat_focused();
    send_line(&mut m, "@pay-qa  ");
    m.apply(Event::MessageRefused {
        to: n("pay-qa"),
        text: String::new(),
        reason: "the message is empty".to_owned(),
    });
    assert_eq!(m.chat_input.text(), "@pay-qa");
    assert_eq!(m.footer_note.as_deref(), Some("the message is empty"));
}

#[test]
fn slash_help_and_agents_are_local() {
    let mut m = chat_focused();
    assert_eq!(send_line(&mut m, "/help"), []);
    assert_eq!(m.chat_input.text(), "");
    assert!(!m.chat.entries.is_empty());
    assert!(m.chat.entries.iter().all(|e| e.kind == ChatKind::Note));
    let help = screen(&m);
    assert!(help.contains("alt+enter new line"), "{help}");
    let before = m.chat.entries.len();
    assert_eq!(send_line(&mut m, "/agents"), []);
    let notes: Vec<&str> = m.chat.entries[before..]
        .iter()
        .map(|e| e.text.as_str())
        .collect();
    assert!(
        notes
            .iter()
            .any(|t| t.contains("auth-service") && t.contains("● working")),
        "{notes:#?}"
    );
    assert!(notes.iter().any(|t| t.starts_with("Duo ")), "{notes:#?}");
    assert_eq!(send_line(&mut m, "/x"), []);
    assert_eq!(
        m.footer_note.as_deref(),
        Some("unknown command /x · /help lists them")
    );
    assert_eq!(m.chat_input.text(), "/x");
    assert_eq!(route("/agents", &m), Route::Agents);
    assert_eq!(route("@nobody hi", &m), Route::NoAgent("nobody".to_owned()));
}

#[test]
fn suggestions_filter_and_take() {
    let mut m = chat_focused();
    let all = suggestions("@", 1, &m).unwrap();
    assert_eq!(all.kind, SuggestKind::Agent);
    let names: Vec<&str> = all.items.iter().map(|(i, _)| i.as_str()).collect();
    assert_eq!(
        names,
        [
            "@Duo",
            "@auth-service",
            "@billing-api",
            "@pay-qa",
            "@squad-1",
            "@squad-2",
            "@squad-3"
        ]
    );
    type_text(&mut m, "@a");
    assert_eq!(items(&m), ["@auth-service"]);
    assert_eq!(press(&mut m, code(KeyCode::Enter)), []);
    assert_eq!(m.chat_input.text(), "@auth-service ");
    assert_eq!(m.suggest, None);
    // `tab` takes too, and ↓ moves the cursor.
    let mut m = chat_focused();
    type_text(&mut m, "@squad");
    assert_eq!(items(&m), ["@squad-1", "@squad-2", "@squad-3"]);
    press(&mut m, code(KeyCode::Down));
    assert_eq!(press(&mut m, code(KeyCode::Tab)), []);
    assert_eq!(m.chat_input.text(), "@squad-2 ");
    assert_eq!(m.focus.pane, Pane::Chat);
    let mut m = chat_focused();
    type_text(&mut m, "/");
    let s = m.suggest.clone().unwrap();
    assert_eq!(s.kind, SuggestKind::Command);
    assert_eq!(
        s.items,
        [
            ("/help".to_owned(), "commands and keys".to_owned()),
            ("/agents".to_owned(), "every agent and its state".to_owned()),
        ]
    );
    type_text(&mut m, "z");
    assert_eq!(m.suggest, None);
    // No list after a space, or for text that isn't `@` or `/`.
    assert_eq!(suggestions("@pay-qa hi", 10, &m), None);
    assert_eq!(suggestions("hello", 5, &m), None);
}

#[test]
fn messages_between_agents_not_in_chat() {
    let mut m = chat_focused();
    m.apply(Event::MessageQueued {
        id: MessageId(7),
        from: Sender::Agent(n("squad-1")),
        to: n("pay-qa"),
    });
    m.apply(Event::InboxChanged {
        agent: n("pay-qa"),
        waiting: 1,
    });
    assert!(m.chat.entries.is_empty());
    assert!(m.queued.is_empty());
    let text = screen(&m);
    assert!(text.contains("pay-qa ✉ 1"), "{text}");
}

#[test]
fn dropped_message_returns_to_input() {
    let mut m = chat_focused();
    send_line(&mut m, "@pay-qa rerun");
    m.apply(Event::MessageQueued {
        id: MessageId(3),
        from: Sender::You,
        to: n("pay-qa"),
    });
    assert!(m.queued.contains(&MessageId(3)));
    m.apply(Event::MessageDropped {
        id: MessageId(3),
        from: Sender::You,
        to: n("pay-qa"),
        text: "rerun".to_owned(),
        reason: "pay-qa stopped before taking it".to_owned(),
    });
    assert!(m.queued.is_empty());
    assert_eq!(m.chat_input.text(), "@pay-qa rerun");
    assert_eq!(
        m.footer_note.as_deref(),
        Some("pay-qa stopped before taking it · enter to send again")
    );
    // To Duo: just the text.
    let mut m = chat_focused();
    m.apply(Event::MessageDropped {
        id: MessageId(4),
        from: Sender::You,
        to: n("Duo"),
        text: "hi".to_owned(),
        reason: "Duo stopped before taking it".to_owned(),
    });
    assert_eq!(m.chat_input.text(), "hi");
    // An agent's dropped message is not yours.
    let mut m = chat_focused();
    m.apply(Event::MessageDropped {
        id: MessageId(5),
        from: Sender::Agent(n("squad-1")),
        to: n("pay-qa"),
        text: "x".to_owned(),
        reason: "r".to_owned(),
    });
    assert_eq!(m.chat_input.text(), "");
    assert_eq!(m.footer_note, None);
}

#[test]
fn queued_follows_message_events() {
    let mut m = chat_focused();
    for id in [1, 2] {
        m.apply(Event::MessageQueued {
            id: MessageId(id),
            from: Sender::You,
            to: n("Duo"),
        });
    }
    m.apply(Event::MessageDelivered { id: MessageId(1) });
    assert_eq!(m.queued.iter().copied().collect::<Vec<_>>(), [MessageId(2)]);
}

#[test]
fn esc_layers_popup_card_pane() {
    let mut m = chat_focused();
    type_text(&mut m, "@");
    assert!(m.suggest.is_some());
    assert_eq!(press(&mut m, code(KeyCode::Esc)), []);
    assert_eq!(m.suggest, None);
    assert_eq!(m.focus.pane, Pane::Chat);
    assert_eq!(press(&mut m, code(KeyCode::Esc)), []);
    assert_eq!(m.focus.pane, Pane::Org);
    assert_eq!(m.chat_input.text(), "@");
}
