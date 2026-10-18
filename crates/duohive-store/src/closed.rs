//! What `duohive mcp` reads and writes while DuoHive is closed (P3-D10, P3-D30).
//!
//! Plain functions over `state.db`; this crate knows nothing of `duohive-mcp`.

use std::time::Duration;

use duohive_core::launch::Token;
use duohive_core::ports::{QueuedToolCall, StoreError};
use duohive_core::time::Millis;
use duohive_core::tool::Rights;
use rusqlite::{Connection, TransactionBehavior};

use crate::Paths;
use crate::db::{db_err, open_db_for_hook, to_i64};

/// The helper's budget for one database operation, as `duohive hook`'s.
const BUSY: Duration = Duration::from_millis(200);

/// Compares two tokens in time independent of where they first differ.
fn same_token(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = a.len() ^ b.len();
    for (i, x) in a.iter().enumerate() {
        diff |= usize::from(x ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

/// The rights of the session whose token is `token`: Duo's when it was launched
/// as Duo, an agent's otherwise; `None` when no session has it.
pub fn rights_for_token(paths: &Paths, token: &str) -> Result<Option<Rights>, StoreError> {
    if token.is_empty() {
        return Ok(None);
    }
    let conn = open_db_for_hook(&paths.state_db(), BUSY)?;
    let mut stmt = conn
        .prepare("SELECT token, launched_as_duo FROM sessions WHERE token IS NOT NULL")
        .map_err(|e| db_err("cannot read sessions", &e))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))
        .map_err(|e| db_err("cannot read sessions", &e))?;
    let mut found = None;
    // Every row is compared, so the time taken says nothing about which matched.
    for row in rows {
        let (t, duo) = row.map_err(|e| db_err("cannot read session", &e))?;
        if same_token(&t, token) {
            found = Some(if duo { Rights::Duo } else { Rights::Agent });
        }
    }
    Ok(found)
}

/// Queues a changing tool call for DuoHive to run when it next opens; returns its id.
pub fn queue_tool_call(
    paths: &Paths,
    token: &str,
    tool: &str,
    arguments: &serde_json::Value,
    at: Millis,
) -> Result<u64, StoreError> {
    let conn = open_db_for_hook(&paths.state_db(), BUSY)?;
    let args = serde_json::to_string(arguments).map_err(|e| db_err("bad arguments", &e))?;
    conn.execute(
        "INSERT INTO pending_tool_calls (token, tool, arguments, queued_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![token, tool, args, to_i64(at.0)],
    )
    .map_err(|e| db_err("cannot queue tool call", &e))?;
    u64::try_from(conn.last_insert_rowid()).map_err(|e| db_err("bad row id", &e))
}

/// Takes the queued calls, oldest first, and clears the queue in one transaction.
pub(crate) fn take_queued(conn: &mut Connection) -> Result<Vec<QueuedToolCall>, StoreError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| db_err("cannot start", &e))?;
    let mut calls = Vec::new();
    {
        let mut stmt = tx
            .prepare(
                "SELECT id, token, tool, arguments, queued_at FROM pending_tool_calls ORDER BY id",
            )
            .map_err(|e| db_err("cannot read tool calls", &e))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })
            .map_err(|e| db_err("cannot read tool calls", &e))?;
        for row in rows {
            let (id, token, tool, arguments, queued_at) =
                row.map_err(|e| db_err("cannot read tool call", &e))?;
            // Arguments that no longer parse must not wedge the queue: run them as null.
            let arguments = serde_json::from_str(&arguments).unwrap_or(serde_json::Value::Null);
            calls.push(QueuedToolCall {
                id: u64::try_from(id).unwrap_or(0),
                token: Token(token),
                tool,
                arguments,
                queued_at: Millis(u64::try_from(queued_at).unwrap_or(0)),
            });
        }
    }
    tx.execute("DELETE FROM pending_tool_calls", [])
        .map_err(|e| db_err("cannot clear tool calls", &e))?;
    tx.commit().map_err(|e| db_err("cannot commit", &e))?;
    Ok(calls)
}
