//! Reading the journal: the journal mark on memories, the mentions an item makes, check-ins
//! that yield mentions only, versions, names that fit two people, and what the page and Ask do
//! with all of it.

mod common;

use common::{EngineExt, Fixture, cand, fixture, keep};
use nocturne_contract::ItemStatus;
use nocturne_contract::{EntityKind, Span};
use nocturne_engine::{Candidate, Predicate};

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

fn replaces(f: &Fixture, item: &str, earlier: &str) {
    conn(f)
        .execute(
            "UPDATE items SET metadata = json_set(metadata, '$.replaces', ?2) WHERE id = ?1",
            rusqlite::params![item, earlier],
        )
        .unwrap();
}

fn person(subject: &str, predicate: Predicate, object: &str, sentence: &str) -> Candidate {
    let mut c = cand(subject, predicate, object, sentence);
    c.subject_kind = Some(EntityKind::Person);
    c
}

fn place(subject: &str, predicate: Predicate, object: &str, sentence: &str) -> Candidate {
    let mut c = cand(subject, predicate, object, sentence);
    c.subject_kind = Some(EntityKind::Place);
    c
}

/// What an item names, in order: name, start, end, new, state.
type Row = (String, Option<u32>, Option<u32>, bool, String);

fn mentions(f: &Fixture, item: &str) -> Vec<Row> {
    let c = conn(f);
    let mut stmt = c
        .prepare(
            "SELECT e.name, n.start, n.end, n.new, n.state FROM mentions n \
             JOIN entities e ON e.id = n.entity_id WHERE n.item_id = ?1 ORDER BY n.ord",
        )
        .unwrap();
    stmt.query_map([item], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
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

#[test]
fn mentions_are_written_in_order_with_utf16_offsets_and_new_on_first_sight() {
    let f = fixture();
    let entry = journal_item(&f, "Ana \u{1F389} met Priya at Caf\u{e9} Rose.", "entry");
    f.engine
        .apply(
            &entry,
            vec![
                person("Priya", Predicate::Knows, "Ana", "Priya knows Ana."),
                place(
                    "Caf\u{e9} Rose",
                    Predicate::Happened,
                    "a meeting",
                    "Ana met Priya at Caf\u{e9} Rose.",
                ),
                person("Ana", Predicate::Knows, "Priya", "Ana knows Priya."),
            ],
        )
        .unwrap();
    assert_eq!(
        mentions(&f, &entry),
        vec![
            ("Ana".into(), Some(0), Some(3), true, "found".into()),
            ("Priya".into(), Some(11), Some(16), true, "found".into()),
            (
                "Caf\u{e9} Rose".into(),
                Some(20),
                Some(29),
                true,
                "found".into()
            ),
        ]
    );

    let next = keep(&f.engine, "Ana called.");
    f.engine
        .apply(
            &next,
            vec![person("Ana", Predicate::Knows, "Leo", "Ana knows Leo.")],
        )
        .unwrap();
    let rows = mentions(&f, &next);
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].3, "Ana was met before");
}

#[test]
fn a_name_that_is_not_in_the_words_is_found_by_its_quote_or_left_without_a_place() {
    let f = fixture();
    let entry = journal_item(
        &f,
        "Today my mother\u{2019}s scan came back clear.",
        "entry",
    );
    let mut mum = person("Mum", Predicate::Happened, "a scan", "Mum had a scan.");
    mum.span = Some(Span::Quote {
        text: "mother\u{2019}s scan".into(),
    });
    let unplaced = person("Gran", Predicate::Knows, "Mum", "Gran knows Mum.");
    f.engine.apply(&entry, vec![unplaced, mum]).unwrap();
    assert_eq!(
        mentions(&f, &entry),
        vec![
            ("Mum".into(), Some(9), Some(22), true, "found".into()),
            ("Gran".into(), None, None, true, "found".into()),
        ]
    );
}

#[test]
fn a_name_inside_a_longer_word_is_not_a_mention_place() {
    let f = fixture();
    let entry = journal_item(&f, "Sampson came. Then Sam left.", "entry");
    f.engine
        .apply(
            &entry,
            vec![person("Sam", Predicate::Knows, "Leo", "Sam knows Leo.")],
        )
        .unwrap();
    assert_eq!(mentions(&f, &entry)[0].1, Some(19));
}

