//! The resume picker's frame (UI §4.13).

use std::time::Duration;

use ratatui::style::Color;

use crate::fixtures::{
    ctx, mins, render_buffer, render_lines, resume_model, session, three_sessions,
};
use crate::model::UiModel;
use crate::render::RenderCtx;
use crate::theme::Palette;

const SIZES: [(u16, u16); 2] = [(80, 24), (160, 48)];

fn with(palette: Palette) -> RenderCtx {
    RenderCtx { palette, ..ctx() }
}

fn text(m: &UiModel, w: u16, h: u16) -> String {
    render_lines(m, &ctx(), w, h).join("\n")
}

#[test]
fn entries_show_title_age_branch_size_newest_first() {
    let (m, _) = resume_model(three_sessions());
    let t = text(&m, 120, 36);
    assert!(t.contains("Resume pay-qa (1 of 3)"), "{t}");
    assert!(t.contains("⌕ Search…"));
    assert!(t.contains("3 minutes ago · fix/payout-retry · 4.2MB · may be open elsewhere"));
    assert!(t.contains("1 day ago · main · 7.2MB"));
    assert!(t.contains("2 days ago · fix/webhook-flake · 610KB"));
    let pos = |s: &str| t.find(s).unwrap();
    assert!(pos("Retry loop") < pos("Payout ledger") && pos("Payout ledger") < pos("Flaky"));
    assert!(t.contains("▌ Retry loop for failed payouts"));
    assert!(t.contains("enter resume · type to search · ↑↓ move · esc cancel"));
}

#[test]
fn worktree_entries_name_the_task() {
    let mut s = three_sessions();
    s[2].worktree = Some(duohive_core::worktree::TaskId("flaky-date".to_owned()));
    let (m, _) = resume_model(s);
    assert!(text(&m, 120, 36).contains("610KB · worktree flaky-date"));
}

#[test]
fn title_less_entry_shows_untitled() {
    let (m, _) = resume_model(vec![session("", mins(60), None, 0)]);
    assert!(text(&m, 120, 36).contains("(untitled)"));
}

#[test]
fn opencode_entry_omits_branch_and_size() {
    let (m, _) = resume_model(vec![session("Bare", mins(120), None, 0)]);
    let t = text(&m, 120, 36);
    assert!(t.contains("2 hours ago"), "{t}");
    let line = t.lines().find(|l| l.contains("2 hours ago")).unwrap();
    assert!(!line.contains(" · "), "{line}");
}

#[test]
fn in_use_entries_say_who() {
    let mut s = three_sessions();
    s[1].in_use_by = Some(duohive_core::testkit::n("squad-2"));
    let (m, _) = resume_model(s);
    assert!(text(&m, 120, 36).contains("1 day ago · main · 7.2MB · in use by squad-2"));
}

#[test]
fn search_filters_and_the_count_follows() {
    let (mut m, _) = resume_model(three_sessions());
    for c in "webhook".chars() {
        if let Some(crate::model::Dialog::Resume(p)) = &mut m.dialog {
            p.type_char(c);
        }
    }
    let t = text(&m, 120, 36);
    assert!(t.contains("Resume pay-qa (1 of 1)"), "{t}");
    assert!(t.contains("⌕ webhook"));
    assert!(!t.contains("Payout ledger"));
}

#[test]
fn empty_shows_no_cli_sessions_for_dir() {
    let (m, _) = resume_model(Vec::new());
    let t = text(&m, 120, 36);
    let line = t
        .lines()
        .find(|l| l.contains("No Claude Code sessions for"))
        .unwrap();
    assert!(line.contains("No Claude Code sessions for "), "{t}");
    assert!(t.contains("esc close"));
}

#[test]
fn unreadable_shows_couldnt_read_with_status_error_colour() {
    let (mut m, agent) = resume_model(Vec::new());
    m.dialog = None;
    m.apply(duohive_core::event::Event::SessionsUnreadable {
        agent,
        reason: "opencode session list failed".to_owned(),
    });
    let c = with(Palette::dark());
    let buf = render_buffer(&m, &c, 120, 36);
    let lines = crate::fixtures::lines_of(&buf);
    let y = lines
        .iter()
        .position(|l| l.contains("Couldn't read"))
        .unwrap();
    let x = lines[y].find("Couldn't").unwrap();
    assert!(
        lines[y].contains("Couldn't read Claude Code's sessions: opencode session list failed")
    );
    let col = lines[y][..x].chars().count();
    let cell = &buf[(u16::try_from(col).unwrap(), u16::try_from(y).unwrap())];
    assert_eq!(cell.fg, Palette::dark().error);
}

