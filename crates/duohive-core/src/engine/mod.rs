//! The sans-I/O engine (TD §3.3): one owner of all state. It takes commands and
//! adapter inputs, publishes events, and reaches the world only through the ports.

#[cfg(test)]
mod harness;
mod lifecycle;
mod signals;

use signals::Exit;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::adapter::{Adapters, CliPresence};
use crate::cli_kind::CliKind;
use crate::command::Command;
use crate::event::{AgentView, Event, Snapshot};
use crate::hook::HookMessage;
use crate::inherit::{expand_home, resolve};
use crate::launch::{Token, WindowId};
use crate::names::AgentName;
use crate::org::Org;
use crate::ports::{Clock, Ids, OrgFileError, Runtime, RuntimeEvent, SessionRow, Store};
use crate::settings::Settings;
use crate::status::{AgentState, Status, Trigger, transition};
use crate::time::Millis;
use crate::validate::{DirProbe, DirStatus, agent_problem};

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

/// One agent's runtime state.
#[derive(Clone, Debug)]
struct AgentRt {
    status: Status,
    /// The current launch's token (D1).
    token: Option<Token>,
    /// The current window: set by `Started`, cleared on a relaunch until the next `Started`.
    window: Option<WindowId>,
    task_at_launch: bool,
    stopping: bool,
    saw_end_hook: bool,
    #[allow(dead_code, reason = "read by quiet (Task 13)")]
    last_signal_at: Millis,
    #[allow(dead_code, reason = "read by activity handling (Task 13)")]
    last_activity_secs: Option<u64>,
    #[allow(dead_code, reason = "read by settle reads (Task 13)")]
    settle_due: Option<Millis>,
    start_deadline: Option<Millis>,
    last_reply: Option<String>,
    notice: Option<String>,
    last_task: Option<String>,
    restart_after_stop: bool,
    /// The `NeedsInput { prompt }` text while waiting (Task 13's excerpt).
    #[allow(dead_code, reason = "read by Needs you (Task 13)")]
    waiting_prompt: Option<String>,
    #[allow(dead_code, reason = "read by the pre-timeout read (Task 13)")]
    pre_read_done: bool,
    exit_pending: Option<ExitPending>,
    /// The CLI the current session was launched on (org edits apply on the next start).
    cli: Option<CliKind>,
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
            },
            token: None,
            window: None,
            task_at_launch: false,
            stopping: false,
            saw_end_hook: false,
            last_signal_at: now,
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
            cli: None,
        }
    }
}

/// The engine. See the module docs.
pub struct Engine {
    org: Org,
    org_error: Option<OrgFileError>,
    #[allow(dead_code, reason = "read by quiet and settings (Tasks 13, 14)")]
    settings: Settings,
    settings_error: Option<OrgFileError>,
    agents: BTreeMap<AgentName, AgentRt>,
    /// `None` until the first `CliInventory` (D30).
    cli: Option<HashMap<CliKind, CliPresence>>,
    /// `None` until the first `DirInventory`: pending (D26).
    dirs: Option<HashMap<PathBuf, DirStatus>>,
    /// Launch commands that arrived before the first `CliInventory`, in order (D30).
    queued_launches: Vec<Command>,
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

    /// Handles one input and returns the events it caused.
    pub fn handle(&mut self, input: Input) -> Vec<Event> {
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
                self.on_connection_lost(&reason)
            }
            Input::Hook(message) => self.on_hook(&message),
            Input::CliInventory(list) => self.on_cli_inventory(list),
            Input::DirInventory(list) => self.on_dir_inventory(list),
            Input::Tick => self.on_tick(),
            // Activity, pastes and inventories: Tasks 13 and 15.
            Input::Runtime(
                RuntimeEvent::Activity { .. }
                | RuntimeEvent::PasteFailed { .. }
                | RuntimeEvent::Inventory(_),
            ) => Vec::new(),
        }
    }

    fn on_command(&mut self, c: Command) -> Vec<Event> {
        match c {
            Command::StartAgent { agent } => self.start_agent(agent),
            Command::StopAgent { agent } => self.stop_agent(&agent),
            Command::GiveWork { agent, task } => self.give_work(agent, task),
            Command::ViewAgent { agent } => self.view_agent(&agent),
            // Org edits and settings: Task 14; shutdown: the driver (Task 15).
            Command::EditOrg(_)
            | Command::ReloadOrg
            | Command::UpdateSettings(_)
            | Command::Shutdown => Vec::new(),
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
                };
                (name, view)
            })
            .collect();
        Snapshot {
            org: self.org.clone(),
            agents,
            // Needs you: Task 13; reconnecting: Task 15.
            needs_you: Vec::new(),
            org_error: self.org_error.clone(),
            settings_error: self.settings_error.clone(),
            reconnecting: 0,
        }
    }

    /// When the next timer is due: start timeouts and zero exits waiting for
    /// their end hook (Task 13 adds settle and quiet timers).
    pub fn next_deadline(&self) -> Option<Millis> {
        let starts = self
            .agents
            .values()
            .filter(|rt| rt.status.state == AgentState::Starting)
            .filter_map(|rt| rt.start_deadline);
        let exits = self
            .agents
            .values()
            .filter_map(|rt| rt.exit_pending.as_ref().map(|p| p.due));
        starts.chain(exits).min()
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
        self.change_with(agent, trigger, error, false, out)
    }

    /// [`Engine::change`], marking the new state as read from the screen (`~`) when `from_screen`.
    fn change_with(
        &mut self,
        agent: &AgentName,
        trigger: &Trigger,
        error: Option<String>,
        from_screen: bool,
        out: &mut Vec<Event>,
    ) -> bool {
        let now = self.ports.clock.now();
        let rt = self.rt_mut(agent);
        let Some(to) = transition(rt.status.state, trigger) else {
            return false;
        };
        rt.status = Status {
            state: to,
            quiet: false,
            quiet_since: None,
            from_screen,
            since: now,
            error,
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
        if to == AgentState::Stopped {
            self.delete_row(agent, out);
        } else {
            self.save_row(agent, out);
        }
        true
    }

    /// Deletes `agent`'s session row (stopped, Task 12); a failure is reported, not fatal.
    fn delete_row(&mut self, agent: &AgentName, out: &mut Vec<Event>) {
        if let Err(e) = self.ports.store.delete_session(agent) {
            out.push(Event::CommandRejected {
                reason: format!("couldn't save state: {e}"),
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
