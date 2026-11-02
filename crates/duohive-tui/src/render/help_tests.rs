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

#[test]
fn key_map_lists_r_in_the_org_column() {
    let wide = render_lines(&help_model(), &ctx(), 160, 48).join("\n");
    assert!(wide.lines().any(|l| l.contains("r     resume")), "{wide}");
    insta::assert_snapshot!("key_map_resume_160x48", wide);
    // The narrow layout scrolls: scroll to the Org pane group.
    let mut m = spec_model();
    m.dialog = Some(Dialog::KeyMap(KeyMapState { scroll: 12 }));
    let narrow = render_lines(&m, &ctx(), 80, 24).join("\n");
    assert!(narrow.contains("resume"), "{narrow}");
    insta::assert_snapshot!("key_map_resume_80x24", narrow);
}

#[test]
fn keys_screen_lists_resume_and_conflict_check_counts_it() {
    let m = editor_model();
    let at = ActionId::ALL
        .iter()
        .position(|a| *a == ActionId::Resume)
        .unwrap();
    assert_eq!(ActionId::ALL[at - 1], ActionId::StartStop);
    let mut m = m;
    m.dialog = Some(Dialog::Keys(KeysState {
        cursor: at,
        ..KeysState::default()
    }));
    let text = render_lines(&m, &ctx(), 80, 24).join("\n");
    assert!(text.contains("Resume"), "{text}");
    insta::assert_snapshot!("keys_editor_resume_80x24", text);
    // The conflict check counts it: `r` is taken in the Org pane, and `s` is refused for it.
    let mut km = crate::keymap::Keymap::default();
    assert_eq!(
        km.rebind(ActionId::Edit, parse_binding("r").unwrap()),
        Err(crate::keymap::RebindError::Conflict {
            key: "r".to_owned(),
            action: ActionId::Resume
        })
    );
    assert_eq!(
        km.rebind(ActionId::Resume, parse_binding("s").unwrap()),
        Err(crate::keymap::RebindError::Conflict {
            key: "s".to_owned(),
            action: ActionId::StartStop
        })
    );
    assert_eq!(ActionId::from_id("resume"), Some(ActionId::Resume));
    assert_eq!(ActionId::Resume.label(), "Resume");
}
