#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::match_wild_err_arm,
    reason = "test-only helpers"
)]

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use duohive_core::hook::HookMessage;
use duohive_core::launch::{Token, WindowId};
use duohive_core::ports::{SessionRow, Store};
use duohive_core::status::{AgentState, Status};
use duohive_core::testkit::{example_org, n};
use duohive_core::time::Millis;
use duohive_core::worktree::{BaseRef, TaskId, WorktreeRow, WorktreeState};
use duohive_store::db::{insert_pending_hook, open_db, open_db_for_hook};
use duohive_store::{Paths, SqliteStore};

fn temp_paths() -> (tempfile::TempDir, Paths) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths {
        config_dir: tmp.path().join("config"),
        data_dir: tmp.path().join("data"),
        tmux_socket: "duohive".into(),
    };
    (tmp, paths)
}

fn row(state: AgentState) -> SessionRow {
    SessionRow {
        agent: n("auth-service"),
        token: Some(Token("tok-1".into())),
        window: Some(WindowId("@3".into())),
        status: Status {
            state,
            quiet: true,
            quiet_since: Some(Millis(4_000)),
            from_screen: true,
            since: Millis(1_000),
            error: None,
            waiting_on: None,
        },
        last_reply: Some("hi".into()),
        task_at_launch: true,
        saw_end_hook: false,
        last_task: Some("fix it".into()),
        worktree: None,
    }
}

fn hook(event: &str) -> HookMessage {
    HookMessage {
        sent_at_ms: 0,
        token: "tok-1".into(),
        agent: "auth-service".into(),
        event: event.into(),
        payload: serde_json::json!({"k": event}),
    }
}

const BUSY: Duration = Duration::from_millis(1000);

fn user_version(conn: &rusqlite::Connection) -> i64 {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn migrations_apply_once_and_set_user_version() {
    let (_t, paths) = temp_paths();
    let first = open_db(&paths.state_db(), BUSY).unwrap();
    assert_eq!(user_version(&first), 3);
    drop(first);
    let second = open_db(&paths.state_db(), BUSY).unwrap();
    assert_eq!(user_version(&second), 3);
}

#[test]
fn wal_mode_enabled() {
    let (_t, paths) = temp_paths();
    let conn = open_db(&paths.state_db(), BUSY).unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

#[test]
fn session_round_trip() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    store.save_session(&row(AgentState::Working)).unwrap();
    assert_eq!(
        store.load_sessions().unwrap(),
        vec![row(AgentState::Working)]
    );
}

#[test]
fn save_session_upserts() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    store.save_session(&row(AgentState::Working)).unwrap();
    store.save_session(&row(AgentState::Finished)).unwrap();
    let rows = store.load_sessions().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status.state, AgentState::Finished);
}

#[test]
fn delete_session_removes_and_tolerates_a_missing_row() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    store.delete_session(&n("auth-service")).unwrap();
    store.save_session(&row(AgentState::Idle)).unwrap();
    store.delete_session(&n("auth-service")).unwrap();
    assert!(store.load_sessions().unwrap().is_empty());
}

#[test]
fn errored_row_keeps_its_error() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    let mut r = row(AgentState::Errored);
    r.status.error = Some("claude not found (code 127)".into());
    r.token = None;
    r.window = None;
    r.last_reply = None;
    r.last_task = None;
    store.save_session(&r).unwrap();
    assert_eq!(store.load_sessions().unwrap(), vec![r]);
}

#[test]
fn pending_hooks_are_fifo_and_taken_once() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    let conn = open_db(&paths.state_db(), BUSY).unwrap();
    for e in ["busy", "turn-end", "session-end"] {
        insert_pending_hook(&conn, &hook(e), Millis(1)).unwrap();
    }
    let events: Vec<String> = store
        .take_pending_hooks()
        .unwrap()
        .into_iter()
        .map(|h| h.event)
        .collect();
    assert_eq!(events, ["busy", "turn-end", "session-end"]);
    assert!(store.take_pending_hooks().unwrap().is_empty());
}

#[test]
fn a_second_connection_can_insert_while_store_is_open() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    store.save_session(&row(AgentState::Working)).unwrap();
    let other = open_db(&paths.state_db(), Duration::from_millis(200)).unwrap();
    insert_pending_hook(&other, &hook("busy"), Millis(5)).unwrap();
    assert_eq!(store.take_pending_hooks().unwrap(), vec![hook("busy")]);
}

