//! The UI's copy of engine state, updated from events.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use duohive_core::command::Command;
use duohive_core::event::{AgentView, Event, NeedsYouItem, Snapshot};
use duohive_core::names::AgentName;
use duohive_core::org::{NodeKey, Org, RoleKey};
use duohive_core::ports::OrgFileError;
use duohive_core::settings::Settings;
use duohive_core::status::{AgentState, Status};
use duohive_core::time::Millis;
use duohive_core::validate::{DirProbe, DirStatus};

use ratatui::layout::Rect;

use crate::chat::{self, ChatLines};
use crate::focus::Focus;
use crate::forms::{Chooser, Form, LineEdit};

/// A dialog over the main screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    /// A yes/cancel question.
    Confirm(Box<Confirm>),
    /// A team, role, agent or Edit org form (UI §4.4.1).
    Form(Box<Form>),
    /// What `n` can create here (UI §4.4.2).
    Chooser(Chooser),
}

/// A confirmation dialog (UI §4.8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirm {
    /// The question.
    pub title: String,
    /// Body lines.
    pub lines: Vec<String>,
    /// The yes button's label.
    pub yes_label: String,
    /// Whether the yes button has focus (dialogs open on Cancel).
    pub focus_yes: bool,
    /// What yes sends.
    pub on_yes: Command,
    /// The form that asked: cancel goes back to it, yes leaves it waiting for the engine.
    pub back: Option<Box<Form>>,
}

/// The directory probe forms use; equal only to itself, so the model stays comparable.
#[derive(Clone)]
pub struct SharedDirs(pub Arc<dyn DirProbe>);

impl SharedDirs {
    /// The probe.
    pub fn probe(&self) -> &dyn DirProbe {
        self.0.as_ref()
    }
}

impl PartialEq for SharedDirs {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for SharedDirs {}

impl fmt::Debug for SharedDirs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SharedDirs(..)")
    }
}

/// Until the app sets the real probe: every directory exists and none is a repo.
struct NoProbe;

impl DirProbe for NoProbe {
    fn probe(&self, _dir: &Path) -> DirStatus {
        DirStatus::Found
    }
}

/// Everything the main screen draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiModel {
    /// The org in use.
    pub org: Org,
    /// Per-agent views, Duo included.
    pub agents: BTreeMap<AgentName, AgentView>,
    /// Needs you, oldest first.
    pub needs_you: Vec<NeedsYouItem>,
    /// `org.toml` doesn't parse: the banner.
    pub org_error: Option<OrgFileError>,
    /// `settings.toml` is invalid (D29): the banner.
    pub settings_error: Option<OrgFileError>,
    /// The selected node.
    pub selected: NodeKey,
    /// A one-off message that replaces the footer's keys.
    pub footer_note: Option<String>,
    /// The open dialog.
    pub dialog: Option<Dialog>,
    /// Agents still being reattached at start-up; 0 = none.
    pub reconnecting: u32,
    /// The settings in use; `v` changes and saves them.
    pub settings: Settings,
    /// The time ages are measured to when an action needs one (the stop dialog).
    pub now: Millis,
    /// `org.toml`, which `o` opens in `$EDITOR`.
    pub org_toml: PathBuf,
    /// Probes the directories forms show (`✓ git repo`).
    pub dirs: SharedDirs,
    /// The home directory `~` expands to.
    pub home: PathBuf,
    /// Which pane has focus.
    pub focus: Focus,
    /// Folded teams and roles.
    pub folded: BTreeSet<NodeKey>,
    /// Rows `ctrl+↑` added to the chat, within P2-D3's bounds.
    pub chat_rows_extra: i16,
    /// The chat history (P2-D8).
    pub chat: ChatLines,
    /// The chat input.
    pub chat_input: LineEdit,
    /// The last text the chat sent, put back if Duo doesn't take it.
    pub chat_sent: Option<String>,
    /// The terminal's size, which bounds the chat's resizing.
    pub screen: Rect,
    /// The agent pane shows a frozen scrollback snapshot (P2-D12), not the live terminal.
    pub scrolled: bool,
    /// The agent whose live terminal the pane shows (its stream has delivered a `Reset`);
    /// kept by the app loop. Typing needs it.
    pub live_agent: Option<AgentName>,
    /// `z`: the selected agent's pane fills the screen (UI §4.2).
    pub zoomed: bool,
}

