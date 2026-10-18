//! Duo's tools that list, read and wait, start and stop (TD §6.2; P3-D6,
//! P3-D16, P3-D23). A read is answered by the agent's next screen, a wait
//! by its turn end, stop or error, or by their timeouts; a call whose
//! caller's token stops being current (Duo stopped or relaunched) is dropped
//! without an answer.

use std::time::Duration;

use super::Engine;
use super::lifecycle::{GiveRefusal, getting_ready};
use crate::card::{CardChange, CardSource};
use crate::event::Event;
use crate::inherit::resolve;
use crate::launch::Token;
use crate::names::{AgentName, NodeName};
use crate::status::AgentState;
use crate::time::Millis;
use crate::tool::{
    AgentEntry, CallId, ErrorCode, ToolCall, ToolError, ToolOk, ToolResult, WaitOutcome,
};
use crate::worktree::RunIn;

/// How long `read_agent` waits for a screen (P3-D23).
pub const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// A `wait_for_agent` not yet answered (P3-D23).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Wait {
    /// The call to answer.
    pub call: CallId,
    /// The token that made it; the wait is dropped once it isn't current.
    pub caller_token: Token,
    /// The agent waited on.
    pub agent: AgentName,
    /// When it times out.
    pub due: Millis,
    /// Its timeout, for the action line.
    pub minutes: u32,
}

/// A `read_agent` waiting for the agent's next screen (P3-D23).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Read {
    /// The call to answer.
    pub call: CallId,
    /// The token that made it; the read is dropped once it isn't current.
    pub caller_token: Token,
    /// The agent read.
    pub agent: AgentName,
    /// When it answers `no_screen`.
    pub due: Millis,
}

impl Engine {
    /// `list_agents` (P3-D16): Duo first, then tree order.
    pub(super) fn list_agents(&self, you: &AgentName) -> ToolOk {
        let duo = self.org.duo.name.clone();
        let mut agents = vec![self.entry(&duo, None, None, false)];
        for loc in self.org.agents() {
            let name = &loc.agent.name;
            let team = loc.team.map(|t| t.name.clone());
            let free = self.is_free(name);
            agents.push(self.entry(name, team, Some(loc.role.name.clone()), free));
        }
        ToolOk::Agents {
            you: you.clone(),
            agents,
        }
    }

    fn entry(
        &self,
        name: &AgentName,
        team: Option<NodeName>,
        role: Option<NodeName>,
        free: bool,
    ) -> AgentEntry {
        let now = self.ports.clock.now();
        let rt = self.agents.get(name);
        let cli = rt
            .and_then(|rt| rt.cli)
            .unwrap_or_else(|| resolve(&self.org, name).map_or(self.org.defaults.cli, |r| r.cli));
        AgentEntry {
            name: name.clone(),
            team,
            role,
            state: self.state_of(name),
            quiet: rt.is_some_and(|rt| rt.status.quiet),
            from_screen: rt.is_some_and(|rt| rt.status.from_screen),
            for_secs: rt.map_or(0, |rt| (now - rt.status.since).as_secs()),
            last_reply: rt.and_then(|rt| rt.last_reply.clone()),
            waiting_messages: self.waiting(name),
            cli,
            worktree: rt.and_then(|rt| rt.worktree.clone()),
            free,
        }
    }

    /// `read_agent`: asks for one capture; `None` while it waits for the screen.
    pub(super) fn read_agent(
        &mut self,
        call: CallId,
        token: &Token,
        agent: &AgentName,
    ) -> Option<ToolResult> {
        if !self.knows(agent) {
            return Some(Err(unknown(agent)));
        }
        let state = self.state_of(agent);
        let has_window = self.agents.get(agent).is_some_and(|rt| rt.window.is_some());
        if state == AgentState::Stopped || (state == AgentState::Errored && !has_window) {
            return Some(Err(ToolError::new(
                ErrorCode::Refused,
                format!("{agent} is {}", state.word()),
            )));
        }
        self.ports.runtime.capture(agent);
        self.reads.push(Read {
            call,
            caller_token: token.clone(),
            agent: agent.clone(),
            due: self.ports.clock.now() + READ_TIMEOUT,
        });
        None
    }

