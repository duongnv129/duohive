//! `space`: opening Give work, and its keys (UI §4.5, P3-D28).

use duohive_core::command::Command;
use duohive_core::event::AgentView;
use duohive_core::names::AgentName;
use duohive_core::org::{NodeKey, RoleKey};

use super::{Action, UiEffect};
use crate::forms::{GiveOutcome, GiveWork};
use crate::model::{Dialog, UiModel};

/// Said on a team or on Duo.
const AGENT_OR_ROLE: &str = "space gives work to an agent or a role";

/// `space` on the selection: an agent or a role opens the dialog; Duo or a team say why not.
pub(super) fn open(m: &mut UiModel) -> Vec<UiEffect> {
    let note = match &m.selected {
        NodeKey::Duo => AGENT_OR_ROLE,
        NodeKey::Agent(a) if m.org.is_duo(a) => AGENT_OR_ROLE,
        NodeKey::Team(_) => AGENT_OR_ROLE,
        NodeKey::Role(r) => {
            if let Some(g) = role_dialog(m, r, "") {
                m.dialog = Some(Dialog::GiveWork(Box::new(g)));
            }
            return Vec::new();
        }
        NodeKey::Agent(a) => {
            let later = match m.give_later.take() {
                Some((who, task, run)) if who == *a => Some((task, run)),
                other => {
                    m.give_later = other;
                    None
                }
            };
            let (task, run) = later.unwrap_or_default();
            let mut g = GiveWork::new(&m.org, a, m.view(a), &task, m.dirs.probe(), &m.home);
            g.set_run(run);
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
            if let Command::SendMessage { to, .. } = cmd.as_ref() {
                m.footer_note = Some(format!("→ queued for {to}"));
            }
            vec![UiEffect::Send(*cmd)]
        }
        GiveOutcome::Wait(cmd) => {
            m.dialog = Some(Dialog::GiveWork(g));
            vec![UiEffect::Send(*cmd)]
        }
    }
}

/// The role's dialog from its agents' states now, with `task` in the box.
pub(crate) fn role_dialog(m: &UiModel, role: &RoleKey, task: &str) -> Option<GiveWork> {
    let found = super::find_role(&m.org, role)?;
    let members: Vec<(AgentName, AgentView)> = found
        .agents
        .iter()
        .map(|a| (a.name.clone(), m.view(&a.name).clone()))
        .collect();
    Some(GiveWork::for_role(
        &m.org,
        role,
        &members,
        task,
        m.dirs.probe(),
        &m.home,
    ))
}
