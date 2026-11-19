//! Reading the journal: the journal mark on memories, the mentions an item makes, check-ins
//! that yield mentions only, versions, names that fit two people, and what the page and Ask do
//! with all of it.

mod common;

use common::{EngineExt, Fixture, cand, fixture, keep};
use nocturne_contract::ItemStatus;
use nocturne_contract::{
    Edition, EntityKind, MentionState, PeriodKind, RefileTarget, SameNameAnswer, Span,
};
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

fn mention_id(f: &Fixture, item: &str, name: &str) -> String {
    conn(f)
        .query_row(
            "SELECT n.id FROM mentions n JOIN entities e ON e.id = n.entity_id \
             WHERE n.item_id = ?1 AND e.name = ?2",
            rusqlite::params![item, name],
            |r| r.get(0),
        )
        .unwrap()
}

fn status_of(f: &Fixture, sentence: &str) -> String {
    conn(f)
        .query_row(
            "SELECT status FROM memories WHERE sentence = ?1",
            [sentence],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn removing_a_mention_lets_go_what_only_this_item_said_and_restoring_brings_it_back() {
    let f = fixture();
    let other = keep(&f.engine, "Priya likes tea.");
    f.engine
        .apply(
            &other,
            vec![person(
                "Priya",
                Predicate::Prefers,
                "tea",
                "Priya likes tea.",
            )],
        )
        .unwrap();
    let entry = journal_item(&f, "Priya likes tea and walks to work.", "entry");
    f.engine
        .apply(
            &entry,
            vec![
                person("Priya", Predicate::Prefers, "tea", "Priya likes tea."),
                person(
                    "Priya",
                    Predicate::Happened,
                    "a walk",
                    "Priya walks to work.",
                ),
            ],
        )
        .unwrap();
    let id = mention_id(&f, &entry, "Priya");

    let removed = f.engine.remove_mention(&id).unwrap();
    assert_eq!(removed.state, MentionState::Removed);
    assert_eq!(status_of(&f, "Priya walks to work."), "letGo");
    assert_eq!(
        status_of(&f, "Priya likes tea."),
        "active",
        "another item says it too"
    );
    assert_eq!(
        count(&f, "SELECT COUNT(*) FROM memories"),
        2,
        "nothing is deleted"
    );

    let back = f.engine.restore_mention(&id).unwrap();
    assert_eq!(back.state, MentionState::Found);
    assert_eq!(status_of(&f, "Priya walks to work."), "active");
    assert_eq!(
        count(
            &f,
            "SELECT COUNT(*) FROM memories WHERE let_go_at IS NOT NULL"
        ),
        0
    );
}

#[test]
fn restoring_brings_back_only_what_removing_let_go() {
    let f = fixture();
    let entry = journal_item(&f, "Priya likes tea and walks to work.", "entry");
    f.engine
        .apply(
            &entry,
            vec![
                person("Priya", Predicate::Prefers, "tea", "Priya likes tea."),
                person(
                    "Priya",
                    Predicate::Happened,
                    "a walk",
                    "Priya walks to work.",
                ),
            ],
        )
        .unwrap();
    conn(&f)
        .execute(
            "UPDATE memories SET status = 'letGo', let_go_at = '2026-10-01T00:00:00Z' \
             WHERE sentence = 'Priya likes tea.'",
            [],
        )
        .unwrap();
    let id = mention_id(&f, &entry, "Priya");
    f.engine.remove_mention(&id).unwrap();
    f.engine.restore_mention(&id).unwrap();
    assert_eq!(
        status_of(&f, "Priya likes tea."),
        "letGo",
        "it was let go before"
    );
    assert_eq!(status_of(&f, "Priya walks to work."), "active");
}

#[test]
fn confirming_a_mention_confirms_this_items_memories_and_keeps_a_role_once() {
    let f = fixture();
    let entry = journal_item(&f, "Priya likes tea.", "entry");
    f.engine
        .apply(
            &entry,
            vec![person(
                "Priya",
                Predicate::Prefers,
                "tea",
                "Priya likes tea.",
            )],
        )
        .unwrap();
    let id = mention_id(&f, &entry, "Priya");
    let m = f
        .engine
        .confirm_mention(&id, Some("a friend from climbing"))
        .unwrap();
    assert_eq!(m.state, MentionState::Confirmed);
    assert_eq!(m.role.as_deref(), Some("a friend from climbing"));
    let (confirmed, used): (bool, Option<String>) = conn(&f)
        .query_row("SELECT confirmed, last_used FROM memories", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert!(confirmed && used.is_some());
    let m = f.engine.confirm_mention(&id, Some("my boss")).unwrap();
    assert_eq!(m.role.as_deref(), Some("a friend from climbing"));
    f.engine.remove_mention(&id).unwrap();
    assert!(f.engine.confirm_mention(&id, None).is_err());
}

#[test]
fn refiling_a_mention_moves_only_this_items_memories_and_the_link() {
    let f = fixture();
    let a = journal_item(&f, "Sam likes tea.", "entry");
    f.engine
        .apply(
            &a,
            vec![person("Sam", Predicate::Prefers, "tea", "Sam likes tea.")],
        )
        .unwrap();
    let b = keep(&f.engine, "Sam lives in Leeds.");
    f.engine
        .apply(
            &b,
            vec![person(
                "Sam",
                Predicate::LivesIn,
                "Leeds",
                "Sam lives in Leeds.",
            )],
        )
        .unwrap();
    let id = mention_id(&f, &a, "Sam");

    let moved = f
        .engine
        .refile_mention(
            &id,
            RefileTarget::New {
                name: "Samir".into(),
                kind: EntityKind::Person,
            },
        )
        .unwrap();
    assert_eq!(moved.name, "Samir");
    assert!(moved.new);
    assert_eq!(moved.memory_count, 1);
    let under = |sentence: &str| -> String {
        conn(&f)
            .query_row(
                "SELECT e.name FROM memories m JOIN entities e ON e.id = m.entity_id \
                 WHERE m.sentence = ?1",
                [sentence],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(
        under("Sam likes tea."),
        "Samir",
        "its words stay as they were"
    );
    assert_eq!(under("Sam lives in Leeds."), "Sam");
    assert_eq!(mentions(&f, &a)[0].0, "Samir");
    assert!(
        f.engine
            .refile_mention(
                &id,
                RefileTarget::New {
                    name: "Samir".into(),
                    kind: EntityKind::Person
                }
            )
            .is_err()
    );

    let sam: String = conn(&f)
        .query_row("SELECT id FROM entities WHERE name = 'Sam'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let back = f
        .engine
        .refile_mention(&id, RefileTarget::Existing { entity_id: sam })
        .unwrap();
    assert_eq!(back.name, "Sam");
    assert_eq!(under("Sam likes tea."), "Sam");
}

fn save_page(f: &Fixture, day: &str) {
    let page = f
        .engine
        .compose_page(common::date(day), Edition::Screen)
        .unwrap();
    f.engine.save_page(&page).unwrap();
}

fn page_for(f: &Fixture, day: &str, edition: Edition) -> nocturne_contract::PageView {
    f.engine
        .page_for(Some(common::date(day)), edition)
        .unwrap()
        .unwrap()
}

fn texts(page: &nocturne_contract::PageView) -> Vec<String> {
    page.needs_you
        .iter()
        .chain(&page.changed)
        .chain(&page.learned)
        .chain(&page.upcoming)
        .chain(&page.let_go)
        .map(|l| l.text.clone())
        .collect()
}

fn promise(f: &Fixture, text: &str, role: Option<&str>, what: &str) {
    let item = match role {
        Some(role) => journal_item(f, text, role),
        None => keep(&f.engine, text),
    };
    f.engine
        .apply(
            &item,
            vec![cand(
                "me",
                Predicate::Promised,
                what,
                &format!("You promised {what}."),
            )],
        )
        .unwrap();
}

#[test]
fn the_spoken_page_skips_a_journal_line_and_counts_it() {
    let f = fixture();
    promise(&f, "I will water the plants.", None, "to water the plants");
    promise(
        &f,
        "I told Priya I would call.",
        Some("entry"),
        "to call Priya",
    );
    save_page(&f, "2026-11-14");

    let screen = page_for(&f, "2026-11-14", Edition::Screen);
    assert_eq!(texts(&screen).len(), 2);
    let spoken = page_for(&f, "2026-11-14", Edition::Audio);
    let said = texts(&spoken);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("water the plants"));
    assert_eq!(spoken.journal_skipped, Some(1));
    assert_eq!(spoken.private_skipped, 0);

    let printed = page_for(&f, "2026-11-14", Edition::Print);
    assert_eq!(texts(&printed).len(), 2, "print keeps the journal");
    assert_eq!(printed.journal_skipped, None);
}

#[test]
fn the_spoken_page_skips_a_question_about_a_journal_memory() {
    let f = fixture();
    promise(
        &f,
        "I told Priya I would call.",
        Some("entry"),
        "to call Priya",
    );
    let mut page = f
        .engine
        .compose_page(common::date("2026-11-14"), Edition::Screen)
        .unwrap();
    let memory_id: String = conn(&f)
        .query_row("SELECT id FROM memories", [], |r| r.get(0))
        .unwrap();
    page.question = Some(nocturne_contract::Question {
        kind: nocturne_contract::QuestionKind::YesNo,
        text: "Did you call Priya?".into(),
        memory_id: Some(memory_id),
        receipt: None,
        last_asked: None,
        answered: None,
    });
    f.engine.save_page(&page).unwrap();
    assert!(
        page_for(&f, "2026-11-14", Edition::Screen)
            .question
            .is_some()
    );
    let spoken = page_for(&f, "2026-11-14", Edition::Audio);
    assert!(spoken.question.is_none());
    assert_eq!(spoken.journal_skipped, Some(2), "the line and its question");
}

#[test]
fn a_memory_keeps_the_journal_mark_through_its_versions_in_the_spoken_page() {
    let f = fixture();
    let entry = journal_item(&f, "Priya lives in Leeds.", "entry");
    f.engine
        .apply(
            &entry,
            vec![cand(
                "Priya",
                Predicate::LivesIn,
                "Leeds",
                "Priya lives in Leeds.",
            )],
        )
        .unwrap();
    let later = keep(&f.engine, "Priya moved to York.");
    f.engine
        .apply(
            &later,
            vec![cand(
                "Priya",
                Predicate::LivesIn,
                "York",
                "Priya lives in York.",
            )],
        )
        .unwrap();
    save_page(&f, "2026-11-14");
    let screen = texts(&page_for(&f, "2026-11-14", Edition::Screen));
    assert!(!screen.is_empty());
    let spoken = page_for(&f, "2026-11-14", Edition::Audio);
    assert!(texts(&spoken).is_empty(), "{:?}", texts(&spoken));
    assert!(spoken.journal_skipped.unwrap() > 0);
}

#[test]
fn a_journal_entrys_memory_is_not_withheld_from_a_phone() {
    let f = fixture();
    promise(
        &f,
        "I told Priya I would call.",
        Some("entry"),
        "to call Priya",
    );
    save_page(&f, "2026-11-14");
    let phone = page_for(&f, "2026-11-14", Edition::Phone);
    assert_eq!(texts(&phone).len(), 1);
}

fn reflect(f: &Fixture, kind: &str, start: &str, end: &str, written: &str, so_far: bool) {
    conn(f)
        .execute(
            "INSERT INTO reflections(id, kind, start, end, written_at, written, so_far, \
             from_count, body) VALUES (?1, ?2, ?3, ?4, '2026-11-14T02:00:00Z', ?5, ?6, 3, '{}')",
            rusqlite::params![
                format!("ref_{kind}_{start}"),
                kind,
                start,
                end,
                written,
                so_far
            ],
        )
        .unwrap();
}

fn line_of(f: &Fixture, edition: Edition) -> Option<nocturne_contract::Period> {
    page_for(f, "2026-11-14", edition).journal_line
}

#[test]
fn the_page_gets_a_quiet_line_after_an_overnight_day_reflection() {
    let f = fixture();
    save_page(&f, "2026-11-14");
    assert_eq!(line_of(&f, Edition::Screen), None);
    reflect(&f, "day", "2026-11-13", "2026-11-13", "overnight", false);
    let day = nocturne_contract::Period {
        kind: PeriodKind::Day,
        start: "2026-11-13".into(),
        end: "2026-11-13".into(),
    };
    assert_eq!(line_of(&f, Edition::Screen), Some(day.clone()));
    assert_eq!(line_of(&f, Edition::Print), Some(day.clone()));
    assert_eq!(line_of(&f, Edition::Phone), Some(day));
    assert_eq!(line_of(&f, Edition::Audio), None, "never read aloud");
}

#[test]
fn a_so_far_or_on_demand_or_older_reflection_makes_no_quiet_line() {
    let f = fixture();
    save_page(&f, "2026-11-14");
    reflect(&f, "day", "2026-11-13", "2026-11-13", "overnight", true);
    assert_eq!(line_of(&f, Edition::Screen), None);
    conn(&f).execute("DELETE FROM reflections", []).unwrap();
    reflect(&f, "day", "2026-11-13", "2026-11-13", "onDemand", false);
    assert_eq!(line_of(&f, Edition::Screen), None);
    conn(&f).execute("DELETE FROM reflections", []).unwrap();
    reflect(&f, "day", "2026-11-12", "2026-11-12", "overnight", false);
    assert_eq!(line_of(&f, Edition::Screen), None);
}

#[test]
fn a_recap_that_ended_yesterday_comes_before_the_day() {
    let f = fixture();
    save_page(&f, "2026-11-14");
    reflect(&f, "day", "2026-11-13", "2026-11-13", "overnight", false);
    reflect(&f, "week", "2026-11-07", "2026-11-13", "overnight", false);
    let line = line_of(&f, Edition::Screen).unwrap();
    assert_eq!(line.kind, PeriodKind::Week);
    assert_eq!(line.start, "2026-11-07");
}

#[test]
fn the_journal_day_carries_its_open_same_name_question() {
    let f = fixture();
    two_sams(&f);
    let entry = journal_item(&f, SAM_ENTRY, "entry");
    f.engine.apply(&entry, vec![sam_claim()]).unwrap();
    let before = f.engine.status_counts().unwrap().needs_you;
    let day = f.engine.journal_day("2026-11-12".parse().unwrap()).unwrap();
    assert_eq!(day.same_name.len(), 1);
    let question = day.same_name[0].id.clone();
    f.engine
        .answer_same_name(&question, nocturne_contract::SameNameAnswer::Leave)
        .unwrap();
    assert_eq!(f.engine.status_counts().unwrap().needs_you, before - 1);
    assert_eq!(day.same_name[0].receipt.item_id, entry);
    assert!(
        f.engine
            .journal_day("2026-11-11".parse().unwrap())
            .unwrap()
            .same_name
            .is_empty()
    );
}
