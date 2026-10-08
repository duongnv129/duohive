//! The phase-2 main screen: three panes, borders, sizes, folding and footers (UI §3, §4.1).

use duohive_core::cli_kind::CliKind;
use duohive_core::event::{Event, Snapshot};
use duohive_core::names::NodeName;
use duohive_core::org::{NodeKey, Org, RoleKey};
use duohive_core::status::AgentState;
use duohive_core::testkit::n;

use crate::chat::{ChatLine, Who};
use crate::fixtures::{NOW, cols, ctx, mins, model_for, render_lines, spec_model, status};
use crate::focus::{Pane, layout};
use crate::forms::LineEdit;
use crate::model::UiModel;

fn nn(s: &str) -> NodeName {
    NodeName::parse(s).unwrap()
}

fn char_at(lines: &[String], x: u16, y: u16) -> char {
    lines[usize::from(y)]
        .chars()
        .nth(usize::from(x))
        .unwrap_or(' ')
}

/// The spec model with a short chat history, as UI §4.1 draws it.
fn spec_with_chat() -> UiModel {
    let mut m = spec_model();
    let at = duohive_core::time::Millis(10 * 3_600_000 + 5 * 60_000);
    m.chat.lines = vec![
        ChatLine {
            at,
            who: Who::You,
            text: "what's blocking the Payments team?".to_owned(),
        },
        ChatLine {
            at,
            who: Who::Duo,
            text: "billing-api is waiting on a file-edit approval; auth-service is mid-test."
                .to_owned(),
        },
    ];
    m
}

#[test]
fn main_screen_snapshots() {
    let m = spec_with_chat();
    for (name, w, h) in [
        ("main_80x24", 80, 24),
        ("main_100x30", 100, 30),
        ("main_120x36", 120, 36),
        ("main_160x48", 160, 48),
    ] {
        insta::assert_snapshot!(name, render_lines(&m, &ctx(), w, h).join("\n"));
    }
}

#[test]
fn focused_pane_has_heavy_border() {
    let (w, h) = (120, 36);
    for pane in [Pane::Org, Pane::NeedsYou, Pane::Agent, Pane::Chat] {
        let mut m = spec_model();
        m.focus.pane = pane;
        let lines = render_lines(&m, &ctx(), w, h);
        let l = layout(
            ratatui::layout::Rect::new(0, 0, w, h),
            0,
            pane == Pane::Chat,
        )
        .unwrap()
        .with_needs_you(m.needs_you.len());
        let corner = |x, y| char_at(&lines, x, y);
        let heavy = |p: Pane| if p == pane { '┏' } else { '┌' };
        assert_eq!(corner(l.org.x, l.org.y), heavy(Pane::Org), "{pane:?}");
        assert_eq!(corner(l.agent.x, l.agent.y), heavy(Pane::Agent), "{pane:?}");
        assert_eq!(corner(l.chat.x, l.chat.y), heavy(Pane::Chat), "{pane:?}");
        // Needs you has no corner of its own: its separator turns heavy below.
        let sep = corner(l.needs_you.x, l.needs_you.y);
        let want = match pane {
            Pane::Org => '┣',
            Pane::NeedsYou => '┢',
            Pane::Agent | Pane::Chat => '├',
        };
        assert_eq!(sep, want, "{pane:?}");
        let bottom = corner(l.needs_you.x, l.needs_you.bottom() - 1);
        let heavy_bottom = matches!(pane, Pane::Org | Pane::NeedsYou);
        assert_eq!(bottom, if heavy_bottom { '┗' } else { '└' }, "{pane:?}");
    }
}

#[test]
fn below_80x24_shows_too_small_with_needs_you_count() {
    for (w, h) in [(79, 24), (100, 23), (60, 40)] {
        let lines = render_lines(&spec_model(), &ctx(), w, h);
        let text = lines.join("\n");
        assert!(text.contains("Make the window at least 80×24"), "{w}×{h}");
        assert!(text.contains("◆ 2 need you"), "{w}×{h}");
        assert!(!text.contains("Org"), "{w}×{h}");
        assert!(!text.contains("Duo ·"), "{w}×{h}");
    }
}

