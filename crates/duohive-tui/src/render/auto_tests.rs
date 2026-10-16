//! Theme `auto` follows the terminal (UI §2.3, P2-D26): nothing painted behind the text,
//! the terminal's own foreground and ANSI colours, dim for muted text and borders.

use duohive_core::settings::Theme;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::fixtures::{ctx, render_buffer, spec_model, term_for};
use crate::focus::Pane;
use crate::render::{LiveScreen, RenderCtx, render_live};
use crate::theme::{Palette, palette_for};

fn auto_ctx() -> RenderCtx {
    RenderCtx {
        palette: palette_for(Theme::Auto, false),
        ..ctx()
    }
}

fn cells(buf: &Buffer) -> impl Iterator<Item = &ratatui::buffer::Cell> {
    buf.content.iter()
}

#[test]
fn auto_paints_no_background() {
    for (w, h) in [(80, 24), (160, 48)] {
        let mut m = spec_model();
        m.focus.pane = Pane::NeedsYou;
        let buf = render_buffer(&m, &auto_ctx(), w, h);
        let painted: Vec<_> = cells(&buf).filter(|c| c.bg != Color::Reset).collect();
        assert!(
            painted.is_empty(),
            "{w}x{h}: {} painted cells",
            painted.len()
        );
    }
}

#[test]
fn auto_uses_terminal_foreground_and_ansi() {
    let p = palette_for(Theme::Auto, false);
    assert_eq!(p, Palette::auto());
    assert_eq!(
        (p.fg, p.bg, p.bg_panel, p.selection),
        (Color::Reset, Color::Reset, Color::Reset, Color::Reset)
    );
    // One accent (blue, for focus); colour only for states that need you.
    assert_eq!(
        (
            p.working,
            p.attention,
            p.done,
            p.error,
            p.accent_duo,
            p.border_focus
        ),
        (
            Color::Reset,
            Color::Yellow,
            Color::Green,
            Color::Red,
            Color::Reset,
            Color::Blue
        )
    );
    // Only the terminal's default and ANSI colours reach the screen.
    let buf = render_buffer(&spec_model(), &auto_ctx(), 160, 48);
    for c in cells(&buf) {
        assert!(
            !matches!(c.fg, Color::Rgb(..) | Color::Indexed(_)),
            "{:?} at {:?}",
            c.fg,
            c.symbol()
        );
    }
}

#[test]
fn auto_muted_and_borders_are_dim() {
    let mut m = spec_model();
    m.focus.pane = Pane::Org;
    let buf = render_buffer(&m, &auto_ctx(), 160, 48);
    // The agent pane is not focused: its light border is dim, in the terminal's colour.
    let lines = crate::fixtures::lines_of(&buf);
    let y = lines.iter().position(|l| l.contains("┌─")).unwrap();
    let x = lines[y].chars().position(|c| c == '┌').unwrap();
    let corner = &buf[(u16::try_from(x).unwrap(), u16::try_from(y).unwrap())];
    assert_eq!(corner.fg, Color::Reset);
    assert!(corner.modifier.contains(Modifier::DIM), "{corner:?}");
    // A muted age ("2m" for auth-service) is dim too.
    let row = lines
        .iter()
        .position(|l| l.contains("├─ auth-service"))
        .unwrap();
    let age_x = lines[row]
        .find(" 2m")
        .map(|i| lines[row][..i + 1].chars().count())
        .unwrap_or_else(|| panic!("no age in {:?}", lines[row]));
    let age = &buf[(u16::try_from(age_x).unwrap(), u16::try_from(row).unwrap())];
    assert!(age.modifier.contains(Modifier::DIM), "{age:?}");
}

#[test]
fn auto_selected_row_is_bar_and_bold() {
    let mut m = spec_model();
    m.focus.pane = Pane::Org;
    let buf = render_buffer(&m, &auto_ctx(), 160, 48);
    let lines = crate::fixtures::lines_of(&buf);
    let y = lines
        .iter()
        .position(|l| l.contains("▌") && l.contains("auth-service"))
        .unwrap();
    let y = u16::try_from(y).unwrap();
    let x = u16::try_from(
        lines[usize::from(y)]
            .find("auth-service")
            .map(|i| lines[usize::from(y)][..i].chars().count())
            .unwrap(),
    )
    .unwrap();
    let cell = &buf[(x, y)];
    assert!(cell.modifier.contains(Modifier::BOLD), "{cell:?}");
    assert_eq!(cell.bg, Color::Reset);
}

#[test]
fn auto_leaves_the_live_terminal_alone() {
    // An agent's own colours are never taken for `auto`'s reserved ones: a cell in one
    // of them moves one step of blue, and isn't dimmed.
    let m = spec_model();
    let term = term_for(&m, 160, 48, b"\x1b[38;2;3;1;4mX\x1b[38;2;1;5;9mY", (2, 0));
    let live = LiveScreen {
        agent: m.selected_agent().unwrap(),
        term: &term,
        first_row: 0,
        no_color: false,
    };
    let mut t = Terminal::new(TestBackend::new(160, 48)).unwrap();
    t.draw(|f| render_live(f, &m, &auto_ctx(), Some(live)))
        .unwrap();
    let buf = t.backend().buffer();
    let x = buf.content.iter().position(|c| c.symbol() == "X").unwrap();
    assert_eq!(buf.content[x].fg, Color::Rgb(3, 1, 5));
    assert_eq!(buf.content[x + 1].fg, Color::Rgb(1, 5, 8));
    assert!(!buf.content[x].modifier.contains(Modifier::DIM));
}

#[test]
fn dark_and_light_are_unchanged() {
    assert_eq!(palette_for(Theme::Dark, false), Palette::dark());
    assert_eq!(palette_for(Theme::Light, false), Palette::light());
    let buf = render_buffer(&spec_model(), &ctx(), 160, 48);
    assert!(cells(&buf).all(|c| !c.modifier.contains(Modifier::DIM)));
}
