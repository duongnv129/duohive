//! Throwaway spike S3.4: does `vt100 =0.16.2` render a recorded CLI session as tmux does?
//!
//! usage: s3-fidelity <capture-dir>...
//!
//! Each dir holds `stream.bin` (the pane's raw output from its first byte, `pipe-pane -O`),
//! `capture-e.txt` (`capture-pane -e -p` at the end), `capture.txt` (`capture-pane -p`) and
//! `meta.txt` (`width height cursor_x cursor_y cursor_flag alternate_on`).
//! It feeds `stream.bin` to one parser and `capture-e.txt` to another (the first-frame path),
//! and compares both with tmux: text, cell colours and attributes, cursor, alternate screen.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::Path;

#[derive(Default)]
struct Unhandled {
    seqs: BTreeMap<String, usize>,
}

impl vt100::Callbacks for Unhandled {
    fn unhandled_char(&mut self, _: &mut vt100::Screen, c: char) {
        *self.seqs.entry(format!("char {c:?}")).or_default() += 1;
    }
    fn unhandled_control(&mut self, _: &mut vt100::Screen, b: u8) {
        *self.seqs.entry(format!("ctl 0x{b:02x}")).or_default() += 1;
    }
    fn unhandled_escape(&mut self, _: &mut vt100::Screen, i1: Option<u8>, i2: Option<u8>, b: u8) {
        let k = format!(
            "ESC {:?}{:?}{}",
            i1.map(char::from),
            i2.map(char::from),
            char::from(b)
        );
        *self.seqs.entry(k).or_default() += 1;
    }
    fn unhandled_csi(
        &mut self,
        _: &mut vt100::Screen,
        i1: Option<u8>,
        i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        let p: Vec<String> = params
            .iter()
            .map(|x| x.iter().map(u16::to_string).collect::<Vec<_>>().join(":"))
            .collect();
        let pre = [i1, i2]
            .iter()
            .flatten()
            .map(|b| char::from(*b))
            .collect::<String>();
        *self
            .seqs
            .entry(format!("CSI {pre}{}{c}", p.join(";")))
            .or_default() += 1;
    }
    fn unhandled_osc(&mut self, _: &mut vt100::Screen, params: &[&[u8]]) {
        let first = params
            .first()
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .unwrap_or_default();
        *self.seqs.entry(format!("OSC {first}")).or_default() += 1;
    }
}

type Attrs = (
    String,
    vt100::Color,
    vt100::Color,
    bool,
    bool,
    bool,
    bool,
    bool,
);

fn cell(s: &vt100::Screen, r: u16, c: u16) -> Option<Attrs> {
    let x = s.cell(r, c)?;
    let text = if x.has_contents() {
        x.contents().to_string()
    } else {
        " ".to_string()
    };
    Some((
        text,
        x.fgcolor(),
        x.bgcolor(),
        x.bold(),
        x.dim(),
        x.italic(),
        x.underline(),
        x.inverse(),
    ))
}

fn rows_text(s: &vt100::Screen, cols: u16) -> Vec<String> {
    s.rows(0, cols).map(|r| r.trim_end().to_string()).collect()
}

