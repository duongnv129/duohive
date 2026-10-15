//! The Settings frame (UI §4.10).

use duohive_core::ports::OrgFileError;
use duohive_core::settings::{Settings, Theme};

use crate::fixtures::{ctx, render_lines, spec_model};
use crate::forms::SettingsState;
use crate::model::{Dialog, UiModel};

fn shown() -> UiModel {
    let mut m = spec_model();
    m.settings = Settings {
        your_name: Some("Duong".to_owned()),
        theme: Theme::Auto,
        ..Settings::default()
    };
    m.settings.alerts.finished = false;
    m.dialog = Some(Dialog::Settings(SettingsState::default()));
    m
}

#[test]
fn settings_matches_the_spec() {
    let text = render_lines(&shown(), &ctx(), 100, 30).join("\n");
    for want in [
        "Settings · saved to settings.toml",
        "You",
        "Your name",
        "[ Duong ]",
        "Attention",
        "Quiet after",
        "[ 5 ] minutes",
        "Desktop alerts",
        "[x] needs you",
        "[ ] finished",
        "Look",
        "Theme",
        "[ auto ▾ ]",
        "Last reply under agents [x]",
        "Reduce motion [ ]",
        "org defaults and Duo: e on Duo (Edit org)",
        "changes save immediately · esc back",
    ] {
        assert!(text.contains(want), "missing {want:?} in\n{text}");
    }
    assert!(!text.contains("Keys"));
}

#[test]
fn broken_file_is_said_in_the_dialog() {
    let mut m = shown();
    m.settings_error = Some(OrgFileError {
        line: Some(3),
        message: "invalid type".to_owned(),
    });
    let text = render_lines(&m, &ctx(), 100, 30).join("\n");
    assert!(text.contains("settings.toml has an error · changes last until DuoHive quits"));
}

#[test]
fn settings_snapshots() {
    let m = shown();
    for (name, w, h) in [("settings_80x24", 80, 24), ("settings_160x48", 160, 48)] {
        insta::assert_snapshot!(name, render_lines(&m, &ctx(), w, h).join("\n"));
    }
}
