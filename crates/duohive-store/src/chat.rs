//! The `chat` table (TD §7): the Duo chat transcript, every entry kept.

use duohive_core::card::{CardId, CardOutcome};
use duohive_core::chat::{ChatEntry, ChatId, ChatKind};
use duohive_core::inbox::MessageId;
use duohive_core::ports::StoreError;
use duohive_core::time::Millis;
use rusqlite::{Connection, Row, params};

use crate::db::{db_err, to_i64};

const COLUMNS: &str = "id, at, kind, text, result, card, message";

/// A card outcome's prefix and suffix around its reason (P3-D12's words).
const NOT_APPLIED: (&str, &str) = ("approved, not applied (", ")");

fn id_of(v: i64) -> Option<u64> {
    u64::try_from(v).ok()
}

pub(crate) fn outcome_text(o: &CardOutcome) -> String {
    match o {
        CardOutcome::Approved => "approved".to_owned(),
        CardOutcome::EditedApproved => "edited and approved".to_owned(),
        CardOutcome::Rejected => "rejected".to_owned(),
        CardOutcome::NotApplied(reason) => format!("{}{reason}{}", NOT_APPLIED.0, NOT_APPLIED.1),
    }
}

pub(crate) fn outcome_from(text: &str) -> Option<CardOutcome> {
    match text {
        "approved" => Some(CardOutcome::Approved),
        "edited and approved" => Some(CardOutcome::EditedApproved),
        "rejected" => Some(CardOutcome::Rejected),
        _ => text
            .strip_prefix(NOT_APPLIED.0)?
            .strip_suffix(NOT_APPLIED.1)
            .map(|r| CardOutcome::NotApplied(r.to_owned())),
    }
}

/// An entry, or the id of a row that could not be read.
fn read(r: &Row<'_>) -> rusqlite::Result<Result<ChatEntry, i64>> {
    let id: i64 = r.get("id")?;
    let kind: String = r.get("kind")?;
    let result: Option<String> = r.get("result")?;
    let card: Option<i64> = r.get("card")?;
    let message: Option<i64> = r.get("message")?;
    let kind = match kind.as_str() {
        "you" => match message {
            None => Some(ChatKind::You { message: None }),
            Some(m) => id_of(m).map(|m| ChatKind::You {
                message: Some(MessageId(m)),
            }),
        },
        "duo" => Some(ChatKind::Duo),
        "action" => result.map(|result| ChatKind::Action { result }),
        "card" => card
            .and_then(id_of)
            .zip(result.as_deref().and_then(outcome_from))
            .map(|(card, outcome)| ChatKind::CardResult {
                card: CardId(card),
                outcome,
            }),
        _ => None,
    };
    let (Some(kind), Some(uid)) = (kind, id_of(id)) else {
        return Ok(Err(id));
    };
    Ok(Ok(ChatEntry {
        id: ChatId(uid),
        at: Millis(id_of(r.get("at")?).unwrap_or(0)),
        kind,
        text: r.get("text")?,
    }))
}

/// The last `last` entries, oldest first; unreadable rows are skipped with a log line.
pub(crate) fn load(conn: &Connection, last: usize) -> Result<Vec<ChatEntry>, StoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM (SELECT {COLUMNS} FROM chat ORDER BY id DESC LIMIT ?1) ORDER BY id"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| db_err("cannot read the chat", &e))?;
    let limit = i64::try_from(last).unwrap_or(i64::MAX);
    let rows = stmt
        .query_map([limit], read)
        .map_err(|e| db_err("cannot read the chat", &e))?;
    let mut entries = Vec::new();
    for row in rows {
        match row.map_err(|e| db_err("cannot read a chat entry", &e))? {
            Ok(entry) => entries.push(entry),
            Err(id) => tracing::warn!(id, "skipping unreadable chat row"),
        }
    }
    Ok(entries)
}

/// Appends an entry; a `Note` is never stored.
pub(crate) fn save(conn: &Connection, e: &ChatEntry) -> Result<(), StoreError> {
    let (kind, result, card, message) = match &e.kind {
        ChatKind::You { message } => ("you", None, None, message.map(|m| to_i64(m.0))),
        ChatKind::Duo => ("duo", None, None, None),
        ChatKind::Action { result } => ("action", Some(result.clone()), None, None),
        ChatKind::CardResult { card, outcome } => (
            "card",
            Some(outcome_text(outcome)),
            Some(to_i64(card.0)),
            None,
        ),
        ChatKind::Note => return Ok(()),
    };
    conn.execute(
        &format!("INSERT INTO chat ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"),
        params![
            to_i64(e.id.0),
            to_i64(e.at.0),
            kind,
            e.text,
            result,
            card,
            message
        ],
    )
    .map(drop)
    .map_err(|err| db_err("cannot save a chat entry", &err))
}
