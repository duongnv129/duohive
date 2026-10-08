//! Hook signals, screen reads and exits (TD §4.2, §5.5, §6.1; D2, D17, D21).

use super::{Engine, ExitPending};
use crate::adapter::{HookCoverage, HookSignal, ScreenState};
use crate::event::Event;
use crate::hook::HookMessage;
use crate::inherit::resolve;
use crate::launch::WindowId;
use crate::names::AgentName;
use crate::status::{AgentState, Trigger, transition};
use crate::worktree::RunIn;

/// How an agent's window ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Exit {
    /// The pane's process ended; `None` when killed by a signal.
    PaneDied(Option<i32>),
    /// The window vanished.
    WindowGone,
}

/// What the exit rules decide (TD §5.5).
enum ExitOutcome {
    Stopped,
    Errored(String),
    /// A zero exit waiting for its end hook (D21).
    Wait,
}

impl Engine {
    /// A hook (TD §6.1): authenticated by agent and token, then mapped to a trigger.
    pub(super) fn on_hook(&mut self, message: &HookMessage) -> Vec<Event> {
        let Some(agent) = self.authenticate(message) else {
            tracing::warn!(
                agent = ?message.agent,
                event = ?message.event,
                "hook dropped: unknown agent or wrong token"
            );
            return Vec::new();
        };
        let signal = self
            .with_running_adapter(&agent, |a| a.parse_hook(&message.event, &message.payload))
            .ok()
            .flatten();
        let now = self.ports.clock.now();
        let rt = self.rt_mut(&agent);
        rt.last_signal_at = now;
        let was_quiet = rt.status.quiet;
        rt.status.quiet = false;
        rt.status.quiet_since = None;
        // The listener runs connections concurrently, so a hook can arrive after
        // a newer one; `0` is an unknown send time and applies as it comes.
        let sent = message.sent_at_ms;
        let stale = sent != 0 && sent < rt.last_hook_sent_at;
        if sent != 0 && !stale {
            rt.last_hook_sent_at = sent;
        }
        let mut out = Vec::new();
        let moved = match signal {
            Some(HookSignal::SessionEnded) => self.on_end_hook(&agent, &mut out),
            Some(signal) if stale => {
                self.apply_stale_hook(&agent, signal, &mut out);
                false
            }
            Some(signal) => self.apply_hook(&agent, signal, &mut out),
            None => false,
        };
        if was_quiet && !moved {
            let status = self.rt_mut(&agent).status.clone();
            out.push(Event::AgentStatusChanged {
                agent: agent.clone(),
                status,
            });
            self.save_row(&agent, &mut out);
        }
        out
    }

    /// The agent a hook names, when it is known and the token is its current one.
    fn authenticate(&self, message: &HookMessage) -> Option<AgentName> {
        let agent = AgentName::parse(&message.agent).ok()?;
        if !self.knows(&agent) {
            return None;
        }
        let token = self.agents.get(&agent)?.token.as_ref()?;
        same_token(&token.0, &message.token).then_some(agent)
    }

    /// Ready, busy, needs-input and turn-end hooks. Returns whether the agent moved.
    fn apply_hook(&mut self, agent: &AgentName, signal: HookSignal, out: &mut Vec<Event>) -> bool {
        let state = self.state_of(agent);
        let is_duo = self.org.is_duo(agent);
        let task_at_launch = self.rt_mut(agent).task_at_launch;
        let (trigger, prompt, reply) = match signal {
            HookSignal::Ready => (
                Trigger::Ready {
                    has_task: task_at_launch,
                },
                None,
                None,
            ),
            HookSignal::Busy => match state {
                AgentState::Starting => (Trigger::Ready { has_task: true }, None, None),
                AgentState::Waiting => (Trigger::Answered, None, None),
                // D2: a new turn typed straight into the pane.
                _ => (Trigger::NewWork, None, None),
            },
            HookSignal::NeedsInput { prompt } => (Trigger::NeedsInput, prompt, None),
            HookSignal::TurnEnded { last_reply } => {
                (Trigger::TurnEnded { is_duo }, None, last_reply)
            }
            HookSignal::SessionEnded => return false,
        };
        if transition(state, &trigger).is_none() {
            return false;
        }
        // The reply goes out before the state change it came with, so a turn end lands
        // with its reply already current (P2-D8).
        out.extend(self.take_reply(agent, reply));
        self.change(agent, &trigger, None, out);
        if trigger == Trigger::NeedsInput {
            self.rt_mut(agent).waiting_prompt = prompt;
        }
        true
    }

