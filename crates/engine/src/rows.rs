//! Row to view conversions shared by every module.

use nocturne_contract::{Certainty, ItemView, MemoryStatus, MemoryView, Receipt, Span, Trust};
use rusqlite::{Connection, Row};
use std::path::Path;

use crate::capture::original_path;
use crate::error::Result;
use crate::util::parse_enum;

pub(crate) fn de<T: serde::de::DeserializeOwned>(s: String) -> rusqlite::Result<T> {
    parse_enum(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::other(e.to_string())),
        )
    })
}

fn de_opt<T: serde::de::DeserializeOwned>(s: Option<String>) -> rusqlite::Result<Option<T>> {
    s.map(de).transpose()
}

/// The item columns every loader selects, with the table aliased `i`. The memory count includes
/// memories that a later item replaced (history still came from this item), but not a version
/// this same item replaced, so a correction counts once.
pub(crate) const ITEM_COLS: &str = "i.id, i.kind, i.source, i.trust, i.title, i.kept_at, i.status, \
    i.reason, i.duration_ms, i.pages, i.file_name, i.private, i.understood_in_ms, i.summary, \
    i.text, i.language, \
    (SELECT COUNT(*) FROM memory_sources s JOIN memories m ON m.id = s.memory_id \
     WHERE s.item_id = i.id AND (m.superseded_at IS NULL OR NOT EXISTS ( \
       SELECT 1 FROM memory_sources n WHERE n.memory_id = m.superseded_by AND n.item_id = i.id))), \
    i.sha256, i.ext";

