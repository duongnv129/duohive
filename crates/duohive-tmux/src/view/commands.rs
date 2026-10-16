//! The tmux command lines the view sends, and the first frame built from their replies.

use duohive_core::names::AgentName;
use duohive_core::view::{PaneChunk, PaneInput, PaneKey, PaneSize, ViewError, Wheel};

use crate::quote::tmux_quote;

/// The first frame's cursor line: column, row, visibility, alternate screen, then the
/// CLI's mouse reporting and its SGR encoding (P2-D25).
const CURSOR_FORMAT: &str =
    "#{cursor_x} #{cursor_y} #{cursor_flag} #{alternate_on} #{mouse_any_flag} #{mouse_sgr_flag}";

/// X10 encodes a cell as one byte past 32, so 223 is its last column and row.
const X10_MAX: u16 = 223;

/// One wheel tick as the bytes a terminal sends: SGR `ESC[<64;col;rowM` (65 for
/// down), or X10 `ESC[M` with each value offset by 32 and the cell clamped to 223.
pub(super) fn encode_wheel(wheel: Wheel, col: u16, row: u16, sgr: bool) -> Vec<u8> {
    let button: u8 = match wheel {
        Wheel::Up => 64,
        Wheel::Down => 65,
    };
    if sgr {
        return format!("\x1b[<{button};{col};{row}M").into_bytes();
    }
    let cell = |v: u16| u8::try_from(v.clamp(1, X10_MAX) + 32).unwrap_or(u8::MAX);
    vec![0x1b, b'[', b'M', button + 32, cell(col), cell(row)]
}

/// tmux's name for `key`.
pub(super) fn key_name(key: PaneKey) -> Result<String, ViewError> {
    let modified = |prefix: &str, c: char| match c {
        ' ' => Ok(format!("{prefix}Space")),
        c if c.is_control() => Err(ViewError::Tmux(format!("no key {prefix}{c:?}"))),
        c => Ok(format!("{prefix}{c}")),
    };
    Ok(match key {
        PaneKey::Enter => "Enter".into(),
        PaneKey::Tab => "Tab".into(),
        PaneKey::BackTab => "BTab".into(),
        PaneKey::Backspace => "BSpace".into(),
        PaneKey::Delete => "DC".into(),
        PaneKey::Escape => "Escape".into(),
        PaneKey::Up => "Up".into(),
        PaneKey::Down => "Down".into(),
        PaneKey::Left => "Left".into(),
        PaneKey::Right => "Right".into(),
        PaneKey::Home => "Home".into(),
        PaneKey::End => "End".into(),
        PaneKey::PageUp => "PPage".into(),
        PaneKey::PageDown => "NPage".into(),
        PaneKey::Insert => "IC".into(),
        PaneKey::F(n @ 1..=12) => format!("F{n}"),
        PaneKey::F(n) => return Err(ViewError::Tmux(format!("no key F{n}"))),
        PaneKey::Ctrl(c) => modified("C-", c.to_ascii_lowercase())?,
        PaneKey::Alt(c) => modified("M-", c)?,
    })
}

/// `tmux_quote` with the error as text.
pub(super) fn q(arg: &str) -> Result<String, String> {
    tmux_quote(arg).map_err(|e| e.to_string())
}

/// The commands that type `input` into `pane` (with their count); a paste's buffer
/// is loaded first, by the caller. Literal text goes with `-l`; a newline or CR in it
/// is sent as `Enter`, and NUL is dropped.
pub(super) fn input_line(
    pane: &str,
    input: &PaneInput,
    buffer: &str,
) -> Result<(String, usize), String> {
    let target = q(pane)?;
    let cmds = match input {
        PaneInput::Text(text) => {
            let mut cmds = Vec::new();
            let text = text
                .replace('\0', "")
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            for (i, part) in text.split('\n').enumerate() {
                if i > 0 {
                    cmds.push(format!("send-keys -t {target} Enter"));
                }
                if !part.is_empty() {
                    cmds.push(format!("send-keys -t {target} -l -- {}", q(part)?));
                }
            }
            cmds
        }
        PaneInput::Key(key) => {
            let name = key_name(*key).map_err(|e| e.to_string())?;
            vec![format!("send-keys -t {target} {}", q(&name)?)]
        }
        PaneInput::Paste(_) => vec![format!("paste-buffer -d -p -b {} -t {target}", q(buffer)?)],
        PaneInput::Mouse {
            wheel,
            col,
            row,
            sgr,
        } => {
            let hex: Vec<String> = encode_wheel(*wheel, *col, *row, *sgr)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            vec![format!("send-keys -H -t {target} {}", hex.join(" "))]
        }
    };
    Ok((cmds.join(" ; "), cmds.len()))
}

