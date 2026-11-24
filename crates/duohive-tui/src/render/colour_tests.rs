//! Quiet colour in every theme (UI §2.3, P2-D27): one accent for focus, colour only on
//! the glyphs of states that need you, Duo bold, neutral borders.

use duohive_core::settings::Theme;
use ratatui::buffer::{Buffer, Cell};
use ratatui::style::Modifier;

use crate::fixtures::{ctx, lines_of, render_buffer, spec_model};
use crate::focus::Pane;
use crate::render::RenderCtx;
use crate::theme::palette_for;

const THEMES: [Theme; 3] = [Theme::Auto, Theme::Dark, Theme::Light];

fn ctx_for(theme: Theme) -> RenderCtx {
    RenderCtx {
        palette: palette_for(theme, false),
        ..ctx()
    }
}

fn screen(theme: Theme, focus: Pane) -> Buffer {
    let mut m = spec_model();
    m.focus.pane = focus;
    render_buffer(&m, &ctx_for(theme), 160, 48)
}

/// The cell of the first `needle` on the first row holding `row_text`.
fn cell_at<'a>(buf: &'a Buffer, row_text: &str, needle: &str) -> &'a Cell {
    let lines = lines_of(buf);
    let y = lines.iter().position(|l| l.contains(row_text)).unwrap();
    let i = lines[y].find(needle).unwrap();
    let x = lines[y][..i].chars().count();
    &buf[(u16::try_from(x).unwrap(), u16::try_from(y).unwrap())]
}

/// `cell` shows the palette's muted colour: `fg.muted`, or dim under `auto`.
fn is_muted(theme: Theme, cell: &Cell) -> bool {
    match theme {
        Theme::Auto => cell.modifier.contains(Modifier::DIM),
        _ => cell.fg == palette_for(theme, false).fg_muted,
    }
}

#[test]
fn working_glyph_is_fg_and_word_is_muted() {
    for theme in THEMES {
        let buf = screen(theme, Pane::NeedsYou);
        let p = palette_for(theme, false);
        let glyph = cell_at(&buf, "├─ auth-service", "●");
        assert_eq!(glyph.fg, p.fg, "{theme:?}");
        let word = cell_at(&buf, "├─ auth-service", "working");
        assert!(is_muted(theme, word), "{theme:?}: {word:?}");
    }
}

#[test]
fn attention_colours_only_its_glyph() {
    for theme in THEMES {
        let buf = screen(theme, Pane::NeedsYou);
        let p = palette_for(theme, false);
        assert_eq!(cell_at(&buf, "◆ waiting", "◆").fg, p.attention, "{theme:?}");
        let word = cell_at(&buf, "◆ waiting", "waiting");
        assert!(is_muted(theme, word), "{theme:?}: {word:?}");
    }
}

#[test]
fn duo_is_bold_without_colour() {
    for theme in THEMES {
        let buf = screen(theme, Pane::NeedsYou);
        let duo = cell_at(&buf, "⬢ Duo", "⬢");
        assert_eq!(duo.fg, palette_for(theme, false).fg, "{theme:?}");
        assert!(duo.modifier.contains(Modifier::BOLD), "{theme:?}: {duo:?}");
    }
}

#[test]
fn focused_border_is_border_and_its_title_is_the_accent() {
    for theme in THEMES {
        let buf = screen(theme, Pane::Org);
        let p = palette_for(theme, false);
        let corner = cell_at(&buf, "┏━ Org", "┏");
        match theme {
            Theme::Auto => assert!(corner.modifier.contains(Modifier::DIM)),
            _ => assert_eq!(corner.fg, p.border, "{theme:?}"),
        }
        let title = cell_at(&buf, "┏━ Org", "Org");
        assert_eq!(title.fg, p.border_focus, "{theme:?}");
        assert!(
            title.modifier.contains(Modifier::BOLD),
            "{theme:?}: {title:?}"
        );
    }
}

#[test]
fn only_auto_bolds_the_selected_row() {
    for theme in THEMES {
        let buf = screen(theme, Pane::Org);
        let row = cell_at(&buf, "├─ auth-service", "auth-service");
        assert_eq!(
            row.modifier.contains(Modifier::BOLD),
            theme == Theme::Auto,
            "{theme:?}"
        );
        assert_eq!(row.bg, palette_for(theme, false).selection, "{theme:?}");
    }
}

