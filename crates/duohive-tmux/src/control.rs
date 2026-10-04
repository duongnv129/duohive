//! Parser for tmux control-mode output lines (measured formats: spike S0.4).

/// One classified control-mode line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlLine {
    /// `%begin <time> <cmd> <flags>`: a command reply starts.
    Begin { cmd: u64 },
    /// `%end ...`: the reply ended successfully.
    End { cmd: u64 },
    /// `%error ...`: the reply ended with an error.
    Error { cmd: u64 },
    /// A line inside a `%begin` block, verbatim.
    Body(String),
    /// `%unlinked-window-close @N` (what tmux 3.2a and 3.7c send) or `%window-close @N`.
    WindowClose { window: String },
    /// `%exit` with an optional reason.
    Exit { reason: Option<String> },
    /// Any other notification or line.
    Other(String),
}

/// Tracks whether the next line is inside a `%begin` block.
#[derive(Debug, Default)]
pub struct ControlParser {
    in_block: bool,
}

/// Parses the command number (second argument) of `%begin`/`%end`/`%error`.
fn cmd_number(rest: &str) -> Option<u64> {
    rest.split_whitespace().nth(1)?.parse().ok()
}

impl ControlParser {
    /// Classifies one line (without its line terminator).
    pub fn feed(&mut self, line: &str) -> ControlLine {
        if self.in_block {
            return self.feed_in_block(line);
        }
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        match word {
            "%begin" => match cmd_number(rest) {
                Some(cmd) => {
                    self.in_block = true;
                    ControlLine::Begin { cmd }
                }
                None => ControlLine::Other(line.to_owned()),
            },
            "%unlinked-window-close" | "%window-close" => ControlLine::WindowClose {
                window: rest
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
            },
            "%exit" => ControlLine::Exit {
                reason: Some(rest.trim())
                    .filter(|r| !r.is_empty())
                    .map(str::to_owned),
            },
            _ => ControlLine::Other(line.to_owned()),
        }
    }

    fn feed_in_block(&mut self, line: &str) -> ControlLine {
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let end = match word {
            "%end" => cmd_number(rest).map(|cmd| ControlLine::End { cmd }),
            "%error" => cmd_number(rest).map(|cmd| ControlLine::Error { cmd }),
            _ => None,
        };
        match end {
            Some(done) => {
                self.in_block = false;
                done
            }
            None => ControlLine::Body(line.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_begin_body_end() {
        let mut p = ControlParser::default();
        assert_eq!(
            p.feed("%begin 1767225600 12 1"),
            ControlLine::Begin { cmd: 12 }
        );
        assert_eq!(p.feed("@3"), ControlLine::Body("@3".into()));
        assert_eq!(p.feed("%end 1767225600 12 1"), ControlLine::End { cmd: 12 });
    }

    #[test]
    fn parses_error_block() {
        let mut p = ControlParser::default();
        assert_eq!(p.feed("%begin 1 13 1"), ControlLine::Begin { cmd: 13 });
        assert_eq!(
            p.feed("can't find window: @99"),
            ControlLine::Body("can't find window: @99".into())
        );
        assert_eq!(p.feed("%error 1 13 1"), ControlLine::Error { cmd: 13 });
    }

    #[test]
    fn lines_inside_a_block_are_body_even_if_they_start_with_percent() {
        let mut p = ControlParser::default();
        p.feed("%begin 1 5 1");
        assert_eq!(
            p.feed("%output %1 x"),
            ControlLine::Body("%output %1 x".into())
        );
        p.feed("%end 1 5 1");
        assert_eq!(
            p.feed("%output %1 x"),
            ControlLine::Other("%output %1 x".into())
        );
    }

    #[test]
    fn poll_reply_with_trailing_space_rows_is_body() {
        let mut p = ControlParser::default();
        p.feed("%begin 1791044832 570 0");
        assert_eq!(
            p.feed("@0 1791044830 0 "),
            ControlLine::Body("@0 1791044830 0 ".into())
        );
        assert_eq!(
            p.feed("@101 1791044832 1 3"),
            ControlLine::Body("@101 1791044832 1 3".into())
        );
        assert_eq!(
            p.feed("%end 1791044832 570 0"),
            ControlLine::End { cmd: 570 }
        );
    }

    #[test]
    fn parses_window_close_and_exit() {
        let mut p = ControlParser::default();
        assert_eq!(
            p.feed("%unlinked-window-close @4"),
            ControlLine::WindowClose {
                window: "@4".into()
            }
        );
        assert_eq!(
            p.feed("%window-close @4"),
            ControlLine::WindowClose {
                window: "@4".into()
            }
        );
        assert_eq!(p.feed("%exit"), ControlLine::Exit { reason: None });
        assert_eq!(
            p.feed("%exit server exited"),
            ControlLine::Exit {
                reason: Some("server exited".into())
            }
        );
    }

    #[test]
    fn unknown_notifications_are_other() {
        assert_eq!(
            ControlParser::default().feed("%session-renamed $1 x"),
            ControlLine::Other("%session-renamed $1 x".into())
        );
        assert_eq!(
            ControlParser::default().feed("%sessions-changed"),
            ControlLine::Other("%sessions-changed".into())
        );
    }

    #[test]
    fn malformed_begin_is_other_and_does_not_open_a_block() {
        let mut p = ControlParser::default();
        assert_eq!(p.feed("%begin x"), ControlLine::Other("%begin x".into()));
        assert_eq!(p.feed("%exit"), ControlLine::Exit { reason: None });
    }
}
