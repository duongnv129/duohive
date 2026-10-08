//! `map_key` and `update`: pure, no terminal; dialogs drawn with `TestBackend`.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::command::{Command, OrgEdit};
use duohive_core::event::Event;
use duohive_core::names::NodeName;
use duohive_core::org::{NodeKey, RoleKey};
use duohive_core::status::AgentState;
use duohive_core::testkit::n;

use super::{Action, UiEffect, map_key, update};
use crate::fixtures::{ctx, mins, render_lines, spec_model, status};
use crate::model::{Confirm, Dialog, UiModel};

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn code(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::NONE)
}

fn agent(name: &str) -> NodeKey {
    NodeKey::Agent(n(name))
}

fn nn(s: &str) -> NodeName {
    NodeName::parse(s).unwrap()
}

fn qa_role() -> NodeKey {
    NodeKey::Role(RoleKey {
        team: Some(nn("Payments")),
        role: nn("QA"),
    })
}

fn send(c: Command) -> Vec<UiEffect> {
    vec![UiEffect::Send(c)]
}

fn confirm(m: &UiModel) -> &Confirm {
    match &m.dialog {
        Some(Dialog::Confirm(c)) => c,
        _ => panic!("no confirmation open"),
    }
}

/// The spec model with auth-service's stop dialog open.
fn stop_dialog_open() -> UiModel {
    let mut m = spec_model();
    assert_eq!(update(&mut m, Action::StartStop), []);
    assert!(m.dialog.is_some());
    m
}

#[test]
fn down_up_skip_reply_lines() {
    let mut m = spec_model();
    assert_eq!(m.selected, agent("auth-service"));
    assert_eq!(update(&mut m, Action::Down), []);
    assert_eq!(m.selected, agent("billing-api"));
    assert_eq!(update(&mut m, Action::Up), []);
    assert_eq!(m.selected, agent("auth-service"));
}

#[test]
fn moving_stops_at_the_ends_and_visits_groups() {
    let mut m = spec_model();
    m.selected = NodeKey::Duo;
    assert_eq!(update(&mut m, Action::Up), []);
    assert_eq!(m.selected, NodeKey::Duo);
    update(&mut m, Action::Down);
    assert_eq!(m.selected, NodeKey::Team(nn("Payments")));
    m.selected = agent("squad-3");
    update(&mut m, Action::Down);
    assert_eq!(m.selected, agent("squad-3"));
}

#[test]
fn selecting_a_finished_agent_sends_view_agent() {
    let mut m = spec_model();
    m.selected = agent("squad-1");
    assert_eq!(
        update(&mut m, Action::Down),
        send(Command::ViewAgent {
            agent: n("squad-2")
        })
    );
    assert_eq!(m.selected, agent("squad-2"));
}

#[test]
fn s_on_stopped_sends_start() {
    let mut m = spec_model();
    m.selected = agent("pay-qa");
    assert_eq!(
        update(&mut m, Action::StartStop),
        send(Command::StartAgent { agent: n("pay-qa") })
    );
    assert_eq!(m.dialog, None);
}

#[test]
fn s_on_errored_sends_stop() {
    let mut m = spec_model();
    let mut errored = status(AgentState::Errored, mins(1));
    errored.error = Some("exited unexpectedly (code 1)".into());
    m.agents.get_mut(&n("pay-qa")).unwrap().status = errored;
    m.selected = agent("pay-qa");
    assert_eq!(
        update(&mut m, Action::StartStop),
        send(Command::StopAgent { agent: n("pay-qa") })
    );
    assert_eq!(m.dialog, None);
}

#[test]
fn s_on_running_opens_stop_confirm_focused_on_cancel() {
    let m = stop_dialog_open();
    let c = confirm(&m);
    assert_eq!(c.title, "Stop agent?");
    assert_eq!(c.lines, ["Stop auth-service? It is working (2m)."]);
    assert_eq!(c.yes_label, "Stop");
    assert!(!c.focus_yes);
    assert_eq!(
        c.on_yes,
        Command::StopAgent {
            agent: n("auth-service")
        }
    );
    let lines = render_lines(&m, &ctx(), 100, 30);
    let text = lines.join("\n");
    assert!(
        text.contains("Stop auth-service? It is working (2m)."),
        "{text}"
    );
    assert!(text.contains("[ y Stop ]   ▌[ n Cancel ]"), "{text}");
    insta::assert_snapshot!("stop_dialog_100x30", text);
}

