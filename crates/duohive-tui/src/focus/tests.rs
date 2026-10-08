//! Focus order and the layout table (UI §3, P2-D3, P2-D19).

use ratatui::layout::Rect;

use super::{Focus, Pane, chat_extra_max, layout};

fn area(w: u16, h: u16) -> Rect {
    Rect::new(0, 0, w, h)
}

#[test]
fn next_and_prev_cycle_every_pane() {
    let order = [Pane::Org, Pane::NeedsYou, Pane::Agent, Pane::Chat];
    for (i, pane) in order.iter().enumerate() {
        let f = Focus {
            pane: *pane,
            ..Focus::default()
        };
        assert_eq!(f.next(), order[(i + 1) % 4]);
        assert_eq!(f.prev(), order[(i + 3) % 4]);
    }
}

#[test]
fn mixed_sizes_take_the_smaller_row() {
    let l = layout(area(130, 30), 0, false).unwrap();
    assert_eq!((l.org.width, l.chat.height, l.compact), (50, 6, false));
    let l = layout(area(110, 24), 0, false).unwrap();
    assert_eq!((l.org.width, l.chat.height, l.compact), (32, 4, true));
    let l = layout(area(90, 40), 0, false).unwrap();
    assert_eq!((l.org.width, l.chat.height, l.compact), (32, 4, true));
    let l = layout(area(160, 48), 0, false).unwrap();
    assert_eq!((l.org.width, l.chat.height, l.compact), (50, 6, false));
}

#[test]
fn panes_tile_the_screen_between_header_and_footer() {
    let l = layout(area(120, 36), 0, false).unwrap();
    assert_eq!(l.org, Rect::new(0, 1, 50, l.org.height));
    assert_eq!(l.needs_you.y, l.org.bottom());
    assert_eq!(l.needs_you.bottom(), l.agent.bottom());
    assert_eq!(l.agent, Rect::new(50, 1, 70, 28));
    assert_eq!(l.chat, Rect::new(0, 29, 120, 6));
    // Needs you can take a third of the org pane's inside, plus its separator and border.
    assert_eq!(l.needs_you.height, 26 / 3 + 2);
    let fitted = l.with_needs_you(2);
    assert_eq!(fitted.needs_you.height, 4);
    assert_eq!(fitted.org.height + fitted.needs_you.height, 28);
    assert_eq!(l.with_needs_you(50).needs_you, l.needs_you);
}

#[test]
fn chat_grows_to_12_rows_while_typing_at_120x36() {
    let rows = |w, h, typing| layout(area(w, h), 0, typing).unwrap().chat.height;
    assert_eq!(rows(120, 36, false), 6);
    assert_eq!(rows(120, 36, true), 12);
    assert_eq!(layout(area(120, 36), 0, true).unwrap().agent.height, 22);
    // Only the largest row grows.
    assert_eq!(rows(119, 36, true), 6);
    assert_eq!(rows(120, 35, true), 6);
    assert_eq!(rows(80, 24, true), 4);
}

#[test]
fn chat_extra_is_clamped_to_p2_d3_bounds() {
    // The agent pane keeps 6 rows: 36 - header - footer - 6 = 28 for the chat.
    assert_eq!(chat_extra_max(area(120, 36)), 22);
    assert_eq!(chat_extra_max(area(80, 24)), 12);
    let rows = |extra, typing| layout(area(120, 36), extra, typing).unwrap().chat.height;
    assert_eq!(rows(3, false), 9);
    assert_eq!(rows(3, true), 12);
    assert_eq!(rows(10, true), 16);
    assert_eq!(rows(99, false), 28);
    assert_eq!(rows(-5, false), 6);
}

#[test]
fn below_80x24_there_is_no_layout() {
    assert!(layout(area(79, 24), 0, false).is_none());
    assert!(layout(area(80, 23), 0, false).is_none());
    assert!(layout(area(80, 24), 0, false).is_some());
}