fn check(dir: &Path) -> Result<(), Box<dyn Error>> {
    let meta = fs::read_to_string(dir.join("meta.txt"))?;
    let m: Vec<u16> = meta
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    let [cols, rows, cx, cy, cflag, alt] = m[..] else {
        return Err("meta.txt: want 6 numbers".into());
    };
    let stream = fs::read(dir.join("stream.bin"))?;
    let mut live = vt100::Parser::new_with_callbacks(rows, cols, 0, Unhandled::default());
    live.process(&stream);

    // The first-frame path: capture-pane -e -p, lines joined by CRLF, into a fresh parser.
    // capture-eN.txt (capture-pane -e -N -p) when present, else capture-e.txt.
    let (flags, cap) = match fs::read(dir.join("capture-eN.txt")) {
        Ok(c) if std::env::var_os("S3_NO_N").is_none() => ("-e -N", c),
        _ => ("-e", fs::read(dir.join("capture-e.txt"))?),
    };
    let cap = cap.strip_suffix(b"\n").unwrap_or(&cap);
    let mut framed = Vec::with_capacity(cap.len() + 64);
    for b in cap {
        if *b == b'\n' {
            framed.extend_from_slice(b"\r\n");
        } else {
            framed.push(*b);
        }
    }
    let mut first = vt100::Parser::new(rows, cols, 0);
    first.process(&framed);

    let plain = fs::read_to_string(dir.join("capture.txt"))?;
    let tmux_rows: Vec<String> = plain.lines().map(|l| l.trim_end().to_string()).collect();
    let s = live.screen();
    let live_rows = rows_text(s, cols);
    let text_diff: Vec<usize> = (0..rows as usize)
        .filter(|&i| {
            live_rows.get(i).map(String::as_str).unwrap_or("")
                != tmux_rows.get(i).map(String::as_str).unwrap_or("")
        })
        .collect();

    let mut cells = 0;
    let mut attr_diff = 0;
    let mut sample = Vec::new();
    let mut colours = BTreeSet::new();
    for r in 0..rows {
        for c in 0..cols {
            let (Some(a), Some(b)) = (cell(s, r, c), cell(first.screen(), r, c)) else {
                continue;
            };
            cells += 1;
            colours.insert(format!("{:?}", a.1));
            colours.insert(format!("bg {:?}", a.2));
            if a != b {
                attr_diff += 1;
                if sample.len() < 5 {
                    sample.push(format!("({r},{c}) stream {a:?} vs capture {b:?}"));
                }
            }
        }
    }
    let (lr, lc) = s.cursor_position();
    let rgb = colours.iter().filter(|c| c.contains("Rgb")).count();
    let idx = colours.iter().filter(|c| c.contains("Idx")).count();
    println!("== {}", dir.display());
    println!("  size {cols}x{rows}, stream {} bytes", stream.len());
    println!(
        "  text rows differing from capture-pane -p: {} {:?}",
        text_diff.len(),
        text_diff.iter().take(5).collect::<Vec<_>>()
    );
    for &i in text_diff.iter().take(4) {
        println!(
            "    row {i}: vt100 {:?}",
            live_rows.get(i).map(String::as_str).unwrap_or("")
        );
        println!(
            "    row {i}: tmux  {:?}",
            tmux_rows.get(i).map(String::as_str).unwrap_or("")
        );
    }
    println!("  cells differing, stream vs capture-pane {flags} frame: {attr_diff} of {cells}");
    for x in &sample {
        println!("    {x}");
    }
    println!(
        "  distinct colours in the stream's screen: {} ({rgb} RGB, {idx} indexed)",
        colours.len()
    );
    println!(
        "  cursor: vt100 ({lc},{lr}) tmux ({cx},{cy}) -> {}",
        if (lc, lr) == (cx, cy) {
            "same"
        } else {
            "DIFFERENT"
        }
    );
    println!(
        "  cursor visible: vt100 {} tmux {} -> {}",
        !s.hide_cursor(),
        cflag == 1,
        if !s.hide_cursor() == (cflag == 1) {
            "same"
        } else {
            "DIFFERENT"
        }
    );
    println!(
        "  alternate screen: vt100 {} tmux {} -> {}",
        s.alternate_screen(),
        alt == 1,
        if s.alternate_screen() == (alt == 1) {
            "same"
        } else {
            "DIFFERENT"
        }
    );
    println!(
        "  vt100 unhandled sequences: {}",
        live.callbacks().seqs.len()
    );
    for (k, n) in &live.callbacks().seqs {
        println!("    {n:>5}  {k}");
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    for arg in std::env::args().skip(1) {
        check(Path::new(&arg))?;
    }
    Ok(())
}
