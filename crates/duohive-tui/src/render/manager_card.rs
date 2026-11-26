//! The Manager card: Duo as a four-line box with a wordmark and a status pill (UI §4.1.5).

use duohive_core::status::AgentState;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use super::{RenderCtx, fg, put};
use crate::boxes::{BoxLine, BoxLineKind, DUO_NAME, ROOT_BOX};
use crate::format::width;
use crate::model::UiModel;
use crate::theme::{Look, Palette, Pill};
use crate::wordmark::{WIDTH as WORDMARK_WIDTH, wordmark};

/// Columns before the wordmark, and between the wordmark and the text.
const INDENT: usize = 2;
const GAP: usize = 3;

/// Draws one line of the card at `(x, y)`; `typed` fills it solid (UI §2.3).
pub(super) fn draw(
    buf: &mut Buffer,
    (x, y): (u16, u16),
    line: &BoxLine,
    m: &UiModel,
    ctx: &RenderCtx,
    typed: bool,
) {
    let p = &ctx.palette;
    let style = CardStyle::of(p, line.selected, typed);
    let w = ROOT_BOX;
    let right_x = x + u16::try_from(w - 1).unwrap_or(0);
    match line.kind {
        BoxLineKind::ManagerTop => top(buf, (x, y), line, &style),
        BoxLineKind::ManagerBottom => {
            let rule = format!("└{}┘", "─".repeat(w - 2));
            put(buf, x, y, &rule, w, style.border);
        }
        kind => {
            if let Some(fill) = style.fill {
                buf.set_style(
                    Rect::new(x + 1, y, u16::try_from(w - 2).unwrap_or(0), 1),
                    fill,
                );
            }
            put(buf, x, y, "│", 1, style.border);
            put(buf, right_x, y, "│", 1, style.border);
            let mark = wordmark(m.org.duo.name.as_str());
            let name_line = kind == BoxLineKind::ManagerName;
            let row = if name_line { &mark[0] } else { &mark[1] };
            let text_x = x + 1 + u16::try_from(INDENT).unwrap_or(0);
            put(buf, text_x, y, row, WORDMARK_WIDTH, style.text);
            let label_x = text_x + u16::try_from(WORDMARK_WIDTH + GAP).unwrap_or(0);
            if name_line {
                put(buf, label_x, y, &line.label, DUO_NAME, style.name);
                pill(buf, (x, y), &line.right, p, typed);
            } else {
                let room = w - 2 - INDENT - WORDMARK_WIDTH - GAP - 1;
                let errored = m.view(&m.org.duo.name).status.state == AgentState::Errored;
                // A selected card keeps the selected text colour: `status.error` would fall
                // below 4.5:1 on the wash.
                let colour = if errored && !line.selected {
                    style.text.fg(p.error)
                } else {
                    style.counts
                };
                put(buf, label_x, y, &line.label, room, colour);
            }
        }
    }
}

/// The colours of the card's parts for one state of the card.
struct CardStyle {
    border: Style,
    /// The wash, or the solid fill when typed into; `None` where none is painted.
    fill: Option<Style>,
    title: Style,
    /// The wordmark.
    text: Style,
    /// Duo's name.
    name: Style,
    /// Line 2.
    counts: Style,
    chip: Style,
}

impl CardStyle {
    fn of(p: &Palette, selected: bool, typed: bool) -> Self {
        let bold = Modifier::BOLD;
        let paints = matches!(p.look, Look::Dark | Look::Light);
        let reversed = Style::default()
            .fg(p.manager)
            .add_modifier(Modifier::REVERSED);
        let chip = if paints {
            Style::default().fg(chip_text(p)).bg(p.manager)
        } else {
            reversed
        };
        let border_colour = if selected {
            p.manager
        } else {
            p.tint_manager(40)
        };
        if typed {
            // Solid manager fill, the text in `bg` (reverse video under `auto`).
            let solid = if paints {
                Style::default().fg(p.bg).bg(p.manager)
            } else {
                reversed
            };
            return Self {
                border: if paints {
                    fg(p.bg).bg(p.manager)
                } else {
                    reversed
                },
                fill: Some(solid),
                title: solid.add_modifier(bold),
                text: solid,
                name: solid.add_modifier(bold),
                counts: solid,
                chip,
            };
        }
        let (percent, strong) = if selected {
            (26, Some(p.emphasis()))
        } else {
            (10, None)
        };
        let wash = paints.then(|| Style::default().bg(p.tint_manager(percent)));
        let on_wash = |s: Style| wash.map_or(s, |w| s.patch(w));
        let ink = fg(strong.unwrap_or(p.manager));
        Self {
            border: on_wash(fg(border_colour)),
            fill: wash,
            title: fg(p.manager).add_modifier(bold),
            text: on_wash(fg(p.manager)),
            name: on_wash(ink.add_modifier(bold)),
            counts: on_wash(ink),
            chip,
        }
    }
}

/// Chip text: white on `light`, else `bg` (UI §2.3).
pub(super) fn chip_text(p: &Palette) -> Color {
    if p.look == Look::Light {
        Color::Rgb(255, 255, 255)
    } else {
        p.bg
    }
}

fn top(buf: &mut Buffer, (x, y): (u16, u16), line: &BoxLine, s: &CardStyle) {
    let w = ROOT_BOX;
    put(buf, x, y, "┌─", w, s.border);
    let title = format!(" {} ", line.label);
    let style = if line.selected {
        s.chip.add_modifier(Modifier::BOLD)
    } else {
        s.title
    };
    let after = put(buf, x + 2, y, &title, w, style);
    let rest = format!("{}┐", "─".repeat(w - 1 - 2 - width(&title)));
    put(buf, after, y, &rest, w, s.border);
}

/// The status pill, right-aligned on the name line.
fn pill(buf: &mut Buffer, (x, y): (u16, u16), text: &str, p: &Palette, typed: bool) {
    let kind = match text.chars().next() {
        Some('◌') => Pill::Starting,
        Some('●') => Pill::Working,
        Some('◆') => Pill::Waiting,
        Some('◷') => Pill::Quiet,
        Some('✕') => Pill::Errored,
        Some('○') => Pill::Stopped,
        _ => Pill::Idle,
    };
    let (bg, text_colour) = p.pill(kind, typed);
    let style = match p.look {
        Look::Dark | Look::Light => Style::default().fg(text_colour).bg(bg),
        Look::Auto => Style::default().fg(bg).add_modifier(Modifier::REVERSED),
        Look::Mono => Style::default().add_modifier(Modifier::REVERSED),
    };
    let label = format!(" {text} ");
    // Right-aligned, so its right edge is the last column inside the border.
    let start = x + u16::try_from(ROOT_BOX - 1 - width(&label)).unwrap_or(0);
    put(buf, start, y, &label, width(&label), style);
}
