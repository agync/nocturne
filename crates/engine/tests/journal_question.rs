//! The page's question from the journal: the rule that picks it (one test per condition), the
//! week rule, the pick order, saving and reading it back, what an answer keeps, and the line
//! added to an answer.

mod common;

use common::{EngineExt, Fixture, cand, date, fixture, keep};
use nocturne_contract::{
    Edition, EntityKind, ItemStatus, JournalRole, PageView, QuestionChoice, QuestionKind, RowKind,
    Span,
};
use nocturne_engine::{Candidate, Predicate};
use rusqlite::params;

const ENTRY: &str = "Parents\u{2019} evening at six. Not looking forward to it.";
const SUBJECT: &str = "Parents evening";
const PAGE: &str = "2026-11-14";
const EVENT: &str = "2026-11-13";

fn conn(f: &Fixture) -> rusqlite::Connection {
    let c = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    c.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    c
}

fn sql(f: &Fixture, sql: &str, args: impl rusqlite::Params) {
    conn(f).execute(sql, args).unwrap();
}

fn compose(f: &Fixture, day: &str) -> PageView {
    f.engine.compose_page(date(day), Edition::Screen).unwrap()
}

fn save_day(f: &Fixture, day: &str) -> PageView {
    let page = compose(f, day);
    f.engine.save_page(&page).unwrap();
    page
}

struct World {
    f: Fixture,
    block: String,
    memory: String,
}

fn scheduled(subject: &str, on: &str, quote: &str) -> Candidate {
    let mut c = cand(
        subject,
        Predicate::Scheduled,
        "at six",
        &format!("{subject} is at six."),
    );
    c.subject_kind = Some(EntityKind::Thing);
    c.due = Some(date(on));
    c.span = Some(Span::Quote { text: quote.into() });
    c
}

/// A scheduled memory the reader gave no day to.
fn undated(subject: &str, quote: &str) -> Candidate {
    let mut c = scheduled(subject, EVENT, quote);
    c.due = None;
    c
}

/// An entry block written on `day`, read, with one scheduled memory on `on`.
fn read_block(
    f: &Fixture,
    day: &str,
    text: &str,
    subject: &str,
    on: &str,
    quote: &str,
) -> (String, String) {
    let block = f
        .engine
        .save_entry_text(date(day), None, 0, text)
        .unwrap()
        .unwrap()
        .item
        .id;
    f.engine.release_entry_holds().unwrap();
    let report = f
        .engine
        .apply(&block, vec![scheduled(subject, on, quote)])
        .unwrap();
    let memory = report
        .created
        .first()
        .or(report.reinforced.first())
        .cloned()
        .expect("a memory");
    (block, memory)
}

/// Page 1 is saved, the evening of the 13th has an entry with a scheduled thing that day, and it
/// is the morning of the 14th: everything the rule asks for holds.
fn world_with(text: &str, quote: &str) -> World {
    let f = fixture();
    save_day(&f, "2026-11-10");
    f.clock.set_str("2026-11-13T19:00:00Z");
    let (block, memory) = read_block(&f, EVENT, text, SUBJECT, EVENT, quote);
    f.clock.set_str("2026-11-14T07:00:00Z");
    World { f, block, memory }
}

fn world() -> World {
    world_with(ENTRY, ENTRY)
}

fn asks_from_the_journal(w: &World) -> bool {
    compose(&w.f, PAGE)
        .question
        .is_some_and(|q| q.kind == QuestionKind::Journal)
}

fn no_journal_question(w: &World) {
    assert!(
        !asks_from_the_journal(w),
        "{:?}",
        compose(&w.f, PAGE).question
    );
}

#[test]
fn a_scheduled_thing_from_yesterdays_entry_is_asked_with_the_sentences_whole() {
    let w = world();
    let page = compose(&w.f, PAGE);
    assert_eq!(page.page_number, 2);
    let q = page.question.expect("a question");
    assert_eq!(q.kind, QuestionKind::Journal);
    assert_eq!(q.text, ENTRY, "both sentences, whole, never cut");
    assert_eq!(q.memory_id.as_deref(), Some(w.memory.as_str()));
    assert_eq!(q.receipt.as_ref().unwrap().item_id, w.block);
    assert_eq!(q.answered, None);
    let ask = q.journal.expect("where it came from");
    assert_eq!(ask.item_id, w.block);
    assert!(!ask.voice);
    assert_eq!(
        (ask.written_on.as_str(), ask.event_day.as_str()),
        (EVENT, EVENT)
    );
    assert_eq!((ask.answer_item_id, ask.line), (None, None));
}

#[test]
fn a_voice_note_block_is_asked_as_said() {
    let w = world();
    sql(
        &w.f,
        "UPDATE items SET kind = 'audio' WHERE id = ?1",
        [&w.block],
    );
    let ask = compose(&w.f, PAGE).question.unwrap().journal.unwrap();
    assert!(ask.voice);
}

#[test]
fn a_block_from_days_before_is_asked_when_it_is_within_thirty_days() {
    let w = world();
    sql(
        &w.f,
        "UPDATE items SET metadata = json_set(metadata, '$.journal_day', '2026-10-14') \
         WHERE id = ?1",
        [&w.block],
    );
    let ask = compose(&w.f, PAGE).question.unwrap().journal.unwrap();
    assert_eq!(ask.written_on, "2026-10-14", "30 days before the event");
    sql(
        &w.f,
        "UPDATE items SET metadata = json_set(metadata, '$.journal_day', '2026-10-13') \
         WHERE id = ?1",
        [&w.block],
    );
    no_journal_question(&w);
}

// ---- one test per condition: flip just that one ------------------------------------------------

#[test]
fn condition_1_a_memory_that_is_not_scheduled_is_not_asked() {
    let w = world();
    sql(
        &w.f,
        "UPDATE memories SET predicate = 'happened' WHERE id = ?1",
        [&w.memory],
    );
    no_journal_question(&w);
}

#[test]
fn condition_2_a_date_other_than_the_day_looked_back_on_is_not_asked() {
    for on in ["2026-11-12", "2026-11-14"] {
        let w = world();
        sql(
            &w.f,
            "UPDATE memories SET due = ?2 WHERE id = ?1",
            params![w.memory, on],
        );
        no_journal_question(&w);
    }
}

