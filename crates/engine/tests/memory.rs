mod common;

use common::*;
use nocturne_contract::{Certainty, MemoryFilter, MemoryStatus, NeedsYouEntry, Source};
use nocturne_engine::Predicate;

fn all(f: &Fixture) -> Vec<nocturne_contract::MemoryView> {
    f.engine.memories(MemoryFilter::default()).unwrap()
}

#[test]
fn same_candidate_twice_reinforces_one_row() {
    let f = fixture();
    let c = || cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming.");
    let (_, id) = learn(&f.engine, "Leo likes swimming", c());
    let first = &all(&f)[0];
    assert_eq!(first.certainty, Certainty::FairlySure);
    assert_eq!(first.last_used, None);
    assert_eq!(first.receipts.len(), 1);

    f.clock.set_str("2026-11-14T09:00:00Z");
    let (_, id2) = learn(&f.engine, "Leo really likes swimming", c());
    assert_eq!(id, id2);
    let rows = all(&f);
    assert_eq!(rows.len(), 1, "one row");
    assert_eq!(rows[0].receipts.len(), 2, "source count 2");
    assert_eq!(rows[0].certainty, Certainty::Sure, "certainty rises");
    assert_eq!(
        rows[0].last_used.as_deref(),
        Some("2026-11-14T09:00:00.000Z")
    );
}

#[test]
fn applying_an_item_again_replaces_what_it_said_and_is_not_a_second_voice() {
    let f = fixture();
    let item = keep(&f.engine, "Gran lives in Leeds, the cat is called Pip");
    let leeds = cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds.");
    let pip = cand("Cat", Predicate::Knows, "Pip", "The cat is called Pip.");
    f.engine.apply(&item, vec![leeds.clone(), pip]).unwrap();
    assert_eq!(all(&f).len(), 2);

    f.clock.set_str("2026-11-14T09:00:00Z");
    let report = f.engine.apply(&item, vec![leeds]).unwrap();
    assert_eq!(report.removed.len(), 1, "what the item no longer says goes");
    let rows = all(&f);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].receipts.len(), 1);
    assert_eq!(rows[0].last_used, None, "re-reading is not reinforcement");
    assert_eq!(rows[0].certainty, Certainty::FairlySure);
}

#[test]
fn a_confirmed_memory_survives_its_item_being_reread() {
    let f = fixture();
    let (item, id) = learn(
        &f.engine,
        "Gran lives in Leeds",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    f.engine.confirm(&id, true).unwrap();
    f.engine.apply(&item, vec![]).unwrap();
    let rows = all(&f);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].receipts.len(),
        1,
        "every memory keeps at least one source"
    );
}

#[test]
fn untrusted_sources_never_create_active_memories() {
    let f = fixture();
    let web = keep_as(&f.engine, "Clinic opens at 9 (web page)", Source::PastedWeb);
    let c = || {
        cand(
            "Clinic",
            Predicate::Knows,
            "opens at 9",
            "The clinic opens at 9.",
        )
    };
    let r = f.engine.apply(&web, vec![c()]).unwrap();
    assert_eq!(r.unconfirmed.len(), 1);
    let m = &all(&f)[0];
    assert_eq!(m.status, MemoryStatus::Unconfirmed);
    assert_eq!(m.certainty, Certainty::NotSure);
    assert!(m.receipts[0].unconfirmed);
    assert_eq!(
        f.engine.get_item(&web).unwrap().status,
        nocturne_contract::ItemStatus::Unconfirmed
    );
    let needs = f.engine.needs_you().unwrap();
    assert!(matches!(
        &needs[0],
        NeedsYouEntry::Unconfirmed {
            agreeing_sources: 1,
            ..
        }
    ));

    // A second untrusted voice does not promote it.
    let agent = keep_as(&f.engine, "agent says clinic opens at 9", Source::Agent);
    f.engine.apply(&agent, vec![c()]).unwrap();
    assert_eq!(all(&f)[0].status, MemoryStatus::Unconfirmed);

    // A trusted, independent item does.
    let note = keep(&f.engine, "I rang: the clinic opens at 9");
    let r = f.engine.apply(&note, vec![c()]).unwrap();
    assert_eq!(r.promoted.len(), 1);
    let m = &all(&f)[0];
    assert_eq!(m.status, MemoryStatus::Active);
    assert_eq!(m.receipts.len(), 3);
    assert!(f.engine.needs_you().unwrap().is_empty());
}

#[test]
fn a_trusted_item_from_the_same_origin_does_not_corroborate() {
    let f = fixture();
    let web = f
        .engine
        .keep_text_from("page text", Source::PastedWeb, Some("example.org"))
        .unwrap();
    let web = match web {
        nocturne_contract::Kept::New { item } => item.id,
        _ => unreachable!(),
    };
    let c = || {
        cand(
            "Clinic",
            Predicate::Knows,
            "opens at 9",
            "The clinic opens at 9.",
        )
    };
    f.engine.apply(&web, vec![c()]).unwrap();
    let mirrored = match f
        .engine
        .keep_text_from("copy of that page", Source::Paste, Some("example.org"))
        .unwrap()
    {
        nocturne_contract::Kept::New { item } => item.id,
        _ => unreachable!(),
    };
    f.engine.apply(&mirrored, vec![c()]).unwrap();
    assert_eq!(all(&f)[0].status, MemoryStatus::Unconfirmed);
}

#[test]
fn unknown_predicates_are_unconfirmed_until_the_person_confirms() {
    let f = fixture();
    let (_, id) = learn(
        &f.engine,
        "Leo is allergic to nothing",
        cand(
            "Leo",
            Predicate::Other("owns".into()),
            "a bike",
            "Leo owns a bike.",
        ),
    );
    assert_eq!(all(&f)[0].status, MemoryStatus::Unconfirmed);
    f.engine.confirm(&id, true).unwrap();
    let m = &all(&f)[0];
    assert_eq!(m.status, MemoryStatus::Active);
    assert_eq!(m.certainty, Certainty::Sure);
    f.engine.confirm(&id, false).unwrap();
    assert_eq!(all(&f)[0].status, MemoryStatus::Unconfirmed);
}

