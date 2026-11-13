//! One editable line of text; the cursor moves, deletes and measures by grapheme.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::format::width;

/// How a newline shows inside a one-line box.
const NEWLINE_GLYPH: &str = "↵";

/// Text and a cursor, counted in graphemes (`0..=len`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineEdit {
    text: String,
    cursor: usize,
    /// The display column `↑` and `↓` keep across rows; any other edit or move forgets it.
    want_col: Option<usize>,
}

impl LineEdit {
    /// `text` with the cursor at its end.
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            cursor: text.graphemes(true).count(),
            want_col: None,
        }
    }

    /// The text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The cursor, in graphemes from the start.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    fn len(&self) -> usize {
        self.text.graphemes(true).count()
    }

    /// The byte offset of grapheme `i` (the end for `i == len`).
    fn byte_at(&self, i: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .nth(i)
            .map_or(self.text.len(), |(b, _)| b)
    }

    /// Types `c` at the cursor.
    pub fn insert(&mut self, c: char) {
        let mut buf = [0u8; 4];
        self.insert_str(c.encode_utf8(&mut buf));
    }

    /// Types `s` at the cursor; the cursor ends after it.
    pub fn insert_str(&mut self, s: &str) {
        self.want_col = None;
        let at = self.byte_at(self.cursor);
        self.text.insert_str(at, s);
        // The insert may join graphemes on either side (a combining mark, a ZWJ), so the
        // cursor goes after the grapheme holding the inserted text's last byte.
        let end = at + s.len();
        self.cursor = self
            .text
            .grapheme_indices(true)
            .take_while(|(b, _)| *b < end)
            .count();
    }

    /// Deletes the grapheme before the cursor.
    pub fn backspace(&mut self) {
        self.want_col = None;
        if self.cursor == 0 {
            return;
        }
        let (from, to) = (self.byte_at(self.cursor - 1), self.byte_at(self.cursor));
        self.text.replace_range(from..to, "");
        self.cursor -= 1;
    }

    /// Deletes the grapheme under the cursor.
    pub fn delete(&mut self) {
        self.want_col = None;
        if self.cursor >= self.len() {
            return;
        }
        let (from, to) = (self.byte_at(self.cursor), self.byte_at(self.cursor + 1));
        self.text.replace_range(from..to, "");
    }

    /// One grapheme left.
    pub fn left(&mut self) {
        self.want_col = None;
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// One grapheme right.
    pub fn right(&mut self) {
        self.want_col = None;
        self.cursor = (self.cursor + 1).min(self.len());
    }

    /// To the start.
    pub fn home(&mut self) {
        self.want_col = None;
        self.cursor = 0;
    }

    /// To the end.
    pub fn end(&mut self) {
        self.want_col = None;
        self.cursor = self.len();
    }

    /// The rows the text takes in a box `cells` wide, as grapheme ranges: a row ends after a
    /// `\n` (which it keeps), else after the last space that fits, else where the next
    /// grapheme would not fit; a wide grapheme never splits and every row holds at least one.
    /// Text ending in `\n`, and empty text, end in an empty row.
    pub fn wrap(&self, cells: usize) -> Vec<Range<usize>> {
        let g: Vec<&str> = self.text.graphemes(true).collect();
        let mut rows = Vec::new();
        let mut start = 0;
        while start < g.len() {
            let end = row_end(&g, start, cells);
            rows.push(start..end);
            start = end;
        }
        if g.last().is_none_or(|last| *last == "\n") {
            rows.push(g.len()..g.len());
        }
        rows
    }

    /// Moves the cursor to the row above, at the remembered column; `false` (no move) on
    /// the first row.
    pub fn line_up(&mut self, cells: usize) -> bool {
        self.line_move(cells, false)
    }

    /// Moves the cursor to the row below, at the remembered column; `false` (no move) on
    /// the last row.
    pub fn line_down(&mut self, cells: usize) -> bool {
        self.line_move(cells, true)
    }

    fn line_move(&mut self, cells: usize, down: bool) -> bool {
        let g: Vec<&str> = self.text.graphemes(true).collect();
        let rows = self.wrap(cells);
        let here = row_of(&rows, self.cursor);
        let target = if down {
            here + 1
        } else {
            match here.checked_sub(1) {
                Some(t) => t,
                None => return false,
            }
        };
        let Some(row) = rows.get(target) else {
            return false;
        };
        let want = self
            .want_col
            .unwrap_or_else(|| columns(&g[rows[here].start..self.cursor]));
        // The cursor can't rest after a row's last grapheme unless it is the text's end:
        // that spot is the next row's start.
        let last = if target + 1 == rows.len() {
            row.end
        } else {
            row.end - 1
        };
        let (mut at, mut used) = (row.start, 0);
        while at < last && used + columns(&g[at..=at]) <= want {
            used += columns(&g[at..=at]);
            at += 1;
        }
        self.cursor = at;
        self.want_col = Some(want);
        true
    }

    /// At most `rows` rows of the wrapped text, scrolled to keep the cursor's row in view,
    /// without newline marks, and the cursor's `(row, column)` in them.
    pub fn window_rows(&self, cells: usize, rows: usize) -> (Vec<String>, (usize, usize)) {
        let g: Vec<&str> = self.text.graphemes(true).collect();
        let all = self.wrap(cells);
        let here = row_of(&all, self.cursor);
        let top = (here + 1).saturating_sub(rows.max(1));
        let shown = all
            .iter()
            .skip(top)
            .take(rows)
            .map(|r| {
                g[r.clone()]
                    .iter()
                    .copied()
                    .filter(|c| *c != "\n")
                    .collect()
            })
            .collect();
        let col = columns(&g[all[here].start..self.cursor]);
        (shown, (here - top, col))
    }

    /// What fits in `cells`, scrolled so the cursor's cell is in view, and the cursor's
    /// column in it. Newlines show as `↵`.
    pub fn window(&self, cells: usize) -> (String, usize) {
        let shown: Vec<&str> = self
            .text
            .graphemes(true)
            .map(|g| if g == "\n" { NEWLINE_GLYPH } else { g })
            .collect();
        let widths: Vec<usize> = shown.iter().map(|g| width(g)).collect();
        let mut start = 0;
        while start < self.cursor && widths[start..self.cursor].iter().sum::<usize>() >= cells {
            start += 1;
        }
        let col = widths[start..self.cursor].iter().sum();
        let mut used = 0;
        let visible = shown[start..]
            .iter()
            .zip(&widths[start..])
            .take_while(|(_, w)| {
                used += *w;
                used <= cells
            })
            .map(|(g, _)| *g)
            .collect();
        (visible, col)
    }

    /// The first `cells` of the text, for a box that isn't focused. Newlines show
    /// as `↵`.
    pub fn head(&self, cells: usize) -> String {
        let mut used = 0;
        self.text
            .graphemes(true)
            .map(|g| if g == "\n" { NEWLINE_GLYPH } else { g })
            .take_while(|g| {
                used += width(g);
                used <= cells
            })
            .collect()
    }
}

/// Display columns of `graphemes`; a newline takes none.
fn columns(graphemes: &[&str]) -> usize {
    graphemes
        .iter()
        .filter(|g| **g != "\n")
        .map(|g| width(g))
        .sum()
}

/// Where the row starting at grapheme `start` ends (exclusive) in a box `cells` wide.
fn row_end(g: &[&str], start: usize, cells: usize) -> usize {
    let (mut used, mut last_space) = (0, None);
    for (i, c) in g.iter().enumerate().skip(start) {
        if *c == "\n" {
            return i + 1;
        }
        let w = width(c);
        if used + w > cells && i > start {
            return last_space.map_or(i, |s| s + 1);
        }
        if *c == " " {
            last_space = Some(i);
        }
        used += w;
    }
    g.len()
}

/// The row holding the cursor: the last one starting at or before it.
fn row_of(rows: &[Range<usize>], cursor: usize) -> usize {
    rows.iter().rposition(|r| r.start <= cursor).unwrap_or(0)
}

#[cfg(test)]
mod tests;