#[test]
fn condition_2_a_scheduled_memory_stored_the_way_the_reader_stores_it_is_asked() {
    let w = world();
    let (due, from): (Option<String>, Option<String>) = conn(&w.f)
        .query_row(
            "SELECT due, valid_from FROM memories WHERE id = ?1",
            [&w.memory],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((due.as_deref(), from), (Some(EVENT), None));
    assert!(asks_from_the_journal(&w));
}

#[test]
fn condition_2_a_day_in_valid_from_alone_is_the_day_too() {
    let w = world();
    sql(
        &w.f,
        "UPDATE memories SET due = NULL, valid_from = ?2 WHERE id = ?1",
        params![w.memory, EVENT],
    );
    assert!(asks_from_the_journal(&w));
    sql(
        &w.f,
        "UPDATE memories SET valid_from = '2026-11-12' WHERE id = ?1",
        [&w.memory],
    );
    no_journal_question(&w);
}

fn undated_world(day: &str) -> World {
    let f = fixture();
    save_day(&f, "2026-11-10");
    f.clock.set_str(&format!("{day}T19:00:00Z"));
    let block = f
        .engine
        .save_entry_text(date(day), None, 0, ENTRY)
        .unwrap()
        .unwrap()
        .item
        .id;
    f.engine.release_entry_holds().unwrap();
    let report = f
        .engine
        .apply(&block, vec![undated(SUBJECT, ENTRY)])
        .unwrap();
    let memory = report.created[0].clone();
    f.clock.set_str("2026-11-14T07:00:00Z");
    World { f, block, memory }
}

#[test]
fn condition_2_a_scheduled_memory_with_no_day_is_dated_by_the_day_its_block_was_kept() {
    let w = undated_world(EVENT);
    let (due, from): (Option<String>, Option<String>) = conn(&w.f)
        .query_row(
            "SELECT due, valid_from FROM memories WHERE id = ?1",
            [&w.memory],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((due, from), (None, None));
    let q = compose(&w.f, PAGE).question.expect("a question");
    assert_eq!(q.kind, QuestionKind::Journal);
    assert_eq!(q.text, ENTRY);
    let ask = q.journal.unwrap();
    assert_eq!(
        (ask.written_on.as_str(), ask.event_day.as_str()),
        (EVENT, EVENT)
    );
}

#[test]
fn condition_2_a_scheduled_memory_with_no_day_is_not_asked_from_another_days_block() {
    let w = undated_world("2026-11-12");
    no_journal_question(&w);
    let w = undated_world(EVENT);
    sql(
        &w.f,
        "UPDATE items SET metadata = json_set(metadata, '$.journal_day', '2026-11-12') \
         WHERE id = ?1",
        [&w.block],
    );
    no_journal_question(&w);
}

#[test]
fn condition_3_only_a_text_or_voice_block_of_an_entry_qualifies() {
    for role in ["checkin", "good"] {
        let w = world();
        sql(
            &w.f,
            "UPDATE items SET metadata = json_set(metadata, '$.journal', ?2) WHERE id = ?1",
            params![w.block, role],
        );
        no_journal_question(&w);
    }
    let w = world();
    sql(
        &w.f,
        "UPDATE items SET metadata = json_remove(metadata, '$.journal') WHERE id = ?1",
        [&w.block],
    );
    no_journal_question(&w);
    let w = world();
    sql(
        &w.f,
        "UPDATE items SET kind = 'image' WHERE id = ?1",
        [&w.block],
    );
    no_journal_question(&w);
    for source in ["quickCapture", "phone"] {
        let w = world();
        sql(
            &w.f,
            "UPDATE items SET source = ?2, metadata = json_remove(metadata, '$.journal') \
             WHERE id = ?1",
            params![w.block, source],
        );
        no_journal_question(&w);
    }
    for kind in ["document", "web"] {
        let w = world();
        sql(
            &w.f,
            "UPDATE items SET kind = ?2, metadata = json_remove(metadata, '$.journal') \
             WHERE id = ?1",
            params![w.block, kind],
        );
        no_journal_question(&w);
    }
}

#[test]
fn condition_3_a_block_kept_after_the_event_or_over_thirty_days_before_is_not_asked() {
    let w = world();
    sql(
        &w.f,
        "UPDATE items SET metadata = json_set(metadata, '$.journal_day', '2026-11-14') \
         WHERE id = ?1",
        [&w.block],
    );
    no_journal_question(&w);
    let w = world();
    sql(
        &w.f,
        "UPDATE items SET metadata = json_set(metadata, '$.journal_day', '2026-10-12') \
         WHERE id = ?1",
        [&w.block],
    );
    no_journal_question(&w);
}

#[test]
fn condition_4_sentences_of_fewer_than_three_or_more_than_twenty_words_are_not_asked() {
    let w = world_with("Parents evening. Dull.", "Parents evening.");
    assert!(!asks_from_the_journal(&w), "two words");
    let long = "Parents evening is at six and I have to bring the form the folder the report \
                and the cheque for the trip before then.";
    let w = world_with(long, "Parents evening is at six");
    assert!(!asks_from_the_journal(&w), "more than twenty words");
    let twenty = "Parents evening is at six and I have to bring the form the folder the report and the cheque too.";
    assert_eq!(twenty.split_whitespace().count(), 20);
    let w = world_with(twenty, "Parents evening is at six");
    assert!(asks_from_the_journal(&w), "twenty words are fine");
    let w = world_with("Parents evening at six.", "Parents evening at six.");
    assert!(asks_from_the_journal(&w), "three words are fine");
}

#[test]
fn condition_4_the_sentences_the_span_touches_are_taken_whole() {
    let w = world_with(
        "Long day. Parents\u{2019} evening at six. Not looking forward to it. Bed early.",
        "evening at six",
    );
    let q = compose(&w.f, PAGE).question.unwrap();
    assert_eq!(q.text, "Parents\u{2019} evening at six.");
    let w = world_with(
        "Long day. Parents\u{2019} evening at six. Not looking forward to it. Bed early.",
        "at six. Not looking",
    );
    assert_eq!(
        compose(&w.f, PAGE).question.unwrap().text,
        "Parents\u{2019} evening at six. Not looking forward to it."
    );
}

#[test]
fn condition_4_a_block_that_was_replaced_is_not_asked_and_a_span_not_in_it_is_not_either() {
    let w = world();
    let newer = keep(&w.f.engine, "Parents\u{2019} evening was moved.");
    sql(
        &w.f,
        "UPDATE items SET metadata = json_set(metadata, '$.journal', 'entry', \
         '$.journal_day', ?3, '$.replaces', ?2) WHERE id = ?1",
        params![newer, w.block, EVENT],
    );
    no_journal_question(&w);
    let w = world_with(ENTRY, "Something the block never said at all");
    assert!(!asks_from_the_journal(&w));
}

#[test]
fn condition_5_a_later_item_that_names_the_subject_stops_the_question() {
    let w = world();
    w.f.clock.set_str("2026-11-13T21:00:00Z");
    keep(
        &w.f.engine,
        "Parents evening went fine, the teacher was kind.",
    );
    w.f.clock.set_str("2026-11-14T07:00:00Z");
    no_journal_question(&w);
}

#[test]
fn condition_5_a_later_mention_of_the_subject_stops_it_without_the_name_in_the_text() {
    let w = world();
    w.f.clock.set_str("2026-11-13T21:00:00Z");
    let later = keep(&w.f.engine, "It went fine, the teacher was kind.");
    let mut c = cand(SUBJECT, Predicate::Happened, "went fine", "It went fine.");
    c.subject_kind = Some(EntityKind::Thing);
    w.f.engine.apply(&later, vec![c]).unwrap();
    let n: i64 = conn(&w.f)
        .query_row(
            "SELECT COUNT(*) FROM mentions WHERE item_id = ?1",
            [&later],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "the reading mentioned the subject");
    w.f.clock.set_str("2026-11-14T07:00:00Z");
    no_journal_question(&w);
}

#[test]
fn condition_5_what_was_kept_before_the_block_or_after_the_event_day_does_not_count() {
    let w = world();
    w.f.clock.set_str("2026-11-12T21:00:00Z");
    keep(&w.f.engine, "Parents evening is soon, the form is signed.");
    sql(
        &w.f,
        "UPDATE items SET kept_at = '2026-11-13T08:00:00.000Z' WHERE text LIKE 'Parents evening is soon%'",
        [],
    );
    w.f.clock.set_str("2026-11-14T06:00:00Z");
    keep(
        &w.f.engine,
        "Parents evening again, written the next morning.",
    );
    w.f.clock.set_str("2026-11-14T07:00:00Z");
    assert!(
        asks_from_the_journal(&w),
        "before the block and after the event's day"
    );
}

#[test]
fn condition_6_a_memory_that_is_not_active_is_not_asked() {
    for status in ["unconfirmed", "fading", "letGo"] {
        let w = world();
        sql(
            &w.f,
            "UPDATE memories SET status = ?2 WHERE id = ?1",
            params![w.memory, status],
        );
        no_journal_question(&w);
    }
}

#[test]
fn condition_6_a_safety_memory_is_not_asked() {
    for tag in ["health", "Allergy", "medication"] {
        let w = world();
        sql(
            &w.f,
            "UPDATE memories SET tags = ?2 WHERE id = ?1",
            params![w.memory, format!("[\"{tag}\"]")],
        );
        no_journal_question(&w);
    }
}

/// Nocturne does not judge what a memory is about: a money, passport or insurance label is no
/// reason to skip it.
#[test]
fn a_memory_labelled_money_or_id_is_asked() {
    for tag in ["money", "passport", "insurance", "appointment"] {
        let w = world();
        sql(
            &w.f,
            "UPDATE memories SET tags = ?2 WHERE id = ?1",
            params![w.memory, format!("[\"{tag}\"]")],
        );
        assert!(asks_from_the_journal(&w), "{tag}");
    }
}

#[test]
fn condition_6_a_memory_asked_before_on_any_page_is_not_asked() {
    let w = world();
    sql(
        &w.f,
        "INSERT INTO questions(page_date, memory_id, kind, text, asked_at) \
         VALUES ('2026-10-01', ?1, 'yesNo', 'Parents evening is at six.', '2026-10-01T07:00:00.000Z')",
        [&w.memory],
    );
    no_journal_question(&w);
}

#[test]
fn never_a_block_left_out_of_reflections() {
    let w = world();
    w.f.engine.set_left_out(&w.block, true).unwrap();
    no_journal_question(&w);
}

#[test]
fn never_anything_private_or_with_a_private_source() {
    let w = world();
    sql(
        &w.f,
        "UPDATE memories SET private = 1 WHERE id = ?1",
        [&w.memory],
    );
    no_journal_question(&w);

    let w = world();
    let secret = keep(&w.f.engine, "Parents evening is at six, said privately.");
    sql(
        &w.f,
        "UPDATE items SET private = 1 WHERE id = ?1",
        [&secret],
    );
    sql(
        &w.f,
        "INSERT INTO memory_sources(memory_id, item_id, span) VALUES (?1, ?2, NULL)",
        params![w.memory, secret],
    );
    no_journal_question(&w);
}

/// A memory or sentence about money or health is asked about like any other.
#[test]
fn a_memory_or_sentence_about_money_or_health_is_asked() {
    let w = world();
    sql(
        &w.f,
        "UPDATE memories SET sentence = 'Parents evening is at six, and the dentist bill is \u{a3}40.' \
         WHERE id = ?1",
        [&w.memory],
    );
    assert!(asks_from_the_journal(&w), "an amount of money");

    let text = "Parents evening at six. Then the doctor about my medication.";
    let w = world_with(text, "Parents evening at six. Then the doctor");
    assert!(asks_from_the_journal(&w), "words about health");
}

#[test]
fn never_a_promise_or_a_due_date() {
    for predicate in ["promised", "due"] {
        let w = world();
        sql(
            &w.f,
            "UPDATE memories SET predicate = ?2 WHERE id = ?1",
            params![w.memory, predicate],
        );
        no_journal_question(&w);
    }
}

#[test]
fn never_a_check_in_or_a_good_thing_or_a_memory_read_from_one() {
    let w = world();
    let thing =
        w.f.engine
            .keep_good_thing(date(EVENT), ENTRY, None, nocturne_contract::Source::Journal)
            .unwrap();
    sql(
        &w.f,
        "UPDATE memory_sources SET item_id = ?2 WHERE memory_id = ?1",
        params![w.memory, thing.id],
    );
    no_journal_question(&w);
}

// ---- when it is asked --------------------------------------------------------------------------

#[test]
fn never_on_the_first_page() {
    let f = fixture();
    f.clock.set_str("2026-11-13T19:00:00Z");
    read_block(&f, EVENT, ENTRY, SUBJECT, EVENT, ENTRY);
    f.clock.set_str("2026-11-14T07:00:00Z");
    let q = compose(&f, PAGE).question.unwrap();
    assert_eq!(compose(&f, PAGE).page_number, 1);
    assert_eq!(q.kind, QuestionKind::PageTime);
}

fn pages_saved(f: &Fixture, days: &[&str]) {
    for d in days {
        f.clock.set_str(&format!("{d}T12:00:00Z"));
        save_day(f, d);
    }
}

fn week_world(journal_asked_on: &str) -> World {
    let f = fixture();
    pages_saved(
        &f,
        &[
            "2026-11-04",
            "2026-11-05",
            "2026-11-06",
            "2026-11-07",
            "2026-11-08",
            "2026-11-09",
            "2026-11-10",
        ],
    );
    sql(
        &f,
        "INSERT OR REPLACE INTO questions(page_date, memory_id, kind, text, asked_at) \
         VALUES (?1, NULL, 'journal', 'an earlier sentence', '2026-11-01T07:00:00.000Z')",
        [journal_asked_on],
    );
    f.clock.set_str("2026-11-13T19:00:00Z");
    let (block, memory) = read_block(&f, EVENT, ENTRY, SUBJECT, EVENT, ENTRY);
    f.clock.set_str("2026-11-14T07:00:00Z");
    World { f, block, memory }
}

#[test]
fn at_most_once_in_seven_days_the_six_pages_before_decide() {
    // The six saved pages before the 14th are the 10th and back to the 5th.
    for blocked in ["2026-11-10", "2026-11-05"] {
        let w = week_world(blocked);
        no_journal_question(&w);
    }
    assert!(
        asks_from_the_journal(&week_world("2026-11-04")),
        "the seventh page back does not count"
    );
    let f = fixture();
    pages_saved(
        &f,
        &[
            "2026-11-03",
            "2026-11-04",
            "2026-11-05",
            "2026-11-06",
            "2026-11-07",
            "2026-11-08",
            "2026-11-10",
        ],
    );
    sql(
        &f,
        "INSERT OR REPLACE INTO questions(page_date, memory_id, kind, text, asked_at) \
         VALUES ('2026-11-03', NULL, 'journal', 'an earlier sentence', '2026-11-03T07:00:00.000Z')",
        [],
    );
    f.clock.set_str("2026-11-13T19:00:00Z");
    read_block(&f, EVENT, ENTRY, SUBJECT, EVENT, ENTRY);
    f.clock.set_str("2026-11-14T07:00:00Z");
    assert_eq!(
        compose(&f, PAGE).question.unwrap().kind,
        QuestionKind::Journal,
        "the seventh page back does not count"
    );
}

#[test]
fn a_yes_no_question_in_the_week_does_not_block_a_journal_question() {
    let w = week_world("2026-10-01");
    sql(
        &w.f,
        "INSERT INTO questions(page_date, memory_id, kind, text, asked_at) \
         VALUES ('2026-11-09', NULL, 'yesNo', 'Leo likes swimming.', '2026-11-09T07:00:00.000Z')",
        [],
    );
    assert!(asks_from_the_journal(&w));
}

#[test]
fn the_journal_question_takes_the_slot_of_the_usual_one() {
    let w = world();
    learn_usual(&w);
    let q = compose(&w.f, PAGE).question.unwrap();
    assert_eq!(q.kind, QuestionKind::Journal);
}

fn learn_usual(w: &World) {
    common::learn(
        &w.f.engine,
        "Leo likes swimming.",
        cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming."),
    );
}

#[test]
fn an_unanswered_journal_question_stays_and_the_next_page_asks_as_usual() {
    let w = world();
    learn_usual(&w);
    save_day(&w.f, PAGE);
    w.f.clock.set_str("2026-11-15T07:00:00Z");
    let next = compose(&w.f, "2026-11-15").question.unwrap();
    assert_eq!(next.kind, QuestionKind::YesNo, "the week is taken");
    let kept = w.f.engine.page(Some(date(PAGE))).unwrap().unwrap();
    assert_eq!(kept.question.unwrap().kind, QuestionKind::Journal);
}

// ---- one pick, once per memory -----------------------------------------------------------------

#[test]
fn one_pick_newest_block_first_then_earliest_time_then_memory_id() {
    let w = world();
    // A second block, kept later the same evening, with a second thing that day.
    w.f.clock.set_str("2026-11-13T20:00:00Z");
    let (later_block, later_memory) = read_block(
        &w.f,
        EVENT,
        "Swimming at nine. Leo is excited.",
        "Swimming",
        EVENT,
        "Swimming at nine. Leo is excited.",
    );
    w.f.clock.set_str("2026-11-14T07:00:00Z");
    let q = compose(&w.f, PAGE).question.unwrap();
    assert_eq!(
        q.memory_id.as_deref(),
        Some(later_memory.as_str()),
        "newest block"
    );
    assert_eq!(q.journal.unwrap().item_id, later_block);

    // Two things in one block: the one at the earlier time of day wins, whatever the ids.
    let f = fixture();
    save_day(&f, "2026-11-10");
    f.clock.set_str("2026-11-13T19:00:00Z");
    let text = "Dinner at 7pm with Gran. Dentist at 9am, ugh.";
    let block = f
        .engine
        .save_entry_text(date(EVENT), None, 0, text)
        .unwrap()
        .unwrap()
        .item
        .id;
    f.engine.release_entry_holds().unwrap();
    let mut dinner = scheduled("Dinner", EVENT, "Dinner at 7pm with Gran.");
    dinner.object = "7pm".into();
    let mut dentist = scheduled("Dentist", EVENT, "Dentist at 9am, ugh.");
    dentist.object = "9am".into();
    let report = f.engine.apply(&block, vec![dinner, dentist]).unwrap();
    assert_eq!(report.created.len(), 2);
    f.clock.set_str("2026-11-14T07:00:00Z");
    let q = compose(&f, PAGE).question.unwrap();
    assert_eq!(q.text, "Dentist at 9am, ugh.", "9am is before 7pm");

    // The same time (none given): the smaller memory id.
    let g = fixture();
    save_day(&g, "2026-11-10");
    g.clock.set_str("2026-11-13T19:00:00Z");
    let block = g
        .engine
        .save_entry_text(
            date(EVENT),
            None,
            0,
            "Choir practice at the hall. Then a quiet bath tonight.",
        )
        .unwrap()
        .unwrap()
        .item
        .id;
    g.engine.release_entry_holds().unwrap();
    let report = g
        .engine
        .apply(
            &block,
            vec![
                scheduled("Choir", EVENT, "Choir practice at the hall."),
                scheduled("Bath", EVENT, "Then a quiet bath tonight."),
            ],
        )
        .unwrap();
    assert_eq!(report.created.len(), 2);
    let smaller = report.created.iter().min().unwrap().clone();
    g.clock.set_str("2026-11-14T07:00:00Z");
    assert_eq!(
        compose(&g, PAGE).question.unwrap().memory_id.as_deref(),
        Some(smaller.as_str())
    );
}

#[test]
fn a_memory_is_asked_once_whatever_came_of_it() {
    let w = world();
    save_day(&w.f, PAGE);
    // Six pages go by, then the same memory is dated again and another qualifies.
    pages_saved(
        &w.f,
        &[
            "2026-11-15",
            "2026-11-16",
            "2026-11-17",
            "2026-11-18",
            "2026-11-19",
            "2026-11-20",
        ],
    );
    sql(
        &w.f,
        "UPDATE memories SET due = '2026-11-23' WHERE id = ?1",
        [&w.memory],
    );
    w.f.clock.set_str("2026-11-23T19:00:00Z");
    let (_, other) = read_block(
        &w.f,
        "2026-11-23",
        "Swimming at nine. Leo is excited.",
        "Swimming",
        "2026-11-23",
        "Swimming at nine. Leo is excited.",
    );
    w.f.clock.set_str("2026-11-24T07:00:00Z");
    let q = compose(&w.f, "2026-11-24").question.unwrap();
    assert_eq!(q.kind, QuestionKind::Journal);
    assert_eq!(
        q.memory_id.as_deref(),
        Some(other.as_str()),
        "not the one asked on the 14th"
    );
}

#[test]
fn a_memory_asked_from_the_journal_is_not_asked_again_as_yes_no() {
    let w = world();
    save_day(&w.f, PAGE);
    w.f.clock.set_str("2026-12-30T07:00:00Z");
    let q = compose(&w.f, "2026-12-30").question;
    assert!(
        q.as_ref()
            .is_none_or(|q| q.memory_id.as_deref() != Some(w.memory.as_str())),
        "{q:?}"
    );
}

// ---- saved and read back -----------------------------------------------------------------------

#[test]
fn a_saved_journal_question_reads_back_whole() {
    let w = world();
    let composed = save_day(&w.f, PAGE);
    let saved = w.f.engine.page(Some(date(PAGE))).unwrap().unwrap();
    let q = saved.question.unwrap();
    assert_eq!(q.kind, QuestionKind::Journal);
    assert_eq!(q.text, ENTRY);
    assert_eq!(q.journal, composed.question.unwrap().journal);
    let row: (String, String, String) = conn(&w.f)
        .query_row(
            "SELECT kind, text, journal FROM questions WHERE page_date = ?1",
            [PAGE],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((row.0.as_str(), row.1.as_str()), ("journal", ENTRY));
    let json: serde_json::Value = serde_json::from_str(&row.2).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"itemId": w.block, "voice": false, "writtenOn": EVENT, "eventDay": EVENT})
    );
    let again =
        w.f.engine
            .page_for(Some(date(PAGE)), Edition::Print)
            .unwrap()
            .unwrap();
    assert_eq!(
        again.question.unwrap().kind,
        QuestionKind::Journal,
        "prints like any"
    );
}

