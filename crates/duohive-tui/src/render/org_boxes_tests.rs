//! The org boxes drawn (UI §4.1.5).

use duohive_core::settings::{OrgLayout, Settings};

use duohive_core::colour::TeamColour;
use duohive_core::names::NodeName;
use duohive_core::org::{NodeKey, RoleKey};
use duohive_core::status::{AgentState, Status};
use duohive_core::testkit::n;
use ratatui::buffer::{Buffer, Cell};
use ratatui::style::Modifier;

use crate::fixtures::{
    ctx, five_agent_org, lines_of, mins, render_buffer, render_lines, status, view,
};
use crate::focus::{AgentMode, Pane};
use crate::model::UiModel;
use crate::render::RenderCtx;

fn boxes_ctx() -> RenderCtx {
    RenderCtx {
        settings: Settings {
            org_layout: OrgLayout::Boxes,
            ..ctx().settings
        },
        ..ctx()
    }
}

fn boxes_model() -> UiModel {
    let mut m = five_agent_org();
    m.settings.org_layout = OrgLayout::Boxes;
    m.screen = ratatui::layout::Rect::new(0, 0, 120, 36);
    m
}

fn typing_model() -> UiModel {
    let mut m = boxes_model();
    m.focus.pane = Pane::Agent;
    m.focus.agent_mode = AgentMode::Typing;
    m
}

/// The org column (the first 50 columns) of the rendered screen.
fn org_column(m: &UiModel, w: u16, h: u16) -> Vec<String> {
    render_lines(m, &boxes_ctx(), w, h)
        .iter()
        .skip(1) // the header
        .map(|l| l.chars().take(50).collect())
        .collect()
}

/// The diagram of UI §4.1: the org column above the Needs you separator.
const DIAGRAM: &str = "\
┌─ Org ──────────────────────────────────────────┐
│ ┌─ ⬢ Manager ────────────────────────────────┐ │
│ │  █▀▄ █ █ ▄▀▄   Duo                  ◦ idle │ │
│ │  █▄▀ █▄█ ▀▄▀   2 need you · 1 card         │ │
│ └────────────────────────────────────────────┘ │
│ ┌─ ▣ Payments ───────────────────── ● 1 ◆ 1 ─┐ │
│ │ ╭────────────── ◇ Backend ───────────────╮ │ │
│ │ │▌auth-service ✉ 1                     ● │ │ │
│ │ │  working 2m · \"Running go test ./...\"  │ │ │
│ │ │ billing-api                          ◆ │ │ │
│ │ ╰────────────────────────────────────────╯ │ │
│ │ ╭───────────────── ◇ QA ─────────────────╮ │ │
│ │ │ pay-qa                               ○ │ │ │
│ │ ╰────────────────────────────────────────╯ │ │
│ └────────────────────────────────────────────┘ │
│ ┌─ ▣ Platform ───────────────────────── ● 1 ─┐ │
│ │ ╭─────────────── ◇ Infra ────────────────╮ │ │
│ │ │ infra-1                              ✓ │ │ │
│ │ │ infra-2                              ● │ │ │
│ │ ╰────────────────────────────────────────╯ │ │
│ └────────────────────────────────────────────┘ │
│                                                │
│                                                │
│                                                │";

fn buffer(m: &UiModel, w: u16, h: u16) -> Buffer {
    render_buffer(m, &boxes_ctx(), w, h)
}

/// The first cell of `needle` on the first org-column row holding `row_text`.
fn cell<'a>(buf: &'a Buffer, row_text: &str, needle: &str) -> &'a Cell {
    let lines = lines_of(buf);
    let (y, line) = lines
        .iter()
        .enumerate()
        .skip(1) // the header
        .find(|(_, l)| l.chars().take(50).collect::<String>().contains(row_text))
        .unwrap_or_else(|| panic!("no row {row_text:?}"));
    let line: String = line.chars().take(50).collect();
    let i = line
        .find(needle)
        .unwrap_or_else(|| panic!("no {needle:?} in {line:?}"));
    &buf[(
        u16::try_from(line[..i].chars().count()).unwrap(),
        u16::try_from(y).unwrap(),
    )]
}

