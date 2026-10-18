//! Approval cards: changes Duo proposes and you answer (TD §6.2, UI §4.6).

use serde::{Deserialize, Serialize};

use crate::command::{AgentDraft, MoveTarget, OrgEdit};
use crate::names::AgentName;
use crate::org::{NodeKey, Org, RoleKey};
use crate::time::Millis;
use crate::worktree::RunIn;

/// Names one card; counts up from 1 and is never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CardId(pub u64);

/// What raised a card; both are shown as Duo's (P3-D3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardSource {
    /// Duo, through a tool.
    Duo,
    /// The engine, for a task to a role with no free agent.
    NoFreeAgent,
}

/// The change a card holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardChange {
    /// A change to the org.
    Org(OrgEdit),
    /// Stop a busy or starting agent.
    Stop {
        /// Which agent.
        agent: AgentName,
    },
    /// Create an agent in a role and give it the task the card holds.
    NewAgent {
        /// The role.
        role: RoleKey,
        /// The new agent.
        agent: AgentDraft,
        /// The task, kept unchanged by Edit (P3-D7).
        task: String,
        /// Where it runs.
        run: RunIn,
        /// The agent whose own fields were copied, if any (P3-D15).
        clone_of: Option<AgentName>,
    },
}

/// How a card was answered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardOutcome {
    /// Approved as proposed.
    Approved,
    /// Edited, then approved.
    EditedApproved,
    /// Rejected; nothing applied.
    Rejected,
    /// Approved, but it could no longer apply (P3-D21).
    NotApplied(String),
}

/// One card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    /// Its id.
    pub id: CardId,
    /// What raised it.
    pub source: CardSource,
    /// The change it holds.
    pub change: CardChange,
    /// When it was raised.
    pub raised_at: Millis,
    /// The answer and when, once answered.
    pub answer: Option<(CardOutcome, Millis)>,
}

impl Card {
    /// A one-line summary of the change, such as `stop squad-1` or `add pay-web`.
    pub fn summary(&self, org: &Org) -> String {
        self.change.summary(org)
    }
}

impl CardChange {
    /// A one-line summary, as [`Card::summary`]; roles are named by their breadcrumb.
    pub fn summary(&self, org: &Org) -> String {
        match self {
            CardChange::Stop { agent } => format!("stop {agent}"),
            CardChange::NewAgent { agent, .. } => format!("add {}", agent.name),
            CardChange::Org(edit) => edit_summary(edit, org),
        }
    }
}

fn edit_summary(edit: &OrgEdit, org: &Org) -> String {
    let role = |key: &RoleKey| org.breadcrumb(&NodeKey::Role(key.clone()));
    match edit {
        OrgEdit::AddTeam(d) => format!("add team {}", d.name),
        OrgEdit::AddRole { team, role: d, .. } => format!(
            "add role {}",
            role(&RoleKey {
                team: team.clone(),
                role: d.name.clone(),
            })
        ),
        OrgEdit::AddAgent { agent, .. } => format!("add {}", agent.name),
        OrgEdit::EditTeam { key, .. } => format!("edit team {key}"),
        OrgEdit::EditRole { key, .. } => format!("edit role {}", role(key)),
        OrgEdit::EditAgent { key, .. } => format!("edit {key}"),
        OrgEdit::EditOrgDefaults { .. } => "edit the org defaults".to_owned(),
        OrgEdit::Delete(node) => format!("remove {}", node_name(node, org)),
        OrgEdit::Move { node, to } => {
            let place = match to {
                MoveTarget::Into(NodeKey::Role(key)) => format!("to {}", key.role),
                MoveTarget::Into(NodeKey::Team(team)) => format!("to {team}"),
                MoveTarget::Into(NodeKey::Duo) => format!("to {}", org.duo.name),
                MoveTarget::Into(NodeKey::Agent(agent)) => format!("to {agent}"),
                MoveTarget::Before(team) => format!("before {team}"),
                MoveTarget::End => "to the end".to_owned(),
            };
            format!("move {} {place}", node_name(node, org))
        }
    }
}

/// `team Growth`, `role Payments › QA`, an agent's name, or Duo's.
fn node_name(node: &NodeKey, org: &Org) -> String {
    match node {
        NodeKey::Duo => org.duo.name.to_string(),
        NodeKey::Team(team) => format!("team {team}"),
        NodeKey::Role(_) => format!("role {}", org.breadcrumb(node)),
        NodeKey::Agent(agent) => agent.to_string(),
    }
}

