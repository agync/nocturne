//! The forgetting policy: use, decay, safety exemptions, keep forever and restore.

use std::path::PathBuf;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use nocturne_contract::MemoryStatus;
use rusqlite::{OptionalExtension, params};

use crate::Engine;
use crate::audio::{drop_audio_rows, pages_citing, shred_all};
use crate::error::{Error, Result};
use crate::memory::chain;
use crate::rows::{MemRow, query_mems};
use crate::settings::{read_settings, thresholds};
use crate::types::{DecayReport, UseKind};
use crate::util::{enum_str, fmt_ts, parse_date, parse_ts};

/// `effective_last_use = max(last_used, recorded_at, date_passed)`. Returns `None` when the
/// memory is dated in the future (or today) and so must not decay yet.
pub(crate) fn effective_last_use(
    m: &MemRow,
    today: Date,
    tz: &TimeZone,
) -> Result<Option<Timestamp>> {
    let mut eff = parse_ts(&m.recorded_at)?;
    if let Some(lu) = &m.last_used {
        eff = eff.max(parse_ts(lu)?);
    }
    if matches!(m.predicate.as_str(), "scheduled" | "promised" | "due")
        && let Some(d) = m.due.as_deref().or(m.valid_from.as_deref())
    {
        let d = parse_date(d)?;
        if d >= today {
            return Ok(None);
        }
        let passed = d
            .tomorrow()?
            .at(0, 0, 0, 0)
            .to_zoned(tz.clone())?
            .timestamp();
        eff = eff.max(passed);
    }
    Ok(Some(eff))
}

impl Engine {
    /// Apply the policy: unused for 30 days fades, for 90 is let go (45/120 gentle, 14/45 brisk).
    /// Idempotent. Commits in chunks of at most 200 rows.
    pub fn decay(&self) -> Result<DecayReport> {
        let now = self.clock().now();
        let tz = self.clock().tz();
        let today = now.to_zoned(tz.clone()).date();
        let (fade_days, let_go_days) = thresholds(self.read(read_settings)?.letting_go);
        let rows = self.read(|conn| {
            query_mems(
                conn,
                "m.superseded_at IS NULL AND m.keep_forever = 0 AND m.status IN ('active', 'fading')",
                [],
            )
        })?;
        // (id, status seen, last_used seen, status to move to)
        let mut changes: Vec<(String, MemoryStatus, Option<String>, MemoryStatus)> = Vec::new();
        for m in &rows {
            let Some(eff) = effective_last_use(m, today, &tz)? else {
                continue;
            };
            let idle_days = now.duration_since(eff).as_secs().div_euclid(86_400);
            let target = if idle_days >= let_go_days {
                MemoryStatus::LetGo
            } else if idle_days >= fade_days {
                MemoryStatus::Fading
            } else {
                MemoryStatus::Active
            };
            match (m.status, target) {
                (MemoryStatus::Active, MemoryStatus::Fading)
                | (MemoryStatus::Active | MemoryStatus::Fading, MemoryStatus::LetGo) => {
                    changes.push((m.id.clone(), m.status, m.last_used.clone(), target));
                }
                _ => {}
            }
        }
        let stamp = fmt_ts(now);
        let mut report = DecayReport::default();
        for chunk in changes.chunks(200) {
            self.write(|tx| {
                for (id, seen_status, seen_used, status) in chunk {
                    // Only if nothing happened since it was read: a memory used, restored,
                    // made keep-forever, superseded or forgotten in between is left alone.
                    let n = tx.execute(
                        "UPDATE memories SET status = ?2, \
                         let_go_at = CASE WHEN ?2 = 'letGo' THEN ?3 ELSE let_go_at END \
                         WHERE id = ?1 AND status = ?4 AND last_used IS ?5 AND keep_forever = 0 \
                           AND superseded_at IS NULL AND forgetting = 0",
                        params![
                            id,
                            enum_str(status),
                            stamp,
                            enum_str(seen_status),
                            seen_used
                        ],
                    )?;
                    if n > 0 {
                        match status {
                            MemoryStatus::LetGo => report.let_go += 1,
                            _ => report.faded += 1,
                        }
                    }
                }
                Ok(())
            })?;
        }
        Ok(report)
    }