fn dark() -> crate::theme::Palette {
    crate::theme::Palette::dark()
}

#[test]
fn boxes_screen_typing() {
    let col = org_column(&typing_model(), 120, 36);
    let above: Vec<&str> = col.iter().take(24).map(String::as_str).collect();
    // The agent pane has focus, so the org pane's border is the light one.
    assert_eq!(above.join("\n"), DIAGRAM);
    insta::assert_snapshot!("boxes_screen_typing", col.join("\n"));
}

#[test]
fn boxes_screen_team_selected() {
    let mut m = boxes_model();
    m.focus.pane = Pane::Org;
    m.selected = NodeKey::Team(NodeName::parse("Payments").unwrap());
    insta::assert_snapshot!(
        "boxes_screen_team_selected",
        org_column(&m, 120, 36).join("\n")
    );
}

#[test]
fn boxes_screen_role_selected() {
    let mut m = boxes_model();
    m.focus.pane = Pane::Org;
    m.selected = NodeKey::Role(RoleKey {
        team: Some(NodeName::parse("Payments").unwrap()),
        role: NodeName::parse("Backend").unwrap(),
    });
    insta::assert_snapshot!(
        "boxes_screen_role_selected",
        org_column(&m, 120, 36).join("\n")
    );
}

#[test]
fn boxes_screen_agent_selected() {
    let mut m = boxes_model();
    m.focus.pane = Pane::Org;
    insta::assert_snapshot!(
        "boxes_screen_agent_selected",
        org_column(&m, 120, 36).join("\n")
    );
}

#[test]
fn boxes_screen_manager_selected() {
    let mut m = boxes_model();
    m.focus.pane = Pane::Org;
    m.selected = NodeKey::Duo;
    insta::assert_snapshot!(
        "boxes_screen_manager_selected",
        org_column(&m, 120, 36).join("\n")
    );
}

#[test]
fn boxes_screen_manager_typed_into() {
    let mut m = typing_model();
    m.selected = NodeKey::Duo;
    insta::assert_snapshot!(
        "boxes_screen_manager_typed_into",
        org_column(&m, 120, 36).join("\n")
    );
}

#[test]
fn a_selected_team_has_a_solid_border_and_a_chip_in_its_hue() {
    let mut m = boxes_model();
    m.focus.pane = Pane::Org;
    m.selected = NodeKey::Team(NodeName::parse("Payments").unwrap());
    let buf = buffer(&m, 120, 36);
    let p = dark();
    let teal = p.hue(TeamColour::Teal);
    assert_eq!(cell(&buf, "▣ Payments", "┌").fg, teal);
    let title = cell(&buf, "▣ Payments", "Payments");
    assert_eq!(title.bg, teal);
    assert_eq!(title.fg, p.bg);
    // The role boxes inside keep the dimmer border.
    assert_eq!(
        cell(&buf, "◇ Backend", "╭").fg,
        p.tint(TeamColour::Teal, 65)
    );
    // An unselected team: 65% border, hue title with no chip.
    let other = cell(&buf, "▣ Platform", "┌");
    assert_eq!(other.fg, p.tint(TeamColour::Violet, 65));
    assert_ne!(
        cell(&buf, "▣ Platform", "Platform").bg,
        p.hue(TeamColour::Violet)
    );
}

#[test]
fn the_manager_card_border_is_40_percent_and_100_when_selected() {
    let mut m = boxes_model();
    m.focus.pane = Pane::Org;
    let p = dark();
    let buf = buffer(&m, 120, 36);
    assert_eq!(cell(&buf, "⬢ Manager", "┌").fg, p.tint_manager(40));
    m.selected = NodeKey::Duo;
    let buf = buffer(&m, 120, 36);
    assert_eq!(cell(&buf, "⬢ Manager", "┌").fg, p.manager);
    assert_eq!(cell(&buf, "⬢ Manager", "Manager").bg, p.manager, "the chip");
    // No `▌` bar on the card: the chip and the border say it.
    assert!(!lines_of(&buf).iter().take(5).any(|l| l.contains('▌')));
    // The brighter wash behind the card's text.
    assert_eq!(cell(&buf, "Duo", "Duo").bg, p.tint_manager(26));
}

