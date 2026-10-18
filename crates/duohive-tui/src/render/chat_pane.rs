//! The chat pane (UI §4.6, §4.7): `⬢ Duo · <state>`, the history, the input, and the
//! suggestion list above the input.

use std::collections::BTreeSet;

use duohive_core::card::{Card, CardId};
use duohive_core::chat::{ChatEntry, ChatKind};
use duohive_core::inbox::MessageId;
use duohive_core::time::Millis;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

use super::{At, LIGHT, RenderCtx, draw_box, fg, focus_title, pane_style, put, put_pieces};
use crate::focus::Pane;
use crate::format::{
    marker, pad_right, state_cell, status_glyph, status_word, truncate_middle, width, wrap_words,
};
use crate::model::UiModel;
use crate::suggest::SuggestKind;
use crate::theme::Palette;

const PROMPT: &str = "❯ ";
const YOU: &str = "you";
const ARROW: &str = "→ ";
/// Between an action line's text and its result (P3-D14).
const RESULT_GAP: &str = "   ";
const MS_PER_MINUTE: u64 = 60_000;
const MINUTES_PER_DAY: i64 = 1_440;
/// Blank cells after the speaker's name: 3, or 2 when compact (UI §4.1's mocks).
const WIDE_GAP: usize = 3;
const COMPACT_GAP: usize = 2;
/// `10:05 `: six cells whatever the offset.
const CLOCK_CELLS: usize = 6;
/// The suggestion list (UI §4.7): at most 6 rows, 44 cells wide, 3 cells in.
const SUGGEST_ROWS: usize = 6;
const SUGGEST_WIDTH: u16 = 44;
const SUGGEST_INDENT: u16 = 4;
/// An agent item's name column, and a command item's.
const AGENT_CELLS: usize = 18;
const COMMAND_CELLS: usize = 10;

/// One screen row of history: styled pieces.
type Row = super::card::Row;

/// What the history shows, in order: the engine's entries, and each pending card at
/// its place (UI §4.6), after the entries up to when it was raised.
#[derive(Clone, Copy)]
enum Item<'a> {
    Entry(&'a ChatEntry),
    Card(&'a Card),
}

fn items(m: &UiModel) -> Vec<Item<'_>> {
    let mut cards: Vec<&Card> = m.cards.values().collect();
    cards.sort_by_key(|c| (c.raised_at, c.id));
    let mut cards = cards.into_iter().peekable();
    let mut out = Vec::with_capacity(m.chat.entries.len() + m.cards.len());
    for e in &m.chat.entries {
        while let Some(c) = cards.next_if(|c| c.raised_at < e.at) {
            out.push(Item::Card(c));
        }
        out.push(Item::Entry(e));
    }
    out.extend(cards.map(Item::Card));
    out
}

/// How the history fits a chat pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HistoryFit {
    /// Rows the whole history takes.
    pub(crate) len: usize,
    /// Rows the pane shows.
    pub(crate) room: usize,
    /// The first row shown while following the newest: the bottom, or the newest entry's
    /// first row when that entry is taller than the room (friction entry 13).
    pub(crate) follow: usize,
}

/// What the rows depend on besides the entries.
struct RowCtx<'a> {
    m: &'a UiModel,
    p: &'a Palette,
    offset_secs: i32,
    cells: usize,
    who_cells: usize,
    queued: &'a BTreeSet<MessageId>,
    now: Millis,
}

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
        "⬢ {duo} · {}{} {}{}",
        marker.map(String::from).unwrap_or_default(),
        status_glyph(status, ctx.spinner()),
        status_word(status),
        if m.chat.scroll.is_some() {
            " · scrolled"
        } else {
            ""
        }
    );
    let (lines, border, inside) = pane_style(p, focused);
    draw_box(buf, r, &title, lines, border, inside);
    if focused {
        focus_title(buf, r, &title, p);
    }
    let fit = history_fit(r, m, compact)?;
    let x = r.x + 2;
    let cells = usize::from(r.width - 4);
    let rc = row_ctx(m, p, ctx.utc_offset_secs, cells, compact, ctx.now);
    let history = chat_rows(m, &rc);
    let bottom = fit.len.saturating_sub(fit.room);
    let top = m.chat.scroll.map_or(fit.follow, |t| t.min(bottom));
    for (i, pieces) in history.iter().skip(top).take(fit.room).enumerate() {
        let y = r.y + 1 + u16::try_from(i).unwrap_or(u16::MAX);
        put_pieces(buf, At { x, y, room: cells }, pieces.iter().cloned());
    }
    let y = r.bottom() - 2;
    let (shown, col) = m.chat_input.window(cells.saturating_sub(width(PROMPT)));
    let prompt = [(PROMPT.to_owned(), p.fg_muted), (shown, p.fg)];
    put_pieces(buf, At { x, y, room: cells }, prompt);
    if focused {
        draw_suggest(buf, r, y, m, ctx);
    }
    let col = u16::try_from(width(PROMPT) + col).unwrap_or(u16::MAX);
    focused.then_some(Position { x: x + col, y })
}

