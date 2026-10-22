//! Frame tests: rendered buffers compared with the UI spec's mock-ups.

use duohive_core::names::NodeName;
use duohive_core::org::{AgentDef, Fields, NodeKey};
use duohive_core::settings::Theme;
use duohive_core::status::{AgentState, Status};
use duohive_core::testkit::{example_org, n};
use ratatui::style::Color;

use super::RenderCtx;
use crate::fixtures::{
    cols, ctx, lines_of, mins, model_for, render_buffer, render_lines, spec_model, status,
};
use crate::model::UiModel;
use crate::theme::palette_for;

const SPEC_4_1_ORG: [&str; 13] = [
    "┃ ⬢ Duo · your manager agent    ◦ idle       1m  ┃",
    "┃ ├─ ▣ Payments                 ● 1 ◆ 1          ┃",
    "┃ │  ├─ ◇ Backend               ● 1 ◆ 1          ┃",
    "┃▌│  │  ├─ auth-service         ● working    2m  ┃",
    "┃ │  │  │   \"Running go test ./...\"              ┃",
    "┃ │  │  └─ billing-api          ◆ waiting    4m  ┃",
    "┃ │  │      \"Allow edit to api.go?\"              ┃",
    "┃ │  └─ ◇ QA                                     ┃",
    "┃ │     └─ pay-qa               ○ stopped    1d  ┃",
    "┃ └─ ◇ Generalist              ~● 1 ◷ 1          ┃",
    "┃    ├─ squad-1                ~● working    1m  ┃",
    "┃    ├─ squad-2                 ✓ finished   3m  ┃",
    "┃    └─ squad-3                 ◷ quiet      6m  ┃",
];

#[test]
fn org_pane_matches_spec_4_1() {
    // Last replies show only at ≥ 120×36 (UI §3).
    let lines = render_lines(&spec_model(), &ctx(), 120, 36);
    assert!(
        lines[0].starts_with(" DuoHive   6 agents · ● 2 working · ◆ 2 need you · ? help  "),
        "{}",
        lines[0]
    );
    assert_eq!(
        cols(&lines[1], 50),
        "┏━ Org ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓"
    );
    let got: Vec<String> = lines[2..=14].iter().map(|l| cols(l, 50)).collect();
    assert_eq!(got, SPEC_4_1_ORG);
}

#[test]
fn needs_you_rows_match_spec() {
    let lines = render_lines(&spec_model(), &ctx(), 100, 30);
    // Rows 19..=22 of 30: separator, two items, bottom border; the chat's 6 rows and the
    // footer follow.
    let got: Vec<String> = lines[19..=22].iter().map(|l| cols(l, 50)).collect();
    assert_eq!(
        got,
        [
            "┣━ Needs you (2) ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫",
            "┃ ◷ squad-3      quiet      6m                   ┃",
            "┃ ◆ billing-api  waiting    4m  \"Allow edit to…\" ┃",
            "┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛",
        ]
    );
}

#[test]
fn compact_org_pane_matches_spec() {
    let lines = render_lines(&spec_model(), &ctx(), 80, 24);
    assert!(lines[0].starts_with(" DuoHive  ● 2 · ◆ 2 "), "{}", lines[0]);
    let tree: Vec<String> = lines[1..=12].iter().map(|l| cols(l, 32)).collect();
    assert_eq!(
        tree,
        [
            "┏━ Org ━━━━━━━━━━━━━━━━━━━━━━━━┓",
            "┃ ⬢ Duo                ◦   1m  ┃",
            "┃ ▾ Payments           ● 1 ◆ 1 ┃",
            "┃   ▾ Backend          ● 1 ◆ 1 ┃",
            "┃▌    auth-service     ●   2m  ┃",
            "┃     billing-api      ◆   4m  ┃",
            "┃   ▾ QA                       ┃",
            "┃     pay-qa           ○   1d  ┃",
            "┃ ▾ Generalist        ~● 1 ◷ 1 ┃",
            "┃     squad-1         ~●   1m  ┃",
            "┃     squad-2          ✓   3m  ┃",
            "┃     squad-3          ◷   6m  ┃",
        ]
    );
    // Rows 15..=18 of 24; the chat's 4 rows and the footer follow.
    let bottom: Vec<String> = lines[15..=18].iter().map(|l| cols(l, 32)).collect();
    assert_eq!(
        bottom,
        [
            "┣━ Needs you (2) ━━━━━━━━━━━━━━┫",
            "┃ ◷ squad-3            6m      ┃",
            "┃ ◆ billing-api        4m      ┃",
            "┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛",
        ]
    );
}

