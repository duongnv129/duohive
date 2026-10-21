//! The tree filter (UI §4.1.1): what it keeps, the selection and the keys.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use duohive_core::org::NodeKey;
use duohive_core::testkit::n;

use super::{map_key, update};
use crate::filter::Filter;
use crate::fixtures::spec_model;
use crate::focus::Pane;
use crate::model::UiModel;
use crate::tree::{TreeRowKind, tree_rows};

fn press(m: &mut UiModel, k: KeyCode) {
    let ev = KeyEvent::new(k, KeyModifiers::NONE);
    if let Some(a) = map_key(m, ev) {
        update(m, a);
    }
}

fn org_model() -> UiModel {
    let mut m = spec_model();
    m.focus.pane = Pane::Org;
    m
}

fn labels(m: &UiModel) -> Vec<String> {
    tree_rows(m, false, false, m.now)
        .into_iter()
        .map(|r| r.label)
        .collect()
}

#[test]
fn filter_keeps_matches_and_parents() {
    let mut m = org_model();
    press(&mut m, KeyCode::Char('f'));
    assert_eq!(m.filter, Filter::NeedYou);
    let rows = labels(&m);
    // billing-api waits and squad-3 is quiet: they stay, with their parents.
    for kept in [
        "▣ Payments",
        "◇ Backend",
        "billing-api",
        "◇ Generalist",
        "squad-3",
    ] {
        assert!(rows.iter().any(|l| l == kept), "{kept} in {rows:?}");
    }
    for gone in ["auth-service", "squad-1", "squad-2", "◇ QA"] {
        assert!(!rows.iter().any(|l| l == gone), "{gone} in {rows:?}");
    }
}

#[test]
fn filter_never_hides_cards() {
    let mut m = org_model();
    let before = (m.needs_you.clone(), m.chat.clone());
    press(&mut m, KeyCode::Char('f'));
    press(&mut m, KeyCode::Char('f'));
    assert_eq!((m.needs_you.clone(), m.chat.clone()), before);
}

#[test]
fn filter_empty_result_message() {
    let mut m = org_model();
    m.agents.retain(|_, _| false);
    press(&mut m, KeyCode::Char('f'));
    press(&mut m, KeyCode::Char('f'));
    press(&mut m, KeyCode::Char('f'));
    assert_eq!(m.filter, Filter::Finished);
    let rows = tree_rows(&m, false, false, m.now);
    let empty = rows
        .iter()
        .find(|r| r.kind == TreeRowKind::FilterEmpty)
        .expect("a message row");
    assert_eq!(empty.label, "no agent is finished · esc clears the filter");
}

#[test]
fn filter_clears_hidden_selection_to_first_match() {
    let mut m = org_model();
    assert_eq!(m.selected, NodeKey::Agent(n("auth-service")));
    press(&mut m, KeyCode::Char('f'));
    assert_eq!(m.selected, NodeKey::Agent(n("billing-api")));
}

#[test]
fn esc_in_org_pane_clears_filter() {
    let mut m = org_model();
    press(&mut m, KeyCode::Char('f'));
    press(&mut m, KeyCode::Esc);
    assert_eq!(m.filter, Filter::Off);
}

#[test]
fn esc_elsewhere_keeps_filter() {
    let mut m = org_model();
    press(&mut m, KeyCode::Char('f'));
    m.focus.pane = Pane::Chat;
    press(&mut m, KeyCode::Esc);
    assert_eq!(
        m.filter,
        Filter::NeedYou,
        "esc only went back to the org pane"
    );
    assert_eq!(m.focus.pane, Pane::Org);
    press(&mut m, KeyCode::Esc);
    assert_eq!(m.filter, Filter::Off);
}