#[test]
fn scheduled_with_different_object_and_overlapping_validity_conflicts() {
    let f = fixture();
    let (_, a) = learn(
        &f.engine,
        "Dentist Tuesday 14:00",
        cand(
            "The dentist appointment",
            Predicate::Scheduled,
            "Tuesday 14:00",
            "Your dentist appointment is Tuesday at 14:00.",
        ),
    );
    let item_b = keep(&f.engine, "Dentist moved to Thursday 10:00");
    let r = f
        .engine
        .apply(
            &item_b,
            vec![cand(
                "dentist appointment",
                Predicate::Scheduled,
                "Thursday 10:00",
                "Your dentist appointment is Thursday at 10:00.",
            )],
        )
        .unwrap();
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(
        f.engine.get_item(&item_b).unwrap().status,
        nocturne_contract::ItemStatus::NeedsYou
    );
    let b = r.created[0].clone();

    let needs = f.engine.needs_you().unwrap();
    assert_eq!(needs.len(), 1);
    let NeedsYouEntry::Which {
        conflict_id,
        options,
        ..
    } = &needs[0]
    else {
        panic!("which")
    };
    assert_eq!(options.len(), 2);
    assert_eq!(options[0].memory_id, a);
    assert_eq!(options[1].memory_id, b);

    f.engine.resolve_which(conflict_id, &b).unwrap();
    assert!(f.engine.needs_you().unwrap().is_empty());
    let rows = all(&f);
    assert_eq!(rows.len(), 1, "the other is history, not a current memory");
    assert_eq!(rows[0].id, b);
    assert_eq!(rows[0].certainty, Certainty::Sure);
    assert_eq!(rows[0].was.as_deref(), Some("Tuesday 14:00"));
    assert!(
        f.engine.resolve_which(conflict_id, &b).is_err(),
        "decided once"
    );
}

#[test]
fn contradiction_detection_ignores_memories_whose_valid_time_does_not_overlap() {
    // Review focus 4: a past address is history, not a conflict.
    let f = fixture();
    let mut old = cand("Gran", Predicate::LivesIn, "Leeds", "Gran lived in Leeds.");
    old.valid_to = Some(date("2020-01-01"));
    learn(&f.engine, "Gran used to live in Leeds", old);
    let mut now = cand("Gran", Predicate::LivesIn, "York", "Gran lives in York.");
    now.valid_from = Some(date("2020-01-01"));
    let (_, york) = learn(&f.engine, "Gran lives in York", now);
    assert!(f.engine.needs_you().unwrap().is_empty());
    assert_eq!(all(&f).len(), 2);

    // The same fact with overlapping validity does conflict.
    let item = keep(&f.engine, "Gran lives in Hull");
    let r = f
        .engine
        .apply(
            &item,
            vec![cand(
                "Gran",
                Predicate::LivesIn,
                "Hull",
                "Gran lives in Hull.",
            )],
        )
        .unwrap();
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(
        r.conflicts.len(),
        1,
        "only the York row overlaps; Leeds ended in 2020"
    );
    let NeedsYouEntry::Which { options, .. } = &f.engine.needs_you().unwrap()[0] else {
        panic!()
    };
    assert_eq!(options[0].memory_id, york);
}

#[test]
fn a_real_world_change_closes_the_old_value_at_the_new_start() {
    let f = fixture();
    learn(
        &f.engine,
        "Gran lives in Leeds",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    let mut moved = cand("Gran", Predicate::LivesIn, "York", "Gran moved to York.");
    moved.valid_from = Some(date("2026-03-01"));
    let item = keep(&f.engine, "Gran moved to York in March");
    let r = f.engine.apply(&item, vec![moved]).unwrap();
    assert!(r.conflicts.is_empty());
    let rows = all(&f);
    let leeds = rows.iter().find(|m| m.object == "Leeds").unwrap();
    let york = rows.iter().find(|m| m.object == "York").unwrap();
    assert_eq!(leeds.valid_to.as_deref(), Some("2026-03-01"));
    assert_eq!(york.was.as_deref(), Some("Leeds"));
}

fn appointment(object: &str, on: &str) -> nocturne_engine::Candidate {
    let mut c = cand(
        "Dentist appointment",
        Predicate::Scheduled,
        object,
        &format!("The dentist appointment is on {object}."),
    );
    // As a small model gives it: the day of the appointment as the start of the fact.
    c.valid_from = Some(date(on));
    c
}

/// An appointment the note says has moved, quoting the words that say so.
fn moved_to(object: &str, on: &str) -> nocturne_engine::Candidate {
    let mut c = appointment(object, on);
    c.span = Some(nocturne_contract::Span::Quote {
        text: format!("The dentist moved to {object}"),
    });
    c
}

fn current_objects(f: &Fixture) -> Vec<String> {
    let mut v: Vec<String> = all(f).into_iter().map(|m| m.object).collect();
    v.sort();
    v
}

/// A note that says the appointment moved, to a date still ahead, replaces it: the old one
/// stays as history and shows as `was`. Nothing waits on the person.
#[test]
fn a_moved_appointment_replaces_the_old_one() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let (first, wed) = learn(
        &f.engine,
        "Dentist on Wednesday 18 November at 10:00",
        appointment("Wednesday 18 November at 10:00", "2026-11-18"),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    let item = keep(
        &f.engine,
        "The dentist moved to Thursday 19 November at 14:30",
    );
    let r = f
        .engine
        .apply(
            &item,
            vec![moved_to("Thursday 19 November at 14:30", "2026-11-19")],
        )
        .unwrap();
    assert!(r.conflicts.is_empty());
    assert!(f.engine.needs_you().unwrap().is_empty());
    let rows = all(&f);
    assert_eq!(rows.len(), 1, "the old appointment is history: {rows:?}");
    let thu = &rows[0];
    assert_eq!(thu.was.as_deref(), Some("Wednesday 18 November at 10:00"));
    let history = f.engine.history(&thu.id).unwrap();
    assert!(history.iter().any(|m| m.id == wed));
    // The first note still counts what came of it, now as history; a correction made from one
    // note still counts once.
    assert_eq!(f.engine.get_item(&first).unwrap().memory_count, 1);
    assert_eq!(f.engine.get_item(&item).unwrap().memory_count, 1);
}

/// The mind may say so itself: a candidate marked as moved is a move without a move word.
#[test]
fn a_candidate_marked_moved_is_a_move() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    learn(
        &f.engine,
        "Dentist on Wednesday 18 November at 10:00",
        appointment("Wednesday 18 November at 10:00", "2026-11-18"),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    let item = keep(&f.engine, "Dentist: Thursday 19 November at 14:30 please");
    let mut thu = appointment("Thursday 19 November at 14:30", "2026-11-19");
    thu.moved = true;
    let r = f.engine.apply(&item, vec![thu]).unwrap();
    assert_eq!(r.replaced.len(), 1);
    assert_eq!(current_objects(&f), ["Thursday 19 November at 14:30"]);
}

/// Two dated visits and nothing saying one moved: two appointments, both kept, nothing asked.
#[test]
fn a_second_dated_appointment_is_not_a_move() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    learn(
        &f.engine,
        "Dentist check-up Wednesday 18 November at 10:00",
        appointment("Wednesday 18 November at 10:00", "2026-11-18"),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    let item = keep(&f.engine, "Hygienist at the dentist Thursday 3 December");
    let r = f
        .engine
        .apply(
            &item,
            vec![appointment("Thursday 3 December", "2026-12-03")],
        )
        .unwrap();
    assert!(r.replaced.is_empty() && r.conflicts.is_empty());
    assert_eq!(
        current_objects(&f),
        ["Thursday 3 December", "Wednesday 18 November at 10:00"]
    );
    // Forgetting one of them leaves the other alone.
    let hygienist = all(&f)
        .into_iter()
        .find(|m| m.object == "Thursday 3 December")
        .unwrap();
    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&hygienist.id), false)
        .unwrap();
    assert_eq!(preview.memories.len(), 1);
    f.engine.forget(&preview.token).unwrap();
    assert_eq!(current_objects(&f), ["Wednesday 18 November at 10:00"]);
}

