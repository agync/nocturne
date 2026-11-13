//! The forgetting policy: use, decay, safety exemptions, keep forever and restore.

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use nocturne_contract::MemoryStatus;
use rusqlite::params;

use crate::Engine;
use crate::audio::{drop_audio_rows, pages_citing, shred_all};
use crate::error::{Error, Result};
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
    if matches!(m.predicate.as_str(), "scheduled" | "promised")
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
        let mut changes: Vec<(String, MemoryStatus)> = Vec::new();
        let mut report = DecayReport::default();
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
                (MemoryStatus::Active, MemoryStatus::Fading) => {
                    report.faded += 1;
                    changes.push((m.id.clone(), target));
                }
                (MemoryStatus::Active | MemoryStatus::Fading, MemoryStatus::LetGo) => {
                    report.let_go += 1;
                    changes.push((m.id.clone(), target));
                }
                _ => {}
            }
        }
        let stamp = fmt_ts(now);
        for chunk in changes.chunks(200) {
            self.write(|tx| {
                for (id, status) in chunk {
                    tx.execute(
                        "UPDATE memories SET status = ?2, \
                         let_go_at = CASE WHEN ?2 = 'letGo' THEN ?3 ELSE let_go_at END \
                         WHERE id = ?1",
                        params![id, enum_str(status), stamp],
                    )?;
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
        self.write(|tx| {
            let n = tx.execute(
                "UPDATE memories SET keep_forever = ?2 WHERE id = ?1",
                params![id, on],
            )?;
            if n == 0 {
                return Err(Error::not_found(format!("memory {id}")));
            }
            Ok(())
        })
    }

    /// Mark a memory private (or not). Cached audio of pages citing it is removed, so a newly
    /// private memory cannot be played back.
    pub fn set_private(&self, id: &str, on: bool) -> Result<()> {
        let files = self.write(|tx| {
            let n = tx.execute(
                "UPDATE memories SET private = ?2 WHERE id = ?1",
                params![id, on],
            )?;
            if n == 0 {
                return Err(Error::not_found(format!("memory {id}")));
            }
            let pages = pages_citing(tx, &[id.to_string()])?;
            drop_audio_rows(tx, &pages)
        })?;
        shred_all(files);
        Ok(())
    }
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
