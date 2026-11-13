//! Reading memories again for a new finding model: the vectors being built (`embeddings_next`)
//! and the progress row (`reindex_state`). The old index stays in use until the swap.

use jiff::Timestamp;
use nocturne_contract::ReindexPhase;

/// A re-read under way or just finished, one row.
#[derive(Debug, Clone, PartialEq)]
pub struct ReindexRow {
    pub model: String,
    pub from_model: String,
    pub dim: Option<u32>,
    pub total: u32,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
    pub phase: ReindexPhase,
}

use rusqlite::{Connection, OptionalExtension, params};

use crate::util::{enum_str, fmt_ts, parse_enum, parse_ts};
use crate::{Engine, Result};

fn load(conn: &Connection) -> Result<Option<ReindexRow>> {
    type Raw = (
        Option<String>,
        Option<String>,
        Option<i64>,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let raw: Option<Raw> = conn
        .query_row(
            "SELECT model, from_model, dim, total, started_at, finished_at, phase \
             FROM reindex_state WHERE id = 1",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .optional()?;
    let Some((Some(model), from, dim, total, started, finished, Some(phase))) = raw else {
        return Ok(None);
    };
    Ok(Some(ReindexRow {
        model,
        from_model: from.unwrap_or_default(),
        dim: dim.and_then(|d| u32::try_from(d).ok()),
        total: u32::try_from(total).unwrap_or(0),
        started_at: started.as_deref().map(parse_ts).transpose()?,
        finished_at: finished.as_deref().map(parse_ts).transpose()?,
        phase: parse_enum(&phase)?,
    }))
}

fn current_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE superseded_at IS NULL",
        [],
        |r| r.get(0),
    )?)
}

fn running_model(conn: &Connection) -> Result<Option<String>> {
    Ok(load(conn)?
        .filter(|r| r.phase == ReindexPhase::Running)
        .map(|r| r.model))
}

impl Engine {
    /// Starts reading every current memory again with `model`, or carries on with a re-read of
    /// the same model already under way (after a restart). A re-read for another model drops
    /// what that one built. The old vectors stay in use until [`Engine::finish_reindex`].
    pub fn begin_reindex(&self, model: &str, dim: Option<u32>) -> Result<ReindexRow> {
        let now = fmt_ts(self.clock().now());
        let from = self.recall_model();
        self.write(|tx| {
            if let Some(row) = load(tx)?.filter(|r| r.phase == ReindexPhase::Running)
                && row.model == model
            {
                return Ok(row);
            }
            tx.execute("DELETE FROM embeddings_next", [])?;
            let total = current_count(tx)?;
            tx.execute(
                "UPDATE reindex_state SET model = ?1, from_model = ?2, dim = ?3, total = ?4, \
                 started_at = ?5, finished_at = NULL, phase = ?6 WHERE id = 1",
                params![
                    model,
                    from,
                    dim.map(i64::from),
                    total,
                    now,
                    enum_str(&ReindexPhase::Running)
                ],
            )?;
            load(tx)?.ok_or_else(|| crate::Error::invalid("the re-read could not be recorded"))
        })
    }