impl UiModel {
    /// A model over `org` with no agent views yet, Duo selected.
    pub fn new(org: Org) -> Self {
        Self {
            org,
            agents: BTreeMap::new(),
            needs_you: Vec::new(),
            org_error: None,
            settings_error: None,
            selected: NodeKey::Duo,
            footer_note: None,
            dialog: None,
            reconnecting: 0,
            settings: Settings::default(),
            now: Millis(0),
            org_toml: PathBuf::new(),
            dirs: SharedDirs(Arc::new(NoProbe)),
            home: PathBuf::new(),
            focus: Focus::default(),
            folded: BTreeSet::new(),
            chat_rows_extra: 0,
            chat: ChatLines::default(),
            chat_input: LineEdit::default(),
            chat_sent: None,
            screen: Rect::default(),
            scrolled: false,
            live_agent: None,
            zoomed: false,
        }
    }

    /// Applies an engine event; `true` when the screen needs a redraw.
    pub fn apply(&mut self, e: Event) -> bool {
        match e {
            Event::Snapshot(s) => self.apply_snapshot(*s),
            Event::OrgChanged { org, problems } => {
                self.apply_org(*org, problems);
                // The edit a form was waiting for (or another) is in: the form is done.
                if matches!(&self.dialog, Some(Dialog::Form(f)) if f.saving) {
                    self.dialog = None;
                }
            }
            Event::AgentStatusChanged { agent, status } => {
                let before = self.view(&agent).status.state;
                let changed = self.update_view(&agent, |v| v.status = status);
                if changed && self.org.is_duo(&agent) {
                    chat::duo_status_changed(self, before);
                }
                return changed;
            }
            Event::LastReplyChanged { agent, reply } => {
                return self.update_view(&agent, |v| v.last_reply = reply);
            }
            Event::AgentNoticeChanged { agent, notice } => {
                return self.update_view(&agent, |v| v.notice = notice);
            }
            Event::NeedsYouChanged(items) => {
                self.set_needs_you(items);
            }
            Event::OrgFileError(e) => self.org_error = e,
            Event::CommandRejected { reason } => match &mut self.dialog {
                Some(Dialog::Form(f)) if f.saving => f.reject(reason),
                _ => self.footer_note = Some(reason),
            },
            // Phase 2 gives Duo work only from the chat (P2-D8, P2-D10).
            Event::GiveWorkRefused {
                agent,
                state,
                pending,
            } if self.org.is_duo(&agent) => chat::refused(self, state, pending),
            Event::GiveWorkRefused {
                agent,
                pending: true,
                ..
            } => self.footer_note = Some(chat::still_taking(&agent)),
            Event::GiveWorkRefused { agent, state, .. } => {
                self.footer_note = Some(format!("{agent} is {}", state.word()));
            }
            Event::GiveWorkNotTaken { agent, .. } if self.org.is_duo(&agent) => {
                chat::not_taken(self);
            }
            Event::Reconnected => self.reconnecting = 0,
            Event::AgentRunChanged {
                agent,
                worktree,
                delivery_pending,
            } => {
                return self.update_view(&agent, |v| {
                    v.worktree = worktree;
                    v.delivery_pending = delivery_pending;
                });
            }
            // Alerts and worktrees reach the screen in later tasks.
            Event::Alert(_)
            | Event::AlertsUnavailable
            | Event::AlertClicked { .. }
            | Event::GiveWorkNotTaken { .. }
            | Event::Worktrees(_) => {}
        }
        true
    }

    /// Typing mode has the keys: the agent pane is focused and Typing, under no dialog.
    pub fn typing(&self) -> bool {
        self.dialog.is_none()
            && self.focus.pane == crate::focus::Pane::Agent
            && self.focus.agent_mode == crate::focus::AgentMode::Typing
    }

    /// The agent whose pane fills the screen: the selected agent while `z` has zoomed.
    pub fn zoomed_agent(&self) -> Option<&AgentName> {
        self.zoomed.then(|| self.selected_agent()).flatten()
    }