    /// A hook older than one already applied: only its reply is kept.
    fn apply_stale_hook(&mut self, agent: &AgentName, signal: HookSignal, out: &mut Vec<Event>) {
        tracing::debug!(%agent, ?signal, "hook older than the last applied, state kept");
        if let HookSignal::TurnEnded {
            last_reply: Some(reply),
        } = signal
            && let Some(event) = self.take_reply(agent, Some(reply))
        {
            self.save_row(agent, out);
            out.push(event);
        }
    }

    /// The end hook: remembered, and it settles a zero exit waiting for it (D21).
    fn on_end_hook(&mut self, agent: &AgentName, out: &mut Vec<Event>) -> bool {
        let now = self.ports.clock.now();
        let rt = self.rt_mut(agent);
        rt.saw_end_hook = true;
        let Some(pending) = rt.exit_pending.take() else {
            self.save_row(agent, out);
            return false;
        };
        let outcome = if now < pending.due {
            ExitOutcome::Stopped
        } else {
            late_exit(rt.stopping, pending.status)
        };
        out.extend(self.finish_exit(agent, &pending.window, true, outcome));
        true
    }

    /// Sets the last reply; the event to emit when it changed.
    fn take_reply(&mut self, agent: &AgentName, reply: Option<String>) -> Option<Event> {
        let reply = reply?;
        let rt = self.rt_mut(agent);
        if rt.last_reply.as_deref() == Some(reply.as_str()) {
            return None;
        }
        rt.last_reply = Some(reply.clone());
        Some(Event::LastReplyChanged {
            agent: agent.clone(),
            reply: Some(reply),
        })
    }

    /// A settled screen read (TD §4.2, D2): moves only where no hook covers the move.
    pub(super) fn on_screen(&mut self, agent: &AgentName, text: &str) -> Vec<Event> {
        let state = self.state_of(agent);
        if !self.agents.contains_key(agent) || !state.is_running() {
            return Vec::new();
        }
        let Ok((signal, coverage)) =
            self.with_running_adapter(agent, |a| (a.read_screen(text), a.coverage()))
        else {
            return Vec::new();
        };
        let is_duo = self.org.is_duo(agent);
        let task_at_launch = self.rt_mut(agent).task_at_launch;
        let trigger = signal
            .state
            .and_then(|seen| screen_trigger(state, seen, &coverage, task_at_launch, is_duo));
        let reply_event = if coverage.last_reply {
            None
        } else {
            self.take_reply(agent, signal.last_reply)
        };
        let had_reply = reply_event.is_some();
        // The reply goes out before the state change it came with (P2-D8).
        let mut out: Vec<Event> = reply_event.into_iter().collect();
        let moved = trigger.is_some_and(|t| self.change_with(agent, &t, None, true, &mut out));
        if !moved && had_reply {
            self.save_row(agent, &mut out);
        }
        out
    }

    /// `Started`: records the agent's current window. A window reported after
    /// its launch already ended never leaks: an errored agent with no window
    /// keeps it (so a stop or relaunch kills it), any other is killed now.
    pub(super) fn on_started(&mut self, agent: &AgentName, window: WindowId) -> Vec<Event> {
        if !self.agents.contains_key(agent) {
            tracing::debug!(%agent, window = %window.0, "window started for an unknown agent");
            return Vec::new();
        }
        let state = self.state_of(agent);
        let rt = self.rt_mut(agent);
        if rt.killed_window.as_ref() == Some(&window) {
            tracing::debug!(%agent, window = %window.0, "window already killed, ignored");
            return Vec::new();
        }
        let keep = state.is_running() || (state == AgentState::Errored && rt.window.is_none());
        if !keep {
            tracing::debug!(%agent, window = %window.0, ?state, "late window killed");
            self.ports.runtime.kill(agent, &window);
            return Vec::new();
        }
        rt.window = Some(window);
        let mut out = Vec::new();
        self.save_row(agent, &mut out);
        out
    }

