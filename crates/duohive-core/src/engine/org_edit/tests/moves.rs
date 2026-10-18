//! Tests for moving teams, roles and agents: Task 3.6 (P3-D20).

use super::*;
use crate::command::MoveTarget;
use crate::tool::{CallId, ErrorCode, ToolCall};

fn moved(node: NodeKey, to: MoveTarget) -> OrgEdit {
    OrgEdit::Move { node, to }
}

fn generalist() -> RoleKey {
    RoleKey {
        team: None,
        role: nn("Generalist"),
    }
}

fn role_names(roles: &[crate::org::Role]) -> Vec<String> {
    roles.iter().map(|r| r.name.to_string()).collect()
}

fn agent_names(role: &crate::org::Role) -> Vec<String> {
    role.agents.iter().map(|a| a.name.to_string()).collect()
}

fn team_names(org: &Org) -> Vec<String> {
    org.teams.iter().map(|t| t.name.to_string()).collect()
}

#[test]
fn move_agent_to_role() {
    let mut h = Harness::new();
    let ev = edit(
        &mut h,
        moved(
            NodeKey::Agent(n("pay-qa")),
            MoveTarget::Into(NodeKey::Role(generalist())),
        ),
    );
    let (org, _) = org_changed(&ev);
    assert!(org.teams[0].roles[1].agents.is_empty());
    assert_eq!(
        agent_names(&org.roles[0]),
        ["squad-1", "squad-2", "squad-3", "pay-qa"]
    );
    assert_eq!(saved_orgs(&h), [org]);
    // Into a role that doesn't exist.
    let ev = edit(
        &mut h,
        moved(
            NodeKey::Agent(n("pay-qa")),
            MoveTarget::Into(NodeKey::Role(RoleKey {
                team: None,
                role: nn("Nope"),
            })),
        ),
    );
    assert_eq!(rejected(&ev), ["no role named Nope"]);
}

#[test]
fn move_role_to_team_and_root() {
    let mut h = Harness::new();
    let ev = edit(
        &mut h,
        moved(NodeKey::Role(qa()), MoveTarget::Into(NodeKey::Duo)),
    );
    let (org, _) = org_changed(&ev);
    assert_eq!(role_names(&org.teams[0].roles), ["Backend"]);
    assert_eq!(role_names(&org.roles), ["Generalist", "QA"]);
    assert_eq!(agent_names(&org.roles[1]), ["pay-qa"]);
    let ev = edit(
        &mut h,
        moved(
            NodeKey::Role(generalist()),
            MoveTarget::Into(NodeKey::Team(nn("Payments"))),
        ),
    );
    let (org, _) = org_changed(&ev);
    assert_eq!(role_names(&org.teams[0].roles), ["Backend", "Generalist"]);
    assert_eq!(role_names(&org.roles), ["QA"]);
    // A role moves only where its name is free (D4).
    let root_backend = OrgEdit::AddRole {
        team: None,
        role: RoleDraft {
            name: nn("Backend"),
            defaults: Fields::default(),
        },
        agents: 0,
    };
    org_changed(&edit(&mut h, root_backend));
    let ev = edit(
        &mut h,
        moved(
            NodeKey::Role(RoleKey {
                team: None,
                role: nn("Backend"),
            }),
            MoveTarget::Into(NodeKey::Team(nn("Payments"))),
        ),
    );
    assert_eq!(
        rejected(&ev),
        ["a role named Backend already exists in Payments"]
    );
}

#[test]
fn move_team_reorders() {
    let mut h = Harness::new();
    for name in ["Growth", "Ops"] {
        org_changed(&edit(
            &mut h,
            OrgEdit::AddTeam(TeamDraft {
                name: nn(name),
                defaults: Fields::default(),
            }),
        ));
    }
    assert_eq!(team_names(&h.engine.org), ["Payments", "Growth", "Ops"]);
    let ev = edit(&mut h, moved(NodeKey::Team(nn("Growth")), MoveTarget::End));
    let (org, _) = org_changed(&ev);
    assert_eq!(team_names(&org), ["Payments", "Ops", "Growth"]);
    let ev = edit(
        &mut h,
        moved(NodeKey::Team(nn("Ops")), MoveTarget::Before(nn("Payments"))),
    );
    let (org, _) = org_changed(&ev);
    assert_eq!(team_names(&org), ["Ops", "Payments", "Growth"]);
    // The team's roles and agents move with it.
    assert_eq!(role_names(&org.teams[1].roles), ["Backend", "QA"]);
    let ev = edit(
        &mut h,
        moved(NodeKey::Team(nn("Ops")), MoveTarget::Before(nn("Nope"))),
    );
    assert_eq!(rejected(&ev), ["no team named Nope"]);
}