/// An old letter kept today, about a date long past, never replaces what is coming up, even
/// when it says "moved": it may be the same appointment, so the person is asked.
#[test]
fn an_old_letter_does_not_replace_an_upcoming_appointment() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    learn(
        &f.engine,
        "Dentist on Wednesday 18 November at 10:00",
        appointment("Wednesday 18 November at 10:00", "2026-11-18"),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    let quiet = keep(&f.engine, "Letter: dentist 3 March 2025");
    let r = f
        .engine
        .apply(&quiet, vec![appointment("3 March 2025", "2025-03-03")])
        .unwrap();
    assert!(r.replaced.is_empty() && r.conflicts.is_empty());
    let moved = keep(&f.engine, "Letter: the dentist moved to 4 March 2025");
    let r = f
        .engine
        .apply(&moved, vec![moved_to("4 March 2025", "2025-03-04")])
        .unwrap();
    assert!(r.replaced.is_empty());
    assert!(!r.conflicts.is_empty());
    assert!(current_objects(&f).contains(&"Wednesday 18 November at 10:00".to_string()));
}

/// An appointment whose day had already gone when the move was kept is not the one moved.
#[test]
fn a_move_needs_the_old_appointment_still_ahead() {
    let f = fixture_at("2026-11-02T08:05:00Z", "Europe/London");
    learn(
        &f.engine,
        "Dentist on Tuesday 10 November",
        appointment("Tuesday 10 November", "2026-11-10"),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    let item = keep(&f.engine, "The dentist moved to Thursday 19 November");
    let r = f
        .engine
        .apply(&item, vec![moved_to("Thursday 19 November", "2026-11-19")])
        .unwrap();
    assert!(r.replaced.is_empty());
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(all(&f).len(), 2);
}

/// Read in the other order (the moved note first), nothing can tell from the older note that
/// it was the one moved: the person is asked.
#[test]
fn an_older_appointment_read_late_is_asked_about() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let first = keep(&f.engine, "Dentist on Wednesday 18 November at 10:00");
    f.clock.set_str("2026-11-12T12:00:00Z");
    learn(
        &f.engine,
        "The dentist moved to Thursday 19 November at 14:30",
        moved_to("Thursday 19 November at 14:30", "2026-11-19"),
    );
    f.clock.set_str("2026-11-12T22:00:00Z");
    let r = f
        .engine
        .apply(
            &first,
            vec![appointment("Wednesday 18 November at 10:00", "2026-11-18")],
        )
        .unwrap();
    assert!(r.replaced.is_empty());
    assert_eq!(r.conflicts.len(), 1);
}

/// Reading both notes again, the second time with new words for the same move, keeps one
/// current value with its history: no duplicate of the old time, nothing brought back, nothing
/// asked.
#[test]
fn reading_a_move_again_keeps_one_current_value() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let a = keep(&f.engine, "Dentist on Wednesday 18 November at 10:00");
    f.engine
        .apply(
            &a,
            vec![appointment("Wednesday 18 November at 10:00", "2026-11-18")],
        )
        .unwrap();
    f.clock.set_str("2026-11-12T12:00:00Z");
    let b = keep(
        &f.engine,
        "The dentist moved to Thursday 19 November at 14:30",
    );
    f.engine
        .apply(
            &b,
            vec![moved_to("Thursday 19 November at 14:30", "2026-11-19")],
        )
        .unwrap();
    f.clock.set_str("2026-11-13T12:00:00Z");
    f.engine
        .apply(
            &a,
            vec![appointment("Wednesday 18 November at 10:00", "2026-11-18")],
        )
        .unwrap();
    f.engine
        .apply(&b, vec![moved_to("Thu 19 Nov, 2:30pm", "2026-11-19")])
        .unwrap();
    let rows = all(&f);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].object, "Thu 19 Nov, 2:30pm");
    assert_eq!(
        rows[0].was.as_deref(),
        Some("Wednesday 18 November at 10:00")
    );
    assert!(f.engine.needs_you().unwrap().is_empty());
}

/// Read again without its date, the new value cannot be shown to be the same move, so it does
/// not take the history: the old time still never comes back, nothing is made twice, and the
/// person is asked.
#[test]
fn reading_a_move_again_without_its_date_asks() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let (a, b, wed, thu) = moved_world(&f);
    f.engine
        .apply(
            &a,
            vec![appointment("Wednesday 18 November at 10:00", "2026-11-18")],
        )
        .unwrap();
    let mut undated = moved_to("Thu 19 Nov, 2:30pm", "2026-11-19");
    undated.valid_from = None;
    f.engine.apply(&b, vec![undated]).unwrap();
    assert!(!all(&f).iter().any(|m| m.id == wed));
    assert!(superseded_by(&f, &wed, &thu));
    let weds = sql_count(
        f.dir.path(),
        "SELECT COUNT(*) FROM memories WHERE object = 'Wednesday 18 November at 10:00'",
    );
    assert_eq!(weds, 1);
    assert!(!f.engine.needs_you().unwrap().is_empty());
}

