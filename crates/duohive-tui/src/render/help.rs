//! The `?` key map (UI §4.11): three columns on a wide screen, one scrolling column
//! below 100 cells.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::{LIGHT, draw_box, fg, put};
use crate::format::{pad_right, width};
use crate::keymap::{ActionId, KeyMapState, Keymap};
use crate::model::UiModel;
use crate::theme::Palette;

const TITLE: &str = "Keys · ? to close";
/// The mock-up's three columns need this much room.
const WIDE: u16 = 100;
/// Before a row's key.
const ROW_INDENT: &str = "  ";
/// Between a key and what it does.
const KEY_GAP: &str = " ";
/// Cells kept between columns.
const COLUMN_GAP: u16 = 2;
/// The last row of a screen that scrolls.
const MORE: &str = "↓ more · PgUp/PgDn scroll";

/// One key map heading and its rows.
struct Group {
    title: &'static str,
    rows: Vec<(String, String)>,
}

/// A binding as the map writes it.
fn key(km: &Keymap, action: ActionId) -> String {
    km.key(action).label()
}

/// Every heading and row, from the live bindings.
fn groups(km: &Keymap) -> Vec<Group> {
    let approve = key(km, ActionId::CardApprove);
    let edit = key(km, ActionId::CardEdit);
    let reject = key(km, ActionId::CardReject);
    let pair = |k: &str, what: &str| (k.to_owned(), what.to_owned());
    vec![
        Group {
            title: "Everywhere",
            rows: vec![
                pair("tab", "next pane/field"),
                pair("shift+tab", "previous pane"),
                pair(&key(km, ActionId::KeyMap), "this help"),
                pair(&key(km, ActionId::Settings), "settings"),
                pair(&key(km, ActionId::Worktrees), "worktrees"),
                pair(&key(km, ActionId::Filter), "filter tree by state"),
                pair("esc", "back one level"),
                pair("q", "quit (agents run on)"),
                pair("ctrl+c", "quit if not typing"),
                pair("ctrl+↑↓", "resize chat"),
            ],
        },
        Group {
            title: "Mouse",
            rows: vec![
                pair("click", "select / focus"),
                pair("right", "node menu"),
                pair("wheel", "scroll"),
                pair("drag", "move node"),
            ],
        },
        Group {
            title: "Org pane",
            rows: vec![
                pair("↑↓", "move"),
                pair("←→", "fold / unfold"),
                pair("enter", "open agent pane"),
                pair(&key(km, ActionId::GiveWork), "give work"),
                pair(&key(km, ActionId::StartStop), "start / stop"),
                pair(&key(km, ActionId::New), "new here…"),
                pair(&key(km, ActionId::Edit), "edit (Duo: Edit org)"),
                pair(&key(km, ActionId::Move), "move to…"),
                pair(&key(km, ActionId::Delete), "delete (stopped)"),
                pair(&key(km, ActionId::Zoom), "zoom agent"),
                pair(&key(km, ActionId::LastReply), "last reply on/off"),
                pair(&key(km, ActionId::OpenOrg), "open org.toml"),
            ],
        },
        Group {
            title: "Needs you",
            rows: vec![pair("↑↓", "move"), pair("enter", "go to agent or card")],
        },
        Group {
            title: "Agent pane",
            rows: vec![
                pair("enter", "start typing"),
                pair("ctrl+q", "stop typing"),
                pair(&key(km, ActionId::Zoom), "zoom / restore"),
                pair(&key(km, ActionId::GiveWork), "give work"),
                pair("PgUp/Dn", "scroll back"),
            ],
        },
        Group {
            title: "Chat (Duo)",
            rows: vec![
                pair("enter", "send"),
                pair("alt+enter", "new line"),
                pair("@ /", "agents / commands"),
                pair("↑", "newest card"),
                pair(&format!("{approve} {edit} {reject}"), "approve/edit/reject"),
                pair("PgUp/Dn", "scroll history"),
            ],
        },
    ]
}

