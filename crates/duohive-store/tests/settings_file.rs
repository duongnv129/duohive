use duohive_core::settings::{OrgLayout, Settings, Theme};
use duohive_store::{load_settings, save_settings};

#[test]
fn missing_settings_file_gives_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    let s = load_settings(&tmp.path().join("settings.toml")).unwrap();
    assert_eq!(s.quiet_after_minutes, 5);
    assert_eq!(s, Settings::default());
}

#[test]
fn settings_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("nested/settings.toml");
    let mut s = Settings {
        quiet_after_minutes: 7,
        theme: Theme::Light,
        your_name: Some("Duong".into()),
        ..Default::default()
    };
    s.keys.insert("quit".into(), "ctrl+q".into());
    s.alerts.finished = false;
    save_settings(&path, &s).unwrap();
    assert_eq!(load_settings(&path).unwrap(), s);
}

#[test]
fn invalid_settings_report_line_and_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("settings.toml");
    std::fs::write(&path, "theme = \"dark\"\nquiet_after_mins = 3\n").unwrap();
    let err = load_settings(&path).unwrap_err();
    assert_eq!(err.line, Some(2));
    assert!(err.message.contains("quiet_after_mins"), "{}", err.message);
}

#[test]
fn save_leaves_no_temp_file() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("settings.toml");
    save_settings(&path, &Settings::default()).unwrap();
    let names: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["settings.toml"]);
}

#[test]
fn keys_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("settings.toml");
    let mut s = Settings::default();
    s.keys.insert("nope".into(), "x".into());
    s.keys.insert("zoom".into(), "ctrl+g".into());
    save_settings(&path, &s).unwrap();
    assert_eq!(load_settings(&path).unwrap(), s);
}

#[test]
fn org_layout_round_trips_and_an_old_file_means_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("settings.toml");
    std::fs::write(&path, "theme = \"dark\"\n").unwrap();
    assert_eq!(load_settings(&path).unwrap().org_layout, OrgLayout::Tree);
    let s = Settings {
        org_layout: OrgLayout::Boxes,
        ..Default::default()
    };
    save_settings(&path, &s).unwrap();
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("org_layout = \"boxes\"")
    );
    assert_eq!(load_settings(&path).unwrap(), s);
}
