//! The Give work task box's wrapping (UI §4.5).

use super::give_work::wrap_task;

fn rows(text: &str, cells: usize) -> Vec<String> {
    wrap_task(text, 0, cells).0
}

#[test]
fn wraps_at_word_boundaries() {
    assert_eq!(rows("aaa bbbb", 6), ["aaa ", "bbbb"]);
    assert_eq!(
        rows("Add retry cases for PAY-88. AC: 3 tries", 22),
        ["Add retry cases for ", "PAY-88. AC: 3 tries"]
    );
}

#[test]
fn a_word_longer_than_the_line_breaks_as_a_last_resort() {
    assert_eq!(rows("ab cdefghijk", 4), ["ab ", "cdef", "ghij", "k"]);
}

#[test]
fn spaces_past_the_edge_hang_and_are_not_drawn() {
    assert_eq!(rows("abcd   efg", 4), ["abcd", "efg"]);
}

#[test]
fn newlines_still_break() {
    assert_eq!(rows("ab\ncd", 8), ["ab", "cd"]);
    assert_eq!(rows("ab\n", 8), ["ab", ""]);
}

#[test]
fn the_cursor_follows_a_wrapped_word() {
    let text = "aaa bbbb";
    // On `b`, the first grapheme of the wrapped word.
    assert_eq!(wrap_task(text, 4, 6).1, (1, 0));
    // On the second `b`.
    assert_eq!(wrap_task(text, 5, 6).1, (1, 1));
    // On the space before `bbbb`, at the end of the first row.
    assert_eq!(wrap_task(text, 3, 6).1, (0, 3));
    // After the text.
    assert_eq!(wrap_task(text, 8, 6).1, (1, 4));
}

#[test]
fn the_cursor_after_a_full_row_starts_the_next() {
    assert_eq!(
        wrap_task("abcd", 4, 4),
        (vec!["abcd".to_owned(), String::new()], (1, 0))
    );
    // On a hanging space: kept at the box's edge.
    assert_eq!(wrap_task("abcd  e", 5, 4).1, (0, 4));
}

#[test]
fn the_cursor_on_a_newline_ends_its_row() {
    assert_eq!(wrap_task("ab\ncd", 2, 8).1, (0, 2));
    assert_eq!(wrap_task("ab\ncd", 3, 8).1, (1, 0));
}
