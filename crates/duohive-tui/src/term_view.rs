//! The terminal widget: draws an agent's live terminal inside a pane.

use duohive_core::view::{PaneChunk, PaneSize};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

#[cfg(test)]
mod tests;

/// A terminal emulator whose screen renders into a Ratatui buffer.
pub struct TermView {
    parser: vt100::Parser,
}

impl TermView {
    /// An empty terminal of `size`.
    pub fn new(size: PaneSize) -> Self {
        Self {
            parser: vt100::Parser::new(size.rows, size.cols, 0),
        }
    }

    /// Feeds one chunk from the pane stream.
    ///
    /// A `Reset` starts from a clean parser (modes, alternate screen and
    /// attributes included), replays its bytes, then puts the cursor at the
    /// reported position.
    pub fn apply(&mut self, chunk: &PaneChunk) {
        match chunk {
            PaneChunk::Reset { bytes, cursor } => {
                let (rows, cols) = self.parser.screen().size();
                self.parser = vt100::Parser::new(rows, cols, 0);
                self.parser.process(bytes);
                self.move_cursor(cursor.0, cursor.1);
            }
            PaneChunk::Bytes(bytes) => self.parser.process(bytes),
            PaneChunk::Closed { .. } => {}
        }
    }

    /// Resizes the terminal. Content scrolls up only as far as needed to
    /// keep the cursor row visible, so the bottom lines stay.
    ///
    /// When rows must go, the screen is rebuilt in a fresh parser from the
    /// kept rows, so a scroll region or the alternate screen cannot corrupt
    /// the result. The hidden primary screen and other modes (mouse, bracketed
    /// paste) are not carried over;
    /// a `Reset` follows a real resize.
    pub fn resize(&mut self, size: PaneSize) {
        let (cols, rows) = (size.cols.max(1), size.rows.max(1));
        let screen = self.parser.screen();
        let (old_rows, old_cols) = screen.size();
        let (cursor_row, cursor_col) = screen.cursor_position();
        let drop = (cursor_row + 1).saturating_sub(rows);
        if drop == 0 {
            self.parser.screen_mut().set_size(rows, cols);
            return;
        }
        let mut seed: Vec<u8> = Vec::new();
        if screen.alternate_screen() {
            seed.extend_from_slice(b"\x1b[?1049h");
        }
        let width = old_cols.min(cols);
        for (i, row) in screen.rows_formatted(0, width).enumerate() {
            let i = u16::try_from(i).unwrap_or(u16::MAX);
            if i < drop || i >= old_rows.min(drop + rows) {
                continue;
            }
            // A wide glyph cut at the new edge would wrap into the next row.
            let splits_wide =
                width < old_cols && screen.cell(i, width - 1).is_some_and(vt100::Cell::is_wide);
            let row = if splits_wide {
                screen
                    .rows_formatted(0, width - 1)
                    .nth(usize::from(i))
                    .unwrap_or(row)
            } else {
                row
            };
            seed.extend_from_slice(format!("\x1b[{};1H", i - drop + 1).as_bytes());
            seed.extend_from_slice(&row);
            seed.extend_from_slice(b"\x1b[m");
        }
        if screen.hide_cursor() {
            seed.extend_from_slice(b"\x1b[?25l");
        }
        let mut next = vt100::Parser::new(rows, cols, 0);
        next.process(&seed);
        self.parser = next;
        self.move_cursor(cursor_col.min(cols - 1), cursor_row - drop);
    }

    /// Draws the screen into `area`, clipped to the terminal's size.
    ///
    /// With `show_cursor`, the cursor cell is drawn inverted unless the
    /// program hid the cursor. With `no_color`, colours are dropped and
    /// inverse is kept.
    pub fn render(&self, area: Rect, buf: &mut Buffer, show_cursor: bool, no_color: bool) {
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let area = area.intersection(buf.area);
        let (cursor_row, cursor_col) = screen.cursor_position();
        let draw_cursor = show_cursor && !screen.hide_cursor();
        let (draw_rows, draw_cols) = (area.height.min(rows), area.width.min(cols));
        for dy in 0..draw_rows {
            for dx in 0..draw_cols {
                let Some(cell) = screen.cell(dy, dx) else {
                    continue;
                };
                let mut style = cell_style(cell, no_color);
                if draw_cursor && dy == cursor_row && dx == cursor_col {
                    style = style.add_modifier(Modifier::REVERSED);
                    if cell.inverse() {
                        style = style.remove_modifier(Modifier::REVERSED);
                    }
                }
                let target = &mut buf[(area.x + dx, area.y + dy)];
                target.reset();
                // A wide glyph whose second half is clipped would overflow.
                let clipped = cell.is_wide() && dx + 1 >= draw_cols;
                let text = if clipped || cell.is_wide_continuation() || !cell.has_contents() {
                    " "
                } else {
                    cell.contents()
                };
                target.set_symbol(text);
                target.set_style(style);
            }
        }
    }

    fn move_cursor(&mut self, col: u16, row: u16) {
        let seq = format!("\x1b[{};{}H", row.saturating_add(1), col.saturating_add(1));
        self.parser.process(seq.as_bytes());
    }
}

fn cell_style(cell: &vt100::Cell, no_color: bool) -> Style {
    let mut style = Style::default();
    if !no_color {
        style = style
            .fg(map_color(cell.fgcolor()))
            .bg(map_color(cell.bgcolor()));
    }
    for (on, modifier) in [
        (cell.bold(), Modifier::BOLD),
        (cell.dim(), Modifier::DIM),
        (cell.italic(), Modifier::ITALIC),
        (cell.underline(), Modifier::UNDERLINED),
        (cell.inverse(), Modifier::REVERSED),
    ] {
        if on {
            style = style.add_modifier(modifier);
        }
    }
    style
}

const fn map_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
