#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::match_wild_err_arm,
    reason = "test-only helpers"
)]

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use duohive_core::card::{Card, CardChange, CardId, CardOutcome, CardSource};
use duohive_core::chat::{ChatEntry, ChatId, ChatKind};
use duohive_core::hook::HookMessage;
use duohive_core::inbox::{Message, MessageId, MessageKind, MessageStatus, Sender};
use duohive_core::launch::{Token, WindowId};
use duohive_core::ports::{SessionRow, Store};
use duohive_core::status::{AgentState, Status};
use duohive_core::testkit::{example_org, n};
use duohive_core::time::Millis;
use duohive_core::worktree::{BaseRef, RunIn, TaskId, WorktreeRow, WorktreeState};
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
        launched_as_duo: false,
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
    assert_eq!(user_version(&first), 7);
    drop(first);
    let second = open_db(&paths.state_db(), BUSY).unwrap();
    assert_eq!(user_version(&second), 7);
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
    assert_eq!(user_version(&conn), 7);
}

#[test]
fn migration_4_applies_on_v3_db() {
    let (_t, paths) = temp_paths();
    {
        let conn = open_db(&paths.state_db(), BUSY).unwrap();
        conn.execute_batch(
            "DROP TABLE cards;
             DROP TABLE chat;
             DROP TABLE messages;
             DROP TABLE pending_tool_calls;
             ALTER TABLE sessions DROP COLUMN launched_as_duo;
             INSERT INTO sessions (agent, token, window, state, quiet, quiet_since, from_screen, since,
               error, last_reply, task_at_launch, saw_end_hook, last_task)
               VALUES ('Duo', 'tok-d', '@1', 'idle', 0, NULL, 0, 1, NULL, NULL, 0, 0, NULL);
             PRAGMA user_version = 3;",
        )
        .unwrap();
    }
    let mut store = SqliteStore::open(&paths).unwrap();
    let rows = store.load_sessions().unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].launched_as_duo);
    assert_eq!(store.take_queued_tool_calls().unwrap(), vec![]);
    drop(store);
    let conn = open_db(&paths.state_db(), BUSY).unwrap();
    assert_eq!(user_version(&conn), 7);
}

#[test]
fn session_launched_as_duo_round_trips() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    let r = SessionRow {
        launched_as_duo: true,
        ..row(AgentState::Idle)
    };
    store.save_session(&r).unwrap();
    assert_eq!(store.load_sessions().unwrap(), vec![r]);
}

#[test]
fn queued_tool_calls_round_trip() {
    use duohive_store::closed::queue_tool_call;
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    let args = serde_json::json!({"to": "pay-qa", "text": "hi"});
    let first = queue_tool_call(&paths, "tok-1", "send_message", &args, Millis(10)).unwrap();
    let second = queue_tool_call(
        &paths,
        "tok-2",
        "stop_agent",
        &serde_json::json!({"agent": "pay-qa"}),
        Millis(20),
    )
    .unwrap();
    assert!(second > first);
    let calls = store.take_queued_tool_calls().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].id, first);
    assert_eq!(calls[0].token, Token("tok-1".into()));
    assert_eq!(calls[0].tool, "send_message");
    assert_eq!(calls[0].arguments, args);
    assert_eq!(calls[0].queued_at, Millis(10));
    assert_eq!(calls[1].tool, "stop_agent");
    assert_eq!(store.take_queued_tool_calls().unwrap(), vec![]);
}

#[test]
fn closed_rights_from_launched_as_duo() {
    use duohive_core::tool::Rights;
    use duohive_store::closed::rights_for_token;
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    store
        .save_session(&SessionRow {
            agent: n("Duo"),
            token: Some(Token("tok-duo".into())),
            launched_as_duo: true,
            ..row(AgentState::Idle)
        })
        .unwrap();
    store.save_session(&row(AgentState::Idle)).unwrap();
    assert_eq!(
        rights_for_token(&paths, "tok-duo").unwrap(),
        Some(Rights::Duo)
    );
    assert_eq!(
        rights_for_token(&paths, "tok-1").unwrap(),
        Some(Rights::Agent)
    );
    assert_eq!(rights_for_token(&paths, "nope").unwrap(), None);
    assert_eq!(rights_for_token(&paths, "").unwrap(), None);
}

fn message(id: u64, from: Sender, kind: MessageKind, status: MessageStatus, at: u64) -> Message {
    Message {
        id: MessageId(id),
        from,
        to: n("pay-qa"),
        kind,
        text: format!("text {id}\nline two"),
        status,
        queued_at: Millis(at),
        delivered_at: None,
    }
}

