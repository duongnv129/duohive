//! The resume picker (UI §4.13): an agent's past sessions, searchable, newest first.
//! Pure state; the keys are in `input/resume.rs` and the frame in `render/resume.rs`.

use duohive_core::names::AgentName;
use duohive_core::session_catalog::SessionInfo;

use crate::format::one_line;

/// A session written within this long ago may still be open in another terminal (5 minutes).
pub const RECENT_MS: u64 = 300_000;
/// Rows `PgUp` / `PgDn` move.
const PAGE_ROWS: usize = 5;
const SECS_PER_MINUTE: u64 = 60;
const SECS_PER_HOUR: u64 = 3_600;
const SECS_PER_DAY: u64 = 86_400;
const KB: u64 = 1024;
const MB: u64 = KB * 1024;
const GB: u64 = MB * 1024;
const ESC: char = '\u{1b}';
const BEL: char = '\u{7}';

/// The open picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumePicker {
    /// The agent whose sessions these are.
    pub agent: AgentName,
    /// Its CLI as the UI names it (`Claude Code`).
    pub cli_label: String,
    /// Its directory as the UI shows it.
    pub dir_label: String,
    /// Every session, newest first, as the engine sorted them.
    pub sessions: Vec<SessionInfo>,
    /// The search box.
    pub search: String,
    /// The highlighted row of [`Self::visible`].
    pub cursor: usize,
    /// While the confirmation is up: the row it asks about.
    pub confirm: Option<usize>,
}

impl ResumePicker {
    /// A picker over `sessions`, on the newest.
    pub fn new(
        agent: AgentName,
        cli_label: String,
        dir_label: String,
        sessions: Vec<SessionInfo>,
    ) -> Self {
        Self {
            agent,
            cli_label,
            dir_label,
            sessions,
            search: String::new(),
            cursor: 0,
            confirm: None,
        }
    }

    /// The sessions the search keeps, in the engine's order. A session with no branch
    /// matches on its title only.
    pub fn visible(&self) -> Vec<&SessionInfo> {
        let needle = self.search.to_lowercase();
        self.sessions
            .iter()
            .filter(|s| {
                needle.is_empty()
                    || s.title.to_lowercase().contains(&needle)
                    || s.branch
                        .as_deref()
                        .is_some_and(|b| b.to_lowercase().contains(&needle))
            })
            .collect()
    }

    /// `1 of 36`; `0 of 0` when nothing matches.
    pub fn count_label(&self) -> String {
        let total = self.visible().len();
        if total == 0 {
            return "0 of 0".to_owned();
        }
        format!("{} of {total}", self.cursor.min(total - 1) + 1)
    }

    /// The highlighted session.
    pub fn selected(&self) -> Option<&SessionInfo> {
        self.visible().get(self.cursor).copied()
    }

    /// Whether `s` was written so lately that it may still be open elsewhere. A timestamp
    /// in the future counts as recent.
    #[expect(
        clippy::unused_self,
        reason = "the picker owns the rule; the interface is a method"
    )]
    pub fn needs_confirm(&self, s: &SessionInfo, now_ms: u64) -> bool {
        now_ms.saturating_sub(s.updated_ms) < RECENT_MS
    }

    /// Moves the highlight one row, staying in the list.
    pub fn step(&mut self, down: bool) {
        self.cursor = if down {
            (self.cursor + 1).min(self.visible().len().saturating_sub(1))
        } else {
            self.cursor.saturating_sub(1)
        };
    }

    /// Moves the highlight a page.
    pub fn page(&mut self, down: bool) {
        let last = self.visible().len().saturating_sub(1);
        self.cursor = if down {
            (self.cursor + PAGE_ROWS).min(last)
        } else {
            self.cursor.saturating_sub(PAGE_ROWS)
        };
    }

    /// A character typed in the search box.
    pub fn type_char(&mut self, c: char) {
        if c.is_control() {
            return;
        }
        self.search.push(c);
        self.cursor = 0;
    }

    /// Backspace in the search box.
    pub fn backspace(&mut self) {
        self.search.pop();
        self.cursor = 0;
    }

    /// Empties the search box.
    pub fn clear_search(&mut self) {
        self.search.clear();
        self.cursor = 0;
    }
}

