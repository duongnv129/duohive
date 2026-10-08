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

    /// Resizes the terminal. Shrinking drops the top lines, so the bottom
    /// ones stay.
    pub fn resize(&mut self, size: PaneSize) {
        let size = PaneSize {
            cols: size.cols.max(1),
            rows: size.rows.max(1),
        };
        let (rows, _) = self.parser.screen().size();
        if size.rows < rows {
            let drop = rows - size.rows;
            let (row, col) = self.parser.screen().cursor_position();
            self.parser.process(format!("\x1b[{drop}S").as_bytes());
            self.move_cursor(col, row.saturating_sub(drop));
        }
        self.parser.screen_mut().set_size(size.rows, size.cols);
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
        for dy in 0..area.height.min(rows) {
            for dx in 0..area.width.min(cols) {
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
                let text = if cell.is_wide_continuation() || !cell.has_contents() {
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
