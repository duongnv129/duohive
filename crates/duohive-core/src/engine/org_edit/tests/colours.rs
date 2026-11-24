//! Tests for team and top-level role colours (TD §7, UI §4.4.2).

use super::*;
use crate::colour::TeamColour;
use crate::colour::TeamColour::{Orchid, Slate, Teal, Violet};
use crate::command::MoveTarget;

fn team_draft(name: &str, color: Option<TeamColour>) -> TeamDraft {
    TeamDraft {
        name: nn(name),
        defaults: Fields::default(),
        color,
    }
}

fn role_draft(name: &str, color: Option<TeamColour>) -> RoleDraft {
    RoleDraft {
        name: nn(name),
        defaults: Fields::default(),
        color,
    }
}

fn team_colour(org: &Org, name: &str) -> Option<TeamColour> {
    org.colour_of(&NodeKey::Team(nn(name)))
}

fn root_colour(org: &Org, name: &str) -> Option<TeamColour> {
    org.colour_of(&NodeKey::Role(RoleKey {
        team: None,
        role: nn(name),
    }))
}

#[test]
fn add_team_without_colour_takes_the_next_free() {
    let mut h = Harness::new();
    // Payments (teal) and Generalist (violet), both derived.
    let ev = edit(&mut h, OrgEdit::AddTeam(team_draft("Growth", None)));
    let (org, _) = org_changed(&ev);
    assert_eq!(team_colour(&org, "Growth"), Some(Slate));
    assert_eq!(org.teams[1].color, Some(Slate), "the save writes it");
}

#[test]
fn add_team_with_colour_keeps_it() {
    let mut h = Harness::new();
    let ev = edit(&mut h, OrgEdit::AddTeam(team_draft("Growth", Some(Orchid))));
    let (org, _) = org_changed(&ev);
    assert_eq!(team_colour(&org, "Growth"), Some(Orchid));
}

#[test]
fn edit_team_sets_the_colour() {
    let mut h = Harness::new();
    let ev = edit(
        &mut h,
        OrgEdit::EditTeam {
            key: nn("Payments"),
            draft: team_draft("Payments", Some(Orchid)),
        },
    );
    let (org, _) = org_changed(&ev);
    assert_eq!(team_colour(&org, "Payments"), Some(Orchid));
}

#[test]
fn a_draft_without_colour_still_parses() {
    let team: TeamDraft =
        serde_json::from_str(r#"{"name":"Growth","defaults":{}}"#).expect("old draft parses");
    assert_eq!(team.color, None);
    let role: RoleDraft =
        serde_json::from_str(r#"{"name":"Ops","defaults":{}}"#).expect("old draft parses");
    assert_eq!(role.color, None);
    let with: TeamDraft =
        serde_json::from_str(r#"{"name":"Growth","defaults":{},"color":"violet"}"#).unwrap();
    assert_eq!(with.color, Some(Violet));
}

#[test]
fn an_edit_draft_without_colour_keeps_the_current_one() {
    let mut h = Harness::new();
    edit(
        &mut h,
        OrgEdit::EditTeam {
            key: nn("Payments"),
            draft: team_draft("Payments", Some(Slate)),
        },
    );
    let ev = edit(
        &mut h,
        OrgEdit::EditTeam {
            key: nn("Payments"),
            draft: team_draft("Payments Core", None),
        },
    );
    let (org, _) = org_changed(&ev);
    assert_eq!(team_colour(&org, "Payments Core"), Some(Slate));
}

#[test]
fn moving_a_root_role_into_a_team_clears_its_colour() {
    let mut h = Harness::new();
    let ev = edit(
        &mut h,
        OrgEdit::Move {
            node: NodeKey::Role(RoleKey {
                team: None,
                role: nn("Generalist"),
            }),
            to: MoveTarget::Into(NodeKey::Team(nn("Payments"))),
        },
    );
    let (org, _) = org_changed(&ev);
    let moved = org.teams[0].roles.last().unwrap();
    assert_eq!(moved.name.as_str(), "Generalist");
    assert_eq!(moved.color, None);
}

#[test]
fn moving_a_role_out_takes_the_next_free() {
    let mut h = Harness::new();
    let ev = edit(
        &mut h,
        OrgEdit::Move {
            node: NodeKey::Role(qa()),
            to: MoveTarget::Into(NodeKey::Duo),
        },
    );
    let (org, _) = org_changed(&ev);
    // Payments is teal and Generalist violet, so the role that left takes slate.
    assert_eq!(root_colour(&org, "QA"), Some(Slate));
}

#[test]
fn a_card_proposed_team_takes_the_next_free_colour_when_approved() {
    // Duo's tool carries no colour, so the draft is colourless, as in this edit.
    let mut h = Harness::new();
    let ev = edit(&mut h, OrgEdit::AddTeam(team_draft("Pay-web", None)));
    let (org, _) = org_changed(&ev);
    assert_eq!(team_colour(&org, "Pay-web"), Some(Slate));
}

#[test]
fn add_root_role_counts_teams_as_siblings() {
    let mut h = Harness::new();
    // A team (teal, derived) and a root role Generalist (violet); a new root role is slate.
    let ev = edit(
        &mut h,
        OrgEdit::AddRole {
            team: None,
            role: role_draft("Ops", None),
            agents: 0,
        },
    );
    let (org, _) = org_changed(&ev);
    assert_eq!(team_colour(&org, "Payments"), Some(Teal));
    assert_eq!(root_colour(&org, "Generalist"), Some(Violet));
    assert_eq!(root_colour(&org, "Ops"), Some(Slate));
}

#[test]
fn a_role_in_a_team_refuses_a_colour() {
    let mut h = Harness::new();
    let ev = edit(
        &mut h,
        OrgEdit::EditRole {
            key: qa(),
            draft: role_draft("QA", Some(Orchid)),
        },
    );
    assert_eq!(
        rejected(&ev),
        ["a role inside a team takes its team's colour"]
    );
    let ev = edit(
        &mut h,
        OrgEdit::AddRole {
            team: Some(nn("Payments")),
            role: role_draft("Docs", Some(Teal)),
            agents: 0,
        },
    );
    assert_eq!(
        rejected(&ev),
        ["a role inside a team takes its team's colour"]
    );
}

#[test]
fn an_add_keeps_the_siblings_effective_colours() {
    let mut h = Harness::new();
    // Payments loads teal and Generalist violet, both derived.
    let ev = edit(&mut h, OrgEdit::AddTeam(team_draft("Growth", None)));
    let (org, _) = org_changed(&ev);
    // Growth sorts before the root role Generalist, which keeps violet.
    assert_eq!(team_colour(&org, "Growth"), Some(Slate));
    assert_eq!(root_colour(&org, "Generalist"), Some(Violet));
    assert_eq!(team_colour(&org, "Payments"), Some(Teal));
}

#[test]
fn deleting_a_sibling_after_a_save_does_not_recolour_the_others() {
    let mut h = Harness::new();
    edit(&mut h, OrgEdit::AddTeam(team_draft("Growth", None)));
    let saved = saved_orgs(&h).pop().unwrap();
    assert_eq!(root_colour(&saved, "Generalist"), Some(Violet));
    let ev = edit(&mut h, OrgEdit::Delete(NodeKey::Team(nn("Payments"))));
    let (org, _) = org_changed(&ev);
    assert_eq!(team_colour(&org, "Growth"), Some(Slate));
    assert_eq!(root_colour(&org, "Generalist"), Some(Violet));
}