/// The dentist moved: A says Wednesday, B moves it to Thursday. Returns (A, B, Wed, Thu).
fn moved_world(f: &Fixture) -> (String, String, String, String) {
    let a = keep(&f.engine, "Dentist on Wednesday 18 November at 10:00");
    let wed = f
        .engine
        .apply(
            &a,
            vec![appointment("Wednesday 18 November at 10:00", "2026-11-18")],
        )
        .unwrap()
        .created[0]
        .clone();
    f.clock.set_str("2026-11-12T12:00:00Z");
    let b = keep(
        &f.engine,
        "The dentist moved to Thursday 19 November at 14:30",
    );
    let thu = f
        .engine
        .apply(
            &b,
            vec![moved_to("Thursday 19 November at 14:30", "2026-11-19")],
        )
        .unwrap()
        .created[0]
        .clone();
    f.clock.set_str("2026-11-13T12:00:00Z");
    (a, b, wed, thu)
}

fn superseded_by(f: &Fixture, id: &str, by: &str) -> bool {
    sql_count(
        f.dir.path(),
        &format!("SELECT COUNT(*) FROM memories WHERE id = '{id}' AND superseded_by = '{by}'"),
    ) == 1
}

/// A memory the person confirmed keeps its history, whatever a re-read of its note says.
#[test]
fn a_reread_never_moves_history_away_from_a_confirmed_memory() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let (_, b, wed, thu) = moved_world(&f);
    f.engine.confirm(&thu, true).unwrap();
    f.engine
        .apply(&b, vec![moved_to("Thu 19 Nov, 2:30pm", "2026-11-19")])
        .unwrap();
    assert!(superseded_by(&f, &wed, &thu));
    assert!(all(&f).iter().any(|m| m.id == thu));
}

/// A memory something depends on (used in an answer) is kept by a re-read, and so is its
/// history.
#[test]
fn a_reread_never_moves_history_away_from_a_protected_memory() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let (_, b, wed, thu) = moved_world(&f);
    f.engine
        .mark_used(std::slice::from_ref(&thu), nocturne_engine::UseKind::Answer)
        .unwrap();
    f.engine
        .apply(&b, vec![moved_to("Thu 19 Nov, 2:30pm", "2026-11-19")])
        .unwrap();
    assert!(superseded_by(&f, &wed, &thu));
}

/// History is never handed to a memory that is not settled: an unconfirmed one, even when the
/// re-read note now agrees with it, does not take it over.
#[test]
fn history_is_never_handed_to_an_unconfirmed_memory() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let a = keep(&f.engine, "Dentist on Wednesday 18 November at 10:00");
    let wed = f
        .engine
        .apply(
            &a,
            vec![appointment("Wednesday 18 November at 10:00", "2026-11-18")],
        )
        .unwrap()
        .created[0]
        .clone();
    f.clock.set_str("2026-11-12T12:00:00Z");
    let b = match f
        .engine
        .keep_text_from(
            "The dentist moved to Thursday 19 November at 14:30",
            Source::QuickCapture,
            Some("same-sender"),
        )
        .unwrap()
    {
        nocturne_contract::Kept::New { item } | nocturne_contract::Kept::Duplicate { item } => {
            item.id
        }
    };
    let thu = f
        .engine
        .apply(
            &b,
            vec![moved_to("Thursday 19 November at 14:30", "2026-11-19")],
        )
        .unwrap()
        .created[0]
        .clone();
    f.clock.set_str("2026-11-12T13:00:00Z");
    let web = match f
        .engine
        .keep_text_from("Dentist Thu 2:30pm", Source::PastedWeb, Some("same-sender"))
        .unwrap()
    {
        nocturne_contract::Kept::New { item } | nocturne_contract::Kept::Duplicate { item } => {
            item.id
        }
    };
    let unsure = f
        .engine
        .apply(&web, vec![appointment("Thu 19 Nov, 2:30pm", "2026-11-19")])
        .unwrap()
        .unconfirmed[0]
        .clone();
    f.clock.set_str("2026-11-13T12:00:00Z");
    f.engine
        .apply(&b, vec![moved_to("Thu 19 Nov, 2:30pm", "2026-11-19")])
        .unwrap();
    assert_eq!(
        f.engine.get_memory(&unsure).unwrap().status,
        MemoryStatus::Unconfirmed
    );
    assert!(
        superseded_by(&f, &wed, &thu),
        "the history stays where it was"
    );
}

fn quoted(object: &str, on: &str, quote: &str) -> nocturne_engine::Candidate {
    let mut c = appointment(object, on);
    c.span = Some(nocturne_contract::Span::Quote {
        text: quote.to_string(),
    });
    c
}

/// A move word about something else, in another sentence than the new date, is no move.
#[test]
fn a_move_word_about_something_else_is_no_move() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    learn(
        &f.engine,
        "Dentist Wed 18 Nov 10:00",
        appointment("Wednesday 18 November at 10:00", "2026-11-18"),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    let text = "The dentist moved to the High Street. Hygienist Thursday 3 December";
    let item = keep(&f.engine, text);
    let r = f
        .engine
        .apply(
            &item,
            vec![quoted("Thursday 3 December", "2026-12-03", text)],
        )
        .unwrap();
    assert!(r.replaced.is_empty(), "{r:?}");
    assert_eq!(all(&f).len(), 2);
}

/// "Now" is not a move: "I now also have" adds an appointment.
#[test]
fn now_is_not_a_move_word() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    learn(
        &f.engine,
        "Hygienist Thursday 3 December",
        appointment("Thursday 3 December", "2026-12-03"),
    );
    f.clock.set_str("2026-11-12T13:00:00Z");
    let text = "I now also have the hygienist on Friday 11 December";
    let item = keep(&f.engine, text);
    let r = f
        .engine
        .apply(
            &item,
            vec![quoted("Friday 11 December", "2026-12-11", text)],
        )
        .unwrap();
    assert!(r.replaced.is_empty() && r.conflicts.is_empty(), "{r:?}");
    assert_eq!(all(&f).len(), 2);
}