#[test]
fn reading_again_keeps_what_the_person_did_and_writes_no_second_row() {
    let f = fixture();
    let entry = journal_item(&f, "Priya came round.", "entry");
    let claim = || person("Priya", Predicate::Knows, "Leo", "Priya knows Leo.");
    f.engine.apply(&entry, vec![claim()]).unwrap();
    conn(&f)
        .execute("UPDATE mentions SET state = 'confirmed'", [])
        .unwrap();
    f.engine.apply(&entry, vec![claim()]).unwrap();
    let rows = mentions(&f, &entry);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].4, "confirmed");
    assert!(rows[0].3, "still the item that first met her");

    conn(&f)
        .execute("UPDATE mentions SET state = 'removed'", [])
        .unwrap();
    f.engine.apply(&entry, vec![claim()]).unwrap();
    assert_eq!(mentions(&f, &entry)[0].4, "removed");
}

const CHECKIN: &str = "Low today. Gran\u{2019}s scan is on my mind and the dentist is on Friday.";

/// A check-in saying what a note would turn into several memories from.
fn checkin_claims() -> Vec<Candidate> {
    vec![
        person(
            "Gran",
            Predicate::Happened,
            "a scan",
            "Gran has a scan on her mind.",
        ),
        cand(
            "the dentist",
            Predicate::Due,
            "Friday",
            "The dentist is on Friday.",
        ),
        person("Gran", Predicate::Prefers, "tea", "Gran likes tea."),
    ]
}

#[test]
fn a_checkin_names_gran_and_yields_a_mention_and_no_memory() {
    let f = fixture();
    let checkin = journal_item(&f, CHECKIN, "checkin");
    let report = f.engine.apply(&checkin, checkin_claims()).unwrap();
    assert!(report.created.is_empty() && report.conflicts.is_empty());
    assert_eq!(count(&f, "SELECT COUNT(*) FROM memories"), 0);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM memory_sources"), 0);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM conflicts"), 0);
    assert_eq!(
        mentions(&f, &checkin)
            .into_iter()
            .map(|m| (m.0, m.1))
            .collect::<Vec<_>>(),
        vec![("Gran".to_string(), Some(11))],
        "the dentist has no kind, so nothing is made of it"
    );
    assert_eq!(
        f.engine.get_item(&checkin).unwrap().status,
        ItemStatus::Understood
    );
    assert!(f.engine.needs_you().unwrap().is_empty());
}

#[test]
fn a_checkin_is_known_by_its_metadata_as_well_as_its_kind() {
    let f = fixture();
    let checkin = keep(&f.engine, CHECKIN);
    conn(&f)
        .execute(
            "UPDATE items SET metadata = json_set(metadata, '$.journal', 'checkin') WHERE id = ?1",
            [&checkin],
        )
        .unwrap();
    f.engine.apply(&checkin, checkin_claims()).unwrap();
    assert_eq!(count(&f, "SELECT COUNT(*) FROM memories"), 0);
    assert_eq!(mentions(&f, &checkin).len(), 1);
}

#[test]
fn reading_a_checkin_again_adds_no_second_mention() {
    let f = fixture();
    let checkin = journal_item(&f, CHECKIN, "checkin");
    f.engine.apply(&checkin, checkin_claims()).unwrap();
    f.engine.apply(&checkin, checkin_claims()).unwrap();
    assert_eq!(mentions(&f, &checkin).len(), 1);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM memories"), 0);
}

#[test]
fn nothing_a_checkin_says_reaches_ask_or_the_online_path() {
    let f = fixture();
    let checkin = journal_item(&f, CHECKIN, "checkin");
    f.engine.apply(&checkin, checkin_claims()).unwrap();
    let ordinary = keep(&f.engine, "Gran likes to knit by the window.");
    f.engine
        .apply(
            &ordinary,
            vec![person(
                "Gran",
                Predicate::Prefers,
                "knitting",
                "Gran likes to knit.",
            )],
        )
        .unwrap();

    let hits = f.engine.retrieve("Gran scan dentist", None, 12).unwrap();
    assert!(
        hits.iter()
            .all(|h| h.memory.receipts.iter().all(|r| r.item_id != checkin))
    );
    let hits = f
        .engine
        .with_source_lines("Gran scan on her mind", hits, 12)
        .unwrap();
    assert!(
        hits.iter()
            .all(|h| h.memory.receipts.iter().all(|r| r.item_id != checkin)),
        "no line of a check-in is offered as a source line"
    );
    let ids: Vec<String> = hits.iter().map(|h| h.memory.id.clone()).collect();
    let sendable = f.engine.sendable(&ids, true).unwrap();
    for (h, ok) in hits.iter().zip(sendable) {
        if ok {
            assert!(h.memory.receipts.iter().all(|r| r.item_id != checkin));
        }
    }
    assert_eq!(
        f.engine
            .sendable(&["mem_nothing".to_string()], true)
            .unwrap(),
        vec![false]
    );
}

