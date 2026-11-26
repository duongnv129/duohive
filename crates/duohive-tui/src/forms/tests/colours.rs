//! The Colour row of the team and top-level role forms, and the wordmark preview in Edit org
//! (UI §4.4.1).

use duohive_core::colour::TeamColour;
use ratatui::style::Modifier;

use super::*;

fn colour(f: &Form) -> TeamColour {
    f.colour_value().expect("a Colour row")
}

#[test]
fn new_team_form_starts_on_the_next_free_colour() {
    // Payments is teal and the root role Generalist violet (both derived): slate is free.
    let f = Form::new_team(&org(), &dirs(), Path::new(HOME));
    assert_eq!(colour(&f), TeamColour::Slate);
    let names = match &f.field(FieldId::Colour).unwrap().kind {
        FieldKind::Choice(n) => n.clone(),
        other => panic!("not a choice: {other:?}"),
    };
    assert_eq!(names, ["teal", "violet", "slate", "orchid"]);
}

#[test]
fn edit_team_form_shows_its_colour() {
    let mut o = org();
    o.teams[0].color = Some(TeamColour::Orchid);
    let f = Form::edit_team(&o, &nn("Payments"), &dirs(), Path::new(HOME));
    assert_eq!(colour(&f), TeamColour::Orchid);
}

#[test]
fn a_role_in_a_team_has_no_colour_row() {
    let o = org();
    let f = Form::new_role(&o, Some(&nn("Payments")), &dirs(), Path::new(HOME));
    assert!(f.field(FieldId::Colour).is_none());
    let f = Form::edit_role(&o, &backend(), &dirs(), Path::new(HOME));
    assert!(f.field(FieldId::Colour).is_none());
}

#[test]
fn a_root_role_form_has_a_colour_row() {
    let o = org();
    let f = Form::new_role(&o, None, &dirs(), Path::new(HOME));
    assert!(f.field(FieldId::Colour).is_some());
    let key = RoleKey {
        team: None,
        role: nn("Generalist"),
    };
    let f = Form::edit_role(&o, &key, &dirs(), Path::new(HOME));
    assert_eq!(colour(&f), TeamColour::Violet);
}

#[test]
fn left_right_choose_a_colour() {
    let o = org();
    let mut f = Form::new_team(&o, &dirs(), Path::new(HOME));
    f.focus_field(FieldId::Colour);
    press(&mut f, &o, Action::Right);
    assert_eq!(colour(&f), TeamColour::Orchid);
    press(&mut f, &o, Action::Left);
    press(&mut f, &o, Action::Left);
    assert_eq!(colour(&f), TeamColour::Violet);
}

#[test]
fn save_sends_the_colour_in_the_draft() {
    let o = org();
    let mut f = Form::new_team(&o, &dirs(), Path::new(HOME));
    typed(&mut f, &o, "Growth");
    f.focus_field(FieldId::Colour);
    press(&mut f, &o, Action::Right);
    let FormOutcome::Edit(OrgEdit::AddTeam(d)) = press(&mut f, &o, Action::Save) else {
        panic!("no AddTeam");
    };
    assert_eq!(d.color, Some(TeamColour::Orchid));
    // A role in a team sends none.
    let mut f = Form::new_role(&o, Some(&nn("Payments")), &dirs(), Path::new(HOME));
    typed(&mut f, &o, "Docs");
    let FormOutcome::Edit(OrgEdit::AddRole { role, .. }) = press(&mut f, &o, Action::Save) else {
        panic!("no AddRole");
    };
    assert_eq!(role.color, None);
}

#[test]
fn the_colour_row_shows_all_four_names_each_in_its_hue() {
    let o = org();
    let mut m = model();
    m.dialog = Some(Dialog::Form(Box::new(Form::new_team(
        &o,
        &dirs(),
        Path::new(HOME),
    ))));
    let buf = render_buffer(&m, &ctx(), 100, 30);
    let p = crate::theme::Palette::dark();
    let lines = crate::fixtures::lines_of(&buf);
    let (y, line) = lines
        .iter()
        .enumerate()
        .find(|(_, l)| l.contains("Colour"))
        .expect("the Colour row");
    for (name, hue, chosen) in [
        ("teal", TeamColour::Teal, false),
        ("violet", TeamColour::Violet, false),
        ("slate", TeamColour::Slate, true),
        ("orchid", TeamColour::Orchid, false),
    ] {
        let x = line.chars().take_while(|_| true).collect::<Vec<_>>();
        let at = line
            .find(name)
            .unwrap_or_else(|| panic!("no {name} in {line}"));
        let col = x[..line[..at].chars().count()].len();
        let cell = &buf[(u16::try_from(col).unwrap(), u16::try_from(y).unwrap())];
        assert_eq!(cell.fg, p.hue(hue), "{name}");
        assert_eq!(cell.modifier.contains(Modifier::BOLD), chosen, "{name}");
    }
    assert!(line.contains("(•) slate"), "{line}");
}