    /// The agent a selection shows: Duo's name for Duo, `None` for teams and roles.
    pub fn selected_agent(&self) -> Option<&AgentName> {
        match &self.selected {
            NodeKey::Duo => Some(&self.org.duo.name),
            NodeKey::Agent(a) => Some(a),
            NodeKey::Team(_) | NodeKey::Role(_) => None,
        }
    }

    /// Whether `key` names a node of the current org.
    pub fn has_node(&self, key: &NodeKey) -> bool {
        match key {
            NodeKey::Duo => true,
            NodeKey::Team(t) => self.org.teams.iter().any(|x| &x.name == t),
            NodeKey::Role(RoleKey { team, role }) => {
                let roles = match team {
                    Some(t) => match self.org.teams.iter().find(|x| &x.name == t) {
                        Some(x) => &x.roles,
                        None => return false,
                    },
                    None => &self.org.roles,
                };
                roles.iter().any(|r| &r.name == role)
            }
            NodeKey::Agent(a) => self.org.is_duo(a) || self.org.find_agent(a).is_some(),
        }
    }

    /// Replaces the Needs you list. The cursor stays on the same agent; when that agent
    /// has left, it keeps its row number, clamped to the new last row (UI §4.1.3).
    fn set_needs_you(&mut self, items: Vec<NeedsYouItem>) {
        let at = self.focus.needs_you_cursor;
        let under_cursor = self.needs_you.get(at).map(|i| i.agent.clone());
        let same = under_cursor.and_then(|a| items.iter().position(|i| i.agent == a));
        let last = items.len().saturating_sub(1);
        self.focus.needs_you_cursor = same.unwrap_or(at).min(last);
        self.needs_you = items;
    }

    fn apply_snapshot(&mut self, s: Snapshot) {
        self.org = s.org;
        self.agents = s.agents;
        self.set_needs_you(s.needs_you);
        self.org_error = s.org_error;
        self.settings_error = s.settings_error;
        self.reconnecting = s.reconnecting;
        self.keep_selection_valid();
    }

    fn apply_org(&mut self, org: Org, mut problems: BTreeMap<AgentName, String>) {
        self.org = org;
        let names: Vec<AgentName> = self.agent_names().into_iter().cloned().collect();
        let mut old = std::mem::take(&mut self.agents);
        self.agents = names
            .into_iter()
            .map(|name| {
                let view = old.remove(&name).unwrap_or_else(|| UNKNOWN_VIEW.clone());
                let problem = problems.remove(&name);
                (name, AgentView { problem, ..view })
            })
            .collect();
        self.keep_selection_valid();
    }

    /// The view of `agent`, or a stopped one when the engine hasn't reported it yet.
    pub fn view(&self, agent: &AgentName) -> &AgentView {
        self.agents.get(agent).unwrap_or(&UNKNOWN_VIEW)
    }

    /// Duo, then every agent in tree order.
    pub fn agent_names(&self) -> Vec<&AgentName> {
        std::iter::once(&self.org.duo.name)
            .chain(self.org.agents().into_iter().map(|a| &a.agent.name))
            .collect()
    }

    /// Changes a known agent's view; an agent not in the org is ignored (no redraw).
    fn update_view(&mut self, agent: &AgentName, f: impl FnOnce(&mut AgentView)) -> bool {
        let known = self.org.is_duo(agent) || self.org.find_agent(agent).is_some();
        if !known {
            return false;
        }
        f(self
            .agents
            .entry(agent.clone())
            .or_insert_with(|| UNKNOWN_VIEW.clone()));
        true
    }

    fn keep_selection_valid(&mut self) {
        if !self.has_node(&self.selected) {
            self.selected = NodeKey::Duo;
        }
        let gone: Vec<NodeKey> = self
            .folded
            .iter()
            .filter(|k| !self.has_node(k))
            .cloned()
            .collect();
        for key in gone {
            self.folded.remove(&key);
        }
    }
}