#[test]
fn a_line_of_an_entry_is_still_offered_but_not_one_of_a_checkin() {
    let f = fixture();
    let entry = journal_item(&f, "Walked the long way home past the old mill.", "entry");
    f.engine
        .apply(
            &entry,
            vec![person("Leo", Predicate::Knows, "Ana", "Leo knows Ana.")],
        )
        .unwrap();
    let checkin = journal_item(&f, "Walked the long way home past the mill.", "checkin");
    f.engine.apply(&checkin, Vec::new()).unwrap();
    let hits = f.engine.retrieve("long way home mill", None, 12).unwrap();
    let hits = f
        .engine
        .with_source_lines("long way home mill", hits, 12)
        .unwrap();
    let from: Vec<&str> = hits
        .iter()
        .flat_map(|h| h.memory.receipts.iter().map(|r| r.item_id.as_str()))
        .collect();
    assert!(from.contains(&entry.as_str()));
    assert!(!from.contains(&checkin.as_str()));
}

#[test]
fn a_journal_memory_may_go_online_like_any_other() {
    let f = fixture();
    let entry = journal_item(&f, "Priya is moving to York in the spring.", "entry");
    let id = f
        .engine
        .apply(
            &entry,
            vec![cand(
                "Priya",
                Predicate::LivesIn,
                "York",
                "Priya is moving to York.",
            )],
        )
        .unwrap()
        .created[0]
        .clone();
    assert!(journal_flag(&f, &id));
    assert_eq!(f.engine.sendable(&[id], false).unwrap(), vec![true]);
}

fn memory_ids(f: &Fixture) -> Vec<String> {
    let c = conn(f);
    let mut stmt = c
        .prepare("SELECT sentence FROM memories ORDER BY sentence")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn a_replaced_blocks_sole_memory_goes_when_the_new_version_no_longer_says_it() {
    let f = fixture();
    let old = journal_item(&f, "Priya likes tea. Leo likes coffee.", "entry");
    f.engine
        .apply(
            &old,
            vec![
                person("Priya", Predicate::Prefers, "tea", "Priya likes tea."),
                person("Leo", Predicate::Prefers, "coffee", "Leo likes coffee."),
            ],
        )
        .unwrap();
    assert_eq!(memory_ids(&f).len(), 2);

    let new = journal_item(&f, "Priya likes tea. Leo likes cake.", "entry");
    replaces(&f, &new, &old);
    let report = f
        .engine
        .apply(
            &new,
            vec![person(
                "Priya",
                Predicate::Prefers,
                "tea",
                "Priya likes tea.",
            )],
        )
        .unwrap();
    assert_eq!(report.removed.len(), 1);
    assert_eq!(memory_ids(&f), vec!["Priya likes tea."]);
    assert!(
        report.created.is_empty(),
        "tea is the same claim, not a second one"
    );
    assert_eq!(
        count(
            &f,
            &format!("SELECT COUNT(*) FROM memory_sources WHERE item_id = '{old}'")
        ),
        0,
        "the earlier version no longer stands behind anything"
    );
    assert_eq!(
        mentions(&f, &old).len(),
        2,
        "the earlier version keeps its mentions"
    );
    assert_eq!(mentions(&f, &new).len(), 1);
}

#[test]
fn what_the_earlier_version_said_stays_when_the_person_confirmed_it() {
    let f = fixture();
    let old = journal_item(&f, "Leo likes coffee.", "entry");
    let id = f
        .engine
        .apply(
            &old,
            vec![person(
                "Leo",
                Predicate::Prefers,
                "coffee",
                "Leo likes coffee.",
            )],
        )
        .unwrap()
        .created[0]
        .clone();
    f.engine.confirm(&id, true).unwrap();
    let new = journal_item(&f, "Leo likes cake.", "entry");
    replaces(&f, &new, &old);
    let report = f.engine.apply(&new, Vec::new()).unwrap();
    assert!(report.removed.is_empty());
    assert_eq!(memory_ids(&f), vec!["Leo likes coffee."]);
    assert_eq!(
        count(
            &f,
            &format!("SELECT COUNT(*) FROM memory_sources WHERE item_id = '{old}'")
        ),
        1,
        "its receipt is still the earlier words"
    );
}
