//! Colour tokens (UI §2.3) and the palettes that map them.

use duohive_core::settings::Theme;
use ratatui::style::Color;

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
}

const fn hex(rgb: u32) -> Color {
    Color::Rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

impl Palette {
    /// The default dark theme.
    pub fn dark() -> Self {
        Self {
            fg: hex(0xD6DAE0),
            fg_muted: hex(0x8B949E),
            bg: hex(0x0F1115),
            bg_panel: hex(0x161A21),
            border: hex(0x2A2F38),
            border_focus: hex(0x6CA0F5),
            working: hex(0x6CA0F5),
            attention: hex(0xF2B33D),
            done: hex(0x3FB950),
            error: hex(0xFF6B61),
            accent_duo: hex(0xC9A227),
            selection: hex(0x1E2738),
        }
    }

    /// The default light theme.
    pub fn light() -> Self {
        Self {
            fg: hex(0x1F2328),
            fg_muted: hex(0x57606A),
            bg: hex(0xFFFFFF),
            bg_panel: hex(0xF6F8FA),
            border: hex(0xD0D7DE),
            border_focus: hex(0x0B5CD5),
            working: hex(0x0B5CD5),
            attention: hex(0x8A5A00),
            done: hex(0x116329),
            error: hex(0xC11C28),
            accent_duo: hex(0x7D5300),
            selection: hex(0xEAF4FF),
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
        }
    }
}

/// The palette for a theme; `Auto` is dark in phase 1, and `NO_COLOR` wins over any theme.
pub fn palette_for(theme: Theme, no_color: bool) -> Palette {
    if no_color {
        return Palette::mono();
    }
    match theme {
        Theme::Auto | Theme::Dark => Palette::dark(),
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
        assert_eq!(d.fg, Color::Rgb(0xD6, 0xDA, 0xE0));
        assert_eq!(d.attention, Color::Rgb(0xF2, 0xB3, 0x3D));
        assert_eq!(d.selection, Color::Rgb(0x1E, 0x27, 0x38));
        let l = Palette::light();
        assert_eq!(l.error, Color::Rgb(0xC1, 0x1C, 0x28));
        assert_eq!(l.accent_duo, Color::Rgb(0x7D, 0x53, 0x00));
    }

    #[test]
    fn palette_follows_theme_and_no_color() {
        assert_eq!(palette_for(Theme::Auto, false), Palette::dark());
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
