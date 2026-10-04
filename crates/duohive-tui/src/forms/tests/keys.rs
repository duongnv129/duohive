//! Keys, paste and line editing.

use super::*;

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

#[test]
fn insert_that_merges_graphemes_keeps_a_valid_cursor() {
    // A lone ZWJ then 👩 is two graphemes; 👩 typed before them joins all three into one.
    let mut e = LineEdit::default();
    e.insert_str("\u{200D}👩");
    assert_eq!(e.cursor(), 2);
    e.home();
    e.insert('👩');
    assert_eq!(e.text(), "👩\u{200D}👩");
    assert_eq!(e.cursor(), 1);
    e.backspace();
    assert_eq!((e.text(), e.cursor()), ("", 0));
}

#[test]
fn prompt_field_merge_then_newline() {
    let mut m = model();
    m.agents.insert(n("billing-api"), stopped());
    update(&mut m, Action::Edit);
    form_mut(&mut m).focus_field(FieldId::Prompt);
    let f = form_mut(&mut m);
    f.fields[f.focus].edit = LineEdit::default();
    paste(&mut m, "\u{200D}👩");
    update(&mut m, Action::Home);
    update(&mut m, Action::Char('👩'));
    update(&mut m, Action::NewLine);
    let p = &form(&m).fields[form(&m).focus];
    assert_eq!(p.edit.text(), "👩\u{200D}👩\n");
    assert_eq!(p.edit.cursor(), 2);
}

#[test]
fn footer_shows_the_open_dialog_keys() {
    let mut m = model();
    let footer = |m: &UiModel| screen_text(m).lines().last().unwrap().trim_end().to_owned();
    update(&mut m, Action::Edit);
    assert_eq!(footer(&m), " tab next field · ctrl+s save · esc cancel");
    form_mut(&mut m).focus_field(FieldId::Prompt);
    assert_eq!(
        footer(&m),
        " tab next field · ctrl+s save · esc cancel · alt+enter new line"
    );
    update(&mut m, Action::Esc);
    update(&mut m, Action::New);
    assert_eq!(footer(&m), " ↑↓ choose · enter open form · esc");
}