/// Next week's bins are not this week's moved: both stay, and the past one can still be found.
#[test]
fn the_next_bin_day_is_not_a_move() {
    let f = fixture_at("2026-11-10T08:05:00Z", "Europe/London");
    let mut a = cand(
        "Bins",
        Predicate::Scheduled,
        "Tuesday 17 November",
        "Bins go out Tuesday 17 November.",
    );
    a.due = Some(date("2026-11-17"));
    learn(&f.engine, "Bins Tue 17 Nov", a);
    f.clock.set_str("2026-11-18T08:00:00Z");
    let mut b = cand(
        "Bins",
        Predicate::Scheduled,
        "Tuesday 24 November",
        "Bins go out Tuesday 24 November.",
    );
    b.due = Some(date("2026-11-24"));
    b.span = Some(nocturne_contract::Span::Quote {
        text: "Bins are now Tuesday 24 November".into(),
    });
    let item = keep(&f.engine, "Bins are now Tuesday 24 November");
    let r = f.engine.apply(&item, vec![b]).unwrap();
    assert!(r.replaced.is_empty() && r.conflicts.is_empty(), "{r:?}");
    assert_eq!(all(&f).len(), 2);
}

/// A, then B moves it, then C moves it again. Undoing B leaves C current with A as its
/// history: A's date never comes back as current.
#[test]
fn undoing_the_middle_move_keeps_the_last_one() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let (_, wed) = learn(
        &f.engine,
        "Dentist Wed 18 Nov",
        appointment("Wednesday 18 November", "2026-11-18"),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    let b = keep(&f.engine, "The dentist moved to Thursday 19 November");
    f.engine
        .apply(&b, vec![moved_to("Thursday 19 November", "2026-11-19")])
        .unwrap();
    f.clock.set_str("2026-11-12T12:06:00Z");
    let c = keep(&f.engine, "The dentist moved to Friday 20 November");
    let fri = f
        .engine
        .apply(&c, vec![moved_to("Friday 20 November", "2026-11-20")])
        .unwrap()
        .created[0]
        .clone();
    f.clock.set_str("2026-11-12T12:09:00Z");
    f.engine.unkeep(&b).unwrap();
    let rows = all(&f);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].id, fri);
    assert_eq!(rows[0].was.as_deref(), Some("Wednesday 18 November"));
    assert!(superseded_by(&f, &wed, &fri));
    assert!(f.engine.needs_you().unwrap().is_empty());
}

/// A re-read that finds only another appointment (no move) never gives it the moved
/// appointment's history, and forgetting that other appointment takes nothing else.
#[test]
fn history_never_goes_to_an_unrelated_appointment() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let (_, wed) = learn(
        &f.engine,
        "Dentist Wed 18 Nov",
        appointment("Wednesday 18 November", "2026-11-18"),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    let b = keep(
        &f.engine,
        "The dentist moved to Thursday 19 November. Hygienist 3 December.",
    );
    f.engine
        .apply(&b, vec![moved_to("Thursday 19 November", "2026-11-19")])
        .unwrap();
    f.clock.set_str("2026-11-13T12:00:00Z");
    f.engine
        .apply(
            &b,
            vec![quoted(
                "Thursday 3 December",
                "2026-12-03",
                "Hygienist 3 December",
            )],
        )
        .unwrap();
    let hygienist = all(&f)
        .into_iter()
        .find(|m| m.object == "Thursday 3 December")
        .unwrap();
    assert_eq!(hygienist.was, None);
    assert!(!superseded_by(&f, &wed, &hygienist.id));
    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&hygienist.id), false)
        .unwrap();
    assert_eq!(preview.memories.len(), 1);
}

/// Confirming a value that was replaced is never a silent no-op: it comes back, and the person
/// is asked which of the two is right.
#[test]
fn confirming_a_replaced_value_asks_which_is_right() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let (_, wed) = learn(
        &f.engine,
        "Dentist Wed 18 Nov",
        appointment("Wednesday 18 November", "2026-11-18"),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    let b = keep(&f.engine, "The dentist moved to Thursday 19 November");
    f.engine
        .apply(&b, vec![moved_to("Thursday 19 November", "2026-11-19")])
        .unwrap();
    f.engine.confirm(&wed, true).unwrap();
    assert!(
        all(&f).iter().any(|m| m.id == wed),
        "the confirmed value is current"
    );
    let needs = f.engine.needs_you().unwrap();
    assert!(
        needs
            .iter()
            .any(|e| matches!(e, NeedsYouEntry::Which { .. })),
        "{needs:?}"
    );
}

/// A later trusted note never silently undoes what the person chose: it is asked about.
#[test]
fn a_move_never_overrides_the_persons_choice() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let (_, wed) = learn(
        &f.engine,
        "Dentist on Wednesday 18 November at 10:00",
        appointment("Wednesday 18 November at 10:00", "2026-11-18"),
    );
    f.engine.confirm(&wed, true).unwrap();
    f.clock.set_str("2026-11-12T12:00:00Z");
    let item = keep(
        &f.engine,
        "The dentist moved to Thursday 19 November at 14:30",
    );
    let r = f
        .engine
        .apply(
            &item,
            vec![moved_to("Thursday 19 November at 14:30", "2026-11-19")],
        )
        .unwrap();
    assert!(r.replaced.is_empty());
    assert_eq!(r.conflicts.len(), 1);
}

/// A later untrusted item (pasted from the web, sent by someone else) never moves a person's own
/// appointment: like any untrusted disagreement it waits as unconfirmed, and the trusted value
/// stays current.
#[test]
fn an_untrusted_later_item_does_not_move_an_appointment() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let (_, wed) = learn(
        &f.engine,
        "Dentist on Wednesday 18 November at 10:00",
        appointment("Wednesday 18 November at 10:00", "2026-11-18"),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    let web = keep_as(&f.engine, "Dentist moved to Thursday", Source::PastedWeb);
    let r = f
        .engine
        .apply(
            &web,
            vec![moved_to("Thursday 19 November at 14:30", "2026-11-19")],
        )
        .unwrap();
    assert!(r.conflicts.is_empty() && r.replaced.is_empty());
    assert_eq!(r.unconfirmed.len(), 1);
    let current: Vec<_> = all(&f)
        .into_iter()
        .filter(|m| m.status == MemoryStatus::Active)
        .collect();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].id, wed);
    assert_eq!(current[0].was, None);
}