impl CardOutcome {
    /// How the answer reads (P3-D12): `approved`, `edited and approved`,
    /// `rejected` or `approved, not applied (<reason>)`.
    pub fn words(&self) -> String {
        match self {
            CardOutcome::Approved => "approved".to_owned(),
            CardOutcome::EditedApproved => "edited and approved".to_owned(),
            CardOutcome::Rejected => "rejected".to_owned(),
            CardOutcome::NotApplied(reason) => format!("approved, not applied ({reason})"),
        }
    }
}

/// The message that answers card `id` to Duo (P3-D12), such as
/// `card 1 approved: stop squad-1`.
pub fn answer_text(id: CardId, outcome: &CardOutcome, summary: &str) -> String {
    format!("card {} {}", id.0, result_line(outcome, summary))
}

/// The chat's `→` line for an answered card (P3-D12): `approved: stop squad-1`.
pub fn result_line(outcome: &CardOutcome, summary: &str) -> String {
    format!("{}: {summary}", outcome.words())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{MoveTarget, TeamDraft};
    use crate::names::NodeName;
    use crate::org::{Fields, NodeKey};
    use crate::testkit::{example_org, n};

    fn nn(s: &str) -> NodeName {
        NodeName::parse(s).unwrap()
    }

    fn card(change: CardChange) -> Card {
        Card {
            id: CardId(1),
            source: CardSource::Duo,
            change,
            raised_at: Millis(0),
            answer: None,
        }
    }

    fn qa() -> RoleKey {
        RoleKey {
            team: Some(nn("Payments")),
            role: nn("QA"),
        }
    }

    fn org_card(edit: OrgEdit) -> CardChange {
        CardChange::Org(edit)
    }

    #[test]
    fn summaries() {
        let org = example_org();
        let draft = AgentDraft {
            name: n("pay-web"),
            fields: Fields::default(),
        };
        let cases = [
            (
                CardChange::Stop {
                    agent: n("squad-1"),
                },
                "stop squad-1",
            ),
            (
                org_card(OrgEdit::AddAgent {
                    role: qa(),
                    agent: draft.clone(),
                }),
                "add pay-web",
            ),
            (
                CardChange::NewAgent {
                    role: qa(),
                    agent: draft,
                    task: "retry cases".into(),
                    run: RunIn::Directory,
                    clone_of: Some(n("pay-qa")),
                },
                "add pay-web",
            ),
            (
                org_card(OrgEdit::AddTeam(TeamDraft {
                    name: nn("Growth"),
                    defaults: Fields::default(),
                })),
                "add team Growth",
            ),
            (
                org_card(OrgEdit::Move {
                    node: NodeKey::Agent(n("pay-qa")),
                    to: MoveTarget::Into(NodeKey::Role(qa())),
                }),
                "move pay-qa to QA",
            ),
            (
                org_card(OrgEdit::Move {
                    node: NodeKey::Role(qa()),
                    to: MoveTarget::Into(NodeKey::Duo),
                }),
                "move role Payments \u{203a} QA to Duo",
            ),
            (
                org_card(OrgEdit::Move {
                    node: NodeKey::Team(nn("Growth")),
                    to: MoveTarget::Before(nn("Payments")),
                }),
                "move team Growth before Payments",
            ),
            (
                org_card(OrgEdit::Move {
                    node: NodeKey::Team(nn("Growth")),
                    to: MoveTarget::End,
                }),
                "move team Growth to the end",
            ),
            (
                org_card(OrgEdit::Delete(NodeKey::Team(nn("Growth")))),
                "remove team Growth",
            ),
            (
                org_card(OrgEdit::Delete(NodeKey::Role(qa()))),
                "remove role Payments \u{203a} QA",
            ),
            (
                org_card(OrgEdit::Delete(NodeKey::Agent(n("pay-qa")))),
                "remove pay-qa",
            ),
        ];
        for (change, want) in cases {
            assert_eq!(card(change).summary(&org), want);
        }
    }

    #[test]
    fn answer_and_result_lines() {
        let s = "stop squad-1";
        let not_applied = CardOutcome::NotApplied("squad-1 is already stopped".into());
        assert_eq!(
            answer_text(CardId(3), &CardOutcome::Approved, s),
            "card 3 approved: stop squad-1"
        );
        assert_eq!(
            answer_text(CardId(3), &CardOutcome::EditedApproved, s),
            "card 3 edited and approved: stop squad-1"
        );
        assert_eq!(
            answer_text(CardId(3), &CardOutcome::Rejected, s),
            "card 3 rejected: stop squad-1"
        );
        assert_eq!(
            answer_text(CardId(3), &not_applied, s),
            "card 3 approved, not applied (squad-1 is already stopped): stop squad-1"
        );
        assert_eq!(
            result_line(&CardOutcome::Approved, s),
            "approved: stop squad-1"
        );
        assert_eq!(
            result_line(&not_applied, s),
            "approved, not applied (squad-1 is already stopped): stop squad-1"
        );
    }
}
