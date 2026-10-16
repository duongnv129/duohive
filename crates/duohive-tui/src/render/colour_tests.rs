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
