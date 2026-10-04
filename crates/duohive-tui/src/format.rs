//! Text formatting: ages, state cells and truncation (UI §2.1, §2.4).

use std::time::Duration;

use duohive_core::status::{AgentState, Status};
use duohive_core::time::Millis;
use ratatui::text::Span;

/// The working spinner's frames, 4 a second (UI §2.1).
pub const SPINNER: [char; 4] = ['◐', '◓', '◑', '◒'];
/// The quiet overlay's glyph.
pub const QUIET_GLYPH: char = '◷';
/// The state word is padded to this many cells (D23).
const WORD_CELLS: usize = 9;
/// The age is right-aligned in this many cells.
const AGE_CELLS: usize = 4;
const ELLIPSIS: char = '…';
/// Separates a message from its hint.
const SEP: &str = " · ";

/// Display width of `s` in terminal cells.
pub fn width(s: &str) -> usize {
    Span::raw(s).width()
}

fn char_width(c: char) -> usize {
    width(c.encode_utf8(&mut [0; 4]))
}

/// `s` padded with spaces to `cells` (never cut).
pub fn pad_right(s: &str, cells: usize) -> String {
    let w = width(s);
    format!("{s}{}", " ".repeat(cells.saturating_sub(w)))
}

/// A short age: `<1m`, `2m`, `4h`, `3d` (D13).
pub fn format_age(d: Duration) -> String {
    let m = d.as_secs() / 60;
    match m {
        0 => "<1m".to_owned(),
        1..60 => format!("{m}m"),
        60..1440 => format!("{}h", m / 60),
        _ => format!("{}d", m / 1440),
    }
}

/// How long the shown state has lasted: for quiet, since the silence began (D13).
/// `None` when the start is unknown (an agent the UI has not heard about yet).
pub fn age_of(s: &Status, now: Millis) -> Option<Duration> {
    let since = if s.quiet {
        s.quiet_since.unwrap_or(s.since)
    } else {
        s.since
    };
    (since > Millis(0)).then(|| now - since)
}

/// The glyph for a status: quiet's `◷`, a spinner frame on a working row, else the state's own.
pub fn status_glyph(s: &Status, spinner: Option<usize>) -> char {
    match (s.quiet, s.state, spinner) {
        (true, _, _) => QUIET_GLYPH,
        (false, AgentState::Working, Some(i)) => SPINNER[i % SPINNER.len()],
        (false, state, _) => state.glyph(),
    }
}

/// The word for a status: `quiet` replaces the state's word.
pub fn status_word(s: &Status) -> &'static str {
    if s.quiet { "quiet" } else { s.state.word() }
}

fn marker(s: &Status) -> char {
    if s.from_screen { '~' } else { ' ' }
}

fn age_cell(s: &Status, now: Millis) -> String {
    let age = age_of(s, now).map(format_age).unwrap_or_default();
    format!("{age:>AGE_CELLS$}")
}

/// The 16-cell state cell: `~` or a space, glyph, space, word padded to 9, age in 4.
pub fn state_cell(s: &Status, now: Millis, spinner: Option<usize>) -> String {
    format!(
        "{}{} {:<WORD_CELLS$}{}",
        marker(s),
        status_glyph(s, spinner),
        status_word(s),
        age_cell(s, now)
    )
}

/// The compact layout's 7-cell state cell: marker, glyph, space, age in 4.
pub fn compact_cell(s: &Status, now: Millis, spinner: Option<usize>) -> String {
    format!(
        "{}{} {}",
        marker(s),
        status_glyph(s, spinner),
        age_cell(s, now)
    )
}

/// Cuts `name` in the middle with `…` to fit `cells` (`billing-…-worker`).
pub fn truncate_middle(name: &str, cells: usize) -> String {
    if width(name) <= cells {
        return name.to_owned();
    }
    if cells == 0 {
        return String::new();
    }
    let keep = cells - 1;
    let head = take_cells(name.chars(), keep.div_ceil(2));
    let tail: String = take_cells(name.chars().rev(), keep / 2)
        .chars()
        .rev()
        .collect();
    format!("{head}{ELLIPSIS}{tail}")
}

/// Cuts `text` at the end with `…` to fit `cells`.
pub fn truncate_end(text: &str, cells: usize) -> String {
    if width(text) <= cells {
        return text.to_owned();
    }
    if cells == 0 {
        return String::new();
    }
    format!(
        "{}{ELLIPSIS}",
        take_cells(text.chars(), cells - 1).trim_end()
    )
}

/// `text` on one line, in quotes, cut before the closing quote to fit `cells`.
pub fn quoted(text: &str, cells: usize) -> String {
    let inner = truncate_end(&one_line(text), cells.saturating_sub(2));
    format!("\"{inner}\"")
}

