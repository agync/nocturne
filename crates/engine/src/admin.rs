//! The small facade the app shell needs around the core: the download ledger, the privacy view,
//! store statistics and the integrity check.

use std::path::Path;

use nocturne_contract::{LedgerEntry, LedgerKind, PrivacyView};
use rusqlite::params;

use crate::Engine;
use crate::error::Result;
use crate::settings::read_settings;
use crate::store;
use crate::types::{Stats, StatusCounts};
use crate::util::{enum_str, fmt_ts};

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

impl Engine {
    /// Record a download Nocturne made, for the person's own ledger of what crossed the network.
    pub fn record_download(
        &self,
        kind: LedgerKind,
        label: &str,
        host: Option<&str>,
        bytes: f64,
    ) -> Result<()> {
        let now = fmt_ts(self.clock().now());
        self.write(|tx| {
            tx.execute(
                "INSERT INTO ledger(kind, label, host, bytes, at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![enum_str(&kind), label, host, bytes, now],
            )?;
            Ok(())
        })
    }

    /// Every download, oldest first.
    pub fn ledger(&self) -> Result<Vec<LedgerEntry>> {
        self.read(|conn| {
            let mut stmt = conn.prepare("SELECT kind, label, bytes, at FROM ledger ORDER BY id")?;
            let rows = stmt.query_map([], |r| {
                Ok(LedgerEntry {
                    kind: crate::rows::de(r.get(0)?)?,
                    label: r.get(1)?,
                    bytes: r.get(2)?,
                    at: r.get(3)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    /// What the privacy screen shows that the engine knows. It cannot measure traffic, so
    /// `sent_measured` is false and the app shell overrides it when it can.
    pub fn privacy(&self, data_dir: &Path) -> Result<PrivacyView> {
        let ledger = self.ledger()?;
        let settings = self.read(read_settings)?;
        let kept = dir_size(&self.originals_dir()) + dir_size(&self.inner.dir.join("audio"));
        let db = std::fs::metadata(self.db_path())
            .map(|m| m.len())
            .unwrap_or(0);
        Ok(PrivacyView {
            sent_to_internet_bytes: 0.0,
            sent_measured: false,
            checked_at: None,
            ledger,
            kept_bytes: (kept + db) as f64,
            mind_bytes: 0.0,
            data_dir: data_dir.to_string_lossy().into_owned(),
            lock: settings.lock,
            check_updates: settings.check_updates,
            sent_to_phones_bytes: 0.0,
            remote_on: false,
            remote_connection: None,
            remote_last_on_at: None,
        })
    }

    /// Counts and an integrity check for the advanced details.
    pub fn stats(&self) -> Result<Stats> {
        self.read(|conn| {
            let ok: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
            Ok(Stats {
                store_version: store::schema_version(conn)?,
                memory_count: conn.query_row(
                    "SELECT COUNT(*) FROM memories WHERE superseded_at IS NULL",
                    [],
                    |r| r.get(0),
                )?,
                item_count: conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))?,
                integrity_ok: ok == "ok",
            })
        })
    }

    /// Counts for the status line. Counted in the store (no rows loaded, no cap), with the same
    /// predicates `needs_you()` uses: snoozed entries and anything in a forget are left out.
    pub fn status_counts(&self) -> Result<StatusCounts> {
        let today = self.clock().now().to_zoned(self.clock().tz()).date();
        let today_s = today.to_string();
        let horizon = today
            .checked_add(jiff::Span::new().days(14))
            .map_err(crate::error::Error::from)?
            .to_string();
        self.read(|conn| {
            let count = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> Result<u32> {
                Ok(conn.query_row(sql, p, |r| r.get(0))?)
            };
            let dated = count(
                "SELECT COUNT(*) FROM memories m WHERE m.predicate IN ('due', 'promised') \
                 AND m.superseded_at IS NULL AND m.done = 0 AND m.status IN ('active', 'fading') \
                 AND (m.snoozed_until IS NULL OR m.snoozed_until <= ?2) \
                 AND (COALESCE(m.due, m.valid_from) IS NULL OR COALESCE(m.due, m.valid_from) <= ?1)",
                &[&horizon, &today_s],
            )?;
            // A "which is right?" shows only when both memories are current, neither is
            // snoozed and each has a source item.
            let which = count(
                "SELECT COUNT(*) FROM conflicts c \
                 JOIN memories a ON a.id = c.a JOIN memories b ON b.id = c.b \
                 WHERE c.resolved_at IS NULL AND a.superseded_at IS NULL AND b.superseded_at IS NULL \
                 AND COALESCE(a.snoozed_until > ?1, 0) = 0 AND COALESCE(b.snoozed_until > ?1, 0) = 0 \
                 AND EXISTS (SELECT 1 FROM memory_sources s JOIN items i ON i.id = s.item_id \
                             WHERE s.memory_id = a.id) \
                 AND EXISTS (SELECT 1 FROM memory_sources s JOIN items i ON i.id = s.item_id \
                             WHERE s.memory_id = b.id)",
                &[&today_s],
            )?;
            let unconfirmed = count(
                "SELECT COUNT(*) FROM memories m WHERE m.status = 'unconfirmed' \
                 AND m.superseded_at IS NULL \
                 AND (m.snoozed_until IS NULL OR m.snoozed_until <= ?1)",
                &[&today_s],
            )?;
            let waiting_to_read = count(
                "SELECT COUNT(*) FROM items WHERE status IN ('kept', 'reading') \
                 AND NOT EXISTS (SELECT 1 FROM forget_jobs \
                                 WHERE instr(items, '\"' || items.id || '\"') > 0)",
                &[],
            )?;
            let couldnt_read = count(
                "SELECT COUNT(*) FROM items WHERE status = 'couldntRead' \
                 AND NOT EXISTS (SELECT 1 FROM forget_jobs \
                                 WHERE instr(items, '\"' || items.id || '\"') > 0)",
                &[],
            )?;
            Ok(StatusCounts {
                needs_you: dated + which + unconfirmed,
                waiting_to_read,
                couldnt_read,
                unconfirmed,
            })
        })
    }
}