/// Builds an `ItemView` from an `ITEM_COLS` row. `originals` is the directory the stored files
/// live in; the view names the original only while that file exists.
pub(crate) fn item_view(originals: &Path) -> impl Fn(&Row<'_>) -> rusqlite::Result<ItemView> + '_ {
    move |row| {
        let mut view = item_view_row(row)?;
        if view.has_original {
            let sha: String = row.get(17)?;
            let ext: Option<String> = row.get(18)?;
            let path = original_path(originals, &sha, ext.as_deref());
            if path.is_file() {
                view.original_path = Some(path.to_string_lossy().into_owned());
            }
        }
        Ok(view)
    }
}

fn item_view_row(row: &Row<'_>) -> rusqlite::Result<ItemView> {
    Ok(ItemView {
        id: row.get(0)?,
        kind: de(row.get(1)?)?,
        source: de(row.get(2)?)?,
        trust: de(row.get(3)?)?,
        title: row.get(4)?,
        kept_at: row.get(5)?,
        status: de(row.get(6)?)?,
        reason: de_opt(row.get(7)?)?,
        duration_ms: row.get(8)?,
        pages: row.get(9)?,
        has_original: row.get::<_, Option<String>>(10)?.is_some(),
        file_name: row.get(10)?,
        original_path: None,
        private: row.get(11)?,
        understood_in_ms: row.get(12)?,
        summary: row.get(13)?,
        text: row.get(14)?,
        language: row.get(15)?,
        memory_count: row.get(16)?,
    })
}

pub(crate) fn get_item_view(
    conn: &Connection,
    originals: &Path,
    id: &str,
) -> Result<Option<ItemView>> {
    let sql = format!("SELECT {ITEM_COLS} FROM items i WHERE i.id = ?1");
    match conn.query_row(&sql, [id], item_view(originals)) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn receipts_for(conn: &Connection, memory_id: &str) -> Result<Vec<Receipt>> {
    let mut stmt = conn.prepare_cached(
        "SELECT i.id, i.kind, i.source, i.trust, i.title, i.kept_at, i.duration_ms, s.span \
         FROM memory_sources s JOIN items i ON i.id = s.item_id \
         WHERE s.memory_id = ?1 ORDER BY i.kept_at, i.id",
    )?;
    let rows = stmt.query_map([memory_id], |r| {
        let trust: Trust = de(r.get(3)?)?;
        let span: Option<String> = r.get(7)?;
        let span: Option<Span> = span.and_then(|s| serde_json::from_str(&s).ok());
        Ok(Receipt {
            item_id: r.get(0)?,
            kind: de(r.get(1)?)?,
            source: de(r.get(2)?)?,
            label: r.get(4)?,
            kept_at: r.get(5)?,
            duration_ms: r.get(6)?,
            span,
            unconfirmed: trust == Trust::Untrusted,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The memory columns every loader selects, with the table aliased `m`.
pub(crate) const MEM_COLS: &str = "m.id, m.subject, m.subject_key, m.entity_id, m.predicate, \
    m.object, m.object_key, m.sentence, m.valid_from, m.valid_to, m.due, m.recorded_at, \
    m.superseded_at, m.superseded_by, m.confidence, m.status, m.confirmed, m.last_used, \
    m.let_go_at, m.done, m.keep_forever, m.private, m.suggest_private, m.tags, \
    (SELECT o.object FROM memories o WHERE o.superseded_by = m.id \
       AND o.forgetting = 0 ORDER BY o.recorded_at DESC, o.id LIMIT 1), \
    (SELECT COUNT(*) FROM memory_sources s WHERE s.memory_id = m.id), \
    (SELECT COUNT(*) FROM memory_sources s JOIN items i ON i.id = s.item_id \
       WHERE s.memory_id = m.id AND i.trust = 'trusted')";

/// A memory row as stored, plus its previous value and source counts.
#[derive(Debug, Clone)]
pub(crate) struct MemRow {
    pub id: String,
    pub subject: String,
    pub subject_key: String,
    pub entity_id: Option<String>,
    pub predicate: String,
    pub object: String,
    pub object_key: String,
    pub sentence: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub due: Option<String>,
    pub recorded_at: String,
    pub superseded_at: Option<String>,
    pub superseded_by: Option<String>,
    pub confidence: f64,
    pub status: MemoryStatus,
    pub confirmed: bool,
    pub last_used: Option<String>,
    pub let_go_at: Option<String>,
    pub done: bool,
    pub keep_forever: bool,
    pub private: bool,
    pub suggest_private: bool,
    pub tags: Vec<String>,
    pub was: Option<String>,
    pub sources: u32,
    pub trusted_sources: u32,
}

pub(crate) fn mem_row(r: &Row<'_>) -> rusqlite::Result<MemRow> {
    let tags: String = r.get(23)?;
    Ok(MemRow {
        id: r.get(0)?,
        subject: r.get(1)?,
        subject_key: r.get(2)?,
        entity_id: r.get(3)?,
        predicate: r.get(4)?,
        object: r.get(5)?,
        object_key: r.get(6)?,
        sentence: r.get(7)?,
        valid_from: r.get(8)?,
        valid_to: r.get(9)?,
        due: r.get(10)?,
        recorded_at: r.get(11)?,
        superseded_at: r.get(12)?,
        superseded_by: r.get(13)?,
        confidence: r.get(14)?,
        status: de(r.get(15)?)?,
        confirmed: r.get(16)?,
        last_used: r.get(17)?,
        let_go_at: r.get(18)?,
        done: r.get(19)?,
        keep_forever: r.get(20)?,
        private: r.get(21)?,
        suggest_private: r.get(22)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        was: r.get(24)?,
        sources: r.get(25)?,
        trusted_sources: r.get(26)?,
    })
}

pub(crate) fn load_mem(conn: &Connection, id: &str) -> Result<Option<MemRow>> {
    let sql = format!("SELECT {MEM_COLS} FROM memories m WHERE m.id = ?1 AND m.forgetting = 0");
    match conn.query_row(&sql, [id], mem_row) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

impl MemRow {
    pub fn certainty(&self) -> Certainty {
        if self.confirmed {
            Certainty::Sure
        } else if self.status == MemoryStatus::LetGo
            || self.status == MemoryStatus::Unconfirmed
            || self.confidence < 0.5
        {
            Certainty::NotSure
        } else if self.trusted_sources >= 2 {
            Certainty::Sure
        } else {
            Certainty::FairlySure
        }
    }

    pub fn is_safety(&self) -> bool {
        self.predicate == "takes"
            || self.tags.iter().any(|t| {
                matches!(
                    t.to_lowercase().as_str(),
                    "health" | "allergy" | "medication"
                )
            })
    }

    pub fn view(&self, receipts: Vec<Receipt>) -> MemoryView {
        MemoryView {
            id: self.id.clone(),
            sentence: self.sentence.clone(),
            subject: self.subject.clone(),
            entity_id: self.entity_id.clone(),
            predicate: self.predicate.clone(),
            object: self.object.clone(),
            certainty: self.certainty(),
            status: self.status,
            valid_from: self.valid_from.clone(),
            valid_to: self.valid_to.clone(),
            due: self.due.clone(),
            learned_at: self.recorded_at.clone(),
            last_used: self.last_used.clone(),
            let_go_at: self.let_go_at.clone(),
            keep_forever: self.keep_forever,
            private: self.private,
            suggest_private: self.suggest_private,
            was: self.was.clone(),
            receipts,
        }
    }
}

pub(crate) fn mem_view(conn: &Connection, row: &MemRow) -> Result<MemoryView> {
    Ok(row.view(receipts_for(conn, &row.id)?))
}

/// Load the current memories matching a WHERE clause (parameters bound by the caller).
pub(crate) fn query_mems(
    conn: &Connection,
    where_sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<MemRow>> {
    let sql = format!("SELECT {MEM_COLS} FROM memories m WHERE {where_sql}");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params, mem_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}
