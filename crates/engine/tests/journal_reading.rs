//! Reading the journal: the journal mark on memories, the mentions an item makes, check-ins
//! that yield mentions only, versions, names that fit two people, and what the page and Ask do
//! with all of it.

mod common;

use common::{EngineExt, Fixture, cand, fixture, keep};
use nocturne_engine::Predicate;

fn conn(f: &Fixture) -> rusqlite::Connection {
    let c = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    c.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    c
}

/// Make a kept item a journal item of `role` (`entry`, `good` or `checkin`), kept on `day`.
fn mark(f: &Fixture, item: &str, role: &str, day: &str) {
    let kind = if role == "checkin" { "checkin" } else { "text" };
    conn(f)
        .execute(
            "UPDATE items SET kind = ?2, \
             metadata = json_set(metadata, '$.journal', ?3, '$.journal_day', ?4) WHERE id = ?1",
            rusqlite::params![item, kind, role, day],
        )
        .unwrap();
}

/// Keep `text` and make it a journal item.
fn journal_item(f: &Fixture, text: &str, role: &str) -> String {
    let id = keep(&f.engine, text);
    mark(f, &id, role, "2026-11-12");
    id
}

fn count(f: &Fixture, sql: &str) -> i64 {
    conn(f).query_row(sql, [], |r| r.get(0)).unwrap()
}

fn journal_flag(f: &Fixture, memory: &str) -> bool {
    conn(f)
        .query_row(
            "SELECT journal FROM memories WHERE id = ?1",
            [memory],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn an_entrys_memory_carries_the_journal_mark_and_an_ordinary_one_does_not() {
    let f = fixture();
    let entry = journal_item(&f, "Went to see Priya about the move.", "entry");
    let report = f
        .engine
        .apply(
            &entry,
            vec![cand(
                "Priya",
                Predicate::Knows,
                "the move",
                "Priya knows about the move.",
            )],
        )
        .unwrap();
    assert!(journal_flag(&f, &report.created[0]));

    let good = journal_item(&f, "Sun on the balcony.", "good");
    let report = f
        .engine
        .apply(
            &good,
            vec![cand(
                "Priya",
                Predicate::Prefers,
                "the balcony",
                "Priya likes the balcony.",
            )],
        )
        .unwrap();
    assert!(journal_flag(&f, &report.created[0]));

    let plain = keep(&f.engine, "Leo likes tea.");
    let report = f
        .engine
        .apply(
            &plain,
            vec![cand("Leo", Predicate::Prefers, "tea", "Leo likes tea.")],
        )
        .unwrap();
    assert!(!journal_flag(&f, &report.created[0]));
}

#[test]
fn a_private_source_still_wins_over_the_journal_mark() {
    let f = fixture();
    let entry = journal_item(&f, "Ana is allergic to cats.", "entry");
    let claim = || {
        cand(
            "Ana",
            Predicate::Prefers,
            "no cats",
            "Ana keeps away from cats.",
        )
    };
    let report = f.engine.apply(&entry, vec![claim()]).unwrap();
    let memory = report.created[0].clone();
    assert!(journal_flag(&f, &memory));
    assert_eq!(count(&f, "SELECT private FROM memories"), 0);

    let secret = keep(&f.engine, "Ana keeps away from cats, privately.");
    f.engine.set_item_private(&secret, true).unwrap();
    f.engine.apply(&secret, vec![claim()]).unwrap();
    assert_eq!(
        count(
            &f,
            "SELECT private FROM memories WHERE superseded_at IS NULL"
        ),
        1,
        "private wins"
    );
    assert!(journal_flag(&f, &memory), "the mark stays beside it");
}

#[test]
fn what_a_journal_memory_asks_never_reaches_the_tray() {
    let f = fixture();
    let entry = journal_item(&f, "I promised Priya the book.", "entry");
    f.engine
        .apply(
            &entry,
            vec![cand(
                "me",
                Predicate::Promised,
                "Priya the book",
                "You promised Priya the book.",
            )],
        )
        .unwrap();
    assert_eq!(f.engine.needs_you().unwrap().len(), 1);
    assert!(f.engine.needs_you_public().unwrap().is_empty());
}
