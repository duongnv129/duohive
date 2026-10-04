//! Frame tests for the side pane, banners and full-screen messages.

use duohive_core::cli_kind::CliKind;
use duohive_core::event::{Event, Snapshot};
use duohive_core::names::NodeName;
use duohive_core::org::{Fields, Org, Role};
use duohive_core::ports::OrgFileError;
use duohive_core::testkit::example_org;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::{BlockingScreen, render_blocking};
use crate::fixtures::{cols, ctx, lines_of, model_for, render_lines, spec_model};
use crate::model::UiModel;

fn snapshot_of(m: &UiModel, reconnecting: u32) -> Event {
    Event::Snapshot(Box::new(Snapshot {
        org: m.org.clone(),
        agents: m.agents.clone(),
        needs_you: m.needs_you.clone(),
        org_error: None,
        settings_error: None,
        reconnecting,
    }))
}

#[test]
fn reconnecting_state_in_side_pane() {
    let mut m = UiModel::new(example_org());
    let spec = spec_model();
    assert!(m.apply(snapshot_of(&spec, 6)));
    let side = |m: &UiModel| -> Vec<String> {
        render_lines(m, &ctx(), 100, 30)
            .iter()
            .map(|l| l.chars().skip(50).collect::<String>().trim().to_owned())
            .collect()
    };
    assert!(
        side(&m)
            .iter()
            .any(|l| l.contains("│   Reconnecting to 6 agents…"))
    );
    assert!(!side(&m).iter().any(|l| l.contains("attach:")));
    assert!(m.apply(Event::Reconnected));
    assert_eq!(m.reconnecting, 0);
    assert!(!side(&m).iter().any(|l| l.contains("Reconnecting")));
    m.apply(snapshot_of(&spec, 1));
    assert!(
        side(&m)
            .iter()
            .any(|l| l.contains("Reconnecting to 1 agent…"))
    );
}

#[test]
fn org_file_banner() {
    let mut m = spec_model();
    m.org_error = Some(OrgFileError {
        line: Some(42),
        message: "unknown CLI \"claud\"".to_owned(),
    });
    let lines = render_lines(&m, &ctx(), 100, 30);
    assert_eq!(
        lines[0].trim_end(),
        " ✕ org.toml line 42: unknown CLI \"claud\" · using your last good org · o open in $EDITOR"
    );
    assert!(lines[1].starts_with(" DuoHive "));
    m.org_error.as_mut().unwrap().line = None;
    let lines = render_lines(&m, &ctx(), 100, 30);
    assert_eq!(
        lines[0].trim_end(),
        " ✕ org.toml: unknown CLI \"claud\" · using your last good org · o open in $EDITOR"
    );
    m.settings_error = Some(OrgFileError {
        line: Some(3),
        message: "invalid type".to_owned(),
    });
    let lines = render_lines(&m, &ctx(), 100, 30);
    assert_eq!(
        lines[1].trim_end(),
        " ✕ settings.toml line 3: invalid type · using default settings"
    );
    assert!(lines[2].starts_with(" DuoHive "));
}

#[test]
fn too_small_screen() {
    for (w, h) in [(79, 24), (100, 23)] {
        let lines = render_lines(&spec_model(), &ctx(), w, h);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Make the window at least 80×24")),
            "{w}×{h}"
        );
        assert!(lines.iter().any(|l| l.contains("◆ 2 need you")), "{w}×{h}");
        assert!(!lines.iter().any(|l| l.contains("Org")), "{w}×{h}");
    }
}

#[test]
fn tmux_missing_screen_matches_spec() {
    let spec = |found: &str| {
        vec![
            "┌─ DuoHive needs tmux ─────────────────────────────────────────────────┐".to_owned(),
            "│                                                                      │".to_owned(),
            format!("│  tmux 3.2 or newer is required. Found: {found:<30}│"),
            "│                                                                      │".to_owned(),
            "│    macOS   brew install tmux                                         │".to_owned(),
            "│    Ubuntu  sudo apt install tmux                                     │".to_owned(),
            "│                                                                      │".to_owned(),
            "│  Agents run in tmux, so they keep working when DuoHive closes.       │".to_owned(),
            "│                                                  [ q Quit ]          │".to_owned(),
            "└──────────────────────────────────────────────────────────────────────┘".to_owned(),
        ]
    };
    for (found, text) in [(None, "none."), (Some("3.1c".to_owned()), "3.1c.")] {
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let s = BlockingScreen::TmuxMissing { found };
        t.draw(|f| render_blocking(f, &s)).unwrap();
        let lines = lines_of(t.backend().buffer());
        let want = spec(text);
        let boxed: Vec<String> = lines
            .iter()
            .map(|l| l.trim().to_owned())
            .filter(|l| l.starts_with(['┌', '│', '└']))
            .collect();
        assert_eq!(boxed, want);
    }
}

#[test]
fn empty_org_and_empty_role() {
    let m = model_for(Org::blank(CliKind::Claude));
    let lines = render_lines(&m, &ctx(), 160, 48);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("│   Your org is empty · n to add a team, role or agent")),
        "{lines:#?}"
    );
    let mut org = example_org();
    org.teams[0].roles.push(Role {
        name: NodeName::parse("Frontend").unwrap(),
        defaults: Fields::default(),
        agents: Vec::new(),
    });
    let lines = render_lines(&model_for(org), &ctx(), 100, 30);
    let tree: Vec<String> = lines.iter().map(|l| cols(l, 50)).collect();
    assert!(tree.contains(&"┃ │  └─ ◇ Frontend                               ┃".to_owned()));
    assert!(tree.contains(&"┃ │      no agents yet · n to add one            ┃".to_owned()));
}
