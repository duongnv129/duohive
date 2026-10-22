//! The accessibility re-check (UI §6): contrast, state without colour, `NO_COLOR` and
//! Reduce motion on the phase 3–4 screens, and a key for every action.

use duohive_core::org::NodeKey;
use duohive_core::settings::Settings;
use duohive_core::status::AgentState;
use duohive_core::testkit::n;
use ratatui::style::Color;

use super::RenderCtx;
use crate::filter::Filter;
use crate::first_run::FirstRun;
use crate::fixtures::{ctx, mins, model_for, render_lines, spec_model, status};
use crate::format::{state_cell, status_word};
use crate::forms::MoveTo;
use crate::keymap::{ActionId, KeyMapState, Keymap, KeysState};
use crate::model::{Dialog, UiModel};
use crate::theme::Palette;

/// WCAG relative luminance of an RGB colour.
fn luminance(c: Color) -> f64 {
    let Color::Rgb(r, g, b) = c else {
        panic!("not an RGB colour: {c:?}");
    };
    let lin = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.039_28 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// WCAG contrast ratio of two colours.
fn ratio(a: Color, b: Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[test]
fn contrast_ratios_meet_4_5() {
    for (name, p) in [("dark", Palette::dark()), ("light", Palette::light())] {
        let tokens = [
            ("fg", p.fg),
            ("fg.muted", p.fg_muted),
            ("border.focus", p.border_focus),
            ("status.attention", p.attention),
            ("status.done", p.done),
            ("status.error", p.error),
        ];
        for (token, color) in tokens {
            // The lowest ratio across the screen and the selected row, as UI §6 states it.
            let lowest = ratio(color, p.bg).min(ratio(color, p.selection));
            assert!(lowest >= 4.5, "{name} {token}: {lowest:.2}");
        }
    }
}

#[test]
fn state_never_colour_only() {
    let now = crate::fixtures::NOW;
    let mut seen = Vec::new();
    for state in [
        AgentState::Stopped,
        AgentState::Starting,
        AgentState::Idle,
        AgentState::Working,
        AgentState::Waiting,
        AgentState::Finished,
        AgentState::Errored,
    ] {
        let s = status(state, mins(3));
        let cell = state_cell(&s, now, None);
        assert!(cell.contains(state.glyph()), "{state:?} glyph in {cell:?}");
        assert!(cell.contains(status_word(&s)), "{state:?} word in {cell:?}");
        seen.push((state.glyph(), state.word()));
    }
    // Quiet is a state of its own: another glyph and word, not a colour.
    let mut quiet = status(AgentState::Working, mins(3));
    quiet.quiet = true;
    let cell = state_cell(&quiet, now, None);
    assert!(cell.contains("quiet"), "{cell:?}");
    // Each state differs from every other by glyph or word.
    for (i, a) in seen.iter().enumerate() {
        for b in &seen[i + 1..] {
            assert!(a.0 != b.0 || a.1 != b.1, "{a:?} vs {b:?}");
        }
    }
}

fn plain_ctx(reduce_motion: bool) -> RenderCtx {
    RenderCtx {
        palette: Palette::mono(),
        settings: Settings {
            reduce_motion,
            ..Settings::default()
        },
        ..ctx()
    }
}

/// Every phase 3–4 screen worth re-checking, named.
fn screens() -> Vec<(&'static str, UiModel)> {
    let mut out = Vec::new();
    let mut wizard = spec_model();
    wizard.wizard = Some(FirstRun::new(Vec::new(), None));
    out.push(("first run", wizard));
    let mut replace = spec_model();
    replace.wizard = Some(FirstRun::replace(Vec::new(), None));
    out.push(("replace org", replace));
    let mut keymap = spec_model();
    keymap.dialog = Some(Dialog::KeyMap(KeyMapState::default()));
    out.push(("key map", keymap));
    let mut keys = spec_model();
    keys.dialog = Some(Dialog::Keys(KeysState::default()));
    out.push(("key editor", keys));
    let mut mv = spec_model();
    let picker = MoveTo::open(&mv.org, &NodeKey::Agent(n("squad-1"))).unwrap();
    mv.dialog = Some(Dialog::MoveTo(Box::new(picker)));
    out.push(("move to", mv));
    let mut filtered = spec_model();
    filtered.filter = Filter::NeedYou;
    out.push(("filter", filtered));
    out
}

#[test]
fn no_color_new_screens_readable() {
    for (name, m) in screens() {
        let text = render_lines(&m, &plain_ctx(true), 100, 40).join("\n");
        assert!(!text.trim().is_empty(), "{name} drew nothing");
        // The glyph and the word carry the meaning with no colour at all.
        if name == "filter" {
            assert!(text.contains("filtered: ◆"), "{text}");
            assert!(text.contains("waiting"), "{text}");
        }
        if name == "move to" {
            assert!(text.contains("Move squad-1 to…"), "{text}");
        }
    }
}

#[test]
fn reduce_motion_new_screens() {
    for (name, m) in screens() {
        let mut still = plain_ctx(true);
        still.spinner = 3;
        let a = render_lines(&m, &still, 100, 40);
        still.spinner = 7;
        let b = render_lines(&m, &still, 100, 40);
        assert_eq!(a, b, "{name} changes with the spinner under Reduce motion");
    }
}

#[test]
fn every_action_has_key() {
    let km = Keymap::default();
    let m = {
        let mut m = model_for(duohive_core::testkit::example_org());
        m.dialog = Some(Dialog::KeyMap(KeyMapState::default()));
        m
    };
    let text = render_lines(&m, &ctx(), 120, 60).join("\n");
    for action in ActionId::ALL {
        let label = km.key(action).label();
        assert!(!label.trim().is_empty(), "{action:?} has no default key");
        assert!(
            text.contains(&label),
            "{action:?} ({label}) missing from the key map"
        );
    }
}
