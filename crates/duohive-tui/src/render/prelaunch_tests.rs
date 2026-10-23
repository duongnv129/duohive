//! The pre-launch screen matrix (phase 5, Task 5.1; UI §5, §6, §8 item 2): every
//! phase 3–4 screen is drawn at 80×24, 100×30 and 160×48 in each palette (`dark`,
//! `light`, `auto` and the `NO_COLOR` mono one). A screen that draws outside its
//! area panics in the buffer, so drawing at all is the overflow check; the rest
//! asserts that each screen shows something of its own and that `NO_COLOR` really
//! draws without colour. `docs/plans/ux-check-prelaunch.md` maps each screen and
//! principle to these tests.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::adapter::CliPresence;
use duohive_core::cli_kind::CliKind;
use ratatui::style::Color;

use super::RenderCtx;
use super::screens::{BlockingScreen, render_blocking};
use crate::filter::Filter;
use crate::first_run::{FirstRun, Step};
use crate::fixtures::{ctx, lines_of, render_buffer, render_lines, spec_model, spec_worktrees};
use crate::input::{map_key, update};
use crate::keymap::KeysState;
use crate::model::{Dialog, UiModel};
use crate::theme::Palette;

/// The sizes of UI §5: the smallest supported, the middle, a large window.
const SIZES: [(u16, u16); 3] = [(80, 24), (100, 30), (160, 48)];

fn press(m: &mut UiModel, c: char) {
    if let Some(a) = map_key(m, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) {
        update(m, a);
    }
}

/// `spec_model` after the key that opens a screen.
fn after(key: char) -> UiModel {
    let mut m = spec_model();
    m.worktrees = spec_worktrees();
    press(&mut m, key);
    m
}

fn wizard(step: Step) -> UiModel {
    let found = CliPresence::Found {
        version: "1.0.0".to_owned(),
    };
    let mut w = FirstRun::new(
        vec![(CliKind::Claude, found.clone()), (CliKind::Opencode, found)],
        None,
    );
    w.step = step;
    let mut m = spec_model();
    m.wizard = Some(w);
    m
}

/// Every phase 3–4 screen reachable from the model, named. The right-click menu,
/// the model dropdown and the first-run template preview keep their own snapshots
/// (`menu_tests`, `forms/tests/dropdown.rs`, `first_run/tests.rs`).
fn screens() -> Vec<(&'static str, UiModel)> {
    let mut keys = spec_model();
    keys.dialog = Some(Dialog::Keys(KeysState::default()));
    let mut filtered = spec_model();
    filtered.filter = Filter::NeedYou;
    let mut replace = spec_model();
    replace.wizard = Some(FirstRun::replace(Vec::new(), None));
    let mut no_cli = spec_model();
    no_cli.wizard = Some(FirstRun::new(
        vec![
            (CliKind::Claude, CliPresence::Missing),
            (CliKind::Opencode, CliPresence::Missing),
        ],
        None,
    ));
    vec![
        ("main", spec_model()),
        ("settings", after(',')),
        ("key map", after('?')),
        ("key editor", keys),
        ("new chooser", after('n')),
        ("give work", after(' ')),
        ("edit form", after('e')),
        ("worktrees", after('w')),
        ("move to", after('m')),
        ("filter chip", filtered),
        ("first run: you", wizard(Step::You)),
        ("first run: template", wizard(Step::Template)),
        ("first run: form", wizard(Step::Form)),
        ("first run: no cli", no_cli),
        ("replace org", replace),
    ]
}

fn palettes() -> [(&'static str, Palette); 4] {
    [
        ("dark", Palette::dark()),
        ("light", Palette::light()),
        ("auto", Palette::auto()),
        ("no color", Palette::mono()),
    ]
}

fn with(palette: Palette) -> RenderCtx {
    RenderCtx { palette, ..ctx() }
}

#[test]
fn prelaunch_screens_fit_every_size() {
    let base = render_lines(&spec_model(), &ctx(), 100, 30).join("\n");
    for (name, m) in screens() {
        for (w, h) in SIZES {
            for (theme, palette) in palettes() {
                let lines = render_lines(&m, &with(palette), w, h);
                let tag = format!("{name} at {w}x{h} in {theme}");
                assert_eq!(lines.len(), usize::from(h), "{tag}");
                assert!(
                    lines.iter().all(|l| l.chars().count() == usize::from(w)),
                    "{tag}: a line is not {w} wide"
                );
                assert!(!lines.join("").trim().is_empty(), "{tag} drew nothing");
            }
        }
        // A screen other than the main one looks different from it.
        if name != "main" && name != "filter chip" {
            let text = render_lines(&m, &ctx(), 100, 30).join("\n");
            assert_ne!(text, base, "{name} looks like the main screen");
        }
    }
}

#[test]
fn no_color_draws_without_any_colour() {
    for (name, m) in screens() {
        for (w, h) in SIZES {
            let buf = render_buffer(&m, &with(Palette::mono()), w, h);
            for cell in buf.content() {
                for c in [cell.fg, cell.bg] {
                    assert!(
                        !matches!(c, Color::Rgb(..) | Color::Indexed(_)),
                        "{name} at {w}x{h}: a cell is coloured {c:?} under NO_COLOR"
                    );
                }
            }
        }
    }
}

#[test]
fn tmux_screen_fits_every_size() {
    for found in [None, Some("3.1c".to_owned())] {
        for (w, h) in SIZES {
            let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            let screen = BlockingScreen::TmuxMissing {
                found: found.clone(),
            };
            t.draw(|f| render_blocking(f, &screen)).unwrap();
            let text = lines_of(t.backend().buffer()).join("\n");
            assert!(text.contains("tmux"), "{found:?} at {w}x{h}: {text}");
            assert!(text.contains("3.2"), "{found:?} at {w}x{h}: {text}");
        }
    }
}

#[test]
fn the_wizard_names_what_is_missing_when_no_cli_is_found() {
    let m = screens()
        .into_iter()
        .find(|(n, _)| *n == "first run: no cli")
        .map_or_else(spec_model, |(_, m)| m);
    let text = render_lines(&m, &ctx(), 100, 30).join("\n");
    assert!(text.contains("Claude Code"), "{text}");
    assert!(text.contains("OpenCode"), "{text}");
}
