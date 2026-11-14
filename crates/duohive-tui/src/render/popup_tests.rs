//! How a popup fits under its field: as many rows as the screen leaves, never an invisible
//! popup that still takes keys (UI §4.4.1).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::form::{draw_rows_popup, suggest_popup};
use crate::dir_suggest::{DirSuggest, SuggestRow};
use crate::theme::Palette;

const HINT: &str = "↑↓ · enter pick · tab into · esc close";

fn nine_folders() -> DirSuggest {
    let mut s = DirSuggest::default();
    s.open = true;
    s.rows = (1..=9)
        .map(|i| SuggestRow {
            path: format!("~/many/proj-{i}/"),
            label: format!("~/many/proj-{i}"),
            is_git: false,
        })
        .collect();
    s
}

/// The popup under a field at row `y` of a 12-row screen, drawn as text.
fn popup_at_row(y: u16) -> String {
    let area = Rect::new(0, 0, 60, 12);
    let mut buf = Buffer::empty(area);
    let popup = suggest_popup(&nine_folders());
    draw_rows_popup(&mut buf, area, (2, y), &popup, 58, &Palette::mono());
    (0..area.height)
        .map(|row| {
            (0..area.width)
                .map(|x| buf[(x, row)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn popup_with_room_shows_six_rows_and_a_more_line() {
    let screen = popup_at_row(0);
    assert!(screen.contains("~/many/proj-6"), "{screen}");
    assert!(!screen.contains("~/many/proj-7"), "{screen}");
    assert!(screen.contains("… 3 more"), "{screen}");
    assert!(screen.contains(HINT), "{screen}");
}

#[test]
fn popup_shrinks_to_the_rows_that_fit_below_the_field() {
    // The field is on row 4: the box starts on row 5 and the hint needs the last line.
    let screen = popup_at_row(4);
    assert!(screen.contains("~/many/proj-3"), "{screen}");
    assert!(!screen.contains("~/many/proj-4"), "{screen}");
    assert!(screen.contains("… 6 more"), "{screen}");
    assert!(screen.contains(HINT), "{screen}");
}

#[test]
fn popup_is_not_drawn_when_not_even_one_row_fits() {
    let screen = popup_at_row(10);
    assert!(!screen.contains("proj-1"), "{screen}");
    assert!(!screen.contains(HINT), "{screen}");
}
