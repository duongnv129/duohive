//! Starting, stopping, viewing and giving work (TD §5.1, §5.5; D24, D26, D27, D30, P2-D11).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::timers::DELIVERY_WAIT;
use super::{Delivery, Engine, reject};
use crate::adapter::{CliAdapter, CliPresence, untested_notice};
use crate::cli_kind::CliKind;
use crate::command::Command;
use crate::event::Event;
use crate::inherit::{expand_home, resolve};
use crate::launch::{Key, LaunchSpec};
use crate::names::AgentName;
use crate::status::{AgentState, Trigger};
use crate::validate::{DirStatus, agent_problem};
use crate::worktree::{RunIn, TaskId};

impl Engine {
    /// `StartAgent`: a fresh launch with no task, from stopped or errored.
    pub(super) fn start_agent(&mut self, agent: AgentName) -> Vec<Event> {
        if !self.knows(&agent) {
            return reject(format!("no agent named {agent}"));
        }
        if self.creating_worktree(&agent) {
            return reject(getting_ready(&agent));
        }
        if self.state_of(&agent).is_running() {
            return reject(format!("{agent} is already running"));
        }
        self.launch_or_queue(agent, None, RunIn::Directory)
    }

    /// `GiveWork` by state (TD §5.1). `run` applies to a fresh launch only: a
    /// finished or idle agent takes the task where its session already runs.
    pub(super) fn give_work(&mut self, agent: AgentName, task: String, run: RunIn) -> Vec<Event> {
        if !self.knows(&agent) {
            return reject(format!("no agent named {agent}"));
        }
        if task.trim().is_empty() {
            return reject("the task is empty");
        }
        if self.creating_worktree(&agent) {
            return reject(getting_ready(&agent));
        }
        match self.state_of(&agent) {
            AgentState::Stopped | AgentState::Errored => {
                self.launch_or_queue(agent, Some(task), run)
            }
            // P2-D11: one delivery at a time.
            state @ (AgentState::Finished | AgentState::Idle)
                if self
                    .agents
                    .get(&agent)
                    .is_some_and(|rt| rt.delivery.is_some()) =>
            {
                vec![Event::GiveWorkRefused {
                    agent,
                    state,
                    pending: true,
                }]
            }
            AgentState::Finished | AgentState::Idle => self.type_next_turn(&agent, task),
            // Starting has no input box yet; working and waiting are busy.
            state @ (AgentState::Starting | AgentState::Working | AgentState::Waiting) => {
                vec![Event::GiveWorkRefused {
                    agent,
                    state,
                    pending: false,
                }]
            }
        }
    }