#[test]
fn the_typing_row_is_solid_in_its_hue() {
    let m = typing_model();
    let buf = buffer(&m, 120, 36);
    let p = dark();
    let teal = p.hue(TeamColour::Teal);
    for needle in ["▌", "auth-service", "✉", "●"] {
        let c = cell(&buf, "auth-service", needle);
        assert_eq!(c.bg, teal, "{needle}");
        assert_eq!(c.fg, p.bg, "{needle}");
    }
    // Both lines: the second line is solid too.
    let second = cell(&buf, "working 2m", "working");
    assert_eq!((second.bg, second.fg), (teal, p.bg));
    assert!(
        cell(&buf, "auth-service", "auth-service")
            .modifier
            .contains(Modifier::BOLD)
    );
}

#[test]
fn a_selected_agent_has_a_bar_in_its_hue_a_30_percent_wash_and_an_fg_icon() {
    let mut m = boxes_model();
    m.focus.pane = Pane::Org;
    let buf = buffer(&m, 120, 36);
    let p = dark();
    let bar = cell(&buf, "auth-service", "▌");
    assert_eq!(bar.fg, p.hue(TeamColour::Teal));
    let wash = p.tint(TeamColour::Teal, 30);
    assert_eq!(cell(&buf, "auth-service", "auth-service").bg, wash);
    let name = cell(&buf, "auth-service", "auth-service");
    assert_eq!(name.fg, p.emphasis());
    assert!(name.modifier.contains(Modifier::BOLD));
    let icon = cell(&buf, "auth-service", "●");
    assert_eq!((icon.fg, icon.bg), (p.fg, wash));
    // The second line shares the wash.
    assert_eq!(cell(&buf, "working 2m", "working").bg, wash);
    // An unselected agent has no wash.
    assert_eq!(cell(&buf, "billing-api", "billing-api").bg, p.bg);
}

#[test]
fn a_problem_line_is_error_coloured_unless_selected() {
    let mut m = boxes_model();
    m.focus.pane = Pane::Org;
    let mut v = view(status(AgentState::Stopped, mins(24 * 60)));
    v.problem = Some("✕ ~/code/pay not found · e to fix".to_owned());
    m.agents.insert(n("pay-qa"), v);
    let p = dark();
    let buf = buffer(&m, 120, 36);
    assert_eq!(cell(&buf, "not found", "~/code").fg, p.error);
    m.selected = NodeKey::Agent(n("pay-qa"));
    let buf = buffer(&m, 120, 36);
    assert_eq!(cell(&buf, "not found", "~/code").fg, p.fg);
}

#[test]
fn duos_error_on_the_card_is_the_short_reason_and_selected_text_stays_fg() {
    let mut m = boxes_model();
    m.focus.pane = Pane::Org;
    let s = Status {
        error: Some("opencode not found (code 127)".to_owned()),
        ..status(AgentState::Errored, mins(1))
    };
    m.agents.insert(m.org.duo.name.clone(), view(s));
    let p = dark();
    let buf = buffer(&m, 120, 36);
    assert_eq!(cell(&buf, "opencode not found", "opencode").fg, p.error);
    m.selected = NodeKey::Duo;
    let buf = buffer(&m, 120, 36);
    let c = cell(&buf, "opencode not found", "opencode");
    assert_eq!(c.fg, p.emphasis());
    assert_ne!(c.fg, p.error);
}

