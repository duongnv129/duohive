//! Approval cards in the chat and in Needs you (UI §4.6, §4.1.3; P3-D4, P3-D15, P3-D25).

use duohive_core::card::{CardId, CardOutcome};
use duohive_core::chat::ChatKind;
use duohive_core::event::Event;
use duohive_core::status::AgentState;
use duohive_core::testkit::n;

use super::card::{card_lines, card_title};
use crate::fixtures::{
    action_entry, add_agent_card, chat_entry, clock, ctx, mins, new_agent_card, raise,
    render_lines, spec_model, status, stop_card,
};
use crate::focus::Pane;
use crate::model::UiModel;

/// UI §4.6's chat: a stop already answered, a split story and Duo's two action lines,
/// then `card` raised at 10:21; the chat has focus.
fn with_card(c: duohive_core::card::Card) -> UiModel {
    let mut m = spec_model();
    m.agents.get_mut(&n("squad-1")).unwrap().status = status(AgentState::Working, mins(12));
    let result = ChatKind::CardResult {
        card: CardId(1),
        outcome: CardOutcome::Approved,
    };
    m.chat.entries = vec![
        chat_entry(
            1,
            clock(10, 15),
            ChatKind::Duo,
            "squad-1 has been quiet 12m. Proposed: stop it.",
        ),
        chat_entry(2, clock(10, 16), result, "approved: stop squad-1"),
        chat_entry(
            3,
            clock(10, 20),
            ChatKind::You { message: None },
            "split story PAY-88 by role and assign it",
        ),
        chat_entry(
            4,
            clock(10, 21),
            ChatKind::Duo,
            "PAY-88 splits into 3 subtasks. No Frontend agent in Payments yet.",
        ),
        action_entry(
            5,
            clock(10, 21),
            "give work auth-service \"PAY-88a backend: retry endpoint\"",
            "typed in",
        ),
        action_entry(
            6,
            clock(10, 21),
            "give work pay-qa \"PAY-88c QA: retry cases\"",
            "started",
        ),
    ];
    let id = c.id;
    raise(&mut m, c, mins(1));
    m.focus.pane = Pane::Chat;
    m.focus.card = Some(id);
    m
}

/// The chat pane and the footer.
fn chat_and_footer(lines: &[String]) -> Vec<String> {
    let top = lines
        .iter()
        .position(|l| l.starts_with("┌─ ⬢") || l.starts_with("┏━ ⬢"))
        .unwrap();
    lines[top..].to_vec()
}

fn snap(name: &str, m: &UiModel, w: u16, h: u16) -> String {
    let lines = render_lines(m, &ctx(), w, h);
    let text = chat_and_footer(&lines).join("\n");
    insta::assert_snapshot!(name, text);
    text
}

#[test]
fn card_focused_snapshots() {
    let m = with_card(add_agent_card(2, clock(10, 21)));
    snap("card_focused_80x24", &m, 80, 24);
    let wide = snap("card_focused_160x48", &m, 160, 48);
    for want in [
        "┃             ┏━ ◆ Proposed change ━",
        "┃             ┃ + Add agent pay-web in Payments › QA",
        "┃             ┃   CLI Claude Code · ~/code/web · worktrees on",
        "┃             ┃ [y] Approve   [e] Edit   [n] Reject",
        " y approve · e edit · n reject · esc input",
    ] {
        assert!(wide.contains(want), "{want}\n{wide}");
    }
}

#[test]
fn card_unfocused_snapshot() {
    let mut m = with_card(add_agent_card(2, clock(10, 21)));
    m.focus.card = None;
    let text = snap("card_unfocused_160x48", &m, 160, 48);
    assert!(
        text.contains("┃             ┌─ ◆ Proposed change ─"),
        "{text}"
    );
    assert!(!text.contains("┏━ ◆"), "{text}");
}

