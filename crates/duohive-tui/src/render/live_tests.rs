//! The agent pane's title, its live terminal and Typing mode (UI §4.1.2, P2-D3, P2-D12).

use duohive_core::cli_kind::CliKind;
use duohive_core::event::Event;
use duohive_core::names::{AgentName, NodeName};
use duohive_core::org::{NodeKey, Org, RoleKey};
use duohive_core::status::AgentState;
use duohive_core::testkit::n;
use duohive_core::worktree::TaskId;

use super::LiveScreen;
use crate::fixtures::{ctx, mins, render_lines, render_live_lines, spec_model, status, term_for};
use crate::focus::{AgentMode, Pane};
use crate::model::UiModel;

fn nn(s: &str) -> NodeName {
    NodeName::parse(s).unwrap()
}

fn set_directory(org: &mut Org, agent: &str, dir: &str) {
    let teams = org.teams.iter_mut().flat_map(|t| t.roles.iter_mut());
    for role in teams.chain(org.roles.iter_mut()) {
        for a in &mut role.agents {
            if a.name == n(agent) {
                a.fields.directory = Some(dir.to_owned());
            }
        }
    }
}

/// The agent pane's top border at 120×36 (the pane starts at column 50).
fn agent_title(m: &UiModel) -> String {
    render_lines(m, &ctx(), 120, 36)[1]
        .chars()
        .skip(50)
        .collect()
}

#[test]
fn title_uses_short_cli_name() {
    let mut m = spec_model();
    set_directory(&mut m.org, "pay-qa", "~/code/pay-qa");
    m.selected = NodeKey::Agent(n("pay-qa"));
    let title = agent_title(&m);
    assert!(
        title.starts_with("┌─ pay-qa · Claude · ~/code/pay-qa ─"),
        "{title}"
    );
    m.org.duo.cli = CliKind::Opencode;
    m.selected = NodeKey::Duo;
    let title = agent_title(&m);
    assert!(
        title.starts_with("┌─ ⬢ Duo · OpenCode · your manager agent ─"),
        "{title}"
    );
}

#[test]
fn title_shows_worktree_or_directory() {
    let mut m = spec_model();
    set_directory(&mut m.org, "auth-service", "~/code/auth");
    assert!(m.apply(Event::AgentRunChanged {
        agent: n("auth-service"),
        worktree: Some(TaskId("retry-loop".into())),
        delivery_pending: false,
    }));
    let title = agent_title(&m);
    assert!(
        title.starts_with("┌─ auth-service · Claude · worktree retry-loop ─"),
        "{title}"
    );
    assert!(m.apply(Event::AgentRunChanged {
        agent: n("auth-service"),
        worktree: None,
        delivery_pending: false,
    }));
    let title = agent_title(&m);
    assert!(
        title.starts_with("┌─ auth-service · Claude · ~/code/auth ─"),
        "{title}"
    );
}

#[test]
fn stopped_agent_shows_give_work_hint() {
    let mut m = spec_model();
    m.selected = NodeKey::Agent(n("pay-qa"));
    let text = render_lines(&m, &ctx(), 120, 36).join("\n");
    assert!(
        text.contains("│   ○ stopped · space to give work"),
        "{text}"
    );
    assert!(!text.contains("s to start"));
}

#[test]
fn typing_title_border_and_footer() {
    let mut m = typing_model();
    let term = term_for(&m, 120, 36, b"$ ", (2, 0));
    let screen = LiveScreen {
        agent: &n("auth-service"),
        term: &term,
        first_row: 0,
        no_color: false,
    };
    let lines = render_live_lines(&m, &ctx(), Some(screen), 120, 36);
    let pane: String = lines[1].chars().skip(50).collect();
    assert!(
        pane.starts_with("┏━ auth-service · Claude · worktree retry-loop · TYPING ━"),
        "{pane}"
    );
    assert_eq!(
        lines[35].trim_end(),
        " TYPING · every key goes to auth-service, ctrl+c too · ctrl+q stop typing"
    );
    m.focus.agent_mode = AgentMode::Selected;
    m.scrolled = true;
    let screen = LiveScreen {
        agent: &n("auth-service"),
        term: &term,
        first_row: 0,
        no_color: false,
    };
    let lines = render_live_lines(&m, &ctx(), Some(screen), 120, 36);
    let pane: String = lines[1].chars().skip(50).collect();
    assert!(
        pane.starts_with("┏━ auth-service · Claude · worktree retry-loop · scrolled ━"),
        "{pane}"
    );
}

/// The agent pane's top border at 80×24 (the pane starts at column 32).
fn compact_title(m: &UiModel) -> String {
    render_lines(m, &ctx(), 80, 24)[1]
        .chars()
        .skip(32)
        .collect()
}