#[test]
fn stop_dialog_mentions_worktree() {
    let mut m = spec_model();
    m.agents.get_mut(&n("auth-service")).unwrap().worktree =
        Some(duohive_core::worktree::TaskId("retry-loop".to_owned()));
    assert_eq!(update(&mut m, Action::StartStop), []);
    assert_eq!(
        confirm(&m).lines,
        [
            "Stop auth-service? It is working (2m) in a worktree. The worktree and its branch are kept."
        ]
    );
}

#[test]
fn s_on_groups_and_duo() {
    let mut m = spec_model();
    m.selected = NodeKey::Team(nn("Payments"));
    assert_eq!(update(&mut m, Action::StartStop), []);
    assert_eq!(m.footer_note.as_deref(), Some("s starts or stops an agent"));
    assert_eq!(m.dialog, None);
    m.selected = NodeKey::Duo;
    assert_eq!(update(&mut m, Action::StartStop), []);
    assert_eq!(confirm(&m).lines, ["Stop Duo? It is idle (1m)."]);
    assert_eq!(confirm(&m).on_yes, Command::StopAgent { agent: n("Duo") });
}

#[test]
fn y_in_stop_confirm_sends_stop() {
    let mut m = stop_dialog_open();
    assert_eq!(
        update(&mut m, Action::DialogYes),
        send(Command::StopAgent {
            agent: n("auth-service")
        })
    );
    assert_eq!(m.dialog, None);
}

#[test]
fn enter_on_cancel_and_esc_close_without_command() {
    let mut m = stop_dialog_open();
    assert_eq!(update(&mut m, Action::DialogEnter), []);
    assert_eq!(m.dialog, None);
    let mut m = stop_dialog_open();
    assert_eq!(update(&mut m, Action::Esc), []);
    assert_eq!(m.dialog, None);
    let mut m = stop_dialog_open();
    assert_eq!(update(&mut m, Action::DialogNo), []);
    assert_eq!(m.dialog, None);
}

#[test]
fn tab_moves_focus_and_enter_acts_on_it() {
    let mut m = stop_dialog_open();
    assert_eq!(update(&mut m, Action::Tab), []);
    assert!(confirm(&m).focus_yes);
    let text = render_lines(&m, &ctx(), 100, 30).join("\n");
    assert!(text.contains("▌[ y Stop ]    [ n Cancel ]"), "{text}");
    assert_eq!(update(&mut m, Action::BackTab), []);
    assert!(!confirm(&m).focus_yes);
    update(&mut m, Action::Tab);
    assert_eq!(
        update(&mut m, Action::DialogEnter),
        send(Command::StopAgent {
            agent: n("auth-service")
        })
    );
    assert_eq!(m.dialog, None);
}

#[test]
fn keys_under_a_dialog_do_not_reach_the_tree() {
    let mut m = stop_dialog_open();
    assert_eq!(update(&mut m, Action::Down), []);
    assert_eq!(m.selected, agent("auth-service"));
    assert!(m.dialog.is_some());
}

#[test]
fn d_on_running_agent_sets_footer_reason() {
    let mut m = spec_model();
    assert_eq!(update(&mut m, Action::Delete), []);
    assert_eq!(m.footer_note.as_deref(), Some("stop auth-service first"));
    assert_eq!(m.dialog, None);
}

#[test]
fn d_on_role_lists_its_agents() {
    let mut m = spec_model();
    m.selected = qa_role();
    assert_eq!(update(&mut m, Action::Delete), []);
    let c = confirm(&m);
    assert_eq!(c.title, "Delete role?");
    assert_eq!(
        c.lines,
        [
            "Delete role QA in Payments? It removes 1 agent:",
            "  pay-qa (stopped)",
            "Worktrees are kept and stay listed under w.",
        ]
    );
    assert_eq!(c.yes_label, "Delete");
    assert!(!c.focus_yes);
    assert_eq!(c.on_yes, Command::EditOrg(OrgEdit::Delete(qa_role())));
    let text = render_lines(&m, &ctx(), 100, 30).join("\n");
    assert!(
        text.contains("Delete role QA in Payments? It removes 1 agent:"),
        "{text}"
    );
    assert!(text.contains("pay-qa (stopped)"), "{text}");
    assert!(text.contains("[ y Delete ]   ▌[ n Cancel ]"), "{text}");
}