#[test]
fn db_file_and_dir_are_owner_only() {
    let (_t, paths) = temp_paths();
    let _store = SqliteStore::open(&paths).unwrap();
    let file = std::fs::metadata(paths.state_db())
        .unwrap()
        .permissions()
        .mode();
    let dir = std::fs::metadata(&paths.data_dir)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(file & 0o077, 0, "file mode {file:o}");
    assert_eq!(dir & 0o077, 0, "dir mode {dir:o}");
}

#[test]
fn store_load_org_uses_org_file_and_last_good() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    assert_eq!(store.load_org(), Ok(None));
    store.save_org(&example_org()).unwrap();
    assert_eq!(store.load_org(), Ok(Some(example_org())));
    std::fs::write(paths.org_toml(), "cli = \"claud\"\n").unwrap();
    let err = store.load_org().unwrap_err();
    assert!(err.line.is_some());
    assert_eq!(store.last_good_org(), Some(example_org()));
}

#[test]
fn mark_last_good_writes_last_good_and_load_does_not() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    let org = example_org();
    // Hand-write a valid org.toml via a scratch save, then drop last-good.
    store.save_org(&org).unwrap();
    std::fs::remove_file(paths.org_last_good()).unwrap();
    assert_eq!(store.load_org(), Ok(Some(org.clone())));
    assert!(!paths.org_last_good().exists());
    assert_eq!(store.last_good_org(), None);
    store.mark_last_good(&org).unwrap();
    assert_eq!(store.last_good_org(), Some(org));
}

#[test]
fn save_settings_writes_the_file() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    store
        .save_settings(&duohive_core::settings::Settings::default())
        .unwrap();
    assert!(paths.settings_toml().exists());
}

#[test]
fn save_org_failure_leaves_files_unchanged_and_no_tmp() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    let org = example_org();
    store.save_org(&org).unwrap();
    store.save_org(&org).unwrap(); // creates org.toml.bak
    let toml_before = std::fs::read(paths.org_toml()).unwrap();
    let bak_before = std::fs::read(paths.org_backup()).unwrap();

    let mut other = org.clone();
    other.defaults.prompt = Some("changed".into());
    let ro = std::fs::Permissions::from_mode(0o500);
    std::fs::set_permissions(&paths.config_dir, ro).unwrap();
    let probe = paths.config_dir.join("probe");
    let is_root = std::fs::write(&probe, b"x").is_ok(); // root ignores directory modes
    let result = if is_root {
        None
    } else {
        Some(store.save_org(&other))
    };
    let _ = std::fs::remove_file(&probe);
    std::fs::set_permissions(&paths.config_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    if is_root {
        return;
    }
    let result = result.unwrap();

    assert!(result.is_err());
    assert_eq!(std::fs::read(paths.org_toml()).unwrap(), toml_before);
    assert_eq!(std::fs::read(paths.org_backup()).unwrap(), bak_before);
    assert!(!paths.config_dir.join("org.toml.tmp").exists());
}

#[test]
fn failed_backup_removes_the_stale_tmp() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    store.save_org(&example_org()).unwrap();
    // A directory where the backup file belongs makes the backup step fail.
    std::fs::create_dir(paths.org_backup()).unwrap();
    let toml_before = std::fs::read(paths.org_toml()).unwrap();
    let mut other = example_org();
    other.defaults.prompt = Some("changed".into());
    assert!(store.save_org(&other).is_err());
    assert_eq!(std::fs::read(paths.org_toml()).unwrap(), toml_before);
    assert!(!paths.config_dir.join("org.toml.tmp").exists());
}

#[test]
fn save_org_reports_a_last_good_failure_as_saved() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    // A directory where the last-good file belongs makes only that write fail.
    std::fs::create_dir(paths.org_last_good()).unwrap();
    let err = store.save_org(&example_org()).unwrap_err();
    assert!(err.0.contains("org.toml was saved"), "{}", err.0);
    assert_eq!(store.load_org(), Ok(Some(example_org())));
    assert!(!paths.data_dir.join("org.last-good.toml.tmp").exists());
}

#[test]
fn a_bad_session_row_is_skipped_and_removed() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    store.save_session(&row(AgentState::Working)).unwrap();
    let conn = open_db(&paths.state_db(), BUSY).unwrap();
    for (agent, state) in [("ghost", "bogus"), ("bad name!", "idle")] {
        conn.execute(
            "INSERT INTO sessions (agent, state, quiet, from_screen, since, task_at_launch, saw_end_hook) \
             VALUES (?1, ?2, 0, 0, 0, 0, 0)",
            [agent, state],
        )
        .unwrap();
    }
    assert_eq!(
        store.load_sessions().unwrap(),
        vec![row(AgentState::Working)]
    );
    let left: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left, 1);
}