#[test]
fn header_counts_match_needs_you() {
    let mut m = spec_model();
    let lines = render_lines(&m, &ctx(), 100, 30);
    assert!(lines[0].starts_with(" DuoHive   6 agents · ● 2 working · ◆ 2 need you"));
    let mut extra = m.needs_you[0].clone();
    extra.agent = n("pay-qa");
    m.needs_you.push(extra);
    let lines = render_lines(&m, &ctx(), 100, 30);
    assert!(
        lines[0].contains(&format!("◆ {} need you", m.needs_you.len())),
        "{}",
        lines[0]
    );
}

fn errored(error: &str) -> Status {
    Status {
        error: Some(error.to_owned()),
        ..status(AgentState::Errored, mins(2))
    }
}

/// The `count` lines under the row that holds `name`, with the pane borders and trailing
/// blanks cut.
fn lines_under(lines: &[String], name: &str, count: usize) -> Vec<String> {
    let i = lines
        .iter()
        .position(|l| cols(l, 50).contains(name))
        .unwrap_or_else(|| panic!("no row for {name}"));
    lines[i + 1..=i + count]
        .iter()
        .map(|l| cols(l, 49).trim_start_matches('┃').trim_end().to_owned())
        .collect()
}

fn line_under(lines: &[String], name: &str) -> String {
    lines_under(lines, name, 1).remove(0)
}

#[test]
fn errors_on_rows_match_spec_4_12() {
    let mut org = example_org();
    org.teams[0].roles[0].agents.push(AgentDef {
        name: n("pay-api"),
        fields: Fields::default(),
    });
    let mut m = model_for(org);
    let set = |m: &mut UiModel, name: &str, f: &dyn Fn(&mut duohive_core::event::AgentView)| {
        f(m.agents.get_mut(&n(name)).unwrap());
    };
    set(&mut m, "Duo", &|v| {
        v.status = errored("opencode not found (code 127)");
    });
    set(&mut m, "auth-service", &|v| {
        v.status = errored("opencode not found · install it or change its CLI (e)");
    });
    set(&mut m, "pay-api", &|v| {
        v.status = errored("exited unexpectedly (code 1)");
    });
    set(&mut m, "billing-api", &|v| {
        v.problem = Some("✕ ~/code/billing not found · e to fix".to_owned());
    });
    set(&mut m, "pay-qa", &|v| {
        v.status = status(AgentState::Working, mins(3));
        v.notice = Some("notice: claude 2.3 untested (tested up to 2.1)".to_owned());
    });
    let lines = render_lines(&m, &ctx(), 100, 40);
    assert_eq!(
        cols(&lines[2], 50),
        "┃▌⬢ Duo · your manager agent    ✕ errored    2m  ┃"
    );
    assert_eq!(
        line_under(&lines, "⬢ Duo"),
        "     opencode not found (code 127) · e Edit org"
    );
    // Too long for the 50-column pane: wrapped after a separator, so the hint stays whole;
    // no hint added where the error carries one.
    assert_eq!(
        lines_under(&lines, "auth-service", 2),
        [
            "   opencode not found ·",
            "   install it or change its CLI (e)"
        ]
        .map(|t| format!(" │  │  │{t}"))
    );
    assert_eq!(
        lines_under(&lines, "pay-api", 2),
        [
            "   exited unexpectedly (code 1) ·",
            "   space retry · s stop"
        ]
        .map(|t| format!(" │  │   {t}"))
    );
    assert_eq!(
        lines_under(&lines, "billing-api", 2),
        ["   ✕ ~/code/billing not found ·", "   e to fix"].map(|t| format!(" │  │  │{t}"))
    );
    // A notice is not an error: cut at the pane edge, like a reply.
    assert_eq!(
        line_under(&lines, "pay-qa"),
        " │         notice: claude 2.3 untested (tested…"
    );
}

