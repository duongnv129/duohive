//! Parser for tmux control-mode output lines (measured formats: spike S0.4).

/// One classified control-mode line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlLine {
    /// `%begin <time> <cmd> <flags>`: a reply block starts. Flags 1 marks a command this
    /// client sent; flags 0 blocks (the attach itself, hook commands) answer nothing it sent.
    Begin { cmd: u64, flags: u32 },
    /// `%end ...`: the reply ended successfully.
    End { cmd: u64, flags: u32 },
    /// `%error ...`: the reply ended with an error.
    Error { cmd: u64, flags: u32 },
    /// A line inside a `%begin` block, verbatim.
    Body(String),
    /// `%unlinked-window-close @N` (what tmux 3.2a and 3.7c send) or `%window-close @N`.
    WindowClose { window: String },
    /// `%window-add @N`: a window was added to the attached session.
    WindowAdd { window: String },
    /// `%output %P <data>`: pane output, its `\ooo` escapes decoded.
    Output { pane: String, data: Vec<u8> },
    /// `%exit` with an optional reason.
    Exit { reason: Option<String> },
    /// Any other notification or line.
    Other(String),
}

/// Tracks the open `%begin` block; it closes only on `%end`/`%error` with its number.
#[derive(Debug, Default)]
pub struct ControlParser {
    /// Command number of the open `%begin` block.
    open_cmd: Option<u64>,
}

/// Parses the command number and flags (second and third arguments) of `%begin`/`%end`/`%error`.
fn cmd_number(rest: &str) -> Option<(u64, u32)> {
    let mut args = rest.split_whitespace().skip(1);
    Some((args.next()?.parse().ok()?, args.next()?.parse().ok()?))
}

/// Parses `%P <data>` after `%output `; `None` without a pane.
fn parse_output(rest: &[u8]) -> Option<ControlLine> {
    let space = rest.iter().position(|&b| b == b' ');
    let (pane, data) = match space {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, &[][..]),
    };
    let pane = std::str::from_utf8(pane).ok()?;
    pane.starts_with('%').then(|| ControlLine::Output {
        pane: pane.to_owned(),
        data: unescape(data),
    })
}

/// Decodes tmux's `\ooo` octal escapes (it escapes bytes below 32 and the backslash); every other
/// byte, UTF-8 or not, is passed through.
pub fn unescape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        let octal = data
            .get(i + 1..i + 4)
            .filter(|d| data[i] == b'\\' && d.iter().all(|b| (b'0'..=b'7').contains(b)))
            .map(|d| d.iter().fold(0u32, |n, b| n * 8 + u32::from(b - b'0')))
            .and_then(|n| u8::try_from(n).ok());
        match octal {
            Some(byte) => {
                out.push(byte);
                i += 4;
            }
            None => {
                out.push(data[i]);
                i += 1;
            }
        }
    }
    out
}

impl ControlParser {
    /// Classifies one raw line (without its terminator). `%output` data is taken as
    /// bytes: tmux passes bytes from 128 up raw, and may split a UTF-8 character
    /// between two lines. Any other line is read as UTF-8, lossily.
    pub fn feed_bytes(&mut self, line: &[u8]) -> ControlLine {
        if self.open_cmd.is_none()
            && let Some(rest) = line.strip_prefix(b"%output ")
            && let Some(output) = parse_output(rest)
        {
            return output;
        }
        self.feed(&String::from_utf8_lossy(line))
    }

    /// Classifies one line (without its line terminator).
    pub fn feed(&mut self, line: &str) -> ControlLine {
        if let Some(open) = self.open_cmd {
            return self.feed_in_block(open, line);
        }
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        match word {
            "%begin" => match cmd_number(rest) {
                Some((cmd, flags)) => {
                    self.open_cmd = Some(cmd);
                    ControlLine::Begin { cmd, flags }
                }
                None => ControlLine::Other(line.to_owned()),
            },
            "%unlinked-window-close" | "%window-close" => match rest.split_whitespace().next() {
                Some(window) => ControlLine::WindowClose {
                    window: window.to_owned(),
                },
                None => ControlLine::Other(line.to_owned()),
            },
            "%window-add" => match rest.split_whitespace().next() {
                Some(window) => ControlLine::WindowAdd {
                    window: window.to_owned(),
                },
                None => ControlLine::Other(line.to_owned()),
            },
            "%output" => {
                parse_output(rest.as_bytes()).unwrap_or_else(|| ControlLine::Other(line.to_owned()))
            }
            "%exit" => ControlLine::Exit {
                reason: Some(rest.trim())
                    .filter(|r| !r.is_empty())
                    .map(str::to_owned),
            },
            _ => ControlLine::Other(line.to_owned()),
        }
    }