/// How long ago, from a span in milliseconds: `just now`, `3 minutes ago`, `1 day ago`.
pub fn age_text(span_ms: u64) -> String {
    let secs = span_ms / 1000;
    let (n, unit) = if secs < SECS_PER_MINUTE {
        return "just now".to_owned();
    } else if secs < SECS_PER_HOUR {
        (secs / SECS_PER_MINUTE, "minute")
    } else if secs < SECS_PER_DAY {
        (secs / SECS_PER_HOUR, "hour")
    } else {
        (secs / SECS_PER_DAY, "day")
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

/// A size on disk: `512B`, `610KB`, `4.2MB`, `1.3GB`.
pub fn size_text(bytes: u64) -> String {
    let tenths = |unit: u64| (bytes.saturating_mul(10) + unit / 2) / unit;
    if bytes < KB {
        format!("{bytes}B")
    } else if bytes < MB {
        format!("{}KB", bytes / KB)
    } else if bytes < GB {
        format!("{}.{}MB", tenths(MB) / 10, tenths(MB) % 10)
    } else {
        format!("{}.{}GB", tenths(GB) / 10, tenths(GB) % 10)
    }
}

/// `text` made safe to draw: escape sequences dropped whole, every other control
/// character a space, runs of white space one space. A session's title and branch come
/// from files on disk and must never move the cursor or colour the screen.
pub fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            ESC => skip_escape(&mut chars),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    one_line(&out)
}

/// Consumes the rest of the escape sequence that follows an ESC.
fn skip_escape(chars: &mut std::str::Chars<'_>) {
    match chars.next() {
        // CSI: parameters, then a final byte.
        Some('[') => {
            for c in chars.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&c) {
                    break;
                }
            }
        }
        // OSC: up to BEL or ST (ESC \).
        Some(']') => {
            while let Some(c) = chars.next() {
                if c == BEL {
                    break;
                }
                if c == ESC {
                    chars.next();
                    break;
                }
            }
        }
        // A two-character escape.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{mins, session, three_sessions};
    use duohive_core::testkit::n;

    fn picker(sessions: Vec<duohive_core::session_catalog::SessionInfo>) -> ResumePicker {
        ResumePicker::new(
            n("pay-qa"),
            "Claude Code".to_owned(),
            "/work/pay-qa".to_owned(),
            sessions,
        )
    }

    fn type_str(p: &mut ResumePicker, s: &str) {
        for c in s.chars() {
            p.type_char(c);
        }
    }

    #[test]
    fn search_filters_by_title_and_branch_and_updates_the_count() {
        let mut p = picker(three_sessions());
        assert_eq!(p.count_label(), "1 of 3");
        type_str(&mut p, "WEBHOOK");
        let titles: Vec<_> = p.visible().iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Flaky webhook test"]);
        assert_eq!(p.count_label(), "1 of 1");
        p.clear_search();
        // A branch match.
        type_str(&mut p, "main");
        assert_eq!(p.visible().len(), 1);
        p.clear_search();
        // Title and branch both match one entry once; two entries match "payout".
        type_str(&mut p, "payout");
        assert_eq!(p.count_label(), "1 of 2");
        p.clear_search();
        type_str(&mut p, "zzz");
        assert_eq!(p.count_label(), "0 of 0");
    }

    #[test]
    fn entry_without_a_branch_matches_on_title_only() {
        let mut s = session("Plain title", mins(5), None, 0);
        s.branch = None;
        let mut p = picker(vec![s]);
        type_str(&mut p, "plain");
        assert_eq!(p.visible().len(), 1);
        p.clear_search();
        type_str(&mut p, "none");
        assert!(p.visible().is_empty());
    }

    #[test]
    fn needs_confirm_is_true_within_five_minutes() {
        let p = picker(three_sessions());
        let s = &p.sessions[0];
        let at = s.updated_ms;
        assert!(p.needs_confirm(s, at + RECENT_MS - 1));
        assert!(!p.needs_confirm(s, at + RECENT_MS));
    }

    #[test]
    fn future_timestamp_counts_as_recent() {
        let p = picker(three_sessions());
        let s = &p.sessions[0];
        assert!(p.needs_confirm(s, s.updated_ms - 10_000));
        assert_eq!(age_text(0), "just now");
    }

    #[test]
    fn age_text_boundaries() {
        let cases = [
            (59, "just now"),
            (60, "1 minute ago"),
            (119, "1 minute ago"),
            (120, "2 minutes ago"),
            (3_599, "59 minutes ago"),
            (3_600, "1 hour ago"),
            (86_399, "23 hours ago"),
            (86_400, "1 day ago"),
            (172_800, "2 days ago"),
        ];
        for (secs, want) in cases {
            assert_eq!(age_text(secs * 1000), want, "{secs}s");
        }
    }

    #[test]
    fn sizes_read_like_the_mockup() {
        assert_eq!(size_text(512), "512B");
        assert_eq!(size_text(610 * 1024), "610KB");
        assert_eq!(size_text(4_400_000), "4.2MB");
    }

    #[test]
    fn clean_drops_escapes_and_controls() {
        assert_eq!(clean("a\u{1b}[31mred\u{1b}[0m\nb\r\tc\u{7}"), "ared b c");
        assert_eq!(clean("x\u{1b}]0;title\u{7}y"), "xy");
        assert_eq!(clean(""), "");
    }

    #[test]
    fn cursor_clamps_and_pages() {
        let mut p = picker(three_sessions());
        p.step(false);
        assert_eq!(p.cursor, 0);
        p.page(true);
        assert_eq!(p.cursor, 2);
        p.page(false);
        assert_eq!(p.cursor, 0);
    }
}