    /// `PaneDied` or `WindowGone`: the exit rules, for the agent's current window only.
    pub(super) fn on_exit(
        &mut self,
        agent: &AgentName,
        window: &WindowId,
        exit: Exit,
    ) -> Vec<Event> {
        let current = self.agents.get(agent).filter(|rt| {
            rt.window.as_ref() == Some(window)
                && rt.status.state.is_running()
                && rt.exit_pending.is_none()
        });
        if current.is_none() {
            if let Some(out) = self.held_exit(agent, window, exit) {
                return out;
            }
            tracing::debug!(%agent, window = %window.0, ?exit, "exit of a window not current, ignored");
            return Vec::new();
        }
        let (status, dead_pane) = match exit {
            Exit::PaneDied(status) => (status, true),
            Exit::WindowGone => (None, false),
        };
        match self.exit_outcome(agent, status) {
            ExitOutcome::Wait => {
                let due = self.ports.clock.now() + self.cfg.end_hook_grace;
                self.rt_mut(agent).exit_pending = Some(ExitPending {
                    window: window.clone(),
                    status,
                    due,
                });
                Vec::new()
            }
            outcome => self.finish_exit(agent, window, dead_pane, outcome),
        }
    }

    /// Rules 1–5 of TD §5.5 for an exit with `status`.
    fn exit_outcome(&mut self, agent: &AgentName, status: Option<i32>) -> ExitOutcome {
        let has_end_hook = self
            .with_running_adapter(agent, |a| a.coverage().session_end)
            .unwrap_or(false);
        let binary = self.binary_of(agent);
        let rt = self.rt_mut(agent);
        if rt.stopping {
            return ExitOutcome::Stopped;
        }
        if rt.status.state == AgentState::Starting {
            return ExitOutcome::Errored(match status {
                Some(127) => format!("{binary} not found (code 127)"),
                Some(code) => format!("{binary} exited at start (code {code})"),
                None => format!("{binary} exited at start (killed by a signal)"),
            });
        }
        match status {
            Some(0) if rt.saw_end_hook || !has_end_hook => ExitOutcome::Stopped,
            Some(0) => ExitOutcome::Wait,
            _ => ExitOutcome::Errored(unexpected(status)),
        }
    }

    /// Applies an exit's outcome: the window is forgotten (and killed when its
    /// pane is dead); stopped may relaunch when a restart was asked for.
    fn finish_exit(
        &mut self,
        agent: &AgentName,
        window: &WindowId,
        dead_pane: bool,
        outcome: ExitOutcome,
    ) -> Vec<Event> {
        let rt = self.rt_mut(agent);
        let restart = rt.stopping && rt.restart_after_stop;
        rt.window = None;
        rt.exit_pending = None;
        rt.stopping = false;
        rt.restart_after_stop = false;
        if dead_pane {
            self.ports.runtime.kill(agent, window);
        }
        let mut out = Vec::new();
        match outcome {
            ExitOutcome::Stopped => {
                self.change(agent, &Trigger::Stop, None, &mut out);
                if restart {
                    out.extend(self.launch_or_queue(agent.clone(), None, RunIn::Directory));
                }
            }
            ExitOutcome::Errored(reason) => {
                self.change(agent, &Trigger::UnexpectedExit, Some(reason), &mut out);
            }
            ExitOutcome::Wait => {}
        }
        out
    }

