//! The `messages` table (TD §7): the inbox's queued, pasting, delivered and recent
//! messages. `status` is free text, so `pasting` needed no migration.

use duohive_core::card::CardId;
use duohive_core::inbox::{Message, MessageId, MessageKind, MessageStatus, Sender};
use duohive_core::names::AgentName;
use duohive_core::ports::StoreError;
use duohive_core::time::Millis;
use rusqlite::{Connection, Row, params};

use crate::db::{db_err, to_i64};

const COLUMNS: &str =
    "id, sender, recipient, kind, card, text, status, reason, queued_at, delivered_at";

fn millis(v: i64) -> Millis {
    Millis(u64::try_from(v).unwrap_or(0))
}

/// A message, or the id of a row that could not be read.
fn read(r: &Row<'_>) -> rusqlite::Result<Result<Message, i64>> {
    let id: i64 = r.get("id")?;
    let sender: Option<String> = r.get("sender")?;
    let recipient: String = r.get("recipient")?;
    let kind: String = r.get("kind")?;
    let card: Option<i64> = r.get("card")?;
    let status: String = r.get("status")?;
    let reason: Option<String> = r.get("reason")?;
    let from = match sender {
        None => Some(Sender::You),
        Some(s) => AgentName::parse(&s).ok().map(Sender::Agent),
    };
    let kind = match (kind.as_str(), card) {
        ("text", _) => Some(MessageKind::Text),
        ("card", Some(c)) => u64::try_from(c)
            .ok()
            .map(|c| MessageKind::CardAnswer(CardId(c))),
        _ => None,
    };
    let status = match status.as_str() {
        "queued" => Some(MessageStatus::Queued),
        "pasting" => Some(MessageStatus::Pasting),
        "delivered" => Some(MessageStatus::Delivered),
        "answered" => Some(MessageStatus::Answered),
        "dropped" => Some(MessageStatus::Dropped {
            reason: reason.unwrap_or_default(),
        }),
        _ => None,
    };
    let (Some(from), Ok(to), Some(kind), Some(status), Ok(uid)) = (
        from,
        AgentName::parse(&recipient),
        kind,
        status,
        u64::try_from(id),
    ) else {
        return Ok(Err(id));
    };
    Ok(Ok(Message {
        id: MessageId(uid),
        from,
        to,
        kind,
        text: r.get("text")?,
        status,
        queued_at: millis(r.get("queued_at")?),
        delivered_at: r.get::<_, Option<i64>>("delivered_at")?.map(millis),
    }))
}

/// The rows `filter` selects, by id; unreadable rows are skipped with a log line.
fn select(
    conn: &Connection,
    filter: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<Message>, StoreError> {
    let sql = format!("SELECT {COLUMNS} FROM messages WHERE {filter} ORDER BY id");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| db_err("cannot read messages", &e))?;
    let rows = stmt
        .query_map(params, read)
        .map_err(|e| db_err("cannot read messages", &e))?;
    let mut messages = Vec::new();
    for row in rows {
        match row.map_err(|e| db_err("cannot read message", &e))? {
            Ok(m) => messages.push(m),
            Err(id) => tracing::warn!(id, "skipping unreadable message row"),
        }
    }
    Ok(messages)
}

/// Queued, pasting and delivered messages, those queued at `since` or later, and the newest.
pub(crate) fn load(conn: &Connection, since: Millis) -> Result<Vec<Message>, StoreError> {
    select(
        conn,
        "status IN ('queued', 'pasting', 'delivered') OR queued_at >= ?1 \
         OR id = (SELECT MAX(id) FROM messages)",
        [to_i64(since.0)],
    )
}

/// One message by id.
pub(crate) fn find(conn: &Connection, id: MessageId) -> Result<Option<Message>, StoreError> {
    Ok(select(conn, "id = ?1", [to_i64(id.0)])?.pop())
}

/// Inserts or replaces a message.
pub(crate) fn save(conn: &Connection, m: &Message) -> Result<(), StoreError> {
    let sender = match &m.from {
        Sender::You => None,
        Sender::Agent(a) => Some(a.as_str()),
    };
    let (kind, card) = match m.kind {
        MessageKind::Text => ("text", None),
        MessageKind::CardAnswer(c) => ("card", Some(to_i64(c.0))),
    };
    let (status, reason) = match &m.status {
        MessageStatus::Queued => ("queued", None),
        MessageStatus::Pasting => ("pasting", None),
        MessageStatus::Delivered => ("delivered", None),
        MessageStatus::Answered => ("answered", None),
        MessageStatus::Dropped { reason } => ("dropped", Some(reason.as_str())),
    };
    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO messages ({COLUMNS}) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
        ),
        params![
            to_i64(m.id.0),
            sender,
            m.to.as_str(),
            kind,
            card,
            m.text,
            status,
            reason,
            to_i64(m.queued_at.0),
            m.delivered_at.map(|t| to_i64(t.0)),
        ],
    )
    .map(drop)
    .map_err(|e| db_err("cannot save message", &e))
}