/// Only trusted items put values in order. An untrusted item agreeing late with the old value
/// does not make it the newer one: the person's own later note still moves the appointment.
#[test]
fn an_untrusted_echo_of_the_old_value_does_not_make_it_newer() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    learn(
        &f.engine,
        "Dentist on Wednesday 18 November at 10:00",
        appointment("Wednesday 18 November at 10:00", "2026-11-18"),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    let moved = keep(
        &f.engine,
        "The dentist moved to Thursday 19 November at 14:30",
    );
    f.clock.set_str("2026-11-12T13:00:00Z");
    let echo = keep_as(&f.engine, "Dentist Wednesday 10:00", Source::PastedWeb);
    f.engine
        .apply(
            &echo,
            vec![appointment("Wednesday 18 November at 10:00", "2026-11-18")],
        )
        .unwrap();
    f.clock.set_str("2026-11-12T22:00:00Z");
    let r = f
        .engine
        .apply(
            &moved,
            vec![moved_to("Thursday 19 November at 14:30", "2026-11-19")],
        )
        .unwrap();
    assert!(r.conflicts.is_empty());
    let rows = all(&f);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].object, "Thursday 19 November at 14:30");
    assert_eq!(
        rows[0].was.as_deref(),
        Some("Wednesday 18 November at 10:00")
    );
}

/// Two notes kept within a few minutes of each other cannot be put in order: ask.
#[test]
fn appointments_kept_too_close_together_to_order_ask_which_is_right() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    learn(
        &f.engine,
        "Dentist on Wednesday 18 November at 10:00",
        appointment("Wednesday 18 November at 10:00", "2026-11-18"),
    );
    f.clock.set_str("2026-11-12T08:07:00Z");
    let item = keep(&f.engine, "Dentist moved to Thursday 19 November at 14:30");
    let r = f
        .engine
        .apply(
            &item,
            vec![moved_to("Thursday 19 November at 14:30", "2026-11-19")],
        )
        .unwrap();
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(all(&f).len(), 2);
}

/// An arrangement with no date (a club that meets monthly) said two ways is a contradiction,
/// however far apart the notes are: one of them is wrong, and only the person knows which.
#[test]
fn an_undated_arrangement_said_two_ways_asks_which_is_right() {
    let f = fixture_at("2026-11-12T11:15:00Z", "Europe/London");
    learn(
        &f.engine,
        "Book club meets on the first Tuesday",
        cand(
            "Book club",
            Predicate::Scheduled,
            "first Tuesday of the month",
            "Book club meets on the first Tuesday of the month.",
        ),
    );
    f.clock.set_str("2026-11-12T16:10:00Z");
    let item = keep(&f.engine, "Sam says book club is on the second Wednesday");
    let r = f
        .engine
        .apply(
            &item,
            vec![cand(
                "Book club",
                Predicate::Scheduled,
                "second Wednesday of the month",
                "Book club meets on the second Wednesday of the month.",
            )],
        )
        .unwrap();
    assert_eq!(r.conflicts.len(), 1);
}

/// An item never shows as understood without its reading time: both are written together.
#[test]
fn an_item_is_understood_and_timed_in_one_step() {
    let f = fixture();
    let item = keep(&f.engine, "Gran lives in Leeds");
    let lease = f
        .engine
        .lease_item(&item, std::time::Duration::from_secs(60))
        .unwrap()
        .lease;
    f.engine
        .apply_candidates_took(
            &item,
            &lease,
            vec![cand(
                "Gran",
                Predicate::LivesIn,
                "Leeds",
                "Gran lives in Leeds.",
            )],
            std::time::Duration::from_millis(4_200),
        )
        .unwrap();
    let view = f.engine.get_item(&item).unwrap();
    assert_eq!(view.status, nocturne_contract::ItemStatus::Understood);
    assert_eq!(view.understood_in_ms, Some(4_200));
}

#[test]
fn only_single_valued_predicates_conflict() {
    let f = fixture();
    learn(
        &f.engine,
        "a",
        cand("Leo", Predicate::Knows, "Sam", "Leo knows Sam."),
    );
    let item = keep(&f.engine, "b");
    let r = f
        .engine
        .apply(
            &item,
            vec![cand("Leo", Predicate::Knows, "Maya", "Leo knows Maya.")],
        )
        .unwrap();
    assert!(r.conflicts.is_empty());

    // Preferences conflict only when their tags share a category.
    let mut tea = cand("Mum", Predicate::Prefers, "tea", "Mum prefers tea.");
    tea.tags = vec!["drink".into()];
    learn(&f.engine, "c", tea);
    let mut coffee = cand("Mum", Predicate::Prefers, "coffee", "Mum prefers coffee.");
    coffee.tags = vec!["drink".into()];
    let item = keep(&f.engine, "d");
    assert_eq!(
        f.engine.apply(&item, vec![coffee]).unwrap().conflicts.len(),
        1
    );
    let mut blue = cand("Mum", Predicate::Prefers, "blue", "Mum prefers blue.");
    blue.tags = vec!["colour".into()];
    let item = keep(&f.engine, "e");
    assert!(
        f.engine
            .apply(&item, vec![blue])
            .unwrap()
            .conflicts
            .is_empty()
    );
}