/// A view for an agent the engine has not reported yet: stopped, since an unknown time.
pub(crate) static UNKNOWN_VIEW: AgentView = AgentView {
    status: Status {
        state: AgentState::Stopped,
        quiet: false,
        quiet_since: None,
        from_screen: false,
        since: Millis(0),
        error: None,
    },
    last_reply: None,
    notice: None,
    problem: None,
    worktree: None,
    delivery_pending: false,
};

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use duohive_core::event::{Event, NeedsYouItem};
    use duohive_core::org::NodeKey;
    use duohive_core::ports::OrgFileError;
    use duohive_core::status::AgentState;
    use duohive_core::testkit::{example_org, n};

    use super::*;
    use crate::fixtures::{NOW, mins, spec_model, status};

    #[test]
    fn apply_events_update_model() {
        let mut m = spec_model();
        let s = status(AgentState::Waiting, mins(0));
        assert!(m.apply(Event::AgentStatusChanged {
            agent: n("squad-2"),
            status: s.clone(),
        }));
        assert_eq!(m.agents[&n("squad-2")].status, s);
        let item = NeedsYouItem {
            agent: n("squad-2"),
            state: AgentState::Waiting,
            quiet: false,
            since: NOW,
            excerpt: None,
        };
        assert!(m.apply(Event::NeedsYouChanged(vec![item.clone()])));
        assert_eq!(m.needs_you, [item]);
        assert!(m.apply(Event::LastReplyChanged {
            agent: n("squad-2"),
            reply: Some("done".into()),
        }));
        assert_eq!(m.agents[&n("squad-2")].last_reply.as_deref(), Some("done"));
        assert!(m.apply(Event::AgentNoticeChanged {
            agent: n("squad-2"),
            notice: Some("notice: x".into()),
        }));
        assert_eq!(m.agents[&n("squad-2")].notice.as_deref(), Some("notice: x"));
        let err = OrgFileError {
            line: Some(1),
            message: "bad".into(),
        };
        assert!(m.apply(Event::OrgFileError(Some(err.clone()))));
        assert_eq!(m.org_error, Some(err));
        assert!(m.apply(Event::CommandRejected {
            reason: "pay-qa is already running".into(),
        }));
        assert_eq!(m.footer_note.as_deref(), Some("pay-qa is already running"));
    }

    #[test]
    fn events_for_unknown_agents_are_ignored() {
        let mut m = spec_model();
        let before = m.clone();
        assert!(!m.apply(Event::AgentStatusChanged {
            agent: n("ghost"),
            status: status(AgentState::Working, mins(1)),
        }));
        assert!(!m.apply(Event::LastReplyChanged {
            agent: n("ghost"),
            reply: None,
        }));
        assert!(!m.apply(Event::AgentNoticeChanged {
            agent: n("ghost"),
            notice: None,
        }));
        assert_eq!(m, before);
    }

    #[test]
    fn org_changed_sets_problems_and_keeps_a_valid_selection() {
        let mut m = spec_model();
        let mut org = example_org();
        org.roles.clear();
        m.selected = NodeKey::Agent(n("squad-1"));
        let problems = BTreeMap::from([(n("pay-qa"), "no directory set · e to fix".to_owned())]);
        assert!(m.apply(Event::OrgChanged {
            org: Box::new(org),
            problems,
        }));
        assert_eq!(
            m.agents[&n("pay-qa")].problem.as_deref(),
            Some("no directory set · e to fix")
        );
        assert_eq!(m.agents[&n("auth-service")].problem, None);
        assert!(!m.agents.contains_key(&n("squad-1")));
        assert!(m.agents.contains_key(&n("Duo")));
        assert_eq!(m.selected, NodeKey::Duo);
    }

    #[test]
    fn snapshot_replaces_everything_and_reconnected_clears() {
        let mut m = UiModel::new(example_org());
        assert_eq!(m.selected, NodeKey::Duo);
        let spec = spec_model();
        let snap = duohive_core::event::Snapshot {
            org: spec.org.clone(),
            agents: spec.agents.clone(),
            needs_you: spec.needs_you.clone(),
            org_error: None,
            settings_error: Some(OrgFileError {
                line: None,
                message: "x".into(),
            }),
            reconnecting: 3,
            worktrees: Vec::new(),
        };
        assert!(m.apply(Event::Snapshot(Box::new(snap))));
        assert_eq!(m.agents, spec.agents);
        assert_eq!(m.needs_you, spec.needs_you);
        assert_eq!(m.reconnecting, 3);
        assert!(m.settings_error.is_some());
        assert!(m.apply(Event::Reconnected));
        assert_eq!(m.reconnecting, 0);
    }
}
