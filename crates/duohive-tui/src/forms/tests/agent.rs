//! The agent form: layout, validation, saving and the engine's answers.

use super::*;

#[test]
fn edit_agent_form_matches_spec_4_4_1() {
    let mut m = model();
    assert_eq!(update(&mut m, Action::Edit), []);
    let got = box_rows(&m, 80);
    let mock: Vec<&str> = include_str!("../../../../../docs/ux/ui-spec.md")
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
fn name_read_only_with_worktrees() {
    let org = org();
    let stopped = crate::fixtures::view(crate::fixtures::status(
        AgentState::Stopped,
        crate::fixtures::mins(1),
    ));
    let f = Form::edit_agent(
        &org,
        &n("billing-api"),
        &stopped,
        true,
        &dirs(),
        std::path::Path::new(HOME),
    );
    let name = f.field(FieldId::Name).unwrap();
    assert_eq!(name.kind, FieldKind::ReadOnly);
    assert_eq!(
        name.note.as_deref(),
        Some("rename only while stopped with no worktrees")
    );
    assert_eq!(f.footer, None);
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
    // Typing clears it.
    update(&mut m, Action::Tab);
    update(&mut m, Action::Char('x'));
    update(&mut m, Action::Backspace);
    assert_eq!(form(&m).error_count(), 0);
    assert_eq!(update(&mut m, Action::Save).len(), 1);
    // A reason no field owns shows on the form itself and doesn't block a retry.
    m.apply(Event::CommandRejected {
        reason: "org.toml has an error · fix it first (o)".into(),
    });
    assert_eq!(
        form(&m).rejected.as_deref(),
        Some("org.toml has an error · fix it first (o)")
    );
    assert_eq!(form(&m).error_count(), 0);
    let screen = screen_text(&m);
    assert!(screen.contains("✕ org.toml has an error · fix it first (o)"));
    assert!(screen.contains("tab next field · ctrl+s save · esc cancel   "));
    assert_eq!(update(&mut m, Action::Save).len(), 1);
    assert_eq!(form(&m).rejected, None);
    // The new org closes the form.
    let org = m.org.clone();
    m.apply(Event::OrgChanged {
        org: Box::new(org),
        problems: Default::default(),
    });
    assert_eq!(m.dialog, None);
}

#[test]
fn worktrees_on_from_org_toml_on_a_plain_dir_is_a_note() {
    let mut org = org();
    let billing = &mut org.teams[0].roles[0].agents[1];
    billing.fields.directory = Some("~/notes".into());
    billing.fields.worktree = Some(true);
    let mut f = edit_billing(&org, &stopped());
    let wt = f.field(FieldId::Worktrees).unwrap();
    assert_eq!(wt.choice(), "on");
    assert_eq!(wt.error, None);
    assert_eq!(
        wt.note.as_deref(),
        Some("new worktree needs a git repo; ~/notes isn't one")
    );
    assert_eq!(f.error_count(), 0);
    assert!(matches!(
        press(&mut f, &org, Action::Save),
        FormOutcome::Edit(_)
    ));
    // Once moved off, `on` can't be chosen again here: it is disabled.
    f.focus_field(FieldId::Worktrees);
    press(&mut f, &org, Action::Right);
    assert_eq!(f.field(FieldId::Worktrees).unwrap().choice(), "off");
    press(&mut f, &org, Action::Right);
    assert_eq!(
        f.field(FieldId::Worktrees).unwrap().choice(),
        "inherit: off"
    );
    press(&mut f, &org, Action::Right);
    assert_eq!(f.field(FieldId::Worktrees).unwrap().choice(), "off");
}
