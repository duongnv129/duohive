//! The inbox (TD §6.3; P3-D16, P3-D18, P3-D19). A message waits until its
//! recipient is at rest with nothing pending and nobody typing into it, goes
//! in through the next-turn paste, and is delivered on P2-D11's sign that the
//! agent took it. A paste that fails or isn't taken drops the message for good.

use super::Engine;
use super::lifecycle::GiveRefusal;
use crate::event::Event;
use crate::inbox::{
    DUPLICATE_WINDOW, INBOX_LIMIT, Message, MessageId, MessageKind, MessageStatus, RATE_LIMIT,
    Sender, check_limits, frame,
};
use crate::inherit::resolve;
use crate::names::AgentName;
use crate::status::AgentState;
use crate::time::Millis;
use crate::tool::{ErrorCode, LimitKind, SendOutcome, ToolError, ToolOk, ToolResult};
use crate::worktree::RunIn;

/// Your `SendMessage` with no text.
const EMPTY: &str = "the message is empty";

impl Engine {
    /// Accepts a message (TD §6.3): checks an agent's limits, saves it queued,
    /// then starts a stopped recipient with your text, pastes it into one at
    /// rest, or leaves it waiting.
    pub(super) fn send(
        &mut self,
        from: Sender,
        to: &AgentName,
        kind: MessageKind,
        text: String,
        out: &mut Vec<Event>,
    ) -> Result<(MessageId, SendOutcome), ToolError> {
        if !self.knows(to) {
            return Err(ToolError::new(
                ErrorCode::UnknownAgent,
                format!("no agent named {to}"),
            ));
        }
        if text.trim().is_empty() {
            return Err(ToolError::new(ErrorCode::InvalidArguments, EMPTY));
        }
        let now = self.ports.clock.now();
        if let Sender::Agent(agent) = &from {
            check_limits(&self.inbox, agent, to, &text, now)
                .map_err(|kind| ToolError::new(ErrorCode::Limit(kind), limit_text(kind, to)))?;
        }
        self.prune_inbox(now);
        let id = MessageId(self.next_message);
        self.next_message += 1;
        let your_text = from == Sender::You && kind == MessageKind::Text;
        let replier = match &from {
            Sender::Agent(agent) => Some(agent.clone()),
            Sender::You => None,
        };
        out.push(Event::MessageQueued {
            id,
            from: from.clone(),
            to: to.clone(),
        });
        self.inbox.push(Message {
            id,
            from,
            to: to.clone(),
            kind,
            text: text.clone(),
            status: MessageStatus::Queued,
            queued_at: now,
            delivered_at: None,
        });
        self.save_message(id, out);
        self.publish_inbox(to, out);
        if let Some(agent) = replier {
            self.mark_answered(&agent, to, out);
        }
        let state = self.state_of(to);
        if !state.is_running() {
            // P3-D18: your text starts a stopped or errored agent; anything else waits.
            if your_text {
                return Ok((id, self.start_with(id, to, text, out)));
            }
            return Ok((id, SendOutcome::Queued(state)));
        }
        self.deliver_next(to, out);
        let typed = self
            .agents
            .get(to)
            .and_then(|rt| rt.delivery.as_ref())
            .is_some_and(|d| d.message == Some(id));
        let outcome = if typed {
            SendOutcome::TypedIn
        } else {
            SendOutcome::Queued(self.state_of(to))
        };
        Ok((id, outcome))
    }

    /// Your `@agent text`: refused for an unknown agent or empty text.
    pub(super) fn send_yours(&mut self, to: AgentName, text: String) -> Vec<Event> {
        let reason = if !self.knows(&to) {
            Some(format!("no agent named {to}"))
        } else if text.trim().is_empty() {
            Some(EMPTY.to_owned())
        } else {
            None
        };
        if let Some(reason) = reason {
            return vec![Event::MessageRefused { to, text, reason }];
        }
        let mut out = Vec::new();
        if let Err(e) = self.send(Sender::You, &to, MessageKind::Text, text.clone(), &mut out) {
            out.push(Event::MessageRefused {
                to,
                text,
                reason: e.message,
            });
        }
        out
    }

    /// Your message to a stopped or errored agent: launched with it as the
    /// task, in the agent's default place. The launch takes it (`launch`
    /// delivers it, a failed one drops it); while a worktree is being got
    /// ready it waits for the agent's first rest.
    fn start_with(
        &mut self,
        id: MessageId,
        to: &AgentName,
        text: String,
        out: &mut Vec<Event>,
    ) -> SendOutcome {
        let run = self.default_run(to);
        self.rt_mut(to).launch_message = Some(id);
        match self.give(to, text, run, out) {
            Ok(_) => return SendOutcome::Started,
            // D30: the launch waits for the CLI inventory, and takes it then.
            Err(GiveRefusal::Launch(_)) if self.cli.is_none() => return SendOutcome::Started,
            Err(GiveRefusal::Launch(reason)) => {
                if let Some(id) = self.rt_mut(to).launch_message.take() {
                    self.drop_message(id, reason, out);
                }
            }
            Err(_) => self.rt_mut(to).launch_message = None,
        }
        SendOutcome::Queued(self.state_of(to))
    }