    /// Current memories the running re-read has no vector for yet, newest first.
    pub fn reindex_todo(&self, limit: usize) -> Result<Vec<(String, String)>> {
        self.read(|conn| {
            if running_model(conn)?.is_none() {
                return Ok(Vec::new());
            }
            let mut stmt = conn.prepare(
                "SELECT m.id, m.sentence FROM memories m \
                 WHERE m.superseded_at IS NULL \
                   AND NOT EXISTS (SELECT 1 FROM embeddings_next n WHERE n.memory_id = m.id) \
                 ORDER BY m.recorded_at DESC, m.id LIMIT ?1",
            )?;
            let rows = stmt.query_map([limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    /// How many current memories the running re-read has a vector for, and how many there are.
    pub fn reindex_progress(&self) -> Result<(u32, u32)> {
        self.read(|conn| {
            let done: i64 = conn.query_row(
                "SELECT COUNT(*) FROM embeddings_next n JOIN memories m ON m.id = n.memory_id \
                 WHERE m.superseded_at IS NULL",
                [],
                |r| r.get(0),
            )?;
            let total = current_count(conn)?;
            Ok((
                u32::try_from(done).unwrap_or(0),
                u32::try_from(total).unwrap_or(0),
            ))
        })
    }

    /// Keeps vectors made with the running re-read's model. Unknown ids are ignored; nothing is
    /// kept when no re-read is running (it was cancelled meanwhile).
    pub fn store_reindexed(&self, rows: Vec<(String, Vec<f32>)>) -> Result<()> {
        for chunk in rows.chunks(200) {
            self.write(|tx| {
                let Some(model) = running_model(tx)? else {
                    return Ok(());
                };
                for (id, vec) in chunk {
                    let blob: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
                    tx.execute(
                        "INSERT OR REPLACE INTO embeddings_next(memory_id, model, dim, vec) \
                         SELECT id, ?2, ?3, ?4 FROM memories WHERE id = ?1",
                        params![id, model, vec.len() as i64, blob],
                    )?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Swaps the new vectors in, in one transaction, once every current memory has one and the
    /// old index has nothing waiting either (a memory re-worded meanwhile is read again first).
    /// The new model becomes the finding model in the same step, so no vector of the old one
    /// can be stored after the swap. False when that is not so yet, or no re-read is running.
    /// The caller then saves the choice.
    pub fn finish_reindex(&self) -> Result<bool> {
        let now = fmt_ts(self.clock().now());
        let mut recall = self
            .inner
            .recall_model
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let swapped = self.write(|tx| {
            let Some(model) = running_model(tx)? else {
                return Ok(None);
            };
            let waiting: i64 = tx.query_row(
                "SELECT COUNT(*) FROM memories m WHERE m.superseded_at IS NULL AND ( \
                   NOT EXISTS (SELECT 1 FROM embeddings_next n WHERE n.memory_id = m.id) \
                   OR NOT EXISTS (SELECT 1 FROM embeddings e \
                                  WHERE e.memory_id = m.id AND e.model = ?1))",
                [recall.as_str()],
                |r| r.get(0),
            )?;
            if waiting > 0 {
                return Ok(None);
            }
            tx.execute("DELETE FROM embeddings", [])?;
            tx.execute(
                "INSERT INTO embeddings(memory_id, dim, vec, model) \
                 SELECT memory_id, dim, vec, model FROM embeddings_next WHERE model = ?1",
                [&model],
            )?;
            tx.execute("DELETE FROM embeddings_next", [])?;
            tx.execute(
                "UPDATE reindex_state SET finished_at = ?1, phase = ?2 WHERE id = 1",
                params![now, enum_str(&ReindexPhase::Done)],
            )?;
            Ok(Some(model))
        })?;
        let Some(model) = swapped else {
            return Ok(false);
        };
        *recall = model;
        drop(recall);
        self.invalidate_vectors();
        Ok(true)
    }

    /// Stops the re-read and drops what it built. The old vectors were never touched.
    pub fn cancel_reindex(&self) -> Result<()> {
        let now = fmt_ts(self.clock().now());
        self.write(|tx| {
            if running_model(tx)?.is_none() {
                return Ok(());
            }
            tx.execute("DELETE FROM embeddings_next", [])?;
            tx.execute(
                "UPDATE reindex_state SET finished_at = ?1, phase = ?2 WHERE id = 1",
                params![now, enum_str(&ReindexPhase::Cancelled)],
            )?;
            Ok(())
        })
    }

    /// The latest re-read, running or ended, if there ever was one.
    pub fn reindex_state(&self) -> Result<Option<ReindexRow>> {
        self.read(load)
    }

    /// The finding model the vectors in use were made with, when they all agree.
    pub fn indexed_model(&self) -> Result<Option<String>> {
        self.read(|conn| {
            let mut stmt = conn.prepare("SELECT DISTINCT model FROM embeddings LIMIT 2")?;
            let models: Vec<String> = stmt
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(match models.as_slice() {
                [one] => Some(one.clone()),
                _ => None,
            })
        })
    }
}
