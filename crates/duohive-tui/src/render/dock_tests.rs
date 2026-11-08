//! The approval card docked over the agent pane (UI §4.6; CD-D1 to CD-D7).

use std::time::Duration;

use duohive_core::card::{Card, CardChange};
use duohive_core::cli_kind::CliKind;
use duohive_core::command::{AgentDraft, OrgEdit};
use duohive_core::org::{Fields, NodeKey};
use duohive_core::testkit::n;
use ratatui::layout::Rect;

use super::card::{CARD_WIDTH, Look, card_rows};
use super::{LiveScreen, pane_areas};
use crate::fixtures::{
    add_agent_card, ago, card, ctx, mins, new_agent_card, qa_role, raise, render_buffer,
    render_live_lines, spec_model, stop_card, term_for,
};
use crate::focus::Pane;
use crate::model::UiModel;

/// squad-1 selected, with a stop card pending for it, on a `w`×`h` screen.
fn docked(w: u16, h: u16) -> UiModel {
    let mut m = spec_model();
    m.screen = Rect::new(0, 0, w, h);
    m.selected = NodeKey::Agent(n("squad-1"));
    raise(&mut m, stop_card(1, ago(mins(1))), Duration::from_secs(60));
    m
}

fn lines(m: &UiModel) -> Vec<String> {
    let area = m.screen;
    crate::fixtures::lines_of(&render_buffer(m, &ctx(), area.width, area.height))
}

fn pane_text(lines: &[String], pane: Rect) -> Vec<String> {
    lines[usize::from(pane.y)..usize::from(pane.bottom())]
        .iter()
        .map(|l| {
            l.chars()
                .skip(usize::from(pane.x))
                .take(usize::from(pane.width))
                .collect()
        })
        .collect()
}

/// An add-agent card that sets every field: the tallest card an add can be.
fn tall_card(id: u64) -> Card {
    let fields = Fields {
        cli: Some(CliKind::Claude),
        flags: Some("--add-dir ../shared".to_owned()),
        directory: Some("~/code/web".to_owned()),
        model: Some("sonnet".to_owned()),
        effort: Some("high".to_owned()),
        worktree: Some(true),
        prompt: Some("Own the web client.\nKeep it small.".to_owned()),
    };
    let edit = OrgEdit::AddAgent {
        role: qa_role(),
        agent: AgentDraft {
            name: n("pay-web"),
            fields,
        },
    };
    card(id, ago(mins(1)), CardChange::Org(edit))
}

#[test]
fn dock_covers_top_rows_and_leaves_the_prompt_row() {
    let m = docked(120, 36);
    let area = pane_areas(&m).unwrap();
    let rows = usize::from(area.live.height);
    let screen: String = (0..rows).map(|i| format!("row{i:02}\r\n")).collect();
    let term = term_for(&m, 120, 36, screen.trim_end().as_bytes(), (0, 0));
    let live = LiveScreen {
        agent: &n("squad-1"),
        term: &term,
        first_row: 0,
        no_color: false,
    };
    let out = render_live_lines(&m, &ctx(), Some(live), 120, 36);
    let pane = pane_text(&out, area.agent);
    assert!(pane[1].contains("◆ Stop squad-1"), "{}", pane[1]);
    let last = &pane[pane.len() - 2];
    assert!(last.contains(&format!("row{:02}", rows - 1)), "{last}");
}

#[test]
fn title_shows_one_of_n_only_when_queued() {
    let mut m = docked(120, 36);
    let pane = pane_text(&lines(&m), pane_areas(&m).unwrap().agent);
    assert!(!pane[1].contains(" of "), "{}", pane[1]);
    raise(&mut m, stop_card(2, ago(mins(2))), Duration::from_secs(120));
    let pane = pane_text(&lines(&m), pane_areas(&m).unwrap().agent);
    // The older card (id 2) shows first.
    assert!(pane[1].contains("◆ Stop squad-1 · 1 of 2"), "{}", pane[1]);
}

