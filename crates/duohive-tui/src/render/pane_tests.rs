//! The phase-2 main screen: three panes, borders, sizes, folding and footers (UI §3, §4.1).

use duohive_core::cli_kind::CliKind;
use duohive_core::event::{Event, Snapshot};
use duohive_core::names::NodeName;
use duohive_core::org::{NodeKey, Org, RoleKey};
use duohive_core::status::AgentState;
use duohive_core::testkit::n;

use crate::fixtures::{chat_entry, cols, ctx, mins, model_for, render_lines, spec_model, status};
use crate::focus::{Pane, layout};
use crate::forms::LineEdit;
use crate::model::UiModel;
use duohive_core::chat::ChatKind;

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
/// The spec model with a role selected, so the agent pane holds a summary.
fn spec_with_chat() -> UiModel {
    let mut m = spec_model();
    m.selected = NodeKey::Role(RoleKey {
        team: Some(nn("Payments")),
        role: nn("Backend"),
    });
    let at = duohive_core::time::Millis(10 * 3_600_000 + 5 * 60_000);
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
            cards: Vec::new(),
            chat: Vec::new(),
            your_queued: Vec::new(),
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
fn agent_pane_footer_fits_what_is_selected() {
    let footer = |selected| {
        let mut m = spec_model();
        m.focus.pane = Pane::Agent;
        m.selected = selected;
        render_lines(&m, &ctx(), 120, 36)
            .last()
            .unwrap()
            .trim_end()
            .to_owned()
    };
    assert_eq!(
        footer(NodeKey::Agent(n("pay-qa"))),
        " z zoom · space give work"
    );
    let role = NodeKey::Role(RoleKey {
        team: Some(nn("Payments")),
        role: nn("QA"),
    });
    assert_eq!(footer(role), " tab pane · esc back");
}

#[test]
fn role_summary_offers_give_work() {
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
    assert_eq!(
        body.last().map(String::as_str),
        Some("space give work to this role · e edit"),
        "{body:#?}"
    );
}

#[test]
fn empty_org_and_role_ask_duo() {
    let m = model_for(Org::blank(CliKind::Claude));
    let text = render_lines(&m, &ctx(), 160, 48).join("\n");
    assert!(
        text.contains(
            "│   Your org is empty · n to add a team, role or agent, or ask Duo to set one up"
        ),
        "{text}"
    );
    // A narrow pane wraps it under itself.
    let narrow = render_lines(&m, &ctx(), 80, 24);
    let side: Vec<String> = narrow
        .iter()
        .map(|l| l.chars().skip(32).collect())
        .collect();
    let at = side
        .iter()
        .position(|l| l.starts_with("│   Your org is empty"))
        .unwrap_or_else(|| panic!("{side:#?}"));
    assert!(side[at + 1].starts_with("│   "), "{side:#?}");
    assert!(
        side[at..=at + 2].join(" ").contains("set one up"),
        "{side:#?}"
    );
    let mut org = duohive_core::testkit::example_org();
    org.teams[0].roles.push(duohive_core::org::Role {
        name: nn("Frontend"),
        defaults: duohive_core::org::Fields::default(),
        agents: Vec::new(),
    });
    let lines = render_lines(&model_for(org), &ctx(), 100, 30);
    let tree: Vec<String> = lines.iter().map(|l| cols(l, 50)).collect();
    assert!(
        tree.contains(&"┃ │      no agents yet · n to add one or ask Duo ┃".to_owned()),
        "{tree:#?}"
    );
}

#[test]
fn running_agent_pane_shows_its_state_and_no_attach_hint() {
    let mut m = spec_model();
    m.agents.get_mut(&n("auth-service")).unwrap().status = status(AgentState::Working, mins(2));
    let text = render_lines(&m, &ctx(), 120, 36).join("\n");
    assert!(text.contains("┌─ auth-service · Claude ─"), "{text}");
    assert!(text.contains("│    ● working"), "{text}");
    assert!(!text.contains("attach"));
}

#[test]
fn chat_times_are_local() {
    let mut m = spec_model();
    let at = |h: u64, min: u64| duohive_core::time::Millis((h * 60 + min) * 60_000);
    let you = ChatKind::You { message: None };
    m.chat.entries = vec![
        chat_entry(1, at(3, 5), you.clone(), "east"),
        chat_entry(2, at(1, 0), you, "west"),
    ];
    let text = |offset: i32| {
        let mut c = ctx();
        c.utc_offset_secs = offset;
        render_lines(&m, &c, 120, 36).join("\n")
    };
    let utc7 = text(7 * 3_600);
    assert!(utc7.contains("10:05 you   east"), "{utc7}");
    assert!(utc7.contains("08:00 you   west"), "{utc7}");
    // A negative offset wraps to the previous day.
    let utc_5 = text(-5 * 3_600);
    assert!(utc_5.contains("22:05 you   east"), "{utc_5}");
    assert!(utc_5.contains("20:00 you   west"), "{utc_5}");
}

#[test]
fn header_count_equals_list() {
    let mut m = spec_model();
    for extra in 0..3_usize {
        let wide = render_lines(&m, &ctx(), 160, 48).join("\n");
        let compact = render_lines(&m, &ctx(), 80, 24).join("\n");
        let count = m.needs_you.len();
        assert!(wide.contains(&format!("◆ {count} need you")), "{wide}");
        assert!(compact.contains(&format!("◆ {count}")), "{compact}");
        assert!(wide.contains(&format!("Needs you ({count})")), "{wide}");
        let mut item = m.needs_you[0].clone();
        item.agent = n("auth-service");
        m.needs_you.push(item);
        assert_eq!(m.needs_you.len(), 2 + extra + 1);
    }
}

#[test]
fn needs_you_focused_snapshots() {
    let mut m = spec_model();
    m.focus.pane = Pane::NeedsYou;
    m.focus.needs_you_cursor = 1;
    for (name, w, h) in [
        ("needs_you_focused_80x24", 80, 24),
        ("needs_you_focused_160x48", 160, 48),
    ] {
        insta::assert_snapshot!(name, render_lines(&m, &ctx(), w, h).join("\n"));
    }
}

/// The spec model with two long-named agents in Needs you.
fn long_needs_you() -> UiModel {
    let mut m = spec_model();
    let first = m.needs_you[1].clone();
    m.needs_you.push(duohive_core::event::NeedsYouItem {
        agent: n("billing-invoice-worker"),
        ..first.clone()
    });
    m.needs_you.push(duohive_core::event::NeedsYouItem {
        agent: n("authentication-service-for-payments-team"),
        ..first
    });
    m
}

/// The Needs you row whose name has `part`, inside the org pane.
fn needs_you_row(lines: &[String], part: &str) -> String {
    lines
        .iter()
        .find(|l| l.contains("◆ ") && l.contains(part))
        .map(|l| l.chars().skip(1).take(48).collect())
        .unwrap_or_default()
}

#[test]
fn needs_you_names_are_never_cut_harder_than_compact() {
    use crate::format::truncate_middle;
    let m = long_needs_you();
    let compact = needs_you_row(&render_lines(&m, &ctx(), 80, 40), "-worker");
    assert!(compact.contains("billing-…-worker"), "{compact}");
    for (w, h) in [(100, 30), (160, 48)] {
        let lines = render_lines(&m, &ctx(), w, h);
        let row = needs_you_row(&lines, "-worker");
        assert!(row.contains("billing-in…ce-worker"), "{w}×{h}: {row}");
        let row = needs_you_row(&lines, "authentic");
        let cut = truncate_middle("authentication-service-for-payments-team", 20);
        assert!(row.contains(&cut), "{w}×{h}: {row}");
        assert!(row.contains("waiting"), "{w}×{h}: {row}");
        assert!(row.contains("  \"Allow"), "{w}×{h}: {row}");
    }
}

#[test]
fn needs_you_long_names_snapshots() {
    let m = long_needs_you();
    for (name, w, h) in [
        ("needs_you_long_names_100x30", 100, 30),
        ("needs_you_long_names_160x48", 160, 48),
    ] {
        insta::assert_snapshot!(name, render_lines(&m, &ctx(), w, h).join("\n"));
    }
}