#[test]
fn an_untrusted_disagreement_creates_unconfirmed_and_never_a_which() {
    let f = fixture();
    learn(
        &f.engine,
        "Gran lives in Leeds",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    let agent = keep_as(&f.engine, "gran lives in Hull", Source::Agent);
    let r = f
        .engine
        .apply(
            &agent,
            vec![cand(
                "Gran",
                Predicate::LivesIn,
                "Hull",
                "Gran lives in Hull.",
            )],
        )
        .unwrap();
    assert!(r.conflicts.is_empty());
    assert_eq!(r.unconfirmed.len(), 1);
    assert!(
        f.engine
            .needs_you()
            .unwrap()
            .iter()
            .all(|e| matches!(e, NeedsYouEntry::Unconfirmed { .. }))
    );
}

#[test]
fn correct_closes_the_old_row_into_history_and_cites_the_same_sources() {
    let f = fixture();
    let mut c = cand(
        "Leo",
        Predicate::Scheduled,
        "Wednesday",
        "Leo swims on Wednesday.",
    );
    c.valid_from = Some(date("2026-11-01"));
    let (item, old) = learn(&f.engine, "Leo swims on Wednesday", c);
    f.clock.set_str("2026-11-15T10:00:00Z");
    let new = f
        .engine
        .correct(&old, nocturne_engine::CorrectPatch::object("Monday"))
        .unwrap();
    assert_ne!(new, old);

    let rows = all(&f);
    assert_eq!(rows.len(), 1, "current rows only");
    let m = &rows[0];
    assert_eq!(m.id, new);
    assert_eq!(m.sentence, "Leo swims on Monday.");
    assert_eq!(m.was.as_deref(), Some("Wednesday"));
    assert_eq!(m.certainty, Certainty::Sure);
    assert_eq!(m.receipts.len(), 1);
    assert_eq!(m.receipts[0].item_id, item);
    assert_eq!(
        f.engine.get_item(&item).unwrap().memory_count,
        1,
        "counted once"
    );
    assert_eq!(m.valid_from.as_deref(), Some("2026-11-01"));

    // The old row is history: superseded, dates untouched (wrong, not ended), still has its source.
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM memories WHERE superseded_at IS NOT NULL"
        ),
        1
    );
    assert_eq!(
        sql_count(
            f.dir.path(),
            &format!(
                "SELECT COUNT(*) FROM memories WHERE id = '{old}' AND valid_to IS NULL AND superseded_by = '{new}'"
            )
        ),
        1
    );
    assert_eq!(
        sql_count(
            f.dir.path(),
            &format!("SELECT COUNT(*) FROM memory_sources WHERE memory_id = '{old}'")
        ),
        1
    );
    assert!(
        f.engine
            .correct(&old, nocturne_engine::CorrectPatch::object("Tuesday"))
            .is_err(),
        "only current rows can be corrected"
    );

    // A re-read of the original does not resurrect the wrong value.
    let mut again = cand(
        "Leo",
        Predicate::Scheduled,
        "Wednesday",
        "Leo swims on Wednesday.",
    );
    again.valid_from = Some(date("2026-11-01"));
    let r = f.engine.apply(&item, vec![again]).unwrap();
    assert!(r.created.is_empty() && r.conflicts.is_empty());
    assert_eq!(all(&f).len(), 1);
}

#[test]
fn correcting_by_text_replaces_the_sentence_and_keeps_one_history_line() {
    let f = fixture();
    let mut c = cand(
        "Trinks",
        Predicate::Scheduled,
        "6.30",
        "Trinks with friends at 6.30 today.",
    );
    c.subject_kind = Some(nocturne_contract::EntityKind::Thing);
    let (_, first) = learn(&f.engine, "Trinks with friends at 6.30 today.", c);
    let fixed = "Drinks with friends at 6.30 today.";
    let second = f
        .engine
        .correct(&first, nocturne_engine::CorrectPatch::text(fixed))
        .unwrap();
    let rows = all(&f);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, second);
    assert_eq!(rows[0].sentence, fixed);
    assert_eq!(
        rows[0].was.as_deref(),
        Some("Trinks with friends at 6.30 today.")
    );
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM memories WHERE superseded_at IS NOT NULL"
        ),
        1
    );

    let again = "Drinks with friends at 7 today.";
    let third = f
        .engine
        .correct(&second, nocturne_engine::CorrectPatch::text(again))
        .unwrap();
    let rows = all(&f);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, third);
    assert_eq!(rows[0].sentence, again);
    assert_eq!(rows[0].was.as_deref(), Some(fixed));
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM memories WHERE superseded_at IS NOT NULL"
        ),
        2
    );
    assert_eq!(f.engine.history(&third).unwrap().len(), 3);
    let entities = f.engine.entities_matching(None, None).unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(
        entities[0].name, "Trinks",
        "a correction leaves the entity alone"
    );
    assert_eq!(entities[0].memory_count, 1);
}

#[test]
fn certainty_mapping() {
    let f = fixture();
    let mut low = cand("Pat", Predicate::Knows, "a rumour", "Pat knows a rumour.");
    low.confidence = 0.4;
    learn(&f.engine, "x", low);
    let (_, clear) = learn(
        &f.engine,
        "y",
        cand("Pat", Predicate::Decided, "to move", "Pat decided to move."),
    );
    let rows = all(&f);
    assert_eq!(
        rows.iter().find(|m| m.id == clear).unwrap().certainty,
        Certainty::FairlySure
    );
    assert_eq!(
        rows.iter()
            .find(|m| m.object == "a rumour")
            .unwrap()
            .certainty,
        Certainty::NotSure
    );
    f.engine.confirm(&clear, true).unwrap();
    assert_eq!(
        f.engine.get_memory(&clear).unwrap().certainty,
        Certainty::Sure
    );
}

#[test]
fn safety_memories_are_keep_forever_and_suggest_private() {
    let f = fixture();
    learn(
        &f.engine,
        "Leo takes metformin",
        cand(
            "Leo",
            Predicate::Takes,
            "metformin 500mg",
            "Leo takes metformin 500mg.",
        ),
    );
    let mut allergy = cand(
        "Leo",
        Predicate::Knows,
        "peanut allergy",
        "Leo is allergic to peanuts.",
    );
    allergy.tags = vec!["allergy".into()];
    learn(&f.engine, "Leo allergy", allergy);
    learn(
        &f.engine,
        "Leo likes jam",
        cand("Leo", Predicate::Prefers, "jam", "Leo likes jam."),
    );
    for m in all(&f) {
        let safety = m.object != "jam";
        assert_eq!(m.keep_forever, safety, "{}", m.sentence);
        assert_eq!(m.suggest_private, safety, "{}", m.sentence);
        assert!(!m.private, "never hidden silently");
    }
}