#[test]
fn stop_card_snapshot() {
    let m = with_card(stop_card(2, clock(10, 21)));
    let text = snap("stop_card_160x48", &m, 160, 48);
    for want in [
        "┏━ ◆ Stop squad-1 ━",
        "┃ squad-1 is ● working 12m",
        "┃ [y] Approve   [n] Reject",
        " y approve · n reject · esc input",
    ] {
        assert!(text.contains(want), "{want}\n{text}");
    }
    assert!(!text.contains("[e] Edit"), "{text}");
}

#[test]
fn new_agent_card_snapshot() {
    let m = with_card(new_agent_card(2, clock(10, 21)));
    let text = snap("new_agent_card_160x48", &m, 160, 48);
    for want in [
        "┏━ ◆ No free agent in Generalist ━",
        "┃ + Add agent squad-4 in Generalist (clone of squad-1)",
        "┃   Task  PAY-88d docs: retry guide",
        "┃ [y] Approve   [e] Edit   [n] Reject",
    ] {
        assert!(text.contains(want), "{want}\n{text}");
    }
}

#[test]
fn card_titles_and_lines() {
    let m = with_card(stop_card(2, clock(10, 21)));
    let c = add_agent_card(3, clock(10, 22));
    assert_eq!(card_title(&c, &m.org), "◆ Proposed change");
    assert_eq!(
        card_lines(&c, &m, m.now),
        [
            "+ Add agent pay-web in Payments › QA",
            "  CLI Claude Code · ~/code/web · worktrees on",
            "[y] Approve   [e] Edit   [n] Reject",
        ]
    );
    let s = stop_card(4, clock(10, 22));
    assert_eq!(card_title(&s, &m.org), "◆ Stop squad-1");
    assert_eq!(
        card_lines(&s, &m, m.now),
        ["squad-1 is ● working 12m", "[y] Approve   [n] Reject"]
    );
    let a = new_agent_card(5, clock(10, 22));
    assert_eq!(card_title(&a, &m.org), "◆ No free agent in Generalist");
}

#[test]
fn answered_card_collapses_to_line() {
    let mut m = with_card(stop_card(7, clock(10, 21)));
    m.apply(Event::CardAnswered {
        card: CardId(7),
        outcome: CardOutcome::Approved,
    });
    m.apply(Event::ChatLine(chat_entry(
        9,
        clock(10, 22),
        ChatKind::CardResult {
            card: CardId(7),
            outcome: CardOutcome::Approved,
        },
        "approved: stop squad-1",
    )));
    assert!(m.cards.is_empty());
    assert_eq!(m.focus.card, None);
    let text = render_lines(&m, &ctx(), 160, 48).join("\n");
    assert!(text.contains("→ approved: stop squad-1   10:22"), "{text}");
    assert!(!text.contains("◆ Stop squad-1"), "{text}");
}

#[test]
fn needs_you_card_row() {
    let mut m = spec_model();
    raise(&mut m, add_agent_card(2, clock(10, 21)), mins(1));
    let lines = render_lines(&m, &ctx(), 160, 48);
    let row: String = lines
        .iter()
        .find(|l| l.contains("◆ Duo"))
        .expect("a card row")
        .chars()
        .take(50)
        .collect();
    let words: Vec<&str> = row
        .trim_matches(|c| c == '│' || c == '┃' || c == ' ')
        .split("  ")
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect();
    assert_eq!(words, ["◆ Duo", "approval", "1m", "Add pay-web"], "{row}");
    let org: Vec<String> = lines.iter().map(|l| l.chars().take(50).collect()).collect();
    insta::assert_snapshot!("needs_you_with_card_160x48", org.join("\n"));
}

#[test]
fn snapshot_and_raised_events_fill_cards() {
    let mut m = spec_model();
    let c = stop_card(3, clock(10, 21));
    m.apply(Event::CardRaised(Box::new(c.clone())));
    assert_eq!(m.cards.get(&CardId(3)), Some(&c));
}