#[test]
fn mode_word_survives_a_long_worktree_name() {
    let mut m = typing_model();
    m.agents.get_mut(&n("auth-service")).unwrap().worktree =
        Some(TaskId("retry-loop-for-the-billing-webhook".into()));
    // Room to spare: the worktree name is cut in the middle, the mode word stays.
    let title = agent_title(&m);
    assert!(
        title.starts_with("┏━ auth-service · Claude · worktree retry-loop-…ing-webhook · TYPING ┓"),
        "{title}"
    );
    // 80×24: no room for the worktree; it goes before the mode word does.
    let title = compact_title(&m);
    assert!(
        title.starts_with("┏━ auth-service · Claude · TYPING ━"),
        "{title}"
    );
    m.focus.agent_mode = AgentMode::Selected;
    m.scrolled = true;
    let title = compact_title(&m);
    assert!(
        title.starts_with("┏━ auth-service · Claude · scrolled ━"),
        "{title}"
    );
}

#[test]
fn mode_word_survives_a_long_directory() {
    let mut m = typing_model();
    m.agents.get_mut(&n("auth-service")).unwrap().worktree = None;
    set_directory(
        &mut m.org,
        "auth-service",
        "~/code/a-very-long-directory-name/auth",
    );
    let title = compact_title(&m);
    assert!(
        title.starts_with("┏━ auth-service · Claude · ~/cod…auth · TYPING ┓"),
        "{title}"
    );
}

#[test]
fn mode_word_survives_a_long_agent_name() {
    let mut m = typing_model();
    let long = n("authentication-service-for-payments-team");
    let teams = m.org.teams.iter_mut().flat_map(|t| t.roles.iter_mut());
    for a in teams.flat_map(|r| r.agents.iter_mut()) {
        if a.name == n("auth-service") {
            a.name = long.clone();
        }
    }
    m.agents
        .insert(long.clone(), m.agents[&n("auth-service")].clone());
    m.selected = NodeKey::Agent(long);
    let title = compact_title(&m);
    assert!(
        title.starts_with("┏━ authenticati…ayments-team · Claude · TYPING ┓"),
        "{title}"
    );
}

#[test]
fn live_screen_of_another_agent_is_not_drawn() {
    let m = typing_model();
    let term = term_for(&m, 120, 36, b"SECRET", (6, 0));
    let other: AgentName = n("billing-api");
    let screen = LiveScreen {
        agent: &other,
        term: &term,
        first_row: 0,
        no_color: false,
    };
    let text = render_live_lines(&m, &ctx(), Some(screen), 120, 36).join("\n");
    assert!(!text.contains("SECRET"), "{text}");
}

/// auth-service in worktree retry-loop, focused and Typing.
fn typing_model() -> UiModel {
    let mut m = spec_model();
    m.agents.get_mut(&n("auth-service")).unwrap().worktree = Some(TaskId("retry-loop".into()));
    m.focus.pane = Pane::Agent;
    m.focus.agent_mode = AgentMode::Typing;
    m
}

const TYPING_SCREEN: &[u8] = "✓ go test ./... (14 passed)\r\n\
✓ Ready for review · 3 files changed\r\n\
\r\n\
❯ also add a test for the third retry"
    .as_bytes();

const DUO_PROMPT: &[u8] = b"opencode wants to run: gh pr list --state open\r\n\
Allow? (y) yes  (n) no";

#[test]
fn agent_pane_typing_snapshots() {
    let m = typing_model();
    for (name, w, h) in [
        ("agent_pane_typing_80x24", 80, 24),
        ("agent_pane_typing_100x30", 100, 30),
        ("agent_pane_typing_160x48", 160, 48),
    ] {
        let term = term_for(&m, w, h, TYPING_SCREEN, (38, 3));
        let screen = LiveScreen {
            agent: &n("auth-service"),
            term: &term,
            first_row: 0,
            no_color: false,
        };
        let lines = render_live_lines(&m, &ctx(), Some(screen), w, h);
        insta::assert_snapshot!(name, lines.join("\n"));
    }
}

#[test]
fn duo_waiting_prompt_snapshots() {
    let mut m = spec_model();
    m.org.duo.cli = CliKind::Opencode;
    m.agents.get_mut(&n("Duo")).unwrap().status = status(AgentState::Waiting, mins(1));
    m.selected = NodeKey::Duo;
    for (name, w, h) in [
        ("duo_waiting_prompt_80x24", 80, 24),
        ("duo_waiting_prompt_160x48", 160, 48),
    ] {
        let term = term_for(&m, w, h, DUO_PROMPT, (22, 1));
        let screen = LiveScreen {
            agent: &n("Duo"),
            term: &term,
            first_row: 0,
            no_color: false,
        };
        let lines = render_live_lines(&m, &ctx(), Some(screen), w, h);
        insta::assert_snapshot!(name, lines.join("\n"));
    }
}

#[test]
fn role_summary_snapshots() {
    let mut m = spec_model();
    m.selected = NodeKey::Role(RoleKey {
        team: Some(nn("Payments")),
        role: nn("Backend"),
    });
    for (name, w, h) in [
        ("role_summary_80x24", 80, 24),
        ("role_summary_160x48", 160, 48),
    ] {
        insta::assert_snapshot!(name, render_lines(&m, &ctx(), w, h).join("\n"));
    }
}
