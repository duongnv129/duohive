//! Which agent a card docks on, and the queue behind it (UI §4.6; CD-D1, CD-D2).

use std::time::Duration;

use duohive_core::card::{Card, CardChange};
use duohive_core::command::{MoveTarget, OrgEdit};
use duohive_core::names::{AgentName, NodeName};
use duohive_core::org::NodeKey;
use duohive_core::testkit::n;

use super::{Dock, dock_agent, dock_for, shown_dock};
use crate::fixtures::{
    add_agent_card, ago, card, mins, new_agent_card, qa_role, raise, spec_model, stop_card,
};
use crate::model::UiModel;

fn stop(id: u64, at_mins: u64, agent: &str) -> Card {
    card(id, ago(mins(at_mins)), CardChange::Stop { agent: n(agent) })
}

fn node(s: &str) -> NodeName {
    NodeName::parse(s).unwrap()
}

fn org_card(edit: OrgEdit) -> Card {
    card(1, ago(mins(1)), CardChange::Org(edit))
}

fn duo(m: &UiModel) -> AgentName {
    m.org.duo.name.clone()
}

#[test]
fn stop_card_docks_on_its_agent() {
    let m = spec_model();
    assert_eq!(
        dock_agent(&stop_card(1, ago(mins(1))), &m.org),
        n("squad-1")
    );
}

#[test]
fn edit_agent_card_docks_on_that_agent() {
    let m = spec_model();
    let draft = duohive_core::command::AgentDraft {
        name: n("billing-api"),
        fields: duohive_core::org::Fields::default(),
    };
    let c = org_card(OrgEdit::EditAgent {
        key: n("billing-api"),
        draft,
    });
    assert_eq!(dock_agent(&c, &m.org), n("billing-api"));
}

#[test]
fn delete_agent_card_docks_on_that_agent() {
    let m = spec_model();
    let c = org_card(OrgEdit::Delete(NodeKey::Agent(n("pay-qa"))));
    assert_eq!(dock_agent(&c, &m.org), n("pay-qa"));
}

#[test]
fn move_of_an_agent_docks_on_that_agent() {
    let m = spec_model();
    let c = org_card(OrgEdit::Move {
        node: NodeKey::Agent(n("pay-qa")),
        to: MoveTarget::Into(NodeKey::Role(qa_role())),
    });
    assert_eq!(dock_agent(&c, &m.org), n("pay-qa"));
}

#[test]
fn new_agent_and_add_agent_and_team_and_role_cards_dock_on_duo() {
    let m = spec_model();
    let team = OrgEdit::Delete(NodeKey::Team(node("Payments")));
    let role = OrgEdit::Delete(NodeKey::Role(qa_role()));
    let moved = OrgEdit::Move {
        node: NodeKey::Team(node("Payments")),
        to: MoveTarget::End,
    };
    for c in [
        new_agent_card(1, ago(mins(1))),
        add_agent_card(2, ago(mins(1))),
        org_card(team),
        org_card(role),
        org_card(moved),
    ] {
        assert_eq!(dock_agent(&c, &m.org), duo(&m));
    }
}

#[test]
fn card_for_an_agent_not_in_the_org_docks_on_duo() {
    let m = spec_model();
    let c = stop(1, 1, "ghost");
    assert_eq!(dock_agent(&c, &m.org), duo(&m));
}

#[test]
fn queue_is_oldest_first_with_position_and_total() {
    let mut m = spec_model();
    // Raised out of id order: id 3 is the oldest; ids 1 and 2 tie on time.
    raise(&mut m, stop(1, 4, "squad-1"), Duration::from_secs(240));
    raise(&mut m, stop(2, 4, "squad-1"), Duration::from_secs(240));
    raise(&mut m, stop(3, 9, "squad-1"), Duration::from_secs(540));
    raise(&mut m, stop(4, 1, "squad-2"), Duration::from_secs(60));
    let first = dock_for(&m, &n("squad-1"));
    assert_eq!(
        first,
        Some(Dock {
            card: duohive_core::card::CardId(3),
            position: 1,
            total: 3
        })
    );
    m.cards.remove(&duohive_core::card::CardId(3));
    let next = dock_for(&m, &n("squad-1"));
    assert_eq!(
        next.map(|d| (d.card, d.position, d.total)),
        Some((duohive_core::card::CardId(1), 1, 2))
    );
    assert_eq!(dock_for(&m, &n("auth-service")), None);
}

#[test]
fn shown_dock_follows_the_selection_and_is_none_for_team_role_reconnecting_and_empty_org() {
    let mut m = spec_model();
    raise(&mut m, stop(1, 1, "squad-1"), Duration::from_secs(60));
    raise(
        &mut m,
        new_agent_card(2, ago(mins(2))),
        Duration::from_secs(120),
    );
    m.selected = NodeKey::Agent(n("squad-1"));
    assert_eq!(shown_dock(&m).map(|d| d.card.0), Some(1));
    m.selected = NodeKey::Duo;
    assert_eq!(shown_dock(&m).map(|d| d.card.0), Some(2));
    m.selected = NodeKey::Team(node("Payments"));
    assert_eq!(shown_dock(&m), None);
    m.selected = NodeKey::Role(qa_role());
    assert_eq!(shown_dock(&m), None);
    m.selected = NodeKey::Duo;
    m.reconnecting = 1;
    assert_eq!(shown_dock(&m), None);
    m.reconnecting = 0;
    m.org.teams.clear();
    m.org.roles.clear();
    assert_eq!(shown_dock(&m), None);
}
