//! Terminal widget tests: snapshots from the S3.4 captures and behaviour checks.

use std::fmt::Write as _;

use duohive_core::view::{PaneChunk, PaneSize};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use super::TermView;

const CLAUDE_IDLE: &[u8] = include_bytes!("../../tests/fixtures/term/claude-idle.bin");
const OPENCODE_MENU: &[u8] = include_bytes!("../../tests/fixtures/term/opencode-slash-menu.bin");

const fn size(cols: u16, rows: u16) -> PaneSize {
    PaneSize { cols, rows }
}

fn bytes(b: &[u8]) -> PaneChunk {
    PaneChunk::Bytes(b.to_vec())
}

fn draw(view: &TermView, cols: u16, rows: u16, cursor: bool, no_color: bool) -> Buffer {
    let area = Rect::new(0, 0, cols, rows);
    let mut buf = Buffer::empty(area);
    view.render(area, &mut buf, cursor, no_color);
    buf
}

fn row_text(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

fn is_plain(st: Style) -> bool {
    let plain = |c: Option<Color>| matches!(c, None | Some(Color::Reset));
    plain(st.fg) && plain(st.bg) && st.add_modifier.is_empty()
}

/// Text rows, then every run of equally styled cells that is not plain.
fn dump(buf: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buf.area.height {
        let _ = writeln!(out, "{:02}|{}|", y, row_text(buf, y).trim_end());
    }
    out.push_str("--- styles\n");
    for y in 0..buf.area.height {
        let mut x = 0;
        while x < buf.area.width {
            let st = buf[(x, y)].style();
            let mut end = x + 1;
            while end < buf.area.width && buf[(end, y)].style() == st {
                end += 1;
            }
            if !is_plain(st) {
                let _ = writeln!(
                    out,
                    "{y:02} {x:02}..{end:02} fg={:?} bg={:?} mod={:?}",
                    st.fg, st.bg, st.add_modifier
                );
            }
            x = end;
        }
    }
    out
}

fn from_fixture(data: &[u8]) -> TermView {
    let mut v = TermView::new(size(80, 20));
    v.apply(&bytes(data));
    v
}

#[test]
fn claude_idle_80x20() {
    let v = from_fixture(CLAUDE_IDLE);
    insta::assert_snapshot!(dump(&draw(&v, 80, 20, true, false)));
}

#[test]
fn opencode_approval_80x20() {
    // The fixture is OpenCode's slash menu: S3.4 has no approval capture.
    let v = from_fixture(OPENCODE_MENU);
    insta::assert_snapshot!(dump(&draw(&v, 80, 20, true, false)));
}

#[test]
fn colours_and_wide_chars() {
    let mut v = TermView::new(size(20, 4));
    v.apply(&bytes(
        b"\x1b[31mred\x1b[0m \x1b[1;3;4;44mbold\x1b[0m \x1b[38;2;1;2;3mrgb\x1b[0m\r\n\
          \xe4\xb8\x96\xe7\x95\x8cx \x1b[38;5;200mi\x1b[7minv\x1b[0m",
    ));
    let buf = draw(&v, 20, 4, false, false);
    assert_eq!(buf[(0, 1)].symbol(), "世");
    assert_eq!(buf[(2, 1)].symbol(), "界");
    assert_eq!(buf[(1, 1)].symbol(), " ");
    assert_eq!(buf[(3, 1)].symbol(), " ");
    assert_eq!(buf[(4, 1)].symbol(), "x");
    insta::assert_snapshot!(dump(&buf));
}

#[test]
fn reset_replaces_screen() {
    let mut v = TermView::new(size(20, 4));
    v.apply(&bytes(b"\x1b[?1049hold text"));
    v.apply(&PaneChunk::Reset {
        bytes: b"new".to_vec(),
        cursor: (3, 0),
    });
    let buf = draw(&v, 20, 4, false, false);
    let all: String = (0..4).map(|y| row_text(&buf, y)).collect();
    assert!(!all.contains("old"));
    assert!(all.contains("new"));
}

#[test]
fn reset_places_cursor_and_keeps_hidden() {
    let mut v = TermView::new(size(10, 3));
    v.apply(&PaneChunk::Reset {
        bytes: b"\x1b[?25lab".to_vec(),
        cursor: (5, 2),
    });
    assert!(
        !draw(&v, 10, 3, true, false)[(5, 2)]
            .modifier
            .contains(Modifier::REVERSED)
    );
    v.apply(&PaneChunk::Reset {
        bytes: b"ab".to_vec(),
        cursor: (5, 2),
    });
    assert!(
        draw(&v, 10, 3, true, false)[(5, 2)]
            .modifier
            .contains(Modifier::REVERSED)
    );
}

#[test]
fn cursor_shown_only_when_asked() {
    let mut v = TermView::new(size(10, 2));
    v.apply(&bytes(b"hi"));
    assert!(
        draw(&v, 10, 2, true, false)[(2, 0)]
            .modifier
            .contains(Modifier::REVERSED)
    );
    assert!(
        !draw(&v, 10, 2, false, false)[(2, 0)]
            .modifier
            .contains(Modifier::REVERSED)
    );
}

#[test]
fn no_color_keeps_inverse() {
    let mut v = TermView::new(size(10, 2));
    v.apply(&bytes(b"\x1b[31;42mhi"));
    let buf = draw(&v, 10, 2, true, true);
    let cursor = &buf[(2, 0)];
    assert!(cursor.modifier.contains(Modifier::REVERSED));
    assert_eq!(cursor.fg, Color::Reset);
    assert_eq!(cursor.bg, Color::Reset);
    assert_eq!(buf[(0, 0)].fg, Color::Reset);
    assert_eq!(buf[(0, 0)].bg, Color::Reset);
}

#[test]
fn resize_keeps_bottom_lines() {
    let mut v = TermView::new(size(10, 5));
    v.apply(&bytes(b"l1\r\nl2\r\nl3\r\nl4\r\nl5"));
    v.resize(size(10, 3));
    let buf = draw(&v, 10, 3, false, false);
    let rows: Vec<String> = (0..3)
        .map(|y| row_text(&buf, y).trim_end().to_owned())
        .collect();
    assert_eq!(rows, ["l3", "l4", "l5"]);
    v.resize(size(6, 3));
    assert_eq!(row_text(&draw(&v, 6, 3, false, false), 2).trim_end(), "l5");
}

fn rows_of(v: &TermView, cols: u16, rows: u16) -> Vec<String> {
    let buf = draw(v, cols, rows, false, false);
    (0..rows)
        .map(|y| row_text(&buf, y).trim_end().to_owned())
        .collect()
}

#[test]
fn resize_with_cursor_mid_screen_drops_nothing() {
    let mut v = TermView::new(size(10, 20));
    v.apply(&bytes(b"l1\r\nl2\r\nl3\r\nl4"));
    v.resize(size(10, 10));
    let rows = rows_of(&v, 10, 10);
    assert_eq!(&rows[..4], ["l1", "l2", "l3", "l4"]);
}

#[test]
fn resize_on_the_alternate_screen_keeps_it() {
    let mut v = TermView::new(size(10, 4));
    v.apply(&bytes(b"main\x1b[?1049h\x1b[Ha1\r\na2\r\na3\r\na4"));
    v.resize(size(10, 2));
    assert_eq!(rows_of(&v, 10, 2), ["a3", "a4"]);
    assert!(v.parser.screen().alternate_screen());
}

#[test]
fn resize_ignores_an_app_scroll_region() {
    let mut v = TermView::new(size(10, 6));
    v.apply(&bytes(
        b"\x1b[Hh1\r\nr2\r\nr3\r\nr4\r\nr5\r\nf6\x1b[2;4r\x1b[6;1H",
    ));
    v.resize(size(10, 3));
    assert_eq!(rows_of(&v, 10, 3), ["r4", "r5", "f6"]);
}

#[test]
fn clipped_wide_glyph_is_blanked() {
    let mut v = TermView::new(size(6, 1));
    v.apply(&bytes("ab世".as_bytes()));
    let buf = draw(&v, 3, 1, false, false);
    assert_eq!(row_text(&buf, 0), "ab ");
}
