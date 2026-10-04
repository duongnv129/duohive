//! Forms and the chooser (UI §4.4.1–4.4.2, D11, D14): driven through `Form::handle` and
//! `update`, drawn with `TestBackend`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::cli_kind::CliKind;
use duohive_core::command::{AgentDraft, Command, OrgEdit, RoleDraft, TeamDraft};
use duohive_core::event::{AgentView, Event};
use duohive_core::names::{AgentName, NodeName};
use duohive_core::org::{DuoConfig, Fields, NodeKey, Org, RoleKey};
use duohive_core::status::AgentState;
use duohive_core::testkit::{FakeDirs, example_org, n};
use duohive_core::validate::{DirProbe, DirStatus};
use ratatui::buffer::Buffer;

use super::{Chooser, FieldId, FieldKind, Form, FormOutcome, LineEdit};
use crate::fixtures::{ctx, mins, model_for, render_buffer, status, view};
use crate::input::{Action, UiEffect, map_key, paste, update};
use crate::model::{Dialog, SharedDirs, UiModel};

const HOME: &str = "/home/me";
const ORG_PROMPT: &str = "Ask before you delete anything.";
const BACKEND_PROMPT: &str = "API URLs use hyphens. Run make test before done.";
const OWN_PROMPT: &str = "Owns the billing service; invoices and retries.";

fn home() -> PathBuf {
    PathBuf::from(HOME)
}

fn nn(s: &str) -> NodeName {
    NodeName::parse(s).unwrap()
}

fn dirs() -> FakeDirs {
    FakeDirs::new(HashMap::from([
        (PathBuf::from("/home/me/code/billing"), DirStatus::GitRepo),
        (PathBuf::from("/home/me/code/web"), DirStatus::GitRepo),
        (PathBuf::from("/home/me/notes"), DirStatus::Found),
        (PathBuf::from("/home/me/gone"), DirStatus::Missing),
    ]))
}

/// The example org dressed as UI §4.4.1 draws it.
fn org() -> Org {
    let mut o = example_org();
    o.defaults.model = Some("sonnet".into());
    o.defaults.effort = Some("high".into());
    o.defaults.prompt = Some(ORG_PROMPT.into());
    let backend = &mut o.teams[0].roles[0];
    backend.defaults.prompt = Some(BACKEND_PROMPT.into());
    let billing = &mut backend.agents[1];
    billing.fields.directory = Some("~/code/billing".into());
    billing.fields.prompt = Some(OWN_PROMPT.into());
    o
}

fn backend() -> RoleKey {
    RoleKey {
        team: Some(nn("Payments")),
        role: nn("Backend"),
    }
}

fn running() -> AgentView {
    view(status(AgentState::Working, mins(2)))
}

fn stopped() -> AgentView {
    view(status(AgentState::Stopped, mins(60)))
}

fn press(f: &mut Form, org: &Org, a: Action) -> FormOutcome {
    let out = f.handle(a, org);
    f.probe_dir(org, &dirs(), Path::new(HOME));
    out
}

fn typed(f: &mut Form, org: &Org, s: &str) {
    for c in s.chars() {
        assert_eq!(press(f, org, Action::Char(c)), FormOutcome::Continue);
    }
}

/// Empties the focused field.
fn clear(f: &mut Form, org: &Org) {
    press(f, org, Action::End);
    while !f.fields[f.focus].edit.text().is_empty() {
        press(f, org, Action::Backspace);
    }
}

fn text(f: &Form, id: FieldId) -> String {
    f.field(id).unwrap().edit.text().to_owned()
}

fn edit_billing(org: &Org, v: &AgentView) -> Form {
    Form::edit_agent(org, &n("billing-api"), v, &dirs(), Path::new(HOME))
}

/// A model over [`org`] with the fake directories, billing-api running and selected.
fn model() -> UiModel {
    let mut m = model_for(org());
    m.dirs = SharedDirs(Arc::new(dirs()));
    m.home = home();
    m.agents.insert(n("billing-api"), running());
    m.selected = NodeKey::Agent(n("billing-api"));
    m
}

fn form(m: &UiModel) -> &Form {
    match &m.dialog {
        Some(Dialog::Form(f)) => f,
        other => panic!("no form open: {other:?}"),
    }
}

fn form_mut(m: &mut UiModel) -> &mut Form {
    match &mut m.dialog {
        Some(Dialog::Form(f)) => f,
        other => panic!("no form open: {other:?}"),
    }
}

