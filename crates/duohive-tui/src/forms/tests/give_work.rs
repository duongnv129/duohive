//! The Give work dialog (UI §4.5, P2-D10, P2-D11, P2-D20): opening, the task box, the run
//! choice and the four variants.

use super::*;
use crate::focus::{AgentMode, Pane};
use crate::forms::GiveWork;
use crate::tree::tree_rows;
use duohive_core::event::Event;
use duohive_core::worktree::{RunIn, TaskId};

fn key(m: &mut UiModel, k: KeyEvent) -> Vec<UiEffect> {
    match map_key(m, k) {
        Some(a) => update(m, a),
        None => Vec::new(),
    }
}

fn plain(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::NONE)
}

fn space(m: &mut UiModel) -> Vec<UiEffect> {
    key(m, plain(KeyCode::Char(' ')))
}

fn type_text(m: &mut UiModel, s: &str) {
    for c in s.chars() {
        assert_eq!(key(m, plain(KeyCode::Char(c))), []);
    }
}

fn dialog(m: &UiModel) -> &GiveWork {
    match &m.dialog {
        Some(Dialog::GiveWork(g)) => g,
        other => panic!("no give work dialog: {other:?}"),
    }
}

/// billing-api (a git repo, default worktree off) selected in the org pane, in `state`.
fn model_in(state: AgentState) -> UiModel {
    let mut m = model();
    m.agents
        .insert(n("billing-api"), view(status(state, mins(3))));
    m.focus.pane = Pane::Org;
    m
}

fn stopped_model() -> UiModel {
    model_in(AgentState::Stopped)
}

fn with_worktree_default(mut m: UiModel, on: bool) -> UiModel {
    m.org.defaults.worktree = on;
    m
}

fn given(task: &str, run: RunIn) -> Vec<UiEffect> {
    vec![UiEffect::Send(Command::GiveWork {
        agent: n("billing-api"),
        task: task.to_owned(),
        run,
    })]
}

#[test]
fn space_opens_for_agent_only() {
    let mut m = stopped_model();
    assert_eq!(space(&mut m), []);
    assert_eq!(dialog(&m).agent, n("billing-api"));
    // The agent pane in Selected mode opens it too.
    let mut m = stopped_model();
    m.focus.pane = Pane::Agent;
    m.focus.agent_mode = AgentMode::Selected;
    assert_eq!(space(&mut m), []);
    assert_eq!(dialog(&m).agent, n("billing-api"));
    // In Typing mode, space goes to the agent.
    let mut m = model_in(AgentState::Working);
    m.live_agent = Some(n("billing-api"));
    m.focus.pane = Pane::Agent;
    key(&mut m, plain(KeyCode::Enter));
    assert_eq!(m.focus.agent_mode, AgentMode::Typing);
    let out = space(&mut m);
    assert!(matches!(out.as_slice(), [UiEffect::Pane(_)]), "{out:?}");
    assert_eq!(m.dialog, None);
}

#[test]
fn role_team_and_duo_notes() {
    let mut m = stopped_model();
    m.selected = NodeKey::Role(backend());
    assert_eq!(space(&mut m), []);
    assert_eq!(
        m.footer_note.as_deref(),
        Some("select an agent to give work")
    );
    assert_eq!(m.dialog, None);
    for sel in [NodeKey::Team(nn("Payments")), NodeKey::Duo] {
        let mut m = stopped_model();
        m.selected = sel;
        assert_eq!(space(&mut m), []);
        assert_eq!(
            m.footer_note.as_deref(),
            Some("space gives work to an agent or a role")
        );
        assert_eq!(m.dialog, None);
    }
    // Duo in the agent pane.
    let mut m = stopped_model();
    m.selected = NodeKey::Duo;
    m.focus.pane = Pane::Agent;
    assert_eq!(space(&mut m), []);
    assert_eq!(
        m.footer_note.as_deref(),
        Some("space gives work to an agent or a role")
    );
    assert_eq!(m.dialog, None);
}