#[test]
fn existing_loose_modes_are_tightened() {
    let (_t, paths) = temp_paths();
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    std::fs::set_permissions(&paths.data_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(paths.state_db(), b"").unwrap();
    std::fs::set_permissions(paths.state_db(), std::fs::Permissions::from_mode(0o644)).unwrap();
    let _store = SqliteStore::open(&paths).unwrap();
    let file = std::fs::metadata(paths.state_db())
        .unwrap()
        .permissions()
        .mode();
    let dir = std::fs::metadata(&paths.data_dir)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(file & 0o777, 0o600);
    assert_eq!(dir & 0o777, 0o700);
}

#[test]
fn negative_user_version_is_a_clear_error() {
    let (_t, paths) = temp_paths();
    drop(open_db(&paths.state_db(), BUSY).unwrap());
    let conn = rusqlite::Connection::open(paths.state_db()).unwrap();
    conn.pragma_update(None, "user_version", -1).unwrap();
    drop(conn);
    let err = open_db(&paths.state_db(), BUSY).unwrap_err();
    assert!(err.0.contains("schema version"), "{}", err.0);
}

#[test]
fn hook_open_never_creates_or_migrates() {
    let (_t, paths) = temp_paths();
    // DuoHive never ran: no file, and the hook does not create one.
    assert!(open_db_for_hook(&paths.state_db(), Duration::from_millis(200)).is_err());
    assert!(!paths.state_db().exists());
    // A database behind the schema (here: empty) is refused rather than migrated.
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    drop(rusqlite::Connection::open(paths.state_db()).unwrap());
    let err = open_db_for_hook(&paths.state_db(), Duration::from_millis(200)).unwrap_err();
    assert!(err.0.contains("schema"), "{err:?}");
    let conn = rusqlite::Connection::open(paths.state_db()).unwrap();
    assert_eq!(user_version(&conn), 0);
}

#[test]
fn hook_open_inserts_into_a_current_database() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    let conn = open_db_for_hook(&paths.state_db(), Duration::from_millis(200)).unwrap();
    insert_pending_hook(&conn, &hook("busy"), Millis(5)).unwrap();
    assert_eq!(store.take_pending_hooks().unwrap(), vec![hook("busy")]);
}

fn fired_at(event: &str, sent_at_ms: u64) -> HookMessage {
    HookMessage {
        sent_at_ms,
        ..hook(event)
    }
}

#[test]
fn pending_hooks_replay_in_the_order_they_fired() {
    // Shutdown: a later hook falls back to the database while the core still
    // holds an earlier one, which it stores afterwards. Replay goes by firing time.
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    let fallback = open_db_for_hook(&paths.state_db(), Duration::from_millis(200)).unwrap();
    insert_pending_hook(&fallback, &fired_at("turn-end", 200), Millis(210)).unwrap();
    store
        .queue_pending_hook(&fired_at("busy", 100), Millis(300))
        .unwrap();
    assert_eq!(
        store.take_pending_hooks().unwrap(),
        vec![fired_at("busy", 100), fired_at("turn-end", 200)]
    );
}

#[test]
fn hooks_queued_before_the_firing_time_column_replay_by_arrival() {
    // A schema-1 database (no `sent_at_ms`): its rows take their `received_at`.
    let (_t, paths) = temp_paths();
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let old = rusqlite::Connection::open(paths.state_db()).unwrap();
    old.execute_batch(
        "CREATE TABLE sessions (agent TEXT PRIMARY KEY, token TEXT, window TEXT, state TEXT NOT NULL,
           quiet INTEGER NOT NULL, quiet_since INTEGER, from_screen INTEGER NOT NULL, since INTEGER NOT NULL,
           error TEXT, last_reply TEXT, task_at_launch INTEGER NOT NULL, saw_end_hook INTEGER NOT NULL, last_task TEXT);
         CREATE TABLE pending_hooks (id INTEGER PRIMARY KEY AUTOINCREMENT, token TEXT NOT NULL, agent TEXT NOT NULL,
           event TEXT NOT NULL, payload TEXT NOT NULL, received_at INTEGER NOT NULL);
         INSERT INTO pending_hooks (token, agent, event, payload, received_at)
           VALUES ('tok-1', 'auth-service', 'busy', '{\"k\":\"busy\"}', 50);
         PRAGMA user_version = 1;",
    )
    .unwrap();
    drop(old);
    let mut store = SqliteStore::open(&paths).unwrap();
    assert_eq!(
        store.take_pending_hooks().unwrap(),
        vec![fired_at("busy", 50)]
    );
}

