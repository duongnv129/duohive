//! Test harness for the engine: fakes wired into an [`Engine`] over the example org.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{AgentRt, Engine, EngineConfig, Input, Ports};
use crate::adapter::{Adapters, CliPresence};
use crate::cli_kind::CliKind;
use crate::command::Command;
use crate::event::Event;
use crate::launch::{LaunchCommand, WindowId};
use crate::org::Org;
use crate::ports::Clock;
use crate::settings::Settings;
use crate::status::AgentState;
use crate::testkit::{
    FakeClock, FakeDirs, FakeIds, FakeRuntime, FakeStore, RuntimeCall, StubAdapter, example_org, n,
};
use crate::time::Millis;
use crate::validate::DirStatus;

/// The home directory every test engine uses.
pub(crate) const HOME: &str = "/home/me";
/// The data directory every test engine uses.
pub(crate) const DATA_DIR: &str = "/data";
/// When every test clock starts.
pub(crate) const T0: Millis = Millis(1_000_000);

/// The example org with a directory on every agent and an org prompt.
pub(crate) fn test_org() -> Org {
    let mut org = example_org();
    org.defaults.prompt = Some("Ask before you delete anything.".into());
    let backend = &mut org.teams[0].roles[0];
    backend.agents[0].fields.directory = Some("~/code/auth".into());
    backend.agents[1].fields.directory = Some("~/code/billing".into());
    org.teams[0].roles[1].agents[0].fields.directory = Some("~/code/pay-qa".into());
    org.roles[0].defaults.directory = Some("~/code/squad".into());
    org
}

/// Every CLI found at a tested version.
pub(crate) fn all_found() -> Vec<(CliKind, CliPresence)> {
    CliKind::ALL
        .into_iter()
        .map(|k| {
            (
                k,
                CliPresence::Found {
                    version: "2.1.0".into(),
                },
            )
        })
        .collect()
}

/// What to build a [`Harness`] from; tweak fields, then [`Setup::build`].
pub(crate) struct Setup {
    pub org: Org,
    pub dirs: HashMap<PathBuf, DirStatus>,
    pub claude: StubAdapter,
    pub opencode: StubAdapter,
    /// A CLI whose adapter is left out of `Adapters`.
    pub without_adapter: Option<CliKind>,
    /// Sent right after `Engine::new`; `None` sends nothing (D30).
    pub inventory: Option<Vec<(CliKind, CliPresence)>>,
    /// The engine's settings.
    pub settings: Settings,
    /// When the clock starts.
    pub start: Millis,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            org: test_org(),
            dirs: HashMap::new(),
            claude: StubAdapter::new(CliKind::Claude),
            opencode: StubAdapter::new(CliKind::Opencode),
            without_adapter: None,
            inventory: Some(all_found()),
            settings: Settings::default(),
            start: T0,
        }
    }
}

impl Setup {
    pub(crate) fn build(self) -> Harness {
        let runtime = FakeRuntime::default();
        let store = FakeStore::default();
        let clock = FakeClock::new(self.start);
        let cfg = EngineConfig {
            home: HOME.into(),
            data_dir: DATA_DIR.into(),
            hook_exe: "/bin/duohive".into(),
            start_timeout: Duration::from_secs(120),
            stop_grace: Duration::from_secs(5),
            settle_after: Duration::from_millis(1_500),
            end_hook_grace: Duration::from_millis(500),
        };
        let dirs = FakeDirs::new(self.dirs);
        let dir_probes = Arc::clone(&dirs.1);
        let ports = Ports {
            runtime: Box::new(runtime.clone()),
            store: Box::new(store.clone()),
            clock: Box::new(clock.clone()),
            ids: Box::new(FakeIds::default()),
            dirs: Arc::new(dirs),
        };
        let adapters = Arc::new(Adapters::new(
            [self.claude.clone(), self.opencode.clone()]
                .into_iter()
                .filter(|a| Some(a.kind) != self.without_adapter)
                .map(|a| Box::new(a) as Box<dyn crate::adapter::CliAdapter>)
                .collect(),
        ));
        let engine = Engine::new(self.org, None, self.settings, None, cfg, ports, adapters);
        let mut h = Harness {
            engine,
            call_log: runtime.calls,
            store,
            clock,
            claude: self.claude,
            opencode: self.opencode,
            dir_probes,
            t0: self.start,
        };
        if let Some(list) = self.inventory {
            h.input(Input::CliInventory(list));
        }
        h
    }
}

/// An engine with handles on every fake.
pub(crate) struct Harness {
    pub engine: Engine,
    pub call_log: Arc<Mutex<Vec<RuntimeCall>>>,
    pub store: FakeStore,
    pub clock: FakeClock,
    pub claude: StubAdapter,
    pub opencode: StubAdapter,
    /// How many directory probes the engine made.
    pub dir_probes: Arc<AtomicUsize>,
    /// When the clock started.
    pub t0: Millis,
}

impl Harness {
    /// The default setup.
    pub(crate) fn new() -> Self {
        Setup::default().build()
    }

    /// Moves the clock to `t0 + at`; panics when that is in the past.
    pub(crate) fn at(&mut self, at: Duration) {
        let target = self.t0 + at;
        let now = self.clock.now();
        assert!(target >= now, "the clock can't go back");
        self.clock.advance(target - now);
    }

    /// Moves the clock to `t0 + at`, then ticks.
    pub(crate) fn tick_at(&mut self, at: Duration) -> Vec<Event> {
        self.at(at);
        self.input(Input::Tick)
    }

    pub(crate) fn input(&mut self, input: Input) -> Vec<Event> {
        self.engine.handle(input)
    }

    pub(crate) fn send(&mut self, c: Command) -> Vec<Event> {
        self.input(Input::Command(c))
    }

    pub(crate) fn start(&mut self, agent: &str) -> Vec<Event> {
        self.send(Command::StartAgent { agent: n(agent) })
    }

    pub(crate) fn give(&mut self, agent: &str, task: &str) -> Vec<Event> {
        self.send(Command::GiveWork {
            agent: n(agent),
            task: task.into(),
        })
    }

    /// Runtime calls so far.
    pub(crate) fn calls(&self) -> Vec<RuntimeCall> {
        self.call_log.lock().unwrap().clone()
    }

    /// The single `Start` call's command; panics otherwise.
    pub(crate) fn only_start(&self) -> LaunchCommand {
        match self.calls().as_slice() {
            [RuntimeCall::Start(_, cmd)] => cmd.clone(),
            other => panic!("expected one Start call, got {other:?}"),
        }
    }

    pub(crate) fn rt(&self, agent: &str) -> &AgentRt {
        &self.engine.agents[&n(agent)]
    }

    pub(crate) fn rt_mut(&mut self, agent: &str) -> &mut AgentRt {
        self.engine.agents.get_mut(&n(agent)).unwrap()
    }

    pub(crate) fn state(&self, agent: &str) -> AgentState {
        self.rt(agent).status.state
    }

    /// Puts `agent` in `state` directly, standing in for hooks and exits.
    pub(crate) fn force(&mut self, agent: &str, state: AgentState) {
        let rt = self.rt_mut(agent);
        rt.status.state = state;
        rt.status.error = None;
    }

    /// Puts `agent` in errored with `error` and, when given, a recorded window.
    pub(crate) fn force_errored(&mut self, agent: &str, error: &str, window: Option<&str>) {
        self.force(agent, AgentState::Errored);
        let rt = self.rt_mut(agent);
        rt.status.error = Some(error.into());
        rt.window = window.map(|w| WindowId(w.into()));
    }
}
