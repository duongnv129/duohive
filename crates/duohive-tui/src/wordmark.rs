//! The Manager card's wordmark font (UI §4.1.5): 38 glyphs, each three columns wide and two
//! rows high, drawn with half blocks: a `▀` is a lit top half, a `▄` a lit bottom half and a
//! `█` both.

/// How many characters have a glyph: A–Z, 0–9, `-` and `_`.
pub const GLYPH_COUNT: usize = 38;

/// The columns a wordmark always occupies: three glyphs and the blank column between them.
pub const WIDTH: usize = 11;

/// How many characters of a name the wordmark draws.
const LETTERS: usize = 3;

/// The font, in the chart's order, keyed by the upper-case character.
const FONT: [(char, [&str; 2]); GLYPH_COUNT] = [
    ('A', ["▄▀▄", "█▀█"]),
    ('B', ["██▄", "█▄▀"]),
    ('C', ["▄▀▀", "▀▄▄"]),
    ('D', ["█▀▄", "█▄▀"]),
    ('E', ["██▀", "█▄▄"]),
    ('F', ["██▀", "█  "]),
    ('G', ["▄▀▀", "▀▄█"]),
    ('H', ["█▄█", "█ █"]),
    ('I', ["▀█▀", "▄█▄"]),
    ('J', ["  █", "▀▄▀"]),
    ('K', ["█▄▀", "█ █"]),
    ('L', ["█  ", "█▄▄"]),
    ('M', ["█▄█", "█▀█"]),
    ('N', ["█▀▄", "█ █"]),
    ('O', ["▄▀▄", "▀▄▀"]),
    ('P', ["█▀▄", "█▀ "]),
    ('Q', ["▄▀▄", "▀▄█"]),
    ('R', ["█▀▄", "█▀▄"]),
    ('S', ["▄█▀", "▄▄▀"]),
    ('T', ["▀█▀", " █ "]),
    ('U', ["█ █", "█▄█"]),
    ('V', ["█ █", "▀▄▀"]),
    ('W', ["█ █", "███"]),
    ('X', ["▀▄▀", "▄▀▄"]),
    ('Y', ["█ █", " █ "]),
    ('Z', ["▀▀█", "█▄▄"]),
    ('0', ["█▀█", "█▄█"]),
    ('1', ["▄█ ", "▄█▄"]),
    ('2', ["▀▀▄", "▄█▄"]),
    ('3', ["▀█▄", "▄▄▀"]),
    ('4', ["█ █", "▀▀█"]),
    ('5', ["██▀", "▄▄▀"]),
    ('6', ["▄▀▀", "▀█▀"]),
    ('7', ["▀▀█", " █ "]),
    ('8', ["███", "█▄█"]),
    ('9', ["█▀█", "▀▀█"]),
    ('-', ["▄▄▄", "   "]),
    ('_', ["   ", "▄▄▄"]),
];

/// The two rows of `c`'s glyph (case does not matter), or `None` for a character outside the 38.
pub fn glyph(c: char) -> Option<[&'static str; 2]> {
    let upper = c.to_ascii_uppercase();
    FONT.iter().find(|(k, _)| *k == upper).map(|(_, g)| *g)
}

