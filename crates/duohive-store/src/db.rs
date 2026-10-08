//! `state.db`: live agent state and hooks queued while DuoHive was closed.
//!
//! SQLite in WAL mode so `duohive hook` can insert while the app holds the
//! file open. Tokens live here, so the file and its directory are owner-only.

use std::fs::{self, DirBuilder, OpenOptions};
use std::io::ErrorKind;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

use duohive_core::hook::HookMessage;
use duohive_core::ports::StoreError;
use duohive_core::time::Millis;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};

const DIR_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;

/// Append-only: a schema change is a new entry, never an edit. `PRAGMA user_version` is the count applied.
const MIGRATIONS: &[&str] = &["
CREATE TABLE sessions (agent TEXT PRIMARY KEY, token TEXT, window TEXT, state TEXT NOT NULL,
  quiet INTEGER NOT NULL, quiet_since INTEGER, from_screen INTEGER NOT NULL, since INTEGER NOT NULL,
  error TEXT, last_reply TEXT, task_at_launch INTEGER NOT NULL, saw_end_hook INTEGER NOT NULL, last_task TEXT);
CREATE TABLE pending_hooks (id INTEGER PRIMARY KEY AUTOINCREMENT, token TEXT NOT NULL, agent TEXT NOT NULL,
  event TEXT NOT NULL, payload TEXT NOT NULL, received_at INTEGER NOT NULL);
", "
-- Replay order is firing order (Review Focus 3); older rows fired about when they arrived.
ALTER TABLE pending_hooks ADD COLUMN sent_at_ms INTEGER NOT NULL DEFAULT 0;
UPDATE pending_hooks SET sent_at_ms = received_at;
", "
CREATE TABLE worktrees(agent TEXT NOT NULL, task TEXT NOT NULL, repo TEXT NOT NULL, path TEXT NOT NULL,
  branch TEXT NOT NULL, created_at INTEGER NOT NULL, created_from TEXT NOT NULL, state TEXT NOT NULL,
  checked_at INTEGER, base TEXT, PRIMARY KEY(agent, task));
ALTER TABLE sessions ADD COLUMN worktree TEXT;
"];

pub(crate) fn db_err(what: &str, e: &impl std::fmt::Display) -> StoreError {
    StoreError(format!("state.db: {what}: {e}"))
}

fn tighten(path: &Path, mode: u32) -> Result<(), StoreError> {
    let current = fs::metadata(path)
        .map_err(|e| db_err("cannot stat", &e))?
        .permissions()
        .mode()
        & 0o777;
    if current == mode {
        return Ok(());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|e| db_err("cannot restrict permissions", &e))
}

/// Creates the data dir (leaf 0700, parents default) and the file (0600), and
/// tightens both when they already existed with looser modes.
fn create_private(path: &Path) -> Result<(), StoreError> {
    if let Some(dir) = path.parent() {
        if let Some(grand) = dir.parent() {
            fs::create_dir_all(grand)
                .map_err(|e| db_err("cannot create the data directory", &e))?;
        }
        match DirBuilder::new().mode(DIR_MODE).create(dir) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            Err(e) => return Err(db_err("cannot create the data directory", &e)),
        }
        tighten(dir, DIR_MODE)?;
    }
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(FILE_MODE)
        .open(path)
        .map_err(|e| db_err("cannot create the file", &e))?;
    tighten(path, FILE_MODE)
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

/// Opens an existing, current database for `duohive hook`, which only inserts:
/// it never creates the file, sets WAL (the app did, and it persists) or
/// migrates, so the hook stays within its single `busy_timeout` budget.
pub fn open_db_for_hook(path: &Path, busy_timeout: Duration) -> Result<Connection, StoreError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| db_err("cannot open", &e))?;
    conn.busy_timeout(busy_timeout)
        .map_err(|e| db_err("cannot set busy timeout", &e))?;
    let seen = read_version(&conn)?;
    if seen != MIGRATIONS.len() {
        return Err(StoreError(format!(
            "state.db schema {seen} is not the {} this build writes",
            MIGRATIONS.len()
        )));
    }
    Ok(conn)
}

fn read_version(conn: &Connection) -> Result<usize, StoreError> {
    let v: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| db_err("cannot read schema version", &e))?;
    usize::try_from(v).map_err(|_| StoreError(format!("state.db: invalid schema version {v}")))
}

fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    let target = MIGRATIONS.len();
    // The hook opens this inside a 1 s budget: take the write lock only when behind.
    let seen = read_version(conn)?;
    if seen > target {
        return Err(newer_error(seen, target));
    }
    if seen == target {
        return Ok(());
    }
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| db_err("cannot start migration", &e))?;
    let applied = read_version(&tx)?; // another process may have migrated meanwhile
    for sql in MIGRATIONS.iter().skip(applied) {
        tx.execute_batch(sql)
            .map_err(|e| db_err("migration failed", &e))?;
    }
    let version = i64::try_from(target).map_err(|e| db_err("bad schema version", &e))?;
    tx.pragma_update(None, "user_version", version)
        .map_err(|e| db_err("cannot record schema version", &e))?;
    tx.commit()
        .map_err(|e| db_err("cannot commit migration", &e))
}

fn newer_error(seen: usize, known: usize) -> StoreError {
    StoreError(format!(
        "state.db is from a newer DuoHive (schema {seen}, this build knows {known})"
    ))
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
        "INSERT INTO pending_hooks (token, agent, event, payload, received_at, sent_at_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            m.token,
            m.agent,
            m.event,
            payload,
            to_i64(received_at.0),
            to_i64(m.sent_at_ms)
        ],
    )
    .map(drop)
    .map_err(|e| db_err("cannot queue hook", &e))
}

/// Takes the queued hooks in the order they fired (`sent_at_ms`, then arrival)
/// and clears the queue in one transaction. A hook the app stored at shutdown
/// can land after a later one that fell back here meanwhile; firing time puts
/// it back first (Review Focus 3).
pub(crate) fn take_pending(conn: &mut Connection) -> Result<Vec<HookMessage>, StoreError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| db_err("cannot start", &e))?;
    let mut hooks = Vec::new();
    {
        let mut stmt = tx
            .prepare(
                "SELECT token, agent, event, payload, sent_at_ms FROM pending_hooks \
                 ORDER BY sent_at_ms, id",
            )
            .map_err(|e| db_err("cannot read hooks", &e))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })
            .map_err(|e| db_err("cannot read hooks", &e))?;
        for row in rows {
            let (token, agent, event, payload, sent_at_ms) =
                row.map_err(|e| db_err("cannot read hook", &e))?;
            // A payload that no longer parses must not wedge the queue: replay it as null.
            let payload = serde_json::from_str(&payload).unwrap_or_else(|e| {
                tracing::debug!(%agent, %event, error = %e, "queued hook payload unreadable; replaying as null");
                serde_json::Value::Null
            });
            hooks.push(HookMessage {
                sent_at_ms: u64::try_from(sent_at_ms).unwrap_or(0),
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