#[test]
fn one_surface_colour() {
    for theme in [Theme::Dark, Theme::Light] {
        let buf = screen(theme, Pane::Chat);
        let p = palette_for(theme, false);
        let painted = buf
            .content
            .iter()
            .filter(|c| c.bg != p.bg && c.bg != p.selection);
        assert_eq!(painted.count(), 0, "{theme:?}");
    }
}

// ---- team hues, the Manager colour and the pill table (UI §2.3, §6) ----

mod tokens {
    use duohive_core::colour::TeamColour;
    use ratatui::style::Color;

    use crate::theme::{Palette, Pill, palette_for};
    use duohive_core::settings::Theme;

    fn rgb(c: Color) -> (f64, f64, f64) {
        let Color::Rgb(r, g, b) = c else {
            panic!("not an RGB colour: {c:?}")
        };
        (f64::from(r), f64::from(g), f64::from(b))
    }

    fn luminance(c: Color) -> f64 {
        let (r, g, b) = rgb(c);
        let lin = |v: f64| {
            let s = v / 255.0;
            if s <= 0.03928 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
    }

    /// The WCAG contrast ratio.
    fn contrast(a: Color, b: Color) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    const WHITE: Color = Color::Rgb(255, 255, 255);

    fn near(got: f64, want: f64, what: &str) {
        assert!((got - want).abs() <= 0.01, "{what}: {got:.2} vs {want:.2}");
    }

    fn dark() -> Palette {
        palette_for(Theme::Dark, false)
    }

    fn light() -> Palette {
        palette_for(Theme::Light, false)
    }

    /// The hue's title text on `bg` and its chip text on the hue: the lower ratio.
    fn hue_ratio(p: &Palette, c: TeamColour, chip_text: Color) -> f64 {
        contrast(p.hue(c), p.bg).min(contrast(chip_text, p.hue(c)))
    }

    #[test]
    fn team_hues_meet_the_contrast_floor() {
        let want_dark = [7.92, 6.53, 6.13, 7.36];
        let want_light = [4.91, 6.88, 7.00, 5.83];
        for (i, c) in TeamColour::ALL.into_iter().enumerate() {
            let d = hue_ratio(&dark(), c, dark().bg);
            near(d, want_dark[i], &format!("dark {c:?}"));
            let l = hue_ratio(&light(), c, WHITE);
            near(l, want_light[i], &format!("light {c:?}"));
            assert!(d >= 4.5 && l >= 4.5);
        }
    }

    /// The lowest ratio over the hues of `fg`-coloured text on the 30% wash.
    fn lowest_on_wash(p: &Palette, text: Color) -> f64 {
        TeamColour::ALL
            .into_iter()
            .map(|c| contrast(text, p.tint(c, 30)))
            .fold(f64::MAX, f64::min)
    }

    #[test]
    fn selected_agent_text_meets_4_5_on_the_wash() {
        near(lowest_on_wash(&dark(), dark().fg), 7.26, "dark fg");
        near(lowest_on_wash(&dark(), WHITE), 10.73, "dark white");
        near(lowest_on_wash(&light(), light().fg), 8.89, "light fg");
    }

    #[test]
    fn manager_and_pills_meet_4_5() {
        near(contrast(dark().manager, dark().bg), 15.17, "dark manager");
        near(
            contrast(light().bg, light().manager),
            7.00,
            "light manager chip",
        );
        // The lowest of the five pills: errored on dark, idle on its 28% fill on light.
        let low = |p: &Palette| {
            [
                Pill::Idle,
                Pill::Starting,
                Pill::Working,
                Pill::Waiting,
                Pill::Quiet,
                Pill::Errored,
                Pill::Stopped,
            ]
            .into_iter()
            .map(|k| {
                let (bg, text) = p.pill(k, false);
                contrast(text, bg)
            })
            .fold(f64::MAX, f64::min)
        };
        near(low(&dark()), 6.50, "dark pills");
        near(low(&light()), 4.56, "light pills");
    }

    #[test]
    fn manager_selected_name_meets_4_5_on_its_wash() {
        near(contrast(WHITE, dark().tint_manager(26)), 9.10, "dark");
        near(
            contrast(light().fg, light().tint_manager(26)),
            9.57,
            "light",
        );
        // Duo's selected tree row: bold fg on the 26% wash.
        near(
            contrast(dark().fg, dark().tint_manager(26)),
            6.16,
            "dark duo row",
        );
        near(
            contrast(light().fg, light().tint_manager(26)),
            9.57,
            "light duo row",
        );
        // Not selected: manager text on its 10% wash.
        near(
            contrast(dark().manager, dark().tint_manager(10)),
            12.13,
            "dark idle card",
        );
        near(
            contrast(light().manager, light().tint_manager(10)),
            6.06,
            "light idle card",
        );
    }

    #[test]
    fn typed_into_attention_and_error_pills_keep_their_usual_text() {
        let l = light();
        for k in [Pill::Waiting, Pill::Quiet, Pill::Errored] {
            let (bg, text) = l.pill(k, true);
            assert_eq!(text, WHITE, "{k:?}");
            assert_eq!(bg, l.pill(k, false).0, "{k:?}");
        }
        near(contrast(WHITE, l.attention), 6.05, "white on #835B10");
        near(contrast(WHITE, l.error), 6.6, "white on #A8323A");
        let d = dark();
        for k in [Pill::Waiting, Pill::Quiet, Pill::Errored] {
            assert_eq!(d.pill(k, true).1, d.bg, "{k:?}");
        }
        // The tints that stand apart from the solid fill (UI §2.3).
        assert_eq!(d.pill(Pill::Idle, true).0, Color::Rgb(0xC9, 0xC1, 0xAB));
        assert_eq!(d.pill(Pill::Stopped, true).0, Color::Rgb(0xCF, 0xC8, 0xB6));
        assert_eq!(d.pill(Pill::Working, true).0, WHITE);
        assert_eq!(l.pill(Pill::Idle, true).0, Color::Rgb(0xE4, 0xDF, 0xD0));
        assert_eq!(l.pill(Pill::Stopped, true).0, Color::Rgb(0xD8, 0xD2, 0xC0));
        near(
            contrast(d.bg, d.pill(Pill::Idle, true).0),
            10.51,
            "dark typed idle",
        );
        near(
            contrast(l.manager, l.pill(Pill::Stopped, true).0),
            4.84,
            "light typed stopped",
        );
    }

    #[test]
    fn auto_maps_hues_to_ansi() {
        let p = palette_for(Theme::Auto, false);
        assert_eq!(p.hue(TeamColour::Teal), Color::Cyan);
        assert_eq!(p.hue(TeamColour::Violet), Color::Magenta);
        assert_eq!(p.hue(TeamColour::Slate), p.fg_muted);
        assert_eq!(p.hue(TeamColour::Orchid), Color::LightMagenta);
        // No wash under auto: the tint is the hue itself, and the painter paints no background.
        assert_eq!(p.tint(TeamColour::Teal, 30), Color::Cyan);
        assert_eq!(p.manager, Color::Reset);
    }

    #[test]
    fn mono_has_no_colour() {
        let p = palette_for(Theme::Dark, true);
        for c in TeamColour::ALL {
            assert_eq!(p.hue(c), Color::Reset);
            assert_eq!(p.tint(c, 30), Color::Reset);
        }
        assert_eq!(p.manager, Color::Reset);
        assert_eq!(p.tint_manager(26), Color::Reset);
        assert_eq!(p.pill(Pill::Errored, false), (Color::Reset, Color::Reset));
    }

    #[test]
    fn blends_round_to_nearest_with_ties_up() {
        // 50% of black over white is 127.5, which rounds up to 128.
        let p = Palette {
            bg: Color::Rgb(0, 0, 0),
            ..dark()
        };
        let Color::Rgb(r, _, _) = p.tint_with(Color::Rgb(255, 255, 255), 50) else {
            panic!("rgb")
        };
        assert_eq!(r, 128);
    }
}