/// The pane a stream shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Target {
    pub(super) window: String,
    pub(super) pane: String,
}

/// `list-panes -s` for the session: every pane with its window and agent tag.
pub(super) fn panes_command(session: &str) -> Result<String, String> {
    Ok(format!(
        "list-panes -s -t {} -F '#{{window_id}} #{{pane_id}} #{{@duohive_agent}}'",
        q(&format!("={session}"))?
    ))
}

/// The agent's newest window (as the runtime's `current`), and every other pane.
pub(super) fn find_target(rows: &[String], agent: &AgentName) -> (Option<Target>, Vec<String>) {
    let parsed: Vec<(&str, &str, &str)> = rows
        .iter()
        .filter_map(|row| {
            let mut words = row.splitn(3, ' ');
            Some((
                words.next()?,
                words.next()?,
                words.next().unwrap_or("").trim(),
            ))
        })
        .collect();
    let number = |w: &str| w.trim_start_matches('@').parse::<u64>().unwrap_or(0);
    let target = parsed
        .iter()
        .filter(|(_, _, tag)| *tag == agent.as_str())
        .max_by_key(|(w, _, _)| number(w))
        .map(|(w, p, _)| Target {
            window: (*w).to_owned(),
            pane: (*p).to_owned(),
        });
    let others = parsed
        .iter()
        .map(|(_, p, _)| (*p).to_owned())
        .filter(|p| target.as_ref().is_none_or(|t| t.pane != *p))
        .collect();
    (target, others)
}

/// S3.8's open line, after sizing the window: `!no-output` first (clearing it resets
/// every pane's on/off state), every other pane off, the viewed pane on, then the
/// cursor and `capture-pane -e -N` for the first frame. tmux runs it in one pass, so
/// the `%output` after the capture's reply continues where the capture ends.
pub(super) fn open_line(
    target: &Target,
    others: &[String],
    size: PaneSize,
) -> Result<(String, usize), String> {
    let pane = q(&target.pane)?;
    let mut cmds = vec![
        resize_command(&target.window, size)?,
        "refresh-client -f '!no-output'".to_owned(),
    ];
    for other in others {
        cmds.push(format!("refresh-client -A {}", q(&format!("{other}:off"))?));
    }
    cmds.push(format!(
        "refresh-client -A {}",
        q(&format!("{}:on", target.pane))?
    ));
    cmds.push(format!("display -p -t {pane} {}", q(CURSOR_FORMAT)?));
    cmds.push(format!("capture-pane -e -N -p -t {pane}"));
    Ok((cmds.join(" ; "), cmds.len()))
}

pub(super) fn resize_command(window: &str, size: PaneSize) -> Result<String, String> {
    Ok(format!(
        "resize-window -t {} -x {} -y {}",
        q(window)?,
        size.cols.max(1),
        size.rows.max(1)
    ))
}