#[test]
fn messages_round_trip() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    let squad = || Sender::Agent(n("squad-1"));
    let text = MessageKind::Text;
    let old_answered = message(1, squad(), text, MessageStatus::Answered, 100);
    let queued = message(2, Sender::You, text, MessageStatus::Queued, 200);
    let delivered = Message {
        delivered_at: Some(Millis(350)),
        ..message(3, squad(), text, MessageStatus::Delivered, 300)
    };
    let card_kind = MessageKind::CardAnswer(CardId(9));
    let card = message(4, Sender::You, card_kind, MessageStatus::Queued, 400);
    let dropped = |reason: &str| MessageStatus::Dropped {
        reason: reason.into(),
    };
    let recent_dropped = message(5, squad(), text, dropped("no sign it started"), 5_000);
    let old_dropped = message(6, squad(), text, dropped("gone"), 150);
    let pasting = message(7, squad(), text, MessageStatus::Pasting, 160);
    for m in [
        &old_answered,
        &queued,
        &delivered,
        &card,
        &recent_dropped,
        &old_dropped,
        &pasting,
    ] {
        store.save_message(m).unwrap();
    }
    // Queued, pasting and delivered whatever their age, recent ones, and the newest id.
    assert_eq!(
        store.load_messages(Millis(1_000)).unwrap(),
        vec![queued.clone(), delivered, card, recent_dropped, pasting]
    );
    assert_eq!(
        store.find_message(MessageId(1)).unwrap(),
        Some(old_answered)
    );
    assert_eq!(store.find_message(MessageId(42)).unwrap(), None);
    // Saving again replaces.
    let taken = Message {
        status: MessageStatus::Delivered,
        delivered_at: Some(Millis(260)),
        ..queued
    };
    store.save_message(&taken).unwrap();
    assert_eq!(store.find_message(MessageId(2)).unwrap(), Some(taken));
}

