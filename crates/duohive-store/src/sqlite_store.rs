//! The `Store` port over `org.toml`, `settings.toml` and `state.db`.

use std::time::Duration;

use duohive_core::hook::HookMessage;
use duohive_core::launch::{Token, WindowId};
use duohive_core::names::AgentName;
use duohive_core::org::Org;
use duohive_core::ports::{OrgFileError, SessionRow, Store, StoreError};
use duohive_core::settings::Settings;
use duohive_core::status::{AgentState, Status};
use duohive_core::time::Millis;
use rusqlite::{Connection, Row};

use crate::db::{db_err, open_db, take_pending, to_i64};
use crate::paths::Paths;
use crate::{org_file, settings_file};

/// How long the app waits on a locked database.
const APP_BUSY_TIMEOUT: Duration = Duration::from_millis(1000);

/// The real store: files under the config dir and `state.db` under the data dir.
pub struct SqliteStore {
    paths: Paths,
    conn: Connection,
}

impl SqliteStore {
    /// Opens `state.db` (creating and migrating it) for the app.
    pub fn open(paths: &Paths) -> Result<SqliteStore, StoreError> {
        let conn = open_db(&paths.state_db(), APP_BUSY_TIMEOUT)?;
        Ok(SqliteStore {
            paths: paths.clone(),
            conn,
        })
    }
}

fn parse_state(word: &str) -> Option<AgentState> {
    [
        AgentState::Stopped,
        AgentState::Starting,
        AgentState::Working,
        AgentState::Waiting,
        AgentState::Finished,
        AgentState::Idle,
        AgentState::Errored,
    ]
    .into_iter()
    .find(|s| s.word() == word)
}

fn millis(v: i64) -> Millis {
    Millis(u64::try_from(v).unwrap_or(0))
}

fn read_row(r: &Row<'_>) -> rusqlite::Result<Result<SessionRow, String>> {
    let agent: String = r.get("agent")?;
    let state: String = r.get("state")?;
    let token: Option<String> = r.get("token")?;
    let window: Option<String> = r.get("window")?;
    let quiet_since: Option<i64> = r.get("quiet_since")?;
    let (Ok(agent_name), Some(state)) = (AgentName::parse(&agent), parse_state(&state)) else {
        return Ok(Err(agent));
    };
    Ok(Ok(SessionRow {
        agent: agent_name,
        token: token.map(Token),
        window: window.map(WindowId),
        status: Status {
            state,
            quiet: r.get("quiet")?,
            quiet_since: quiet_since.map(millis),
            from_screen: r.get("from_screen")?,
            since: millis(r.get("since")?),
            error: r.get("error")?,
        },
        last_reply: r.get("last_reply")?,
        task_at_launch: r.get("task_at_launch")?,
        saw_end_hook: r.get("saw_end_hook")?,
        last_task: r.get("last_task")?,
    }))
}

impl Store for SqliteStore {
    fn load_org(&mut self) -> Result<Option<Org>, OrgFileError> {
        org_file::load_org(&self.paths.org_toml())
    }

    fn last_good_org(&mut self) -> Option<Org> {
        org_file::load_org(&self.paths.org_last_good())
            .ok()
            .flatten()
    }

    fn save_org(&mut self, org: &Org) -> Result<(), StoreError> {
        org_file::save_org(&self.paths, org)
    }

    fn mark_last_good(&mut self, org: &Org) -> Result<(), StoreError> {
        org_file::mark_last_good(&self.paths, org)
    }

    fn save_settings(&mut self, s: &Settings) -> Result<(), StoreError> {
        settings_file::save_settings(&self.paths.settings_toml(), s)
    }

    fn load_sessions(&mut self) -> Result<Vec<SessionRow>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM sessions ORDER BY agent")
            .map_err(|e| db_err("cannot read sessions", &e))?;
        let parsed = stmt
            .query_map([], read_row)
            .map_err(|e| db_err("cannot read sessions", &e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| db_err("cannot read session", &e))?;
        drop(stmt);
        let mut good = Vec::with_capacity(parsed.len());
        for item in parsed {
            match item {
                Ok(row) => good.push(row),
                Err(bad_agent) => {
                    // One unreadable row must not hide the healthy ones: drop it and say so.
                    tracing::warn!(agent = %bad_agent, "dropping unreadable session row");
                    self.conn
                        .execute("DELETE FROM sessions WHERE agent = ?1", [&bad_agent])
                        .map_err(|e| db_err("cannot drop bad session", &e))?;
                }
            }
        }
        Ok(good)
    }

    fn save_session(&mut self, row: &SessionRow) -> Result<(), StoreError> {
        let st = &row.status;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO sessions (agent, token, window, state, quiet, quiet_since, \
                 from_screen, since, error, last_reply, task_at_launch, saw_end_hook, last_task) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                rusqlite::params![
                    row.agent.as_str(),
                    row.token.as_ref().map(|t| t.0.as_str()),
                    row.window.as_ref().map(|w| w.0.as_str()),
                    st.state.word(),
                    st.quiet,
                    st.quiet_since.map(|m| to_i64(m.0)),
                    st.from_screen,
                    to_i64(st.since.0),
                    st.error,
                    row.last_reply,
                    row.task_at_launch,
                    row.saw_end_hook,
                    row.last_task,
                ],
            )
            .map(drop)
            .map_err(|e| db_err("cannot save session", &e))
    }

    fn delete_session(&mut self, agent: &AgentName) -> Result<(), StoreError> {
        // A row that is not there is fine: the engine deletes on every reach of Stopped.
        self.conn
            .execute("DELETE FROM sessions WHERE agent = ?1", [agent.as_str()])
            .map(drop)
            .map_err(|e| db_err("cannot delete session", &e))
    }

    fn take_pending_hooks(&mut self) -> Result<Vec<HookMessage>, StoreError> {
        take_pending(&mut self.conn)
    }
}