/// How the history fits a chat pane at `r`.
pub(super) fn history_fit(r: Rect, m: &UiModel, compact: bool) -> Option<HistoryFit> {
    if r.width < 6 || r.height < 3 {
        return None;
    }
    let cells = usize::from(r.width - 4);
    let room = usize::from(r.height - 2) - 1;
    // Row counts don't depend on colours or the time zone.
    let p = Palette::dark();
    let rc = row_ctx(m, &p, 0, cells, compact, m.now);
    let per_entry: Vec<usize> = items(m)
        .into_iter()
        .map(|i| item_rows(i, &rc).len())
        .collect();
    let len: usize = per_entry.iter().sum();
    let bottom = len.saturating_sub(room);
    let newest = len - per_entry.last().copied().unwrap_or(0);
    Some(HistoryFit {
        len,
        room,
        follow: bottom.min(newest),
    })
}

fn row_ctx<'a>(
    m: &'a UiModel,
    p: &'a Palette,
    offset_secs: i32,
    cells: usize,
    compact: bool,
    now: Millis,
) -> RowCtx<'a> {
    let gap = if compact { COMPACT_GAP } else { WIDE_GAP };
    RowCtx {
        m,
        p,
        offset_secs,
        cells,
        who_cells: width(m.org.duo.name.as_str()).max(width(YOU)) + gap,
        queued: &m.queued,
        now,
    }
}

/// Every entry and card as screen rows, wrapped with a hanging indent under the
/// message text.
fn chat_rows(m: &UiModel, rc: &RowCtx<'_>) -> Vec<Row> {
    items(m)
        .into_iter()
        .flat_map(|i| item_rows(i, rc))
        .collect()
}

fn item_rows(item: Item<'_>, rc: &RowCtx<'_>) -> Vec<Row> {
    match item {
        Item::Entry(e) => entry_rows(e, rc),
        Item::Card(c) => card_rows(c, rc),
    }
}

/// A pending card, indented like action lines (UI §4.6).
fn card_rows(c: &Card, rc: &RowCtx<'_>) -> Vec<Row> {
    let indent = card_indent(rc);
    let focused = rc.m.focus.pane == Pane::Chat && rc.m.focus.card == Some(c.id);
    let cells = rc.cells.saturating_sub(indent);
    super::card::card_rows(c, rc.m, rc.now, focused, cells, rc.p)
        .into_iter()
        .map(|row| {
            std::iter::once((" ".repeat(indent), rc.p.fg))
                .chain(row)
                .collect()
        })
        .collect()
}

/// Cards and action lines start under the message column.
fn card_indent(rc: &RowCtx<'_>) -> usize {
    CLOCK_CELLS + rc.who_cells
}

/// What a press at `at` hits in a chat pane at `r`: a card, and the button under it.
pub(super) fn card_at(
    r: Rect,
    m: &UiModel,
    compact: bool,
    at: Position,
) -> Option<(CardId, Option<char>)> {
    let fit = history_fit(r, m, compact)?;
    let first = r.y + 1;
    let room = u16::try_from(fit.room).unwrap_or(u16::MAX);
    if at.y < first || at.y >= first.saturating_add(room) {
        return None;
    }
    let bottom = fit.len.saturating_sub(fit.room);
    let top = m.chat.scroll.map_or(fit.follow, |t| t.min(bottom));
    let row = top + usize::from(at.y - first);
    let (card, start, len) = card_spans(r, m, compact)
        .into_iter()
        .find(|(_, start, len)| (*start..start + len).contains(&row))?;
    let p = Palette::dark();
    let rc = row_ctx(m, &p, 0, usize::from(r.width - 4), compact, m.now);
    let left = usize::from(r.x) + 2 + card_indent(&rc);
    let x = usize::from(at.x);
    let card_cells = rc
        .cells
        .saturating_sub(card_indent(&rc))
        .min(super::card::CARD_WIDTH);
    if x < left || x >= left + card_cells {
        return None;
    }
    let c = m.cards.get(&card)?;
    // The button row is the last but one.
    let button = (row + 2 == start + len)
        .then(|| x.checked_sub(left + super::card::text_offset()))
        .flatten()
        .and_then(|col| super::card::button_at(c, col));
    Some((card, button))
}