#[test]
fn no_color_keeps_heavy_border_and_bar() {
    let mut c = ctx();
    c.palette = palette_for(Theme::Auto, true);
    let buf = render_buffer(&spec_model(), &c, 100, 30);
    assert!(
        buf.content()
            .iter()
            .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset)
    );
    let lines = lines_of(&buf);
    assert!(lines[1].starts_with("┏━ Org"));
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("┃▌│  │  ├─ auth-service"))
    );
    assert_eq!(
        palette_for(Theme::Auto, false),
        crate::theme::Palette::auto()
    );
    assert_eq!(
        palette_for(Theme::Light, false),
        crate::theme::Palette::light()
    );
}

#[test]
fn spinner_animates_only_without_reduce_motion() {
    let row = |c: &RenderCtx| -> String {
        render_lines(&spec_model(), c, 100, 30)
            .into_iter()
            .find(|l| l.contains("├─ auth-service"))
            .unwrap()
    };
    let mut c = ctx();
    c.settings.reduce_motion = false;
    c.spinner = 0;
    assert!(row(&c).contains(" ◐ working    2m"));
    c.spinner = 1;
    assert!(row(&c).contains(" ◓ working    2m"));
    c.settings.reduce_motion = true;
    for spinner in [0, 1] {
        c.spinner = spinner;
        assert!(row(&c).contains(" ● working    2m"));
    }
    // A quiet row never animates.
    c.settings.reduce_motion = false;
    let lines = render_lines(&spec_model(), &c, 100, 30);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("squad-3                 ◷ quiet"))
    );
}

#[test]
fn agent_pane_for_agents_and_groups() {
    let side = |m: &UiModel| -> String {
        render_lines(m, &ctx(), 120, 30)
            .iter()
            .map(|l| l.chars().skip(50).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut m = spec_model();
    let s = side(&m);
    assert!(s.contains("┌─ auth-service · Claude ─"), "{s}");
    m.selected = NodeKey::Agent(n("pay-qa"));
    let s = side(&m);
    assert!(s.contains("○ stopped · space to give work"), "{s}");
    m.selected = NodeKey::Role(duohive_core::org::RoleKey {
        team: Some(NodeName::parse("Payments").unwrap()),
        role: NodeName::parse("Backend").unwrap(),
    });
    let s = side(&m);
    assert!(s.contains("┌─ ◇ Backend · in Payments ─"), "{s}");
    assert!(s.contains("2 agents · ● 1 working · ◆ 1 waiting"), "{s}");
    assert!(s.contains("space give work to this role · e edit"), "{s}");
    m.selected = NodeKey::Team(NodeName::parse("Payments").unwrap());
    assert!(side(&m).contains("3 agents · ● 1 working · ◆ 1 waiting"));
}

#[test]
fn long_errors_wrap_to_three_lines_and_stay_in_view() {
    let mut org = example_org();
    let names: Vec<String> = (1..=30).map(|i| format!("extra-{i}")).collect();
    for name in &names {
        org.roles[0].agents.push(AgentDef {
            name: n(name),
            fields: Fields::default(),
        });
    }
    let mut m = crate::fixtures::model_for(org);
    let long = "exited unexpectedly ".repeat(10) + "(code 1)";
    let last = n("extra-30");
    m.agents.get_mut(&last).unwrap().status = Status {
        error: Some(long),
        ..status(AgentState::Errored, mins(2))
    };
    m.selected = NodeKey::Agent(last);
    let lines = render_lines(&m, &ctx(), 100, 30);
    let i = lines
        .iter()
        .position(|l| cols(l, 50).contains("extra-30"))
        .expect("selected row on screen");
    let detail: Vec<String> = lines[i + 1..=i + 3]
        .iter()
        .map(|l| cols(l, 49).trim_end().to_owned())
        .collect();
    assert!(detail.iter().all(|l| l.contains("exited")), "{detail:#?}");
    assert!(detail[2].ends_with('…'), "{detail:#?}");
    assert!(!cols(&lines[i + 4], 50).contains("exited"));
}

#[test]
fn header_counts_only_org_agents() {
    let mut m = spec_model();
    let stray = m.agents[&n("auth-service")].clone();
    m.agents.insert(n("ghost"), stray);
    let lines = render_lines(&m, &ctx(), 100, 30);
    assert!(
        lines[0].starts_with(" DuoHive   6 agents · ● 2 working"),
        "{}",
        lines[0]
    );
}
