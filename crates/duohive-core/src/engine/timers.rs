//! Quiet, settle reads, the start timeout and Needs you (TD §4.2; D2, D6, D13, D19, D20).

use std::time::Duration;

use super::{AgentRt, Engine};
use crate::event::{Event, NeedsYouItem};
use crate::names::AgentName;
use crate::status::{AgentState, Trigger};
use crate::time::Millis;
use crate::worktree::RunIn;

/// D6: a starting agent's screen is read this long before its start timeout.
const PRE_READ_LEAD: Duration = Duration::from_secs(5);
/// P2-D11: how long a next-turn paste waits for a sign the agent took it.
pub(super) const DELIVERY_WAIT: Duration = Duration::from_secs(5);
/// TD §5.3: how often every worktree is checked.
pub(super) const WORKTREE_CHECK_EVERY: Duration = Duration::from_secs(600);
/// P2-D11's reason when no sign came in time.
const NO_SIGN: &str = "no sign it started";

impl Engine {
    /// `Activity`: a new activity time ends any quiet and schedules a settle read.
    /// While a delivery waits on an agent with no busy hook, the screen is also read
    /// at once: a turn's output rarely settles, and a `~` busy read confirms it (P2-D11).
    pub(super) fn on_activity(&mut self, agent: &AgentName, at_secs: u64) -> Vec<Event> {
        if !self.agents.contains_key(agent) || !self.state_of(agent).is_running() {
            return Vec::new();
        }
        let settle_due = self.ports.clock.now() + self.cfg.settle_after;
        let rt = self.rt_mut(agent);
        if rt.last_activity_secs == Some(at_secs) {
            return Vec::new();
        }
        rt.last_activity_secs = Some(at_secs);
        rt.last_signal_at = rt.last_signal_at.max(Millis(at_secs.saturating_mul(1000)));
        rt.settle_due = Some(settle_due);
        let confirm = rt.delivery.is_some()
            && !self
                .with_running_adapter(agent, |a| a.coverage().busy)
                .unwrap_or(true);
        if confirm {
            self.ports.runtime.capture(agent);
        }
        let rt = self.rt_mut(agent);
        let mut out = Vec::new();
        if rt.status.quiet {
            rt.status.quiet = false;
            rt.status.quiet_since = None;
            self.report_status(agent, &mut out);
        }
        out
    }

    /// A timer is due: zero exits past their grace, start timeouts, screen
    /// reads, quiet, deliveries waiting past `DELIVERY_WAIT` and the 10-minute
    /// worktree check.
    pub(super) fn on_tick(&mut self) -> Vec<Event> {
        let now = self.ports.clock.now();
        let mut out = self.expire_exits();
        if self.worktree_check_due <= now {
            self.worktree_check_due = now + WORKTREE_CHECK_EVERY;
            self.check_worktrees(None, &mut out);
        }
        let agents: Vec<AgentName> = self.agents.keys().cloned().collect();
        for agent in agents {
            self.tick_agent(&agent, now, &mut out);
        }
        out
    }

    fn tick_agent(&mut self, agent: &AgentName, now: Millis, out: &mut Vec<Event>) {
        let Some(rt) = self.agents.get(agent) else {
            return;
        };
        if rt.delivery.as_ref().is_some_and(|d| d.due <= now) {
            out.extend(self.not_taken(agent, NO_SIGN.to_owned()));
        }
        let Some(rt) = self.agents.get(agent) else {
            return;
        };
        if start_timeout_at(rt).is_some_and(|at| at <= now) {
            self.time_out_start(agent, out);
            return;
        }
        let pre_read = self.pre_read_at(agent, rt).is_some_and(|at| at <= now);
        let settle = rt.settle_due.is_some_and(|at| at <= now);
        let quiet_since = quiet_at(rt, self.settings.quiet_after())
            .filter(|at| *at <= now)
            .map(|_| rt.last_signal_at);
        // D2: a waiting agent is read whatever its coverage (no hook reports a declined prompt).
        let settle_read = settle
            && rt.status.state.is_running()
            && (rt.status.state == AgentState::Waiting || !self.complete_coverage(agent));
        let rt = self.rt_mut(agent);
        if settle {
            rt.settle_due = None;
        }
        if pre_read {
            rt.pre_read_done = true;
        }
        if pre_read || settle_read {
            self.ports.runtime.capture(agent);
        }
        if let Some(since) = quiet_since {
            let rt = self.rt_mut(agent);
            let before = rt.status.clone();
            rt.status.quiet = true;
            rt.status.quiet_since = Some(since);
            self.report_status(agent, out);
            self.alert_after(agent, &before, out);
        }
    }