/// Cells `x0..x1` of row `y`, one string per row.
fn rows(buf: &Buffer, x0: u16, x1: u16) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| (x0..x1).map(|x| buf[(x, y)].symbol()).collect())
        .collect()
}

/// The open dialog's box, drawn on a 100×30 screen.
fn box_rows(m: &UiModel, w: u16) -> Vec<String> {
    let buf = render_buffer(m, &ctx(), 100, 30);
    let x0 = (100 - w) / 2;
    rows(&buf, x0, x0 + w)
        .into_iter()
        .skip_while(|r| !r.starts_with('┌'))
        .take_while(|r| !r.starts_with('└'))
        .collect()
}

fn screen_text(m: &UiModel) -> String {
    let buf = render_buffer(m, &ctx(), 100, 30);
    rows(&buf, 0, 100).join("\n")
}

#[test]
fn edit_agent_form_matches_spec_4_4_1() {
    let mut m = model();
    assert_eq!(update(&mut m, Action::Edit), []);
    let got = box_rows(&m, 80);
    let mock: Vec<&str> = include_str!("../../../../docs/ux/ui-spec.md")
        .lines()
        .skip_while(|l| !l.starts_with("┌─ Edit agent"))
        .take_while(|l| !l.starts_with('└'))
        .collect();
    assert_eq!(got.len(), mock.len(), "{got:#?}");
    for (i, (g, want)) in got.iter().zip(&mock).enumerate() {
        if want.contains("Model ") {
            // D14: model and effort are text fields with `inherit:` placeholders.
            assert!(g.contains("Model       [ inherit: sonnet"), "{g}");
            assert!(g.contains("Effort  [ inherit: high"), "{g}");
            let col = |s: &str| s.chars().position(|c| c == 'E');
            assert_eq!(col(g), col(want), "{g}");
        } else {
            // The focus marker `▌` sits left of the focused control.
            assert_eq!(&g.replace('▌', " "), want, "row {i}");
        }
    }
    insta::assert_snapshot!("edit_agent_100x30", screen_text(&m));
}

/// The mock-up starting `┌─ <title>` in UI §4.4.1, without its bottom border.
fn mock(title: &str) -> Vec<String> {
    include_str!("../../../../docs/ux/ui-spec.md")
        .lines()
        .skip_while(|l| !l.starts_with(&format!("┌─ {title}")))
        .take_while(|l| !l.starts_with('└'))
        .map(str::to_owned)
        .collect()
}

/// Every mock-up row not named in `except` appears, in order, among the drawn rows.
fn follows_mock(got: &[String], mock: &[String], except: &[&str]) {
    assert!(mock.len() > 5, "mock-up not found");
    let got: Vec<String> = got.iter().map(|g| g.replace('▌', " ")).collect();
    let mut at = 0;
    for want in mock
        .iter()
        .filter(|w| !except.iter().any(|e| w.contains(e)))
    {
        let found = got[at..].iter().position(|g| g == want);
        assert!(found.is_some(), "missing {want:?} in {got:#?}");
        at += found.unwrap_or(0) + 1;
    }
}

#[test]
fn new_role_and_edit_org_follow_spec_4_4_1() {
    let org = org();
    let mut f = Form::new_role(&org, Some(&nn("Payments")), &dirs(), Path::new(HOME));
    typed(&mut f, &org, "Frontend");
    f.focus_field(FieldId::Directory);
    typed(&mut f, &org, "~/code/web");
    f.focus_field(FieldId::Prompt);
    typed(
        &mut f,
        &org,
        "You build React UI. Follow the design system.",
    );
    f.focus_field(FieldId::Name);
    let mut m = model();
    m.dialog = Some(Dialog::Form(Box::new(f)));
    // D11 adds the Model, Effort and Flags rows; everything else is the mock-up's.
    follows_mock(&box_rows(&m, 72), &mock("New role"), &[]);
    let mut org = org;
    org.duo.cli = CliKind::Opencode;
    m.dialog = Some(Dialog::Form(Box::new(Form::edit_org(&org, &stopped()))));
    // Name is wider than the mock-up's 9 cells (agent names run to 40), model and effort
    // are text (D14), and prompts pad with spaces as the other mock-ups do.
    follows_mock(
        &box_rows(&m, 80),
        &mock("Edit org"),
        &["Name", "CLI ", "Org prompt"],
    );
}