#[test]
fn the_status_pill_takes_its_colours_from_the_table() {
    use crate::theme::Pill;
    let p = dark();
    for (state, pill, word) in [
        (AgentState::Idle, Pill::Idle, "idle"),
        (AgentState::Working, Pill::Working, "working"),
        (AgentState::Waiting, Pill::Waiting, "waiting"),
        (AgentState::Stopped, Pill::Stopped, "stopped"),
        (AgentState::Starting, Pill::Starting, "starting"),
    ] {
        let mut m = boxes_model();
        m.focus.pane = Pane::Org;
        m.agents
            .insert(m.org.duo.name.clone(), view(status(state, mins(1))));
        let buf = buffer(&m, 120, 36);
        let c = cell(&buf, "Duo", word);
        let (bg, text) = p.pill(pill, false);
        assert_eq!((c.bg, c.fg), (bg, text), "{state:?}");
    }
    // Typed into, the pill takes the lighter tint that stands apart from the solid fill.
    let mut m = typing_model();
    m.selected = NodeKey::Duo;
    let buf = buffer(&m, 120, 36);
    let c = cell(&buf, "Duo", "idle");
    assert_eq!((c.bg, c.fg), p.pill(Pill::Idle, true));
}

#[test]
fn no_colour_keeps_the_chip_and_the_fill_as_reverse_video() {
    for palette in [crate::theme::Palette::auto(), crate::theme::Palette::mono()] {
        let c = RenderCtx {
            palette,
            settings: boxes_ctx().settings,
            ..ctx()
        };
        let mut m = boxes_model();
        m.focus.pane = Pane::Org;
        m.selected = NodeKey::Team(NodeName::parse("Payments").unwrap());
        let buf = render_buffer(&m, &c, 120, 36);
        assert!(
            cell(&buf, "▣ Payments", "Payments")
                .modifier
                .contains(Modifier::REVERSED)
        );
        let mut m = typing_model();
        m.selected = NodeKey::Duo;
        let buf = render_buffer(&m, &c, 120, 36);
        assert!(
            cell(&buf, "Duo", "Duo")
                .modifier
                .contains(Modifier::REVERSED)
        );
    }
}

fn y_of(m: &UiModel, text: &str) -> u16 {
    let col = org_column(m, 120, 36);
    u16::try_from(
        col.iter()
            .position(|l| l.contains(text))
            .unwrap_or_else(|| panic!("no {text:?}"))
            + 1,
    )
    .unwrap()
}

#[test]
fn a_click_on_the_second_line_selects_the_agent() {
    let m = boxes_model();
    let y = y_of(&m, "working 2m");
    assert_eq!(
        crate::render::org_key_at(&m, y),
        Some(NodeKey::Agent(n("auth-service")))
    );
}

#[test]
fn a_click_on_a_role_title_selects_the_role() {
    let m = boxes_model();
    let y = y_of(&m, "◇ Backend");
    assert_eq!(
        crate::render::org_key_at(&m, y),
        Some(NodeKey::Role(RoleKey {
            team: Some(NodeName::parse("Payments").unwrap()),
            role: NodeName::parse("Backend").unwrap(),
        }))
    );
}

#[test]
fn a_click_on_the_manager_card_selects_duo() {
    let m = boxes_model();
    for text in ["⬢ Manager", "Duo", "2 need you"] {
        assert_eq!(
            crate::render::org_key_at(&m, y_of(&m, text)),
            Some(NodeKey::Duo),
            "{text}"
        );
    }
}

#[test]
fn a_click_on_a_box_border_selects_its_box() {
    let m = boxes_model();
    let payments = NodeKey::Team(NodeName::parse("Payments").unwrap());
    assert_eq!(
        crate::render::org_key_at(&m, y_of(&m, "▣ Payments")),
        Some(payments.clone())
    );
    // The team's bottom line is the row above Platform's top line.
    let bottom = y_of(&m, "▣ Platform") - 1;
    assert_eq!(crate::render::org_key_at(&m, bottom), Some(payments));
    // A role's bottom line selects the role, not the agent above it.
    let qa = y_of(&m, "◇ QA");
    let role_bottom = qa - 1;
    assert!(matches!(
        crate::render::org_key_at(&m, role_bottom),
        Some(NodeKey::Role(_))
    ));
}

#[test]
fn the_tree_setting_at_120x36_still_draws_the_tree() {
    let mut m = boxes_model();
    m.settings.org_layout = OrgLayout::Tree;
    let c = RenderCtx {
        settings: Settings {
            org_layout: OrgLayout::Tree,
            ..ctx().settings
        },
        ..ctx()
    };
    let text = render_lines(&m, &c, 120, 36).join("\n");
    assert!(text.contains("├─ ▣ Payments"), "{text}");
    assert!(!text.contains("⬢ Manager"));
}

