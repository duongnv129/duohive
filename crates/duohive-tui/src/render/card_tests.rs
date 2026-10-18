//! Approval cards in the chat and in Needs you (UI §4.6, §4.1.3; P3-D4, P3-D15, P3-D25).

use duohive_core::card::{Card, CardChange, CardId, CardOutcome};
use duohive_core::chat::ChatKind;
use duohive_core::cli_kind::CliKind;
use duohive_core::command::{AgentDraft, MoveTarget, OrgEdit, RoleDraft, TeamDraft};
use duohive_core::event::Event;
use duohive_core::names::NodeName;
use duohive_core::org::{Fields, NodeKey};
use duohive_core::status::AgentState;
use duohive_core::testkit::n;

use super::card::{CARD_WIDTH, card_lines, card_rows, card_title};
use crate::fixtures::{
    action_entry, add_agent_card, card, chat_entry, clock, ctx, mins, new_agent_card, qa_role,
    raise, render_lines, spec_model, status, stop_card,
};
use crate::focus::Pane;
use crate::format::width;
use crate::model::UiModel;
use crate::theme::Palette;

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
        "┃             ┃   + cli: Claude Code",
        "┃             ┃   + directory: ~/code/web",
        "┃             ┃   + worktree: on",
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
            "  + cli: Claude Code",
            "  + directory: ~/code/web",
            "  + worktree: on",
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

fn node(s: &str) -> NodeName {
    NodeName::parse(s).unwrap()
}

/// `edit` as a pending card in [`with_card`]'s chat.
fn org_card(edit: OrgEdit) -> Card {
    card(2, clock(10, 21), CardChange::Org(edit))
}

/// The card's body, buttons dropped.
fn body_of(c: &Card, m: &UiModel) -> Vec<String> {
    let mut lines = card_lines(c, m, m.now);
    lines.pop();
    lines
}

#[test]
fn edit_role_card_lists_changed_fields() {
    let mut m = with_card(stop_card(9, clock(10, 20)));
    m.org.teams[0].roles[1].defaults = Fields {
        model: Some("sonnet".to_owned()),
        directory: Some("~/code/api".to_owned()),
        worktree: Some(true),
        ..Fields::default()
    };
    let c = org_card(OrgEdit::EditRole {
        key: qa_role(),
        draft: RoleDraft {
            name: node("Testing"),
            defaults: Fields {
                model: Some("opus".to_owned()),
                directory: Some("~/code/api".to_owned()),
                effort: Some("high".to_owned()),
                ..Fields::default()
            },
        },
    });
    assert_eq!(
        body_of(&c, &m),
        [
            "~ Edit role Payments › QA",
            "  ~ name: QA → Testing",
            "  ~ model: sonnet → opus",
            "  ~ effort: inherit → high",
            "  ~ worktree: on → inherit",
        ]
    );
    let same = org_card(OrgEdit::EditRole {
        key: qa_role(),
        draft: RoleDraft {
            name: node("QA"),
            defaults: m.org.teams[0].roles[1].defaults.clone(),
        },
    });
    assert_eq!(
        body_of(&same, &m),
        ["~ Edit role Payments › QA", "  no changes"]
    );
}

#[test]
fn flags_change_is_shown_and_marked() {
    let flags = "--dangerously-skip-permissions";
    let c = org_card(OrgEdit::EditAgent {
        key: n("pay-qa"),
        draft: AgentDraft {
            name: n("pay-qa"),
            fields: Fields {
                flags: Some(flags.to_owned()),
                ..Fields::default()
            },
        },
    });
    let m = with_card(c.clone());
    assert_eq!(
        body_of(&c, &m),
        [
            "~ Edit agent pay-qa",
            &format!("  ~ flags: inherit → {flags}"),
        ]
    );
    let p = Palette::dark();
    let rows = card_rows(&c, &m, m.now, true, CARD_WIDTH, &p);
    let row = rows
        .iter()
        .find(|r| r.iter().any(|(t, _)| t.contains("~ flags:")))
        .expect("a flags row");
    let text = row.iter().find(|(t, _)| t.contains("~ flags:")).unwrap();
    assert_eq!(text.1, p.attention, "{row:?}");
    assert_eq!(row[0].1, p.border, "{row:?}");
    assert_eq!(row[row.len() - 1].1, p.border, "{row:?}");
    let other = rows
        .iter()
        .find(|r| r.iter().any(|(t, _)| t.contains("Edit agent")))
        .unwrap();
    assert!(
        other.iter().all(|(_, col)| *col != p.attention),
        "{other:?}"
    );

    // A flags value wider than the card is shown in full, hanging under itself.
    let long = format!("{flags} --allowed-tools Bash,Edit,Write --model opus --verbose");
    let c = org_card(OrgEdit::EditAgent {
        key: n("pay-qa"),
        draft: AgentDraft {
            name: n("pay-qa"),
            fields: Fields {
                flags: Some(long.clone()),
                ..Fields::default()
            },
        },
    });
    let rows = card_rows(&c, &m, m.now, true, CARD_WIDTH, &p);
    let flag_text: String = rows
        .iter()
        .filter(|r| r.iter().any(|(_, col)| *col == p.attention) && r.len() == 3)
        .map(|r| r[1].0.trim().to_owned())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(flag_text.contains(&long), "{flag_text}");
    for r in &rows {
        let w: usize = r.iter().map(|(t, _)| width(t)).sum();
        assert_eq!(w, CARD_WIDTH, "{r:?}");
    }
}

