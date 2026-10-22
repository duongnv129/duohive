//! The Model and Effort dropdowns (UI §4.4.1).

use super::*;
use crate::forms::DropRow;

fn many(n: usize) -> Models {
    let list = ModelList {
        models: (0..n).map(|i| format!("prov/model-{i:03}")).collect(),
        ..ModelList::default()
    };
    Models::from([(CliKind::Claude, Ok(list))])
}

fn open_model(m: &Models) -> (Form, Org) {
    let org = org();
    let mut f = edit_billing(&org, &stopped());
    f.set_models(m);
    f.focus_field(FieldId::Model);
    press(&mut f, &org, Action::DialogEnter);
    (f, org)
}

fn model_field(f: &Form) -> &crate::forms::Field {
    f.field(FieldId::Model).unwrap()
}

fn open_model_screen(m: Models) -> String {
    let (f, _) = open_model(&m);
    let mut ui = model();
    ui.models = m;
    ui.dialog = Some(Dialog::Form(Box::new(f)));
    screen_text(&ui)
}

#[test]
fn loading_then_list() {
    let org = org();
    let mut f = edit_billing(&org, &stopped());
    f.set_models(&Models::new());
    assert!(model_field(&f).drop.loading);
    f.focus_field(FieldId::Model);
    typed(&mut f, &org, "opus");
    assert_eq!(
        text(&f, FieldId::Model),
        "opus",
        "typing works while loading"
    );
    f.set_models(&claude_lists());
    assert!(!model_field(&f).drop.loading);
    assert_eq!(model_field(&f).drop.options, ["sonnet", "opus", "haiku"]);
}

#[test]
fn typeahead_filters_117_models() {
    let (mut f, org) = open_model(&many(117));
    assert_eq!(
        model_field(&f).drop_rows().len(),
        118,
        "inherit row and 117"
    );
    typed(&mut f, &org, "model-11");
    let rows = model_field(&f).drop_rows();
    // inherit, model-110..model-116, and the typed row.
    assert_eq!(rows.len(), 1 + 7 + 1, "{rows:?}");
    assert!(matches!(rows.last(), Some(DropRow::Typed(t)) if t == "model-11"));
    insta::assert_snapshot!(
        "dropdown_open_filtered_100x30",
        open_model_screen(many(117))
    );
}

#[test]
fn custom_name_accepted() {
    let (mut f, org) = open_model(&claude_lists());
    typed(&mut f, &org, "claude-x");
    for _ in 0..4 {
        press(&mut f, &org, Action::Down);
    }
    press(&mut f, &org, Action::DialogEnter);
    assert_eq!(text(&f, FieldId::Model), "claude-x");
    assert!(!model_field(&f).drop.open);
}

#[test]
fn inherit_row_first_and_sets_inherit() {
    let (mut f, org) = open_model(&claude_lists());
    typed(&mut f, &org, "opus");
    let first = model_field(&f).drop_rows()[0].clone();
    assert!(matches!(first, DropRow::Inherit(l) if l == "inherit: sonnet"));
    // Cursor sits on the inherit row: choosing it clears the value.
    press(&mut f, &org, Action::DialogEnter);
    assert_eq!(text(&f, FieldId::Model), "");
}

#[test]
fn choosing_an_option_sets_it() {
    let (mut f, org) = open_model(&claude_lists());
    press(&mut f, &org, Action::Down);
    press(&mut f, &org, Action::Down);
    press(&mut f, &org, Action::DialogEnter);
    assert_eq!(text(&f, FieldId::Model), "opus");
}

#[test]
fn inherit_uses_the_inherited_clis_list() {
    let org = org();
    let opencode = ModelList {
        models: vec!["acme/fast".to_owned()],
        ..ModelList::default()
    };
    let mut f = edit_billing(&org, &stopped());
    // The CLI field inherits Claude Code; choose OpenCode explicitly.
    f.focus_field(FieldId::Cli);
    press(&mut f, &org, Action::Right);
    press(&mut f, &org, Action::Right);
    let mut lists = claude_lists();
    lists.insert(CliKind::Opencode, Ok(opencode));
    f.set_models(&lists);
    assert_eq!(model_field(&f).drop.options, ["acme/fast"]);
}