#[test]
fn migration_5_applies() {
    let (_t, paths) = temp_paths();
    {
        let conn = open_db(&paths.state_db(), BUSY).unwrap();
        conn.execute_batch(
            "DROP TABLE cards; DROP TABLE chat; DROP TABLE messages; PRAGMA user_version = 4;",
        )
        .unwrap();
    }
    let mut store = SqliteStore::open(&paths).unwrap();
    assert_eq!(store.load_messages(Millis(0)).unwrap(), vec![]);
    let first = message(1, Sender::You, MessageKind::Text, MessageStatus::Queued, 1);
    store.save_message(&first).unwrap();
    drop(store);
    let conn = open_db(&paths.state_db(), BUSY).unwrap();
    assert_eq!(user_version(&conn), 7);
    let sender: Option<String> = conn
        .query_row("SELECT sender FROM messages WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(sender, None);
}

fn chat_entry(id: u64, kind: ChatKind, text: &str) -> ChatEntry {
    ChatEntry {
        id: ChatId(id),
        at: Millis(1_000 + id),
        kind,
        text: text.into(),
    }
}

#[test]
fn chat_round_trip() {
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    let card = |id: u64, outcome: CardOutcome| {
        chat_entry(
            id,
            ChatKind::CardResult {
                card: CardId(id),
                outcome,
            },
            "stop squad-1",
        )
    };
    let entries = vec![
        chat_entry(
            1,
            ChatKind::You { message: None },
            "what's blocked?\nand why",
        ),
        chat_entry(
            2,
            ChatKind::You {
                message: Some(MessageId(7)),
            },
            "fix it",
        ),
        chat_entry(3, ChatKind::Duo, "billing-api waits on an approval"),
        chat_entry(
            4,
            ChatKind::Action {
                result: "queued (pay-qa is working)".into(),
            },
            "@pay-qa \"rerun\"",
        ),
        card(5, CardOutcome::Approved),
        card(6, CardOutcome::EditedApproved),
        card(7, CardOutcome::Rejected),
        card(
            8,
            CardOutcome::NotApplied("squad-1 is already stopped".into()),
        ),
    ];
    for e in &entries {
        store.save_chat(e).unwrap();
    }
    // A note is never stored.
    store
        .save_chat(&chat_entry(9, ChatKind::Note, "/help"))
        .unwrap();
    assert_eq!(store.load_chat(500).unwrap(), entries);
    // The last `n`, oldest first.
    assert_eq!(store.load_chat(3).unwrap(), entries[5..].to_vec());
    assert_eq!(store.load_chat(0).unwrap(), vec![]);
}

#[test]
fn migration_6_applies() {
    let (_t, paths) = temp_paths();
    {
        let conn = open_db(&paths.state_db(), BUSY).unwrap();
        conn.execute_batch("DROP TABLE cards; DROP TABLE chat; PRAGMA user_version = 5;")
            .unwrap();
    }
    let mut store = SqliteStore::open(&paths).unwrap();
    assert_eq!(store.load_chat(500).unwrap(), vec![]);
    let first = chat_entry(1, ChatKind::Duo, "hello");
    store.save_chat(&first).unwrap();
    assert_eq!(store.load_chat(500).unwrap(), vec![first]);
    drop(store);
    let conn = open_db(&paths.state_db(), BUSY).unwrap();
    assert_eq!(user_version(&conn), 7);
    let kind: String = conn
        .query_row("SELECT kind FROM chat WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(kind, "duo");
}

fn a_card(id: u64, source: CardSource, change: CardChange, answer: Option<CardOutcome>) -> Card {
    Card {
        id: CardId(id),
        source,
        change,
        raised_at: Millis(1_000 + id),
        answer: answer.map(|o| (o, Millis(2_000 + id))),
    }
}

#[test]
fn cards_round_trip() {
    use duohive_core::command::{AgentDraft, OrgEdit};
    use duohive_core::org::{Fields, RoleKey};
    use duohive_core::testkit::change_examples;
    let (_t, paths) = temp_paths();
    let mut store = SqliteStore::open(&paths).unwrap();
    let role = RoleKey {
        team: None,
        role: duohive_core::names::NodeName::parse("Generalist").unwrap(),
    };
    let mut changes: Vec<CardChange> = change_examples()
        .into_iter()
        .map(|(_, edit)| CardChange::Org(edit))
        .collect();
    changes.push(CardChange::Org(OrgEdit::Delete(
        duohive_core::org::NodeKey::Agent(n("squad-2")),
    )));
    changes.push(CardChange::Stop {
        agent: n("squad-1"),
    });
    changes.push(CardChange::NewAgent {
        role,
        agent: AgentDraft {
            name: n("squad-4"),
            fields: Fields {
                model: Some("opus".into()),
                ..Fields::default()
            },
        },
        task: "Fix the \"flaky\" test\nthen rerun".into(),
        run: RunIn::NewWorktree,
        clone_of: Some(n("squad-1")),
    });
    let pending: Vec<Card> = changes
        .into_iter()
        .enumerate()
        .map(|(i, change)| {
            let source = if i % 2 == 0 {
                CardSource::Duo
            } else {
                CardSource::NoFreeAgent
            };
            a_card(u64::try_from(i).unwrap() + 1, source, change, None)
        })
        .collect();
    for c in &pending {
        store.save_card(c).unwrap();
    }
    assert_eq!(store.load_cards().unwrap(), pending);
    // Answered cards leave the load, except the newest, so ids continue.
    let stop = CardChange::Stop {
        agent: n("squad-1"),
    };
    let outcomes = [
        CardOutcome::Approved,
        CardOutcome::EditedApproved,
        CardOutcome::Rejected,
        CardOutcome::NotApplied("squad-1 is already stopped".into()),
    ];
    let next = u64::try_from(pending.len()).unwrap() + 1;
    let answered: Vec<Card> = outcomes
        .into_iter()
        .enumerate()
        .map(|(i, o)| {
            a_card(
                next + u64::try_from(i).unwrap(),
                CardSource::Duo,
                stop.clone(),
                Some(o),
            )
        })
        .collect();
    for c in &answered {
        store.save_card(c).unwrap();
    }
    let mut want = pending.clone();
    want.push(answered[3].clone());
    assert_eq!(store.load_cards().unwrap(), want);
    // Answering replaces the row.
    let mut first = pending[0].clone();
    first.answer = Some((CardOutcome::Rejected, Millis(9_000)));
    store.save_card(&first).unwrap();
    assert_eq!(store.load_cards().unwrap(), want[1..].to_vec());
    // Each answered outcome reads back: make each the newest in turn.
    for (i, c) in answered.iter().enumerate() {
        let mut newest = c.clone();
        newest.id = CardId(100 + u64::try_from(i).unwrap());
        store.save_card(&newest).unwrap();
        assert_eq!(store.load_cards().unwrap().last(), Some(&newest));
    }
}

#[test]
fn migration_7_applies() {
    let (_t, paths) = temp_paths();
    {
        let conn = open_db(&paths.state_db(), BUSY).unwrap();
        conn.execute_batch("DROP TABLE cards; PRAGMA user_version = 6;")
            .unwrap();
    }
    let mut store = SqliteStore::open(&paths).unwrap();
    assert_eq!(store.load_cards().unwrap(), vec![]);
    let first = a_card(
        1,
        CardSource::Duo,
        CardChange::Stop {
            agent: n("squad-1"),
        },
        None,
    );
    store.save_card(&first).unwrap();
    assert_eq!(store.load_cards().unwrap(), vec![first]);
    drop(store);
    let conn = open_db(&paths.state_db(), BUSY).unwrap();
    assert_eq!(user_version(&conn), 7);
    let source: String = conn
        .query_row("SELECT source FROM cards WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(source, "duo");
}