    /// Zero exits whose end hook never came: errored at their due time (D21),
    /// or stopped when a stop arrived during the grace.
    pub(super) fn expire_exits(&mut self) -> Vec<Event> {
        let now = self.ports.clock.now();
        let due: Vec<(AgentName, ExitPending)> = self
            .agents
            .iter()
            .filter_map(|(a, rt)| rt.exit_pending.clone().map(|p| (a.clone(), p)))
            .filter(|(_, p)| p.due <= now)
            .collect();
        due.into_iter()
            .flat_map(|(agent, p)| {
                let outcome = late_exit(self.rt_mut(&agent).stopping, p.status);
                self.finish_exit(&agent, &p.window, true, outcome)
            })
            .collect()
    }

    /// `ConnectionLost`: every running agent is errored; windows stay recorded
    /// so a relaunch kills them first.
    pub(super) fn on_connection_lost(&mut self, reason: &str) -> Vec<Event> {
        tracing::warn!(reason, "tmux connection lost");
        let running: Vec<AgentName> = self
            .agents
            .iter()
            .filter(|(_, rt)| rt.status.state.is_running())
            .map(|(a, _)| a.clone())
            .collect();
        let mut out = Vec::new();
        for agent in running {
            let rt = self.rt_mut(&agent);
            rt.stopping = false;
            rt.restart_after_stop = false;
            rt.exit_pending = None;
            let reason = Some("tmux server exited".to_owned());
            self.change(&agent, &Trigger::UnexpectedExit, reason, &mut out);
        }
        out
    }

    /// The executable of the CLI `agent` was launched on, else the one it resolves to.
    fn binary_of(&self, agent: &AgentName) -> &'static str {
        self.agents
            .get(agent)
            .and_then(|rt| rt.cli)
            .or_else(|| resolve(&self.org, agent).ok().map(|r| r.cli))
            .map_or("the CLI", |cli| cli.binary())
    }
}

/// The trigger a settled screen implies, or `None` where a hook covers it (D2).
fn screen_trigger(
    state: AgentState,
    seen: ScreenState,
    coverage: &HookCoverage,
    task_at_launch: bool,
    is_duo: bool,
) -> Option<Trigger> {
    use AgentState::{Finished, Idle, Starting, Waiting, Working};
    match (state, seen) {
        (Starting, ScreenState::Busy) if !coverage.busy => Some(Trigger::Ready { has_task: true }),
        (Finished | Idle, ScreenState::Busy) if !coverage.busy => Some(Trigger::NewWork),
        // D2: no hook reports the user's answer to a prompt.
        (Waiting, ScreenState::Busy) => Some(Trigger::Answered),
        // D2: a trust or login prompt shows before any hook can fire.
        (Starting, ScreenState::NeedsInput) => Some(Trigger::NeedsInput),
        (Working, ScreenState::NeedsInput) if !coverage.needs_input => Some(Trigger::NeedsInput),
        (Working, ScreenState::AtRest) if !coverage.turn_end => Some(Trigger::TurnEnded { is_duo }),
        // D2: a declined prompt fires no hook (spike S1).
        (Waiting, ScreenState::AtRest) => Some(Trigger::TurnEnded { is_duo }),
        (Starting, ScreenState::AtRest) if !coverage.ready => Some(Trigger::Ready {
            has_task: task_at_launch,
        }),
        _ => None,
    }
}

/// A zero exit whose end hook missed the grace: rule 5, unless a stop was asked for.
fn late_exit(stopping: bool, status: Option<i32>) -> ExitOutcome {
    if stopping {
        ExitOutcome::Stopped
    } else {
        ExitOutcome::Errored(unexpected(status))
    }
}

/// Rule 5's row text.
fn unexpected(status: Option<i32>) -> String {
    match status {
        Some(code) => format!("exited unexpectedly (code {code})"),
        None => "exited unexpectedly".to_owned(),
    }
}

/// Compares tokens in time independent of where they differ (TD §6.1).
fn same_token(expected: &str, given: &str) -> bool {
    let (a, b) = (expected.as_bytes(), given.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod order;
#[cfg(test)]
mod races;
#[cfg(test)]
mod tests;