    /// The place a launch runs in when none is given: the agent's resolved default.
    pub(super) fn default_run(&self, agent: &AgentName) -> RunIn {
        if resolve(&self.org, agent).is_ok_and(|r| r.worktree) {
            RunIn::NewWorktree
        } else {
            RunIn::Directory
        }
    }

    /// Pastes `agent`'s oldest queued message when it may take one: at rest,
    /// no delivery pending, not stopping, not being typed into (P3-D19), no
    /// worktree being got ready, and not during the start-up replay.
    pub(super) fn deliver_next(&mut self, agent: &AgentName, out: &mut Vec<Event>) {
        if self.replaying || self.typing.as_ref() == Some(agent) || self.creating_worktree(agent) {
            return;
        }
        let Some(rt) = self.agents.get(agent) else {
            return;
        };
        if !rt.status.state.is_at_rest() || rt.delivery.is_some() || rt.stopping {
            return;
        }
        let Some((id, from, body)) = self
            .inbox
            .iter()
            .find(|m| &m.to == agent && m.status == MessageStatus::Queued)
            .map(|m| (m.id, m.from.clone(), m.text.clone()))
        else {
            return;
        };
        let text = match &from {
            Sender::Agent(sender) => frame(sender, &self.fresh_boundary(&body), &body),
            Sender::You => body,
        };
        match self.type_next_turn(agent, text, Some(id)) {
            Ok(events) => out.extend(events),
            Err(reason) => self.drop_message(id, reason, out),
        }
    }

    /// Tries every agent, in tree order.
    pub(super) fn deliver_all(&mut self, out: &mut Vec<Event>) {
        for agent in self.agent_names() {
            self.deliver_next(&agent, out);
        }
    }

    /// `Typing`: nothing is delivered to the agent you type into (P3-D19);
    /// when you leave it, its oldest message may go in.
    pub(super) fn typing(&mut self, agent: Option<AgentName>) -> Vec<Event> {
        let before = std::mem::replace(&mut self.typing, agent);
        let mut out = Vec::new();
        if let Some(left) = before.filter(|b| self.typing.as_ref() != Some(b)) {
            self.deliver_next(&left, &mut out);
        }
        out
    }

    /// A boundary minted until it isn't in `text` (P3-D18).
    fn fresh_boundary(&mut self, text: &str) -> String {
        loop {
            let boundary = self.ports.ids.boundary();
            if !text.contains(&boundary) {
                return boundary;
            }
        }
    }

    /// The paste of message `id` ended with `agent` in `to`: taken when it
    /// moved to working or waiting (P2-D11's sign), else dropped.
    pub(super) fn delivery_left_rest(
        &mut self,
        agent: &AgentName,
        id: MessageId,
        to: AgentState,
        out: &mut Vec<Event>,
    ) {
        match to {
            AgentState::Working | AgentState::Waiting => self.mark_delivered(id, out),
            _ => {
                let reason = self
                    .agents
                    .get(agent)
                    .and_then(|rt| rt.status.error.clone())
                    .unwrap_or_else(|| format!("{agent} {} before taking it", to.word()));
                self.drop_message(id, reason, out);
            }
        }
    }

    /// Message `id` was taken.
    pub(super) fn mark_delivered(&mut self, id: MessageId, out: &mut Vec<Event>) {
        let now = self.ports.clock.now();
        let Some(m) = self.inbox.iter_mut().find(|m| m.id == id) else {
            return;
        };
        m.status = MessageStatus::Delivered;
        m.delivered_at = Some(now);
        let to = m.to.clone();
        self.save_message(id, out);
        out.push(Event::MessageDelivered { id });
        self.publish_inbox(&to, out);
    }

    /// Message `id` will never be delivered.
    pub(super) fn drop_message(&mut self, id: MessageId, reason: String, out: &mut Vec<Event>) {
        let Some(m) = self.inbox.iter_mut().find(|m| m.id == id) else {
            return;
        };
        m.status = MessageStatus::Dropped {
            reason: reason.clone(),
        };
        let (from, to, text) = (m.from.clone(), m.to.clone(), m.text.clone());
        self.save_message(id, out);
        out.push(Event::MessageDropped {
            id,
            from,
            to: to.clone(),
            text,
            reason,
        });
        self.publish_inbox(&to, out);
    }