#[test]
fn shrink_order_drops_replies_then_connectors() {
    let reply = "\"Running go test ./...\"";
    let at = |w, h| render_lines(&spec_model(), &ctx(), w, h).join("\n");
    // ≥ 120×36: connectors and last replies.
    let large = at(120, 36);
    assert!(large.contains("├─ auth-service"), "{large}");
    assert!(large.contains(reply), "{large}");
    // 100–119 × 30: the replies go first.
    let medium = at(100, 30);
    assert!(medium.contains("├─ auth-service"), "{medium}");
    assert!(!medium.contains(reply), "{medium}");
    // 80–99 × 24: then the connectors.
    let compact = at(80, 24);
    assert!(!compact.contains("├─"), "{compact}");
    assert!(compact.contains("    auth-service"), "{compact}");
    // The agent pane loses detail before any of that: a role summary drops its
    // defaults and prompt when its pane is short, and keeps its counts and keys.
    let mut m = spec_model();
    m.selected = NodeKey::Role(RoleKey {
        team: Some(nn("Payments")),
        role: nn("Backend"),
    });
    m.chat_rows_extra = 12;
    let short = render_lines(&m, &ctx(), 80, 24).join("\n");
    assert!(short.contains("2 agents · ● 1 working"), "{short}");
    assert!(short.contains("e edit"), "{short}");
    assert!(!short.contains("Defaults"), "{short}");
}

#[test]
fn fold_hides_children_and_shows_count() {
    let mut m = spec_model();
    m.folded.insert(NodeKey::Role(RoleKey {
        team: Some(nn("Payments")),
        role: nn("QA"),
    }));
    let lines = render_lines(&m, &ctx(), 80, 24);
    let tree: Vec<String> = lines[1..=12].iter().map(|l| cols(l, 32)).collect();
    assert!(
        tree.contains(&"┃   ▸ QA (1)           ○       ┃".to_owned()),
        "{tree:#?}"
    );
    assert!(!lines.join("\n").contains("pay-qa"));
}

#[test]
fn footer_matches_focused_pane() {
    let footer = |pane, w, h| {
        let mut m = spec_model();
        m.focus.pane = pane;
        render_lines(&m, &ctx(), w, h)
            .last()
            .unwrap()
            .trim_end()
            .to_owned()
    };
    let want = [
        (
            Pane::Org,
            " tab pane · ↑↓ move · space give work · s start/stop · n new · e edit · z zoom",
        ),
        (Pane::NeedsYou, " ↑↓ move · enter go to agent"),
        (
            Pane::Agent,
            " enter type · z zoom · PgUp/PgDn scroll · space give work",
        ),
        (Pane::Chat, " enter send · alt+enter new line · tab pane"),
    ];
    for (pane, text) in want {
        assert_eq!(footer(pane, 120, 36), text, "{pane:?}");
    }
    assert_eq!(
        footer(Pane::Org, 80, 24),
        " tab pane · ↑↓ move · space give work"
    );
    assert_eq!(
        footer(Pane::Chat, 80, 24),
        " enter send · alt+enter new line · tab pane"
    );
    for (w, h) in [(80, 24), (160, 48)] {
        let mut m = spec_model();
        let text = render_lines(&m, &ctx(), w, h).join("\n");
        assert!(!text.contains("? help"));
        m.focus.pane = Pane::Chat;
        assert!(!render_lines(&m, &ctx(), w, h).join("\n").contains("? help"));
    }
}

