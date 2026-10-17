//! Settings (UI §4.10, P2-D13): You, Attention and Look, saved as they change, and where
//! the org's own settings live.

use duohive_core::settings::Theme;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;

use super::{LIGHT, draw_box, fg, put};
use crate::format::pad_right;
use crate::forms::{SettingsRow, SettingsState};
use crate::model::UiModel;
use crate::theme::Palette;

/// Wide enough for the longest line, with room to spare; narrower screens get the screen
/// less a margin.
const WIDTH: u16 = 76;
const MARGIN: u16 = 2;
/// Cells between the border and a group heading.
const GROUP_X: u16 = 2;
/// Cells between the border and a control's label.
const LABEL_X: u16 = 4;
/// The label column: the control's marker sits in its last cell, so controls line up.
const LABEL_W: u16 = 20;
/// `Last reply under agents`, which the mock-up runs straight into its checkbox.
const LAST_REPLY: &str = "Last reply under agents";
/// Cells a text box gives the text while it is being typed in.
const BOX_W: usize = 14;
const FOCUS: &str = "▌";
const TITLE: &str = "Settings · saved to settings.toml";
const FOOTER: &str = "changes save immediately · esc back";
const ORG_NOTE: &str = "org defaults and Duo: e on Duo (Edit org)";
const BROKEN: &str = "settings.toml has an error · changes last until DuoHive quits";
/// Border, blank, 3 headings and 5 control lines, the Org heading and its text, a note line,
/// a blank, the footer, border.
const HEIGHT: u16 = 15;

/// One control or piece of text on a line.
struct Piece {
    text: String,
    focused: bool,
    /// The terminal cursor's column inside `text`, while a box is open.
    cursor: Option<usize>,
}

/// Plain text.
fn plain(text: &str) -> Piece {
    Piece {
        text: text.to_owned(),
        focused: false,
        cursor: None,
    }
}

/// A control, led by `▌` while it has focus; focus is never colour alone.
fn control(st: &SettingsState, row: SettingsRow, text: &str) -> Piece {
    let focused = st.row == row;
    let marker = if focused { FOCUS } else { " " };
    Piece {
        text: format!("{marker}{text}"),
        focused,
        cursor: None,
    }
}

/// A text box: `[ Duong ]`, or while typing `[ text___ ]` with the cursor in it.
fn text_box(st: &SettingsState, row: SettingsRow, shown: &str) -> Piece {
    match st.editing.as_ref().filter(|_| st.row == row) {
        Some(line) => {
            let (visible, col) = line.window(BOX_W);
            let mut piece = control(st, row, &format!("[ {} ]", pad_right(&visible, BOX_W)));
            piece.cursor = Some(1 + 2 + col);
            piece
        }
        None => control(st, row, &format!("[ {shown} ]")),
    }
}

/// `[x]`, `[ ]`: the state in characters, not colour.
fn check(on: bool) -> &'static str {
    if on { "[x]" } else { "[ ]" }
}

fn theme_word(t: Theme) -> &'static str {
    match t {
        Theme::Auto => "auto",
        Theme::Dark => "dark",
        Theme::Light => "light",
    }
}

/// Writes the pieces left to right from `x`; returns the cursor if one is in them.
fn draw_pieces(
    buf: &mut Buffer,
    r: Rect,
    (x, y): (u16, u16),
    pieces: &[Piece],
    p: &Palette,
) -> Option<Position> {
    let right = usize::from(r.right().saturating_sub(1));
    let mut at = x;
    let mut cursor = None;
    for piece in pieces {
        let style = if piece.focused {
            p.selected().fg(p.fg)
        } else {
            Style::default().fg(p.fg).bg(p.bg_panel)
        };
        if let Some(col) = piece.cursor {
            cursor = Some(Position {
                x: at + u16::try_from(col).unwrap_or(0),
                y,
            });
        }
        at = put(
            buf,
            at,
            y,
            &piece.text,
            right.saturating_sub(usize::from(at)),
            style,
        );
    }
    cursor
}

/// Draws Settings centred over `area`; returns the cursor while a text box is open.
pub(super) fn draw(
    buf: &mut Buffer,
    area: Rect,
    st: &SettingsState,
    m: &UiModel,
    p: &Palette,
) -> Option<Position> {
    let w = WIDTH.min(area.width.saturating_sub(MARGIN));
    if w < 40 || area.height < HEIGHT {
        return None;
    }
    let r = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - HEIGHT) / 2,
        width: w,
        height: HEIGHT,
    };
    let inside = Style::default().fg(p.fg).bg(p.bg_panel);
    for y in r.y..r.bottom() {
        put(
            buf,
            r.x,
            y,
            &" ".repeat(usize::from(w)),
            usize::from(w),
            inside,
        );
    }
    draw_box(buf, r, TITLE, &LIGHT, fg(p.border_focus), inside);
    let room = usize::from(w - 2);
    let muted = Style::default().fg(p.fg_muted).bg(p.bg_panel);
    let label_x = r.x + 1 + LABEL_X;
    let control_x = label_x + LABEL_W - 1;
    let s = &m.settings;

    let at = |row: u16| r.y + row;
    let group = |buf: &mut Buffer, row: u16, text: &str| {
        put(buf, r.x + 1 + GROUP_X, at(row), text, room, inside);
    };
    let labelled = |buf: &mut Buffer, row: u16, label: &str, pieces: &[Piece]| {
        put(buf, label_x, at(row), label, room, muted);
        draw_pieces(buf, r, (control_x, at(row)), pieces, p)
    };

    group(buf, 2, "You");
    let name = s.your_name.as_deref().unwrap_or("not set");
    let name_box = text_box(st, SettingsRow::Name, name);
    let mut cursor = labelled(buf, 3, "Your name", &[name_box]);

    group(buf, 4, "Attention");
    let quiet = text_box(st, SettingsRow::Quiet, &s.quiet_after_minutes.to_string());
    cursor = cursor.or(labelled(buf, 5, "Quiet after", &[quiet, plain(" minutes")]));
    let needs = control(
        st,
        SettingsRow::NeedsYou,
        &format!("{} needs you", check(s.alerts.needs_you)),
    );
    let finished = control(
        st,
        SettingsRow::Finished,
        &format!("{} finished", check(s.alerts.finished)),
    );
    labelled(buf, 6, "Desktop alerts", &[needs, plain("  "), finished]);

    group(buf, 7, "Look");
    let theme = control(
        st,
        SettingsRow::Theme,
        &format!("[ {} ▾ ]", theme_word(s.theme)),
    );
    labelled(buf, 8, "Theme", &[theme]);
    let reply = control(st, SettingsRow::LastReply, check(s.show_last_reply));
    let motion = control(st, SettingsRow::ReduceMotion, check(s.reduce_motion));
    put(buf, label_x, at(9), LAST_REPLY, room, muted);
    let reply_x = label_x + u16::try_from(LAST_REPLY.chars().count()).unwrap_or(0);
    draw_pieces(
        buf,
        r,
        (reply_x, at(9)),
        &[reply, plain("  Reduce motion"), motion],
        p,
    );

    group(buf, 10, "Org");
    put(buf, label_x, at(11), ORG_NOTE, room, muted);
    if m.settings_error.is_some() {
        put(buf, label_x, at(12), BROKEN, room, fg(p.attention));
    }
    put(buf, r.x + 1 + GROUP_X, at(HEIGHT - 2), FOOTER, room, muted);
    cursor
}