#[test]
fn duplicate_name_error_disables_save() {
    let org = org();
    let mut f = Form::new_agent(&org, &backend(), &dirs(), Path::new(HOME));
    assert_eq!(text(&f, FieldId::Name), "payments-backend-1");
    assert_eq!(f.error_count(), 0);
    clear(&mut f, &org);
    typed(&mut f, &org, "auth-service");
    let name = f.field(FieldId::Name).unwrap();
    assert_eq!(
        name.error.as_deref(),
        Some("an agent named auth-service already exists")
    );
    assert_eq!(f.error_count(), 1);
    assert_eq!(press(&mut f, &org, Action::Save), FormOutcome::Continue);
    let mut m = model();
    m.dialog = Some(Dialog::Form(Box::new(f)));
    let screen = screen_text(&m);
    assert!(
        screen.contains("✕ an agent named auth-service already exists"),
        "{screen}"
    );
    assert!(screen.contains("fix 1 error to save"), "{screen}");
    assert_eq!(update(&mut m, Action::Save), []);
}

#[test]
fn worktree_on_disabled_for_non_git_dir() {
    let org = org();
    let mut f = Form::new_agent(&org, &backend(), &dirs(), Path::new(HOME));
    f.focus_field(FieldId::Directory);
    typed(&mut f, &org, "~/notes");
    let dir = f.field(FieldId::Directory).unwrap();
    assert_eq!(dir.suffix.as_deref(), Some("✓ found"));
    let wt = f.field(FieldId::Worktrees).unwrap();
    assert_eq!(
        wt.note.as_deref(),
        Some("new worktree needs a git repo; ~/notes isn't one")
    );
    // ←→ cycle skips the disabled `on`: inherit → off → inherit.
    f.focus_field(FieldId::Worktrees);
    press(&mut f, &org, Action::Right);
    assert_eq!(f.field(FieldId::Worktrees).unwrap().choice(), "off");
    press(&mut f, &org, Action::Left);
    assert_eq!(
        f.field(FieldId::Worktrees).unwrap().choice(),
        "inherit: off"
    );
    // A git repo enables it.
    f.focus_field(FieldId::Directory);
    clear(&mut f, &org);
    typed(&mut f, &org, "~/code/billing");
    assert_eq!(
        f.field(FieldId::Directory).unwrap().suffix.as_deref(),
        Some("✓ git repo")
    );
    assert_eq!(f.field(FieldId::Worktrees).unwrap().note, None);
    f.focus_field(FieldId::Worktrees);
    press(&mut f, &org, Action::Right);
    assert_eq!(f.field(FieldId::Worktrees).unwrap().choice(), "on");
    // Moving back to a plain directory makes the chosen `on` an error.
    f.focus_field(FieldId::Directory);
    clear(&mut f, &org);
    typed(&mut f, &org, "~/notes");
    assert_eq!(
        f.field(FieldId::Worktrees).unwrap().error.as_deref(),
        Some("new worktree needs a git repo; ~/notes isn't one")
    );
    f.focus_field(FieldId::Directory);
    clear(&mut f, &org);
    typed(&mut f, &org, "~/gone");
    assert_eq!(
        f.field(FieldId::Directory).unwrap().suffix.as_deref(),
        Some("✕ not found")
    );
}

#[test]
fn clearing_a_field_restores_inherit() {
    let org = org();
    let mut f = edit_billing(&org, &stopped());
    f.focus_field(FieldId::Model);
    typed(&mut f, &org, "opus");
    let Some(OrgEdit::EditAgent { draft, .. }) = f.edit(&org) else {
        panic!("no agent edit");
    };
    assert_eq!(draft.fields.model.as_deref(), Some("opus"));
    clear(&mut f, &org);
    let Some(OrgEdit::EditAgent { draft, .. }) = f.edit(&org) else {
        panic!("no agent edit");
    };
    assert_eq!(draft.fields.model, None);
    let model_field = f.field(FieldId::Model).unwrap();
    assert_eq!(model_field.placeholder.as_deref(), Some("inherit: sonnet"));
    let mut m = model();
    m.dialog = Some(Dialog::Form(Box::new(f)));
    let screen = screen_text(&m);
    assert!(screen.contains("Model      ▌[ inherit: sonnet"), "{screen}");
}

#[test]
fn rename_read_only_while_running() {
    let org = org();
    let mut f = edit_billing(&org, &running());
    let name = f.field(FieldId::Name).unwrap();
    assert_eq!(name.kind, FieldKind::ReadOnly);
    assert_eq!(
        name.note.as_deref(),
        Some("rename only while stopped with no worktrees")
    );
    assert_eq!(
        f.footer.as_deref(),
        Some("billing-api is running: changes apply on next start")
    );
    for _ in 0..3 {
        press(&mut f, &org, Action::Char('x'));
        press(&mut f, &org, Action::BackTab);
    }
    assert_eq!(text(&f, FieldId::Name), "billing-api");
    // Stopped: the name is editable and the footer is gone.
    let f = edit_billing(&org, &stopped());
    assert_eq!(f.field(FieldId::Name).unwrap().kind, FieldKind::Text);
    assert_eq!(f.field(FieldId::Name).unwrap().note, None);
    assert_eq!(f.footer, None);
}

