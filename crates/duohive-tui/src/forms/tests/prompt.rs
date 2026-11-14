//! The Own prompt: a box that grows to four rows, and `↑↓` that move between its lines
//! (UI §4.4.1 *Moving*, DSP.6).

use ratatui::layout::Position;

use super::*;
use crate::render::keys_at;

/// Billing-api's form with the Own prompt focused and holding `lines`, the cursor at the end.
fn prompt_form(lines: &[&str]) -> (Form, Org) {
    let org = org();
    let mut f = edit_billing(&org, &stopped());
    f.focus_field(FieldId::Prompt);
    clear(&mut f, &org);
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            press(&mut f, &org, Action::NewLine);
        }
        typed(&mut f, &org, line);
    }
    (f, org)
}

fn cursor(f: &Form) -> usize {
    f.field(FieldId::Prompt).unwrap().edit.cursor()
}

fn in_prompt(f: &Form) -> bool {
    f.focused_is(FieldId::Prompt)
}

#[test]
fn up_inside_a_prompt_moves_the_cursor_not_the_field() {
    let (mut f, org) = prompt_form(&["line1", "line2"]);
    assert_eq!(cursor(&f), 11);
    press(&mut f, &org, Action::Up);
    assert!(in_prompt(&f));
    assert_eq!(cursor(&f), 5, "column 5 of the first line");
}

#[test]
fn up_on_the_first_line_goes_to_the_previous_field_from_any_column() {
    let (mut f, org) = prompt_form(&["line1", "line2"]);
    press(&mut f, &org, Action::Up);
    press(&mut f, &org, Action::Left);
    assert_eq!(cursor(&f), 4, "mid-line");
    press(&mut f, &org, Action::Up);
    assert!(
        f.focused_is(FieldId::Worktrees),
        "the field before the prompt"
    );
}

#[test]
fn down_on_the_last_line_goes_to_the_next_field() {
    let (mut f, org) = prompt_form(&["line1", "line2"]);
    press(&mut f, &org, Action::Up);
    assert!(in_prompt(&f) && cursor(&f) == 5, "up stayed in the box");
    press(&mut f, &org, Action::Down);
    assert!(in_prompt(&f), "down from line 1 stays in the box");
    assert_eq!(cursor(&f), 11);
    press(&mut f, &org, Action::Down);
    assert!(f.on_save());
}

#[test]
fn one_line_prompt_arrows_leave_as_before() {
    let org = org();
    let mut f = edit_billing(&org, &stopped());
    f.focus_field(FieldId::Prompt);
    press(&mut f, &org, Action::Up);
    assert!(f.focused_is(FieldId::Worktrees));
    f.focus_field(FieldId::Prompt);
    press(&mut f, &org, Action::Down);
    assert!(f.on_save());
}

#[test]
fn a_wrapped_long_line_counts_as_lines() {
    let long = "abcd ".repeat(20);
    let (mut f, org) = prompt_form(&[long.as_str()]);
    assert_eq!(cursor(&f), 100);
    press(&mut f, &org, Action::Up);
    assert!(in_prompt(&f), "the wrapped second row is a line");
    assert!(cursor(&f) < 50, "now in the first row");
    press(&mut f, &org, Action::Up);
    assert!(f.focused_is(FieldId::Worktrees));
}

#[test]
fn tab_and_shift_tab_always_leave_the_prompt() {
    let (mut f, org) = prompt_form(&["line1", "line2"]);
    press(&mut f, &org, Action::Up);
    press(&mut f, &org, Action::Tab);
    assert!(f.on_save());
    f.focus_field(FieldId::Prompt);
    press(&mut f, &org, Action::BackTab);
    assert!(f.focused_is(FieldId::Worktrees));
}

#[test]
fn up_from_an_empty_first_line_leaves() {
    let (mut f, org) = prompt_form(&["", "abc"]);
    press(&mut f, &org, Action::Up);
    assert!(in_prompt(&f));
    assert_eq!(cursor(&f), 0);
    press(&mut f, &org, Action::Up);
    assert!(f.focused_is(FieldId::Worktrees));
}

