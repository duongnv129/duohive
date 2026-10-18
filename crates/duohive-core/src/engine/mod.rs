//! The sans-I/O engine (TD §3.3): one owner of all state. It takes commands and
//! adapter inputs, publishes events, and reaches the world only through the ports.

mod alerts;
mod cards;
mod chat;
mod driver;
#[cfg(test)]
mod harness;
mod inbox;
mod lifecycle;
mod org_edit;
mod reconnect;
mod rights;
mod signals;
mod timers;
mod tools;
mod worktrees;

pub use driver::run_core;
pub use org_edit::preview_edit;
use reconnect::{Held, Reconnect};
use rights::Caller;
use signals::Exit;
use worktrees::Creating;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::adapter::{Adapters, CliPresence};
use crate::card::Card;
use crate::chat::ChatEntry;
use crate::cli_kind::CliKind;
use crate::command::Command;
use crate::event::{AgentView, Event, NeedsYouItem, Snapshot};
use crate::hook::HookMessage;
use crate::inbox::{Message, MessageId};
use crate::inherit::{expand_home, resolve};
use crate::launch::{Token, WindowId};
use crate::names::AgentName;
use crate::org::Org;
use crate::ports::{
    Clock, Git, GitEvent, Ids, Notifier, OrgFileError, Runtime, RuntimeEvent, SessionRow, Store,
};
use crate::settings::Settings;
use crate::status::{AgentState, Status, Trigger, WaitingOn, transition};
use crate::time::Millis;
use crate::tool::ToolCall;
use crate::validate::{DirProbe, DirStatus, agent_problem};
use crate::worktree::{TaskId, WorktreeRow};

/// Timings and places the engine needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineConfig {
    /// The user's home directory.
    pub home: PathBuf,
    /// DuoHive's data directory (D18).
    pub data_dir: PathBuf,
    /// The `duohive` executable that hooks call.
    pub hook_exe: PathBuf,
    /// How long a start may take before it errors (120 s, D6).
    pub start_timeout: Duration,
    /// How long a stop waits after the quit keys before killing (5 s, D6).
    pub stop_grace: Duration,
    /// Settle read delay after the last activity change (1.5 s, D6).
    pub settle_after: Duration,
    /// How long a zero exit waits for an end hook (500 ms, D21).
    pub end_hook_grace: Duration,
}

/// The ports the engine talks through.
pub struct Ports {
    /// The terminal multiplexer.
    pub runtime: Box<dyn Runtime>,
    /// Persistence.
    pub store: Box<dyn Store>,
    /// The wall clock.
    pub clock: Box<dyn Clock>,
    /// Token source.
    pub ids: Box<dyn Ids>,
    /// Directory probe (D26).
    pub dirs: Arc<dyn DirProbe>,
    /// Desktop alerts.
    pub notifier: Box<dyn Notifier>,
    /// Git worktrees.
    pub git: Box<dyn Git>,
}

/// Everything the engine reacts to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    /// A command from the UI (or Duo).
    Command(Command),
    /// A report from the runtime.
    Runtime(RuntimeEvent),
    /// A hook from an agent's CLI.
    Hook(HookMessage),
    /// Which CLIs are installed; launches wait for the first one (D30).
    CliInventory(Vec<(CliKind, CliPresence)>),
    /// D26: every resolved agent directory, probed after the first frame.
    DirInventory(Vec<(PathBuf, DirStatus)>),
    /// A report from git.
    Git(GitEvent),
    /// The user clicked an agent's alert.
    AlertClicked {
        /// Which agent.
        agent: AgentName,
    },
    /// A timer is due.
    Tick,
}

/// A zero exit waiting briefly for its end hook (D21).
#[derive(Clone, Debug, PartialEq, Eq)]
struct ExitPending {
    window: WindowId,
    status: Option<i32>,
    due: Millis,
}

/// A task pasted as the next turn, waiting for a sign the agent took it (P2-D11).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Delivery {
    task: String,
    due: Millis,
    /// The message this paste carries; `None` for a task you gave.
    message: Option<MessageId>,
}