/// The key column's width for `groups`.
fn key_width(groups: &[&Group]) -> usize {
    groups
        .iter()
        .flat_map(|g| g.rows.iter().map(|(k, _)| width(k)))
        .max()
        .unwrap_or(0)
}

/// `  key    what`.
fn row_text(key: &str, what: &str, kw: usize) -> String {
    format!("{ROW_INDENT}{}{KEY_GAP}{what}", pad_right(key, kw))
}

/// Draws the key map over `area`.
pub(super) fn draw(buf: &mut Buffer, area: Rect, st: KeyMapState, m: &UiModel, p: &Palette) {
    if area.width < 4 || area.height < 3 {
        return;
    }
    let inside = Style::default().fg(p.fg).bg(p.bg_panel);
    for y in area.y..area.bottom() {
        put(
            buf,
            area.x,
            y,
            &" ".repeat(usize::from(area.width)),
            usize::from(area.width),
            inside,
        );
    }
    draw_box(buf, area, TITLE, &LIGHT, fg(p.border_focus), inside);
    let groups = groups(&m.keymap);
    if area.width >= WIDE {
        wide(buf, area, &groups, p, inside);
    } else {
        narrow(buf, area, &groups, st, p, inside);
    }
}

/// Three columns: Everywhere and Mouse; Org pane and Needs you; Agent pane and Chat.
fn wide(buf: &mut Buffer, area: Rect, groups: &[Group], p: &Palette, inside: Style) {
    let room = usize::from(area.width - 2);
    let col_w = (room.saturating_sub(2 * usize::from(COLUMN_GAP))) / 3;
    let columns: [&[usize]; 3] = [&[0, 1], &[2, 3], &[4, 5]];
    let muted = Style::default().fg(p.fg_muted).bg(p.bg_panel);
    for (i, take) in columns.iter().enumerate() {
        let x = area.x + 1 + u16::try_from(i * (col_w + usize::from(COLUMN_GAP))).unwrap_or(0);
        let picked: Vec<&Group> = take.iter().filter_map(|n| groups.get(*n)).collect();
        let kw = key_width(&picked);
        let mut y = area.y + 1;
        for g in picked {
            if y + 1 >= area.bottom() {
                break;
            }
            put(buf, x, y, g.title, col_w, muted);
            y += 1;
            for (k, what) in &g.rows {
                if y >= area.bottom() - 1 {
                    break;
                }
                put(buf, x, y, &row_text(k, what, kw), col_w, inside);
                y += 1;
            }
            y += 1;
        }
    }
}

/// One scrolling column.
fn narrow(
    buf: &mut Buffer,
    area: Rect,
    groups: &[Group],
    st: KeyMapState,
    p: &Palette,
    inside: Style,
) {
    let room = usize::from(area.height.saturating_sub(2));
    let picked: Vec<&Group> = groups.iter().collect();
    let kw = key_width(&picked);
    let mut lines: Vec<(String, bool)> = Vec::new();
    for g in groups {
        lines.push((g.title.to_owned(), true));
        for (k, what) in &g.rows {
            lines.push((row_text(k, what, kw), false));
        }
    }
    let start = st.scroll.min(lines.len().saturating_sub(1));
    let shown = &lines[start..];
    let more = shown.len() > room;
    let body = if more { room.saturating_sub(1) } else { room };
    let muted = Style::default().fg(p.fg_muted).bg(p.bg_panel);
    let mut y = area.y + 1;
    for (text, heading) in shown.iter().take(body) {
        let style = if *heading { muted } else { inside };
        put(buf, area.x + 1, y, text, usize::from(area.width - 2), style);
        y += 1;
    }
    if more && y < area.bottom() - 1 {
        put(buf, area.x + 1, y, MORE, usize::from(area.width - 2), muted);
    }
}
