//! Open tool calls, each waiting for its `Event::ToolResult`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use duohive_core::tool::{CallId, ToolResult};
use tokio::sync::oneshot;

/// The calls waiting for the core. Ids count up from 1 per run and are never
/// reused, so a result never reaches another call; `CallId::REPLAY` is never issued.
#[derive(Clone, Debug, Default)]
pub struct Calls {
    open: Arc<Mutex<HashMap<CallId, oneshot::Sender<ToolResult>>>>,
    last: Arc<AtomicU64>,
}

impl Calls {
    /// No calls yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a call; its result arrives on the receiver.
    pub fn open(&self) -> (CallId, oneshot::Receiver<ToolResult>) {
        let id = CallId(self.last.fetch_add(1, Ordering::Relaxed) + 1);
        let (tx, rx) = oneshot::channel();
        self.lock().insert(id, tx);
        (id, rx)
    }

    /// Hands `result` to call `id`; a forgotten or unknown id is dropped.
    pub fn resolve(&self, id: CallId, result: ToolResult) {
        if id == CallId::REPLAY {
            return;
        }
        if let Some(tx) = self.lock().remove(&id) {
            let _ = tx.send(result);
        }
    }

    /// Drops call `id`: its connection closed before the answer.
    pub fn forget(&self, id: CallId) {
        self.lock().remove(&id);
    }

    /// Call `id` still waits for its result.
    pub fn is_open(&self, id: CallId) -> bool {
        self.lock().contains_key(&id)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<CallId, oneshot::Sender<ToolResult>>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