/// One agent's runtime state.
#[derive(Clone, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent per-agent flags read separately across the engine"
)]
struct AgentRt {
    status: Status,
    /// The current launch's token (D1).
    token: Option<Token>,
    /// The current window: set by `Started`, cleared on a relaunch until the next `Started`.
    window: Option<WindowId>,
    task_at_launch: bool,
    stopping: bool,
    saw_end_hook: bool,
    last_signal_at: Millis,
    /// The newest `sent_at_ms` of a hook applied this launch (0 = none), so a
    /// hook that arrives after a newer one moves nothing.
    last_hook_sent_at: u64,
    last_activity_secs: Option<u64>,
    settle_due: Option<Millis>,
    start_deadline: Option<Millis>,
    last_reply: Option<String>,
    notice: Option<String>,
    last_task: Option<String>,
    restart_after_stop: bool,
    /// The `NeedsInput { prompt }` text while waiting (Task 13's excerpt).
    waiting_prompt: Option<String>,
    pre_read_done: bool,
    exit_pending: Option<ExitPending>,
    /// The window the start timeout killed, so a late `Started` for it is ignored.
    /// Kept across launches: tmux window ids are unique, so it never matches a new window.
    killed_window: Option<WindowId>,
    /// The CLI the current session was launched on (org edits apply on the next start).
    cli: Option<CliKind>,
    /// The worktree the current session runs in (saved on its session row).
    worktree: Option<TaskId>,
    /// The current session was launched as Duo (P3-D30).
    launched_as_duo: bool,
    /// A next-turn paste not yet taken (P2-D11).
    delivery: Option<Delivery>,
    /// The worktree being created for the next launch (P2-D20).
    creating: Option<Creating>,
    /// Your message the next launch carries as its task (P3-D18).
    launch_message: Option<MessageId>,
}

impl AgentRt {
    fn stopped(now: Millis) -> Self {
        Self {
            status: Status {
                state: AgentState::Stopped,
                quiet: false,
                quiet_since: None,
                from_screen: false,
                since: now,
                error: None,
                waiting_on: None,
            },
            token: None,
            window: None,
            task_at_launch: false,
            stopping: false,
            saw_end_hook: false,
            last_signal_at: now,
            last_hook_sent_at: 0,
            last_activity_secs: None,
            settle_due: None,
            start_deadline: None,
            last_reply: None,
            notice: None,
            last_task: None,
            restart_after_stop: false,
            waiting_prompt: None,
            pre_read_done: false,
            exit_pending: None,
            killed_window: None,
            cli: None,
            worktree: None,
            launched_as_duo: false,
            delivery: None,
            creating: None,
            launch_message: None,
        }
    }
}

/// The engine. See the module docs.
pub struct Engine {
    org: Org,
    org_error: Option<OrgFileError>,
    settings: Settings,
    settings_error: Option<OrgFileError>,
    agents: BTreeMap<AgentName, AgentRt>,
    /// `None` until the first `CliInventory` (D30).
    cli: Option<HashMap<CliKind, CliPresence>>,
    /// `None` until the first `DirInventory`: pending (D26).
    dirs: Option<HashMap<PathBuf, DirStatus>>,
    /// Launch commands that arrived before the first `CliInventory`, in order (D30).
    queued_launches: Vec<Command>,
    /// The Needs you list last reported in `NeedsYouChanged`.
    needs_you_shown: Vec<NeedsYouItem>,
    /// Set from `start_up` until the start-up inventory is applied (Task 15).
    reconnect: Option<Reconnect>,
    /// Start-up is replaying history: no alerts until `Reconnected` (P2-D14).
    replaying: bool,
    /// Rows of running agents no org names (controller ruling B).
    held: Held,
    /// The worktree rows last published in `Worktrees` (and `Snapshot`).
    worktrees_shown: Vec<WorktreeRow>,
    /// A worktree row, or whether one is in use, may have changed since.
    worktrees_stale: bool,
    /// When the 10-minute worktree check is next due (TD §5.3).
    worktree_check_due: Millis,
    /// Each worktree row's incarnation, given at its first check and dropped
    /// with the row, so a late answer never reaches a newer row of the same
    /// agent and task.
    worktree_generations: HashMap<(AgentName, TaskId), u64>,
    /// The next incarnation number.
    next_generation: u64,
    /// Messages queued or delivered, and the last 10 minutes' others, by id (TD §6.3).
    inbox: Vec<Message>,
    /// The next message id.
    next_message: u64,
    /// The agent you are typing into, which nothing is pasted into (P3-D19).
    typing: Option<AgentName>,
    /// Calls queued while DuoHive was closed, with the callers start-up
    /// found for their tokens; run after `Reconnected` (P3-D24).
    closed_calls: Vec<(Caller, ToolCall)>,
    /// The chat's last [`crate::chat::CHAT_LOAD`] entries, oldest first (TD §6.5).
    chat: Vec<ChatEntry>,
    /// The next chat entry id.
    next_chat: u64,
    /// Pending cards, oldest first (TD §6.2).
    cards: Vec<Card>,
    /// The next card id.
    next_card: u64,
    cfg: EngineConfig,
    ports: Ports,
    adapters: Arc<Adapters>,
}