#[test]
fn chat_pane_shows_history_and_input() {
    let mut m = spec_with_chat();
    m.focus.pane = Pane::Chat;
    m.chat_input = LineEdit::new("hello");
    let lines = render_lines(&m, &ctx(), 120, 36);
    // Focused at ≥ 120×36: 12 rows, heavy, titled with Duo's state.
    assert!(
        lines[23].starts_with("┏━ ⬢ Duo · ◦ idle ━"),
        "{}",
        lines[23]
    );
    assert!(
        lines[24..34]
            .iter()
            .any(|l| l.starts_with("┃ 10:05 you   what's blocking"))
    );
    assert!(
        lines[24..34]
            .iter()
            .any(|l| l.starts_with("┃ 10:05 Duo   billing-api is waiting"))
    );
    assert!(lines[33].starts_with("┃ ❯ hello"), "{}", lines[33]);
    assert!(lines[34].starts_with("┗━"));
}

#[test]
fn chat_lines_wrap_with_a_hanging_indent() {
    let mut m = spec_model();
    m.chat.lines = vec![ChatLine {
        at: NOW,
        who: Who::Duo,
        text: "word ".repeat(30),
    }];
    let lines = render_lines(&m, &ctx(), 80, 24);
    // 80×24: chat is 4 rows, two inside: the end of the reply, then the input.
    let chat: Vec<&String> = lines[19..23].iter().collect();
    assert!(chat[1].starts_with("│            word"), "{chat:#?}");
    assert!(chat[2].starts_with("│ ❯ "), "{chat:#?}");
}

#[test]
fn reconnecting_state_in_agent_pane() {
    let spec = spec_model();
    let snapshot = |reconnecting| {
        Event::Snapshot(Box::new(Snapshot {
            org: spec.org.clone(),
            agents: spec.agents.clone(),
            needs_you: spec.needs_you.clone(),
            org_error: None,
            settings_error: None,
            reconnecting,
            worktrees: Vec::new(),
        }))
    };
    let mut m = UiModel::new(spec.org.clone());
    assert!(m.apply(snapshot(6)));
    let agent = |m: &UiModel| -> String {
        render_lines(m, &ctx(), 100, 30)
            .iter()
            .map(|l| l.chars().skip(50).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert!(
        agent(&m).contains("│   Reconnecting to 6 agents…"),
        "{}",
        agent(&m)
    );
    assert!(!agent(&m).contains("attach"));
    assert!(m.apply(Event::Reconnected));
    assert!(!agent(&m).contains("Reconnecting"));
    m.apply(snapshot(1));
    assert!(agent(&m).contains("Reconnecting to 1 agent…"));
}

#[test]
fn role_summary_has_phase_2_footer() {
    let mut m = spec_model();
    m.selected = NodeKey::Role(RoleKey {
        team: Some(nn("Payments")),
        role: nn("Backend"),
    });
    let lines = render_lines(&m, &ctx(), 160, 48);
    let pane: Vec<String> = lines
        .iter()
        .map(|l| l.chars().skip(50).collect::<String>())
        .collect();
    assert!(
        pane[1].starts_with("┌─ ◇ Backend · in Payments ─"),
        "{}",
        pane[1]
    );
    let body: Vec<String> = pane[2..]
        .iter()
        .map(|l| l.trim_matches(['│', ' ']).to_owned())
        .take_while(|l| !l.starts_with('└'))
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(body.last().map(String::as_str), Some("e edit"), "{body:#?}");
    assert!(!pane.join("\n").contains("space give work to this role"));
}

#[test]
fn empty_org_keeps_d7_text() {
    let m = model_for(Org::blank(CliKind::Claude));
    let text = render_lines(&m, &ctx(), 160, 48).join("\n");
    assert!(
        text.contains("│   Your org is empty · n to add a team, role or agent "),
        "{text}"
    );
    assert!(!text.contains("ask Duo"));
}

#[test]
fn running_agent_pane_shows_its_state_and_no_attach_hint() {
    let mut m = spec_model();
    m.agents.get_mut(&n("auth-service")).unwrap().status = status(AgentState::Working, mins(2));
    let text = render_lines(&m, &ctx(), 120, 36).join("\n");
    assert!(text.contains("┌─ auth-service · Claude Code ─"), "{text}");
    assert!(text.contains("│    ● working"), "{text}");
    assert!(!text.contains("attach"));
}