#[test]
fn at_99_columns_draws_the_compact_tree_and_at_100_the_boxes() {
    let mut m = boxes_model();
    m.screen = ratatui::layout::Rect::new(0, 0, 99, 40);
    let narrow = render_lines(&m, &boxes_ctx(), 99, 40).join("\n");
    assert!(
        !narrow.contains("⬢ Manager") && narrow.contains("▾ Payments"),
        "{narrow}"
    );
    m.screen = ratatui::layout::Rect::new(0, 0, 100, 30);
    let wide = render_lines(&m, &boxes_ctx(), 100, 30).join("\n");
    assert!(wide.contains("⬢ Manager"), "{wide}");
    // 29 rows is compact whatever the width.
    let short = render_lines(&m, &boxes_ctx(), 140, 29).join("\n");
    assert!(!short.contains("⬢ Manager"), "{short}");
}

#[test]
fn at_110x24_the_screen_is_compact_so_it_draws_the_tree() {
    let m = boxes_model();
    let text = render_lines(&m, &boxes_ctx(), 110, 24).join("\n");
    assert!(!text.contains("⬢ Manager"));
}

#[test]
fn compact_draws_the_tree_not_boxes_and_duo_stays_one_row() {
    let text = render_lines(&boxes_model(), &boxes_ctx(), 80, 24).join("\n");
    assert!(
        text.contains("⬢ Duo") && !text.contains("⬢ Manager"),
        "{text}"
    );
}

#[test]
fn the_second_line_carries_the_reply_at_100x30() {
    let mut m = boxes_model();
    m.screen = ratatui::layout::Rect::new(0, 0, 100, 30);
    let text = render_lines(&m, &boxes_ctx(), 100, 30).join("\n");
    assert!(
        text.contains("working 2m · \"Running go test ./...\""),
        "{text}"
    );
    m.settings.show_last_reply = false;
    let c = RenderCtx {
        settings: Settings {
            show_last_reply: false,
            ..boxes_ctx().settings
        },
        ..boxes_ctx()
    };
    let text = render_lines(&m, &c, 100, 30).join("\n");
    assert!(
        text.contains("working 2m") && !text.contains("Running go test"),
        "{text}"
    );
}

#[test]
fn a_resize_while_typing_keeps_typing() {
    let mut m = typing_model();
    m.screen = ratatui::layout::Rect::new(0, 0, 99, 30);
    let _ = render_lines(&m, &boxes_ctx(), 99, 30);
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
    assert_eq!(m.focus.pane, Pane::Agent);
}

fn big_squad() -> UiModel {
    use duohive_core::names::AgentName;
    use duohive_core::org::{AgentDef, Fields, Role};
    let mut m = boxes_model();
    let names: Vec<AgentName> = (0..100).map(|i| n(&format!("squad-{i}"))).collect();
    m.org.teams.clear();
    m.org.roles = vec![Role {
        name: NodeName::parse("Squad").unwrap(),
        defaults: Fields::default(),
        agents: names
            .iter()
            .map(|a| AgentDef {
                name: a.clone(),
                fields: Fields::default(),
            })
            .collect(),
        color: None,
    }];
    for a in &names {
        m.agents
            .insert(a.clone(), view(status(AgentState::Idle, mins(1))));
    }
    m.needs_you.clear();
    m.focus.pane = Pane::Org;
    m
}

#[test]
fn scroll_keeps_the_selected_agent_and_its_second_line_in_view() {
    let mut m = big_squad();
    for i in 0..100 {
        m.selected = NodeKey::Agent(n(&format!("squad-{i}")));
        m.org_scroll = None;
        let text = org_column(&m, 120, 36).join("\n");
        assert!(
            text.contains(&format!("squad-{i}")),
            "agent {i} not in view:\n{text}"
        );
        assert!(
            text.contains("idle 1m"),
            "second line of {i} cut off:\n{text}"
        );
    }
}