#[test]
fn the_audio_edition_keeps_the_kind_and_none_of_the_words() {
    let w = world();
    save_day(&w.f, PAGE);
    let page =
        w.f.engine
            .page_for(Some(date(PAGE)), Edition::Audio)
            .unwrap()
            .unwrap();
    let q = page
        .question
        .expect("the reading needs to know there is one");
    assert_eq!(q.kind, QuestionKind::Journal);
    assert_eq!(
        (q.text.as_str(), q.memory_id, q.receipt, q.journal),
        ("", None, None, None),
        "never read aloud"
    );
    let lines = compose(&w.f, PAGE)
        .learned
        .iter()
        .filter(|l| l.memory_ids.contains(&w.memory))
        .count() as u32;
    assert_eq!(
        page.journal_skipped,
        Some(lines),
        "only lines are counted, not the question"
    );
}

#[test]
fn the_phone_page_keeps_what_it_always_did() {
    let w = world();
    save_day(&w.f, PAGE);
    let page =
        w.f.engine
            .page_for(Some(date(PAGE)), Edition::Phone)
            .unwrap()
            .unwrap();
    assert_eq!(page.question.unwrap().kind, QuestionKind::Journal);
}

#[test]
fn the_markdown_export_renders_the_question_in_words() {
    let w = world();
    save_day(&w.f, PAGE);
    let out = tempfile::tempdir().unwrap();
    let root = std::path::PathBuf::from(w.f.engine.export_to(out.path()).unwrap().path);
    let page = std::fs::read_to_string(root.join("pages").join(format!("{PAGE}.md"))).unwrap();
    assert!(page.contains("## Question"), "{page}");
    assert!(
        page.contains(&format!(
            "Yesterday you wrote: \u{201c}{ENTRY}\u{201d} How did it go?"
        )),
        "{page}"
    );
}

