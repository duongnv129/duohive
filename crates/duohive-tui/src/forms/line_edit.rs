//! One editable line of text; the cursor moves, deletes and measures by grapheme.

use unicode_segmentation::UnicodeSegmentation;

use crate::format::width;

/// How a newline shows inside a one-line box.
const NEWLINE_GLYPH: &str = "↵";

/// Text and a cursor, counted in graphemes (`0..=len`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineEdit {
    text: String,
    cursor: usize,
}

impl LineEdit {
    /// `text` with the cursor at its end.
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            cursor: text.graphemes(true).count(),
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
        if self.cursor == 0 {
            return;
        }
        let (from, to) = (self.byte_at(self.cursor - 1), self.byte_at(self.cursor));
        self.text.replace_range(from..to, "");
        self.cursor -= 1;
    }

    /// Deletes the grapheme under the cursor.
    pub fn delete(&mut self) {
        if self.cursor >= self.len() {
            return;
        }
        let (from, to) = (self.byte_at(self.cursor), self.byte_at(self.cursor + 1));
        self.text.replace_range(from..to, "");
    }

    /// One grapheme left.
    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// One grapheme right.
    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.len());
    }

    /// To the start.
    pub fn home(&mut self) {
        self.cursor = 0;
    }

    /// To the end.
    pub fn end(&mut self) {
        self.cursor = self.len();
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
}
