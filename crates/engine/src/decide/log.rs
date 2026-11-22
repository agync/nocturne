//! The decision log: one row per thing a rule decided, with what a decider said once it was
//! asked. Rows are derived from items and memories and cascade with them, so forgetting removes
//! them; Delete everything empties the table. Rows older than [`KEEP_DAYS`] are dropped.

use jiff::civil::Date;
use jiff::{Timestamp, ToSpan};
use rusqlite::{Connection, Transaction, params};

use super::{Kind, Logged, Pending, Settled, verdict};
use crate::Engine;
use crate::error::Result;
use crate::journal::Ctx;
use crate::journal::question::blocked_by_later;
use crate::util::{fmt_ts, new_id};

/// How long a row is kept, asked or not.
pub(crate) const KEEP_DAYS: i64 = 30;

/// Logs what the move rule said about a new appointment (`memory_id`, read from `item_id`) and
/// a kept one in its slot (`other_id`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_moved(
    tx: &Transaction<'_>,
    item_id: &str,
    memory_id: &str,
    other_id: &str,
    kept: &str,
    note: &str,
    rule: bool,
    detail: &str,
    now: &str,
) -> Result<()> {
    let state = format!(
        "Kept appointment: {}\nNew note: {}",
        kept.trim(),
        note.trim()
    );
    prune(tx, now)?;
    insert(
        tx,
        Row {
            kind: Kind::Moved,
            item_id: Some(item_id),
            memory_id: Some(memory_id),
            other_id: Some(other_id),
            state: &state,
            rule,
            detail: Some(detail),
            copied_from: &[item_id],
        },
        now,
    )?;
    Ok(())
}

/// Drops rows past [`KEEP_DAYS`], and rows of a kind no longer asked (the health, money and ID
/// check of earlier versions). Run wherever rows are written, so the log stays bounded on a
/// computer that never has a night's tidy.
fn prune(conn: &Connection, now: &str) -> Result<()> {
    let known: Vec<String> = Kind::ALL
        .iter()
        .map(|k| format!("'{}'", k.as_str()))
        .collect();
    conn.execute(
        &format!(
            "DELETE FROM decisions WHERE kind NOT IN ({})",
            known.join(", ")
        ),
        [],
    )?;
    let Ok(now) = now.parse::<Timestamp>() else {
        return Ok(());
    };
    let oldest = fmt_ts(now - (KEEP_DAYS * 24).hours());
    conn.execute("DELETE FROM decisions WHERE created_at < ?1", [&oldest])?;
    Ok(())
}

/// A row to log. `copied_from` names every item whose words `state` holds: when one of them goes,
/// so does the row.
struct Row<'a> {
    kind: Kind,
    item_id: Option<&'a str>,
    memory_id: Option<&'a str>,
    other_id: Option<&'a str>,
    state: &'a str,
    rule: bool,
    detail: Option<&'a str>,
    copied_from: &'a [&'a str],
}

fn insert(conn: &Connection, row: Row<'_>, now: &str) -> Result<usize> {
    let id = new_id("dec");
    let added = conn.execute(
        "INSERT OR IGNORE INTO decisions(id, kind, item_id, memory_id, other_id, state, rule, \
         rule_detail, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            row.kind.as_str(),
            row.item_id,
            row.memory_id,
            row.other_id,
            row.state,
            row.rule,
            row.detail,
            now
        ],
    )?;
    if added > 0 {
        for item in row.copied_from {
            conn.execute(
                "INSERT OR IGNORE INTO decision_items(decision_id, item_id) VALUES (?1, ?2)",
                params![id, item],
            )?;
        }
    }
    Ok(added)
}

/// Queues "already wrote how it went" for each memory rule 5 kept off the page dated
/// `page_day`.
fn queue_followed_up(tx: &Transaction<'_>, ctx: &Ctx, page_day: Date, now: &str) -> Result<usize> {
    let mut queued = 0;
    for b in blocked_by_later(tx, ctx, page_day)? {
        if b.later.is_empty() {
            continue;
        }
        let texts: Vec<&str> = b.later.iter().map(|(_, text)| text.as_str()).collect();
        let state = format!(
            "Planned, in the person's words: {}\nWritten later: {}",
            b.sentence.trim(),
            texts.join("\n")
        );
        let mut copied: Vec<&str> = vec![b.block_id.as_str()];
        copied.extend(b.later.iter().map(|(item, _)| item.as_str()));
        queued += insert(
            tx,
            Row {
                kind: Kind::FollowedUp,
                item_id: Some(&b.block_id),
                memory_id: Some(&b.memory_id),
                other_id: None,
                state: &state,
                rule: true,
                detail: None,
                copied_from: &copied,
            },
            now,
        )?;
    }
    Ok(queued)
}

