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
        worktrees: Vec::new(),
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

/// The side pane's part of each line (columns 50 onwards).
fn side_of(lines: &[String]) -> Vec<String> {
    lines.iter().map(|l| l.chars().skip(50).collect()).collect()
}

#[test]
fn attach_hint_is_whole_on_its_own_line() {
    let side = side_of(&render_lines(&spec_model(), &ctx(), 100, 30));
    let i = side
        .iter()
        .position(|l| l.trim_end_matches([' ', '│']).ends_with("attach:"))
        .unwrap_or_else(|| panic!("{side:#?}"));
    assert!(
        side[i + 1].starts_with("│tmux -L duohive attach -t duohive:auth-service "),
        "{:?}",
        side[i + 1]
    );
}

#[test]
fn a_long_attach_command_breaks_inside_without_losing_characters() {
    let long = "billing-invoice-reconciliation-worker-01";
    let mut org = example_org();
    org.roles[0].agents[0].name = duohive_core::testkit::n(long);
    let mut m = model_for(org);
    let name = duohive_core::testkit::n(long);
    m.agents.get_mut(&name).unwrap().status.state = duohive_core::status::AgentState::Working;
    m.selected = duohive_core::org::NodeKey::Agent(name);
    // At 80×24 the side pane is 48 wide: 46 cells from the padding cell to the border.
    let lines = render_lines(&m, &ctx(), 80, 24);
    let side: Vec<String> = lines.iter().map(|l| l.chars().skip(32).collect()).collect();
    let i = side
        .iter()
        .position(|l| l.trim_end_matches([' ', '│']).ends_with("attach:"))
        .unwrap();
    let cmd = format!("tmux -L duohive attach -t duohive:{long}");
    let row = |k: usize| -> String { side[k].chars().skip(1).take(46).collect() };
    let first: String = cmd.chars().take(46).collect();
    let rest: String = cmd.chars().skip(46).collect();
    assert_eq!(row(i + 1), first);
    assert!(row(i + 2).starts_with(&rest), "{:?}", row(i + 2));
}

#[test]
fn banners_wrap_and_keep_their_hint() {
    let mut m = spec_model();
    m.org_error = Some(OrgFileError {
        line: Some(42),
        message: "unknown CLI \"claud\"".to_owned(),
    });
    let lines = render_lines(&m, &ctx(), 80, 24);
    assert_eq!(
        lines[0].trim_end(),
        " ✕ org.toml line 42: unknown CLI \"claud\" · using your last good org ·"
    );
    assert_eq!(lines[1].trim_end(), "   o open in $EDITOR");
    assert!(lines[2].starts_with(" DuoHive "));
}

#[test]
fn needs_you_overflow_shows_how_many_more() {
    let mut m = spec_model();
    let item = m.needs_you[1].clone();
    m.needs_you = vec![item; 12];
    // 100×30: the org pane is 28 rows, 26 inside, so Needs you gets a third (8 rows):
    // 7 items and "… 5 more".
    let lines = render_lines(&m, &ctx(), 100, 30);
    assert_eq!(
        cols(&lines[27], 50).trim_end_matches([' ', '┃']),
        "┃ … 5 more"
    );
    assert!(cols(&lines[26], 50).contains("billing-api"));
}

#[test]
fn unset_values_read_none_everywhere() {
    let mut m = spec_model();
    m.selected = duohive_core::org::NodeKey::Team(NodeName::parse("Payments").unwrap());
    let side = side_of(&render_lines(&m, &ctx(), 120, 30)).join("\n");
    assert!(side.contains("model (none)"), "{side}");
    assert!(side.contains("Prompt    (none)"), "{side}");
    assert!(!side.contains("inherit: none"));
}