impl Engine {
    /// An engine with every agent stopped (D29: the caller passes any file errors).
    pub fn new(
        org: Org,
        org_error: Option<OrgFileError>,
        settings: Settings,
        settings_error: Option<OrgFileError>,
        cfg: EngineConfig,
        ports: Ports,
        adapters: Arc<Adapters>,
    ) -> Engine {
        let now = ports.clock.now();
        let mut engine = Engine {
            org,
            org_error,
            settings,
            settings_error,
            agents: BTreeMap::new(),
            cli: None,
            dirs: None,
            queued_launches: Vec::new(),
            needs_you_shown: Vec::new(),
            reconnect: None,
            replaying: false,
            held: Held::default(),
            worktrees_shown: Vec::new(),
            worktrees_stale: false,
            worktree_check_due: now + timers::WORKTREE_CHECK_EVERY,
            worktree_generations: HashMap::new(),
            next_generation: 0,
            inbox: Vec::new(),
            next_message: 1,
            typing: None,
            closed_calls: Vec::new(),
            chat: Vec::new(),
            next_chat: 1,
            cards: Vec::new(),
            next_card: 1,
            cfg,
            ports,
            adapters,
        };
        engine.agents = engine
            .agent_names()
            .into_iter()
            .map(|name| (name, AgentRt::stopped(now)))
            .collect();
        engine
    }

    /// Handles one input and returns the events it caused, ending with
    /// `Worktrees` when the worktree rows changed and `NeedsYouChanged` when
    /// the Needs you list changed.
    pub fn handle(&mut self, input: Input) -> Vec<Event> {
        let mut out = self.dispatch(input);
        self.publish_worktrees(&mut out);
        self.publish_needs_you(&mut out);
        out
    }

    fn dispatch(&mut self, input: Input) -> Vec<Event> {
        match input {
            Input::Command(c) => self.on_command(c),
            Input::Runtime(RuntimeEvent::StartFailed { agent, reason }) => {
                self.on_start_failed(&agent, reason)
            }
            Input::Runtime(RuntimeEvent::Started { agent, window }) => {
                self.on_started(&agent, window)
            }
            Input::Runtime(RuntimeEvent::Screen { agent, text }) => self.on_screen(&agent, &text),
            Input::Runtime(RuntimeEvent::PaneDied {
                agent,
                window,
                status,
            }) => self.on_exit(&agent, &window, Exit::PaneDied(status)),
            Input::Runtime(RuntimeEvent::WindowGone { agent, window }) => {
                self.on_exit(&agent, &window, Exit::WindowGone)
            }
            Input::Runtime(RuntimeEvent::ConnectionLost { reason }) => {
                self.connection_lost(&reason)
            }
            Input::Hook(message) => self.hook_or_buffer(message),
            Input::CliInventory(list) => self.on_cli_inventory(list),
            Input::DirInventory(list) => self.on_dir_inventory(list),
            Input::Runtime(RuntimeEvent::Activity { agent, at_secs }) => {
                self.on_activity(&agent, at_secs)
            }
            Input::Git(GitEvent::Created {
                agent,
                task,
                result,
            }) => self.on_created(&agent, &task, result),
            Input::Git(GitEvent::Checked {
                agent,
                task,
                merge,
                dirty_files,
                base,
                generation,
            }) => self.on_checked(&agent, &task, merge, dirty_files, base, generation),
            Input::Git(GitEvent::Removed {
                agent,
                task,
                result,
            }) => self.on_removed(&agent, &task, result),
            Input::AlertClicked { agent } => vec![Event::AlertClicked { agent }],
            Input::Tick => self.on_tick(),
            Input::Runtime(RuntimeEvent::Inventory(windows)) => self.on_inventory(&windows),
            Input::Runtime(RuntimeEvent::PasteFailed { agent, reason }) => {
                self.on_paste_failed(&agent, reason)
            }
        }
    }

