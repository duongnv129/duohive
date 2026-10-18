//! D22: the `Store` the engine uses, enriching replayed hooks as live ones are.

use std::sync::Arc;

use duohive_core::adapter::Adapters;
use duohive_core::card::Card;
use duohive_core::chat::ChatEntry;
use duohive_core::hook::HookMessage;
use duohive_core::inbox::{Message, MessageId};
use duohive_core::names::AgentName;
use duohive_core::org::Org;
use duohive_core::ports::{OrgFileError, QueuedToolCall, SessionRow, Store, StoreError};
use duohive_core::settings::Settings;
use duohive_core::time::Millis;
use duohive_core::worktree::{TaskId, WorktreeRow};
use duohive_store::SqliteStore;

use crate::hook_listener::{AgentClis, enrich_message};

/// [`SqliteStore`], except that `take_pending_hooks` runs each hook through
/// its agent's adapter `enrich_hook` (CLI looked up in `AgentClis`), so hooks
/// queued while DuoHive was closed carry the last reply exactly as live ones do.
pub struct EnrichingStore {
    inner: SqliteStore,
    adapters: Arc<Adapters>,
    agents: AgentClis,
}

impl EnrichingStore {
    /// Wraps `inner`; `agents` is the map the system keeps current.
    pub fn new(inner: SqliteStore, adapters: Arc<Adapters>, agents: AgentClis) -> Self {
        Self {
            inner,
            adapters,
            agents,
        }
    }
}

impl Store for EnrichingStore {
    fn load_org(&mut self) -> Result<Option<Org>, OrgFileError> {
        self.inner.load_org()
    }

    fn last_good_org(&mut self) -> Option<Org> {
        self.inner.last_good_org()
    }

    fn save_org(&mut self, org: &Org) -> Result<(), StoreError> {
        self.inner.save_org(org)
    }

    fn mark_last_good(&mut self, org: &Org) -> Result<(), StoreError> {
        self.inner.mark_last_good(org)
    }

    fn save_settings(&mut self, s: &Settings) -> Result<(), StoreError> {
        self.inner.save_settings(s)
    }

    fn load_sessions(&mut self) -> Result<Vec<SessionRow>, StoreError> {
        self.inner.load_sessions()
    }

    fn save_session(&mut self, row: &SessionRow) -> Result<(), StoreError> {
        self.inner.save_session(row)
    }

    fn delete_session(&mut self, agent: &AgentName) -> Result<(), StoreError> {
        self.inner.delete_session(agent)
    }

    fn load_worktrees(&mut self) -> Result<Vec<WorktreeRow>, StoreError> {
        self.inner.load_worktrees()
    }

    fn save_worktree(&mut self, row: &WorktreeRow) -> Result<(), StoreError> {
        self.inner.save_worktree(row)
    }

    fn delete_worktree(&mut self, agent: &AgentName, task: &TaskId) -> Result<(), StoreError> {
        self.inner.delete_worktree(agent, task)
    }

    fn take_pending_hooks(&mut self) -> Result<Vec<HookMessage>, StoreError> {
        let mut hooks = self.inner.take_pending_hooks()?;
        for msg in &mut hooks {
            enrich_message(&self.agents, &self.adapters, msg);
        }
        Ok(hooks)
    }

    fn queue_pending_hook(&mut self, msg: &HookMessage, at: Millis) -> Result<(), StoreError> {
        self.inner.queue_pending_hook(msg, at)
    }

    fn take_queued_tool_calls(&mut self) -> Result<Vec<QueuedToolCall>, StoreError> {
        self.inner.take_queued_tool_calls()
    }

    fn load_messages(&mut self, since: Millis) -> Result<Vec<Message>, StoreError> {
        self.inner.load_messages(since)
    }

    fn find_message(&mut self, id: MessageId) -> Result<Option<Message>, StoreError> {
        self.inner.find_message(id)
    }

    fn save_message(&mut self, m: &Message) -> Result<(), StoreError> {
        self.inner.save_message(m)
    }

    fn load_chat(&mut self, last: usize) -> Result<Vec<ChatEntry>, StoreError> {
        self.inner.load_chat(last)
    }

    fn save_chat(&mut self, entry: &ChatEntry) -> Result<(), StoreError> {
        self.inner.save_chat(entry)
    }

    fn load_cards(&mut self) -> Result<Vec<Card>, StoreError> {
        self.inner.load_cards()
    }

    fn save_card(&mut self, card: &Card) -> Result<(), StoreError> {
        self.inner.save_card(card)
    }
}