fn prompt_model(f: Form) -> UiModel {
    let mut ui = model();
    ui.screen = ratatui::layout::Rect::new(0, 0, 100, 30);
    ui.dialog = Some(Dialog::Form(Box::new(f)));
    ui
}

#[test]
fn prompt_one_row_form_unchanged() {
    let org = org();
    let f = edit_billing(&org, &stopped());
    let screen = screen_text(&prompt_model(f));
    assert!(
        screen.contains("[ Own: Owns the billing service; invoices and retries."),
        "{screen}"
    );
    assert!(!screen.contains('↵'), "{screen}");
}

#[test]
fn prompt_grows_to_two_rows() {
    let (f, _) = prompt_form(&["first line", "second line"]);
    let screen = screen_text(&prompt_model(f));
    let lines: Vec<&str> = screen.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.contains("[ Own: first line"))
        .unwrap_or_else(|| panic!("{screen}"));
    assert!(lines[at + 1].contains("[      second line"), "{screen}");
    assert!(!screen.contains('↵'), "{screen}");
    insta::assert_snapshot!("prompt_two_rows_100x30", screen);
}

#[test]
fn prompt_caps_at_four_rows_and_scrolls() {
    let (f, _) = prompt_form(&["row-1", "row-2", "row-3", "row-4", "row-5", "row-6"]);
    let screen = screen_text(&prompt_model(f));
    for shown in ["row-3", "row-4", "row-5", "row-6"] {
        assert!(screen.contains(shown), "{shown}\n{screen}");
    }
    for hidden in ["row-1", "row-2"] {
        assert!(!screen.contains(hidden), "{hidden}\n{screen}");
    }
}

#[test]
fn form_with_four_row_prompt_keeps_buttons_at_80x24() {
    let (f, _) = prompt_form(&["a", "b", "c", "d"]);
    let screen = screen_text_at(&prompt_model(f), 80, 24);
    assert!(screen.contains("[ Save ]"), "{screen}");
    assert!(screen.contains("[ Cancel ]"), "{screen}");
}

#[test]
fn click_on_a_later_prompt_row_focuses_it() {
    let (mut f, org) = prompt_form(&["first line", "second line"]);
    f.focus_field(FieldId::Name);
    press(&mut f, &org, Action::Home);
    let mut ui = prompt_model(f);
    let buf = render_buffer(&ui, &ctx(), 100, 30);
    let (x, y) = rows(&buf, 0, 100)
        .iter()
        .enumerate()
        .find_map(|(y, r)| r.find("second line").map(|x| (x, y)))
        .expect("the second row is drawn");
    let at = Position::new(
        u16::try_from(r_chars(&buf, y, x)).unwrap(),
        u16::try_from(y).unwrap(),
    );
    for k in keys_at(&ui, at).expect("the row is clickable") {
        update(&mut ui, k);
    }
    assert!(form(&ui).focused_is(FieldId::Prompt));
}

/// The column of byte offset `byte` in row `y`.
fn r_chars(buf: &Buffer, y: usize, byte: usize) -> usize {
    rows(buf, 0, 100)[y][..byte].chars().count()
}

#[test]
fn role_prompt_rows_line_up_under_the_first() {
    // The role form's prompt has its label outside the box, so every row starts after it.
    let org = org();
    let mut f = Form::edit_role(&org, &backend(), &dirs(), Path::new(HOME));
    f.focus_field(FieldId::Prompt);
    clear(&mut f, &org);
    typed(&mut f, &org, "first line");
    press(&mut f, &org, Action::NewLine);
    typed(&mut f, &org, "second line");
    let screen = screen_text(&prompt_model(f));
    let lines: Vec<&str> = screen.lines().collect();
    let first = lines
        .iter()
        .position(|l| l.contains("first line"))
        .unwrap_or_else(|| panic!("{screen}"));
    let col = |l: &str, needle: &str| l[..l.find(needle).unwrap()].chars().count();
    assert_eq!(
        col(lines[first], "first line"),
        col(lines[first + 1], "second line"),
        "{screen}"
    );
}
