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
    "remote_devices",
    "remote_keeps",
    "remote_events",
    "remote_state",
    "online_state",
    "online_sends",
    "embeddings_next",
    "reindex_state",
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

#[test]
fn deleted_text_is_overwritten_in_the_store_and_in_both_search_indexes() {
    let dir = tempfile::tempdir().unwrap();
    drop(engine(dir.path()));
    let conn = rusqlite::Connection::open(dir.path().join("nocturne.db")).unwrap();
    for table in ["items_fts", "memories_fts"] {
        let v: String = conn
            .query_row(
                &format!("SELECT v FROM {table}_config WHERE k = 'secure-delete'"),
                [],
                |r| {
                    r.get::<_, rusqlite::types::Value>(0).map(|v| match v {
                        rusqlite::types::Value::Integer(i) => i.to_string(),
                        rusqlite::types::Value::Text(t) => t,
                        other => format!("{other:?}"),
                    })
                },
            )
            .unwrap();
        assert_eq!(v, "1", "{table} deletes securely");
    }
}

#[test]
fn migration_7_applies_on_a_v6_store() {
    let dir = tempfile::tempdir().unwrap();
    {
        let conn = rusqlite::Connection::open(dir.path().join("nocturne.db")).unwrap();
        for (version, sql) in store::migrations() {
            if version <= 6 {
                conn.execute_batch(sql).unwrap();
                conn.pragma_update(None, "user_version", version).unwrap();
            }
        }
        conn.execute(
            "INSERT INTO items(id, kind, source, trust, sha256, title, kept_at, status) \
             VALUES ('itm_old', 'text', 'quickCapture', 'trusted', 'abc', 't', \
             '2026-11-01T00:00:00.000Z', 'kept')",
            [],
        )
        .unwrap();
    }
    let e = engine(dir.path());
    assert_eq!(e.get_item("itm_old").unwrap().title, "t");
    drop(e);

    let conn = rusqlite::Connection::open(dir.path().join("nocturne.db")).unwrap();
    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert!(version >= 7);
    for table in ["remote_devices", "remote_keeps", "remote_events"] {
        let n: u32 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{table} starts empty");
    }
    // Off is the fresh state: one settings row, nothing wanted on, nothing sent.
    let row: (String, i64, String, i64, f64, Option<String>) = conn
        .query_row(
            "SELECT connection, wanted_on, auto_off, keep_awake, sent_bytes, on_since \
             FROM remote_state",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        row,
        ("simple".into(), 0, "unused30Days".into(), 0, 0.0, None)
    );
    assert!(
        conn.execute("INSERT INTO remote_state(id) VALUES (2)", [])
            .is_err(),
        "one row only"
    );
}

/// A store at version 9, as the release before Online answers left it.
fn v9_store(dir: &std::path::Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(dir.join("nocturne.db")).unwrap();
    for (version, sql) in store::migrations() {
        if version <= 9 {
            conn.execute_batch(sql).unwrap();
            conn.pragma_update(None, "user_version", version).unwrap();
        }
    }
    conn
}

#[test]
fn migration_10_applies_on_a_v9_store() {
    let dir = tempfile::tempdir().unwrap();
    drop(v9_store(dir.path()));
    drop(engine(dir.path()));

    let conn = rusqlite::Connection::open(dir.path().join("nocturne.db")).unwrap();
    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert!(version >= 10);
    for table in ["online_sends", "embeddings_next"] {
        let n: u32 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{table} starts empty");
    }
    // Off is the fresh state: nothing wanted on, nothing set up, phones on, sensitive off,
    // no monthly limit, nothing sent, no key storage named.
    let row: (
        i64,
        i64,
        i64,
        i64,
        String,
        i64,
        f64,
        Option<String>,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT wanted_on, set_up, phones, send_sensitive, limit_kind, limit_questions, \
             sent_bytes, base_url, key_storage FROM online_state",
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
                    r.get(7)?,
                    r.get(8)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(row, (0, 0, 1, 0, "none".into(), 200, 0.0, None, None));
    let phase: Option<String> = conn
        .query_row("SELECT phase FROM reindex_state", [], |r| r.get(0))
        .unwrap();
    assert_eq!(phase, None, "no re-read under way");
    for table in ["online_state", "reindex_state"] {
        assert!(
            conn.execute(&format!("INSERT INTO {table}(id) VALUES (2)"), [])
                .is_err(),
            "{table} has one row only"
        );
    }
}

#[test]
fn migration_10_backfills_embedding_model() {
    let dir = tempfile::tempdir().unwrap();
    {
        let conn = v9_store(dir.path());
        conn.execute_batch(
            "INSERT INTO memories(id, subject, subject_key, predicate, object, object_key, \
               sentence, recorded_at, confidence, status) \
             VALUES ('mem_old', 'Bins', 'bins', 'go out', 'Tuesday', 'tuesday', \
               'Bins go out on Tuesday.', '2026-11-01T00:00:00.000Z', 0.9, 'active'); \
             INSERT INTO embeddings(memory_id, dim, vec) VALUES ('mem_old', 2, x'0000803f00000000');",
        )
        .unwrap();
    }
    drop(engine(dir.path()));

    let conn = rusqlite::Connection::open(dir.path().join("nocturne.db")).unwrap();
    let (model, dim): (String, u32) = conn
        .query_row(
            "SELECT model, dim FROM embeddings WHERE memory_id = 'mem_old'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(model, "Qwen3-Embedding-0.6B-GGUF");
    assert_eq!(dim, 2, "the vector itself is untouched");
}
