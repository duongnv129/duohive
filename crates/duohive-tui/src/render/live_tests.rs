//! The agent pane's title, its live terminal and Typing mode (UI §4.1.2, P2-D3, P2-D12).

use duohive_core::cli_kind::CliKind;
use duohive_core::event::Event;
use duohive_core::names::{AgentName, NodeName};
use duohive_core::org::{NodeKey, Org, RoleKey};
use duohive_core::status::{AgentState, Status, WaitingOn};
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
fn typing_footer_keeps_its_ends_around_a_note() {
    let mut m = typing_model();
    m.footer_note = Some("pay-qa's session ended".to_owned());
    let lines = render_lines(&m, &ctx(), 80, 24);
    assert_eq!(
        lines[23].trim_end(),
        " TYPING · pay-qa's session ended · ctrl+q stop typing"
    );
}

#[test]
fn typing_footer_drops_space_to_try_again() {
    let mut m = typing_model();
    m.footer_note = Some("billing-api didn't take the task · space to try again".to_owned());
    let lines = render_lines(&m, &ctx(), 120, 36);
    assert_eq!(
        lines[35].trim_end(),
        " TYPING · billing-api didn't take the task · ctrl+q stop typing"
    );
}

#[test]
fn typing_footer_cuts_a_long_note() {
    let mut m = typing_model();
    m.footer_note = Some(
        "billing-api didn't take the task: the worktree could not be created in time".to_owned(),
    );
    let lines = render_lines(&m, &ctx(), 80, 24);
    let footer = lines[23].trim_end();
    assert!(footer.starts_with(" TYPING · billing-api"), "{footer}");
    assert!(footer.ends_with("… · ctrl+q stop typing"), "{footer}");
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

/// Claude's folder-trust dialog as S3.7 saw it: `No, exit` is highlighted.
const TRUST_DIALOG: &[u8] = concat!(
    " Quick safety check: Is this a project you created or one you trust?\r\n",
    "\r\n",
    " ❯ No, exit\r\n",
    "   Yes, I trust this folder\r\n",
    "\r\n",
    " Enter to confirm · Esc to cancel",
)
.as_bytes();

/// billing-api selected, waiting on Claude's folder-trust dialog (P2-D23).
fn trust_model() -> UiModel {
    let mut m = spec_model();
    m.agents.get_mut(&n("billing-api")).unwrap().status = Status {
        waiting_on: Some(WaitingOn::TrustFolder),
        ..status(AgentState::Waiting, mins(4))
    };
    m.selected = NodeKey::Agent(n("billing-api"));
    m
}

#[test]
fn trust_hint_snapshots() {
    let m = trust_model();
    for (name, w, h) in [("trust_hint_80x24", 80, 24), ("trust_hint_160x48", 160, 48)] {
        let term = term_for(&m, w, h, TRUST_DIALOG, (1, 2));
        let screen = LiveScreen {
            agent: &n("billing-api"),
            term: &term,
            first_row: 0,
            no_color: false,
        };
        let lines = render_live_lines(&m, &ctx(), Some(screen), w, h);
        insta::assert_snapshot!(name, lines.join("\n"));
    }
}

#[test]
fn trust_hint_shows_zoomed_and_clears_when_the_state_changes() {
    let hinted = |m: &UiModel| {
        render_lines(m, &ctx(), 160, 48)
            .iter()
            .any(|l| l.contains("choose Yes (↓ then enter) · enter alone exits"))
    };
    let mut m = trust_model();
    assert!(hinted(&m));
    m.zoomed = true;
    m.focus.pane = Pane::Agent;
    assert!(hinted(&m));
    // Another agent selected: its pane has no hint.
    m.zoomed = false;
    m.selected = NodeKey::Agent(n("auth-service"));
    assert!(!hinted(&m));
    m.selected = NodeKey::Agent(n("billing-api"));
    assert!(m.apply(Event::AgentStatusChanged {
        agent: n("billing-api"),
        status: status(AgentState::Working, mins(0)),
    }));
    assert!(!hinted(&m));
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