#[test]
fn a_card_edit_form_has_the_colour_row() {
    let m = model();
    let card = duohive_core::card::Card {
        id: duohive_core::card::CardId(1),
        change: duohive_core::card::CardChange::Org(OrgEdit::AddTeam(TeamDraft {
            name: nn("Growth"),
            defaults: Fields::default(),
            color: None,
        })),
        source: duohive_core::card::CardSource::Duo,
        raised_at: duohive_core::time::Millis(0),
        answer: None,
    };
    let f = super::super::form_for_card(&card, &m).expect("a form");
    assert!(f.field(FieldId::Colour).is_some());
    // Duo's tool carries no colour: the form starts on the next free one.
    assert_eq!(colour(&f), TeamColour::Slate);
}

#[test]
fn edit_org_shows_the_wordmark_of_the_typed_name() {
    let o = org();
    let mut f = Form::edit_org(&o, &stopped());
    f.focus_field(FieldId::Name);
    clear(&mut f, &o);
    typed(&mut f, &o, "queen-bee");
    let mut m = model();
    m.dialog = Some(Dialog::Form(Box::new(f)));
    let text = box_rows(&m, 80).join("\n");
    // `QUE`: Q, U and E on two rows.
    assert!(text.contains("▄▀▄ █ █ ██▀"), "{text}");
    assert!(text.contains("▀▄█ █▄█ █▄▄"), "{text}");
    assert!(text.contains("its mark on the card"));
}

#[test]
fn edit_org_name_rules_refuse_bad_names() {
    let o = org();
    for (name, bad) in [
        ("-bee", true),
        ("queen-bee", false),
        (&"x".repeat(41), true),
        ("billing-api", true),
    ] {
        let mut f = Form::edit_org(&o, &stopped());
        f.focus_field(FieldId::Name);
        clear(&mut f, &o);
        typed(&mut f, &o, name);
        assert_eq!(f.error_count() > 0, bad, "{name}");
    }
}

#[test]
fn duo_name_is_read_only_unless_duo_is_stopped() {
    let o = org();
    let f = Form::edit_org(&o, &running());
    let name = f.field(FieldId::Name).unwrap();
    assert!(name.locked || name.kind == FieldKind::ReadOnly);
    assert!(name.note.is_some(), "the field shows the reason");
    let f = Form::edit_org(&o, &stopped());
    assert!(f.field(FieldId::Name).unwrap().focusable());
}

fn card_form(edit: OrgEdit) -> Form {
    let m = model();
    let card = duohive_core::card::Card {
        id: duohive_core::card::CardId(1),
        change: duohive_core::card::CardChange::Org(edit),
        source: duohive_core::card::CardSource::Duo,
        raised_at: duohive_core::time::Millis(0),
        answer: None,
    };
    super::super::form_for_card(&card, &m).expect("a form")
}

#[test]
fn card_forms_show_and_return_an_explicit_proposal_colour() {
    let o = org();
    // The constructor default for a new team is slate, and Payments is teal: pick others.
    let f = card_form(OrgEdit::AddTeam(TeamDraft {
        name: nn("Growth"),
        defaults: Fields::default(),
        color: Some(TeamColour::Orchid),
    }));
    assert_eq!(colour(&f), TeamColour::Orchid);
    let Some(OrgEdit::AddTeam(d)) = f.edit(&o) else {
        panic!("no AddTeam");
    };
    assert_eq!(d.color, Some(TeamColour::Orchid));

    let f = card_form(OrgEdit::EditTeam {
        key: nn("Payments"),
        draft: TeamDraft {
            name: nn("Payments"),
            defaults: Fields::default(),
            color: Some(TeamColour::Violet),
        },
    });
    assert_eq!(colour(&f), TeamColour::Violet);
    let Some(OrgEdit::EditTeam { draft, .. }) = f.edit(&o) else {
        panic!("no EditTeam");
    };
    assert_eq!(draft.color, Some(TeamColour::Violet));

    // A top-level role: the default is slate (free), the saved colour violet.
    let role_draft = |color| RoleDraft {
        name: nn("Ops"),
        defaults: Fields::default(),
        color,
    };
    let f = card_form(OrgEdit::AddRole {
        team: None,
        role: role_draft(Some(TeamColour::Orchid)),
        agents: 1,
    });
    assert_eq!(colour(&f), TeamColour::Orchid);
    let Some(OrgEdit::AddRole { role, .. }) = f.edit(&o) else {
        panic!("no AddRole");
    };
    assert_eq!(role.color, Some(TeamColour::Orchid));

    let key = RoleKey {
        team: None,
        role: nn("Generalist"),
    };
    let f = card_form(OrgEdit::EditRole {
        key,
        draft: RoleDraft {
            name: nn("Generalist"),
            defaults: Fields::default(),
            color: Some(TeamColour::Teal),
        },
    });
    assert_eq!(colour(&f), TeamColour::Teal);
    let Some(OrgEdit::EditRole { draft, .. }) = f.edit(&o) else {
        panic!("no EditRole");
    };
    assert_eq!(draft.color, Some(TeamColour::Teal));
}
