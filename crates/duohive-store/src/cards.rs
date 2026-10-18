//! The `cards` table (TD §7): approval cards, pending and answered.

use duohive_core::card::{Card, CardChange, CardId, CardSource};
use duohive_core::ports::StoreError;
use duohive_core::time::Millis;
use rusqlite::{Connection, Row, params};

use crate::chat::{outcome_from, outcome_text};
use crate::db::{db_err, to_i64};

const COLUMNS: &str = "id, source, change, raised_at, outcome, answered_at";

fn source_text(s: CardSource) -> &'static str {
    match s {
        CardSource::Duo => "duo",
        CardSource::NoFreeAgent => "no_free_agent",
    }
}

fn source_from(text: &str) -> Option<CardSource> {
    match text {
        "duo" => Some(CardSource::Duo),
        "no_free_agent" => Some(CardSource::NoFreeAgent),
        _ => None,
    }
}

fn millis(v: i64) -> Millis {
    Millis(u64::try_from(v).unwrap_or(0))
}

/// A card, or the id of a row that could not be read.
fn read(r: &Row<'_>) -> rusqlite::Result<Result<Card, i64>> {
    let id: i64 = r.get("id")?;
    let source: String = r.get("source")?;
    let change: String = r.get("change")?;
    let outcome: Option<String> = r.get("outcome")?;
    let answered_at: Option<i64> = r.get("answered_at")?;
    let answer = match outcome {
        None => Some(None),
        Some(text) => outcome_from(&text).map(|o| Some((o, millis(answered_at.unwrap_or(0))))),
    };
    let parts = (
        u64::try_from(id).ok(),
        source_from(&source),
        serde_json::from_str::<CardChange>(&change).ok(),
        answer,
    );
    let (Some(uid), Some(source), Some(change), Some(answer)) = parts else {
        return Ok(Err(id));
    };
    Ok(Ok(Card {
        id: CardId(uid),
        source,
        change,
        raised_at: millis(r.get("raised_at")?),
        answer,
    }))
}

/// The pending cards, and the newest one answered or not so ids continue,
/// oldest first; unreadable rows are skipped with a log line.
pub(crate) fn load(conn: &Connection) -> Result<Vec<Card>, StoreError> {
    let sql = format!(
        "SELECT {COLUMNS} FROM cards WHERE outcome IS NULL OR id = (SELECT MAX(id) FROM cards) ORDER BY id"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| db_err("cannot read the cards", &e))?;
    let rows = stmt
        .query_map([], read)
        .map_err(|e| db_err("cannot read the cards", &e))?;
    let mut cards = Vec::new();
    for row in rows {
        match row.map_err(|e| db_err("cannot read a card", &e))? {
            Ok(card) => cards.push(card),
            Err(id) => tracing::warn!(id, "skipping unreadable card row"),
        }
    }
    Ok(cards)
}

/// Inserts or replaces a card, keyed by id.
pub(crate) fn save(conn: &Connection, card: &Card) -> Result<(), StoreError> {
    let change =
        serde_json::to_string(&card.change).map_err(|e| db_err("cannot encode a card", &e))?;
    let (outcome, answered_at) = match &card.answer {
        Some((o, at)) => (Some(outcome_text(o)), Some(to_i64(at.0))),
        None => (None, None),
    };
    conn.execute(
        &format!("INSERT OR REPLACE INTO cards ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"),
        params![
            to_i64(card.id.0),
            source_text(card.source),
            change,
            to_i64(card.raised_at.0),
            outcome,
            answered_at
        ],
    )
    .map(drop)
    .map_err(|err| db_err("cannot save a card", &err))
}