/// Each pending card's first history row and row count, at a chat pane at `r`.
pub(super) fn card_spans(r: Rect, m: &UiModel, compact: bool) -> Vec<(CardId, usize, usize)> {
    if r.width < 6 {
        return Vec::new();
    }
    let p = Palette::dark();
    let rc = row_ctx(m, &p, 0, usize::from(r.width - 4), compact, m.now);
    let mut start = 0;
    let mut out = Vec::new();
    for item in items(m) {
        let len = item_rows(item, &rc).len();
        if let Item::Card(c) = item {
            out.push((c.id, start, len));
        }
        start += len;
    }
    out
}

/// One entry's rows: `10:05 you   text` and `10:05 Duo   text`, wrapped under the text;
/// `→ text   result` and notes indented under the message column.
fn entry_rows(e: &ChatEntry, rc: &RowCtx<'_>) -> Vec<Row> {
    let p = rc.p;
    let indent = CLOCK_CELLS + rc.who_cells;
    let text_cells = rc.cells.saturating_sub(indent).max(1);
    match &e.kind {
        ChatKind::You { message } => {
            let tag = message
                .filter(|id| rc.queued.contains(id))
                .map(|_| queued_tag(rc.m));
            said(e, YOU, p.fg_muted, tag, rc)
        }
        ChatKind::Duo => {
            let duo = rc.m.org.duo.name.to_string();
            said(e, &duo, p.accent_duo, None, rc)
        }
        ChatKind::Action { result } => action(&e.text, Some(result), indent, text_cells, p),
        // `→ approved: stop squad-1   10:16` (UI §4.6).
        ChatKind::CardResult { .. } => {
            let when = clock(e.at, rc.offset_secs);
            action(&e.text, Some(&when), indent, text_cells, p)
        }
        ChatKind::Note => paragraphs(&e.text, text_cells)
            .into_iter()
            .map(|t| vec![(" ".repeat(indent), p.fg), (t, p.fg_muted)])
            .collect(),
    }
}

/// `queued (Duo is working)` (P3-D26).
fn queued_tag(m: &UiModel) -> String {
    let duo = &m.org.duo.name;
    format!("queued ({duo} is {})", m.view(duo).status.state.word())
}

/// A `you` or Duo line; `tag` goes right-aligned on its last row, or a row of its own.
fn said(e: &ChatEntry, who: &str, color: Color, tag: Option<String>, rc: &RowCtx<'_>) -> Vec<Row> {
    let p = rc.p;
    let head = format!("{} ", clock(e.at, rc.offset_secs));
    let indent = width(&head) + rc.who_cells;
    let text_cells = rc.cells.saturating_sub(indent).max(1);
    let mut rows: Vec<Row> = paragraphs(&e.text, text_cells)
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            if i == 0 {
                vec![
                    (head.clone(), p.fg_muted),
                    (pad_right(who, rc.who_cells), color),
                    (text, p.fg),
                ]
            } else {
                vec![(" ".repeat(indent), p.fg), (text, p.fg)]
            }
        })
        .collect();
    if let Some(tag) = tag {
        let used = rows
            .last()
            .map_or(0, |r| r.iter().map(|(t, _)| width(t)).sum());
        let free = rc.cells.saturating_sub(used);
        if free >= width(&tag) + RESULT_GAP.len()
            && let Some(last) = rows.last_mut()
        {
            last.push((" ".repeat(free - width(&tag)), p.fg));
            last.push((tag, p.fg_muted));
        } else {
            let pad = rc.cells.saturating_sub(width(&tag));
            rows.push(vec![(" ".repeat(pad), p.fg), (tag, p.fg_muted)]);
        }
    }
    rows
}