/// Collapses newlines and runs of whitespace to single spaces.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Splits `text` into lines of at most `cells`, breaking after a ` · ` separator where it can
/// (so a hint such as `s stop` stays whole), else between words; a word longer than a line is
/// cut with `…`.
pub fn wrap_words(text: &str, cells: usize) -> Vec<String> {
    let segments: Vec<&str> = text.split(SEP).collect();
    let last = segments.len().saturating_sub(1);
    let units = segments.iter().enumerate().flat_map(|(i, seg)| {
        let unit = if i < last {
            format!("{seg} ·")
        } else {
            (*seg).to_owned()
        };
        if width(&unit) <= cells {
            vec![unit]
        } else {
            unit.split_whitespace().map(str::to_owned).collect()
        }
    });
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for unit in units {
        let joined = if line.is_empty() {
            unit.clone()
        } else {
            format!("{line} {unit}")
        };
        if width(&joined) <= cells || line.is_empty() {
            line = joined;
        } else {
            lines.push(std::mem::replace(&mut line, unit));
        }
    }
    lines.push(line);
    lines.into_iter().map(|l| truncate_end(&l, cells)).collect()
}

fn take_cells(chars: impl Iterator<Item = char>, cells: usize) -> String {
    let mut used = 0;
    chars
        .take_while(|c| {
            used += char_width(*c);
            used <= cells
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use duohive_core::status::{AgentState, Status};

    use super::*;
    use crate::fixtures::{NOW, mins, quiet, status};

    #[test]
    fn state_cells() {
        let working = status(AgentState::Working, mins(2));
        assert_eq!(state_cell(&working, NOW, None), " ● working    2m");
        let screen = Status {
            from_screen: true,
            ..status(AgentState::Working, mins(1))
        };
        assert_eq!(state_cell(&screen, NOW, None), "~● working    1m");
        assert_eq!(state_cell(&quiet(mins(6)), NOW, None), " ◷ quiet      6m");
        assert_eq!(state_cell(&working, NOW, Some(2)), " ◑ working    2m");
        assert_eq!(
            state_cell(&quiet(mins(6)), NOW, Some(2)),
            " ◷ quiet      6m"
        );
        let stopped = status(AgentState::Stopped, mins(24 * 60));
        assert_eq!(state_cell(&stopped, NOW, Some(1)), " ○ stopped    1d");
        assert_eq!(state_cell(&working, NOW, None).chars().count(), 16);
    }

    #[test]
    fn ages() {
        let secs = Duration::from_secs;
        assert_eq!(format_age(secs(0)), "<1m");
        assert_eq!(format_age(secs(59)), "<1m");
        assert_eq!(format_age(secs(60)), "1m");
        assert_eq!(format_age(mins(59)), "59m");
        assert_eq!(format_age(mins(4 * 60)), "4h");
        assert_eq!(format_age(mins(24 * 60 - 1)), "23h");
        assert_eq!(format_age(secs(3 * 86_400)), "3d");
    }

    #[test]
    fn names_truncate_in_the_middle() {
        assert_eq!(
            truncate_middle("billing-invoice-worker", 16),
            "billing-…-worker"
        );
        assert_eq!(truncate_middle("auth-service", 16), "auth-service");
        assert_eq!(truncate_middle("abcdef", 1), "…");
        assert_eq!(truncate_middle("abcdef", 0), "");
    }

    #[test]
    fn words_wrap_between_words() {
        assert_eq!(
            wrap_words("exited unexpectedly (code 1) · s stop", 30),
            ["exited unexpectedly (code 1) ·", "s stop"]
        );
        assert_eq!(
            wrap_words("✕ ~/code/billing not found · e to fix", 36),
            ["✕ ~/code/billing not found ·", "e to fix"]
        );
        assert_eq!(wrap_words("short", 30), ["short"]);
        assert_eq!(wrap_words("abcdefgh ij", 4), ["abc…", "ij"]);
        assert_eq!(wrap_words("", 4), [""]);
    }

    #[test]
    fn end_truncation_keeps_the_closing_quote() {
        assert_eq!(quoted("Allow edit to api.go?", 16), "\"Allow edit to…\"");
        assert_eq!(quoted("ok", 16), "\"ok\"");
        assert_eq!(quoted("two\n  lines", 16), "\"two lines\"");
        assert_eq!(truncate_end("abcdef", 4), "abc…");
        assert_eq!(truncate_end("abc", 4), "abc");
        assert_eq!(truncate_end("ab cdef", 4), "ab…");
    }
}