#[test]
fn stacked_prompts_show_none_for_empty_levels() {
    let org = org();
    let f = edit_billing(&org, &running());
    assert_eq!(
        f.stacked,
        [
            ("Org".to_owned(), Some(ORG_PROMPT.to_owned())),
            ("Payments".to_owned(), None),
            ("Backend".to_owned(), Some(BACKEND_PROMPT.to_owned())),
        ]
    );
    let mut m = model();
    m.dialog = Some(Dialog::Form(Box::new(f)));
    assert!(screen_text(&m).contains("┊ Payments  (none)"));
}

#[test]
fn ctrl_s_on_valid_agent_form_sends_edit() {
    let mut m = model();
    update(&mut m, Action::Edit);
    let want = Command::EditOrg(OrgEdit::EditAgent {
        key: n("billing-api"),
        draft: AgentDraft {
            name: n("billing-api"),
            fields: Fields {
                directory: Some("~/code/billing".into()),
                prompt: Some(OWN_PROMPT.into()),
                ..Fields::default()
            },
        },
    });
    assert_eq!(update(&mut m, Action::Save), [UiEffect::Send(want)]);
    // The form waits for the engine: a second save sends nothing.
    assert!(form(&m).saving);
    assert_eq!(update(&mut m, Action::Save), []);
}

#[test]
fn saved_form_closes_on_org_changed_and_shows_rejections() {
    let mut m = model();
    update(&mut m, Action::Edit);
    update(&mut m, Action::Save);
    assert!(m.apply(Event::CommandRejected {
        reason: "flags: missing closing quote".into(),
    }));
    assert_eq!(m.footer_note, None);
    let f = form(&m);
    assert!(!f.saving);
    assert_eq!(
        f.field(FieldId::Flags).unwrap().error.as_deref(),
        Some("flags: missing closing quote")
    );
    assert_eq!(f.error_count(), 1);
    // The refusal stands until something changes: ctrl+s sends nothing.
    assert_eq!(update(&mut m, Action::Save), []);
    // A reason no field owns shows on the form itself.
    form_mut(&mut m).saving = true;
    m.apply(Event::CommandRejected {
        reason: "org.toml has an error · fix it first (o)".into(),
    });
    assert_eq!(
        form(&m).rejected.as_deref(),
        Some("org.toml has an error · fix it first (o)")
    );
    assert!(screen_text(&m).contains("✕ org.toml has an error · fix it first (o)"));
    // Typing clears it, saving again and the new org close the form.
    update(&mut m, Action::Tab);
    update(&mut m, Action::Char('x'));
    update(&mut m, Action::Backspace);
    assert_eq!(form(&m).error_count(), 0);
    assert_eq!(update(&mut m, Action::Save).len(), 1);
    let org = m.org.clone();
    m.apply(Event::OrgChanged {
        org: Box::new(org),
        problems: Default::default(),
    });
    assert_eq!(m.dialog, None);
}

#[test]
fn new_role_form_creates_agents() {
    let org = org();
    let payments = nn("Payments");
    let mut f = Form::new_role(&org, Some(&payments), &dirs(), Path::new(HOME));
    assert_eq!(f.title, "New role · in Payments");
    typed(&mut f, &org, "Frontend");
    let agents = f.field(FieldId::Agents).unwrap();
    assert_eq!(agents.kind, FieldKind::Count);
    assert_eq!(agents.edit.text(), "1");
    assert_eq!(
        agents.suffix.as_deref(),
        Some("create this many now (payments-frontend-1 …)")
    );
    f.focus_field(FieldId::Agents);
    clear(&mut f, &org);
    typed(&mut f, &org, "2");
    assert_eq!(
        press(&mut f, &org, Action::Save),
        FormOutcome::Edit(OrgEdit::AddRole {
            team: Some(payments),
            role: RoleDraft {
                name: nn("Frontend"),
                defaults: Fields::default(),
            },
            agents: 2,
        })
    );
    let mut f = Form::new_role(&org, None, &dirs(), Path::new(HOME));
    assert_eq!(f.title, "New role");
    typed(&mut f, &org, "Frontend");
    assert_eq!(
        f.field(FieldId::Agents).unwrap().suffix.as_deref(),
        Some("create this many now (frontend-1 …)")
    );
    // Only digits go in, and the count is bounded.
    f.focus_field(FieldId::Agents);
    typed(&mut f, &org, "x5");
    assert_eq!(text(&f, FieldId::Agents), "15");
    typed(&mut f, &org, "0");
    assert_eq!(
        f.field(FieldId::Agents).unwrap().error.as_deref(),
        Some("from 0 to 20")
    );
}

