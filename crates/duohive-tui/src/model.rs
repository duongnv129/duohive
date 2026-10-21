//! The UI's copy of engine state, updated from events.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use duohive_core::adapter::{CliPresence, ModelList};
use duohive_core::card::{Card, CardId};
use duohive_core::cli_kind::CliKind;
use duohive_core::command::Command;
use duohive_core::event::{AgentView, Event, NeedsYouItem, RoleOutcome, Snapshot};
use duohive_core::inbox::{MessageId, Sender};
use duohive_core::names::AgentName;
use duohive_core::org::{NodeKey, Org, RoleKey};
use duohive_core::ports::OrgFileError;
use duohive_core::settings::Settings;
use duohive_core::status::{AgentState, Status};
use duohive_core::time::Millis;
use duohive_core::validate::{DirProbe, DirStatus};
use duohive_core::worktree::{RunIn, WorktreeRow};

use ratatui::layout::Rect;

use crate::chat::{self, ChatLines};
use crate::first_run::FirstRun;
use crate::focus::Focus;
use crate::forms::{Chooser, Form, GiveWork, LineEdit, MoveTo, SettingsState};
use crate::keymap::{KeyMapState, Keymap, KeysState};
use crate::suggest::Suggest;

/// A dialog over the main screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    /// A yes/cancel question.
    Confirm(Box<Confirm>),
    /// A team, role, agent or Edit org form (UI §4.4.1).
    Form(Box<Form>),
    /// What `n` can create here (UI §4.4.2).
    Chooser(Chooser),
    /// Hand an agent a task (UI §4.5).
    GiveWork(Box<GiveWork>),
    /// DuoHive's worktrees (UI §4.9).
    Worktrees(WorktreeList),
    /// Where a node can move to (UI §4.4.2).
    MoveTo(Box<MoveTo>),
    /// Personal settings (UI §4.10).
    Settings(SettingsState),
    /// The key map (UI §4.11).
    KeyMap(KeyMapState),
    /// The key bindings editor (UI §4.10).
    Keys(KeysState),
}

