//! The phase-2 chat pane (P2-D8): `⬢ Duo · <state>`, the history, and the input.

use duohive_core::time::Millis;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use super::{At, RenderCtx, draw_box, pane_style, put_pieces};
use crate::chat::{ChatLine, Who};
use crate::focus::Pane;
use crate::format::{marker, pad_right, status_glyph, status_word, width, wrap_words};
use crate::model::UiModel;

const PROMPT: &str = "❯ ";
const YOU: &str = "you";
const MS_PER_MINUTE: u64 = 60_000;
const MINUTES_PER_DAY: u64 = 1_440;
/// Blank cells after the speaker's name: 3, or 2 when compact (UI §4.1's mocks).
const WIDE_GAP: usize = 3;
const COMPACT_GAP: usize = 2;

/// Draws the chat pane in `r`; returns where the cursor goes while the input has focus.
pub(super) fn draw(
    buf: &mut Buffer,
    r: Rect,
    m: &UiModel,
    ctx: &RenderCtx,
    compact: bool,
) -> Option<Position> {
    let p = &ctx.palette;
    let focused = m.focus.pane == Pane::Chat;
    let duo = &m.org.duo.name;
    let status = &m.view(duo).status;
    let marker = Some(marker(status)).filter(|c| *c != ' ');
    let title = format!(
        "⬢ {duo} · {}{} {}",
        marker.map(String::from).unwrap_or_default(),
        status_glyph(status, ctx.spinner()),
        status_word(status)
    );
    let (lines, border, inside) = pane_style(p, focused);
    draw_box(buf, r, &title, lines, border, inside);
    if r.width < 6 || r.height < 3 {
        return None;
    }
    // One padding cell inside each border (UI §3).
    let x = r.x + 2;
    let cells = usize::from(r.width - 4);
    let rows = usize::from(r.height - 2);
    let gap = if compact { COMPACT_GAP } else { WIDE_GAP };
    let who_cells = width(duo.as_str()).max(width(YOU)) + gap;
    let history = history_rows(m, &m.chat.lines, cells, who_cells, ctx);
    let room = rows - 1;
    for (i, pieces) in history
        .iter()
        .skip(history.len().saturating_sub(room))
        .enumerate()
    {
        let y = r.y + 1 + u16::try_from(i).unwrap_or(u16::MAX);
        put_pieces(buf, At { x, y, room: cells }, pieces.iter().cloned());
    }
    let y = r.bottom() - 2;
    let (shown, col) = m.chat_input.window(cells.saturating_sub(width(PROMPT)));
    let prompt = [(PROMPT.to_owned(), p.fg_muted), (shown, p.fg)];
    put_pieces(buf, At { x, y, room: cells }, prompt);
    let col = u16::try_from(width(PROMPT) + col).unwrap_or(u16::MAX);
    focused.then_some(Position { x: x + col, y })
}

/// Every history line as screen rows: `10:05 Duo   text`, wrapped under the text.
fn history_rows(
    m: &UiModel,
    lines: &[ChatLine],
    cells: usize,
    who_cells: usize,
    ctx: &RenderCtx,
) -> Vec<Vec<(String, Color)>> {
    let p = &ctx.palette;
    lines
        .iter()
        .flat_map(|line| {
            let (who, color) = match line.who {
                Who::You => (YOU.to_owned(), p.fg_muted),
                Who::Duo => (m.org.duo.name.to_string(), p.accent_duo),
            };
            let head = format!("{} ", clock(line.at));
            let indent = width(&head) + who_cells;
            let text_cells = cells.saturating_sub(indent).max(1);
            let wrapped: Vec<String> = line
                .text
                .split('\n')
                .flat_map(|para| wrap_words(para, text_cells))
                .collect();
            wrapped
                .into_iter()
                .enumerate()
                .map(|(i, text)| {
                    if i == 0 {
                        vec![
                            (head.clone(), p.fg_muted),
                            (pad_right(&who, who_cells), color),
                            (text, p.fg),
                        ]
                    } else {
                        vec![(" ".repeat(indent), p.fg), (text, p.fg)]
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// `10:05`: the time of day, in UTC.
fn clock(at: Millis) -> String {
    let minutes = (at.0 / MS_PER_MINUTE) % MINUTES_PER_DAY;
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}
