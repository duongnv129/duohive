//! Quiet colour in every theme (UI §2.3, P2-D27): one accent for focus, colour only on
//! the glyphs of states that need you, Duo bold, neutral borders.

use duohive_core::colour::TeamColour;
use duohive_core::org::NodeKey;
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
        // Select Duo: the selected row's text is all `fg` (UI §4.1.6), so not auth-service.
        let mut m = spec_model();
        m.focus.pane = Pane::NeedsYou;
        m.selected = NodeKey::Duo;
        let buf = render_buffer(&m, &ctx_for(theme), 160, 48);
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
fn the_selected_tree_row_has_a_wash_of_its_hue_and_a_bold_name() {
    for theme in THEMES {
        let buf = screen(theme, Pane::Org);
        let row = cell_at(&buf, "├─ auth-service", "auth-service");
        assert!(row.modifier.contains(Modifier::BOLD), "{theme:?}");
        let p = palette_for(theme, false);
        match theme {
            // The wash is the hue at 30%: Payments is teal.
            Theme::Dark | Theme::Light => {
                assert_eq!(row.bg, p.tint(TeamColour::Teal, 30), "{theme:?}");
            }
            Theme::Auto => assert_eq!(row.bg, p.bg, "{theme:?}"),
        }
    }
}

#[test]
fn one_surface_colour() {
    for theme in [Theme::Dark, Theme::Light] {
        let buf = screen(theme, Pane::Chat);
        let p = palette_for(theme, false);
        // The one selected row (and Duo's) carries a wash of its hue (UI §2.3).
        let washes: Vec<_> = TeamColour::ALL
            .iter()
            .map(|c| p.tint(*c, 30))
            .chain([p.tint_manager(26)])
            .collect();
        let painted = buf
            .content
            .iter()
            .filter(|c| c.bg != p.bg && c.bg != p.selection && !washes.contains(&c.bg));
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

// ---- the tree's hue and selected row (UI §4.1.6, OB.3a) ----

mod tree_hue {
    use super::*;
    use crate::focus::AgentMode;
    use ratatui::style::Color;

    fn shown(theme: Theme, selected: NodeKey, w: u16, h: u16) -> Buffer {
        let mut m = spec_model();
        m.focus.pane = Pane::Org;
        m.selected = selected;
        render_buffer(&m, &ctx_for(theme), w, h)
    }

    fn payments() -> NodeKey {
        NodeKey::Team(duohive_core::names::NodeName::parse("Payments").unwrap())
    }

    /// The org column's cells (the first 50) of the first row whose org text holds `row_text`;
    /// the agent pane's title can hold the same words.
    fn org_row(buf: &Buffer, row_text: &str) -> usize {
        lines_of(buf)
            .iter()
            .position(|l| l.chars().take(50).collect::<String>().contains(row_text))
            .unwrap()
    }

    fn cell_at<'a>(buf: &'a Buffer, row_text: &str, needle: &str) -> &'a Cell {
        let y = org_row(buf, row_text);
        let line: String = lines_of(buf)[y].chars().take(50).collect();
        let i = line.find(needle).unwrap();
        let x = line[..i].chars().count();
        &buf[(u16::try_from(x).unwrap(), u16::try_from(y).unwrap())]
    }

    fn row_cells<'a>(buf: &'a Buffer, row_text: &str) -> Vec<&'a Cell> {
        let y = org_row(buf, row_text);
        (1..buf.area.width.min(50) - 1)
            .map(|x| &buf[(x, u16::try_from(y).unwrap())])
            .collect()
    }

    #[test]
    fn tree_glyphs_and_names_take_the_hue() {
        for theme in [Theme::Dark, Theme::Light, Theme::Auto] {
            // Select an agent so the team and role rows are not selected.
            let buf = shown(
                theme,
                NodeKey::Agent(duohive_core::testkit::n("auth-service")),
                160,
                48,
            );
            let p = palette_for(theme, false);
            let teal = p.hue(TeamColour::Teal);
            for (row, needle) in [
                ("▣ Payments", "▣"),
                ("▣ Payments", "Payments"),
                ("◇ Backend", "◇"),
                ("◇ Backend", "Backend"),
            ] {
                let c = cell_at(&buf, row, needle);
                assert_eq!(c.fg, teal, "{theme:?} {needle}");
                assert!(c.modifier.contains(Modifier::BOLD), "{theme:?} {needle}");
            }
            // The root role is the second sibling: violet.
            let g = cell_at(&buf, "◇ Generalist", "Generalist");
            assert_eq!(g.fg, p.hue(TeamColour::Violet), "{theme:?}");
            // Connectors keep their colour, and Duo's row stays bold fg.
            let connector = cell_at(&buf, "▣ Payments", "├");
            assert_ne!(connector.fg, teal, "{theme:?}");
            let duo = cell_at(&buf, "⬢ Duo", "Duo");
            assert_eq!(duo.fg, p.fg, "{theme:?}");
            assert!(duo.modifier.contains(Modifier::BOLD));
        }
    }

    #[test]
    fn the_compact_tree_tints_too() {
        let buf = shown(
            Theme::Dark,
            NodeKey::Agent(duohive_core::testkit::n("auth-service")),
            80,
            24,
        );
        let p = palette_for(Theme::Dark, false);
        // The compact tree draws `▾` for a team, and the name takes the hue.
        let c = cell_at(&buf, "▾ Payments", "Payments");
        assert_eq!(c.fg, p.hue(TeamColour::Teal));
        let g = cell_at(&buf, "▾ Payments", "▾");
        assert_eq!(g.fg, p.hue(TeamColour::Teal));
    }

    #[test]
    fn the_selected_tree_row_has_the_bar_and_a_30_percent_wash() {
        for theme in [Theme::Dark, Theme::Light] {
            let p = palette_for(theme, false);
            let buf = shown(theme, payments(), 160, 48);
            let bar = cell_at(&buf, "▣ Payments", "▌");
            assert_eq!(bar.fg, p.hue(TeamColour::Teal), "{theme:?}");
            for c in row_cells(&buf, "▣ Payments") {
                assert_eq!(c.bg, p.tint(TeamColour::Teal, 30), "{theme:?}");
            }
            let buf = shown(theme, NodeKey::Duo, 160, 48);
            for c in row_cells(&buf, "⬢ Duo") {
                assert_eq!(c.bg, p.tint_manager(26), "{theme:?}");
            }
        }
    }

    #[test]
    fn the_selected_tree_rows_text_is_fg() {
        for theme in [Theme::Dark, Theme::Light] {
            let p = palette_for(theme, false);
            let buf = shown(
                theme,
                NodeKey::Agent(duohive_core::testkit::n("auth-service")),
                160,
                48,
            );
            let glyph = cell_at(&buf, "├─ auth-service", "●");
            assert_eq!(glyph.fg, p.fg, "{theme:?}");
            let word = cell_at(&buf, "├─ auth-service", "working");
            assert_eq!(word.fg, p.fg, "{theme:?}");
            let name = cell_at(&buf, "├─ auth-service", "auth-service");
            let white = if theme == Theme::Dark {
                Color::Rgb(255, 255, 255)
            } else {
                p.fg
            };
            assert_eq!(name.fg, white, "{theme:?}");
            assert!(name.modifier.contains(Modifier::BOLD));
            // A team row: glyph fg, name bold.
            let buf = shown(theme, payments(), 160, 48);
            assert_eq!(cell_at(&buf, "▣ Payments", "▣").fg, p.fg, "{theme:?}");
            let team = cell_at(&buf, "▣ Payments", "Payments");
            assert_eq!(team.fg, white, "{theme:?}");
            assert!(team.modifier.contains(Modifier::BOLD));
        }
    }

    #[test]
    fn a_selected_tree_row_never_pairs_the_hue_with_its_own_wash() {
        for theme in [Theme::Dark, Theme::Light] {
            let p = palette_for(theme, false);
            for colour in TeamColour::ALL {
                let mut m = spec_model();
                m.focus.pane = Pane::Org;
                m.org.teams[0].color = Some(colour);
                m.selected = payments();
                let buf = render_buffer(&m, &ctx_for(theme), 160, 48);
                for c in row_cells(&buf, "▣ Payments") {
                    let clash = c.fg == p.hue(colour) && c.bg == p.tint(colour, 30);
                    // Only the bar `▌` is the hue; it is a block, not text.
                    assert!(!clash || c.symbol() == "▌", "{theme:?} {colour:?}: {c:?}");
                }
            }
        }
    }

    #[test]
    fn auto_and_mono_paint_no_wash_and_keep_the_bar_and_bold() {
        for (theme, no_color) in [(Theme::Auto, false), (Theme::Dark, true)] {
            let mut m = spec_model();
            m.focus.pane = Pane::Org;
            m.selected = payments();
            let ctx = RenderCtx {
                palette: palette_for(theme, no_color),
                ..ctx()
            };
            let buf = render_buffer(&m, &ctx, 160, 48);
            for c in row_cells(&buf, "▣ Payments") {
                assert_eq!(c.bg, Color::Reset, "{theme:?}/{no_color}");
            }
            assert_eq!(cell_at(&buf, "▣ Payments", "▌").symbol(), "▌");
            assert!(
                cell_at(&buf, "▣ Payments", "Payments")
                    .modifier
                    .contains(Modifier::BOLD)
            );
        }
    }

    #[test]
    fn typing_leaves_the_tree_selection_unchanged() {
        let theme = Theme::Dark;
        let mut m = spec_model();
        m.focus.pane = Pane::Org;
        let before = render_buffer(&m, &ctx_for(theme), 160, 48);
        m.focus.pane = Pane::Agent;
        m.focus.agent_mode = AgentMode::Typing;
        let after = render_buffer(&m, &ctx_for(theme), 160, 48);
        for (a, b) in row_cells(&before, "├─ auth-service")
            .into_iter()
            .zip(row_cells(&after, "├─ auth-service"))
        {
            assert_eq!((a.fg, a.bg, a.modifier), (b.fg, b.bg, b.modifier));
        }
    }
}
