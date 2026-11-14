//! Finding answers when words differ only in their endings: stemmed search.

mod common;

use common::*;
use nocturne_contract::MemoryFilter;
use nocturne_engine::{Predicate, store};

#[test]
fn a_plural_finds_the_singular_and_search_as_you_type_still_works() {
    let f = fixture();
    let (_, plumber) = learn(
        &f.engine,
        "The plumber is coming on Friday.",
        cand(
            "Mr Hughes",
            Predicate::Scheduled,
            "Friday",
            "The plumber, Mr Hughes, is coming on Friday.",
        ),
    );
    let (_, swim) = learn(
        &f.engine,
        "Leo swims on Mondays.",
        cand(
            "Leo",
            Predicate::Scheduled,
            "Mondays",
            "Leo swims on Mondays.",
        ),
    );
    let found = |q: &str| -> Vec<String> {
        f.engine
            .retrieve(q, None, 5)
            .unwrap()
            .into_iter()
            .map(|h| h.memory.id)
            .collect()
    };
    assert_eq!(found("Which plumbers?"), std::slice::from_ref(&plumber));
    assert_eq!(found("plumber"), std::slice::from_ref(&plumber));
    assert_eq!(found("swimming on a Monday"), std::slice::from_ref(&swim));
    let typed = |q: &str| -> Vec<String> {
        f.engine
            .memories(MemoryFilter {
                query: Some(q.into()),
                limit: Some(10),
                ..MemoryFilter::default()
            })
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect()
    };
    for q in [
        "plu", "plum", "plumber", "plumbers", "Mr Hug", "Leo sw", "Mond",
    ] {
        assert!(!typed(q).is_empty(), "{q:?} finds something");
    }
    assert_eq!(typed("swims"), [swim]);
    assert!(typed("zebra").is_empty());
}

#[test]
fn a_store_from_before_stemming_is_reindexed_when_it_opens() {
    let dir = tempfile::tempdir().unwrap();
    {
        let conn = rusqlite::Connection::open(dir.path().join("nocturne.db")).unwrap();
        for (version, sql) in store::migrations() {
            if version <= 10 {
                conn.execute_batch(sql).unwrap();
                conn.pragma_update(None, "user_version", version).unwrap();
            }
        }
        conn.execute_batch(
            "INSERT INTO items(id, kind, source, trust, sha256, title, kept_at, status, text) \
             VALUES ('itm_old', 'text', 'quickCapture', 'trusted', 'abc', 'Plumber', \
             '2026-11-01T00:00:00.000Z', 'understood', 'The plumber fixes taps.'); \
             INSERT INTO memories(id, subject, subject_key, predicate, object, object_key, \
             sentence, recorded_at, confidence, status) VALUES ('mem_old', 'Plumber', 'plumber', \
             'knows', 'taps', 'taps', 'The plumber fixes taps.', '2026-11-01T00:00:00.000Z', \
             0.9, 'active'); \
             INSERT INTO memory_sources(memory_id, item_id) VALUES ('mem_old', 'itm_old');",
        )
        .unwrap();
    }
    let clock = std::sync::Arc::new(nocturne_engine::ManualClock::at(
        "2026-11-13T07:00:00Z",
        "Europe/London",
    ));
    let engine = nocturne_engine::Engine::open(dir.path(), clock).unwrap();
    let hits = engine.retrieve("who fixed the plumbing", None, 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.id, "mem_old");
    let conn = rusqlite::Connection::open(dir.path().join("nocturne.db")).unwrap();
    for table in ["items_fts", "memories_fts"] {
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = ?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        assert!(sql.contains("porter"), "{table}: {sql}");
        let n: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE {table} MATCH 'taps'"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "{table} holds what was there before");
    }
    // Every write still reaches both indexes, and a deletion leaves nothing behind.
    drop(conn);
    let item = keep(&engine, "The zebrafish47 tank needs cleaning.");
    assert_eq!(
        sql_count(
            dir.path(),
            "SELECT COUNT(*) FROM items_fts WHERE items_fts MATCH 'zebrafish47'"
        ),
        1
    );
    engine.unkeep(&item).unwrap();
    assert_eq!(
        sql_count(
            dir.path(),
            "SELECT COUNT(*) FROM items_fts WHERE items_fts MATCH 'zebrafish47'"
        ),
        0
    );
}