    /// D20: starting past the start timeout errors, whatever the quiet threshold.
    /// A stop in progress ends stopped instead, relaunching when a restart was asked for.
    fn time_out_start(&mut self, agent: &AgentName, out: &mut Vec<Event>) {
        let rt = self.rt_mut(agent);
        let stopping = rt.stopping;
        let restart = stopping && rt.restart_after_stop;
        rt.stopping = false;
        rt.restart_after_stop = false;
        if let Some(window) = rt.window.take() {
            rt.killed_window = Some(window.clone());
            self.ports.runtime.kill(agent, &window);
        }
        if stopping {
            self.change(agent, &Trigger::Stop, None, out);
            if restart {
                out.extend(self.launch_or_queue(agent.clone(), None, RunIn::Directory));
            }
            return;
        }
        let reason = format!("not ready within {}s", self.cfg.start_timeout.as_secs());
        self.change(agent, &Trigger::LaunchFailed, Some(reason), out);
    }

    /// When a starting agent's pre-timeout screen read is due (D2): once per
    /// launch, only for a CLI that can show a start-up prompt.
    fn pre_read_at(&self, agent: &AgentName, rt: &AgentRt) -> Option<Millis> {
        if rt.pre_read_done {
            return None;
        }
        let deadline = start_timeout_at(rt)?;
        let prompts = self
            .with_running_adapter(agent, |a| a.startup_prompts())
            .unwrap_or(false);
        prompts.then(|| Millis(deadline.0.saturating_sub(lead_ms())))
    }

    fn complete_coverage(&self, agent: &AgentName) -> bool {
        self.with_running_adapter(agent, |a| a.coverage().complete())
            .unwrap_or(true)
    }

    /// Reports `agent`'s current status and saves its row.
    fn report_status(&mut self, agent: &AgentName, out: &mut Vec<Event>) {
        let status = self.rt_mut(agent).status.clone();
        out.push(Event::AgentStatusChanged {
            agent: agent.clone(),
            status,
        });
        self.save_row(agent, out);
    }

    /// Needs you: waiting, quiet and errored agents, oldest first, then by name (D13).
    pub fn needs_you(&self) -> Vec<NeedsYouItem> {
        let mut items: Vec<NeedsYouItem> = self
            .agents
            .iter()
            .filter_map(|(agent, rt)| {
                let s = &rt.status;
                let excerpt = match s.state {
                    AgentState::Waiting => rt.waiting_prompt.clone(),
                    AgentState::Errored => s.error.clone(),
                    _ if s.quiet => None,
                    _ => return None,
                };
                let since = if s.quiet {
                    s.quiet_since.unwrap_or(s.since)
                } else {
                    s.since
                };
                Some(NeedsYouItem {
                    agent: agent.clone(),
                    state: s.state,
                    quiet: s.quiet,
                    since,
                    excerpt,
                })
            })
            .collect();
        // Agents iterate in name order and the sort is stable: ties stay by name.
        items.sort_by_key(|i| i.since);
        items
    }

    /// Appends `NeedsYouChanged` when the list differs from the last one reported.
    pub(super) fn publish_needs_you(&mut self, out: &mut Vec<Event>) {
        let items = self.needs_you();
        if items != self.needs_you_shown {
            self.needs_you_shown.clone_from(&items);
            out.push(Event::NeedsYouChanged(items));
        }
    }

    /// When the next timer is due: settle reads, start timeouts, pre-timeout
    /// reads, zero exits waiting for their end hook, quiet thresholds,
    /// deliveries waiting for a sign they were taken, and, while there are
    /// worktrees, the 10-minute check.
    /// Each is cleared or acted on by the `Tick` that reaches it.
    pub fn next_deadline(&self) -> Option<Millis> {
        let quiet_after = self.settings.quiet_after();
        let worktree_check = (!self.worktrees_shown.is_empty()).then_some(self.worktree_check_due);
        self.agents
            .iter()
            .flat_map(|(agent, rt)| {
                [
                    rt.settle_due,
                    start_timeout_at(rt),
                    self.pre_read_at(agent, rt),
                    rt.exit_pending.as_ref().map(|p| p.due),
                    quiet_at(rt, quiet_after),
                    rt.delivery.as_ref().map(|d| d.due),
                ]
            })
            .flatten()
            .chain(worktree_check)
            .min()
    }
}

/// When a starting agent times out.
fn start_timeout_at(rt: &AgentRt) -> Option<Millis> {
    (rt.status.state == AgentState::Starting)
        .then_some(rt.start_deadline)
        .flatten()
}

/// When a working or starting agent that is not yet quiet goes quiet.
fn quiet_at(rt: &AgentRt, quiet_after: Duration) -> Option<Millis> {
    let can_go_quiet = matches!(rt.status.state, AgentState::Working | AgentState::Starting);
    (can_go_quiet && !rt.status.quiet).then(|| rt.last_signal_at + quiet_after)
}

fn lead_ms() -> u64 {
    u64::try_from(PRE_READ_LEAD.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod edges;
#[cfg(test)]
mod tests;
