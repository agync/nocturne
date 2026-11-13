//! Shared test helpers.
#![allow(dead_code)]

use std::path::Path;
use std::sync::Arc;

use nocturne_engine::{Engine, ManualClock};

pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub engine: Engine,
    pub clock: Arc<ManualClock>,
}

/// A fresh store in a temp dir, with a clock at 2026-11-13 07:00 London time.
pub fn fixture() -> Fixture {
    fixture_at("2026-11-13T07:00:00Z", "Europe/London")
}

pub fn fixture_at(now: &str, tz: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::at(now, tz));
    let engine = Engine::open(dir.path(), clock.clone()).unwrap();
    Fixture { dir, engine, clock }
}

pub fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

/// Run raw SQL against the store with a separate connection.
pub fn sql_count(dir: &Path, sql: &str) -> i64 {
    let conn = rusqlite::Connection::open(dir.join("nocturne.db")).unwrap();
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}