// ---- answers -----------------------------------------------------------------------------------

fn answer_items(f: &Fixture) -> Vec<(String, String, String, String)> {
    let c = conn(f);
    let mut stmt = c
        .prepare(
            "SELECT id, status, COALESCE(text, 'NULL'), metadata FROM items \
             WHERE json_extract(metadata, '$.journal') = 'answer' ORDER BY rowid",
        )
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn last_used(f: &Fixture, memory: &str) -> Option<String> {
    conn(f)
        .query_row(
            "SELECT last_used FROM memories WHERE id = ?1",
            [memory],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn pressing_fine_keeps_an_answer_on_todays_journal_at_once_and_counts_as_use() {
    let w = world();
    save_day(&w.f, PAGE);
    let before = last_used(&w.f, &w.memory);
    w.f.clock.set_str("2026-11-14T08:15:00Z");
    w.f.engine
        .answer_question(date(PAGE), QuestionChoice::Fine)
        .unwrap();
    let items = answer_items(&w.f);
    assert_eq!(items.len(), 1);
    let (id, status, text, meta) = &items[0];
    assert_eq!((status.as_str(), text.as_str()), ("understood", ""));
    let meta: serde_json::Value = serde_json::from_str(meta).unwrap();
    assert_eq!(meta["journal"], "answer");
    assert_eq!(meta["journal_day"], "2026-11-14");
    assert_eq!(meta["choice"], "fine");
    assert_eq!(meta["about"], w.block.as_str());
    assert_eq!(meta["sentence"], ENTRY);
    assert_eq!(meta["question"], PAGE);
    let item = w.f.engine.get_item(id).unwrap();
    assert_eq!(item.journal, Some(JournalRole::Answer));
    assert_eq!(item.status, ItemStatus::Understood);
    assert_ne!(last_used(&w.f, &w.memory), before);
    assert_eq!(
        last_used(&w.f, &w.memory).as_deref(),
        Some("2026-11-14T08:15:00.000Z")
    );

    let page = w.f.engine.page(Some(date(PAGE))).unwrap().unwrap();
    let q = page.question.unwrap();
    assert_eq!(q.answered, Some(QuestionChoice::Fine));
    let ask = q.journal.unwrap();
    assert_eq!(ask.answer_item_id.as_deref(), Some(id.as_str()));
    assert_eq!(ask.line, None);
    assert!(
        w.f.engine
            .answer_question(date(PAGE), QuestionChoice::NotGreat)
            .is_err(),
        "once"
    );
}

#[test]
fn not_great_counts_as_use_and_not_yet_changes_nothing() {
    let w = world();
    save_day(&w.f, PAGE);
    w.f.clock.set_str("2026-11-14T08:15:00Z");
    w.f.engine
        .answer_question(date(PAGE), QuestionChoice::NotGreat)
        .unwrap();
    assert_eq!(
        last_used(&w.f, &w.memory).as_deref(),
        Some("2026-11-14T08:15:00.000Z")
    );

    let w = world();
    save_day(&w.f, PAGE);
    let before = last_used(&w.f, &w.memory);
    w.f.clock.set_str("2026-11-14T08:15:00Z");
    w.f.engine
        .answer_question(date(PAGE), QuestionChoice::NotYet)
        .unwrap();
    assert_eq!(last_used(&w.f, &w.memory), before);
    let (_, _, _, meta) = &answer_items(&w.f)[0];
    assert!(meta.contains("\"notYet\""), "{meta}");
    let status: String = conn(&w.f)
        .query_row(
            "SELECT status FROM memories WHERE id = ?1",
            [&w.memory],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "active");
}

#[test]
fn only_fine_not_great_and_not_yet_fit_a_journal_question_and_they_fit_nothing_else() {
    for choice in [
        QuestionChoice::Yes,
        QuestionChoice::No,
        QuestionChoice::NotSure,
        QuestionChoice::Right,
        QuestionChoice::Earlier,
        QuestionChoice::Later,
    ] {
        let w = world();
        save_day(&w.f, PAGE);
        assert!(w.f.engine.answer_question(date(PAGE), choice).is_err());
        assert!(answer_items(&w.f).is_empty());
    }
    let f = fixture();
    learn_usual_in(&f);
    save_day(&f, "2026-11-10");
    f.clock.set_str("2026-11-11T07:00:00Z");
    save_day(&f, "2026-11-11");
    for choice in [
        QuestionChoice::Fine,
        QuestionChoice::NotGreat,
        QuestionChoice::NotYet,
    ] {
        assert!(
            f.engine
                .answer_question(date("2026-11-11"), choice)
                .is_err(),
            "{choice:?} on a yes or no question"
        );
    }
    let page_one = fixture();
    save_day(&page_one, "2026-11-10");
    assert!(
        page_one
            .engine
            .answer_question(date("2026-11-10"), QuestionChoice::Fine)
            .is_err()
    );
}

fn learn_usual_in(f: &Fixture) {
    common::learn(
        &f.engine,
        "Leo likes swimming.",
        cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming."),
    );
}

#[test]
fn a_journal_question_on_the_page_that_is_composed_but_not_saved_can_be_answered() {
    let w = world();
    assert!(w.f.engine.page(Some(date(PAGE))).unwrap().is_none());
    w.f.engine
        .answer_question(date(PAGE), QuestionChoice::Fine)
        .unwrap();
    let after = compose(&w.f, PAGE).question.unwrap();
    assert_eq!(after.answered, Some(QuestionChoice::Fine));
    assert!(after.journal.unwrap().answer_item_id.is_some());
    w.f.engine.save_page(&compose(&w.f, PAGE)).unwrap();
    assert_eq!(
        w.f.engine
            .page(Some(date(PAGE)))
            .unwrap()
            .unwrap()
            .question
            .unwrap()
            .answered,
        Some(QuestionChoice::Fine)
    );
}

#[test]
fn a_line_is_added_once_and_read_like_an_entry() {
    let w = world();
    save_day(&w.f, PAGE);
    assert!(
        w.f.engine
            .add_answer_line(date(PAGE), "It went well")
            .is_err(),
        "nothing answered yet"
    );
    w.f.engine
        .answer_question(date(PAGE), QuestionChoice::Fine)
        .unwrap();
    assert!(
        w.f.engine.add_answer_line(date(PAGE), "   ").is_err(),
        "not empty"
    );
    w.f.engine
        .add_answer_line(date(PAGE), "  Priya was there too.  ")
        .unwrap();
    assert!(
        w.f.engine.add_answer_line(date(PAGE), "Another").is_err(),
        "once"
    );
    let (id, status, text, _) = answer_items(&w.f).remove(0);
    assert_eq!(
        (status.as_str(), text.as_str()),
        ("kept", "Priya was there too.")
    );
    let hold: Option<String> = conn(&w.f)
        .query_row("SELECT hold_until FROM items WHERE id = ?1", [&id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(hold, None, "no waiting: the reader picks it up");
    let ask =
        w.f.engine
            .page(Some(date(PAGE)))
            .unwrap()
            .unwrap()
            .question
            .unwrap()
            .journal
            .unwrap();
    assert_eq!(ask.line.as_deref(), Some("Priya was there too."));

    // Read like an entry: its memories carry the journal mark.
    let report =
        w.f.engine
            .apply(
                &id,
                vec![cand(
                    "Priya",
                    Predicate::Knows,
                    "the teacher",
                    "Priya knows the teacher.",
                )],
            )
            .unwrap();
    let marked: bool = conn(&w.f)
        .query_row(
            "SELECT journal FROM memories WHERE id = ?1",
            [&report.created[0]],
            |r| r.get(0),
        )
        .unwrap();
    assert!(marked);
}

#[test]
fn a_line_cannot_be_added_once_the_page_is_replaced() {
    let w = world();
    save_day(&w.f, PAGE);
    w.f.engine
        .answer_question(date(PAGE), QuestionChoice::NotGreat)
        .unwrap();
    w.f.clock.set_str("2026-11-15T07:00:00Z");
    save_day(&w.f, "2026-11-15");
    assert!(w.f.engine.add_answer_line(date(PAGE), "Too late").is_err());
    let (_, status, text, _) = answer_items(&w.f).remove(0);
    assert_eq!((status.as_str(), text.as_str()), ("understood", ""));
}

#[test]
fn a_line_cannot_be_added_to_another_kind_of_question() {
    let f = fixture();
    save_day(&f, "2026-11-10");
    f.engine
        .answer_question(date("2026-11-10"), QuestionChoice::Right)
        .unwrap();
    assert!(
        f.engine
            .add_answer_line(date("2026-11-10"), "Hello")
            .is_err()
    );
    assert!(
        f.engine
            .add_answer_line(date("2026-11-20"), "Hello")
            .is_err()
    );
}

#[test]
fn the_answer_is_a_row_of_its_day_on_the_timeline() {
    let w = world();
    save_day(&w.f, PAGE);
    w.f.clock.set_str("2026-11-14T08:15:00Z");
    w.f.engine
        .answer_question(date(PAGE), QuestionChoice::NotGreat)
        .unwrap();
    let day = w.f.engine.journal_day(date("2026-11-14")).unwrap();
    let rows: Vec<_> = day
        .rows
        .iter()
        .filter(|r| r.kind == RowKind::Answer)
        .collect();
    assert_eq!(rows.len(), 1);
    let answer = rows[0].answer.clone().unwrap();
    assert_eq!(answer.choice, QuestionChoice::NotGreat);
    assert_eq!(answer.line, None);
    assert_eq!(answer.about, w.block);
    assert_eq!(answer.sentence, ENTRY);
    assert_eq!(rows[0].item.journal, Some(JournalRole::Answer));
    assert!(
        w.f.engine
            .journal_days()
            .unwrap()
            .contains(&date("2026-11-14"))
    );

    w.f.engine
        .add_answer_line(date(PAGE), "Long queue.")
        .unwrap();
    let day = w.f.engine.journal_day(date("2026-11-14")).unwrap();
    let row = day.rows.iter().find(|r| r.kind == RowKind::Answer).unwrap();
    assert_eq!(
        row.answer.as_ref().unwrap().line.as_deref(),
        Some("Long queue.")
    );
    assert!(
        day.rows.iter().all(|r| r.kind != RowKind::Entry),
        "an answer is not part of the entry"
    );
}

#[test]
fn an_answer_never_reaches_the_tray_or_a_notification() {
    let w = world();
    save_day(&w.f, PAGE);
    w.f.engine
        .answer_question(date(PAGE), QuestionChoice::Fine)
        .unwrap();
    w.f.engine
        .add_answer_line(date(PAGE), "I will ring Priya.")
        .unwrap();
    let (id, ..) = answer_items(&w.f).remove(0);
    w.f.engine
        .apply(
            &id,
            vec![cand(
                "me",
                Predicate::Promised,
                "Priya a ring",
                "You promised to ring Priya.",
            )],
        )
        .unwrap();
    assert_eq!(w.f.engine.needs_you().unwrap().len(), 1);
    assert!(w.f.engine.needs_you_public().unwrap().is_empty());
}

// ---- a deleted block takes its sentence with it ------------------------------------------------

const NEEDLE: &str = "Not looking forward";

/// Every place the sentence could still be read: the stored rows and what the views show.
fn places_with_the_sentence(f: &Fixture) -> Vec<String> {
    let c = conn(f);
    let mut found = Vec::new();
    for (name, sql) in [
        ("items.metadata", "SELECT metadata FROM items"),
        (
            "items.text",
            "SELECT text FROM items WHERE text IS NOT NULL",
        ),
        ("questions.text", "SELECT text FROM questions"),
        ("pages.body", "SELECT body FROM pages"),
        ("page_lines.text", "SELECT text FROM page_lines"),
    ] {
        let mut stmt = c.prepare(sql).unwrap();
        let rows: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        if rows.iter().any(|t| t.contains(NEEDLE)) {
            found.push(name.to_string());
        }
    }
    for edition in [Edition::Screen, Edition::Print, Edition::Phone] {
        let saved = f.engine.page_for(Some(date(PAGE)), edition).unwrap();
        if serde_json::to_string(&saved).unwrap().contains(NEEDLE) {
            found.push(format!("saved page {edition:?}"));
        }
    }
    if serde_json::to_string(&compose(f, PAGE))
        .unwrap()
        .contains(NEEDLE)
    {
        found.push("composed page".into());
    }
    let day = f.engine.journal_day(date("2026-11-14")).unwrap();
    if serde_json::to_string(&day).unwrap().contains(NEEDLE) {
        found.push("journal day".into());
    }
    found
}

fn answered_world() -> World {
    let w = world();
    save_day(&w.f, PAGE);
    w.f.clock.set_str("2026-11-14T08:15:00Z");
    w.f.engine
        .answer_question(date(PAGE), QuestionChoice::NotGreat)
        .unwrap();
    assert!(
        places_with_the_sentence(&w.f).len() > 3,
        "the sentence is kept while the block is"
    );
    w
}

fn assert_nothing_left(w: &World) {
    assert_eq!(places_with_the_sentence(&w.f), Vec::<String>::new());
    let (_, _, _, meta) = &answer_items(&w.f)[0];
    let meta: serde_json::Value = serde_json::from_str(meta).unwrap();
    assert_eq!(meta["sentence"], "", "blanked, not removed");
    assert_eq!(meta["choice"], "notGreat", "the answer itself stays");
    let day = w.f.engine.journal_day(date("2026-11-14")).unwrap();
    let row = day.rows.iter().find(|r| r.kind == RowKind::Answer).unwrap();
    assert_eq!(row.answer.as_ref().unwrap().sentence, "");
    let (kind, text): (String, String) = conn(&w.f)
        .query_row(
            "SELECT kind, text FROM questions WHERE page_date = ?1",
            [PAGE],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((kind.as_str(), text.as_str()), ("journal", ""));
    assert!(compose(&w.f, PAGE).question.is_none());
    assert!(
        w.f.engine
            .page(Some(date(PAGE)))
            .unwrap()
            .unwrap()
            .question
            .is_none()
    );
}

#[test]
fn deleting_the_block_and_its_memory_leaves_the_sentence_nowhere() {
    let w = answered_world();
    let preview =
        w.f.engine
            .preview_delete(std::slice::from_ref(&w.memory), true, &[], false)
            .unwrap();
    assert!(preview.originals.iter().any(|o| o.item_id == w.block));
    w.f.engine.forget(&preview.token).unwrap();
    assert_nothing_left(&w);
}

#[test]
fn taking_the_block_back_while_its_memory_lives_on_leaves_the_sentence_nowhere() {
    let f = fixture();
    save_day(&f, "2026-11-10");
    f.clock.set_str("2026-11-14T06:55:00Z");
    let (block, memory) = read_block(&f, EVENT, ENTRY, SUBJECT, EVENT, ENTRY);
    f.clock.set_str("2026-11-14T07:02:00Z");
    let w = World { f, block, memory };
    save_day(&w.f, PAGE);
    w.f.engine
        .answer_question(date(PAGE), QuestionChoice::NotGreat)
        .unwrap();
    let other = keep(&w.f.engine, "Another note about the evening.");
    sql(
        &w.f,
        "INSERT INTO memory_sources(memory_id, item_id, span) VALUES (?1, ?2, NULL)",
        params![w.memory, other],
    );
    w.f.engine.unkeep(&w.block).unwrap();
    let alive: bool = conn(&w.f)
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM memories WHERE id = ?1)",
            [&w.memory],
            |r| r.get(0),
        )
        .unwrap();
    assert!(alive, "the memory has another source");
    assert_nothing_left(&w);
}

#[test]
fn deleting_everything_clears_the_answer_and_the_question() {
    let w = answered_world();
    w.f.engine
        .delete_everything(nocturne_engine::DELETE_PHRASE)
        .unwrap();
    assert_eq!(
        places_with_the_sentence(&w.f),
        Vec::<String>::new(),
        "{:?}",
        places_with_the_sentence(&w.f)
    );
}

#[test]
fn a_journal_question_whose_block_is_gone_is_not_shown_even_with_the_memory_left() {
    let w = world();
    save_day(&w.f, PAGE);
    let other = keep(&w.f.engine, "Another note about the evening.");
    sql(
        &w.f,
        "INSERT INTO memory_sources(memory_id, item_id, span) VALUES (?1, ?2, NULL)",
        params![w.memory, other],
    );
    sql(
        &w.f,
        "DELETE FROM memory_sources WHERE memory_id = ?1 AND item_id = ?2",
        params![w.memory, w.block],
    );
    assert!(
        compose(&w.f, PAGE).question.is_none(),
        "no fall back to another source"
    );
}

// ---- the week rule survives a forgotten memory -------------------------------------------------

#[test]
fn a_forgotten_memorys_journal_question_still_counts_for_the_week() {
    let w = world();
    save_day(&w.f, PAGE);
    let preview =
        w.f.engine
            .preview_forget(std::slice::from_ref(&w.memory), false)
            .unwrap();
    w.f.engine.forget(&preview.token).unwrap();
    let (memory, text, kind): (Option<String>, String, String) = conn(&w.f)
        .query_row(
            "SELECT memory_id, text, kind FROM questions WHERE page_date = ?1",
            [PAGE],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (memory, text.as_str(), kind.as_str()),
        (None, "", "journal")
    );
    assert!(
        w.f.engine
            .page(Some(date(PAGE)))
            .unwrap()
            .unwrap()
            .question
            .is_none(),
        "a saved journal question with no memory is not shown"
    );

    w.f.clock.set_str("2026-11-14T19:00:00Z");
    read_block(
        &w.f,
        "2026-11-14",
        "Swimming at nine. Leo is excited.",
        "Swimming",
        "2026-11-14",
        "Swimming at nine. Leo is excited.",
    );
    w.f.clock.set_str("2026-11-15T07:00:00Z");
    let next = compose(&w.f, "2026-11-15").question;
    assert!(
        next.is_none_or(|q| q.kind != QuestionKind::Journal),
        "the week is still taken"
    );
}

#[test]
fn a_forgotten_yes_no_question_is_still_dropped() {
    let f = fixture();
    let (_, memory) = common::learn(
        &f.engine,
        "Leo likes swimming.",
        cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming."),
    );
    save_day(&f, "2026-11-10");
    f.clock.set_str("2026-11-11T07:00:00Z");
    save_day(&f, "2026-11-11");
    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&memory), false)
        .unwrap();
    f.engine.forget(&preview.token).unwrap();
    let left: i64 = conn(&f)
        .query_row(
            "SELECT COUNT(*) FROM questions WHERE kind = 'yesNo'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, 0);
}

fn phone(f: &Fixture) -> String {
    f.engine
        .add_device(
            "Pixel",
            &[4u8; 65],
            "https://home.example",
            nocturne_contract::RemoteConnection::Simple,
            "addr-a",
        )
        .unwrap()
        .id
}

fn answered(f: &Fixture) -> (Option<QuestionChoice>, Option<String>) {
    let q = f
        .engine
        .page(Some(date(PAGE)))
        .unwrap()
        .unwrap()
        .question
        .unwrap();
    (q.answered, q.journal.unwrap().line)
}

#[test]
fn an_answer_from_a_phone_is_kept_once_however_often_it_is_sent() {
    let w = world();
    save_day(&w.f, PAGE);
    let me = phone(&w.f);
    let kept =
        w.f.engine
            .answer_from_phone(date(PAGE), QuestionChoice::NotGreat, None, &me, "c1")
            .unwrap()
            .expect("kept now");
    for _ in 0..2 {
        assert_eq!(
            w.f.engine
                .answer_from_phone(date(PAGE), QuestionChoice::NotGreat, None, &me, "c1")
                .unwrap(),
            None,
            "a retry keeps nothing more"
        );
    }
    let items = answer_items(&w.f);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0, kept);
    assert_eq!(answered(&w.f), (Some(QuestionChoice::NotGreat), None));
    assert_eq!(
        w.f.engine.remote_keep(&me, "c1").unwrap().as_deref(),
        Some(kept.as_str())
    );
    assert!(
        last_used(&w.f, &w.memory).is_some(),
        "Not great counts as use"
    );
    let day = w.f.engine.journal_day(date(PAGE)).unwrap();
    let row = day.rows.iter().find(|r| r.kind == RowKind::Answer).unwrap();
    assert_eq!(row.item.from_device.as_deref(), Some("Pixel"));
}

#[test]
fn an_answer_from_a_phone_with_a_line_goes_across_whole() {
    let w = world();
    save_day(&w.f, PAGE);
    let me = phone(&w.f);
    let line = Some("  Long queue.  ");
    let first =
        w.f.engine
            .answer_from_phone(date(PAGE), QuestionChoice::Fine, line, &me, "c1")
            .unwrap();
    assert!(first.is_some());
    assert_eq!(
        w.f.engine
            .answer_from_phone(date(PAGE), QuestionChoice::Fine, line, &me, "c1")
            .unwrap(),
        None
    );
    let items = answer_items(&w.f);
    assert_eq!(items.len(), 1);
    assert_eq!(
        (items[0].1.as_str(), items[0].2.as_str()),
        ("kept", "Long queue.")
    );
    assert_eq!(
        answered(&w.f),
        (Some(QuestionChoice::Fine), Some("Long queue.".into()))
    );
}

#[test]
fn a_line_from_a_phone_goes_on_the_answer_given_elsewhere_once() {
    let w = world();
    save_day(&w.f, PAGE);
    let me = phone(&w.f);
    w.f.engine
        .answer_question(date(PAGE), QuestionChoice::Fine)
        .unwrap();
    let added =
        w.f.engine
            .answer_from_phone(
                date(PAGE),
                QuestionChoice::Fine,
                Some("Priya came."),
                &me,
                "c2",
            )
            .unwrap();
    assert!(added.is_some(), "the line goes on the computer's answer");
    for (line, client) in [("Priya came.", "c2"), ("Something else.", "c3")] {
        assert_eq!(
            w.f.engine
                .answer_from_phone(date(PAGE), QuestionChoice::Fine, Some(line), &me, client)
                .unwrap(),
            None,
            "a line goes on once"
        );
    }
    assert_eq!(answer_items(&w.f).len(), 1);
    assert_eq!(
        answered(&w.f),
        (Some(QuestionChoice::Fine), Some("Priya came.".into()))
    );
}

#[test]
fn an_answer_given_elsewhere_stands_and_a_line_for_another_choice_is_not_added() {
    let w = world();
    save_day(&w.f, PAGE);
    let me = phone(&w.f);
    w.f.engine
        .answer_question(date(PAGE), QuestionChoice::Fine)
        .unwrap();
    assert_eq!(
        w.f.engine
            .answer_from_phone(
                date(PAGE),
                QuestionChoice::NotGreat,
                Some("Awful."),
                &me,
                "c1"
            )
            .unwrap(),
        None
    );
    assert_eq!(answer_items(&w.f).len(), 1);
    assert_eq!(answered(&w.f), (Some(QuestionChoice::Fine), None));
    assert_eq!(w.f.engine.remote_keep(&me, "c1").unwrap(), None);
}

#[test]
fn a_line_from_a_phone_is_not_added_once_the_page_is_replaced() {
    let w = world();
    save_day(&w.f, PAGE);
    let me = phone(&w.f);
    w.f.engine
        .answer_from_phone(date(PAGE), QuestionChoice::NotYet, None, &me, "c1")
        .unwrap();
    w.f.clock.set_str("2026-11-15T07:00:00Z");
    save_day(&w.f, "2026-11-15");
    assert_eq!(
        w.f.engine
            .answer_from_phone(
                date(PAGE),
                QuestionChoice::NotYet,
                Some("Too late."),
                &me,
                "c1"
            )
            .unwrap(),
        None
    );
    assert_eq!(answered(&w.f), (Some(QuestionChoice::NotYet), None));
}

#[test]
fn a_phone_answers_only_a_question_from_the_journal_with_its_choices() {
    let w = world();
    save_day(&w.f, PAGE);
    let me = phone(&w.f);
    assert!(
        w.f.engine
            .answer_from_phone(date(PAGE), QuestionChoice::Yes, None, &me, "c1")
            .is_err()
    );
    let f = fixture();
    save_day(&f, "2026-11-10");
    let me = phone(&f);
    for choice in [QuestionChoice::Fine, QuestionChoice::Right] {
        assert!(
            f.engine
                .answer_from_phone(date("2026-11-10"), choice, None, &me, "c1")
                .is_err(),
            "{choice:?} on the page-time question"
        );
    }
    assert!(answer_items(&w.f).is_empty());
}