/// The first frame: the capture's rows joined by CRLF, on the alternate screen when the
/// pane is on it, then the cursor's position and visibility.
pub(super) fn first_frame(cursor_line: &[String], capture: &[String]) -> PaneChunk {
    let fields: Vec<u16> = cursor_line
        .first()
        .map(|l| {
            l.split_whitespace()
                .map(|n| n.parse().unwrap_or(0))
                .collect()
        })
        .unwrap_or_default();
    let field = |i: usize| fields.get(i).copied().unwrap_or(0);
    let (x, y, visible, alternate) = (field(0), field(1), field(2) == 1, field(3) == 1);
    let mut bytes = Vec::new();
    if alternate {
        bytes.extend_from_slice(b"\x1b[?1049h\x1b[H\x1b[2J");
    }
    // The capture carries no modes, so the CLI's mouse reporting is replayed (P2-D25).
    if field(4) == 1 {
        bytes.extend_from_slice(b"\x1b[?1000h");
        if field(5) == 1 {
            bytes.extend_from_slice(b"\x1b[?1006h");
        }
    }
    bytes.extend_from_slice(capture.join("\r\n").as_bytes());
    bytes.extend_from_slice(format!("\x1b[{};{}H", y + 1, x + 1).as_bytes());
    if !visible {
        bytes.extend_from_slice(b"\x1b[?25l");
    }
    PaneChunk::Reset {
        bytes,
        cursor: (x, y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> AgentName {
        AgentName::parse(s).unwrap()
    }

    #[test]
    fn keys_have_tmux_names() {
        let cases = [
            (PaneKey::Enter, "Enter"),
            (PaneKey::Tab, "Tab"),
            (PaneKey::BackTab, "BTab"),
            (PaneKey::Backspace, "BSpace"),
            (PaneKey::Delete, "DC"),
            (PaneKey::Escape, "Escape"),
            (PaneKey::Up, "Up"),
            (PaneKey::Down, "Down"),
            (PaneKey::Left, "Left"),
            (PaneKey::Right, "Right"),
            (PaneKey::Home, "Home"),
            (PaneKey::End, "End"),
            (PaneKey::PageUp, "PPage"),
            (PaneKey::PageDown, "NPage"),
            (PaneKey::Insert, "IC"),
            (PaneKey::F(1), "F1"),
            (PaneKey::F(12), "F12"),
            (PaneKey::Ctrl('x'), "C-x"),
            (PaneKey::Ctrl('C'), "C-c"),
            (PaneKey::Ctrl(' '), "C-Space"),
            (PaneKey::Alt('x'), "M-x"),
            (PaneKey::Alt(';'), "M-;"),
        ];
        for (key, name) in cases {
            assert_eq!(key_name(key).unwrap(), name, "{key:?}");
        }
        assert!(key_name(PaneKey::F(0)).is_err());
        assert!(key_name(PaneKey::F(13)).is_err());
        assert!(key_name(PaneKey::Ctrl('\n')).is_err());
    }

    #[test]
    fn input_targets_the_pane_and_quotes_everything() {
        assert_eq!(
            input_line("%3", &PaneInput::Text("-x it's".into()), "b").unwrap(),
            (r"send-keys -t '%3' -l -- '-x it'\''s'".to_owned(), 1)
        );
        assert_eq!(
            input_line("%3", &PaneInput::Text("a\nb\r".into()), "b").unwrap(),
            (
                "send-keys -t '%3' -l -- 'a' ; send-keys -t '%3' Enter ; \
                 send-keys -t '%3' -l -- 'b' ; send-keys -t '%3' Enter"
                    .to_owned(),
                4
            )
        );
        assert_eq!(
            input_line("%3", &PaneInput::Key(PaneKey::Alt(';')), "b").unwrap(),
            ("send-keys -t '%3' 'M-;'".to_owned(), 1)
        );
        assert_eq!(
            input_line("%3", &PaneInput::Paste("x\ny".into()), "duohive-1-view").unwrap(),
            (
                "paste-buffer -d -p -b 'duohive-1-view' -t '%3'".to_owned(),
                1
            )
        );
        assert_eq!(
            input_line("%3", &PaneInput::Text(String::new()), "b").unwrap(),
            (String::new(), 0)
        );
    }

    fn rows(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|l| (*l).to_owned()).collect()
    }

    #[test]
    fn the_target_is_the_agents_newest_window() {
        let rows = rows(&["@0 %0 ", "@2 %2 a", "@10 %10 a", "@3 %3 b", "@4 %4"]);
        let (target, others) = find_target(&rows, &n("a"));
        assert_eq!(
            target,
            Some(Target {
                window: "@10".into(),
                pane: "%10".into()
            })
        );
        assert_eq!(others, ["%0", "%2", "%3", "%4"]);
        let (none, all) = find_target(&rows, &n("c"));
        assert_eq!(none, None);
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn the_open_line_clears_no_output_first() {
        let target = Target {
            window: "@2".into(),
            pane: "%2".into(),
        };
        let others = vec!["%0".to_owned(), "%5".to_owned()];
        let (line, blocks) = open_line(&target, &others, PaneSize { cols: 80, rows: 0 }).unwrap();
        assert_eq!(
            line,
            "resize-window -t '@2' -x 80 -y 1 ; refresh-client -f '!no-output' ; \
             refresh-client -A '%0:off' ; refresh-client -A '%5:off' ; \
             refresh-client -A '%2:on' ; \
             display -p -t '%2' '#{cursor_x} #{cursor_y} #{cursor_flag} #{alternate_on} #{mouse_any_flag} #{mouse_sgr_flag}' ; \
             capture-pane -e -N -p -t '%2'"
        );
        assert_eq!(blocks, 7);
    }

    #[test]
    fn the_first_frame_is_crlf_rows_then_the_cursor() {
        let frame = first_frame(&rows(&["3 1 1 0"]), &rows(&["ab", "\x1b[1mc"]));
        assert_eq!(
            frame,
            PaneChunk::Reset {
                bytes: b"ab\r\n\x1b[1mc\x1b[2;4H".to_vec(),
                cursor: (3, 1)
            }
        );
        let PaneChunk::Reset { bytes, cursor } = first_frame(&rows(&["0 0 0 1"]), &rows(&["x"]))
        else {
            panic!("not a Reset");
        };
        assert_eq!(cursor, (0, 0));
        assert_eq!(
            bytes,
            b"\x1b[?1049h\x1b[H\x1b[2Jx\x1b[1;1H\x1b[?25l".to_vec()
        );
    }
    #[test]
    fn mouse_flags_in_first_frame_format() {
        assert!(CURSOR_FORMAT.ends_with("#{mouse_any_flag} #{mouse_sgr_flag}"));
        let bytes = |line: &str| match first_frame(&rows(&[line]), &rows(&["x"])) {
            PaneChunk::Reset { bytes, .. } => String::from_utf8(bytes).unwrap(),
            other => panic!("not a Reset: {other:?}"),
        };
        let sgr = bytes("0 0 1 1 1 1");
        assert!(sgr.contains("\x1b[?1000h") && sgr.contains("\x1b[?1006h"));
        let x10 = bytes("0 0 1 1 1 0");
        assert!(x10.contains("\x1b[?1000h") && !x10.contains("\x1b[?1006h"));
        let off = bytes("0 0 1 1 0 0");
        assert!(!off.contains("\x1b[?1000h") && !off.contains("\x1b[?1006h"));
    }

    #[test]
    fn mouse_input_sends_sgr_bytes_with_send_keys_h() {
        let input = PaneInput::Mouse {
            wheel: Wheel::Up,
            col: 5,
            row: 7,
            sgr: true,
        };
        assert_eq!(
            input_line("%3", &input, "b").unwrap(),
            (
                "send-keys -H -t '%3' 1b 5b 3c 36 34 3b 35 3b 37 4d".to_owned(),
                1
            )
        );
    }

    #[test]
    fn mouse_input_sends_x10_clamped_to_223() {
        let input = PaneInput::Mouse {
            wheel: Wheel::Down,
            col: 300,
            row: 2,
            sgr: false,
        };
        assert_eq!(
            input_line("%3", &input, "b").unwrap().0,
            "send-keys -H -t '%3' 1b 5b 4d 61 ff 22"
        );
    }
}
