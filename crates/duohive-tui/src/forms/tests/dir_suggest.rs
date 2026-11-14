//! Folder suggestions under a Directory field (UI §4.4.1, DSP.3).

use ratatui::layout::Position;

use super::*;
use crate::forms::Field;
use crate::render::keys_at;

const HINT: &str = "↑↓ · enter pick · tab into · esc close";

/// The fake disk: `~/code` holds three folders, one of them a repo with a `src` inside, and
/// `~/many` holds nine.
fn listing_dirs() -> FakeDirs {
    let nine: Vec<(String, bool)> = (1..=9).map(|i| (format!("proj-{i}"), false)).collect();
    let nine: Vec<(&str, bool)> = nine.iter().map(|(n, g)| (n.as_str(), *g)).collect();
    FakeDirs::new(HashMap::from([(
        PathBuf::from("/home/me/code/billing-api"),
        DirStatus::GitRepo,
    )]))
    .with_listing(
        "/home/me/code",
        &[
            ("billing-api", true),
            ("billing-docs", false),
            ("web", false),
        ],
    )
    .with_listing("/home/me/code/billing-api", &[("src", false)])
    .with_listing("/home/me/many", &nine)
}

/// What `input.rs` does after a key: handle it, probe the directory, refresh the popup.
fn press_l(f: &mut Form, org: &Org, a: Action) -> FormOutcome {
    let d = listing_dirs();
    let out = f.handle(a, org);
    f.probe_dir(org, &d, Path::new(HOME));
    f.refresh_suggest(&d, Path::new(HOME));
    out
}

fn typed_l(f: &mut Form, org: &Org, s: &str) {
    for c in s.chars() {
        press_l(f, org, Action::Char(c));
    }
}

/// Billing-api's form with the Directory field focused and emptied.
fn dir_form() -> (Form, Org) {
    let org = org();
    let mut f = edit_billing(&org, &stopped());
    f.focus_field(FieldId::Directory);
    clear(&mut f, &org);
    (f, org)
}

fn dir_field(f: &Form) -> &Field {
    f.field(FieldId::Directory).unwrap()
}

fn labels(f: &Form) -> Vec<&str> {
    dir_field(f)
        .suggest
        .rows
        .iter()
        .map(|r| r.label.as_str())
        .collect()
}

#[test]
fn typing_a_path_opens_the_popup() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "~/code/bil");
    assert!(dir_field(&f).suggest.open);
    assert_eq!(labels(&f), ["~/code/billing-api", "~/code/billing-docs"]);
}

#[test]
fn opening_the_form_or_focusing_the_field_does_not_open_it() {
    let org = org();
    let mut f = edit_billing(&org, &stopped());
    assert!(!dir_field(&f).suggest.open);
    f.focus_field(FieldId::Name);
    while !f.focused_is(FieldId::Directory) {
        press_l(&mut f, &org, Action::Down);
    }
    assert!(!dir_field(&f).suggest.open);
    assert_eq!(text(&f, FieldId::Directory), "~/code/billing");
}

#[test]
fn down_then_tab_goes_into_the_folder_and_lists_children() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "~/code/bil");
    press_l(&mut f, &org, Action::Down);
    assert_eq!(dir_field(&f).suggest.cursor, 1);
    press_l(&mut f, &org, Action::Up);
    press_l(&mut f, &org, Action::Tab);
    assert_eq!(text(&f, FieldId::Directory), "~/code/billing-api/");
    assert!(f.focused_is(FieldId::Directory), "taking a row stays");
    assert!(dir_field(&f).suggest.open);
    assert_eq!(labels(&f), ["~/code/billing-api/src"]);
}

#[test]
fn enter_picks_when_open_and_moves_on_when_closed() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "~/code/we");
    press_l(&mut f, &org, Action::DialogEnter);
    assert_eq!(text(&f, FieldId::Directory), "~/code/web");
    assert!(f.focused_is(FieldId::Directory));
    assert!(!dir_field(&f).suggest.open);
    press_l(&mut f, &org, Action::DialogEnter);
    assert!(!f.focused_is(FieldId::Directory));
}

#[test]
fn enter_does_not_list_the_folders_children_but_typing_a_slash_does() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "~/code/bil");
    press_l(&mut f, &org, Action::DialogEnter);
    assert_eq!(text(&f, FieldId::Directory), "~/code/billing-api");
    assert!(
        !dir_field(&f).suggest.open,
        "billing-api has a src, still closed"
    );
    typed_l(&mut f, &org, "/");
    assert_eq!(labels(&f), ["~/code/billing-api/src"]);
}

#[test]
fn esc_closes_the_popup_then_cancels_the_form() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "~/code/bil");
    assert_eq!(press_l(&mut f, &org, Action::Esc), FormOutcome::Continue);
    assert!(!dir_field(&f).suggest.open);
    assert_eq!(text(&f, FieldId::Directory), "~/code/bil");
    assert_eq!(press_l(&mut f, &org, Action::Esc), FormOutcome::Cancel);
}