    /// `replier` wrote back to `asker`: `asker`'s delivered messages to
    /// `replier` are answered (P3-D18, agent to agent only).
    fn mark_answered(&mut self, replier: &AgentName, asker: &AgentName, out: &mut Vec<Event>) {
        let sender = Sender::Agent(asker.clone());
        let answered: Vec<MessageId> = self
            .inbox
            .iter_mut()
            .filter(|m| {
                m.from == sender && &m.to == replier && m.status == MessageStatus::Delivered
            })
            .map(|m| {
                m.status = MessageStatus::Answered;
                m.id
            })
            .collect();
        for id in answered {
            self.save_message(id, out);
        }
    }

    /// How many messages wait for `agent` (`✉ N`).
    pub(super) fn waiting(&self, agent: &AgentName) -> u32 {
        let n = self
            .inbox
            .iter()
            .filter(|m| &m.to == agent && m.status == MessageStatus::Queued)
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// Your messages not yet delivered, oldest first.
    pub(super) fn your_queued(&self) -> Vec<MessageId> {
        self.inbox
            .iter()
            .filter(|m| m.from == Sender::You && m.status == MessageStatus::Queued)
            .map(|m| m.id)
            .collect()
    }

    fn publish_inbox(&self, agent: &AgentName, out: &mut Vec<Event>) {
        out.push(Event::InboxChanged {
            agent: agent.clone(),
            waiting: self.waiting(agent),
        });
    }

    /// `message_status`: the caller's own messages only, with P3-D16's reason.
    pub(super) fn message_status(&mut self, caller: &AgentName, id: MessageId) -> ToolResult {
        let found = match self.inbox.iter().find(|m| m.id == id) {
            Some(m) => Some(m.clone()),
            None => self.ports.store.find_message(id).ok().flatten(),
        };
        let Some(m) = found.filter(|m| m.from == Sender::Agent(caller.clone())) else {
            return Err(ToolError::new(
                ErrorCode::UnknownMessage,
                format!("you sent no message {}", id.0),
            ));
        };
        let reason = match &m.status {
            MessageStatus::Queued => Some(format!("{} is {}", m.to, self.state_of(&m.to).word())),
            MessageStatus::Dropped { reason } => Some(reason.clone()),
            MessageStatus::Delivered | MessageStatus::Answered => None,
        };
        Ok(ToolOk::Status {
            message: m.id,
            to: m.to,
            status: m.status,
            reason,
        })
    }

    /// Start-up: the queued and delivered messages, and the last 10 minutes'
    /// for the limits; ids continue from the highest.
    pub(super) fn load_inbox(&mut self, out: &mut Vec<Event>) {
        let now = self.ports.clock.now();
        let since = Millis(now.0.saturating_sub(window_ms()));
        match self.ports.store.load_messages(since) {
            Ok(mut messages) => {
                messages.sort_by_key(|m| m.id);
                self.next_message = messages.last().map_or(1, |m| m.id.0 + 1);
                self.inbox = messages;
            }
            Err(e) => out.push(Event::CommandRejected {
                reason: format!("couldn't load messages: {e}"),
            }),
        }
    }

    /// Forgets answered and dropped messages older than the duplicate window;
    /// `message_status` still finds them in the store.
    fn prune_inbox(&mut self, now: Millis) {
        self.inbox.retain(|m| {
            matches!(m.status, MessageStatus::Queued | MessageStatus::Delivered)
                || now - m.queued_at < DUPLICATE_WINDOW
        });
    }

    /// Saves message `id`; a failure is reported, not fatal.
    fn save_message(&mut self, id: MessageId, out: &mut Vec<Event>) {
        let Some(m) = self.inbox.iter().find(|m| m.id == id) else {
            return;
        };
        if let Err(e) = self.ports.store.save_message(m) {
            out.push(Event::CommandRejected {
                reason: format!("couldn't save state: {e}"),
            });
        }
    }
}

fn window_ms() -> u64 {
    u64::try_from(DUPLICATE_WINDOW.as_millis()).unwrap_or(u64::MAX)
}

/// What a limit refusal tells the model.
fn limit_text(kind: LimitKind, to: &AgentName) -> String {
    match kind {
        LimitKind::Duplicate => format!("you sent {to} this message in the last 10 minutes"),
        LimitKind::Rate => format!("more than {RATE_LIMIT} messages to {to} in a minute"),
        LimitKind::InboxFull => format!("{to} already has {INBOX_LIMIT} messages waiting"),
    }
}

#[cfg(test)]
mod tests;
