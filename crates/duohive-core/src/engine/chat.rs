//! The Duo chat transcript (TD §6.5; P3-D14, P3-D26, P3-Q5): your lines,
//! Duo's replies and the `→` action lines, stored as they come (append-only);
//! the snapshot carries the last [`CHAT_LOAD`].

use super::Engine;
use crate::chat::{CHAT_LOAD, ChatEntry, ChatId, ChatKind, cut};
use crate::event::Event;
use crate::inbox::{MessageKind, Sender};
use crate::names::AgentName;
use crate::tool::SendOutcome;

/// How much of a message an action line quotes (P3-D14).
const QUOTE: usize = 40;

impl Engine {
    /// `ChatToDuo` (P3-D26): a `you` line, then your message to Duo — typed
    /// in at rest, queued while busy, starting Duo when stopped or errored.
    /// Empty text is refused like any message of yours, and writes nothing.
    pub(super) fn chat_to_duo(&mut self, text: String) -> Vec<Event> {
        let duo = self.org.duo.name.clone();
        let mut sent = Vec::new();
        match self.send(
            Sender::You,
            &duo,
            MessageKind::Text,
            text.clone(),
            &mut sent,
        ) {
            Ok((id, _)) => {
                let mut out = Vec::new();
                let kind = ChatKind::You { message: Some(id) };
                self.chat_entry(kind, text, &mut out);
                out.extend(sent);
                out
            }
            Err(e) => vec![Event::MessageRefused {
                to: duo,
                text,
                reason: e.message,
            }],
        }
    }

    /// A `→` action line with its result word (P3-D14).
    pub(super) fn chat_action(&mut self, text: String, result: String, out: &mut Vec<Event>) {
        self.chat_entry(ChatKind::Action { result }, text, out);
    }

    /// Your message to `to` was accepted: its `@<to> "<text>"` line.
    pub(super) fn chat_sent(
        &mut self,
        to: &AgentName,
        text: &str,
        outcome: SendOutcome,
        out: &mut Vec<Event>,
    ) {
        let line = format!("@{to} \"{}\"", cut(text, QUOTE));
        self.chat_action(line, send_result(to, outcome), out);
    }

    /// Duo's reply at a turn end: always a line, even when it repeats the last one.
    pub(super) fn chat_reply(&mut self, reply: String, out: &mut Vec<Event>) {
        self.chat_entry(ChatKind::Duo, reply, out);
    }

    /// Stores an entry (a `Note` never is), then publishes it as `ChatLine`.
    pub(super) fn chat_entry(&mut self, kind: ChatKind, text: String, out: &mut Vec<Event>) {
        let entry = ChatEntry {
            id: ChatId(self.next_chat),
            at: self.ports.clock.now(),
            kind,
            text,
        };
        self.next_chat += 1;
        if entry.kind != ChatKind::Note
            && let Err(e) = self.ports.store.save_chat(&entry)
        {
            out.push(Event::CommandRejected {
                reason: format!("couldn't save state: {e}"),
            });
        }
        self.chat.push(entry.clone());
        if self.chat.len() > CHAT_LOAD {
            let extra = self.chat.len() - CHAT_LOAD;
            self.chat.drain(..extra);
        }
        out.push(Event::ChatLine(entry));
    }

    /// Start-up: the last [`CHAT_LOAD`] entries; ids continue from the highest.
    pub(super) fn load_chat(&mut self, out: &mut Vec<Event>) {
        match self.ports.store.load_chat(CHAT_LOAD) {
            Ok(entries) => {
                self.next_chat = entries.last().map_or(1, |e| e.id.0 + 1);
                self.chat = entries;
            }
            Err(e) => out.push(Event::CommandRejected {
                reason: format!("couldn't load the chat: {e}"),
            }),
        }
    }
}

/// A send's result word (P3-D14): `typed in`, `queued (<to> is <state>)` or `started`.
pub(super) fn send_result(to: &AgentName, outcome: SendOutcome) -> String {
    match outcome {
        SendOutcome::TypedIn => "typed in".to_owned(),
        SendOutcome::Queued(state) => format!("queued ({to} is {})", state.word()),
        SendOutcome::Started => "started".to_owned(),
    }
}

#[cfg(test)]
mod tests;