#[test]
fn the_wheel_scrolls_box_lines() {
    let mut m = big_squad();
    m.selected = NodeKey::Agent(n("squad-0"));
    crate::render::org_wheel(&mut m, false, 3);
    assert_eq!(m.org_scroll.as_ref().map(|(top, _)| *top), Some(3));
    let text = org_column(&m, 120, 36).join("\n");
    // The first lines (the card) scrolled away by three: the top rule is gone.
    assert!(!text.contains("⬢ Manager"), "{text}");
    crate::render::org_wheel(&mut m, true, 10);
    assert_eq!(m.org_scroll.as_ref().map(|(top, _)| *top), Some(0));
}

#[test]
fn a_drag_marks_a_valid_target_with_a_marker() {
    use crate::input::Drag;
    let mut m = boxes_model();
    m.focus.pane = Pane::Org;
    m.drag = Some(Drag {
        node: NodeKey::Agent(n("pay-qa")),
        over: None,
    });
    let buf = buffer(&m, 120, 36);
    let marked = lines_of(&buf).iter().filter(|l| l.contains('▹')).count();
    assert!(marked >= 1, "no marker");
    let c = cell(&buf, "◇ Backend", "▹");
    assert_eq!(c.fg, dark().fg);
}

fn wrap_model(in_team: bool, reason: &str) -> UiModel {
    let mut m = boxes_model();
    m.focus.pane = Pane::Org;
    let s = Status {
        error: Some(reason.to_owned()),
        ..status(AgentState::Errored, mins(1))
    };
    let name = if in_team { "pay-qa" } else { "squad-1" };
    if !in_team {
        use duohive_core::org::{AgentDef, Fields, Role};
        m.org.roles = vec![Role {
            name: NodeName::parse("Generalist").unwrap(),
            defaults: Fields::default(),
            agents: vec![AgentDef {
                name: n("squad-1"),
                fields: Fields::default(),
            }],
            color: None,
        }];
    }
    m.agents.insert(n(name), view(s));
    m.selected = NodeKey::Agent(n(name));
    m
}

#[test]
fn a_long_problem_wraps_at_the_box_edge_keeping_the_hint() {
    for in_team in [true, false] {
        let two = org_column(
            &wrap_model(in_team, "opencode not found · install it or change its CLI"),
            120,
            36,
        );
        let at = two
            .iter()
            .position(|l| l.contains("errored · opencode"))
            .unwrap();
        assert!(two[at + 1].contains("(e)"), "{in_team}: {two:#?}");
        let three = org_column(
            &wrap_model(
                in_team,
                "worktree failed: the directory already exists and is not empty · remove it",
            ),
            120,
            36,
        );
        let at = three
            .iter()
            .position(|l| l.contains("errored · worktree"))
            .unwrap();
        assert!(three[at + 2].contains("(e)"), "{in_team}: {three:#?}");
        // A fourth line would be the next agent or the box's bottom, never more problem text.
        assert!(!three[at + 3].contains("remove"), "{in_team}");
        // A longer reason is cut with `…` on the third line.
        let long = org_column(&wrap_model(in_team, &"word ".repeat(40)), 120, 36);
        let at = long
            .iter()
            .position(|l| l.contains("errored · word"))
            .unwrap();
        assert!(long[at + 2].contains('…'), "{in_team}: {long:#?}");
    }
}

#[test]
fn a_40_character_or_wide_name_is_cut_in_the_middle_beside_the_roll_up_and_fold_marker() {
    let mut m = boxes_model();
    m.focus.pane = Pane::Org;
    let long = NodeName::parse(&"w".repeat(40)).unwrap();
    m.org.teams[0].name = long.clone();
    m.folded.insert(NodeKey::Team(long));
    let col = org_column(&m, 120, 36);
    let top = col
        .iter()
        .find(|l| l.contains("▸"))
        .expect("the folded title");
    assert!(top.contains('…') && top.contains("● 1 ◆ 1"), "{top}");
    assert_eq!(top.chars().count(), 50);
}