    /// `wait_for_agent`: answered at once when the agent is at rest with
    /// nothing pending, stopped or errored; else `None` until it moves.
    pub(super) fn wait_for_agent(
        &mut self,
        call: CallId,
        token: &Token,
        agent: &AgentName,
        minutes: u32,
    ) -> Option<ToolResult> {
        if !self.knows(agent) {
            return Some(Err(unknown(agent)));
        }
        let pending = self.creating_worktree(agent)
            || self
                .agents
                .get(agent)
                .is_some_and(|rt| rt.delivery.is_some());
        let outcome = match self.state_of(agent) {
            _ if pending => None,
            AgentState::Stopped => Some(WaitOutcome::Stopped),
            AgentState::Errored => Some(WaitOutcome::Errored),
            state if state.is_at_rest() => Some(WaitOutcome::AtRest),
            _ => None,
        };
        if let Some(outcome) = outcome {
            return Some(Ok(self.waited(agent, outcome)));
        }
        let due = self.ports.clock.now() + Duration::from_secs(u64::from(minutes) * 60);
        self.waits.push(Wait {
            call,
            caller_token: token.clone(),
            agent: agent.clone(),
            due,
            minutes,
        });
        None
    }

    fn waited(&self, agent: &AgentName, outcome: WaitOutcome) -> ToolOk {
        ToolOk::Waited {
            agent: agent.clone(),
            outcome,
            state: self.state_of(agent),
            last_reply: self.agents.get(agent).and_then(|rt| rt.last_reply.clone()),
        }
    }

    /// `start_agent`: gives work by state (TD §5.1) in `run`, or the agent's
    /// resolved default.
    pub(super) fn start_by_tool(
        &mut self,
        agent: &AgentName,
        task: String,
        run: Option<RunIn>,
        out: &mut Vec<Event>,
    ) -> ToolResult {
        let run = run.unwrap_or_else(|| self.default_run(agent));
        let refusal = match self.give(agent, task, run, out) {
            Ok(given) => {
                return Ok(ToolOk::Given {
                    agent: agent.clone(),
                    given,
                });
            }
            Err(GiveRefusal::Unknown) => return Err(unknown(agent)),
            Err(GiveRefusal::EmptyTask) => {
                return Err(ToolError::new(
                    ErrorCode::InvalidArguments,
                    "the task is empty",
                ));
            }
            Err(GiveRefusal::Busy(state)) => {
                format!("{agent} is {}; send_message instead", state.word())
            }
            Err(GiveRefusal::Pending) => format!("{agent} is still taking the last task"),
            Err(GiveRefusal::GettingReady) => getting_ready(agent),
            Err(GiveRefusal::Launch(reason)) => reason,
        };
        Err(ToolError::new(ErrorCode::Refused, refusal))
    }

    /// `stop_agent` by state (P3-D6): finished, idle or errored stop directly;
    /// starting, working or waiting raise a card; stopped is refused.
    pub(super) fn stop_by_tool(&mut self, agent: &AgentName, out: &mut Vec<Event>) -> ToolResult {
        if !self.knows(agent) {
            return Err(unknown(agent));
        }
        if self.creating_worktree(agent) {
            return Err(ToolError::new(ErrorCode::Refused, getting_ready(agent)));
        }
        match self.state_of(agent) {
            AgentState::Stopped => Err(ToolError::new(
                ErrorCode::Refused,
                format!("{agent} is already stopped"),
            )),
            AgentState::Finished | AgentState::Idle | AgentState::Errored => {
                let events = self.stop_agent(agent);
                let rejected = events.iter().find_map(|e| match e {
                    Event::CommandRejected { reason } => Some(reason.clone()),
                    _ => None,
                });
                out.extend(events);
                match rejected {
                    Some(reason) => Err(ToolError::new(ErrorCode::Refused, reason)),
                    None => Ok(ToolOk::Stopped {
                        agent: agent.clone(),
                    }),
                }
            }
            AgentState::Starting | AgentState::Working | AgentState::Waiting => {
                let change = CardChange::Stop {
                    agent: agent.clone(),
                };
                let card = self.raise_card(CardSource::Duo, change, out);
                Ok(ToolOk::Pending { card })
            }
        }
    }

