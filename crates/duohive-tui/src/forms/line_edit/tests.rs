//! Wrapped rows and vertical moves in the line editor (DSP.5).

use super::*;

/// `text` with the cursor at grapheme `at`.
fn edit_at(text: &str, at: usize) -> LineEdit {
    let mut e = LineEdit::new(text);
    e.home();
    for _ in 0..at {
        e.right();
    }
    e
}

#[test]
fn wrap_breaks_at_newline() {
    assert_eq!(LineEdit::new("ab\ncd").wrap(10), [0..3, 3..5]);
}

#[test]
fn wrap_breaks_at_last_space_that_fits() {
    assert_eq!(LineEdit::new("hello world").wrap(8), [0..6, 6..11]);
}

#[test]
fn wrap_long_word_mid_word() {
    assert_eq!(LineEdit::new("abcdefghij").wrap(4), [0..4, 4..8, 8..10]);
}

#[test]
fn wrap_never_splits_wide_graphemes() {
    assert_eq!(LineEdit::new("日本語").wrap(3), [0..1, 1..2, 2..3]);
    assert_eq!(LineEdit::new("日本語").wrap(5), [0..2, 2..3]);
    assert_eq!(
        LineEdit::new("日本").wrap(1),
        [0..1, 1..2],
        "always progresses"
    );
}

#[test]
fn wrap_trailing_newline_gives_empty_last_row() {
    assert_eq!(LineEdit::new("ab\n").wrap(10), [0..3, 3..3]);
    assert_eq!(LineEdit::new("\n").wrap(10), [0..1, 1..1]);
}

#[test]
fn wrap_empty_text_is_one_row() {
    let rows = LineEdit::new("").wrap(10);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].is_empty());
}

#[test]
fn line_up_down_keep_column_across_a_short_row() {
    // Rows: `abcdef\n` 0..7, `ab\n` 7..10, `abcdef` 10..16.
    let mut e = edit_at("abcdef\nab\nabcdef", 5);
    assert!(e.line_down(80));
    assert_eq!(e.cursor(), 9, "the short row's end, before its newline");
    assert!(e.line_down(80));
    assert_eq!(e.cursor(), 15, "back to the remembered column 5");
    assert!(e.line_up(80));
    assert_eq!(e.cursor(), 9);
}

#[test]
fn line_up_and_down_cross_a_soft_wrap() {
    let mut e = LineEdit::new("hello world");
    assert_eq!(e.cursor(), 11);
    assert!(e.line_up(8));
    assert_eq!(e.cursor(), 5, "column 5 of `hello `");
    assert!(e.line_down(8));
    assert_eq!(e.cursor(), 11);
}

#[test]
fn line_up_on_first_row_returns_false_and_stays() {
    let mut e = edit_at("abc\ndef", 3);
    assert!(!e.line_up(80));
    assert_eq!(e.cursor(), 3);
}

#[test]
fn line_down_on_last_row_returns_false_and_stays() {
    let mut e = edit_at("abc\ndef", 5);
    assert!(!e.line_down(80));
    assert_eq!(e.cursor(), 5);
}

#[test]
fn horizontal_move_clears_remembered_column() {
    let mut e = edit_at("abcdef\nab\nabcdef", 5);
    assert!(e.line_down(80));
    e.left();
    assert_eq!(e.cursor(), 8);
    assert!(e.line_down(80));
    assert_eq!(e.cursor(), 11, "column 1, not the old 5");
}

#[test]
fn typing_clears_remembered_column() {
    let mut e = edit_at("abcdef\nab\nabcdef", 5);
    assert!(e.line_down(80));
    e.insert('x');
    assert!(e.line_down(80));
    // The last row now starts at 11; column 3 (after `abx`) is 14, the old column 5 would be 16.
    assert_eq!(e.cursor(), 14, "column 3 after `abx`");
}

#[test]
fn window_rows_scrolls_to_cursor_row() {
    let e = LineEdit::new("a\nb\nc\nd\ne");
    assert_eq!(
        e.window_rows(10, 3),
        (vec!["c".into(), "d".into(), "e".into()], (2, 1))
    );
    let e = edit_at("a\nb\nc\nd\ne", 0);
    assert_eq!(
        e.window_rows(10, 3),
        (vec!["a".into(), "b".into(), "c".into()], (0, 0))
    );
}

#[test]
fn window_rows_shows_no_newline_glyph_and_fewer_rows_when_short() {
    let e = edit_at("ab\ncd", 1);
    assert_eq!(
        e.window_rows(10, 4),
        (vec!["ab".into(), "cd".into()], (0, 1))
    );
}
