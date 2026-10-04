//! Starting, stopping, viewing and giving work (TD §5.1, §5.5; D24, D26, D27, D30).

use std::path::PathBuf;
use std::sync::Arc;

use super::{Engine, reject};
use crate::adapter::{CliAdapter, CliPresence, untested_notice};
use crate::cli_kind::CliKind;
use crate::command::Command;
use crate::event::Event;
use crate::inherit::{expand_home, resolve};
use crate::launch::LaunchSpec;
use crate::names::AgentName;
use crate::status::{AgentState, Trigger};
use crate::validate::{DirStatus, agent_problem};

impl Engine {
    /// `StartAgent`: a fresh launch with no task, from stopped or errored.
    pub(super) fn start_agent(&mut self, agent: AgentName) -> Vec<Event> {
        if !self.knows(&agent) {
            return reject(format!("no agent named {agent}"));
        }
        if self.state_of(&agent).is_running() {
            return reject(format!("{agent} is already running"));
        }
        self.launch_or_queue(agent, None)
    }

    /// `GiveWork` by state (TD §5.1).
    pub(super) fn give_work(&mut self, agent: AgentName, task: String) -> Vec<Event> {
        if !self.knows(&agent) {
            return reject(format!("no agent named {agent}"));
        }
        if task.trim().is_empty() {
            return reject("the task is empty");
        }
        match self.state_of(&agent) {
            AgentState::Stopped | AgentState::Errored => self.launch_or_queue(agent, Some(task)),
            AgentState::Finished | AgentState::Idle => self.type_next_turn(&agent, task),
            // Starting has no input box yet; working and waiting are busy.
            state @ (AgentState::Starting | AgentState::Working | AgentState::Waiting) => {
                vec![Event::GiveWorkRefused { agent, state }]
            }
        }
    }

    /// `StopAgent`: quit a running agent, or clear an errored one.
    pub(super) fn stop_agent(&mut self, agent: &AgentName) -> Vec<Event> {
        if !self.knows(agent) {
            return reject(format!("no agent named {agent}"));
        }
        let mut out = Vec::new();
        match self.state_of(agent) {
            AgentState::Stopped => {}
            AgentState::Errored => {
                if let Some(window) = self.rt_mut(agent).window.take() {
                    self.ports.runtime.kill(agent, &window);
                }
                self.change(agent, &Trigger::Stop, None, &mut out);
            }
            AgentState::Starting
            | AgentState::Working
            | AgentState::Waiting
            | AgentState::Finished
            | AgentState::Idle => {
                self.rt_mut(agent).stopping = true;
                // Stopping must always work: with no adapter there are no quit
                // keys, and the runtime kills the window after the grace.
                let quit = self
                    .with_running_adapter(agent, |a| a.quit_keys())
                    .unwrap_or_else(|_| Vec::new());
                let grace = self.cfg.stop_grace;
                self.ports.runtime.stop(agent, quit, grace);
            }
        }
        out
    }

    /// `ViewAgent`: finished → idle; anything else is unchanged.
    pub(super) fn view_agent(&mut self, agent: &AgentName) -> Vec<Event> {
        if !self.knows(agent) {
            return reject(format!("no agent named {agent}"));
        }
        let mut out = Vec::new();
        if self.state_of(agent) == AgentState::Finished {
            self.change(agent, &Trigger::Viewed, None, &mut out);
        }
        out
    }

    /// `StartFailed`: the window could not open; `last_task` stays for a retry.
    pub(super) fn on_start_failed(&mut self, agent: &AgentName, reason: String) -> Vec<Event> {
        let mut out = Vec::new();
        if self.agents.contains_key(agent) {
            self.change(agent, &Trigger::LaunchFailed, Some(reason), &mut out);
        }
        out
    }

    /// Records which CLIs exist, then runs the queued launches in order (D30).
    pub(super) fn on_cli_inventory(&mut self, list: Vec<(CliKind, CliPresence)>) -> Vec<Event> {
        self.cli = Some(list.into_iter().collect());
        std::mem::take(&mut self.queued_launches)
            .into_iter()
            .flat_map(|c| self.on_command(c))
            .collect()
    }

    /// Caches directory statuses and reports each agent's problem (D26).
    pub(super) fn on_dir_inventory(&mut self, list: Vec<(PathBuf, DirStatus)>) -> Vec<Event> {
        self.dirs = Some(list.into_iter().collect());
        vec![Event::OrgChanged {
            org: Box::new(self.org.clone()),
            problems: self.problems(),
        }]
    }

    /// Queues a launch until the first `CliInventory` (D30), else runs it.
    /// The queued command runs through `on_command` again, so its state is re-checked.
    pub(super) fn launch_or_queue(&mut self, agent: AgentName, task: Option<String>) -> Vec<Event> {
        if self.cli.is_none() {
            self.queued_launches.push(match task {
                None => Command::StartAgent { agent },
                Some(task) => Command::GiveWork { agent, task },
            });
            return Vec::new();
        }
        self.launch(&agent, task)
    }

