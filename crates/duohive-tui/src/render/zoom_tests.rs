//! The zoomed agent: header, footer and the full-screen pane (UI §4.2).

use duohive_core::event::NeedsYouItem;
use duohive_core::org::NodeKey;
use duohive_core::testkit::n;

use super::LiveScreen;
use crate::fixtures::{ctx, render_lines, render_live_lines, spec_model, term_for};
use crate::focus::Pane;
use crate::model::UiModel;

const SCREEN: &[u8] = "✓ go test ./... (14 passed)\r\n\
✓ Ready for review · 3 files changed\r\n\
\r\n\
❯"
.as_bytes();

fn zoomed() -> UiModel {
    let mut m = spec_model();
    m.zoomed = true;
    m.focus.pane = Pane::Agent;
    m
}

fn lines_at(m: &UiModel, w: u16, h: u16) -> Vec<String> {
    let mut sized = m.clone();
    sized.screen = ratatui::layout::Rect::new(0, 0, w, h);
    let term = term_for(&sized, w, h, SCREEN, (2, 3));
    let screen = LiveScreen {
        agent: &n("auth-service"),
        term: &term,
        first_row: 0,
        no_color: false,
    };
    render_live_lines(&sized, &ctx(), Some(screen), w, h)
}

#[test]
fn zoomed_snapshots() {
    let m = zoomed();
    for (name, w, h) in [("zoomed_80x24", 80, 24), ("zoomed_160x48", 160, 48)] {
        insta::assert_snapshot!(name, lines_at(&m, w, h).join("\n"));
    }
}

#[test]
fn zoom_header_names_the_agent_and_restores_at_the_right() {
    let lines = lines_at(&zoomed(), 100, 30);
    let header = &lines[0];
    assert!(
        header.starts_with(" DuoHive › auth-service (zoomed)"),
        "{header}"
    );
    assert!(header.trim_end().ends_with("z restore"), "{header}");
    assert!(header.ends_with("z restore "), "{header}");
}

#[test]
fn zoom_footer_lists_needs_you() {
    let lines = lines_at(&zoomed(), 100, 30);
    assert_eq!(
        lines.last().unwrap().trim_end(),
        " enter type · z restore · PgUp/PgDn scroll · ◆ 2 need you (squad-3, billing-api)"
    );
}

#[test]
fn zoom_footer_is_cut_with_an_ellipsis() {
    let mut m = zoomed();
    let extra = m.needs_you[0].clone();
    m.needs_you.push(NeedsYouItem {
        agent: n("pay-qa"),
        ..extra
    });
    let lines = lines_at(&m, 80, 24);
    let footer = lines.last().unwrap();
    assert!(footer.trim_end().ends_with('…'), "{footer}");
    assert!(
        footer.starts_with(" enter type · z restore · PgUp/PgDn scroll"),
        "{footer}"
    );
}

#[test]
fn zoom_footer_without_needs_you_has_just_the_keys() {
    let mut m = zoomed();
    m.needs_you.clear();
    let lines = lines_at(&m, 100, 30);
    assert_eq!(
        lines.last().unwrap().trim_end(),
        " enter type · z restore · PgUp/PgDn scroll"
    );
}

#[test]
fn zoom_hides_the_other_panes() {
    let lines = lines_at(&zoomed(), 100, 30).join("\n");
    assert!(!lines.contains("Needs you"));
    assert!(!lines.contains("Chat"));
    assert!(lines.contains("┏━ auth-service · Claude"));
}

#[test]
fn zoomed_stopped_agent_shows_its_stopped_line() {
    let mut m = zoomed();
    m.selected = NodeKey::Agent(n("pay-qa"));
    let lines = render_lines(&m, &ctx(), 100, 30).join("\n");
    assert!(lines.contains("○ stopped · space to give work"), "{lines}");
    assert!(lines.starts_with(" DuoHive › pay-qa (zoomed)"));
}

#[test]
fn zoomed_stopped_agent_footer_has_no_type_or_scroll() {
    let mut m = zoomed();
    m.selected = NodeKey::Agent(n("pay-qa"));
    m.needs_you.clear();
    let lines = render_lines(&m, &ctx(), 100, 30);
    assert_eq!(
        lines.last().unwrap().trim_end(),
        " z restore · space give work"
    );
}

#[test]
fn zoom_footer_need_you_segment_is_attention_coloured() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    let m = zoomed();
    let p = ctx().palette;
    let mut buf = Buffer::empty(Rect::new(0, 0, 100, 1));
    super::zoom::draw_footer(&mut buf, Rect::new(0, 0, 100, 1), &m, &p);
    let text: String = (0..100).map(|x| buf[(x, 0)].symbol().to_owned()).collect();
    let at = text.chars().position(|c| c == '◆').unwrap();
    let at = u16::try_from(at).unwrap();
    assert_eq!(buf[(at, 0)].fg, p.attention);
    assert_eq!(buf[(at + 4, 0)].fg, p.attention);
    assert_eq!(buf[(2, 0)].fg, p.fg_muted);
    // Cut: the ellipsis keeps the attention colour.
    let mut narrow = Buffer::empty(Rect::new(0, 0, 60, 1));
    super::zoom::draw_footer(&mut narrow, Rect::new(0, 0, 60, 1), &m, &p);
    assert_eq!(narrow[(59, 0)].symbol(), "…");
    assert_eq!(narrow[(59, 0)].fg, p.attention);
}

#[test]
fn zoom_footer_note_goes_before_the_needs_you_count() {
    let mut m = zoomed();
    m.footer_note = Some("pay-qa's session ended".to_owned());
    let lines = lines_at(&m, 100, 30);
    assert_eq!(
        lines.last().unwrap().trim_end(),
        " pay-qa's session ended · ◆ 2 need you (squad-3, billing-api)"
    );
}

#[test]
fn zoom_footer_cuts_a_long_note_before_the_needs_you_count() {
    let mut m = zoomed();
    m.footer_note = Some(
        "billing-api didn't take the task: the worktree could not be created in time".to_owned(),
    );
    let lines = lines_at(&m, 80, 24);
    let footer = lines.last().unwrap().trim_end().to_owned();
    assert!(footer.starts_with(" billing-api didn't take"), "{footer}");
    assert!(
        footer.ends_with("… · ◆ 2 need you (squad-3, billing-api)"),
        "{footer}"
    );
}

#[test]
fn zoom_footer_note_while_typing_keeps_typing_footer() {
    let mut m = zoomed();
    m.focus.agent_mode = crate::focus::AgentMode::Typing;
    m.footer_note = Some("pay-qa's session ended".to_owned());
    let lines = lines_at(&m, 100, 30);
    assert_eq!(
        lines.last().unwrap().trim_end(),
        " TYPING · pay-qa's session ended · ctrl+q stop typing"
    );
}
