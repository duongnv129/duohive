use std::collections::HashMap;
use std::path::Path;

use duohive_core::testkit::FakeDirs;

use super::*;

const HOME: &str = "/h";

fn dirs() -> FakeDirs {
    FakeDirs::new(HashMap::new())
        .with_listing(
            "/h/code",
            &[
                ("billing-api", true),
                ("billing-docs", false),
                ("web", false),
            ],
        )
        .with_listing("/h/code/billing-api", &[("src", false)])
        .with_listing("/", &[("etc", false), ("srv", false)])
}

fn refreshed(typed: &str) -> DirSuggest {
    let mut s = DirSuggest::default();
    s.refresh(typed, &dirs(), Path::new(HOME));
    s
}

#[test]
fn split_typed_needs_tilde_slash_or_slash() {
    for none in ["", "~", "~bob/x", "rel/x", "code", " /x"] {
        assert_eq!(split_typed(none), None, "{none:?}");
    }
    assert_eq!(split_typed("~/co"), Some(("~/", "co")));
    assert_eq!(split_typed("/"), Some(("/", "")));
    assert_eq!(split_typed("~/code/bi"), Some(("~/code/", "bi")));
    assert_eq!(split_typed("/srv/"), Some(("/srv/", "")));
}

#[test]
fn refresh_opens_with_matches_and_highlights_first() {
    let s = refreshed("~/code/bil");
    assert!(s.open);
    assert_eq!(s.cursor, 0);
    let labels: Vec<&str> = s.rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, ["~/code/billing-api", "~/code/billing-docs"]);
}

#[test]
fn rows_carry_the_git_mark() {
    let s = refreshed("~/code/bil");
    let git: Vec<bool> = s.rows.iter().map(|r| r.is_git).collect();
    assert_eq!(git, [true, false]);
}

#[test]
fn refresh_with_unchanged_text_does_not_reopen() {
    let mut s = DirSuggest::default();
    s.reset("~/code/bil");
    s.refresh("~/code/bil", &dirs(), Path::new(HOME));
    assert!(!s.open);
    s.refresh("~/code/bill", &dirs(), Path::new(HOME));
    assert!(s.open);
    s.close();
    s.refresh("~/code/bill", &dirs(), Path::new(HOME));
    assert!(!s.open, "esc stays closed until the text changes");
}

#[test]
fn reset_closes_and_remembers_text() {
    let mut s = refreshed("~/code/bil");
    s.reset("~/code/bil");
    assert!(!s.open);
    assert!(s.rows.is_empty());
}

#[test]
fn refresh_with_no_match_closes() {
    let s = refreshed("~/code/zzz");
    assert!(!s.open);
    assert!(s.rows.is_empty());
}

#[test]
fn refresh_of_a_relative_or_tilde_name_path_stays_closed() {
    for typed in ["code", "~", "~bob/x", "rel/path"] {
        assert!(!refreshed(typed).open, "{typed:?}");
    }
}

#[test]
fn step_clamps_at_both_ends() {
    let mut s = refreshed("~/code/");
    assert_eq!(s.rows.len(), 3);
    s.step(false);
    assert_eq!(s.cursor, 0);
    for _ in 0..5 {
        s.step(true);
    }
    assert_eq!(s.cursor, 2);
}

#[test]
fn go_into_returns_path_with_trailing_slash_and_closes() {
    let mut s = refreshed("~/code/bil");
    s.step(true);
    assert_eq!(s.go_into().as_deref(), Some("~/code/billing-docs/"));
    assert!(!s.open);
    assert_eq!(s.go_into(), None);
}

#[test]
fn pick_returns_the_folder_without_a_slash_and_stays_closed() {
    let mut s = refreshed("~/code/bil");
    assert_eq!(s.pick().as_deref(), Some("~/code/billing-api"));
    assert!(!s.open);
    s.refresh("~/code/billing-api", &dirs(), Path::new(HOME));
    assert!(!s.open, "picking does not list the folder's children");
    assert_eq!(s.pick(), None);
}

#[test]
fn typing_a_slash_after_a_pick_lists_the_children() {
    let mut s = refreshed("~/code/bil");
    let picked = s.pick().unwrap();
    s.refresh(&format!("{picked}/"), &dirs(), Path::new(HOME));
    assert!(s.open);
    let labels: Vec<&str> = s.rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, ["~/code/billing-api/src"]);
}

#[test]
fn refresh_after_go_into_lists_children() {
    let mut s = refreshed("~/code/bil");
    let taken = s.go_into().unwrap();
    s.refresh(&taken, &dirs(), Path::new(HOME));
    assert!(s.open);
    let labels: Vec<&str> = s.rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, ["~/code/billing-api/src"]);
}

#[test]
fn absolute_paths_list_from_the_root() {
    let s = refreshed("/s");
    let paths: Vec<&str> = s.rows.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(paths, ["/srv/"]);
}