#[test]
fn hostile_title_cannot_move_the_cursor() {
    let evil = "\u{1b}[2J\u{1b}[1;1Hhacked\u{1b}]0;x\u{7}\nsecond\r\u{8}\u{8}end";
    let mut s = three_sessions();
    s[1].title = evil.to_owned();
    s[1].branch = Some("b\u{1b}[31mr".to_owned());
    let (m, _) = resume_model(s);
    let t = text(&m, 120, 36);
    assert!(!t.contains('\u{1b}') && !t.contains('\u{8}') && !t.contains('\r'));
    assert!(t.contains("hacked second end"), "{t}");
    for (w, h) in SIZES {
        let lines = render_lines(&m, &ctx(), w, h);
        assert_eq!(lines.len(), usize::from(h));
        assert!(lines.iter().all(|l| l.chars().count() == usize::from(w)));
    }
}

#[test]
fn a_long_list_scrolls_with_more_at_the_foot() {
    let many: Vec<_> = (0..30)
        .map(|i| {
            session(
                &format!("Session {i:02}"),
                Duration::from_secs(600 + i * 600),
                Some("main"),
                1000,
            )
        })
        .collect();
    let (mut m, _) = resume_model(many);
    let t = text(&m, 80, 24);
    assert!(t.contains("↓ more"), "{t}");
    if let Some(crate::model::Dialog::Resume(p)) = &mut m.dialog {
        p.cursor = 29;
    }
    let t = text(&m, 80, 24);
    assert!(t.contains("Session 29") && !t.contains("↓ more"), "{t}");
}

#[test]
fn snapshots() {
    let mut s = three_sessions();
    s[1].in_use_by = Some(duohive_core::testkit::n("squad-2"));
    s[2].worktree = Some(duohive_core::worktree::TaskId("flaky-date".to_owned()));
    s.push(session("OpenCode style entry", mins(5 * 24 * 60), None, 0));
    let (m, _) = resume_model(s);
    for (w, h) in SIZES {
        insta::assert_snapshot!(format!("resume_{w}x{h}"), text(&m, w, h));
    }
    let (empty, _) = resume_model(Vec::new());
    insta::assert_snapshot!("resume_empty_80x24", text(&empty, 80, 24));
}

#[test]
fn themes_change_style_not_words() {
    let (m, _) = resume_model(three_sessions());
    for (w, h) in SIZES {
        let base = text(&m, w, h);
        for p in [Palette::light(), Palette::mono()] {
            let lines = render_lines(&m, &with(p), w, h).join("\n");
            assert_eq!(lines, base);
        }
        let buf = render_buffer(&m, &with(Palette::mono()), w, h);
        for cell in buf.content() {
            for c in [cell.fg, cell.bg] {
                assert!(!matches!(c, Color::Rgb(..) | Color::Indexed(_)));
            }
        }
        // The selected row is marked in colour too under dark and light.
        for p in [Palette::dark(), Palette::light()] {
            let buf = render_buffer(&m, &with(p), w, h);
            assert!(buf.content().iter().any(|c| c.bg == p.selection));
        }
    }
}

/// The details line under the entry titled `title`, from `first` to the dialog's edge.
fn details_line(t: &str, title: &str, first: &str) -> String {
    let mut lines = t.lines().skip_while(|l| !l.contains(title));
    lines.next();
    let line = lines.next().unwrap_or_default();
    let from = line.find(first).unwrap_or(0);
    line[from..]
        .trim_end_matches(|c: char| c == '│' || c == '┃' || c.is_whitespace())
        .to_owned()
}

fn long_entry(holder: &str) -> Vec<duohive_core::session_catalog::SessionInfo> {
    let mut s = session(
        "Long one",
        mins(3),
        Some("feature/a-really-long-branch-name-that-is-cut"),
        4_200_000,
    );
    s.worktree = Some(duohive_core::worktree::TaskId("w".repeat(32)));
    s.in_use_by = Some(duohive_core::testkit::n(holder));
    vec![s]
}

#[test]
fn long_lead_is_cut_before_the_marks() {
    let holder = "squad-holder1";
    let (m, _) = resume_model(long_entry(holder));
    let t = text(&m, 80, 24);
    let line = details_line(&t, "Long one", "3 minutes ago");
    assert!(
        line.ends_with(&format!("in use by {holder} · may be open elsewhere")),
        "{t}"
    );
    assert!(line.contains('…'), "{line}");
    insta::assert_snapshot!("resume_long_line_80x24", t);
}

#[test]
fn marks_alone_wider_than_the_dialog_are_cut_last() {
    let holder = "h".repeat(40);
    let (m, _) = resume_model(long_entry(&holder));
    let t = text(&m, 80, 24);
    let line = details_line(&t, "Long one", "in use by");
    assert!(line.starts_with(&format!("in use by {holder}")), "{line}");
    assert!(line.ends_with('…'), "{line}");
    assert!(!line.contains("minutes ago"), "{line}");
}

#[test]
fn short_lines_are_unchanged() {
    let (m, _) = resume_model(three_sessions());
    let t = text(&m, 80, 24);
    let line = details_line(&t, "Retry loop", "3 minutes ago");
    assert_eq!(
        line,
        "3 minutes ago · fix/payout-retry · 4.2MB · may be open elsewhere"
    );
}
