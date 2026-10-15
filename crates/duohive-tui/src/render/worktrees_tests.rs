//! The worktree list's frame (UI §4.9).

use duohive_core::worktree::WorktreeState as S;

use super::worktrees::state_word;
use crate::fixtures::{ctx, render_lines, spec_model, spec_worktrees, worktree_row};
use crate::model::{Dialog, UiModel, WorktreeList};

fn listed() -> UiModel {
    let mut m = spec_model();
    m.worktrees = spec_worktrees();
    m.dialog = Some(Dialog::Worktrees(WorktreeList { cursor: 0 }));
    m
}

#[test]
fn states_render_as_words_not_glyphs() {
    let cases = [
        (S::Unchecked, "not checked yet"),
        (S::InUse, "in use"),
        (S::Merged, "merged · removing"),
        (S::Removing, "merged · removing"),
        (
            S::NotMerged { files_changed: 2 },
            "not merged · 2 files changed",
        ),
        (
            S::NotMerged { files_changed: 1 },
            "not merged · 1 file changed",
        ),
        (S::MergedDirty, "merged · uncommitted changes, kept"),
        (
            S::CheckFailed("no remote".to_owned()),
            "couldn't check · no remote",
        ),
        (
            S::CheckFailed("couldn't remove: busy".to_owned()),
            "couldn't remove: busy",
        ),
    ];
    for (state, word) in cases {
        let got = state_word(&state);
        assert_eq!(got, word);
        assert!(
            got.chars()
                .all(|c| c.is_alphanumeric() || " ·,'.:".contains(c))
        );
    }
}

#[test]
fn list_matches_the_spec() {
    let lines = render_lines(&listed(), &ctx(), 120, 30);
    let text = lines.join("\n");
    assert!(text.contains("Worktrees · 4 · removed automatically once merged and clean"));
    assert!(text.contains("duohive/squad-2/flaky-date"));
    assert!(text.contains("not merged · 2 files changed"));
    assert!(text.contains("checked 3m ago against origin/main"));
    assert!(
        text.contains("enter open in agent · d remove (asks unless merged and clean) · esc back")
    );
}

#[test]
fn empty_list_says_so() {
    let mut m = listed();
    m.worktrees.clear();
    let text = render_lines(&m, &ctx(), 100, 30).join("\n");
    assert!(text.contains("Worktrees · 0"));
    assert!(text.contains("No worktrees"));
}

#[test]
fn unchecked_rows_have_no_check_line_age() {
    let mut m = listed();
    m.worktrees = vec![duohive_core::worktree::WorktreeRow {
        checked_at: None,
        base: None,
        ..worktree_row("squad-1", "readme", S::Unchecked)
    }];
    let text = render_lines(&m, &ctx(), 100, 30).join("\n");
    assert!(text.contains("not checked yet"));
}

#[test]
fn worktrees_snapshots() {
    let m = listed();
    for (name, w, h) in [("worktrees_80x24", 80, 24), ("worktrees_160x48", 160, 48)] {
        insta::assert_snapshot!(name, render_lines(&m, &ctx(), w, h).join("\n"));
    }
}

#[test]
fn check_age_follows_the_render_clock() {
    let m = listed();
    let mut later = ctx();
    later.now =
        crate::fixtures::ago(std::time::Duration::ZERO) + std::time::Duration::from_secs(7 * 60);
    let at = |c: &crate::render::RenderCtx| render_lines(&m, c, 120, 30).join("\n");
    assert!(at(&ctx()).contains("checked 3m ago"));
    assert!(at(&later).contains("checked 10m ago"));
}

/// The spec rows and one whose check failed with a long reason.
fn with_long_reason() -> UiModel {
    let mut m = listed();
    let reason = "git fetch origin failed: could not resolve host github.com, check your network";
    m.worktrees.push(worktree_row(
        "billing-api",
        "webhook-retries",
        S::CheckFailed(reason.to_owned()),
    ));
    m
}

#[test]
fn a_long_check_failure_does_not_squeeze_branch() {
    let lines = render_lines(&with_long_reason(), &ctx(), 160, 48);
    // The list is 100 cells wide, centred: columns 30 to 129.
    let row: String = lines
        .iter()
        .find(|l| l.contains("couldn't check"))
        .unwrap()
        .chars()
        .skip(30)
        .take(100)
        .collect();
    assert!(row.contains("duohive/billing-api/webhook-retries"), "{row}");
    assert!(row.contains("couldn't check · git fetch origin"), "{row}");
    // Cut with `…`, and a padding cell before the right border.
    assert!(row.ends_with(" │"), "{row}");
    assert!(row.trim_end_matches('│').trim_end().ends_with('…'), "{row}");
}

#[test]
fn long_check_failure_snapshots() {
    let m = with_long_reason();
    for (name, w, h) in [
        ("worktrees_long_reason_80x24", 80, 24),
        ("worktrees_long_reason_160x48", 160, 48),
    ] {
        insta::assert_snapshot!(name, render_lines(&m, &ctx(), w, h).join("\n"));
    }
}
