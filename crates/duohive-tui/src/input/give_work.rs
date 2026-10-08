//! `space`: opening Give work, and its keys (UI §4.5, P2-D10).

use duohive_core::command::Command;
use duohive_core::org::NodeKey;

use super::{Action, UiEffect};
use crate::forms::{GiveOutcome, GiveWork};
use crate::model::{Dialog, UiModel};

/// Said on a team, on Duo, or on a role in a pane that can't pick an agent.
const AGENT_OR_ROLE: &str = "space gives work to an agent or a role";
/// Said on a role: phase 2 gives work to agents only (P2-D10).
const SELECT_AGENT: &str = "select an agent to give work";

/// `space` on the selection: an agent opens the dialog; Duo, a team or a role say why not.
pub(super) fn open(m: &mut UiModel) -> Vec<UiEffect> {
    let note = match &m.selected {
        NodeKey::Duo => AGENT_OR_ROLE,
        NodeKey::Agent(a) if m.org.is_duo(a) => AGENT_OR_ROLE,
        NodeKey::Team(_) => AGENT_OR_ROLE,
        NodeKey::Role(_) => SELECT_AGENT,
        NodeKey::Agent(a) => {
            let task = match m.give_later.take() {
                Some((later, task)) if later == *a => task,
                _ => String::new(),
            };
            m.give_sent = None;
            let g = GiveWork::new(&m.org, a, m.view(a), &task, m.dirs.probe(), &m.home);
            m.dialog = Some(Dialog::GiveWork(Box::new(g)));
            return Vec::new();
        }
    };
    m.footer_note = Some(note.to_owned());
    Vec::new()
}

/// A key in the open dialog: `enter` sends, `esc` closes.
pub(super) fn action(m: &mut UiModel, mut g: Box<GiveWork>, a: Action) -> Vec<UiEffect> {
    match g.handle(a) {
        GiveOutcome::Continue => {
            m.dialog = Some(Dialog::GiveWork(g));
            Vec::new()
        }
        GiveOutcome::Close => Vec::new(),
        GiveOutcome::Send(cmd) => {
            if let Command::GiveWork { agent, task, run } = cmd.as_ref() {
                m.give_sent = Some((agent.clone(), task.clone(), *run));
            }
            vec![UiEffect::Send(*cmd)]
        }
    }
}
