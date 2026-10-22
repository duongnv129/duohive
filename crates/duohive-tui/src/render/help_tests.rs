//! The `?` key map and the bindings editor (UI §4.10, §4.11).

use crate::fixtures::{ctx, render_lines, spec_model};
use crate::keymap::{ActionId, KeyMapState, KeysState, parse_binding};
use crate::model::Dialog;

fn help_model() -> crate::model::UiModel {
    let mut m = spec_model();
    m.dialog = Some(Dialog::KeyMap(KeyMapState::default()));
    m
}

fn editor_model() -> crate::model::UiModel {
    let mut m = spec_model();
    m.dialog = Some(Dialog::Keys(KeysState::default()));
    m
}

#[test]
fn help_lists_current_bindings() {
    let mut m = help_model();
    let binding = parse_binding("x").expect("x is a key");
    assert_eq!(m.keymap.rebind(ActionId::Zoom, binding), Ok(()));
    let text = render_lines(&m, &ctx(), 160, 48).join("\n");
    let row = text
        .lines()
        .find(|l| l.contains("zoom agent"))
        .expect("zoom agent row");
    assert!(row.contains('x'), "{row}");
    assert!(!row.contains("z zoom"), "{row}");
}

#[test]
fn key_map_160x48() {
    insta::assert_snapshot!(
        "key_map_160x48",
        render_lines(&help_model(), &ctx(), 160, 48).join("\n")
    );
}

#[test]
fn key_map_100x30() {
    insta::assert_snapshot!(
        "key_map_100x30",
        render_lines(&help_model(), &ctx(), 100, 30).join("\n")
    );
}

#[test]
fn key_map_80x24() {
    insta::assert_snapshot!(
        "key_map_80x24",
        render_lines(&help_model(), &ctx(), 80, 24).join("\n")
    );
}

#[test]
fn keys_editor_80x24() {
    insta::assert_snapshot!(
        "keys_editor_80x24",
        render_lines(&editor_model(), &ctx(), 80, 24).join("\n")
    );
}

#[test]
fn keys_editor_conflict_notice() {
    let mut m = editor_model();
    m.dialog = Some(Dialog::Keys(KeysState {
        cursor: 0,
        notice: Some("s is already Start / stop · rebind that first".to_owned()),
        ..KeysState::default()
    }));
    insta::assert_snapshot!(
        "keys_editor_conflict_notice",
        render_lines(&m, &ctx(), 80, 24).join("\n")
    );
}

#[test]
fn keys_editor_ignored_settings_line() {
    let mut m = editor_model();
    m.keys_notices = vec!["ignored in settings.toml: nope · using the default".to_owned()];
    insta::assert_snapshot!(
        "keys_editor_ignored_settings_line",
        render_lines(&m, &ctx(), 80, 24).join("\n")
    );
}