/// The first three characters of `name` as two rows of block letters, one blank column
/// between glyphs, padded on the right to 11 columns so the text beside it never moves.
/// Taken by `char`, never by byte, so no multi-byte character is cut.
pub fn wordmark(name: &str) -> [String; 2] {
    let mut rows = [String::new(), String::new()];
    for (i, c) in name.chars().take(LETTERS).enumerate() {
        let g = glyph(c).unwrap_or(["   ", "   "]);
        for (row, part) in rows.iter_mut().zip(g) {
            if i > 0 {
                row.push(' ');
            }
            row.push_str(part);
        }
    }
    for row in &mut rows {
        let have = row.chars().count();
        row.push_str(&" ".repeat(WIDTH.saturating_sub(have)));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chart pasted from UI §4.1.5 (its rows of glyphs and labels).
    const CHART: &str = "▄▀▄  ██▄  ▄▀▀  █▀▄  ██▀  ██▀  ▄▀▀  █▄█  ▀█▀    █  █▄▀  █    █▄█
█▀█  █▄▀  ▀▄▄  █▄▀  █▄▄  █    ▀▄█  █ █  ▄█▄  ▀▄▀  █ █  █▄▄  █▀█
 A    B    C    D    E    F    G    H    I    J    K    L    M

█▀▄  ▄▀▄  █▀▄  ▄▀▄  █▀▄  ▄█▀  ▀█▀  █ █  █ █  █ █  ▀▄▀  █ █  ▀▀█
█ █  ▀▄▀  █▀   ▀▄█  █▀▄  ▄▄▀   █   █▄█  ▀▄▀  ███  ▄▀▄   █   █▄▄
 N    O    P    Q    R    S    T    U    V    W    X    Y    Z

█▀█  ▄█   ▀▀▄  ▀█▄  █ █  ██▀  ▄▀▀  ▀▀█  ███  █▀█  ▄▄▄
█▄█  ▄█▄  ▄█▄  ▄▄▀  ▀▀█  ▄▄▀  ▀█▀   █   █▄█  ▀▀█       ▄▄▄
 0    1    2    3    4    5    6    7    8    9    -    _
";

    #[test]
    fn duo_draws_its_wordmark() {
        assert_eq!(wordmark("Duo"), ["█▀▄ █ █ ▄▀▄", "█▄▀ █▄█ ▀▄▀"]);
    }

    #[test]
    fn case_does_not_matter() {
        assert_eq!(wordmark("duo"), wordmark("DUO"));
        assert_eq!(glyph('q'), glyph('Q'));
    }

    #[test]
    fn a_short_name_is_padded_to_eleven_columns() {
        let a = wordmark("a");
        assert_eq!(a[0].chars().count(), 11);
        assert_eq!(a[1].chars().count(), 11);
        assert_eq!(a[0].trim_end(), "▄▀▄");
        for name in ["", "ab", "a-", "7-eleven", "platform-manager"] {
            for row in wordmark(name) {
                assert_eq!(row.chars().count(), 11, "{name}");
            }
        }
    }

    #[test]
    fn every_name_character_has_a_glyph() {
        let all: Vec<char> = ('A'..='Z')
            .chain('a'..='z')
            .chain('0'..='9')
            .chain(['-', '_'])
            .collect();
        for c in &all {
            assert!(glyph(*c).is_some(), "{c}");
        }
        let mut upper: Vec<_> = FONT.iter().map(|(k, _)| *k).collect();
        upper.sort_unstable();
        upper.dedup();
        assert_eq!(upper.len(), GLYPH_COUNT);
        assert_eq!(FONT.len(), 38);
        assert_eq!(glyph('é'), None);
        assert_eq!(glyph(' '), None);
        // Every glyph is three columns wide in both rows.
        for (k, g) in FONT {
            assert!(g.iter().all(|r| r.chars().count() == 3), "{k}");
        }
    }

    #[test]
    fn a_wide_character_is_never_cut_mid_byte() {
        // Not a valid name, but the font must not panic on one.
        let w = wordmark("ééé");
        assert_eq!(w[0].chars().count(), 11);
    }

    #[test]
    fn the_chart_matches_the_ui_spec() {
        let lines: Vec<&str> = CHART.lines().collect();
        let mut at = 0;
        for labels in ["ABCDEFGHIJKLM", "NOPQRSTUVWXYZ", "0123456789-_"] {
            let render = |row: usize| {
                labels
                    .chars()
                    .map(|c| glyph(c).map_or("   ", |g| g[row]))
                    .collect::<Vec<_>>()
                    .join("  ")
            };
            assert_eq!(render(0).trim_end(), lines[at].trim_end(), "{labels} top");
            assert_eq!(
                render(1).trim_end(),
                lines[at + 1].trim_end(),
                "{labels} bottom"
            );
            at += 4;
        }
    }
}