#[test]
fn prompt_change_shows_first_line_and_count() {
    let mut m = with_card(stop_card(9, clock(10, 20)));
    m.org.teams[0].defaults.prompt = Some("Old team prompt".to_owned());
    let edit = |prompt: &str| {
        org_card(OrgEdit::EditTeam {
            key: node("Payments"),
            draft: TeamDraft {
                name: node("Payments"),
                defaults: Fields {
                    prompt: Some(prompt.to_owned()),
                    ..Fields::default()
                },
            },
        })
    };
    let c = edit("You own payments.\nBe careful.\nShip small.");
    assert_eq!(
        body_of(&c, &m),
        [
            "~ Edit team Payments",
            "  ~ prompt: Old team prompt → You own payments. (3 lines)",
        ]
    );
    let one = edit("Just one line");
    assert_eq!(
        body_of(&one, &m)[1],
        "  ~ prompt: Old team prompt → Just one line"
    );
    // A long first line is cut so the line count still shows, inside the card.
    let long = edit(&format!("{}\nsecond", "Rewrite everything ".repeat(8)));
    let rows = card_rows(&long, &m, m.now, true, CARD_WIDTH, &Palette::dark());
    let row: String = rows
        .iter()
        .map(|r| r.iter().map(|(t, _)| t.as_str()).collect::<String>())
        .find(|r| r.contains("~ prompt:"))
        .unwrap();
    assert!(row.contains("… (2 lines)"), "{row}");
    assert_eq!(width(&row), CARD_WIDTH, "{row}");
}

#[test]
fn add_agent_card_lists_all_set_fields() {
    let m = with_card(stop_card(9, clock(10, 20)));
    let c = org_card(OrgEdit::AddAgent {
        role: qa_role(),
        agent: AgentDraft {
            name: n("pay-web"),
            fields: Fields {
                cli: Some(CliKind::Opencode),
                flags: Some("--verbose".to_owned()),
                directory: Some("~/code/web".to_owned()),
                model: Some("opus".to_owned()),
                effort: Some("high".to_owned()),
                worktree: Some(false),
                prompt: Some("Test the web app.\nAll of it.".to_owned()),
            },
        },
    });
    assert_eq!(
        body_of(&c, &m),
        [
            "+ Add agent pay-web in Payments › QA",
            "  + cli: OpenCode",
            "  + flags: --verbose",
            "  + directory: ~/code/web",
            "  + model: opus",
            "  + effort: high",
            "  + worktree: off",
            "  + prompt: Test the web app. (2 lines)",
        ]
    );
    let bare = org_card(OrgEdit::AddAgent {
        role: qa_role(),
        agent: AgentDraft {
            name: n("pay-web"),
            fields: Fields::default(),
        },
    });
    assert_eq!(
        body_of(&bare, &m),
        [
            "+ Add agent pay-web in Payments › QA",
            "  every field inherited"
        ]
    );
}

#[test]
fn org_defaults_card_lists_changes() {
    let m = with_card(stop_card(9, clock(10, 20)));
    let mut defaults = m.org.defaults.clone();
    defaults.model = Some("opus".to_owned());
    defaults.worktree = true;
    defaults.prompt = Some("Be terse.".to_owned());
    let mut duo = m.org.duo.clone();
    duo.name = n("Boss");
    duo.cli = CliKind::Opencode;
    let c = org_card(OrgEdit::EditOrgDefaults { defaults, duo });
    assert_eq!(
        body_of(&c, &m),
        [
            "~ Edit the org defaults",
            "  ~ model: unset → opus",
            "  ~ worktree: off → on",
            "  ~ prompt: unset → Be terse.",
            "  ~ Duo name: Duo → Boss",
            "  ~ Duo cli: Claude Code → OpenCode",
        ]
    );
}

#[test]
fn move_and_remove_cards_say_what() {
    let m = with_card(stop_card(9, clock(10, 20)));
    let mv = org_card(OrgEdit::Move {
        node: NodeKey::Agent(n("auth-service")),
        to: MoveTarget::Into(NodeKey::Role(qa_role())),
    });
    assert_eq!(
        body_of(&mv, &m),
        [
            "~ Move auth-service to QA",
            "  from Payments › Backend → Payments › QA",
        ]
    );
    let team = org_card(OrgEdit::Move {
        node: NodeKey::Team(node("Payments")),
        to: MoveTarget::End,
    });
    assert_eq!(body_of(&team, &m)[1], "  from the end → the end");
    let rm = org_card(OrgEdit::Delete(NodeKey::Team(node("Payments"))));
    assert_eq!(
        body_of(&rm, &m),
        ["- Remove team Payments", "  with 2 roles, 3 agents"]
    );
    let rm_role = org_card(OrgEdit::Delete(NodeKey::Role(qa_role())));
    assert_eq!(
        body_of(&rm_role, &m),
        ["- Remove role Payments › QA", "  with 1 agent"]
    );
}

#[test]
fn edit_card_snapshot() {
    let c = org_card(OrgEdit::EditRole {
        key: qa_role(),
        draft: RoleDraft {
            name: node("QA"),
            defaults: Fields {
                flags: Some("--dangerously-skip-permissions".to_owned()),
                prompt: Some("You test payments.\nRun the suite.\nFile bugs.".to_owned()),
                ..Fields::default()
            },
        },
    });
    let m = with_card(c);
    let text = snap("edit_card_160x48", &m, 160, 48);
    for want in [
        "┃ ~ Edit role Payments › QA",
        "┃   ~ flags: inherit → --dangerously-skip-permissions",
        "┃   ~ prompt: inherit → You test payments. (3 lines)",
    ] {
        assert!(text.contains(want), "{want}\n{text}");
    }
}