    fn on_command(&mut self, c: Command) -> Vec<Event> {
        match c {
            Command::StartAgent { agent } => self.start_agent(agent),
            Command::StopAgent { agent } => self.stop_agent(&agent),
            Command::GiveWork { agent, task, run } => self.give_work(agent, task, run),
            Command::ViewAgent { agent } => self.view_agent(&agent),
            Command::EditOrg(edit) => self.edit_org(&edit),
            Command::ReloadOrg => self.reload_org(),
            Command::CheckWorktrees => {
                let mut out = Vec::new();
                self.check_worktrees(None, &mut out);
                out
            }
            Command::DeleteWorktree {
                agent,
                task,
                confirmed,
            } => self.delete_worktree(&agent, &task, confirmed),
            Command::UpdateSettings(s) => self.update_settings(s),
            // Shutdown ends the driver loop before it gets here.
            Command::Shutdown => Vec::new(),
            Command::Tool {
                call_id,
                token,
                call,
            } => self.on_tool(call_id, &token, &call),
            Command::SendMessage { to, text } => self.send_yours(to, text),
            Command::Typing { agent } => self.typing(agent),
            Command::ChatToDuo { text } => self.chat_to_duo(text),
            Command::ApproveCard { card, edited } => self.approve_card(card, edited.map(|c| *c)),
            Command::RejectCard { card } => self.reject_card(card),
            Command::AssignToRole { .. } => reject("not built yet"),
        }
    }

    /// Everything the UI needs to draw a frame.
    pub fn snapshot(&self) -> Snapshot {
        let mut problems = self.problems();
        let now = self.ports.clock.now();
        let agents = self
            .agent_names()
            .into_iter()
            .map(|name| {
                let rt = self.agents.get(&name);
                let view = AgentView {
                    status: rt.map_or_else(|| AgentRt::stopped(now).status, |r| r.status.clone()),
                    last_reply: rt.and_then(|r| r.last_reply.clone()),
                    notice: rt.and_then(|r| r.notice.clone()),
                    problem: problems.remove(&name),
                    worktree: rt.and_then(|r| r.worktree.clone()),
                    delivery_pending: rt.is_some_and(|r| r.delivery.is_some()),
                    inbox: self.waiting(&name),
                };
                (name, view)
            })
            .collect();
        Snapshot {
            org: self.org.clone(),
            agents,
            needs_you: self.needs_you(),
            org_error: self.org_error.clone(),
            settings_error: self.settings_error.clone(),
            reconnecting: self.reconnect.as_ref().map_or(0, |r| r.reconnecting),
            worktrees: self.worktrees_shown.clone(),
            cards: self.cards.clone(),
            chat: self.chat.clone(),
            your_queued: self.your_queued(),
        }
    }

    /// Duo, then every agent in tree order.
    fn agent_names(&self) -> Vec<AgentName> {
        std::iter::once(self.org.duo.name.clone())
            .chain(self.org.agents().into_iter().map(|a| a.agent.name.clone()))
            .collect()
    }

    fn knows(&self, agent: &AgentName) -> bool {
        self.org.is_duo(agent) || self.org.find_agent(agent).is_some()
    }

    fn rt_mut(&mut self, agent: &AgentName) -> &mut AgentRt {
        let now = self.ports.clock.now();
        self.agents
            .entry(agent.clone())
            .or_insert_with(|| AgentRt::stopped(now))
    }

    fn state_of(&self, agent: &AgentName) -> AgentState {
        self.agents
            .get(agent)
            .map_or(AgentState::Stopped, |rt| rt.status.state)
    }

    /// Each agent's row problem, from the cached directory statuses (D26, D27).
    fn problems(&self) -> BTreeMap<AgentName, String> {
        self.agent_names()
            .into_iter()
            .filter_map(|name| {
                let r = resolve(&self.org, &name).ok()?;
                let dir = r.directory.as_deref().and_then(|d| {
                    let path = expand_home(d, &self.cfg.home);
                    self.dirs.as_ref()?.get(&path).copied()
                });
                agent_problem(&r, &self.cfg.home, dir).map(|p| (name, p))
            })
            .collect()
    }

    /// Moves `agent` by `trigger`, saves its session row (stopped deletes it) and reports the change.
    /// Returns whether it moved.
    fn change(
        &mut self,
        agent: &AgentName,
        trigger: &Trigger,
        error: Option<String>,
        out: &mut Vec<Event>,
    ) -> bool {
        self.change_with(agent, trigger, error, false, None, out)
    }