#[test]
fn up_down_leave_the_field_when_the_popup_is_closed() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "~/code/bil");
    press_l(&mut f, &org, Action::Esc);
    press_l(&mut f, &org, Action::Down);
    assert!(!f.focused_is(FieldId::Directory));
}

#[test]
fn relative_path_gets_no_popup() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "code/bil");
    assert!(!dir_field(&f).suggest.open);
}

#[test]
fn paste_opens_the_popup() {
    let (mut f, org) = dir_form();
    f.paste("~/code/bil", &org);
    f.refresh_suggest(&listing_dirs(), Path::new(HOME));
    assert_eq!(labels(&f), ["~/code/billing-api", "~/code/billing-docs"]);
}

#[test]
fn paste_line_break_becomes_a_space_and_closes_it() {
    let (mut f, org) = dir_form();
    f.paste("~/code/\nbil", &org);
    f.refresh_suggest(&listing_dirs(), Path::new(HOME));
    assert_eq!(text(&f, FieldId::Directory), "~/code/ bil");
    assert!(!dir_field(&f).suggest.open);
}

#[test]
fn popup_closes_when_focus_leaves() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "~/code/bil");
    press_l(&mut f, &org, Action::BackTab);
    assert!(!f.focused_is(FieldId::Directory));
    assert!(!dir_field(&f).suggest.open);
}

#[test]
fn picking_a_row_updates_the_check_note() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "~/code/billing-a");
    press_l(&mut f, &org, Action::DialogEnter);
    assert_eq!(text(&f, FieldId::Directory), "~/code/billing-api");
    assert_eq!(dir_field(&f).suffix.as_deref(), Some("✓ git repo"));
}

#[test]
fn taking_a_row_updates_the_check_note() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "~/code/billing-a");
    press_l(&mut f, &org, Action::Tab);
    assert_eq!(dir_field(&f).suffix.as_deref(), Some("✓ git repo"));
}

fn dialog_model(f: Form) -> UiModel {
    let mut ui = model();
    ui.dirs = SharedDirs(Arc::new(listing_dirs()));
    ui.screen = ratatui::layout::Rect::new(0, 0, 100, 30);
    ui.dialog = Some(Dialog::Form(Box::new(f)));
    ui
}

/// The cell where `needle` starts on the 100×30 screen.
fn find(ui: &UiModel, needle: &str) -> Position {
    let buf = render_buffer(ui, &ctx(), 100, 30);
    for (y, row) in rows(&buf, 0, 100).iter().enumerate() {
        if let Some(at) = row.find(needle) {
            let x = row[..at].chars().count();
            return Position::new(u16::try_from(x).unwrap(), u16::try_from(y).unwrap());
        }
    }
    panic!("{needle:?} not on screen:\n{}", screen_text(ui));
}

#[test]
fn clicking_a_row_takes_it() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "~/code/bil");
    let mut ui = dialog_model(f);
    let at = find(&ui, "~/code/billing-docs");
    for k in keys_at(&ui, at).expect("the row is clickable") {
        update(&mut ui, k);
    }
    assert_eq!(text(form(&ui), FieldId::Directory), "~/code/billing-docs");
}

#[test]
fn directory_popup_under_field() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "~/code/bil");
    let screen = screen_text(&dialog_model(f));
    assert!(screen.contains(HINT), "{screen}");
    assert!(screen.contains("~/code/billing-api   git"), "{screen}");
    insta::assert_snapshot!("directory_popup_100x30", screen);
}

#[test]
fn directory_popup_caps_at_six_rows_with_more_line() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "~/many/");
    let screen = screen_text(&dialog_model(f));
    assert!(screen.contains("~/many/proj-6"), "{screen}");
    assert!(!screen.contains("~/many/proj-7"), "{screen}");
    assert!(screen.contains("… 3 more"), "{screen}");
}

#[test]
fn clicking_another_field_with_the_popup_open_leaves_the_path_alone() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "~/code/bil");
    let mut ui = dialog_model(f);
    let at = find(&ui, "[ billing-api");
    for k in keys_at(&ui, at).expect("the Name box is clickable") {
        update(&mut ui, k);
    }
    assert_eq!(text(form(&ui), FieldId::Directory), "~/code/bil");
    assert!(form(&ui).focused_is(FieldId::Name));
    assert!(!dir_field(form(&ui)).suggest.open);
}

#[test]
fn clicking_cancel_with_the_popup_open_cancels_in_one_click() {
    let (mut f, org) = dir_form();
    typed_l(&mut f, &org, "~/code/bil");
    let mut ui = dialog_model(f);
    let at = find(&ui, "[ Cancel ]");
    for k in keys_at(&ui, at).expect("Cancel is clickable") {
        update(&mut ui, k);
    }
    assert!(ui.dialog.is_none());
}