#[test]
fn eight_field_card_at_80x24_shows_title_buttons_and_more_rows_line() {
    let mut m = spec_model();
    m.screen = Rect::new(0, 0, 80, 24);
    m.selected = NodeKey::Duo;
    raise(&mut m, tall_card(1), Duration::from_secs(60));
    let area = pane_areas(&m).unwrap().agent;
    assert!(area.height - 2 >= 7, "pane rows {}", area.height - 2);
    let pane = pane_text(&lines(&m), area);
    let text = pane.join("\n");
    assert!(text.contains("[y] Approve"), "{text}");
    assert!(text.contains("[n] Reject"), "{text}");
    let whole = card_rows(
        &tall_card(1),
        &m,
        m.now,
        Look::whole(false, CARD_WIDTH),
        &crate::theme::Palette::dark(),
    );
    let top = pane
        .iter()
        .position(|l| l.contains("◆ Proposed change"))
        .unwrap();
    let bottom = pane[top..].iter().position(|l| l.contains("└─")).unwrap();
    let drawn = bottom + 1;
    let cut = text
        .lines()
        .find_map(|l| {
            l.split("… ")
                .nth(1)?
                .split(' ')
                .next()?
                .parse::<usize>()
                .ok()
        })
        .unwrap_or(0);
    assert!(cut > 0, "{text}");
    // One `… N more rows` row stands for the N it cut.
    assert_eq!(whole.len(), drawn + cut - 1, "{text}");
}

#[test]
fn flags_row_is_never_cut() {
    let mut m = spec_model();
    m.screen = Rect::new(0, 0, 80, 24);
    m.selected = NodeKey::Duo;
    raise(&mut m, tall_card(1), Duration::from_secs(60));
    let text = pane_text(&lines(&m), pane_areas(&m).unwrap().agent).join("\n");
    assert!(text.contains("flags"), "{text}");
    assert!(text.contains("--add-dir ../shared"), "{text}");
}

#[test]
fn unfocused_is_light_and_focused_is_heavy() {
    let mut m = docked(120, 36);
    let pane = pane_text(&lines(&m), pane_areas(&m).unwrap().agent);
    assert!(
        pane[1].contains('┌') || pane[1].contains("┌─"),
        "{}",
        pane[1]
    );
    m.focus.pane = Pane::Agent;
    m.focus.card = Some(duohive_core::card::CardId(1));
    let pane = pane_text(&lines(&m), pane_areas(&m).unwrap().agent);
    assert!(pane[1].contains('┏'), "{}", pane[1]);
}

#[test]
fn nothing_drawn_for_a_team_selection_or_while_reconnecting() {
    let mut m = docked(120, 36);
    let in_pane = |m: &UiModel| pane_text(&lines(m), pane_areas(m).unwrap().agent).join("\n");
    m.selected = NodeKey::Team(duohive_core::names::NodeName::parse("Payments").unwrap());
    assert!(!in_pane(&m).contains("◆ Stop squad-1"));
    m.selected = NodeKey::Agent(n("squad-1"));
    m.reconnecting = 1;
    assert!(!in_pane(&m).contains("◆ Stop squad-1"));
}

#[test]
fn zoomed_pane_draws_the_card_too() {
    let mut m = docked(120, 36);
    m.zoomed = true;
    let out = lines(&m).join("\n");
    assert!(out.contains("◆ Stop squad-1"), "{out}");
}

#[test]
fn new_agent_card_docks_on_duo_pane() {
    let mut m = spec_model();
    m.screen = Rect::new(0, 0, 120, 36);
    m.selected = NodeKey::Duo;
    raise(
        &mut m,
        new_agent_card(1, ago(mins(1))),
        Duration::from_secs(60),
    );
    raise(
        &mut m,
        add_agent_card(2, ago(mins(2))),
        Duration::from_secs(120),
    );
    let area = pane_areas(&m).unwrap().agent;
    let pane = pane_text(&lines(&m), area);
    assert!(pane[0].contains("Duo"), "{}", pane[0]);
    assert!(
        pane[1].contains("◆ Proposed change · 1 of 2"),
        "{}",
        pane[1]
    );
}