#[test]
fn enter_folds_and_unfolds_a_box_and_a_click_does_not() {
    use crate::input::{map_key, update};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let press = |m: &mut UiModel| {
        if let Some(a) = map_key(m, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)) {
            update(m, a);
        }
    };
    let mut m = boxes_model();
    m.focus.pane = Pane::Org;
    m.selected = NodeKey::Team(NodeName::parse("Payments").unwrap());
    press(&mut m);
    assert!(
        org_column(&m, 120, 36)
            .iter()
            .any(|l| l.contains("▣ Payments ▸"))
    );
    press(&mut m);
    assert!(!org_column(&m, 120, 36).iter().any(|l| l.contains("▸")));
}

#[test]
fn a_click_on_a_problem_agents_second_line_selects_it() {
    let m = wrap_model(true, "opencode not found");
    let col = org_column(&m, 120, 36);
    let y = u16::try_from(
        col.iter()
            .position(|l| l.contains("errored · opencode"))
            .unwrap()
            + 1,
    )
    .unwrap();
    assert_eq!(
        crate::render::org_key_at(&m, y),
        Some(NodeKey::Agent(n("pay-qa")))
    );
}

#[test]
fn duos_error_on_the_card_is_the_short_reason_cut_at_27_columns() {
    let mut m = boxes_model();
    let s = Status {
        error: Some("the launcher could not start the manager process at all".to_owned()),
        ..status(AgentState::Errored, mins(1))
    };
    m.agents.insert(m.org.duo.name.clone(), view(s));
    let col = org_column(&m, 120, 36);
    assert!(col[3].contains("the launcher could not sta…"), "{}", col[3]);
    assert_eq!(col[3].chars().count(), 50);
}

#[test]
fn a_colour_set_in_the_org_recolours_the_boxes() {
    let mut m = boxes_model();
    m.focus.pane = Pane::Org;
    m.org.teams[0].color = Some(TeamColour::Orchid);
    let buf = buffer(&m, 120, 36);
    assert_eq!(
        cell(&buf, "▣ Payments", "Payments").fg,
        dark().hue(TeamColour::Orchid)
    );
}

/// The same column with Needs you under it (UI §4.1): icon, name and reason, no word or time.
const WHOLE_COLUMN_TAIL: &str = "
├─ Needs you (2) ────────────────────────────────┤
│ ◆  billing-api  \"Allow edit to api.go?\"        │
│ ◆  Duo  approval  Add pay-web                  │
└────────────────────────────────────────────────┘";

#[test]
fn boxes_screen_typing_whole_column() {
    let col = org_column(&typing_model(), 120, 36);
    let whole: Vec<&str> = col.iter().take(28).map(String::as_str).collect();
    assert_eq!(whole.join("\n"), format!("{DIAGRAM}{WHOLE_COLUMN_TAIL}"));
}

#[test]
fn wide_needs_you_rows_are_icon_name_reason() {
    let mut m = typing_model();
    m.needs_you.push(duohive_core::event::NeedsYouItem {
        agent: n("squad-3"),
        state: AgentState::Working,
        quiet: true,
        since: crate::fixtures::ago(mins(6)),
        excerpt: None,
        card: None,
    });
    m.org.teams[0].roles[1].agents[0].name = n("squad-3");
    let text = org_column(&m, 120, 36).join("\n");
    assert!(
        text.contains("◆  billing-api  \"Allow edit to api.go?\""),
        "{text}"
    );
    assert!(text.contains("◆  Duo  approval  Add pay-web"), "{text}");
    assert!(text.contains("◷  squad-3  quiet"), "{text}");
    // No state word and no time on a row.
    assert!(!text.contains("waiting  "), "{text}");
}

#[test]
fn the_tree_keeps_its_word_and_time_needs_you_rows() {
    let mut m = typing_model();
    m.settings.org_layout = OrgLayout::Tree;
    let c = RenderCtx {
        settings: Settings {
            org_layout: OrgLayout::Tree,
            ..ctx().settings
        },
        ..ctx()
    };
    let text = render_lines(&m, &c, 120, 36).join("\n");
    assert!(text.contains("billing-api  waiting    4m"), "{text}");
}
