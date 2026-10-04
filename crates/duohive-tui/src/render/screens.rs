//! Full-screen messages: a window too small (UI §3) and missing tmux (UI §4.12).

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::{LIGHT, draw_box, put};
use crate::format::width;

const TOO_SMALL: &str = "Make the window at least 80×24";
/// The tmux box's size (UI §4.12).
const TMUX_BOX_WIDTH: u16 = 72;
const TMUX_BOX_HEIGHT: u16 = 10;

/// A screen shown instead of the main screen until the problem is fixed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockingScreen {
    /// tmux is missing or older than 3.2; `found` is the version found, if any.
    TmuxMissing {
        /// The version found, e.g. `3.1c`.
        found: Option<String>,
    },
}

/// Only the size request and the Needs you count, centred.
pub(super) fn too_small(buf: &mut Buffer, area: Rect, need: usize) {
    let count = format!("◆ {need} need you");
    let mid = area.y + area.height / 2;
    for (dy, text) in [(0, TOO_SMALL), (1, count.as_str())] {
        let x = area.x + centre(area.width, width(text));
        let y = mid.saturating_sub(1) + dy;
        put(buf, x, y, text, usize::from(area.width), Style::default());
    }
}

/// Draws a blocking screen.
pub fn render_blocking(frame: &mut Frame, s: &BlockingScreen) {
    let area = frame.area();
    let buf = frame.buffer_mut();
    match s {
        BlockingScreen::TmuxMissing { found } => tmux_missing(buf, area, found.as_deref()),
    }
}

fn tmux_missing(buf: &mut Buffer, area: Rect, found: Option<&str>) {
    let r = Rect {
        x: area.x + centre(area.width, usize::from(TMUX_BOX_WIDTH)),
        y: area.y + area.height.saturating_sub(TMUX_BOX_HEIGHT) / 2,
        width: TMUX_BOX_WIDTH.min(area.width),
        height: TMUX_BOX_HEIGHT.min(area.height),
    };
    let plain = Style::default();
    draw_box(buf, r, "DuoHive needs tmux", &LIGHT, plain, plain);
    let found = format!(
        "  tmux 3.2 or newer is required. Found: {}.",
        found.unwrap_or("none")
    );
    let body = [
        "",
        found.as_str(),
        "",
        "    macOS   brew install tmux",
        "    Ubuntu  sudo apt install tmux",
        "",
        "  Agents run in tmux, so they keep working when DuoHive closes.",
        "                                                  [ q Quit ]",
    ];
    let room = usize::from(r.width.saturating_sub(2));
    for (i, line) in body.iter().enumerate() {
        let y = r.y + 1 + u16::try_from(i).unwrap_or(u16::MAX);
        if y + 1 < r.bottom() {
            put(buf, r.x + 1, y, line, room, plain);
        }
    }
}

/// The offset that centres `cells` in `total`.
fn centre(total: u16, cells: usize) -> u16 {
    let cells = u16::try_from(cells).unwrap_or(u16::MAX);
    total.saturating_sub(cells) / 2
}