#[test]
fn new_team_form() {
    let org = org();
    let mut f = Form::new_team(&org, &dirs(), Path::new(HOME));
    assert!(f.field(FieldId::Agents).is_none());
    // An empty name blocks saving without an error before the field is touched.
    assert_eq!(f.error_count(), 0);
    assert_eq!(press(&mut f, &org, Action::Save), FormOutcome::Continue);
    assert_eq!(
        f.field(FieldId::Name).unwrap().error.as_deref(),
        Some("name is empty")
    );
    typed(&mut f, &org, "Growth");
    assert_eq!(
        press(&mut f, &org, Action::Save),
        FormOutcome::Edit(OrgEdit::AddTeam(TeamDraft {
            name: nn("Growth"),
            defaults: Fields::default(),
        }))
    );
    // A taken team name is the engine's error, shown on the field.
    let mut f = Form::new_team(&org, &dirs(), Path::new(HOME));
    typed(&mut f, &org, "Payments");
    assert_eq!(
        f.field(FieldId::Name).unwrap().error.as_deref(),
        Some("a team named Payments already exists")
    );
}

#[test]
fn edit_role_and_team_forms_save_their_defaults() {
    let org = org();
    let mut f = Form::edit_role(&org, &backend(), &dirs(), Path::new(HOME));
    assert_eq!(f.title, "Edit role · Payments › Backend");
    assert!(f.field(FieldId::Agents).is_none());
    assert_eq!(text(&f, FieldId::Prompt), BACKEND_PROMPT);
    f.focus_field(FieldId::Flags);
    typed(&mut f, &org, "--verbose");
    let Some(OrgEdit::EditRole { key, draft }) = f.edit(&org) else {
        panic!("no role edit");
    };
    assert_eq!(key, backend());
    assert_eq!(draft.defaults.flags.as_deref(), Some("--verbose"));
    assert_eq!(draft.defaults.prompt.as_deref(), Some(BACKEND_PROMPT));
    let mut f = Form::edit_team(&org, &nn("Payments"), &dirs(), Path::new(HOME));
    assert_eq!(f.title, "Edit team · Payments");
    f.focus_field(FieldId::Cli);
    press(&mut f, &org, Action::Right);
    assert_eq!(f.field(FieldId::Cli).unwrap().choice(), "Claude Code");
    press(&mut f, &org, Action::Right);
    assert_eq!(
        press(&mut f, &org, Action::Save),
        FormOutcome::Edit(OrgEdit::EditTeam {
            key: nn("Payments"),
            draft: TeamDraft {
                name: nn("Payments"),
                defaults: Fields {
                    cli: Some(CliKind::Opencode),
                    ..Fields::default()
                },
            },
        })
    );
}

#[test]
fn edit_org_form_changing_duo_cli_asks_first() {
    let mut org = org();
    org.duo.cli = CliKind::Opencode;
    let idle = view(status(AgentState::Idle, mins(1)));
    let mut f = Form::edit_org(&org, &idle);
    assert_eq!(f.title, "Edit org · saved to org.toml");
    f.focus_field(FieldId::DuoCli);
    assert_eq!(f.field(FieldId::DuoCli).unwrap().choice(), "OpenCode");
    press(&mut f, &org, Action::Right);
    assert_eq!(f.field(FieldId::DuoCli).unwrap().choice(), "Claude Code");
    let want = OrgEdit::EditOrgDefaults {
        defaults: org.defaults.clone(),
        duo: DuoConfig {
            name: n("Duo"),
            cli: CliKind::Claude,
        },
    };
    let FormOutcome::Confirm(c) = press(&mut f, &org, Action::Save) else {
        panic!("no confirmation");
    };
    assert_eq!(c.title, "Restart Duo on Claude Code?");
    assert!(!c.focus_yes);
    assert_eq!(c.on_yes, Command::EditOrg(want.clone()));
    // Through `update`: yes sends the edit, cancel goes back to the form.
    let mut m = model();
    m.org = org.clone();
    m.agents.insert(n("Duo"), idle);
    m.selected = NodeKey::Duo;
    update(&mut m, Action::Edit);
    form_mut(&mut m).focus_field(FieldId::DuoCli);
    update(&mut m, Action::Right);
    assert_eq!(update(&mut m, Action::Save), []);
    assert!(matches!(m.dialog, Some(Dialog::Confirm(_))));
    assert_eq!(update(&mut m, Action::DialogNo), []);
    assert_eq!(
        form(&m).field(FieldId::DuoCli).unwrap().choice(),
        "Claude Code"
    );
    update(&mut m, Action::Save);
    assert_eq!(
        update(&mut m, Action::DialogYes),
        [UiEffect::Send(Command::EditOrg(want))]
    );
    assert!(form(&m).saving);
    // While Duo is stopped, nothing to ask.
    let mut f = Form::edit_org(&org, &stopped());
    f.focus_field(FieldId::DuoCli);
    press(&mut f, &org, Action::Right);
    assert!(matches!(
        press(&mut f, &org, Action::Save),
        FormOutcome::Edit(OrgEdit::EditOrgDefaults { .. })
    ));
}

