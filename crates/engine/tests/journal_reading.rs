//! Reading the journal: the journal mark on memories, the mentions an item makes, check-ins
//! that yield mentions only, versions, names that fit two people, and what the page and Ask do
//! with all of it.

mod common;

use common::{EngineExt, Fixture, cand, fixture, keep};
use nocturne_contract::ItemStatus;
use nocturne_contract::{EntityKind, SameNameAnswer, Span};
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

/// Two people called Sam, each known by something already kept. Returns their entity ids.
fn two_sams(f: &Fixture) -> (String, String) {
    let learn_sam = |full: &str, role: &str| {
        let item = keep(&f.engine, &format!("{full} is {role}."));
        let report = f
            .engine
            .apply(
                &item,
                vec![person(
                    full,
                    Predicate::Knows,
                    "you",
                    &format!("{full} is {role}."),
                )],
            )
            .unwrap();
        let entity: String = conn(f)
            .query_row(
                "SELECT entity_id FROM memories WHERE id = ?1",
                [&report.created[0]],
                |r| r.get(0),
            )
            .unwrap();
        conn(f)
            .execute(
                "UPDATE entities SET role = ?2 WHERE id = ?1",
                rusqlite::params![entity, role],
            )
            .unwrap();
        entity
    };
    (
        learn_sam("Sam Jones", "your neighbour"),
        learn_sam("Sam Patel", "Leo\u{2019}s teacher"),
    )
}

const SAM_ENTRY: &str = "Priya made tea. Sam came round to fix the sink.";

fn sam_claim() -> Candidate {
    person(
        "Sam",
        Predicate::Promised,
        "the sink",
        "Sam will fix the sink.",
    )
}

fn about(f: &Fixture, entity: &str) -> i64 {
    count(
        f,
        &format!("SELECT COUNT(*) FROM memories WHERE entity_id = '{entity}'"),
    )
}

#[test]
fn two_sams_make_a_same_name_question_and_file_nothing() {
    let f = fixture();
    let (jones, patel) = two_sams(&f);
    let entry = journal_item(&f, SAM_ENTRY, "entry");
    let report = f.engine.apply(&entry, vec![sam_claim()]).unwrap();
    assert!(report.created.is_empty());
    assert_eq!(about(&f, &jones), 1);
    assert_eq!(about(&f, &patel), 1);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM memories"), 2);
    assert!(mentions(&f, &entry).is_empty());

    let questions = f.engine.same_name_questions(None).unwrap();
    assert_eq!(questions.len(), 1);
    let q = &questions[0];
    assert_eq!(q.name, "Sam");
    assert_eq!(q.quote, "Sam came round to fix the sink.");
    assert_eq!(q.receipt.item_id, entry);
    assert_eq!(
        q.receipt.span,
        Some(Span::Quote {
            text: q.quote.clone()
        })
    );
    assert_eq!(
        q.options
            .iter()
            .map(|o| (o.name.as_str(), o.role.as_deref()))
            .collect::<Vec<_>>(),
        vec![
            ("Sam Jones", Some("your neighbour")),
            ("Sam Patel", Some("Leo\u{2019}s teacher"))
        ]
    );
    assert!(
        f.engine
            .same_name_questions(Some(&["other".to_string()]))
            .unwrap()
            .is_empty()
    );
    let needs = f.engine.needs_you().unwrap();
    assert!(matches!(
        needs.as_slice(),
        [nocturne_contract::NeedsYouEntry::SameName { .. }]
    ));

    f.engine.apply(&entry, vec![sam_claim()]).unwrap();
    assert_eq!(
        f.engine.same_name_questions(None).unwrap().len(),
        1,
        "reading again asks once"
    );
}

#[test]
fn a_name_that_is_a_whole_name_or_fits_one_person_is_not_asked_about() {
    let f = fixture();
    let (_jones, _patel) = two_sams(&f);
    let entry = journal_item(&f, "Sam Jones came. Priya came.", "entry");
    f.engine
        .apply(
            &entry,
            vec![
                person(
                    "Sam Jones",
                    Predicate::Knows,
                    "Priya",
                    "Sam Jones knows Priya.",
                ),
                person(
                    "Priya",
                    Predicate::Knows,
                    "Sam Jones",
                    "Priya knows Sam Jones.",
                ),
            ],
        )
        .unwrap();
    assert!(f.engine.same_name_questions(None).unwrap().is_empty());
}

