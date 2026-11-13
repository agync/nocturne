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

use jiff::civil::Date;
use nocturne_contract::{Kept, Source};
use nocturne_engine::{Candidate, Predicate};

pub fn date(s: &str) -> Date {
    s.parse().unwrap()
}

/// Keep a note and return its id.
pub fn keep(engine: &Engine, text: &str) -> String {
    keep_as(engine, text, Source::QuickCapture)
}

pub fn keep_as(engine: &Engine, text: &str, source: Source) -> String {
    match engine.keep_text(text, source).unwrap() {
        Kept::New { item } | Kept::Duplicate { item } => item.id,
    }
}

pub fn cand(subject: &str, predicate: Predicate, object: &str, sentence: &str) -> Candidate {
    Candidate::new(subject, predicate, object, sentence, 0.8)
}

/// Keep a note and apply one candidate from it; returns (item id, memory id).
pub fn learn(engine: &Engine, text: &str, c: Candidate) -> (String, String) {
    let item = keep(engine, text);
    let report = engine.apply_candidates(&item, vec![c]).unwrap();
    let id = report
        .created
        .first()
        .or(report.reinforced.first())
        .cloned()
        .expect("a memory was created or reinforced");
    (item, id)
}