#[test]
fn edit_org_name_read_only_while_duo_runs() {
    let org = org();
    let idle = view(status(AgentState::Idle, mins(1)));
    let mut f = Form::edit_org(&org, &idle);
    let name = f.field(FieldId::Name).unwrap();
    assert_eq!(name.kind, FieldKind::ReadOnly);
    assert_eq!(
        name.note.as_deref(),
        Some("rename only while stopped with no worktrees")
    );
    press(&mut f, &org, Action::Char('x'));
    assert_eq!(text(&f, FieldId::Name), "Duo");
    let mut f = Form::edit_org(&org, &stopped());
    assert_eq!(f.field(FieldId::Name).unwrap().kind, FieldKind::Text);
    f.focus_field(FieldId::Name);
    clear(&mut f, &org);
    typed(&mut f, &org, "Boss");
    let FormOutcome::Edit(OrgEdit::EditOrgDefaults { defaults, duo }) =
        press(&mut f, &org, Action::Save)
    else {
        panic!("no org edit");
    };
    assert_eq!(defaults, org.defaults);
    assert_eq!(
        duo,
        DuoConfig {
            name: AgentName::parse("Boss").unwrap(),
            cli: CliKind::Claude,
        }
    );
    // Duo may not take an agent's name.
    let mut f = Form::edit_org(&org, &stopped());
    f.focus_field(FieldId::Name);
    clear(&mut f, &org);
    typed(&mut f, &org, "squad-1");
    assert_eq!(
        f.field(FieldId::Name).unwrap().error.as_deref(),
        Some("an agent named squad-1 already exists")
    );
}

#[test]
fn chooser_options_follow_selection() {
    let org = org();
    let labels = |c: &Chooser| -> Vec<(String, bool)> {
        c.items
            .iter()
            .map(|i| (i.label.clone(), i.enabled))
            .collect()
    };
    let c = Chooser::for_selection(&org, &NodeKey::Duo);
    assert_eq!(c.title, "New · Duo selected");
    assert_eq!(
        labels(&c),
        [("Team".to_owned(), true), ("Role".to_owned(), true)]
    );
    let c = Chooser::for_selection(&org, &NodeKey::Team(nn("Payments")));
    assert_eq!(c.title, "New · Payments selected");
    assert_eq!(
        labels(&c),
        [
            ("Role in Payments".to_owned(), true),
            ("Team next to Payments".to_owned(), true),
            ("Agent: select a role first".to_owned(), false),
        ]
    );
    let c = Chooser::for_selection(&org, &NodeKey::Agent(n("auth-service")));
    assert_eq!(labels(&c), [("Agent in Backend".to_owned(), true)]);
    let c = Chooser::for_selection(&org, &NodeKey::Role(backend()));
    assert_eq!(labels(&c), [("Agent in Backend".to_owned(), true)]);
}

