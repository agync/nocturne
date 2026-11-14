//! Moves the small reader leaves unlinked, from the memories Tiny actually stored for the
//! dentist notes: the old appointment, and a later note that says it moved.

mod common;

use common::*;
use nocturne_contract::{MemoryFilter, Span};
use nocturne_engine::{Candidate, Predicate, is_source_line};

const FIRST: &str = "Dentist appointment with Dr Okafor on Wednesday 18 November at 10:00.";
const MOVED: &str = "The dentist appointment has moved. Dr Okafor's receptionist called: it is now Thursday 19 November at 14:30, not Wednesday 18 November.";
const NEW_LINE: &str = "Dr Okafor's receptionist called: it is now Thursday 19 November at 14:30, not Wednesday 18 November.";

fn dentist(subject: &str, object: &str, on: &str, quote: &str) -> Candidate {
    let mut c = cand(
        subject,
        Predicate::Scheduled,
        object,
        &format!("The dentist appointment with Dr Okafor is on {object}."),
    );
    c.due = Some(date(on));
    c.span = Some(Span::Quote {
        text: quote.to_string(),
    });
    c
}

/// The first note, kept at 08:05; the clock then moves to when the second was kept.
fn first_note(subject: &str) -> (Fixture, String) {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let (_, id) = learn(
        &f.engine,
        FIRST,
        dentist(
            subject,
            "Wednesday 18 November at 10:00",
            "2026-11-18",
            FIRST,
        ),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    (f, id)
}

fn current(f: &Fixture) -> Vec<nocturne_contract::MemoryView> {
    f.engine.memories(MemoryFilter::default()).unwrap()
}

/// Tiny's reading of the move in the runs that linked it, without the `moved` flag: the quoted
/// sentence giving the new day says the old day no longer holds.
#[test]
fn a_new_day_that_says_not_the_old_one_moves_it() {
    let (f, old) = first_note("Dentist appointment");
    let item = keep(&f.engine, MOVED);
    let r = f
        .engine
        .apply(
            &item,
            vec![dentist(
                "Dentist appointment",
                "Thursday 19 November at 14:30",
                "2026-11-19",
                NEW_LINE,
            )],
        )
        .unwrap();
    assert_eq!(r.replaced, [old], "{r:?}");
    assert!(r.conflicts.is_empty());
    let rows = current(&f);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(
        rows[0].was.as_deref(),
        Some("Wednesday 18 November at 10:00")
    );
}

/// "The dentist appointment has moved. It is now ...": the move word is in the sentence before,
/// which names the appointment, and the new day's sentence speaks of it as "it".
#[test]
fn a_move_said_just_before_about_the_same_thing_moves_it() {
    let (f, old) = first_note("Dentist appointment");
    let text = "The dentist appointment has moved. It is now Thursday 19 November at 14:30.";
    let item = keep(&f.engine, text);
    let r = f
        .engine
        .apply(
            &item,
            vec![dentist(
                "Dentist appointment",
                "Thursday 19 November at 14:30",
                "2026-11-19",
                text,
            )],
        )
        .unwrap();
    assert_eq!(r.replaced, [old], "{r:?}");
}

/// The mind's own sentence saying it moved, with the new day in it, counts as well.
#[test]
fn the_reading_sentence_saying_it_moved_moves_it() {
    let (f, old) = first_note("Dentist appointment");
    let item = keep(&f.engine, MOVED);
    let mut c = dentist(
        "Dentist appointment",
        "Thursday 19 November at 14:30",
        "2026-11-19",
        "it is now Thursday 19 November at 14:30",
    );
    c.sentence = "The dentist appointment has moved to Thursday 19 November at 14:30.".into();
    let r = f.engine.apply(&item, vec![c]).unwrap();
    assert_eq!(r.replaced, [old], "{r:?}");
}

/// The reader's sentence saying it moved is not enough when the words it quotes say no such
/// thing: the two appointments stay apart.
#[test]
fn the_reading_sentence_alone_is_no_move() {
    let (f, _) = first_note("Dentist appointment");
    let item = keep(&f.engine, "Dentist Thursday 19 November at 14:30.");
    let mut c = dentist(
        "Dentist appointment",
        "Thursday 19 November at 14:30",
        "2026-11-19",
        "Dentist Thursday 19 November at 14:30.",
    );
    c.sentence = "The dentist appointment has moved to Thursday 19 November at 14:30.".into();
    let r = f.engine.apply(&item, vec![c]).unwrap();
    assert!(r.replaced.is_empty() && r.conflicts.is_empty(), "{r:?}");
}

/// A quote whose only cue is "now", naming neither the appointment nor a day or time, does not
/// let the reader's sentence count as a move.
#[test]
fn now_alone_in_the_quote_does_not_back_the_reading_sentence() {
    let (f, _) = first_note("Dentist appointment");
    let item = keep(&f.engine, "Got it now, thanks.");
    let mut c = dentist(
        "Dentist appointment",
        "Thursday 19 November at 14:30",
        "2026-11-19",
        "Got it now, thanks.",
    );
    c.sentence = "The dentist appointment has moved to Thursday 19 November at 14:30.".into();
    let r = f.engine.apply(&item, vec![c]).unwrap();
    assert!(r.replaced.is_empty(), "{r:?}");
}

/// Guards: "not" naming another day than the old one, or a move said about something else, is
/// no move; the two appointments stay apart.
#[test]
fn not_another_day_or_a_move_of_something_else_is_no_move() {
    let (f, _) = first_note("Dentist appointment");
    let text = "Hygienist on Thursday 3 December, not Friday 4 December.";
    let item = keep(&f.engine, text);
    let r = f
        .engine
        .apply(
            &item,
            vec![dentist(
                "Dentist appointment",
                "Thursday 3 December",
                "2026-12-03",
                text,
            )],
        )
        .unwrap();
    assert!(r.replaced.is_empty() && r.conflicts.is_empty(), "{r:?}");

    let (f, _) = first_note("Dentist appointment");
    let text = "The dentist surgery moved. Hygienist appointment on Thursday 3 December.";
    let item = keep(&f.engine, text);
    let r = f
        .engine
        .apply(
            &item,
            vec![dentist(
                "Dentist appointment",
                "Thursday 3 December",
                "2026-12-03",
                text,
            )],
        )
        .unwrap();
    assert!(r.replaced.is_empty() && r.conflicts.is_empty(), "{r:?}");
}

/// Filed under another name ("Dr Okafor"), the same words only ask: it may be someone else's
/// appointment, so nothing is replaced until the person says.
#[test]
fn a_move_filed_under_another_name_asks() {
    let (f, old) = first_note("Dr Okafor");
    let item = keep(&f.engine, MOVED);
    let r = f
        .engine
        .apply(
            &item,
            vec![dentist(
                "Dentist appointment",
                "Thursday 19 November at 14:30",
                "2026-11-19",
                NEW_LINE,
            )],
        )
        .unwrap();
    assert!(r.replaced.is_empty(), "{r:?}");
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(f.engine.open_disputes(&[old]).unwrap().len(), 1);
}

/// What Tiny stored from the move note in two of three runs: the move with no new day, and the
/// new day read as the time of the call.
fn tiny_move_reading() -> Vec<Candidate> {
    vec![
        cand(
            "Dentist appointment",
            Predicate::Other("moved".into()),
            "moved",
            "The dentist appointment has been moved.",
        ),
        cand(
            "Dr Okafor's receptionist",
            Predicate::Other("called".into()),
            "at 14:30 on Thursday 19 November",
            "Dr Okafor's receptionist called at 14:30 on Thursday 19 November.",
        ),
        cand(
            "The call",
            Predicate::Happened,
            "Thursday 19 November at 14:30",
            "The call was made on Thursday 19 November at 14:30.",
        ),
    ]
}

/// The appointment a later note says moved, with the line of that note that names the new day
/// as the other side, its receipt the note.
#[test]
fn a_later_move_with_no_new_day_kept_brings_the_line_that_has_it() {
    let (f, old) = first_note("Dentist appointment");
    let item = keep(&f.engine, MOVED);
    f.engine.apply(&item, tiny_move_reading()).unwrap();
    let hits = f
        .engine
        .retrieve("When is my dentist appointment?", None, 12)
        .unwrap();
    let moves = f.engine.later_moves(&hits).unwrap();
    assert_eq!(moves.len(), 1, "{moves:?}");
    let (from, notice, side) = &moves[0];
    assert_eq!(from, &old);
    assert_eq!(
        f.engine.get_memory(notice).unwrap().sentence,
        "The dentist appointment has been moved."
    );
    assert!(is_source_line(&side.memory.id));
    assert_eq!(side.memory.sentence, NEW_LINE);
    assert_eq!(side.memory.receipts[0].item_id, item);
}

/// Filed under "Dr Okafor", the old appointment is found by a later note whose sentence names
/// Dr Okafor; Tiny's own "The dentist appointment has been moved." does not name it, so it is
/// not taken for this appointment.
#[test]
fn a_later_move_under_another_name_must_name_it() {
    let (f, old) = first_note("Dr Okafor");
    let item = keep(&f.engine, MOVED);
    f.engine.apply(&item, tiny_move_reading()).unwrap();
    let hits = f
        .engine
        .retrieve("When is my dentist appointment?", None, 12)
        .unwrap();
    assert!(f.engine.later_moves(&hits).unwrap().is_empty());

    let (f, old2) = first_note("Dr Okafor");
    let item = keep(&f.engine, MOVED);
    let mut reading = tiny_move_reading();
    reading[0].sentence = "The dentist appointment with Dr Okafor has been moved.".into();
    f.engine.apply(&item, reading).unwrap();
    let hits = f
        .engine
        .retrieve("When is my dentist appointment?", None, 12)
        .unwrap();
    let moves = f.engine.later_moves(&hits).unwrap();
    assert_eq!(moves.len(), 1, "{moves:?}");
    assert_eq!(moves[0].0, old2);
    let _ = old;
}

/// Apply a later note's reading and look for later moves of `first`'s appointment.
fn moves_after(f: &Fixture, text: &str, reading: Vec<Candidate>, question: &str) -> usize {
    let item = keep(&f.engine, text);
    f.engine.apply(&item, reading).unwrap();
    let hits = f.engine.retrieve(question, None, 12).unwrap();
    f.engine.later_moves(&hits).unwrap().len()
}

/// Sam's party, then a later note read as `notice`: how many later moves it makes.
fn sams_party_then(notice: Candidate) -> usize {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let mut party = cand(
        "Sam",
        Predicate::Scheduled,
        "Saturday 21 November",
        "Sam's party is on Saturday 21 November.",
    );
    party.due = Some(date("2026-11-21"));
    learn(&f.engine, "Sam's party on Saturday 21 November.", party);
    f.clock.set_str("2026-11-12T12:00:00Z");
    moves_after(
        &f,
        "A later note about Sam.",
        vec![notice],
        "When is Sam's party?",
    )
}

/// My dentist appointment, then a later note read as one memory saying `sentence`, with
/// `predicate` and `object`: how many later moves it makes.
fn dentist_then(predicate: &str, object: &str, sentence: &str) -> usize {
    let (f, _) = first_note("Dentist appointment");
    let notice = cand(
        "Dentist appointment",
        Predicate::Other(predicate.into()),
        object,
        sentence,
    );
    moves_after(
        &f,
        sentence,
        vec![notice],
        "When is my dentist appointment?",
    )
}

/// Each guard on its own: the same note passes when only that guard is satisfied.
#[test]
fn where_someone_lives_is_no_move() {
    let moved = |p: Predicate| cand("Sam", p, "moved", "Sam's party has been moved.");
    assert_eq!(sams_party_then(moved(Predicate::Other("moved".into()))), 1);
    assert_eq!(sams_party_then(moved(Predicate::LivesIn)), 0);
}

#[test]
fn a_move_that_says_where_to_besides_is_no_move() {
    let moved = |o: &str| cand("Sam", Predicate::Other("moved".into()), o, "Sam moved.");
    assert_eq!(sams_party_then(moved("moved")), 1);
    assert_eq!(sams_party_then(moved("Leeds")), 0);
}

#[test]
fn someone_elses_appointment_is_no_move() {
    assert_eq!(
        dentist_then("moved", "moved", "The dentist appointment has been moved."),
        1
    );
    assert_eq!(
        dentist_then(
            "moved",
            "moved",
            "Mum's dentist appointment has been moved."
        ),
        0
    );
    assert_eq!(
        dentist_then("moved", "moved", "Her dentist appointment has been moved."),
        0
    );
    assert_eq!(
        dentist_then(
            "moved",
            "moved",
            "Sam has had the dentist appointment moved."
        ),
        0
    );
}

#[test]
fn changed_is_a_move_only_of_an_appointment() {
    assert_eq!(
        dentist_then("changed", "changed", "The dentist appointment was changed."),
        1
    );
    assert_eq!(
        dentist_then("changed", "changed", "The dentist changed my filling."),
        0
    );
}

/// With Leo's dentist appointment kept too, a note that says "the dentist appointment" moved
/// could be about either: no move, even when the search found only mine.
#[test]
fn a_move_that_could_be_another_appointment_found_or_not_is_left_out() {
    let (f, mine) = first_note("Dentist appointment");
    f.clock.set_str("2026-11-12T09:00:00Z");
    learn(
        &f.engine,
        "Leo's check-up on Monday 23 November at 9:00.",
        dentist(
            "Dentist appointment",
            "Monday 23 November at 9:00",
            "2026-11-23",
            "Leo's check-up on Monday 23 November at 9:00.",
        ),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    let text = "The dentist appointment has been moved. It is now Friday 27 November.";
    let item = keep(&f.engine, text);
    f.engine
        .apply(&item, vec![tiny_move_reading().remove(0)])
        .unwrap();
    let hits: Vec<_> = f
        .engine
        .retrieve("When is my dentist appointment?", None, 12)
        .unwrap()
        .into_iter()
        .filter(|h| h.memory.id == mine || !h.memory.sentence.contains("Monday"))
        .collect();
    assert!(hits.iter().all(|h| !h.memory.sentence.contains("Monday")));
    assert!(f.engine.later_moves(&hits).unwrap().is_empty());
}

/// A line naming another day that is not where the appointment went (another sentence, about
/// something else) is never taken: the memory that says it moved stands for the note instead.
#[test]
fn an_unrelated_line_with_a_day_is_not_the_new_day() {
    let (f, _) = first_note("Dentist appointment");
    let text = "The dentist appointment has been moved. I will ring them back. Parking is free on Saturday 21 November.";
    let item = keep(&f.engine, text);
    f.engine
        .apply(&item, vec![tiny_move_reading().remove(0)])
        .unwrap();
    let hits = f
        .engine
        .retrieve("When is my dentist appointment?", None, 12)
        .unwrap();
    let moves = f.engine.later_moves(&hits).unwrap();
    assert_eq!(moves.len(), 1, "{moves:?}");
    assert!(!is_source_line(&moves[0].2.memory.id));
    assert_eq!(moves[0].2.memory.id, moves[0].1);
}

/// No later move: kept at once with the appointment (no order), with two appointments it could
/// be about, or once reading kept the new day in its place.
#[test]
fn a_move_note_is_left_out_when_it_is_not_clearly_about_one_later_change() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    learn(
        &f.engine,
        FIRST,
        dentist(
            "Dentist appointment",
            "Wednesday 18 November at 10:00",
            "2026-11-18",
            FIRST,
        ),
    );
    f.clock.set_str("2026-11-12T08:06:00Z");
    let item = keep(&f.engine, MOVED);
    f.engine.apply(&item, tiny_move_reading()).unwrap();
    let hits = f.engine.retrieve("dentist appointment", None, 12).unwrap();
    assert!(f.engine.later_moves(&hits).unwrap().is_empty());

    let (f, _) = first_note("Dentist appointment");
    f.clock.set_str("2026-11-12T09:00:00Z");
    learn(
        &f.engine,
        "Dentist appointment for Leo on Monday 23 November at 9:00.",
        dentist(
            "Dentist appointment",
            "Monday 23 November at 9:00",
            "2026-11-23",
            "Dentist appointment for Leo on Monday 23 November at 9:00.",
        ),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    let item = keep(&f.engine, MOVED);
    f.engine.apply(&item, tiny_move_reading()).unwrap();
    let hits = f.engine.retrieve("dentist appointment", None, 12).unwrap();
    assert!(f.engine.later_moves(&hits).unwrap().is_empty());

    let (f, _) = first_note("Dentist appointment");
    let item = keep(&f.engine, MOVED);
    let mut reading = tiny_move_reading();
    let mut moved = dentist(
        "Dentist appointment",
        "Thursday 19 November at 14:30",
        "2026-11-19",
        NEW_LINE,
    );
    moved.moved = true;
    reading.push(moved);
    f.engine.apply(&item, reading).unwrap();
    let hits = f.engine.retrieve("dentist appointment", None, 12).unwrap();
    assert!(f.engine.later_moves(&hits).unwrap().is_empty());
}

/// Read out of order: the note saying "not Wednesday 18 November" is read first, then the first
/// note about Wednesday. They may be one appointment, so the person is asked, not given two.
#[test]
fn the_old_day_read_after_a_note_that_cancels_it_asks() {
    let f = fixture_at("2026-11-12T12:00:00Z", "Europe/London");
    let first = keep(&f.engine, FIRST);
    f.clock.set_str("2026-11-12T12:30:00Z");
    let moved = keep(&f.engine, MOVED);
    f.engine
        .apply(
            &moved,
            vec![dentist(
                "Dentist appointment",
                "Thursday 19 November at 14:30",
                "2026-11-19",
                NEW_LINE,
            )],
        )
        .unwrap();
    let r = f
        .engine
        .apply(
            &first,
            vec![dentist(
                "Dentist appointment",
                "Wednesday 18 November at 10:00",
                "2026-11-18",
                FIRST,
            )],
        )
        .unwrap();
    assert_eq!(r.conflicts.len(), 1, "{r:?}");
}