    fn feed_in_block(&mut self, open: u64, line: &str) -> ControlLine {
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let end = match word {
            "%end" => cmd_number(rest)
                .filter(|&(c, _)| c == open)
                .map(|(cmd, flags)| ControlLine::End { cmd, flags }),
            "%error" => cmd_number(rest)
                .filter(|&(c, _)| c == open)
                .map(|(cmd, flags)| ControlLine::Error { cmd, flags }),
            _ => None,
        };
        match end {
            Some(done) => {
                self.open_cmd = None;
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
            ControlLine::Begin { cmd: 12, flags: 1 }
        );
        assert_eq!(p.feed("@3"), ControlLine::Body("@3".into()));
        assert_eq!(
            p.feed("%end 1767225600 12 1"),
            ControlLine::End { cmd: 12, flags: 1 }
        );
    }

    #[test]
    fn parses_error_block() {
        let mut p = ControlParser::default();
        assert_eq!(
            p.feed("%begin 1 13 1"),
            ControlLine::Begin { cmd: 13, flags: 1 }
        );
        assert_eq!(
            p.feed("can't find window: @99"),
            ControlLine::Body("can't find window: @99".into())
        );
        assert_eq!(
            p.feed("%error 1 13 1"),
            ControlLine::Error { cmd: 13, flags: 1 }
        );
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
            ControlLine::Output {
                pane: "%1".into(),
                data: b"x".to_vec()
            }
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
            ControlLine::End { cmd: 570, flags: 0 }
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
    fn output_is_decoded_as_bytes() {
        let mut p = ControlParser::default();
        assert_eq!(
            p.feed_bytes(b"%output %3 hi\\015\\012\\033[1m\\134x"),
            ControlLine::Output {
                pane: "%3".into(),
                data: b"hi\r\n\x1b[1m\\x".to_vec()
            }
        );
        // Half a UTF-8 character: tmux splits output at any byte.
        assert_eq!(
            p.feed_bytes(b"%output %3 \xe2\x94"),
            ControlLine::Output {
                pane: "%3".into(),
                data: vec![0xe2, 0x94]
            }
        );
        assert_eq!(
            p.feed("%output %12 a b"),
            ControlLine::Output {
                pane: "%12".into(),
                data: b"a b".to_vec()
            }
        );
        assert_eq!(
            p.feed_bytes(b"%output %4"),
            ControlLine::Output {
                pane: "%4".into(),
                data: Vec::new()
            }
        );
    }

    #[test]
    fn output_inside_a_block_is_body() {
        let mut p = ControlParser::default();
        p.feed_bytes(b"%begin 1 5 1");
        assert_eq!(
            p.feed_bytes(b"%output %1 x\xe2"),
            ControlLine::Body("%output %1 x\u{fffd}".into())
        );
    }

    #[test]
    fn unescape_leaves_partial_escapes_alone() {
        assert_eq!(unescape(br"\01"), br"\01".to_vec());
        assert_eq!(unescape(br"\777x"), br"\777x".to_vec());
        assert_eq!(unescape(br"\0089"), br"\0089".to_vec());
        assert_eq!(unescape(br"\134\134"), br"\\".to_vec());
    }

    #[test]
    fn parses_window_add() {
        let mut p = ControlParser::default();
        assert_eq!(
            p.feed_bytes(b"%window-add @7"),
            ControlLine::WindowAdd {
                window: "@7".into()
            }
        );
        assert_eq!(
            p.feed("%window-add"),
            ControlLine::Other("%window-add".into())
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
    fn block_closes_only_on_matching_command_number() {
        let mut p = ControlParser::default();
        p.feed("%begin 1 7 1");
        assert_eq!(p.feed("%end 1 2 0"), ControlLine::Body("%end 1 2 0".into()));
        assert_eq!(
            p.feed("%error 1 8 1"),
            ControlLine::Body("%error 1 8 1".into())
        );
        assert_eq!(p.feed("%end 1 7 1"), ControlLine::End { cmd: 7, flags: 1 });
    }

    #[test]
    fn window_close_without_id_is_other() {
        let mut p = ControlParser::default();
        assert_eq!(
            p.feed("%window-close"),
            ControlLine::Other("%window-close".into())
        );
        assert_eq!(
            p.feed("%unlinked-window-close "),
            ControlLine::Other("%unlinked-window-close ".into())
        );
    }

    #[test]
    fn malformed_begin_is_other_and_does_not_open_a_block() {
        let mut p = ControlParser::default();
        assert_eq!(p.feed("%begin x"), ControlLine::Other("%begin x".into()));
        assert_eq!(
            p.feed("%begin 1 5"),
            ControlLine::Other("%begin 1 5".into())
        );
        assert_eq!(p.feed("%exit"), ControlLine::Exit { reason: None });
    }

    #[test]
    fn flags_tell_client_commands_from_server_blocks() {
        let mut p = ControlParser::default();
        // The attach's own block and hook commands carry flags 0 (spike S0.4, task 19).
        assert_eq!(
            p.feed("%begin 1 283 0"),
            ControlLine::Begin { cmd: 283, flags: 0 }
        );
        assert_eq!(
            p.feed("%end 1 283 0"),
            ControlLine::End { cmd: 283, flags: 0 }
        );
        assert_eq!(
            p.feed("%begin 1 290 1"),
            ControlLine::Begin { cmd: 290, flags: 1 }
        );
        assert_eq!(
            p.feed("%error 1 290 1"),
            ControlLine::Error { cmd: 290, flags: 1 }
        );
    }
}