#[test]
fn chooser_opens_the_chosen_form() {
    let mut m = model();
    m.selected = NodeKey::Team(nn("Payments"));
    assert_eq!(update(&mut m, Action::New), []);
    let Some(Dialog::Chooser(c)) = &m.dialog else {
        panic!("no chooser");
    };
    assert_eq!(c.focus, 0);
    let screen = screen_text(&m);
    assert!(screen.contains("┌─ New · Payments selected ─"), "{screen}");
    assert!(screen.contains("│▌ Role in Payments"), "{screen}");
    assert!(
        screen.contains("↑↓ choose · enter open form · esc"),
        "{screen}"
    );
    // ↓ stops before the disabled entry.
    update(&mut m, Action::Down);
    update(&mut m, Action::Down);
    let Some(Dialog::Chooser(c)) = &m.dialog else {
        panic!("no chooser");
    };
    assert_eq!(c.focus, 1);
    update(&mut m, Action::DialogEnter);
    assert_eq!(form(&m).title, "New team");
    update(&mut m, Action::Esc);
    assert_eq!(m.dialog, None);
    // `q` quits from the chooser, which isn't typing.
    update(&mut m, Action::New);
    assert_eq!(
        map_key(&m, KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
        Some(Action::Quit)
    );
    // On an agent the chooser opens a new agent in its role.
    m.dialog = None;
    m.selected = NodeKey::Agent(n("auth-service"));
    update(&mut m, Action::New);
    update(&mut m, Action::DialogEnter);
    assert_eq!(form(&m).title, "New agent · Payments › Backend");
}

#[test]
fn e_opens_the_form_for_the_selection() {
    let mut m = model();
    let cases = [
        (NodeKey::Duo, "Edit org · saved to org.toml"),
        (NodeKey::Team(nn("Payments")), "Edit team · Payments"),
        (NodeKey::Role(backend()), "Edit role · Payments › Backend"),
        (
            NodeKey::Agent(n("auth-service")),
            "Edit agent · Payments › Backend › auth-service",
        ),
    ];
    for (key, title) in cases {
        m.dialog = None;
        m.selected = key;
        update(&mut m, Action::Edit);
        assert_eq!(form(&m).title, title);
    }
    // While org.toml is broken, edits are refused before a form opens.
    m.dialog = None;
    m.org_error = Some(duohive_core::ports::OrgFileError {
        line: Some(3),
        message: "bad".into(),
    });
    update(&mut m, Action::Edit);
    assert_eq!(m.dialog, None);
    assert_eq!(
        m.footer_note.as_deref(),
        Some("org.toml has an error · fix it first (o)")
    );
}

#[test]
fn esc_cancels_without_command() {
    let org = org();
    let mut f = edit_billing(&org, &running());
    f.focus_field(FieldId::Model);
    typed(&mut f, &org, "opus");
    assert_eq!(press(&mut f, &org, Action::Esc), FormOutcome::Cancel);
    let mut m = model();
    update(&mut m, Action::Edit);
    form_mut(&mut m).focus_field(FieldId::Model);
    assert_eq!(update(&mut m, Action::Char('o')), []);
    assert_eq!(update(&mut m, Action::Esc), []);
    assert_eq!(m.dialog, None);
}

#[test]
fn keys_type_into_forms_and_q_does_not_quit() {
    let mut m = model();
    update(&mut m, Action::Edit);
    let k = |c: KeyCode, mods: KeyModifiers| KeyEvent::new(c, mods);
    let none = KeyModifiers::NONE;
    assert_eq!(
        map_key(&m, k(KeyCode::Char('q'), none)),
        Some(Action::Char('q'))
    );
    assert_eq!(
        map_key(&m, k(KeyCode::Char('s'), none)),
        Some(Action::Char('s'))
    );
    // ctrl+c quits only when not typing; a form is typing.
    assert_eq!(
        map_key(&m, k(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        None
    );
    assert_eq!(
        map_key(&m, k(KeyCode::Char('s'), KeyModifiers::CONTROL)),
        Some(Action::Save)
    );
    assert_eq!(
        map_key(&m, k(KeyCode::Enter, KeyModifiers::ALT)),
        Some(Action::NewLine)
    );
    let keys = [
        (KeyCode::Left, Action::Left),
        (KeyCode::Right, Action::Right),
        (KeyCode::Home, Action::Home),
        (KeyCode::End, Action::End),
        (KeyCode::Backspace, Action::Backspace),
        (KeyCode::Delete, Action::DeleteForward),
        (KeyCode::Enter, Action::DialogEnter),
        (KeyCode::Tab, Action::Tab),
        (KeyCode::Esc, Action::Esc),
    ];
    for (code, action) in keys {
        assert_eq!(map_key(&m, k(code, none)), Some(action), "{code:?}");
    }
    // Typing `q` into the flags field.
    form_mut(&mut m).focus_field(FieldId::Flags);
    update(&mut m, Action::Char('q'));
    assert_eq!(text(form(&m), FieldId::Flags), "q");
}

#[test]
fn tab_enter_and_alt_enter_move_and_edit() {
    let org = org();
    let mut f = edit_billing(&org, &stopped());
    assert_eq!(f.fields[f.focus].id, FieldId::Name);
    press(&mut f, &org, Action::Tab);
    assert_eq!(f.fields[f.focus].id, FieldId::Cli);
    press(&mut f, &org, Action::BackTab);
    press(&mut f, &org, Action::BackTab);
    assert!(
        f.on_cancel(),
        "shift+tab from the first field wraps to Cancel"
    );
    // enter moves to the next field; on the last field it moves to Save.
    f.focus_field(FieldId::Prompt);
    press(&mut f, &org, Action::NewLine);
    assert_eq!(text(&f, FieldId::Prompt), format!("{OWN_PROMPT}\n"));
    press(&mut f, &org, Action::DialogEnter);
    assert!(f.on_save());
    // enter on Save saves; on Cancel cancels.
    assert!(matches!(
        press(&mut f, &org, Action::DialogEnter),
        FormOutcome::Edit(OrgEdit::EditAgent { .. })
    ));
    press(&mut f, &org, Action::Tab);
    assert_eq!(
        press(&mut f, &org, Action::DialogEnter),
        FormOutcome::Cancel
    );
    // NewLine only types into prompt fields.
    f.focus_field(FieldId::Flags);
    press(&mut f, &org, Action::NewLine);
    assert_eq!(text(&f, FieldId::Flags), "");
    f.focus_field(FieldId::Directory);
    press(&mut f, &org, Action::DialogEnter);
    assert_eq!(f.fields[f.focus].id, FieldId::Model);
}

#[test]
fn paste_keeps_newlines_only_in_prompts() {
    let mut m = model();
    m.agents.insert(n("billing-api"), stopped());
    update(&mut m, Action::Edit);
    form_mut(&mut m).focus_field(FieldId::Flags);
    paste(&mut m, "--a\r\n--b\n");
    assert_eq!(text(form(&m), FieldId::Flags), "--a --b ");
    form_mut(&mut m).focus_field(FieldId::Prompt);
    paste(&mut m, "\r\nline 2\r\nline\t3");
    assert_eq!(
        text(form(&m), FieldId::Prompt),
        format!("{OWN_PROMPT}\nline 2\nline 3")
    );
    // A directory pasted is probed like a typed one.
    form_mut(&mut m).focus_field(FieldId::Directory);
    let f = form_mut(&mut m);
    f.fields[f.focus].edit = LineEdit::default();
    paste(&mut m, "~/notes\n");
    let f = form(&m);
    assert_eq!(text(f, FieldId::Directory), "~/notes ");
    // Pasting with no form open does nothing.
    m.dialog = None;
    let before = m.clone();
    paste(&mut m, "x");
    assert_eq!(m, before);
}

#[test]
fn line_edit_basics() {
    let mut e = LineEdit::default();
    for c in "abc".chars() {
        e.insert(c);
    }
    e.left();
    e.backspace();
    assert_eq!((e.text(), e.cursor()), ("ac", 1));
    e.home();
    e.delete();
    assert_eq!((e.text(), e.cursor()), ("c", 0));
    e.end();
    e.insert('d');
    assert_eq!((e.text(), e.cursor()), ("cd", 2));
}

#[test]
fn line_edit_moves_by_grapheme() {
    // "é" as e + combining acute is one grapheme; the flag is one grapheme of two chars.
    let mut e = LineEdit::new("ae\u{301}🇻🇳");
    assert_eq!(e.cursor(), 3);
    e.backspace();
    assert_eq!(e.text(), "ae\u{301}");
    e.left();
    assert_eq!(e.cursor(), 1);
    e.delete();
    assert_eq!(e.text(), "a");
    e.right();
    e.right();
    assert_eq!(e.cursor(), 1);
    e.insert_str("世界");
    assert_eq!(e.cursor(), 3);
    // A window of 4 cells keeps the cursor in view: "界" and the cursor cell.
    assert_eq!(e.window(4), ("界".to_owned(), 2));
    e.home();
    assert_eq!(e.window(4), ("a世".to_owned(), 0));
}

/// `DirProbe` is object safe for the shared handle.
#[test]
fn shared_dirs_compare_by_identity() {
    let a: Arc<dyn DirProbe> = Arc::new(dirs());
    assert_eq!(SharedDirs(Arc::clone(&a)), SharedDirs(Arc::clone(&a)));
    assert_ne!(SharedDirs(a), SharedDirs(Arc::new(dirs())));
}