    /// [`Engine::change`], marking the new state as read from the screen (`~`) when
    /// `from_screen`, and keeping `waiting_on` when the new state is waiting (P2-D23).
    fn change_with(
        &mut self,
        agent: &AgentName,
        trigger: &Trigger,
        error: Option<String>,
        from_screen: bool,
        waiting_on: Option<WaitingOn>,
        out: &mut Vec<Event>,
    ) -> bool {
        let now = self.ports.clock.now();
        let rt = self.rt_mut(agent);
        let Some(to) = transition(rt.status.state, trigger) else {
            return false;
        };
        let before = rt.status.clone();
        if before.state.is_running() != to.is_running() {
            // Whether a session runs in a worktree may have changed (P2-D6).
            self.worktrees_stale = true;
        }
        let rt = self.rt_mut(agent);
        rt.status = Status {
            state: to,
            quiet: false,
            quiet_since: None,
            from_screen,
            since: now,
            error,
            waiting_on: waiting_on.filter(|_| to == AgentState::Waiting),
        };
        if to != AgentState::Waiting {
            rt.waiting_prompt = None;
        }
        if to == AgentState::Stopped {
            // A stopped agent has no session: its token must match no hook.
            rt.token = None;
        }
        let status = rt.status.clone();
        out.push(Event::AgentStatusChanged {
            agent: agent.clone(),
            status,
        });
        self.alert_after(agent, &before, out);
        // P2-D11: leaving rest ends a delivery's wait; a stopped agent runs in no worktree.
        let mut ended = None;
        self.update_run(agent, out, |rt| {
            if !to.is_at_rest() {
                ended = rt.delivery.take().and_then(|d| d.message);
            }
            if to == AgentState::Stopped {
                rt.worktree = None;
            }
        });
        if let Some(id) = ended {
            self.delivery_left_rest(agent, id, to, out);
        }
        if to == AgentState::Stopped {
            self.delete_row(agent, out);
        } else {
            self.save_row(agent, out);
        }
        // TD §5.3: an agent's worktrees are checked when it finishes.
        if to == AgentState::Finished {
            self.check_worktrees(Some(agent), out);
        }
        // TD §6.3: at rest, a waiting message may go in.
        if to.is_at_rest() {
            self.deliver_next(agent, out);
        }
        true
    }

    /// Applies `f` to `agent`'s runtime state, then publishes `AgentRunChanged`
    /// when its worktree or pending delivery changed.
    fn update_run(
        &mut self,
        agent: &AgentName,
        out: &mut Vec<Event>,
        f: impl FnOnce(&mut AgentRt),
    ) {
        let rt = self.rt_mut(agent);
        let before = (rt.worktree.clone(), rt.delivery.is_some());
        f(rt);
        let after = (rt.worktree.clone(), rt.delivery.is_some());
        if after.0 != before.0 {
            self.worktrees_stale = true;
        }
        if after != before {
            let (worktree, delivery_pending) = after;
            out.push(Event::AgentRunChanged {
                agent: agent.clone(),
                worktree,
                delivery_pending,
            });
        }
    }

    /// Deletes `agent`'s session row (stopped, Task 12); a failure is reported, not fatal.
    fn delete_row(&mut self, agent: &AgentName, out: &mut Vec<Event>) {
        if let Err(e) = self.ports.store.delete_session(agent) {
            out.push(Event::CommandRejected {
                reason: format!("couldn't save state: {e}"),
            });
        }
    }

    /// Records the current org as the last good one (D10); a failure is reported, not fatal.
    fn mark_last_good(&mut self, out: &mut Vec<Event>) {
        if let Err(e) = self.ports.store.mark_last_good(&self.org) {
            out.push(Event::CommandRejected {
                reason: format!("couldn't save org.last-good.toml: {e}"),
            });
        }
    }

    /// Saves `agent`'s session row; a failure is reported, not fatal.
    fn save_row(&mut self, agent: &AgentName, out: &mut Vec<Event>) {
        let rt = self.rt_mut(agent);
        let row = SessionRow {
            agent: agent.clone(),
            token: rt.token.clone(),
            window: rt.window.clone(),
            status: rt.status.clone(),
            last_reply: rt.last_reply.clone(),
            task_at_launch: rt.task_at_launch,
            saw_end_hook: rt.saw_end_hook,
            last_task: rt.last_task.clone(),
            worktree: rt.worktree.clone(),
            launched_as_duo: rt.launched_as_duo,
        };
        if let Err(e) = self.ports.store.save_session(&row) {
            out.push(Event::CommandRejected {
                reason: format!("couldn't save state: {e}"),
            });
        }
    }
}

/// A refusal with `reason`.
fn reject(reason: impl Into<String>) -> Vec<Event> {
    vec![Event::CommandRejected {
        reason: reason.into(),
    }]
}