#[test]
fn answering_with_a_person_files_it_under_that_sam_only() {
    let f = fixture();
    let (jones, patel) = two_sams(&f);
    let entry = journal_item(&f, SAM_ENTRY, "entry");
    f.engine.apply(&entry, vec![sam_claim()]).unwrap();
    let q = f.engine.same_name_questions(None).unwrap().remove(0);
    let answered = f
        .engine
        .answer_same_name(
            &q.id,
            SameNameAnswer::Entity {
                entity_id: jones.clone(),
            },
        )
        .unwrap();
    assert_eq!(answered.name, "Sam Jones");
    assert_eq!(answered.role.as_deref(), Some("your neighbour"));
    assert_eq!(about(&f, &jones), 2);
    assert_eq!(about(&f, &patel), 1);
    assert!(journal_flag(
        &f,
        &count_text(
            &f,
            &format!("SELECT id FROM memories WHERE entity_id = '{jones}' AND journal = 1")
        )
    ));
    let rows = mentions(&f, &entry);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "Sam Jones");
    assert_eq!(rows[0].1, Some(16), "where Sam stands in the words");
    assert!(f.engine.same_name_questions(None).unwrap().is_empty());
    assert!(
        !f.engine
            .needs_you()
            .unwrap()
            .iter()
            .any(|e| matches!(e, nocturne_contract::NeedsYouEntry::SameName { .. }))
    );
    assert!(
        f.engine
            .answer_same_name(&q.id, SameNameAnswer::New)
            .is_err()
    );

    f.engine.apply(&entry, vec![sam_claim()]).unwrap();
    assert_eq!(about(&f, &jones), 2, "a re-read keeps the answer");
    assert!(f.engine.same_name_questions(None).unwrap().is_empty());
}

fn count_text(f: &Fixture, sql: &str) -> String {
    conn(f).query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn leave_it_files_nothing_and_someone_new_files_under_a_new_person() {
    let f = fixture();
    let (jones, patel) = two_sams(&f);
    let leave = journal_item(&f, SAM_ENTRY, "entry");
    f.engine.apply(&leave, vec![sam_claim()]).unwrap();
    let q = f.engine.same_name_questions(None).unwrap().remove(0);
    f.engine
        .answer_same_name(&q.id, SameNameAnswer::Leave)
        .unwrap();
    assert_eq!(count(&f, "SELECT COUNT(*) FROM memories"), 2);
    assert!(mentions(&f, &leave).is_empty());
    assert!(f.engine.same_name_questions(None).unwrap().is_empty());
    f.engine.apply(&leave, vec![sam_claim()]).unwrap();
    assert_eq!(
        count(&f, "SELECT COUNT(*) FROM memories"),
        2,
        "left means left"
    );

    let other = journal_item(&f, "Sam waved from the car. Then Sam drove off.", "entry");
    f.engine.apply(&other, vec![sam_claim()]).unwrap();
    let q = f.engine.same_name_questions(None).unwrap().remove(0);
    let answered = f
        .engine
        .answer_same_name(&q.id, SameNameAnswer::New)
        .unwrap();
    assert_eq!(answered.name, "Sam");
    let sam: String = conn(&f)
        .query_row(
            "SELECT id FROM entities WHERE name = 'Sam' AND kind = 'person'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(sam, jones);
    assert_ne!(sam, patel);
    assert_eq!(about(&f, &sam), 1);
    assert_eq!(mentions(&f, &other)[0].0, "Sam");
}

#[test]
fn a_checkin_naming_a_sam_asks_too_and_answering_records_the_mention_only() {
    let f = fixture();
    let (jones, _patel) = two_sams(&f);
    let checkin = journal_item(&f, "Tense. Sam is coming round again.", "checkin");
    f.engine
        .apply(
            &checkin,
            vec![person(
                "Sam",
                Predicate::Knows,
                "you",
                "Sam is coming round.",
            )],
        )
        .unwrap();
    let q = f.engine.same_name_questions(None).unwrap().remove(0);
    f.engine
        .answer_same_name(&q.id, SameNameAnswer::Entity { entity_id: jones })
        .unwrap();
    assert_eq!(count(&f, "SELECT COUNT(*) FROM memories"), 2);
    assert_eq!(mentions(&f, &checkin)[0].0, "Sam Jones");
}