/// `→ text   result` under the message column; wrapped rows hang under the text, and
/// the result follows the last row, or takes a row of its own.
fn action(
    text: &str,
    result: Option<&String>,
    indent: usize,
    text_cells: usize,
    p: &Palette,
) -> Vec<Row> {
    let lead = " ".repeat(indent);
    let hang = " ".repeat(indent + width(ARROW));
    let cells = text_cells.saturating_sub(width(ARROW)).max(1);
    let mut rows: Vec<Row> = paragraphs(text, cells)
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            if i == 0 {
                vec![
                    (lead.clone(), p.fg),
                    (ARROW.to_owned(), p.fg_muted),
                    (t, p.fg),
                ]
            } else {
                vec![(hang.clone(), p.fg), (t, p.fg)]
            }
        })
        .collect();
    if let Some(result) = result.filter(|r| !r.is_empty()) {
        let used = rows.last().map_or(0, |r| {
            r.iter().skip(1).map(|(t, _)| width(t)).sum::<usize>()
        });
        let limit = if rows.len() == 1 {
            width(ARROW) + cells
        } else {
            cells
        };
        let fits = used + width(RESULT_GAP) + width(result) <= limit;
        match rows.last_mut() {
            Some(last) if fits => {
                last.push((format!("{RESULT_GAP}{result}"), p.fg_muted));
            }
            _ => rows.push(vec![(hang, p.fg), (result.clone(), p.fg_muted)]),
        }
    }
    rows
}

/// `text`'s lines, each wrapped to `cells`.
fn paragraphs(text: &str, cells: usize) -> Vec<String> {
    text.split('\n')
        .flat_map(|para| wrap_words(para, cells))
        .collect()
}

/// The suggestion list above the input row `input_y` (UI §4.7): at most 6 items around
/// the highlighted one, the highlighted row with `▌`.
fn draw_suggest(buf: &mut Buffer, r: Rect, input_y: u16, m: &UiModel, ctx: &RenderCtx) {
    let Some(s) = &m.suggest else {
        return;
    };
    let p = &ctx.palette;
    let shown = s.items.len().min(SUGGEST_ROWS);
    let first = s
        .cursor
        .saturating_sub(shown - 1)
        .min(s.items.len() - shown);
    let rows = u16::try_from(shown).unwrap_or(0) + 2;
    let w = SUGGEST_WIDTH.min(r.width.saturating_sub(SUGGEST_INDENT + 1));
    let Some(y) = input_y.checked_sub(rows) else {
        return;
    };
    if w < 4 || y < buf.area.y {
        return;
    }
    let area = Rect::new(r.x + SUGGEST_INDENT, y, w, rows);
    for row in area.rows() {
        put(
            buf,
            area.x,
            row.y,
            &" ".repeat(usize::from(w)),
            usize::from(w),
            fg(p.fg),
        );
    }
    draw_box(
        buf,
        area,
        "",
        &LIGHT,
        fg(p.border),
        Style::default().bg(p.bg),
    );
    let inner = usize::from(w - 2);
    for (i, (item, detail)) in s.items.iter().enumerate().skip(first).take(shown) {
        let y = area.y + 1 + u16::try_from(i - first).unwrap_or(0);
        let x = area.x + 1;
        let picked = i == s.cursor;
        if picked {
            put(buf, x, y, "▌", 1, fg(p.border_focus));
        }
        let style = |c: Color| {
            let s = fg(c);
            if picked {
                s.add_modifier(Modifier::BOLD)
            } else {
                s
            }
        };
        let (name, rest, rest_color) = match s.kind {
            SuggestKind::Agent => {
                let agent = item.trim_start_matches('@');
                let cell = m
                    .agent_names()
                    .into_iter()
                    .find(|a| a.as_str() == agent)
                    .map_or_else(
                        || detail.clone(),
                        |a| state_cell(&m.view(a).status, ctx.now, ctx.spinner()),
                    );
                let name = pad_right(&truncate_middle(item, AGENT_CELLS - 1), AGENT_CELLS);
                (name, cell, p.fg_muted)
            }
            SuggestKind::Command => (pad_right(item, COMMAND_CELLS), detail.clone(), p.fg_muted),
        };
        let x = put(buf, x + 1, y, &name, inner - 1, style(p.fg));
        let room = inner.saturating_sub(1 + width(&name));
        put(buf, x, y, &rest, room, style(rest_color));
    }
}

/// `10:05`: the local time of day, `offset_secs` east of UTC.
fn clock(at: Millis, offset_secs: i32) -> String {
    let utc = i64::try_from(at.0 / MS_PER_MINUTE).unwrap_or(0);
    let local = utc + i64::from(offset_secs / 60);
    let minutes = local.rem_euclid(MINUTES_PER_DAY);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}
