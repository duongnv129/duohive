//! Role, team and Edit org forms, the chooser and `e`.

use super::*;

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
    // Model and effort are text (D14), so only the CLI row differs.
    follows_mock(&box_rows(&m, 80), &mock("Edit org"), &["CLI "]);
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
    // Only digits go in; any count the engine takes (a u8) is fine.
    f.focus_field(FieldId::Agents);
    typed(&mut f, &org, "x5");
    assert_eq!(text(&f, FieldId::Agents), "15");
    typed(&mut f, &org, "0");
    assert_eq!(f.field(FieldId::Agents).unwrap().error, None);
    assert!(matches!(
        press(&mut f, &org, Action::Save),
        FormOutcome::Edit(OrgEdit::AddRole { agents: 150, .. })
    ));
    typed(&mut f, &org, "0");
    assert_eq!(
        f.field(FieldId::Agents).unwrap().error.as_deref(),
        Some("from 0 to 255")
    );
}

#[test]
fn restart_question_follows_duo_state_at_save() {
    let mut org = org();
    org.duo.cli = CliKind::Opencode;
    let mut m = model();
    m.org = org;
    m.agents.insert(n("Duo"), stopped());
    m.selected = NodeKey::Duo;
    update(&mut m, Action::Edit);
    form_mut(&mut m).focus_field(FieldId::DuoCli);
    update(&mut m, Action::Right);
    // Duo started while the form was open: saving asks first.
    m.apply(Event::AgentStatusChanged {
        agent: n("Duo"),
        status: status(AgentState::Idle, mins(0)),
    });
    assert_eq!(update(&mut m, Action::Save), []);
    assert!(matches!(m.dialog, Some(Dialog::Confirm(_))));
    update(&mut m, Action::DialogNo);
    // Duo stopped again: saving sends at once.
    m.apply(Event::AgentStatusChanged {
        agent: n("Duo"),
        status: status(AgentState::Stopped, mins(0)),
    });
    assert_eq!(update(&mut m, Action::Save).len(), 1);
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