/// The worktree list's own state; its rows are `UiModel::worktrees`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorktreeList {
    /// The selected row.
    pub cursor: usize,
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
    /// The worktree list that asked, with its cursor: cancel and yes go back to it.
    pub back_list: Option<WorktreeList>,
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
    /// The key bindings in use, built from `settings.keys` (UI §4.11).
    pub keymap: Keymap,
    /// Settings entries the key map ignored, one line each (UI §4.10).
    pub keys_notices: Vec<String>,
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
    /// The chat history.
    pub chat: ChatLines,
    /// The chat input.
    pub chat_input: LineEdit,
    /// The last line the chat sent, put back if the engine refuses it.
    pub chat_sent: Option<String>,
    /// Your messages not yet delivered (a `you` line shows `queued` while its id is here).
    pub queued: BTreeSet<MessageId>,
    /// The chat's open suggestion list.
    pub suggest: Option<Suggest>,
    /// The terminal's size, which bounds the chat's resizing.
    pub screen: Rect,
    /// The agent pane shows a frozen scrollback snapshot (P2-D12), not the live terminal.
    pub scrolled: bool,
    /// The agent whose live terminal the pane shows (its stream has delivered a `Reset`);
    /// kept by the app loop. Typing needs it.
    pub live_agent: Option<AgentName>,
    /// `z`: the selected agent's pane fills the screen (UI §4.2).
    pub zoomed: bool,
    /// The pane that had focus when zoom started; restoring goes back to it.
    pub zoom_from: crate::focus::Pane,
    /// The last Give work the dialog sent, put back if the engine refuses it.
    pub give_sent: Option<(AgentName, String, RunIn)>,
    /// A task that couldn't be put back yet; the next `space` on its agent opens it.
    pub give_later: Option<(AgentName, String, Option<RunIn>)>,
    /// The worktrees the engine lists, as it last published them.
    pub worktrees: Vec<WorktreeRow>,
    /// Pending approval cards (UI §4.6).
    pub cards: BTreeMap<CardId, Card>,
    /// No org exists yet: the CLIs detection found (UI §4.3).
    pub first_run: Option<Vec<(CliKind, CliPresence)>>,
    /// The first-run wizard, while `first_run` is `Some`.
    pub wizard: Option<FirstRun>,
    /// The tree filter by state (UI §4.1.1).
    pub filter: crate::filter::Filter,
    /// Each CLI's models and efforts once listed; absent while loading (TD §5.2).
    pub models: BTreeMap<CliKind, Result<ModelList, String>>,
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
            keymap: Keymap::default(),
            keys_notices: Vec::new(),
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
            queued: BTreeSet::new(),
            suggest: None,
            screen: Rect::default(),
            scrolled: false,
            live_agent: None,
            zoomed: false,
            zoom_from: crate::focus::Pane::Org,
            give_sent: None,
            give_later: None,
            worktrees: Vec::new(),
            cards: BTreeMap::new(),
            first_run: None,
            wizard: None,
            filter: crate::filter::Filter::default(),
            models: BTreeMap::new(),
        }
    }

    /// Opens `form` with the CLIs' model lists in its Model and Effort popups.
    pub fn open_form(&mut self, mut form: Form) {
        form.set_models(&self.models);
        self.dialog = Some(Dialog::Form(Box::new(form)));
    }

    /// Replaces the settings and rebuilds the key map from them (UI §4.11).
    pub fn set_settings(&mut self, settings: Settings) {
        let (keymap, notices) = Keymap::from_settings(&settings.keys);
        self.settings = settings;
        self.keymap = keymap;
        self.keys_notices = notices;
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
                let changed = self.update_view(&agent, |v| v.status = status);
                self.sync_give_work(&agent);
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
            Event::CommandRejected { reason } => {
                if self.reject_wizard(&reason) {
                    return true;
                }
                match &mut self.dialog {
                    Some(Dialog::Form(f)) if f.saving => f.reject(reason),
                    _ => match self.give_sent.clone() {
                        Some((agent, task, run)) => {
                            self.reopen_give_work(&agent, &task, Some(run), &reason, "");
                        }
                        None => self.footer_note = Some(reason),
                    },
                }
            }
            Event::GiveWorkRefused {
                agent,
                state,
                pending,
            } => {
                let reason = if pending {
                    chat::still_taking(&agent)
                } else {
                    format!("{agent} is {}", state.word())
                };
                self.refused_give_work(&agent, reason);
            }
            Event::GiveWorkNotTaken { agent, task, .. } => {
                let reason = format!("{agent} didn't take the task");
                self.give_sent = None;
                self.reopen_give_work(&agent, &task, None, &reason, NOT_TAKEN_RETRY);
            }
            Event::Reconnected => self.reconnecting = 0,
            Event::AgentRunChanged {
                agent,
                worktree,
                delivery_pending,
            } => {
                let changed = self.update_view(&agent, |v| {
                    v.worktree = worktree;
                    v.delivery_pending = delivery_pending;
                });
                self.sync_give_work(&agent);
                return changed;
            }
            Event::AlertClicked { agent } => {
                self.reveal_agent(&agent);
            }
            // The engine publishes this once per session (UI §4.1.4).
            Event::AlertsUnavailable => {
                self.footer_note = Some(ALERTS_UNAVAILABLE.to_owned());
            }
            Event::Worktrees(rows) => self.set_worktrees(rows),
            // The engine hands alerts to the notifier itself; the row's state already shows here.
            Event::Alert(_) => {}
            Event::ChatLine(entry) => chat::push(&mut self.chat, entry),
            e @ (Event::MessageQueued { .. }
            | Event::MessageDelivered { .. }
            | Event::MessageDropped { .. }) => self.apply_message(e),
            Event::InboxChanged { agent, waiting } => {
                return self.update_view(&agent, |v| v.inbox = waiting);
            }
            Event::MessageRefused { reason, .. } => chat::refused(self, reason),
            // The chat draws a pending card at its place (UI §4.6).
            Event::CardRaised(card) => {
                self.cards.insert(card.id, *card);
            }
            // Its `CardResult` chat line follows and takes its place.
            Event::CardAnswered { card, .. } => {
                self.cards.remove(&card);
                if self.focus.card == Some(card) {
                    self.focus.card = None;
                }
            }
            e @ (Event::RoleAssigned { .. } | Event::AssignRefused { .. }) => self.apply_role(e),
            // The wizard's org is saved: the main screen takes over (UI §4.3).
            Event::OrgCreated => (self.first_run, self.wizard) = (None, None),
            e @ (Event::OrgReplaced | Event::OrgReplaceFailed { .. }) => self.apply_replace(e),
            Event::ModelsListed { cli, result } => self.models_listed(cli, result),
            // Duo's tool results show as its action lines (UI §4.6).
            Event::ToolResult { .. } => {}
        }
        true
    }

    /// A CLI's lists arrived: kept, and handed to the open form's popups.
    fn models_listed(&mut self, cli: CliKind, result: Result<ModelList, String>) {
        self.models.insert(cli, result);
        if let Some(Dialog::Form(f)) = &mut self.dialog {
            f.set_models(&self.models);
        }
    }

    /// The end of a replace: saved, or an agent would not stop (UI §4.8).
    fn apply_replace(&mut self, e: Event) {
        self.wizard = None;
        self.footer_note = match e {
            Event::OrgReplaceFailed { agent: Some(a), .. } => {
                Some(format!("{a} would not stop · org not replaced"))
            }
            Event::OrgReplaceFailed { reason, .. } => Some(format!("{reason} · org not replaced")),
            _ => Some("org replaced".to_owned()),
        };
    }

    /// A refusal while the wizard is up: Create org keeps it open with the reason
    /// under its fields (UI §4.3).
    fn reject_wizard(&mut self, reason: &str) -> bool {
        let Some(w) = self.wizard.as_mut() else {
            return false;
        };
        w.write_error = Some(reason.to_owned());
        true
    }

    /// The answer to your task for a role: taken or carded closes its dialog with a note;
    /// refused keeps it open with the task and the reason under it (P3-D28).
    fn apply_role(&mut self, e: Event) {
        match e {
            Event::RoleAssigned { role, outcome, .. } => {
                if self.role_dialog_for(&role).is_some() {
                    self.dialog = None;
                }
                self.footer_note = Some(match outcome {
                    RoleOutcome::Given { agent, .. } => format!("{agent} took the task"),
                    RoleOutcome::Card(_) => NO_FREE_CARD.to_owned(),
                });
            }
            Event::AssignRefused { role, reason, .. } => match self.role_dialog_for(&role) {
                Some(g) => {
                    g.sent = false;
                    g.note = Some(reason);
                }
                None => self.footer_note = Some(reason),
            },
            _ => {}
        }
    }

    /// A message event. Messages between agents show only as `✉` (UI §4.7): only yours
    /// are tracked, and a dropped one of yours comes back into the input (P3-D26).
    fn apply_message(&mut self, e: Event) {
        match e {
            Event::MessageQueued {
                id,
                from: Sender::You,
                ..
            } => {
                self.queued.insert(id);
                // Accepted: a later refusal is not about the line the chat sent.
                self.chat_sent = None;
            }
            Event::MessageDelivered { id } => {
                self.queued.remove(&id);
            }
            Event::MessageDropped {
                id,
                from,
                to,
                text,
                reason,
            } => {
                self.queued.remove(&id);
                if from == Sender::You {
                    chat::dropped(self, &to, &text, &reason);
                }
            }
            _ => {}
        }
    }

    /// The open Give work dialog follows its agent's state, or a role's its agents'.
    fn sync_give_work(&mut self, agent: &AgentName) {
        let Some(Dialog::GiveWork(g)) = &self.dialog else {
            return;
        };
        match g.role().cloned() {
            Some(role) => {
                let member = crate::input::find_role(&self.org, &role)
                    .is_some_and(|r| r.agents.iter().any(|a| &a.name == agent));
                let fresh = member
                    .then(|| crate::input::give_work::role_dialog(self, &role, ""))
                    .flatten();
                if let (Some(fresh), Some(Dialog::GiveWork(g))) = (fresh, &mut self.dialog) {
                    g.follow_role(fresh);
                }
            }
            None if &g.agent == agent => {
                let view = self.view(agent).clone();
                if let Some(Dialog::GiveWork(g)) = &mut self.dialog {
                    g.sync(&view);
                }
            }
            None => {}
        }
    }

    /// The open Give work dialog, when it is `role`'s.
    fn role_dialog_for(&mut self, role: &RoleKey) -> Option<&mut GiveWork> {
        match &mut self.dialog {
            Some(Dialog::GiveWork(g)) if g.role() == Some(role) => Some(g),
            _ => None,
        }
    }

    /// The engine refused the last Give work: its text comes back in the dialog.
    fn refused_give_work(&mut self, agent: &AgentName, reason: String) {
        match self.give_sent.clone() {
            Some((a, task, run)) if &a == agent => {
                self.reopen_give_work(agent, &task, Some(run), &reason, "");
            }
            _ => self.footer_note = Some(reason),
        }
    }

    /// Give work opens again with `task` in the box and `reason` under it (P2-D11, P2-D20).
    /// Typing mode, a chat draft and another dialog keep the keyboard: the footer says
    /// why and the task waits for the next `space` on the agent.
    fn reopen_give_work(
        &mut self,
        agent: &AgentName,
        task: &str,
        run: Option<RunIn>,
        reason: &str,
        dialog_suffix: &str,
    ) {
        if self.org.find_agent(agent).is_none() {
            return;
        }
        self.give_sent = None;
        let busy = self.dialog.is_some()
            || self.typing()
            || (self.focus.pane == crate::focus::Pane::Chat && !self.chat_input.text().is_empty());
        if busy {
            self.give_later = Some((agent.clone(), task.to_owned(), run));
            self.footer_note = Some(format!("{reason}{SPACE_RETRY}"));
            return;
        }
        self.give_later = None;
        let mut g = GiveWork::new(
            &self.org,
            agent,
            self.view(agent),
            task,
            self.dirs.probe(),
            &self.home,
        );
        g.set_run(run);
        g.note = Some(format!("{reason}{dialog_suffix}"));
        self.dialog = Some(Dialog::GiveWork(Box::new(g)));
    }

    /// Selects `agent`, unfolding its team and role, and focuses the agent pane in
    /// Selected mode (UI §4.1.3, §4.1.4). `false` when the org doesn't name it.
    pub fn reveal_agent(&mut self, agent: &AgentName) -> bool {
        if self.org.is_duo(agent) {
            self.selected = NodeKey::Duo;
        } else {
            let Some(loc) = self.org.find_agent(agent) else {
                return false;
            };
            let team = loc.team.map(|t| t.name.clone());
            let role = RoleKey {
                team: team.clone(),
                role: loc.role.name.clone(),
            };
            if let Some(t) = team {
                self.folded.remove(&NodeKey::Team(t));
            }
            self.folded.remove(&NodeKey::Role(role));
            self.selected = NodeKey::Agent(agent.clone());
        }
        self.focus.pane = crate::focus::Pane::Agent;
        self.focus.agent_mode = crate::focus::AgentMode::Selected;
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

    /// Replaces the Needs you list. The cursor stays on the same agent or card; when it
    /// has left, the cursor keeps its row number, clamped to the new last row (UI §4.1.3).
    fn set_needs_you(&mut self, items: Vec<NeedsYouItem>) {
        let at = self.focus.needs_you_cursor;
        let key = |i: &NeedsYouItem| (i.agent.clone(), i.card);
        let under_cursor = self.needs_you.get(at).map(key);
        let same = under_cursor.and_then(|k| items.iter().position(|i| key(i) == k));
        let last = items.len().saturating_sub(1);
        self.focus.needs_you_cursor = same.unwrap_or(at).min(last);
        self.needs_you = items;
    }

    /// Replaces the worktree rows. An open list's cursor stays on the same row, or keeps its
    /// row number when that row has gone, clamped to the last.
    fn set_worktrees(&mut self, rows: Vec<WorktreeRow>) {
        let key = |r: &WorktreeRow| (r.agent.clone(), r.task.clone());
        if let Some(Dialog::Worktrees(l)) = &mut self.dialog {
            let under = self.worktrees.get(l.cursor).map(key);
            let same = under.and_then(|k| rows.iter().position(|r| key(r) == k));
            l.cursor = same.unwrap_or(l.cursor).min(rows.len().saturating_sub(1));
        }
        self.worktrees = rows;
    }

    fn apply_snapshot(&mut self, s: Snapshot) {
        self.org = s.org;
        self.agents = s.agents;
        self.set_needs_you(s.needs_you);
        self.org_error = s.org_error;
        self.settings_error = s.settings_error;
        self.reconnecting = s.reconnecting;
        self.set_worktrees(s.worktrees);
        self.chat.entries = s.chat;
        self.chat.scroll = None;
        self.queued = s.your_queued.into_iter().collect();
        self.cards = s.cards.into_iter().map(|c| (c.id, c)).collect();
        // No org yet: the wizard, once, so a later snapshot keeps what was typed.
        self.first_run = s.first_run;
        match &self.first_run {
            Some(list) if self.wizard.is_none() => {
                let name = self.settings.your_name.clone();
                self.wizard = Some(FirstRun::new(list.clone(), name));
            }
            Some(_) => {}
            None => self.wizard = None,
        }
        if self
            .focus
            .card
            .is_some_and(|c| !self.cards.contains_key(&c))
        {
            self.focus.card = None;
        }
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
        if self.selected_agent().is_none() {
            self.zoomed = false;
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
        waiting_on: None,
    },
    last_reply: None,
    notice: None,
    problem: None,
    worktree: None,
    delivery_pending: false,
    inbox: 0,
};

/// What the reopened dialog adds when the task wasn't taken.
const NOT_TAKEN_RETRY: &str = " · enter to try again";
/// Ends a note about work that waits for `space`; not shown while typing, where `space`
/// goes to the agent.
pub(crate) const SPACE_RETRY: &str = " · space to try again";
/// The footer note when a role's task raised a clone card (UI §4.5, P3-D3).
const NO_FREE_CARD: &str = "No free agent · a card in the chat proposes one";

/// The footer note when the system has no desktop notifier (UI §4.1.4).
const ALERTS_UNAVAILABLE: &str = "desktop alerts unavailable";

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
    fn alerts_unavailable_note_once() {
        let mut m = spec_model();
        m.apply(Event::AlertsUnavailable);
        assert_eq!(m.footer_note.as_deref(), Some("desktop alerts unavailable"));
        m.footer_note = None;
        m.apply(Event::Reconnected);
        assert_eq!(m.footer_note, None);
    }

    #[test]
    fn alert_click_selects_agent() {
        let mut m = spec_model();
        let agent = n("pay-qa");
        m.focus.pane = crate::focus::Pane::Org;
        m.apply(Event::AlertClicked {
            agent: agent.clone(),
        });
        assert_eq!(m.selected, NodeKey::Agent(agent));
        assert_eq!(m.focus.pane, crate::focus::Pane::Agent);
        assert_eq!(m.focus.agent_mode, crate::focus::AgentMode::Selected);
    }

    #[test]
    fn alert_click_unfolds_parents() {
        let mut m = spec_model();
        let agent = n("pay-qa");
        let loc = m.org.find_agent(&agent).unwrap();
        let team = NodeKey::Team(loc.team.unwrap().name.clone());
        let role = NodeKey::Role(RoleKey {
            team: loc.team.map(|t| t.name.clone()),
            role: loc.role.name.clone(),
        });
        m.folded.insert(team.clone());
        m.folded.insert(role.clone());
        m.apply(Event::AlertClicked { agent });
        assert!(!m.folded.contains(&team) && !m.folded.contains(&role));
    }

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
            card: None,
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
            cards: Vec::new(),
            chat: Vec::new(),
            your_queued: Vec::new(),
            first_run: None,
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