    /// `agent` moved from `before` to `to`: its waits end on a turn end (a
    /// move to rest), a stop or an error (P3-D23).
    pub(super) fn end_waits(
        &mut self,
        agent: &AgentName,
        before: AgentState,
        to: AgentState,
        out: &mut Vec<Event>,
    ) {
        let outcome = match to {
            AgentState::Stopped => WaitOutcome::Stopped,
            AgentState::Errored => WaitOutcome::Errored,
            to if to.is_at_rest() && !before.is_at_rest() => WaitOutcome::TurnEnded,
            _ => return,
        };
        let (due, keep) = std::mem::take(&mut self.waits)
            .into_iter()
            .partition(|w: &Wait| &w.agent == agent);
        self.waits = keep;
        for w in due {
            let result = Ok(self.waited(agent, outcome));
            self.answer_wait(&w, result, out);
        }
    }

    /// A screen of `agent`'s answers every read waiting for it.
    pub(super) fn answer_reads(&mut self, agent: &AgentName, text: &str, out: &mut Vec<Event>) {
        let (due, keep) = std::mem::take(&mut self.reads)
            .into_iter()
            .partition(|r: &Read| &r.agent == agent);
        self.reads = keep;
        for r in due {
            let result = Ok(ToolOk::Screen {
                agent: agent.clone(),
                state: self.state_of(agent),
                text: text.to_owned(),
            });
            self.answer_read(&r, result, out);
        }
    }

    /// The tick: waits past their timeout answer `timed_out`, reads past
    /// [`READ_TIMEOUT`] `no_screen`.
    pub(super) fn expire_waits(&mut self, now: Millis, out: &mut Vec<Event>) {
        let (due, keep) = std::mem::take(&mut self.waits)
            .into_iter()
            .partition(|w: &Wait| w.due <= now);
        self.waits = keep;
        for w in due {
            let result = Ok(self.waited(&w.agent, WaitOutcome::TimedOut));
            self.answer_wait(&w, result, out);
        }
        let (due, keep) = std::mem::take(&mut self.reads)
            .into_iter()
            .partition(|r: &Read| r.due <= now);
        self.reads = keep;
        for r in due {
            let secs = READ_TIMEOUT.as_secs();
            let result = Err(ToolError::new(
                ErrorCode::NoScreen,
                format!("no screen from {} within {secs}s", r.agent),
            ));
            self.answer_read(&r, result, out);
        }
        // A wait or read whose caller is gone is dropped now, not at its timeout.
        let waits = std::mem::take(&mut self.waits);
        self.waits = waits
            .into_iter()
            .filter(|w| self.caller(&w.caller_token).is_ok())
            .collect();
        let reads = std::mem::take(&mut self.reads);
        self.reads = reads
            .into_iter()
            .filter(|r| self.caller(&r.caller_token).is_ok())
            .collect();
    }

    /// The earliest wait or read timeout.
    pub(super) fn waits_due(&self) -> Option<Millis> {
        let waits = self.waits.iter().map(|w| w.due);
        waits.chain(self.reads.iter().map(|r| r.due)).min()
    }

    fn answer_wait(&mut self, w: &Wait, result: ToolResult, out: &mut Vec<Event>) {
        if self.caller(&w.caller_token).is_err() {
            tracing::debug!(agent = %w.agent, "wait dropped: its caller's launch ended");
            return;
        }
        let call = ToolCall::WaitForAgent {
            agent: w.agent.clone(),
            timeout_minutes: w.minutes,
        };
        self.publish_result(w.call, Some(&call), result, out);
    }

    fn answer_read(&mut self, r: &Read, result: ToolResult, out: &mut Vec<Event>) {
        if self.caller(&r.caller_token).is_err() {
            tracing::debug!(agent = %r.agent, "read dropped: its caller's launch ended");
            return;
        }
        let call = ToolCall::ReadAgent {
            agent: r.agent.clone(),
        };
        self.publish_result(r.call, Some(&call), result, out);
    }
}

fn unknown(agent: &AgentName) -> ToolError {
    ToolError::new(ErrorCode::UnknownAgent, format!("no agent named {agent}"))
}

#[cfg(test)]
mod tests;