#[test]
fn move_team_onto_role_refused() {
    let org = test_org();
    let refused = [
        moved(
            NodeKey::Team(nn("Payments")),
            MoveTarget::Into(NodeKey::Role(generalist())),
        ),
        moved(
            NodeKey::Team(nn("Payments")),
            MoveTarget::Into(NodeKey::Duo),
        ),
        moved(NodeKey::Agent(n("pay-qa")), MoveTarget::End),
        moved(
            NodeKey::Agent(n("pay-qa")),
            MoveTarget::Before(nn("Payments")),
        ),
        moved(
            NodeKey::Agent(n("pay-qa")),
            MoveTarget::Into(NodeKey::Team(nn("Payments"))),
        ),
        moved(NodeKey::Role(qa()), MoveTarget::End),
        moved(NodeKey::Role(qa()), MoveTarget::Before(nn("Payments"))),
        moved(
            NodeKey::Role(qa()),
            MoveTarget::Into(NodeKey::Role(generalist())),
        ),
        moved(NodeKey::Duo, MoveTarget::End),
    ];
    for e in &refused {
        let errs = super::super::preview_edit(&org, e).unwrap_err();
        assert_eq!(errs.len(), 1, "{e:?}");
        assert_eq!(errs[0].path, "", "{e:?}");
    }
    // From Duo, through propose_org_change.
    let mut h = Harness::new();
    h.start("Duo");
    let token = h.rt("Duo").token.clone().unwrap();
    for e in refused {
        let out = h.send(Command::Tool {
            call_id: CallId(1),
            token: token.clone(),
            call: ToolCall::ProposeOrgChange {
                change: Box::new(e.clone()),
            },
        });
        let code = out.iter().find_map(|ev| match ev {
            Event::ToolResult { result, .. } => Some(result.clone().map_err(|err| err.code)),
            _ => None,
        });
        assert_eq!(code, Some(Err(ErrorCode::InvalidChange)), "{e:?}");
    }
    // And by hand.
    let ev = edit(
        &mut h,
        moved(
            NodeKey::Team(nn("Payments")),
            MoveTarget::Into(NodeKey::Role(generalist())),
        ),
    );
    assert_eq!(rejected(&ev), ["a team moves only among the teams"]);
}

#[test]
fn move_running_agent_applies_next_start() {
    let mut org = test_org();
    org.teams[0].roles[1].defaults.directory = Some("~/code/qa".into());
    let mut h = Setup {
        org,
        ..Setup::default()
    }
    .build();
    h.start("squad-1");
    h.input(started("squad-1", "@1"));
    h.force("squad-1", AgentState::Working);
    let token = h.rt("squad-1").token.clone();
    let calls_before = h.calls().len();
    let ev = edit(
        &mut h,
        moved(
            NodeKey::Agent(n("squad-1")),
            MoveTarget::Into(NodeKey::Role(qa())),
        ),
    );
    org_changed(&ev);
    // The session continues: no runtime call, same state and token.
    assert_eq!(h.calls().len(), calls_before, "no runtime call");
    assert_eq!(h.state("squad-1"), AgentState::Working);
    assert_eq!(h.rt("squad-1").token, token);
    h.send(Command::StopAgent {
        agent: n("squad-1"),
    });
    h.input(gone("squad-1", "@1"));
    assert_eq!(h.state("squad-1"), AgentState::Stopped);
    h.start("squad-1");
    let launches = h.claude.launches.lock().unwrap();
    assert_eq!(launches.len(), 2);
    assert_eq!(launches[0].agent.directory.as_deref(), Some("~/code/squad"));
    assert_eq!(launches[1].agent.directory.as_deref(), Some("~/code/qa"));
}