#[test]
fn d_on_agent_team_and_duo() {
    let mut m = spec_model();
    m.selected = agent("pay-qa");
    update(&mut m, Action::Delete);
    assert_eq!(confirm(&m).title, "Delete agent?");
    assert_eq!(confirm(&m).lines, ["Delete agent pay-qa in Payments › QA?"]);
    assert_eq!(
        confirm(&m).on_yes,
        Command::EditOrg(OrgEdit::Delete(agent("pay-qa")))
    );
    let mut m = spec_model();
    m.selected = NodeKey::Team(nn("Payments"));
    assert_eq!(update(&mut m, Action::Delete), []);
    assert_eq!(m.footer_note.as_deref(), Some("stop auth-service first"));
    m.selected = NodeKey::Duo;
    assert_eq!(update(&mut m, Action::Delete), []);
    assert_eq!(m.footer_note.as_deref(), Some("Duo can't be deleted"));
    assert_eq!(m.dialog, None);
}

#[test]
fn d_on_a_stopped_team_lists_every_agent() {
    let mut m = spec_model();
    for a in ["auth-service", "billing-api"] {
        m.agents.get_mut(&n(a)).unwrap().status = status(AgentState::Stopped, mins(1));
    }
    m.selected = NodeKey::Team(nn("Payments"));
    update(&mut m, Action::Delete);
    assert_eq!(confirm(&m).title, "Delete team?");
    assert_eq!(
        confirm(&m).lines,
        [
            "Delete team Payments? It removes 3 agents:",
            "  auth-service (stopped)",
            "  billing-api (stopped)",
            "  pay-qa (stopped)",
            "Worktrees are kept and stay listed under w.",
        ]
    );
}

#[test]
fn v_toggles_replies_and_saves() {
    let mut m = spec_model();
    assert!(m.settings.show_last_reply);
    let effects = update(&mut m, Action::ToggleReplies);
    let mut expected = m.settings.clone();
    expected.show_last_reply = false;
    assert_eq!(effects, send(Command::UpdateSettings(expected)));
    assert!(!m.settings.show_last_reply);
}

#[test]
fn o_opens_org_toml_in_editor() {
    let mut m = spec_model();
    m.org_toml = PathBuf::from("/cfg/org.toml");
    assert_eq!(map_key(&m, key('o')), Some(Action::OpenOrgFile));
    assert_eq!(
        update(&mut m, Action::OpenOrgFile),
        [UiEffect::OpenEditor(PathBuf::from("/cfg/org.toml"))]
    );
}

#[test]
fn q_and_ctrl_c_quit() {
    let mut m = spec_model();
    assert_eq!(map_key(&m, key('q')), Some(Action::Quit));
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(map_key(&m, ctrl_c), Some(Action::Quit));
    assert_eq!(update(&mut m, Action::Quit), [UiEffect::Quit]);
}

#[test]
fn keys_map_by_context() {
    let m = spec_model();
    assert_eq!(map_key(&m, code(KeyCode::Up)), Some(Action::Up));
    assert_eq!(map_key(&m, code(KeyCode::Down)), Some(Action::Down));
    assert_eq!(map_key(&m, key('s')), Some(Action::StartStop));
    assert_eq!(map_key(&m, key('n')), Some(Action::New));
    assert_eq!(map_key(&m, key('e')), Some(Action::Edit));
    assert_eq!(map_key(&m, key('d')), Some(Action::Delete));
    assert_eq!(map_key(&m, key('v')), Some(Action::ToggleReplies));
    assert_eq!(map_key(&m, key('x')), None);
    let m = stop_dialog_open();
    assert_eq!(map_key(&m, key('y')), Some(Action::DialogYes));
    assert_eq!(map_key(&m, key('n')), Some(Action::DialogNo));
    assert_eq!(map_key(&m, code(KeyCode::Enter)), Some(Action::DialogEnter));
    assert_eq!(map_key(&m, code(KeyCode::Esc)), Some(Action::Esc));
    assert_eq!(map_key(&m, code(KeyCode::Tab)), Some(Action::Tab));
    assert_eq!(map_key(&m, code(KeyCode::BackTab)), Some(Action::BackTab));
    assert_eq!(map_key(&m, key('s')), None);
}

#[test]
fn rejection_shows_in_footer_until_next_key() {
    let mut m = spec_model();
    assert!(m.apply(Event::CommandRejected {
        reason: "pay-qa is already running".into(),
    }));
    assert_eq!(m.footer_note.as_deref(), Some("pay-qa is already running"));
    update(&mut m, Action::Down);
    assert_eq!(m.footer_note, None);
    assert!(m.apply(Event::GiveWorkRefused {
        agent: n("auth-service"),
        state: AgentState::Working,
        pending: false,
    }));
    assert_eq!(m.footer_note.as_deref(), Some("auth-service is working"));
    update(&mut m, Action::Esc);
    assert_eq!(m.footer_note, None);
}
