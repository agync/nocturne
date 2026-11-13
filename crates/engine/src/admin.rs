//! The small facade the app shell needs around the core: the download ledger, the privacy view,
//! store statistics and the integrity check.

use std::path::Path;

use nocturne_contract::{LedgerEntry, LedgerKind, PrivacyView};
use rusqlite::params;

use crate::Engine;
use crate::error::Result;
use crate::settings::read_settings;
use crate::store;
use crate::types::Stats;
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
}
