use std::sync::Arc;

use nocturne_engine::{Engine, ManualClock, store};

fn engine(dir: &std::path::Path) -> Engine {
    Engine::open(
        dir,
        Arc::new(ManualClock::at("2026-11-13T07:00:00Z", "Europe/London")),
    )
    .unwrap()
}

const TABLES: &[&str] = &[
    "items",
    "entities",
    "memories",
    "memory_sources",
    "conflicts",
    "pages",
    "page_lines",
    "page_line_memories",
    "questions",
    "audio_cache",
    "embeddings",
    "settings",
    "ledger",
    "forget_tokens",
    "items_fts",
    "memories_fts",
];

#[test]
fn store_opens_and_migrates() {
    let dir = tempfile::tempdir().unwrap();
    drop(engine(dir.path()));
    // Opening twice is idempotent.
    drop(engine(dir.path()));

    let conn = rusqlite::Connection::open(dir.path().join("nocturne.db")).unwrap();
    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, store::latest_version());
    assert!(version >= 1);

    for table in TABLES {
        let n: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = ?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "table {table} exists");
    }

    // FTS5 is available in this build.
    conn.execute_batch("CREATE VIRTUAL TABLE probe USING fts5(a); DROP TABLE probe;")
        .unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

#[test]
fn migrations_are_ordered_and_numbered_from_one() {
    let m = store::migrations();
    assert!(!m.is_empty());
    for (i, (version, sql)) in m.iter().enumerate() {
        assert_eq!(*version as usize, i + 1);
        assert!(!sql.trim().is_empty());
    }
}

#[test]
fn a_store_from_the_future_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    drop(engine(dir.path()));
    {
        let conn = rusqlite::Connection::open(dir.path().join("nocturne.db")).unwrap();
        conn.pragma_update(None, "user_version", 999).unwrap();
    }
    let err = Engine::open(
        dir.path(),
        Arc::new(ManualClock::at("2026-11-13T07:00:00Z", "UTC")),
    )
    .err()
    .expect("refused");
    assert!(err.to_string().contains("newer"));
}

#[test]
fn engine_is_send_sync_clone() {
    fn assert<T: Send + Sync + Clone>() {}
    assert::<Engine>();
}
