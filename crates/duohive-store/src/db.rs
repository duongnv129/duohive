//! `state.db`: live agent state and hooks queued while DuoHive was closed.
//!
//! SQLite in WAL mode so `duohive hook` can insert while the app holds the
//! file open. Tokens live here, so the file and its directory are owner-only.

use std::fs::{DirBuilder, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;
use std::time::Duration;

use duohive_core::hook::HookMessage;
use duohive_core::ports::StoreError;
use duohive_core::time::Millis;
use rusqlite::{Connection, TransactionBehavior};

const DIR_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;

/// Append-only: a schema change is a new entry, never an edit. `PRAGMA user_version` is the count applied.
const MIGRATIONS: &[&str] = &["
CREATE TABLE sessions (agent TEXT PRIMARY KEY, token TEXT, window TEXT, state TEXT NOT NULL,
  quiet INTEGER NOT NULL, quiet_since INTEGER, from_screen INTEGER NOT NULL, since INTEGER NOT NULL,
  error TEXT, last_reply TEXT, task_at_launch INTEGER NOT NULL, saw_end_hook INTEGER NOT NULL, last_task TEXT);
CREATE TABLE pending_hooks (id INTEGER PRIMARY KEY AUTOINCREMENT, token TEXT NOT NULL, agent TEXT NOT NULL,
  event TEXT NOT NULL, payload TEXT NOT NULL, received_at INTEGER NOT NULL);
"];

pub(crate) fn db_err(what: &str, e: &impl std::fmt::Display) -> StoreError {
    StoreError(format!("state.db: {what}: {e}"))
}

fn create_private(path: &Path) -> Result<(), StoreError> {
    if let Some(dir) = path.parent() {
        DirBuilder::new()
            .recursive(true)
            .mode(DIR_MODE)
            .create(dir)
            .map_err(|e| db_err("cannot create the data directory", &e))?;
    }
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(FILE_MODE)
        .open(path)
        .map(drop)
        .map_err(|e| db_err("cannot create the file", &e))
}

/// Opens (creating, owner-only) the database in WAL mode and applies the migrations.
/// The app passes a 1000 ms `busy_timeout`, `duohive hook` 200 ms.
pub fn open_db(path: &Path, busy_timeout: Duration) -> Result<Connection, StoreError> {
    create_private(path)?;
    let mut conn = Connection::open(path).map_err(|e| db_err("cannot open", &e))?;
    conn.busy_timeout(busy_timeout)
        .map_err(|e| db_err("cannot set busy timeout", &e))?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| db_err("cannot enable WAL", &e))?;
    migrate(&mut conn)?;
    Ok(conn)
}

fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| db_err("cannot start migration", &e))?;
    let applied: i64 = tx
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| db_err("cannot read schema version", &e))?;
    let applied = usize::try_from(applied).unwrap_or(0);
    if applied > MIGRATIONS.len() {
        return Err(StoreError(format!(
            "state.db is from a newer DuoHive (schema {applied}, this build knows {})",
            MIGRATIONS.len()
        )));
    }
    for sql in &MIGRATIONS[applied..] {
        tx.execute_batch(sql)
            .map_err(|e| db_err("migration failed", &e))?;
    }
    tx.pragma_update(None, "user_version", to_i64(MIGRATIONS.len() as u64))
        .map_err(|e| db_err("cannot record schema version", &e))?;
    tx.commit()
        .map_err(|e| db_err("cannot commit migration", &e))
}

pub(crate) fn to_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

/// Queues a hook for the app to replay (used by `duohive hook` when the socket is unreachable).
pub fn insert_pending_hook(
    conn: &Connection,
    m: &HookMessage,
    received_at: Millis,
) -> Result<(), StoreError> {
    let payload = serde_json::to_string(&m.payload).map_err(|e| db_err("bad payload", &e))?;
    conn.execute(
        "INSERT INTO pending_hooks (token, agent, event, payload, received_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![m.token, m.agent, m.event, payload, to_i64(received_at.0)],
    )
    .map(drop)
    .map_err(|e| db_err("cannot queue hook", &e))
}

/// Takes the queued hooks, oldest first, and clears the queue in one transaction.
pub(crate) fn take_pending(conn: &mut Connection) -> Result<Vec<HookMessage>, StoreError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| db_err("cannot start", &e))?;
    let mut hooks = Vec::new();
    {
        let mut stmt = tx
            .prepare("SELECT token, agent, event, payload FROM pending_hooks ORDER BY id")
            .map_err(|e| db_err("cannot read hooks", &e))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, String>(3)?))
            })
            .map_err(|e| db_err("cannot read hooks", &e))?;
        for row in rows {
            let (token, agent, event, payload) = row.map_err(|e| db_err("cannot read hook", &e))?;
            // A payload that no longer parses must not wedge the queue: replay it as null.
            let payload = serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null);
            hooks.push(HookMessage {
                token,
                agent,
                event,
                payload,
            });
        }
    }
    tx.execute("DELETE FROM pending_hooks", [])
        .map_err(|e| db_err("cannot clear hooks", &e))?;
    tx.commit().map_err(|e| db_err("cannot commit", &e))?;
    Ok(hooks)
}