    /// `StopAgent`: quit a running agent, or clear an errored one. Refused while
    /// its worktree is being created, so the launch can't run over the stop (P2-D20).
    pub(super) fn stop_agent(&mut self, agent: &AgentName) -> Vec<Event> {
        if !self.knows(agent) {
            return reject(format!("no agent named {agent}"));
        }
        if self.creating_worktree(agent) {
            return reject(getting_ready(agent));
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
            // A key could answer the open prompt ("Yes" is often the default),
            // so the window is killed with no keys; the exit still ends stopped.
            AgentState::Waiting => {
                let rt = self.rt_mut(agent);
                rt.stopping = true;
                match rt.window.clone() {
                    Some(window) => self.ports.runtime.kill(agent, &window),
                    None => self.ports.runtime.stop(agent, Vec::new(), Duration::ZERO),
                }
            }
            AgentState::Starting
            | AgentState::Working
            | AgentState::Finished
            | AgentState::Idle => {
                self.rt_mut(agent).stopping = true;
                // Escape first declines a prompt that opened meanwhile. Stopping
                // must always work: with no adapter there are no quit keys, and
                // the runtime kills the window after the grace.
                let mut keys = vec![Key::Escape];
                keys.extend(
                    self.with_running_adapter(agent, |a| a.quit_keys())
                        .unwrap_or_else(|_| Vec::new()),
                );
                let grace = self.cfg.stop_grace;
                self.ports.runtime.stop(agent, keys, grace);
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

    /// Caches directory statuses and reports each agent's problem (D26). The
    /// first one, after the first frame, is start-up's worktree check (TD §5.3).
    pub(super) fn on_dir_inventory(&mut self, list: Vec<(PathBuf, DirStatus)>) -> Vec<Event> {
        let first = self.dirs.is_none();
        self.dirs = Some(list.into_iter().collect());
        let mut out = vec![Event::OrgChanged {
            org: Box::new(self.org.clone()),
            problems: self.problems(),
        }];
        if first {
            self.check_worktrees(None, &mut out);
        }
        out
    }

    /// Queues a launch until the first `CliInventory` (D30), else runs it: in
    /// the agent's directory, or in a new worktree when `run` asks for one and
    /// there is a task. The queued command keeps `run` and runs through
    /// `on_command` again, so its state is re-checked.
    pub(super) fn launch_or_queue(
        &mut self,
        agent: AgentName,
        task: Option<String>,
        run: RunIn,
    ) -> Vec<Event> {
        if self.cli.is_none() {
            self.queued_launches.push(match task {
                None => Command::StartAgent { agent },
                Some(task) => Command::GiveWork { agent, task, run },
            });
            return Vec::new();
        }
        match (task, run) {
            (Some(task), RunIn::NewWorktree) => self.begin_worktree(&agent, task),
            (task, _) => self.launch(&agent, task, None),
        }
    }

    /// A fresh launch (TD §5.1), in `worktree`'s path when given.
    pub(super) fn launch(
        &mut self,
        agent: &AgentName,
        task: Option<String>,
        worktree: Option<(TaskId, PathBuf)>,
    ) -> Vec<Event> {
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
        let (worktree, cwd) = worktree.unzip();
        let spec = LaunchSpec {
            agent: r,
            task: task.clone(),
            run_dir: self.cfg.data_dir.join("run").join(agent.as_str()),
            hook_exe: self.cfg.hook_exe.clone(),
            home,
            cwd,
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
        rt.last_hook_sent_at = 0;
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
        rt.settle_due = None;
        rt.last_activity_secs = None;
        // Saved on the session row by the change below, so a restart keeps it.
        self.update_run(agent, &mut out, |rt| {
            rt.worktree = worktree;
            rt.delivery = None;
        });
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
    pub(super) fn launch_fails(
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
        self.update_run(agent, &mut out, |rt| rt.worktree = None);
        self.change(agent, &Trigger::Launch, None, &mut out);
        self.change(agent, &Trigger::LaunchFailed, Some(reason), &mut out);
        out
    }

    /// Give work on finished or idle: the task is typed in as the next turn. The
    /// agent stays at rest until a sign it took the task (P2-D11): a busy or
    /// prompt-submit hook, or a `~` busy read, moves it to working.
    fn type_next_turn(&mut self, agent: &AgentName, task: String) -> Vec<Event> {
        let submit = match self.with_running_adapter(agent, |a| a.submit_keys()) {
            Ok(keys) => keys,
            Err(reason) => return reject(reason),
        };
        self.ports.runtime.paste(agent, task.clone(), submit);
        let now = self.ports.clock.now();
        let rt = self.rt_mut(agent);
        rt.last_task = Some(task.clone());
        rt.last_signal_at = now;
        let mut out = Vec::new();
        self.update_run(agent, &mut out, |rt| {
            rt.delivery = Some(Delivery {
                task,
                due: now + DELIVERY_WAIT,
            });
        });
        self.save_row(agent, &mut out);
        out
    }

    /// `PasteFailed`: a pending delivery is reported not taken, with the reason.
    pub(super) fn on_paste_failed(&mut self, agent: &AgentName, reason: String) -> Vec<Event> {
        if !self
            .agents
            .get(agent)
            .is_some_and(|rt| rt.delivery.is_some())
        {
            tracing::debug!(%agent, reason, "paste failed with no delivery pending");
            return Vec::new();
        }
        self.not_taken(agent, reason)
    }

    /// Ends `agent`'s pending delivery as not taken (P2-D11).
    pub(super) fn not_taken(&mut self, agent: &AgentName, reason: String) -> Vec<Event> {
        let mut out = Vec::new();
        let mut task = None;
        self.update_run(agent, &mut out, |rt| {
            task = rt.delivery.take().map(|d| d.task)
        });
        if let Some(task) = task {
            out.insert(
                0,
                Event::GiveWorkNotTaken {
                    agent: agent.clone(),
                    task,
                    reason,
                },
            );
        }
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

/// P2-D20's refusal while `agent`'s worktree is being created.
pub(super) fn getting_ready(agent: &AgentName) -> String {
    format!("{agent} is getting a worktree ready")
}

/// D24's row text for a CLI that is not installed.
pub(super) fn not_found(cli: CliKind) -> String {
    format!(
        "{} not found · install it or change its CLI (e)",
        cli.binary()
    )
}

#[cfg(test)]
mod delivery_tests;
#[cfg(test)]
mod stop_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod worktree_tests;