#[test]
fn effort_resets_when_model_changes() {
    let org = org();
    let mut lists = Models::new();
    let opencode = ModelList {
        models: vec!["a/one".to_owned(), "a/two".to_owned()],
        variants: [("a/one".to_owned(), vec!["high".to_owned()])].into(),
        ..ModelList::default()
    };
    lists.insert(CliKind::Opencode, Ok(opencode));
    let mut f = edit_billing(&org, &stopped());
    f.focus_field(FieldId::Cli);
    press(&mut f, &org, Action::Right);
    press(&mut f, &org, Action::Right);
    f.set_models(&lists);
    f.focus_field(FieldId::Model);
    typed(&mut f, &org, "a/one");
    f.focus_field(FieldId::Effort);
    typed(&mut f, &org, "high");
    f.focus_field(FieldId::Model);
    clear(&mut f, &org);
    typed(&mut f, &org, "a/two");
    assert_eq!(
        text(&f, FieldId::Effort),
        "high",
        "a/two lists no variants: kept"
    );
    clear(&mut f, &org);
    typed(&mut f, &org, "a/one");
    f.focus_field(FieldId::Effort);
    clear(&mut f, &org);
    typed(&mut f, &org, "max");
    f.focus_field(FieldId::Model);
    clear(&mut f, &org);
    typed(&mut f, &org, "a/one");
    assert_eq!(text(&f, FieldId::Effort), "", "max isn't a/one's variant");
}

#[test]
fn opencode_effort_disabled_without_model() {
    let org = org();
    let mut f = Form::edit_org(&org, &running());
    f.focus_field(FieldId::Cli);
    press(&mut f, &org, Action::Right);
    f.set_models(&Models::from([(
        CliKind::Opencode,
        Ok(ModelList::default()),
    )]));
    let effort = f.field(FieldId::Effort).unwrap();
    if effort.locked {
        assert_eq!(
            effort.note.as_deref(),
            Some(crate::forms::EFFORT_NEEDS_MODEL)
        );
    }
}

#[test]
fn claude_effort_list_from_help_flag() {
    let org = org();
    let mut f = edit_billing(&org, &stopped());
    f.set_models(&claude_lists());
    let effort = f.field(FieldId::Effort).unwrap();
    assert_eq!(
        effort.drop.options,
        ["low", "medium", "high", "xhigh", "max"]
    );
    assert!(!effort.locked);
}

#[test]
fn empty_list_allows_typing() {
    let org = org();
    let mut f = edit_billing(&org, &stopped());
    f.set_models(&Models::from([(CliKind::Claude, Ok(ModelList::default()))]));
    f.focus_field(FieldId::Model);
    typed(&mut f, &org, "custom");
    assert_eq!(text(&f, FieldId::Model), "custom");
    assert_eq!(model_field(&f).drop_rows().len(), 2, "inherit and typed");
}

#[test]
fn failure_allows_typing() {
    let org = org();
    let mut f = edit_billing(&org, &stopped());
    f.set_models(&Models::from([(CliKind::Claude, Err("boom".to_owned()))]));
    assert_eq!(
        model_field(&f).note.as_deref(),
        Some("✕ couldn't list claude's models · type a name or keep inherit")
    );
    f.focus_field(FieldId::Model);
    typed(&mut f, &org, "x");
    assert_eq!(text(&f, FieldId::Model), "x");
}

#[test]
fn esc_closes_without_change() {
    let (mut f, org) = open_model(&claude_lists());
    typed(&mut f, &org, "xyz");
    assert_eq!(press(&mut f, &org, Action::Esc), FormOutcome::Continue);
    assert!(!model_field(&f).drop.open);
    assert_eq!(text(&f, FieldId::Model), "", "back to what it was");
    assert_eq!(
        press(&mut f, &org, Action::Esc),
        FormOutcome::Cancel,
        "esc again closes the form"
    );
}

#[test]
fn form_stays_usable_while_a_list_is_loading() {
    let org = org();
    let mut f = edit_billing(&org, &stopped());
    f.set_models(&Models::new());
    f.focus_field(FieldId::Name);
    typed(&mut f, &org, "x");
    assert!(matches!(
        press(&mut f, &org, Action::Save),
        FormOutcome::Edit(_)
    ));
}

#[test]
fn dropdown_snapshots() {
    let org = org();
    let form = |lists: &Models, open: bool| {
        let mut f = edit_billing(&org, &stopped());
        f.set_models(lists);
        f.focus_field(FieldId::Model);
        if open {
            press(&mut f, &org, Action::DialogEnter);
        }
        let mut ui = model();
        ui.models = lists.clone();
        ui.dialog = Some(Dialog::Form(Box::new(f)));
        ui
    };
    let failed = Models::from([(CliKind::Claude, Err("x".to_owned()))]);
    let cases = [
        ("dropdown_closed", form(&claude_lists(), false)),
        ("dropdown_open", form(&claude_lists(), true)),
        ("dropdown_loading", form(&Models::new(), false)),
        ("dropdown_failed", form(&failed, false)),
    ];
    for (name, ui) in &cases {
        insta::assert_snapshot!(format!("{name}_100x30"), screen_text_at(ui, 100, 30));
        insta::assert_snapshot!(format!("{name}_80x24"), screen_text_at(ui, 80, 24));
    }
}