    /// Record that memories were used. Use moves fading and let-go memories back to active.
    /// Showing a memory in the browser is not use and never calls this.
    pub fn mark_used(&self, ids: &[String], _why: UseKind) -> Result<()> {
        let now = fmt_ts(self.clock().now());
        for chunk in ids.chunks(200) {
            self.write(|tx| {
                for id in chunk {
                    mark_used_tx(tx, id, &now)?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Bring fading and let-go memories back. Other ids are left alone.
    pub fn restore(&self, ids: &[String]) -> Result<()> {
        self.mark_used_where(ids, "status IN ('fading', 'letGo')")
    }

    fn mark_used_where(&self, ids: &[String], cond: &str) -> Result<()> {
        let now = fmt_ts(self.clock().now());
        for chunk in ids.chunks(200) {
            self.write(|tx| {
                for id in chunk {
                    tx.execute(
                        &format!(
                            "UPDATE memories SET status = 'active', last_used = ?2, let_go_at = NULL \
                             WHERE id = ?1 AND {cond}"
                        ),
                        params![id, now],
                    )?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }

    pub fn set_keep_forever(&self, id: &str, on: bool) -> Result<()> {
        self.write(|tx| set_keep_forever_tx(tx, id, on))
    }

    /// Mark a memory private (or not). Cached audio of pages citing it is removed, so a newly
    /// private memory cannot be played back.
    pub fn set_private(&self, id: &str, on: bool) -> Result<()> {
        let files = self.write(|tx| set_private_tx(tx, id, on))?;
        self.bump_audio();
        shred_all(files);
        Ok(())
    }

    /// Note how a thing was kept on a phone ("typed", "taken", ...), for its page's Kept row. It
    /// travels with the thing in a backup or a move.
    pub fn set_kept_how(&self, item_id: &str, how: &str) -> Result<()> {
        self.write(|tx| {
            item_exists(tx, item_id)?;
            tx.execute(
                "UPDATE items SET metadata = json_set(metadata, '$.kept_how', ?2) WHERE id = ?1",
                params![item_id, how],
            )?;
            Ok(())
        })
    }

    /// How a thing was kept, when a phone noted it.
    pub fn kept_how(&self, item_id: &str) -> Result<Option<String>> {
        self.read(|conn| {
            conn.query_row(
                "SELECT json_extract(metadata, '$.kept_how') FROM items WHERE id = ?1",
                [item_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .ok_or_else(|| Error::not_found("that item"))
        })
    }

    /// Mark an item private (or not). Turning it on makes every memory linked to the item
    /// private, history and earlier versions included, and removes the cached audio of pages
    /// citing any of them. Turning it off clears the flag only on memories this item alone
    /// supports: another source, or another version, may still call for privacy.
    pub fn set_item_private(&self, item_id: &str, on: bool) -> Result<()> {
        let files = self.write(|tx| {
            item_exists(tx, item_id)?;
            let journal: bool = tx.query_row(
                "SELECT json_extract(metadata, '$.journal') IS NOT NULL FROM items WHERE id = ?1",
                [item_id],
                |r| r.get(0),
            )?;
            if journal {
                return Err(Error::invalid("a journal item cannot be marked private"));
            }
            let kind: String =
                tx.query_row("SELECT kind FROM items WHERE id = ?1", [item_id], |r| {
                    r.get(0)
                })?;
            if kind == "reading" {
                return Err(Error::invalid("a health reading cannot be marked private"));
            }
            tx.execute(
                "UPDATE items SET private = ?2 WHERE id = ?1",
                params![item_id, on],
            )?;
            let mut files = Vec::new();
            if on {
                for m in memories_of(tx, item_id, false)? {
                    files.extend(set_private_tx(tx, &m, true)?);
                }
            } else {
                for m in memories_of(tx, item_id, true)? {
                    tx.execute("UPDATE memories SET private = 0 WHERE id = ?1", [&m])?;
                }
            }
            Ok(files)
        })?;
        self.bump_audio();
        shred_all(files);
        Ok(())
    }

    /// Keep (or stop keeping) the memories that came from an item forever. Turning it on covers
    /// every memory linked to the item and its earlier versions; turning it off only those this
    /// item alone supports.
    pub fn set_item_keep_forever(&self, item_id: &str, on: bool) -> Result<()> {
        self.write(|tx| {
            item_exists(tx, item_id)?;
            if on {
                for m in memories_of(tx, item_id, false)? {
                    for v in chain(tx, &m)? {
                        set_keep_forever_tx(tx, &v, true)?;
                    }
                }
            } else {
                for m in memories_of(tx, item_id, true)? {
                    set_keep_forever_tx(tx, &m, false)?;
                }
            }
            Ok(())
        })
    }
}

fn item_exists(tx: &rusqlite::Transaction<'_>, item_id: &str) -> Result<()> {
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)",
        [item_id],
        |r| r.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(Error::not_found(format!("item {item_id}")))
    }
}

/// Every memory linked to this item, current or history (not those already being forgotten).
/// With `only_this_item`, just those whose one source is this item.
fn memories_of(
    tx: &rusqlite::Transaction<'_>,
    item_id: &str,
    only_this_item: bool,
) -> Result<Vec<String>> {
    let mut stmt = tx.prepare(
        "SELECT m.id FROM memories m JOIN memory_sources s ON s.memory_id = m.id \
         WHERE s.item_id = ?1 AND m.forgetting = 0 \
           AND (?2 = 0 OR (SELECT COUNT(*) FROM memory_sources o WHERE o.memory_id = m.id) = 1) \
         ORDER BY m.id",
    )?;
    Ok(stmt
        .query_map(params![item_id, only_this_item], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

fn set_keep_forever_tx(tx: &rusqlite::Transaction<'_>, id: &str, on: bool) -> Result<()> {
    let n = tx.execute(
        "UPDATE memories SET keep_forever = ?2 WHERE id = ?1",
        params![id, on],
    )?;
    if n == 0 {
        return Err(Error::not_found(format!("memory {id}")));
    }
    Ok(())
}

/// Set the flag on the whole version chain, so an earlier version cited on an old page is
/// covered; returns the cached audio files to shred once the transaction has committed.
pub(crate) fn set_private_tx(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    on: bool,
) -> Result<Vec<PathBuf>> {
    let versions = chain(tx, id)?;
    if versions.is_empty() {
        return Err(Error::not_found(format!("memory {id}")));
    }
    for v in &versions {
        tx.execute(
            "UPDATE memories SET private = ?2 WHERE id = ?1",
            params![v, on],
        )?;
    }
    let pages = pages_citing(tx, &versions)?;
    drop_audio_rows(tx, &pages)
}

pub(crate) fn mark_used_tx(tx: &rusqlite::Transaction<'_>, id: &str, now: &str) -> Result<()> {
    tx.execute(
        "UPDATE memories SET last_used = ?2, \
         status = CASE WHEN status IN ('fading', 'letGo') THEN 'active' ELSE status END, \
         let_go_at = CASE WHEN status = 'letGo' THEN NULL ELSE let_go_at END \
         WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}