#[test]
fn a_new_dose_closes_the_old_one_and_the_same_dose_agrees() {
    let f = fixture();
    learn(
        &f.engine,
        "metformin 500",
        cand(
            "Leo",
            Predicate::Takes,
            "metformin 500mg",
            "Leo takes metformin 500mg.",
        ),
    );
    let mut up = cand(
        "Leo",
        Predicate::Takes,
        "metformin 850mg",
        "Leo takes metformin 850mg.",
    );
    up.valid_from = Some(date("2026-11-10"));
    learn(&f.engine, "metformin 850", up);
    let rows = all(&f);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .find(|m| m.object == "metformin 500mg")
            .unwrap()
            .valid_to
            .as_deref(),
        Some("2026-11-10")
    );
    assert!(f.engine.needs_you().unwrap().is_empty());
    // A different drug is independent.
    let item = keep(&f.engine, "aspirin");
    let r = f
        .engine
        .apply(
            &item,
            vec![cand(
                "Leo",
                Predicate::Takes,
                "aspirin",
                "Leo takes aspirin.",
            )],
        )
        .unwrap();
    assert!(r.conflicts.is_empty());
}

#[test]
fn memory_filters_and_search_text() {
    let f = fixture();
    learn(
        &f.engine,
        "n1",
        cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming."),
    );
    learn(
        &f.engine,
        "n2",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    let hits = f
        .engine
        .memories(MemoryFilter {
            query: Some("swim".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].subject, "Leo");
    let hits = f
        .engine
        .memories(MemoryFilter {
            limit: Some(1),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn entities_group_memories() {
    let f = fixture();
    let mut c = cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming.");
    c.subject_kind = Some(nocturne_contract::EntityKind::Person);
    learn(&f.engine, "n1", c);
    // A later candidate for the same subject, with no kind given, joins the entity.
    learn(
        &f.engine,
        "n2",
        cand("leo", Predicate::Knows, "Sam", "Leo knows Sam."),
    );
    let people = f
        .engine
        .entities(Some(nocturne_contract::EntityKind::Person))
        .unwrap();
    assert_eq!(people.len(), 1);
    assert_eq!((people[0].memory_count, people[0].item_count), (2, 2));
    let (view, mems) = f.engine.entity(&people[0].id).unwrap();
    assert_eq!(view.name, "Leo");
    assert_eq!(mems.len(), 2);
    assert!(
        f.engine
            .entities(Some(nocturne_contract::EntityKind::Place))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn an_untrusted_reread_cannot_rewrite_a_memory_trusted_sources_support() {
    let f = fixture();
    let (_, id) = learn(
        &f.engine,
        "I rang: clinic opens at 9",
        cand(
            "Clinic",
            Predicate::Knows,
            "opens at 9",
            "The clinic opens at 9.",
        ),
    );
    let web = keep_as(&f.engine, "web page about the clinic", Source::PastedWeb);
    let agrees = cand(
        "Clinic",
        Predicate::Knows,
        "opens at 9",
        "The clinic opens at 9.",
    );
    f.engine.apply(&web, vec![agrees]).unwrap();

    // Read again, now claiming different wording, confidence and dates.
    let mut rewritten = cand(
        "Clinic",
        Predicate::Knows,
        "opens at 9",
        "Everything here is free.",
    );
    rewritten.confidence = 0.1;
    rewritten.valid_from = Some(date("2020-01-01"));
    rewritten.due = Some(date("2020-01-02"));
    f.engine.apply(&web, vec![rewritten]).unwrap();
    let m = f.engine.get_memory(&id).unwrap();
    assert_eq!(m.sentence, "The clinic opens at 9.");
    assert_eq!(
        m.certainty,
        Certainty::FairlySure,
        "the web source does not add certainty"
    );
    assert_eq!((m.valid_from, m.due), (None, None));
    assert_eq!(m.receipts.len(), 2);
}

#[test]
fn an_untrusted_reread_may_refine_an_unconfirmed_memory_it_alone_supports() {
    let f = fixture();
    let web = keep_as(&f.engine, "page", Source::PastedWeb);
    let r = f
        .engine
        .apply(
            &web,
            vec![cand(
                "Clinic",
                Predicate::Knows,
                "opens at 9",
                "The clinic opens at 9.",
            )],
        )
        .unwrap();
    f.engine
        .apply(
            &web,
            vec![cand(
                "Clinic",
                Predicate::Knows,
                "opens at 9",
                "The clinic opens at nine.",
            )],
        )
        .unwrap();
    let m = f.engine.get_memory(&r.created[0]).unwrap();
    assert_eq!(m.sentence, "The clinic opens at nine.");
    assert_eq!(m.status, MemoryStatus::Unconfirmed);
}

#[test]
fn a_date_only_match_never_promotes_an_unconfirmed_memory() {
    let f = fixture();
    let web = keep_as(&f.engine, "web: form due Friday", Source::PastedWeb);
    let mut a = cand(
        "Trip form",
        Predicate::Due,
        "Friday",
        "The trip form is due Friday.",
    );
    a.due = Some(date("2026-11-14"));
    let r = f.engine.apply(&web, vec![a]).unwrap();
    let unconfirmed = r.created[0].clone();

    // An untrusted candidate with the same date but different words: a new unconfirmed memory.
    let agent = keep_as(&f.engine, "agent: form due 14 Nov", Source::Agent);
    let mut b = cand(
        "Trip form",
        Predicate::Due,
        "14 Nov",
        "The trip form is due on 14 November.",
    );
    b.due = Some(date("2026-11-14"));
    let r = f.engine.apply(&agent, vec![b]).unwrap();
    assert_eq!(r.created.len(), 1);
    assert!(r.promoted.is_empty() && r.reinforced.is_empty());
    assert_eq!(
        f.engine.get_memory(&unconfirmed).unwrap().status,
        MemoryStatus::Unconfirmed
    );

    // A trusted item with the same date but different words does not promote it either.
    let note = keep(&f.engine, "my note: form due 14 Nov");
    let mut c = cand(
        "Trip form",
        Predicate::Due,
        "Fri 14th",
        "The trip form is due on Fri 14th.",
    );
    c.due = Some(date("2026-11-14"));
    let r = f.engine.apply(&note, vec![c]).unwrap();
    assert!(r.promoted.is_empty());
    assert_eq!(
        f.engine.get_memory(&unconfirmed).unwrap().status,
        MemoryStatus::Unconfirmed
    );
}