fn wt_row(agent: &str, task: &str, state: WorktreeState) -> WorktreeRow {
    WorktreeRow {
        agent: n(agent),
        task: TaskId(task.into()),
        repo: "/repo".into(),
        path: format!("/data/worktrees/{agent}/{task}").into(),
        branch: format!("duohive/{agent}/{task}"),
        created_at: Millis(5),
        created_from: "abc123".into(),
        state,
        checked_at: Some(Millis(9)),
        base: Some(BaseRef("origin/main".into())),
    }
}

#[test]
fn worktree_rows_round_trip() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    let states = [
        WorktreeState::Unchecked,
        WorktreeState::InUse,
        WorktreeState::NotMerged { files_changed: 7 },
        WorktreeState::MergedDirty,
        WorktreeState::Merged,
        WorktreeState::Removing,
        WorktreeState::CheckFailed("no default branch: it's \"odd\" | ünï".into()),
        WorktreeState::CheckFailed(String::new()),
    ];
    for (i, state) in states.into_iter().enumerate() {
        let mut row = wt_row("squad-1", "fix", state);
        if i == 0 {
            row.checked_at = None;
            row.base = None;
        }
        store.save_worktree(&row).unwrap();
        assert_eq!(store.load_worktrees().unwrap(), vec![row.clone()]);
        store.delete_worktree(&row.agent, &row.task).unwrap();
        assert!(store.load_worktrees().unwrap().is_empty());
    }
}

#[test]
fn same_task_two_agents_two_rows() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    let a = wt_row("squad-1", "fix-tests", WorktreeState::InUse);
    let b = wt_row("squad-2", "fix-tests", WorktreeState::Merged);
    store.save_worktree(&a).unwrap();
    store.save_worktree(&b).unwrap();
    assert_eq!(store.load_worktrees().unwrap(), vec![a.clone(), b.clone()]);
    store.delete_worktree(&a.agent, &a.task).unwrap();
    assert_eq!(store.load_worktrees().unwrap(), vec![b]);
}

#[test]
fn session_worktree_round_trips() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    let mut r = row(AgentState::Working);
    r.worktree = Some(TaskId("fix-tests".into()));
    store.save_session(&r).unwrap();
    assert_eq!(store.load_sessions().unwrap(), vec![r]);
}

#[test]
fn migration_3_applies_on_v2_db() {
    let (_t, paths) = temp_paths();
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let old = rusqlite::Connection::open(paths.state_db()).unwrap();
    old.execute_batch(
        "CREATE TABLE sessions (agent TEXT PRIMARY KEY, token TEXT, window TEXT, state TEXT NOT NULL,
           quiet INTEGER NOT NULL, quiet_since INTEGER, from_screen INTEGER NOT NULL, since INTEGER NOT NULL,
           error TEXT, last_reply TEXT, task_at_launch INTEGER NOT NULL, saw_end_hook INTEGER NOT NULL, last_task TEXT);
         CREATE TABLE pending_hooks (id INTEGER PRIMARY KEY AUTOINCREMENT, token TEXT NOT NULL, agent TEXT NOT NULL,
           event TEXT NOT NULL, payload TEXT NOT NULL, received_at INTEGER NOT NULL, sent_at_ms INTEGER NOT NULL DEFAULT 0);
         INSERT INTO sessions VALUES ('auth-service', NULL, NULL, 'idle', 0, NULL, 0, 1, NULL, NULL, 0, 0, 'old task');
         PRAGMA user_version = 2;",
    )
    .unwrap();
    drop(old);
    let mut store = SqliteStore::open(&paths).unwrap();
    let rows = store.load_sessions().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].worktree, None);
    assert_eq!(rows[0].last_task.as_deref(), Some("old task"));
    store
        .save_worktree(&wt_row("squad-1", "fix", WorktreeState::InUse))
        .unwrap();
    drop(store);
    let conn = open_db(&paths.state_db(), BUSY).unwrap();
    assert_eq!(user_version(&conn), 3);
}
