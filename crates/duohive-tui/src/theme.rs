//! Colour tokens (UI §2.3) and the palettes that map them.

use duohive_core::settings::Theme;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier, Style};

/// `auto`'s `fg.muted` and `border` (P2-D26): reserved colours that [`finish`] turns
/// into the terminal's own foreground, dimmed. Colours travel through the renderers as
/// plain `Color`s, which can't carry `DIM`; the live terminal never draws these two
/// (`TermView` nudges them), so only DuoHive's own cells hold them.
pub(crate) const DIM_MUTED: Color = Color::Rgb(3, 1, 4);
pub(crate) const DIM_BORDER: Color = Color::Rgb(1, 5, 9);

/// Every UI §2.3 colour token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// `fg`: text.
    pub fg: Color,
    /// `fg.muted`: secondary text, inherited values, stopped, starting, idle.
    pub fg_muted: Color,
    /// `bg`: the screen.
    pub bg: Color,
    /// `bg.panel`: the focused pane.
    pub bg_panel: Color,
    /// `border`: pane borders.
    pub border: Color,
    /// `border.focus`: the focused pane's border.
    pub border_focus: Color,
    /// `status.working`.
    pub working: Color,
    /// `status.attention`: waiting, quiet, Needs you.
    pub attention: Color,
    /// `status.done`.
    pub done: Color,
    /// `status.error`: errored, error lines, banners.
    pub error: Color,
    /// `accent.duo`: Duo's glyph and name.
    pub accent_duo: Color,
    /// `selection`: the selected row's background.
    pub selection: Color,
    /// The focused pane's border lines; `border.focus` itself colours its title.
    pub border_focused: Color,
    /// The selected row is bold (`auto`, which paints no background behind it).
    pub bold_marks: bool,
    /// Status colour goes on the glyph only; the word is `fg.muted` (P2-D27).
    pub glyph_only: bool,
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "each cast keeps one byte of the 0xRRGGBB value on purpose"
)]
const fn hex(rgb: u32) -> Color {
    Color::Rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

impl Palette {
    /// The default dark theme.
    pub fn dark() -> Self {
        let fg = hex(0xD4D4D8);
        let bg = hex(0x111113);
        let border = hex(0x2A2A2E);
        Self {
            fg,
            fg_muted: hex(0x8E8E99),
            bg,
            bg_panel: bg,
            border,
            border_focus: hex(0x7DA3E0),
            working: fg,
            attention: hex(0xD9A94E),
            done: hex(0x6DB585),
            error: hex(0xE07A7A),
            accent_duo: fg,
            selection: hex(0x1F2430),
            border_focused: border,
            bold_marks: false,
            glyph_only: true,
        }
    }

    /// The default light theme.
    pub fn light() -> Self {
        let fg = hex(0x27272A);
        let bg = hex(0xFAFAFA);
        let border = hex(0xE4E4E7);
        Self {
            fg,
            fg_muted: hex(0x5E5E68),
            bg,
            bg_panel: bg,
            border,
            border_focus: hex(0x2F5BB8),
            working: fg,
            attention: hex(0x835B10),
            done: hex(0x2A6E43),
            error: hex(0xA8323A),
            accent_duo: fg,
            selection: hex(0xECEFF5),
            border_focused: border,
            bold_marks: false,
            glyph_only: true,
        }
    }

    /// `auto`, the default (UI §2.3, P2-D26): the terminal's own background and
    /// foreground; one accent (ANSI blue) for focus; ANSI yellow, green and red on the
    /// glyphs of states that need you; dim muted text and borders; bold emphasis.
    pub fn auto() -> Self {
        Self {
            fg: Color::Reset,
            fg_muted: DIM_MUTED,
            bg: Color::Reset,
            bg_panel: Color::Reset,
            border: DIM_BORDER,
            border_focus: Color::Blue,
            working: Color::Reset,
            attention: Color::Yellow,
            done: Color::Green,
            error: Color::Red,
            accent_duo: Color::Reset,
            selection: Color::Reset,
            border_focused: DIM_BORDER,
            bold_marks: true,
            glyph_only: true,
        }
    }

    /// The selected row's style: its background, and bold where the palette says.
    pub fn selected(&self) -> Style {
        self.bold(Style::default().bg(self.selection))
    }

    /// `style`, bold where the palette marks the selected row with bold.
    pub fn bold(&self, style: Style) -> Style {
        if self.bold_marks {
            style.add_modifier(Modifier::BOLD)
        } else {
            style
        }
    }

    /// `NO_COLOR`: the terminal's own colours; glyphs, words and borders carry the meaning.
    pub fn mono() -> Self {
        Self {
            fg: Color::Reset,
            fg_muted: Color::Reset,
            bg: Color::Reset,
            bg_panel: Color::Reset,
            border: Color::Reset,
            border_focus: Color::Reset,
            working: Color::Reset,
            attention: Color::Reset,
            done: Color::Reset,
            error: Color::Reset,
            accent_duo: Color::Reset,
            selection: Color::Reset,
            border_focused: Color::Reset,
            bold_marks: false,
            glyph_only: false,
        }
    }
}

/// Turns `auto`'s reserved dim colours into the terminal's foreground with `DIM`; a
/// no-op for every other palette. Run once a frame is drawn.
pub(crate) fn finish(buf: &mut Buffer) {
    for cell in &mut buf.content {
        if cell.fg == DIM_MUTED || cell.fg == DIM_BORDER {
            cell.fg = Color::Reset;
            cell.modifier.insert(Modifier::DIM);
        }
    }
}

/// An agent's own colour, kept off `auto`'s two reserved ones by one step of blue.
pub(crate) const fn not_reserved(c: Color) -> Color {
    match c {
        Color::Rgb(3, 1, 4) => Color::Rgb(3, 1, 5),
        Color::Rgb(1, 5, 9) => Color::Rgb(1, 5, 8),
        other => other,
    }
}

/// The palette for a theme; `NO_COLOR` wins over any theme.
pub fn palette_for(theme: Theme, no_color: bool) -> Palette {
    if no_color {
        return Palette::mono();
    }
    match theme {
        Theme::Auto => Palette::auto(),
        Theme::Dark => Palette::dark(),
        Theme::Light => Palette::light(),
    }
}

#[cfg(test)]
mod tests {
    use duohive_core::settings::Theme;
    use ratatui::style::Color;

    use super::*;

    #[test]
    fn dark_and_light_tokens_match_ui_2_3() {
        let d = Palette::dark();
        assert_eq!(
            (d.fg, d.fg_muted, d.bg, d.border, d.border_focus),
            (
                hex(0xD4D4D8),
                hex(0x8E8E99),
                hex(0x111113),
                hex(0x2A2A2E),
                hex(0x7DA3E0)
            )
        );
        assert_eq!(
            (d.attention, d.done, d.error, d.selection),
            (hex(0xD9A94E), hex(0x6DB585), hex(0xE07A7A), hex(0x1F2430))
        );
        let l = Palette::light();
        assert_eq!(
            (l.fg, l.fg_muted, l.bg, l.border, l.border_focus),
            (
                hex(0x27272A),
                hex(0x5E5E68),
                hex(0xFAFAFA),
                hex(0xE4E4E7),
                hex(0x2F5BB8)
            )
        );
        assert_eq!(
            (l.attention, l.done, l.error, l.selection),
            (hex(0x835B10), hex(0x2A6E43), hex(0xA8323A), hex(0xECEFF5))
        );
        // P2-D27: one surface, neutral focused lines, no colour for working or Duo.
        for p in [d, l] {
            assert_eq!((p.bg_panel, p.border_focused), (p.bg, p.border));
            assert_eq!((p.working, p.accent_duo), (p.fg, p.fg));
            assert!(p.glyph_only && !p.bold_marks);
        }
    }

    #[test]
    fn palette_follows_theme_and_no_color() {
        assert_eq!(palette_for(Theme::Auto, false), Palette::auto());
        assert_eq!(palette_for(Theme::Dark, false), Palette::dark());
        assert_eq!(palette_for(Theme::Light, false), Palette::light());
        for t in [Theme::Auto, Theme::Dark, Theme::Light] {
            assert_eq!(palette_for(t, true), Palette::mono());
        }
        let m = Palette::mono();
        assert_eq!(
            (m.fg, m.bg, m.selection, m.error),
            (Color::Reset, Color::Reset, Color::Reset, Color::Reset)
        );
    }
}
