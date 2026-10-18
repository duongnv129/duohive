//! The Give work dialog (UI §4.5, P2-D10, P2-D11, P2-D20): opening, the task box, the run
//! choice, the agent variants, the role variants and Send as message instead (P3-D28, P3-D29).

use super::*;
use crate::focus::{AgentMode, Pane};
use crate::forms::GiveWork;
use crate::forms::{GiveFocus, Variant};
use crate::tree::tree_rows;
use duohive_core::card::CardId;
use duohive_core::event::{Event, RoleOutcome};
use duohive_core::tool::Given;
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
fn team_and_duo_still_note() {
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
    // A role opens the dialog, from the tree and from its summary.
    for pane in [Pane::Org, Pane::Agent] {
        let mut m = stopped_model();
        m.selected = NodeKey::Role(backend());
        m.focus.pane = pane;
        assert_eq!(space(&mut m), []);
        assert_eq!(m.footer_note, None);
        assert!(
            matches!(dialog(&m).variant, Variant::Role { .. }),
            "{:?}",
            dialog(&m).variant
        );
    }
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
fn busy_variant_follows_the_agent() {
    let mut m = model_in(AgentState::Working);
    space(&mut m);
    type_text(&mut m, "x");
    // At rest, the same text can be given.
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
    assert_eq!(
        dialog(&m).note.as_deref(),
        Some("billing-api didn't take the task · enter to try again")
    );
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
    let variants: [(&str, UiModel); 2] = [
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
fn sent_memory_ends_at_the_next_user_action() {
    let mut m = model_in(AgentState::Idle);
    send(&mut m, "x");
    key(&mut m, plain(KeyCode::Down));
    m.apply(Event::CommandRejected {
        reason: "unrelated".into(),
    });
    assert_eq!(m.dialog, None);
    assert_eq!(m.footer_note.as_deref(), Some("unrelated"));
}

#[test]
fn refusal_after_the_status_change_still_reopens() {
    let mut m = model_in(AgentState::Idle);
    send(&mut m, "x");
    m.apply(Event::AgentStatusChanged {
        agent: n("billing-api"),
        status: status(AgentState::Working, mins(0)),
    });
    m.apply(Event::GiveWorkRefused {
        agent: n("billing-api"),
        state: AgentState::Working,
        pending: false,
    });
    assert_eq!(dialog(&m).task.text(), "x");
    assert_eq!(dialog(&m).note.as_deref(), Some("billing-api is working"));
}

#[test]
fn space_on_another_agent_leaves_the_deferred_task() {
    let mut m = stopped_model();
    update(&mut m, Action::Delete);
    not_taken(&mut m);
    key(&mut m, plain(KeyCode::Esc));
    m.selected = NodeKey::Agent(n("auth-service"));
    space(&mut m);
    assert_eq!(dialog(&m).task.text(), "");
    assert!(m.give_later.is_some());
}

#[test]
fn deferred_reopen_keeps_the_run_choice() {
    let mut m = with_worktree_default(stopped_model(), false);
    space(&mut m);
    type_text(&mut m, "go");
    key(&mut m, plain(KeyCode::Tab));
    space(&mut m);
    assert_eq!(dialog(&m).run, RunIn::NewWorktree);
    assert_eq!(key(&mut m, plain(KeyCode::Enter)).len(), 1);
    m.dialog = Some(Dialog::Chooser(Chooser::for_selection(&m.org, &m.selected)));
    m.apply(Event::CommandRejected {
        reason: "busy".into(),
    });
    key(&mut m, plain(KeyCode::Esc));
    space(&mut m);
    assert_eq!(dialog(&m).task.text(), "go");
    assert_eq!(dialog(&m).run, RunIn::NewWorktree);
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

fn generalist() -> RoleKey {
    RoleKey {
        team: None,
        role: nn("Generalist"),
    }
}

/// The Generalist role selected in the org pane, squad-1..3 in these states and ages.
fn role_model(states: [(AgentState, u64); 3]) -> UiModel {
    let mut m = model();
    for (i, (state, age)) in states.into_iter().enumerate() {
        m.agents.insert(
            n(&format!("squad-{}", i + 1)),
            view(status(state, mins(age))),
        );
    }
    m.selected = NodeKey::Role(generalist());
    m.focus.pane = Pane::Org;
    m
}

fn assigned(role: RoleKey, task: &str, pick: Option<&str>, run: Option<RunIn>) -> Vec<UiEffect> {
    vec![UiEffect::Send(Command::AssignToRole {
        role,
        task: task.to_owned(),
        pick: pick.map(n),
        run,
    })]
}

fn picked(m: &UiModel) -> AgentName {
    match &dialog(m).variant {
        Variant::Role { free, pick, .. } => free[*pick].clone(),
        other => panic!("not the role variant: {other:?}"),
    }
}

/// Backend selected, with both of its agents busy.
fn none_free_model() -> UiModel {
    let mut m = model();
    m.agents.insert(
        n("auth-service"),
        view(status(AgentState::Working, mins(2))),
    );
    m.agents
        .insert(n("billing-api"), view(status(AgentState::Waiting, mins(4))));
    m.selected = NodeKey::Role(backend());
    m.focus.pane = Pane::Org;
    m
}

#[test]
fn role_pick_cycles_free_agents() {
    let mut m = role_model([
        (AgentState::Working, 1),
        (AgentState::Idle, 5),
        (AgentState::Finished, 10),
    ]);
    space(&mut m);
    // At rest longest first (P3-D28).
    match &dialog(&m).variant {
        Variant::Role {
            role,
            free,
            total,
            pick,
        } => {
            assert_eq!(role, &generalist());
            assert_eq!(free, &vec![n("squad-3"), n("squad-2")]);
            assert_eq!((*total, *pick), (3, 0));
        }
        other => panic!("not the role variant: {other:?}"),
    }
    let text = screen_text(&m);
    assert!(
        text.contains("Give work · role Generalist (2 of 3 free)"),
        "{text}"
    );
    assert!(text.contains("Goes to   squad-3 (finished 10m)"), "{text}");
    assert!(text.contains("↑↓ pick another"), "{text}");
    key(&mut m, plain(KeyCode::Down));
    assert_eq!(picked(&m), n("squad-2"));
    assert!(screen_text(&m).contains("Goes to   squad-2 (idle 5m)"));
    key(&mut m, plain(KeyCode::Down));
    assert_eq!(picked(&m), n("squad-3"));
    key(&mut m, plain(KeyCode::Up));
    assert_eq!(picked(&m), n("squad-2"));
    type_text(&mut m, "Fix it");
    assert_eq!(
        key(&mut m, plain(KeyCode::Enter)),
        assigned(generalist(), "Fix it", Some("squad-2"), None)
    );
    // The dialog waits for the engine's answer.
    assert!(m.dialog.is_some());
    m.apply(Event::RoleAssigned {
        role: generalist(),
        task: "Fix it".into(),
        outcome: RoleOutcome::Given {
            agent: n("squad-2"),
            given: Given::TypedIn,
        },
    });
    assert_eq!(m.dialog, None);
    assert_eq!(m.footer_note.as_deref(), Some("squad-2 took the task"));
}

#[test]
fn role_run_row_follows_pick() {
    let mut m = role_model([
        (AgentState::Finished, 10),
        (AgentState::Stopped, 5),
        (AgentState::Working, 1),
    ]);
    let squads = &mut m.org.roles[0].agents;
    squads[0].fields.directory = Some("~/code/my-app".into());
    squads[1].fields.directory = Some("~/code/web".into());
    squads[1].fields.worktree = Some(true);
    space(&mut m);
    // A finished pick: the next-turn line, no radios.
    let text = screen_text(&m);
    assert!(
        text.contains("Run      in ~/code/my-app, as the next turn of squad-1's session"),
        "{text}"
    );
    assert!(!text.contains("new worktree"), "{text}");
    // A stopped pick: the radios, preset from its own default.
    key(&mut m, plain(KeyCode::Down));
    assert_eq!(picked(&m), n("squad-2"));
    assert_eq!(dialog(&m).run, RunIn::NewWorktree);
    let text = screen_text(&m);
    assert!(text.contains("(•) new worktree"), "{text}");
    assert!(text.contains("agent default: worktree"), "{text}");
    assert!(!text.contains("next turn"), "{text}");
    // tab reaches Goes to, then Run; space picks the directory.
    key(&mut m, plain(KeyCode::Tab));
    assert_eq!(dialog(&m).focus, GiveFocus::Pick);
    key(&mut m, plain(KeyCode::Tab));
    assert_eq!(dialog(&m).focus, GiveFocus::Run);
    space(&mut m);
    assert_eq!(dialog(&m).run, RunIn::Directory);
    // Back to the finished pick: Run is no longer a stop.
    key(&mut m, plain(KeyCode::Up));
    assert_eq!(picked(&m), n("squad-1"));
    assert_ne!(dialog(&m).focus, GiveFocus::Run);
    // And to the stopped one again: preset afresh.
    key(&mut m, plain(KeyCode::Down));
    assert_eq!(dialog(&m).run, RunIn::NewWorktree);
    key(&mut m, plain(KeyCode::BackTab));
    assert_eq!(dialog(&m).focus, GiveFocus::Task);
    type_text(&mut m, "go");
    assert_eq!(
        key(&mut m, plain(KeyCode::Enter)),
        assigned(
            generalist(),
            "go",
            Some("squad-2"),
            Some(RunIn::NewWorktree)
        )
    );
}

#[test]
fn no_free_proposes_clone_card() {
    let mut m = none_free_model();
    space(&mut m);
    assert!(matches!(
        dialog(&m).variant,
        Variant::RoleNoneFree { total: 2, .. }
    ));
    let text = screen_text(&m);
    assert!(
        text.contains("Give work · role Payments › Backend (0 of 2 free)"),
        "{text}"
    );
    assert!(
        text.contains("Goes to  No free agent · enter proposes one"),
        "{text}"
    );
    assert!(
        text.contains("         (the approval card holds the task)"),
        "{text}"
    );
    type_text(&mut m, "Add a key");
    assert_eq!(dialog(&m).hint(), "enter propose one · esc");
    assert!(screen_text(&m).contains("enter propose one · esc"));
    assert_eq!(
        key(&mut m, plain(KeyCode::Enter)),
        assigned(backend(), "Add a key", None, None)
    );
    // Sent once: a second enter waits for the answer.
    assert!(m.dialog.is_some());
    assert_eq!(key(&mut m, plain(KeyCode::Enter)), []);
    m.apply(Event::RoleAssigned {
        role: backend(),
        task: "Add a key".into(),
        outcome: RoleOutcome::Card(CardId(7)),
    });
    assert_eq!(m.dialog, None);
    assert_eq!(
        m.footer_note.as_deref(),
        Some("No free agent · a card in the chat proposes one")
    );
}

#[test]
fn send_as_message_queues() {
    let mut m = model_in(AgentState::Working);
    space(&mut m);
    // Nothing to send yet.
    assert_eq!(key(&mut m, plain(KeyCode::Enter)), []);
    type_text(&mut m, "Also cover");
    let text = screen_text(&m);
    for part in [
        "billing-api is working. Give work when it is at rest, or send this as a",
        "typed in when billing-api is at rest.",
        "[ Give work ] (disabled)",
        "[ Send as message instead ]",
    ] {
        assert!(text.contains(part), "{part}\n{text}");
    }
    key(&mut m, plain(KeyCode::Tab));
    assert_eq!(dialog(&m).focus, GiveFocus::SendAsMessage);
    assert!(screen_text(&m).contains("▌[ Send as message instead ]"));
    assert_eq!(
        key(&mut m, plain(KeyCode::Enter)),
        vec![UiEffect::Send(Command::SendMessage {
            to: n("billing-api"),
            text: "Also cover".into(),
        })]
    );
    assert_eq!(m.dialog, None);
    assert_eq!(m.footer_note.as_deref(), Some("→ queued for billing-api"));
}

#[test]
fn pick_not_free_keeps_dialog() {
    let mut m = role_model([
        (AgentState::Working, 1),
        (AgentState::Idle, 5),
        (AgentState::Working, 1),
    ]);
    space(&mut m);
    type_text(&mut m, "x");
    assert_eq!(key(&mut m, plain(KeyCode::Enter)).len(), 1);
    m.apply(Event::AssignRefused {
        role: generalist(),
        task: "x".into(),
        reason: "squad-2 isn't free any more".into(),
    });
    assert_eq!(dialog(&m).task.text(), "x");
    assert_eq!(
        dialog(&m).note.as_deref(),
        Some("squad-2 isn't free any more")
    );
    assert!(screen_text(&m).contains("squad-2 isn't free any more"));
    // It can be sent again.
    assert_eq!(
        key(&mut m, plain(KeyCode::Enter)),
        assigned(generalist(), "x", Some("squad-2"), None)
    );
    // Once closed, a refusal is a footer note.
    key(&mut m, plain(KeyCode::Esc));
    m.apply(Event::AssignRefused {
        role: generalist(),
        task: "x".into(),
        reason: "squad-2 isn't free any more".into(),
    });
    assert_eq!(m.dialog, None);
    assert_eq!(
        m.footer_note.as_deref(),
        Some("squad-2 isn't free any more")
    );
}

#[test]
fn role_dialog_follows_its_agents() {
    let mut m = role_model([
        (AgentState::Working, 1),
        (AgentState::Idle, 5),
        (AgentState::Working, 1),
    ]);
    space(&mut m);
    type_text(&mut m, "x");
    m.apply(Event::AgentStatusChanged {
        agent: n("squad-1"),
        status: status(AgentState::Finished, mins(9)),
    });
    assert!(screen_text(&m).contains("(2 of 3 free)"));
    // The pick stays on the agent it was on.
    assert_eq!(picked(&m), n("squad-2"));
    assert_eq!(dialog(&m).task.text(), "x");
    m.apply(Event::AgentStatusChanged {
        agent: n("squad-2"),
        status: status(AgentState::Working, mins(0)),
    });
    assert_eq!(picked(&m), n("squad-1"));
    m.apply(Event::AgentStatusChanged {
        agent: n("squad-1"),
        status: status(AgentState::Working, mins(0)),
    });
    assert!(matches!(dialog(&m).variant, Variant::RoleNoneFree { .. }));
    assert_eq!(dialog(&m).task.text(), "x");
}

#[test]
fn role_and_send_as_message_snapshots() {
    let variants: [(&str, UiModel); 3] = [
        ("role", {
            let mut m = role_model([
                (AgentState::Working, 1),
                (AgentState::Finished, 3),
                (AgentState::Working, 5),
            ]);
            m.org.roles[0].agents[1].fields.directory = Some("~/code/my-app".into());
            space(&mut m);
            type_text(&mut m, "Fix the flaky date test in billing/time_test.go");
            key(&mut m, plain(KeyCode::Tab));
            m
        }),
        ("role_none_free", {
            let mut m = none_free_model();
            space(&mut m);
            type_text(&mut m, "Add an idempotency key to POST /payments.");
            m
        }),
        ("busy_send_as_message", {
            let mut m = model_in(AgentState::Working);
            space(&mut m);
            type_text(&mut m, "Also cover the timeout path.");
            key(&mut m, plain(KeyCode::Tab));
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