    /// A fresh launch (TD §5.1).
    fn launch(&mut self, agent: &AgentName, task: Option<String>) -> Vec<Event> {
        let r = match resolve(&self.org, agent) {
            Ok(r) => r,
            Err(e) => return reject(e.message),
        };
        let home = self.cfg.home.clone();
        let probe = r
            .directory
            .as_deref()
            .map(|d| self.ports.dirs.probe(&expand_home(d, &home)));
        if let Some(problem) = agent_problem(&r, &home, probe) {
            return reject(problem);
        }
        let cli = r.cli;
        let adapters = Arc::clone(&self.adapters);
        let presence = self.cli.as_ref().and_then(|m| m.get(&cli)).cloned();
        let (adapter, version) = match (presence, adapters.get(cli)) {
            (Some(CliPresence::Found { version }), Some(a)) => (a, version),
            _ => {
                return self.launch_fails(agent, task, not_found(cli));
            }
        };
        let mut out = Vec::new();
        if let Some(old) = self.rt_mut(agent).window.take() {
            self.ports.runtime.kill(agent, &old);
        }
        let spec = LaunchSpec {
            agent: r,
            task: task.clone(),
            run_dir: self.cfg.data_dir.join("run").join(agent.as_str()),
            hook_exe: self.cfg.hook_exe.clone(),
            home,
        };
        let mut cmd = match adapter.launch(&spec) {
            Ok(cmd) => cmd,
            Err(e) => return self.launch_fails(agent, task, e.to_string()),
        };
        let token = self.ports.ids.new_token();
        cmd.env.extend([
            ("DUOHIVE_TOKEN".to_owned(), token.0.clone()),
            ("DUOHIVE_AGENT".to_owned(), agent.to_string()),
            (
                "DUOHIVE_DATA_DIR".to_owned(),
                self.cfg.data_dir.to_string_lossy().into_owned(),
            ),
        ]);
        self.ports.runtime.start(agent, cmd);
        let now = self.ports.clock.now();
        let deadline = now + self.cfg.start_timeout;
        let rt = self.rt_mut(agent);
        rt.token = Some(token);
        rt.cli = Some(cli);
        rt.task_at_launch = task.is_some();
        rt.last_task = task;
        rt.start_deadline = Some(deadline);
        rt.last_signal_at = now;
        rt.stopping = false;
        rt.saw_end_hook = false;
        rt.waiting_prompt = None;
        rt.pre_read_done = false;
        rt.exit_pending = None;
        self.change(agent, &Trigger::Launch, None, &mut out);
        let notice = untested_notice(cli.binary(), &version, adapter.tested_versions());
        let rt = self.rt_mut(agent);
        if rt.notice != notice {
            rt.notice.clone_from(&notice);
            out.push(Event::AgentNoticeChanged {
                agent: agent.clone(),
                notice,
            });
        }
        out
    }

    /// A launch that fails before any window: starting, then errored (D24).
    fn launch_fails(
        &mut self,
        agent: &AgentName,
        task: Option<String>,
        reason: String,
    ) -> Vec<Event> {
        let mut out = Vec::new();
        let rt = self.rt_mut(agent);
        // No window runs this launch, so hooks from an older one must not match.
        rt.token = None;
        rt.task_at_launch = task.is_some();
        rt.last_task = task;
        self.change(agent, &Trigger::Launch, None, &mut out);
        self.change(agent, &Trigger::LaunchFailed, Some(reason), &mut out);
        out
    }

    /// Give work on finished or idle: the task becomes the next turn.
    fn type_next_turn(&mut self, agent: &AgentName, task: String) -> Vec<Event> {
        let submit = match self.with_running_adapter(agent, |a| a.submit_keys()) {
            Ok(keys) => keys,
            Err(reason) => return reject(reason),
        };
        self.ports.runtime.paste(agent, task.clone(), submit);
        let now = self.ports.clock.now();
        let rt = self.rt_mut(agent);
        rt.last_task = Some(task);
        rt.last_signal_at = now;
        let mut out = Vec::new();
        self.change(agent, &Trigger::NewWork, None, &mut out);
        out
    }

    /// Calls `f` with the adapter of the CLI `agent` runs on: the one it was
    /// launched on, else the one the org resolves to.
    pub(super) fn with_running_adapter<T>(
        &self,
        agent: &AgentName,
        f: impl FnOnce(&dyn CliAdapter) -> T,
    ) -> Result<T, String> {
        let cli: CliKind = self
            .agents
            .get(agent)
            .and_then(|rt| rt.cli)
            .or_else(|| resolve(&self.org, agent).ok().map(|r| r.cli))
            .ok_or_else(|| format!("no agent named {agent}"))?;
        self.adapters.get(cli).map(f).ok_or_else(|| not_found(cli))
    }
}

/// D24's row text for a CLI that is not installed.
fn not_found(cli: CliKind) -> String {
    format!(
        "{} not found · install it or change its CLI (e)",
        cli.binary()
    )
}

#[cfg(test)]
mod tests;