#[test]
fn space_in_needs_you_does_nothing() {
    let mut m = stopped_model();
    m.focus.pane = Pane::NeedsYou;
    assert_eq!(space(&mut m), []);
    assert_eq!(m.dialog, None);
    assert_eq!(m.footer_note, None);
}

#[test]
fn enter_disabled_when_empty() {
    let mut m = stopped_model();
    space(&mut m);
    assert_eq!(key(&mut m, plain(KeyCode::Enter)), []);
    assert!(m.dialog.is_some());
    type_text(&mut m, "  ");
    assert_eq!(key(&mut m, plain(KeyCode::Enter)), []);
    assert!(m.dialog.is_some());
    // Global letters type into the box; esc closes, never quits.
    type_text(&mut m, "qsz");
    assert_eq!(dialog(&m).task.text(), "  qsz");
    assert_eq!(key(&mut m, plain(KeyCode::Esc)), []);
    assert_eq!(m.dialog, None);
}

#[test]
fn alt_enter_adds_line() {
    let mut m = stopped_model();
    space(&mut m);
    type_text(&mut m, "a");
    key(&mut m, KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
    type_text(&mut m, "b");
    assert_eq!(dialog(&m).task.text(), "a\nb");
    assert_eq!(
        key(&mut m, plain(KeyCode::Enter)),
        given("a\nb", RunIn::Directory)
    );
    assert_eq!(m.dialog, None);
}

#[test]
fn run_preset_from_agent_default() {
    let mut m = with_worktree_default(stopped_model(), true);
    space(&mut m);
    assert_eq!(dialog(&m).run, RunIn::NewWorktree);
    assert!(screen_text(&m).contains("agent default: worktree"));
    type_text(&mut m, "go");
    assert_eq!(
        key(&mut m, plain(KeyCode::Enter)),
        given("go", RunIn::NewWorktree)
    );
    let mut m = with_worktree_default(stopped_model(), false);
    space(&mut m);
    assert_eq!(dialog(&m).run, RunIn::Directory);
    assert!(screen_text(&m).contains("agent default: directory"));
    // tab reaches the radio; space picks the other choice.
    key(&mut m, plain(KeyCode::Tab));
    space(&mut m);
    assert_eq!(dialog(&m).run, RunIn::NewWorktree);
    // The radios read left to right: `new worktree`, then the directory.
    key(&mut m, plain(KeyCode::Right));
    assert_eq!(dialog(&m).run, RunIn::Directory);
    key(&mut m, plain(KeyCode::Left));
    assert_eq!(dialog(&m).run, RunIn::NewWorktree);
    assert_eq!(dialog(&m).task.text(), "");
}

#[test]
fn worktree_radio_disabled_without_git_repo() {
    let mut m = with_worktree_default(stopped_model(), true);
    // auth-service's directory is a plain folder, not a repo.
    m.org.teams[0].roles[0].agents[0].fields.directory = Some("~/notes".into());
    m.selected = NodeKey::Agent(n("auth-service"));
    m.agents.insert(
        n("auth-service"),
        view(status(AgentState::Stopped, mins(1))),
    );
    space(&mut m);
    assert_eq!(dialog(&m).run, RunIn::Directory);
    key(&mut m, plain(KeyCode::Tab));
    space(&mut m);
    assert_eq!(dialog(&m).run, RunIn::Directory);
    let text = screen_text(&m);
    assert!(
        text.contains("new worktree needs a git repo; ~/notes isn't one"),
        "{text}"
    );
}

#[test]
fn errored_title_shows_errored() {
    let mut m = model_in(AgentState::Errored);
    space(&mut m);
    let text = screen_text(&m);
    assert!(
        text.contains("Give work · billing-api · ✕ errored"),
        "{text}"
    );
    assert!(text.contains("Starts a fresh session."), "{text}");
}

#[test]
fn next_turn_variant_shows_where_it_runs() {
    let mut m = model_in(AgentState::Finished);
    space(&mut m);
    let text = screen_text(&m);
    assert!(
        text.contains("Give work · billing-api · ✓ finished"),
        "{text}"
    );
    assert!(
        text.contains("in ~/code/billing, as the next turn of billing-api's session"),
        "{text}"
    );
    type_text(&mut m, "more");
    assert_eq!(
        key(&mut m, plain(KeyCode::Enter)),
        given("more", RunIn::Directory)
    );
    let mut m = model_in(AgentState::Idle);
    m.agents.get_mut(&n("billing-api")).unwrap().worktree = Some(TaskId("flaky-date".into()));
    space(&mut m);
    let text = screen_text(&m);
    assert!(
        text.contains("in worktree flaky-date, as the next turn of billing-api's session"),
        "{text}"
    );
}

#[test]
fn busy_variant_refuses() {
    let mut m = model_in(AgentState::Working);
    space(&mut m);
    type_text(&mut m, "x");
    let text = screen_text(&m);
    assert!(
        text.contains("billing-api is working. Give work when it is at rest."),
        "{text}"
    );
    assert_eq!(key(&mut m, plain(KeyCode::Enter)), []);
    assert!(m.dialog.is_some());
    // It follows the agent: at rest, the same text can be given.
    m.apply(Event::AgentStatusChanged {
        agent: n("billing-api"),
        status: status(AgentState::Idle, mins(0)),
    });
    assert_eq!(
        key(&mut m, plain(KeyCode::Enter)),
        given("x", RunIn::Directory)
    );
}

#[test]
fn pending_variant_says_still_taking() {
    let mut m = model_in(AgentState::Idle);
    m.apply(Event::AgentRunChanged {
        agent: n("billing-api"),
        worktree: None,
        delivery_pending: true,
    });
    space(&mut m);
    type_text(&mut m, "x");
    assert!(screen_text(&m).contains("billing-api is still taking the last task"));
    assert_eq!(key(&mut m, plain(KeyCode::Enter)), []);
    assert!(m.dialog.is_some());
}

#[test]
fn not_taken_reopens_prefilled() {
    let mut m = model_in(AgentState::Idle);
    m.apply(Event::GiveWorkNotTaken {
        agent: n("billing-api"),
        task: "Fix it\nthen run tests".into(),
        reason: "no sign it started".into(),
    });
    assert_eq!(dialog(&m).agent, n("billing-api"));
    assert_eq!(dialog(&m).task.text(), "Fix it\nthen run tests");
    assert!(screen_text(&m).contains("billing-api didn't take the task"));
    assert_eq!(
        key(&mut m, plain(KeyCode::Enter)),
        given("Fix it\nthen run tests", RunIn::Directory)
    );
    // Duo's goes to the chat, not a dialog.
    let mut m = model_in(AgentState::Idle);
    m.apply(Event::GiveWorkNotTaken {
        agent: n("Duo"),
        task: "x".into(),
        reason: "r".into(),
    });
    assert_eq!(m.dialog, None);
}

fn errored_label(error: &str) -> String {
    let mut m = model();
    let mut s = status(AgentState::Errored, mins(5));
    s.error = Some(error.to_owned());
    m.agents.insert(n("billing-api"), view(s));
    let rows = tree_rows(&m, false, false, m.now);
    let at = rows
        .iter()
        .position(|r| r.key == Some(NodeKey::Agent(n("billing-api"))))
        .unwrap();
    rows[at + 1].label.clone()
}

#[test]
fn errored_row_hint_offers_space_retry() {
    assert_eq!(
        errored_label("exited unexpectedly (code 1)"),
        "exited unexpectedly (code 1) · space retry · s stop"
    );
    // The engine's worktree reason already ends in `· space`: no second one.
    assert_eq!(
        errored_label("couldn't create worktree: no room · space"),
        "couldn't create worktree: no room · space · s stop"
    );
}

#[test]
fn give_work_snapshots() {
    let variants: [(&str, UiModel); 3] = [
        ("stopped", {
            let mut m = stopped_model();
            space(&mut m);
            type_text(
                &mut m,
                "Add retry cases for PAY-88. AC: 3 tries, jitter, no double charge.",
            );
            m
        }),
        ("next_turn", {
            let mut m = model_in(AgentState::Finished);
            m.agents.get_mut(&n("billing-api")).unwrap().worktree =
                Some(TaskId("flaky-date".into()));
            space(&mut m);
            type_text(&mut m, "Now make the date test run in UTC as well.");
            m
        }),
        ("busy", {
            let mut m = model_in(AgentState::Working);
            space(&mut m);
            type_text(&mut m, "Also cover the timeout path.");
            m
        }),
    ];
    for (name, m) in &variants {
        for (w, h) in [(80, 24), (160, 48)] {
            let lines = crate::fixtures::render_lines(m, &ctx(), w, h);
            insta::assert_snapshot!(format!("give_work_{name}_{w}x{h}"), lines.join("\n"));
        }
    }
}

fn send(m: &mut UiModel, task: &str) {
    space(m);
    type_text(m, task);
    assert_eq!(key(m, plain(KeyCode::Enter)).len(), 1);
    assert_eq!(m.dialog, None);
}

#[test]
fn rejected_give_work_comes_back_with_its_text() {
    let mut m = with_worktree_default(stopped_model(), true);
    send(&mut m, "go");
    m.apply(Event::CommandRejected {
        reason: "billing-api is getting a worktree ready".into(),
    });
    assert_eq!(dialog(&m).task.text(), "go");
    assert_eq!(dialog(&m).run, RunIn::NewWorktree);
    assert_eq!(
        dialog(&m).note.as_deref(),
        Some("billing-api is getting a worktree ready")
    );
}

#[test]
fn refused_give_work_comes_back_with_its_text() {
    let mut m = model_in(AgentState::Idle);
    send(&mut m, "x");
    m.apply(Event::GiveWorkRefused {
        agent: n("billing-api"),
        state: AgentState::Working,
        pending: false,
    });
    assert_eq!(dialog(&m).task.text(), "x");
    assert_eq!(dialog(&m).note.as_deref(), Some("billing-api is working"));
}

#[test]
fn sent_memory_ends_when_the_agent_leaves_rest() {
    let mut m = model_in(AgentState::Idle);
    send(&mut m, "x");
    m.apply(Event::AgentStatusChanged {
        agent: n("billing-api"),
        status: status(AgentState::Working, mins(0)),
    });
    m.apply(Event::CommandRejected {
        reason: "unrelated".into(),
    });
    assert_eq!(m.dialog, None);
    assert_eq!(m.footer_note.as_deref(), Some("unrelated"));
}

fn not_taken(m: &mut UiModel) {
    m.apply(Event::GiveWorkNotTaken {
        agent: n("billing-api"),
        task: "again".into(),
        reason: "no sign it started".into(),
    });
}

#[test]
fn not_taken_under_another_dialog_waits_for_space() {
    let mut m = stopped_model();
    update(&mut m, Action::Delete);
    assert!(matches!(m.dialog, Some(Dialog::Confirm(_))));
    not_taken(&mut m);
    assert!(matches!(m.dialog, Some(Dialog::Confirm(_))));
    assert_eq!(
        m.footer_note.as_deref(),
        Some("billing-api didn't take the task · space to try again")
    );
    key(&mut m, plain(KeyCode::Esc));
    assert_eq!(m.dialog, None);
    space(&mut m);
    assert_eq!(dialog(&m).task.text(), "again");
}

#[test]
fn not_taken_never_takes_typing_or_a_chat_draft() {
    let mut m = model_in(AgentState::Idle);
    m.live_agent = Some(n("billing-api"));
    m.focus.pane = Pane::Agent;
    m.focus.agent_mode = AgentMode::Typing;
    not_taken(&mut m);
    assert_eq!(m.dialog, None);
    assert!(m.footer_note.is_some());
    m.focus.agent_mode = AgentMode::Selected;
    space(&mut m);
    assert_eq!(dialog(&m).task.text(), "again");

    let mut m = model_in(AgentState::Idle);
    m.focus.pane = Pane::Chat;
    m.chat_input = LineEdit::new("draft");
    not_taken(&mut m);
    assert_eq!(m.dialog, None);
    assert_eq!(m.chat_input.text(), "draft");
    assert!(m.footer_note.is_some());
    m.focus.pane = Pane::Org;
    space(&mut m);
    assert_eq!(dialog(&m).task.text(), "again");
}