/// One row, or none for a kind this build does not know.
fn parse_logged(r: &rusqlite::Row<'_>) -> rusqlite::Result<Option<Logged>> {
    let kind: String = r.get(1)?;
    let Some(kind) = Kind::parse(&kind) else {
        return Ok(None);
    };
    Ok(Some(Logged {
        id: r.get(0)?,
        kind,
        rule: r.get(2)?,
        rule_detail: r.get(3)?,
        created_at: r.get(4)?,
        asked_at: r.get(5)?,
        decider: r.get(6)?,
        label: r.get(7)?,
        p: r.get(8)?,
        verdict: r.get(9)?,
        ms: r
            .get::<_, Option<i64>>(10)?
            .and_then(|v| u64::try_from(v).ok()),
        error: r.get(11)?,
        item_id: r.get(12)?,
        memory_id: r.get(13)?,
    }))
}

impl Engine {
    /// Queues the decisions the night's tidy asks about in shadow: rule 5 of the page question
    /// for the page dated `page_day`. Rows past their time are dropped first. Returns how many
    /// were queued.
    pub fn queue_decisions(&self, page_day: Date) -> Result<usize> {
        let stamp = fmt_ts(self.clock_now());
        let clock = self.inner.clock.clone();
        self.write(|tx| {
            prune(tx, &stamp)?;
            let ctx = Ctx::load(tx, clock.as_ref())?;
            queue_followed_up(tx, &ctx, page_day, &stamp)
        })
    }

    /// Up to `limit` logged decisions no decider has been asked about, oldest first.
    pub fn pending_decisions(&self, limit: usize) -> Result<Vec<Pending>> {
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, kind, state, rule, item_id, memory_id FROM decisions \
                 WHERE asked_at IS NULL ORDER BY created_at, id LIMIT ?1",
            )?;
            let rows = stmt.query_map([i64::try_from(limit).unwrap_or(i64::MAX)], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (id, kind, state, rule, item_id, memory_id) = row?;
                let Some(kind) = Kind::parse(&kind) else {
                    continue;
                };
                out.push(Pending {
                    id,
                    kind,
                    item_id,
                    memory_id,
                    decision: super::Decision {
                        state,
                        question: kind.question(),
                    },
                    rule,
                });
            }
            Ok(out)
        })
    }

    /// Records what came of asking about decision `id`. The rule's outcome is never changed.
    pub fn settle_decision(&self, id: &str, settled: &Settled) -> Result<()> {
        let now = fmt_ts(self.clock_now());
        self.write(|tx| {
            let kind: Option<String> = rusqlite::OptionalExtension::optional(tx.query_row(
                "SELECT kind FROM decisions WHERE id = ?1",
                [id],
                |r| r.get(0),
            ))?;
            let Some(kind) = kind.as_deref().and_then(Kind::parse) else {
                return Ok(());
            };
            let ms = i64::try_from(settled.ms).unwrap_or(i64::MAX);
            match &settled.answer {
                Ok(answer) => {
                    let decided = verdict(kind, settled.backend, answer);
                    tx.execute(
                        "UPDATE decisions SET asked_at = ?2, decider = ?3, label = ?4, \
                         probabilities = ?5, p = ?6, verdict = ?7, ms = ?8, error = NULL \
                         WHERE id = ?1",
                        params![
                            id,
                            now,
                            format!("{}:{}", settled.backend.as_str(), answer.by),
                            answer.label,
                            serde_json::to_string(&answer.probabilities)?,
                            answer.p(&answer.label),
                            decided,
                            ms
                        ],
                    )?;
                }
                Err(why) => {
                    tx.execute(
                        "UPDATE decisions SET asked_at = ?2, decider = ?3, verdict = NULL, \
                         ms = ?4, error = ?5 WHERE id = ?1",
                        params![id, now, settled.backend.as_str(), ms, why],
                    )?;
                }
            }
            Ok(())
        })
    }

    /// Every row of the decision log, oldest first.
    pub fn decision_log(&self) -> Result<Vec<Logged>> {
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, kind, rule, rule_detail, created_at, asked_at, decider, label, p, \
                 verdict, ms, error, item_id, memory_id FROM decisions ORDER BY created_at, id",
            )?;
            let rows = stmt.query_map([], parse_logged)?;
            let rows: Vec<Option<Logged>> = rows.collect::<rusqlite::Result<_>>()?;
            Ok(rows.into_iter().flatten().collect())
        })
    }
}
