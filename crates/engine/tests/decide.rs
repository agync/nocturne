//! Decisions in shadow: what the rules said is logged beside what a decider said, nothing the
//! person sees changes, and the log goes with what it is about.

mod common;

use std::collections::BTreeMap;

use common::*;
use nocturne_contract::{Edition, EntityKind, MemoryFilter, Span};
use nocturne_engine::decide::{Answer, Backend, Kind, Settled, threshold, verdict};
use nocturne_engine::{Candidate, Predicate};

const FIRST: &str = "Dentist appointment with Dr Okafor on Wednesday 18 November at 10:00.";
const MOVED: &str = "The dentist appointment has moved. It is now Thursday 19 November at 14:30.";
const OTHER: &str = "Hygienist with Dr Okafor on Thursday 3 December at 09:00.";

fn dentist(object: &str, on: &str, quote: &str) -> Candidate {
    let mut c = cand(
        "Dentist appointment",
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

/// The first appointment kept in the morning; the clock then moves to midday.
fn with_appointment() -> (Fixture, String) {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let (_, id) = learn(
        &f.engine,
        FIRST,
        dentist("Wednesday 18 November at 10:00", "2026-11-18", FIRST),
    );
    f.clock.set_str("2026-11-12T12:00:00Z");
    (f, id)
}

fn answer(label: &str, p: f64) -> Answer {
    let mut probabilities = BTreeMap::new();
    probabilities.insert(label.to_string(), p);
    Answer {
        label: label.to_string(),
        probabilities,
        by: "scripted".into(),
    }
}

/// Every line of a page, as words with what they cite; line ids are made afresh each time.
fn page_words(page: &nocturne_contract::PageView) -> Vec<(String, Vec<String>)> {
    [
        &page.needs_you,
        &page.changed,
        &page.let_go,
        &page.upcoming,
        &page.learned,
    ]
    .into_iter()
    .flatten()
    .map(|l| (l.text.clone(), l.memory_ids.clone()))
    .collect()
}

fn moved_rows(f: &Fixture) -> Vec<nocturne_engine::decide::Pending> {
    f.engine
        .pending_decisions(50)
        .unwrap()
        .into_iter()
        .filter(|p| p.kind == Kind::Moved)
        .collect()
}

#[test]
fn a_move_the_rule_saw_is_logged_with_both_appointments() {
    let (f, old) = with_appointment();
    let item = keep(&f.engine, MOVED);
    let r = f
        .engine
        .apply(
            &item,
            vec![dentist(
                "Thursday 19 November at 14:30",
                "2026-11-19",
                MOVED,
            )],
        )
        .unwrap();
    assert_eq!(r.replaced, [old], "the rule still acts: {r:?}");
    let rows = moved_rows(&f);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].rule, "the rule said moved");
    assert!(rows[0].decision.state.contains("Kept appointment:"));
    assert!(rows[0].decision.state.contains("Wednesday 18 November"));
    assert!(rows[0].decision.state.contains("Thursday 19 November"));
    assert_eq!(rows[0].decision.question.labels().len(), 4);
}

#[test]
fn a_second_appointment_the_rule_kept_apart_is_logged_as_not_moved() {
    let (f, _) = with_appointment();
    let item = keep(&f.engine, OTHER);
    f.engine
        .apply(
            &item,
            vec![dentist("Thursday 3 December at 09:00", "2026-12-03", OTHER)],
        )
        .unwrap();
    let rows = moved_rows(&f);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(!rows[0].rule);
}

#[test]
fn settling_records_the_decider_beside_the_rule_and_changes_nothing() {
    let (f, old) = with_appointment();
    let item = keep(&f.engine, MOVED);
    f.engine
        .apply(
            &item,
            vec![dentist(
                "Thursday 19 November at 14:30",
                "2026-11-19",
                MOVED,
            )],
        )
        .unwrap();
    let before = f.engine.memories(MemoryFilter::default()).unwrap();
    let page_before = f
        .engine
        .compose_page(date("2026-11-13"), Edition::Screen)
        .unwrap();
    let row = moved_rows(&f).remove(0);
    f.engine
        .settle_decision(
            &row.id,
            &Settled {
                backend: Backend::Mind,
                answer: Ok(answer("same", 1.0)),
                ms: 812,
            },
        )
        .unwrap();
    assert!(
        moved_rows(&f).is_empty(),
        "a settled decision is not asked again"
    );
    let log = f.engine.decision_log().unwrap();
    let logged = log.iter().find(|l| l.id == row.id).unwrap();
    assert!(logged.rule);
    assert_eq!(logged.label.as_deref(), Some("same"));
    assert_eq!(logged.verdict, Some(false));
    assert_eq!(logged.ms, Some(812));
    assert!(logged.error.is_none());
    assert_eq!(f.engine.memories(MemoryFilter::default()).unwrap(), before);
    let page_after = f
        .engine
        .compose_page(date("2026-11-13"), Edition::Screen)
        .unwrap();
    assert_eq!(page_words(&page_after), page_words(&page_before));
    assert!(
        f.engine
            .memories(MemoryFilter::default())
            .unwrap()
            .iter()
            .all(|m| m.id != old),
        "the old appointment stays replaced"
    );
}

#[test]
fn an_error_is_logged_and_the_rule_stands() {
    let (f, _) = with_appointment();
    let item = keep(&f.engine, MOVED);
    f.engine
        .apply(
            &item,
            vec![dentist(
                "Thursday 19 November at 14:30",
                "2026-11-19",
                MOVED,
            )],
        )
        .unwrap();
    let row = moved_rows(&f).remove(0);
    f.engine
        .settle_decision(
            &row.id,
            &Settled {
                backend: Backend::Mind,
                answer: Err("timed out".into()),
                ms: 20_000,
            },
        )
        .unwrap();
    let log = f.engine.decision_log().unwrap();
    let logged = log.iter().find(|l| l.id == row.id).unwrap();
    assert_eq!(logged.error.as_deref(), Some("timed out"));
    assert_eq!(
        logged.verdict, None,
        "no verdict: the rule's outcome stands"
    );
}

#[test]
fn forgetting_the_memory_takes_its_decisions_with_it() {
    let (f, _) = with_appointment();
    let item = keep(&f.engine, MOVED);
    let r = f
        .engine
        .apply(
            &item,
            vec![dentist(
                "Thursday 19 November at 14:30",
                "2026-11-19",
                MOVED,
            )],
        )
        .unwrap();
    assert_eq!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM decisions"), 1);
    let preview = f.engine.preview_forget(&r.created, true).unwrap();
    f.engine.forget(&preview.token).unwrap();
    assert_eq!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM decisions"), 0);
}

#[test]
fn delete_everything_empties_the_log() {
    let (f, _) = with_appointment();
    let item = keep(&f.engine, MOVED);
    f.engine
        .apply(
            &item,
            vec![dentist(
                "Thursday 19 November at 14:30",
                "2026-11-19",
                MOVED,
            )],
        )
        .unwrap();
    f.engine
        .queue_decisions(date("2026-11-13"), "2026-11-11T00:00:00Z".parse().unwrap())
        .unwrap();
    assert!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM decisions") > 0);
    f.engine
        .delete_everything(nocturne_engine::DELETE_PHRASE)
        .unwrap();
    assert_eq!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM decisions"), 0);
}

#[test]
fn the_health_money_and_id_check_is_queued_for_new_memories_with_the_rule_outcome() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let mut tagged = cand(
        "Me",
        Predicate::Scheduled,
        "counsellor on Friday",
        "I am seeing a counsellor on Friday because I have been feeling low.",
    );
    tagged.tags = vec!["health".into()];
    learn(
        &f.engine,
        "Seeing a counsellor on Friday, feeling low.",
        tagged,
    );
    learn(
        &f.engine,
        "Gran lives in Leeds.",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    f.clock.set_str("2026-11-13T03:00:00Z");
    let queued = f
        .engine
        .queue_decisions(date("2026-11-13"), "2026-11-12T00:00:00Z".parse().unwrap())
        .unwrap();
    assert_eq!(queued, 2);
    let mut rows: Vec<_> = f
        .engine
        .pending_decisions(50)
        .unwrap()
        .into_iter()
        .filter(|p| p.kind == Kind::Sensitive)
        .map(|p| (p.decision.state.contains("counsellor"), p.rule))
        .collect();
    rows.sort();
    assert_eq!(rows, [(false, false), (true, true)]);
    let again = f
        .engine
        .queue_decisions(date("2026-11-13"), "2026-11-12T00:00:00Z".parse().unwrap())
        .unwrap();
    assert_eq!(again, 0, "a memory is checked once");
}

#[test]
fn a_page_question_a_later_note_stopped_is_queued_as_followed_up() {
    let f = fixture();
    f.engine
        .save_page(
            &f.engine
                .compose_page(date("2026-11-10"), Edition::Screen)
                .unwrap(),
        )
        .unwrap();
    f.clock.set_str("2026-11-13T19:00:00Z");
    let entry = "Parents\u{2019} evening at six. Not looking forward to it.";
    let block = f
        .engine
        .save_entry_text(date("2026-11-13"), None, 0, entry)
        .unwrap()
        .unwrap()
        .item
        .id;
    f.engine.release_entry_holds().unwrap();
    let mut c = cand(
        "Parents evening",
        Predicate::Scheduled,
        "at six",
        "Parents evening is at six.",
    );
    c.subject_kind = Some(EntityKind::Thing);
    c.due = Some(date("2026-11-13"));
    c.span = Some(Span::Quote { text: entry.into() });
    f.engine.apply(&block, vec![c]).unwrap();
    f.clock.set_str("2026-11-13T21:00:00Z");
    keep(
        &f.engine,
        "Parents evening went fine, the teacher was kind.",
    );
    f.clock.set_str("2026-11-14T04:00:00Z");
    f.engine
        .queue_decisions(date("2026-11-14"), "2026-11-14T00:00:00Z".parse().unwrap())
        .unwrap();
    let rows: Vec<_> = f
        .engine
        .pending_decisions(50)
        .unwrap()
        .into_iter()
        .filter(|p| p.kind == Kind::FollowedUp)
        .collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].rule, "the rule stopped the question");
    assert!(
        rows[0]
            .decision
            .state
            .contains("Not looking forward to it.")
    );
    assert!(rows[0].decision.state.contains("the teacher was kind"));
}

#[test]
fn thresholds_live_in_code_per_question_and_per_backend() {
    for kind in [Kind::Moved, Kind::Sensitive, Kind::FollowedUp] {
        for backend in [Backend::Mind, Backend::LlamaCpp, Backend::Flm] {
            let t = threshold(kind, backend);
            assert!((0.5..1.0).contains(&t), "{kind:?} {backend:?} {t}");
        }
    }
    assert!(
        threshold(Kind::Moved, Backend::LlamaCpp) > threshold(Kind::Sensitive, Backend::LlamaCpp)
    );
    // Below the threshold the decider has no verdict, either way.
    let mut unsure = answer("moved", 0.55);
    unsure.probabilities.insert("same".into(), 0.45);
    assert_eq!(verdict(Kind::Moved, Backend::LlamaCpp, &unsure), None);
    assert_eq!(
        verdict(Kind::Moved, Backend::LlamaCpp, &answer("cancelled", 0.97)),
        Some(true)
    );
    assert_eq!(
        verdict(Kind::Sensitive, Backend::Flm, &answer("yes", 0.9)),
        Some(true)
    );
    assert_eq!(
        verdict(Kind::FollowedUp, Backend::Mind, &answer("no", 1.0)),
        Some(false)
    );
}

#[test]
fn undoing_the_keep_a_quote_came_from_takes_the_row_with_it() {
    let f = fixture_at("2026-11-12T08:05:00Z", "Europe/London");
    let first = "Seeing a counsellor on Friday, feeling low.";
    let mut c = cand(
        "Me",
        Predicate::Scheduled,
        "counsellor on Friday",
        "I am seeing a counsellor on Friday.",
    );
    c.span = Some(Span::Quote { text: first.into() });
    let (a, memory) = learn(&f.engine, first, c.clone());
    f.clock.set_str("2026-11-12T08:06:00Z");
    let second = "Counsellor on Friday, booked.";
    c.span = Some(Span::Quote {
        text: second.into(),
    });
    let b = keep(&f.engine, second);
    let r = f.engine.apply(&b, vec![c]).unwrap();
    assert_eq!(r.reinforced, [memory], "{r:?}");
    f.engine
        .queue_decisions(date("2026-11-13"), "2026-11-12T00:00:00Z".parse().unwrap())
        .unwrap();
    let quoted = "SELECT COUNT(*) FROM decisions WHERE state LIKE '%feeling low%'";
    assert_eq!(sql_count(f.dir.path(), quoted), 1);
    f.engine.unkeep(&a).unwrap();
    assert_eq!(
        f.engine.memories(MemoryFilter::default()).unwrap().len(),
        1,
        "the memory stays with its other source"
    );
    assert_eq!(
        sql_count(f.dir.path(), quoted),
        0,
        "no word of the undone keep stays"
    );
}

#[test]
fn undoing_a_later_note_takes_the_followed_up_row_with_it() {
    let f = fixture();
    f.engine
        .save_page(
            &f.engine
                .compose_page(date("2026-11-10"), Edition::Screen)
                .unwrap(),
        )
        .unwrap();
    f.clock.set_str("2026-11-13T19:00:00Z");
    let entry = "Parents\u{2019} evening at six. Not looking forward to it.";
    let block = f
        .engine
        .save_entry_text(date("2026-11-13"), None, 0, entry)
        .unwrap()
        .unwrap()
        .item
        .id;
    f.engine.release_entry_holds().unwrap();
    let mut c = cand(
        "Parents evening",
        Predicate::Scheduled,
        "at six",
        "Parents evening is at six.",
    );
    c.subject_kind = Some(EntityKind::Thing);
    c.due = Some(date("2026-11-13"));
    c.span = Some(Span::Quote { text: entry.into() });
    f.engine.apply(&block, vec![c]).unwrap();
    f.clock.set_str("2026-11-13T21:00:00Z");
    let later = keep(
        &f.engine,
        "Parents evening went fine, the teacher was kind.",
    );
    f.clock.set_str("2026-11-13T21:02:00Z");
    f.engine
        .queue_decisions(date("2026-11-14"), "2026-11-13T00:00:00Z".parse().unwrap())
        .unwrap();
    let quoted = "SELECT COUNT(*) FROM decisions WHERE state LIKE '%teacher was kind%'";
    assert_eq!(sql_count(f.dir.path(), quoted), 1);
    f.engine.unkeep(&later).unwrap();
    assert_eq!(sql_count(f.dir.path(), quoted), 0);
}

#[test]
fn rows_older_than_thirty_days_are_dropped_even_without_a_night_tidy() {
    let (f, _) = with_appointment();
    let item = keep(&f.engine, MOVED);
    f.engine
        .apply(
            &item,
            vec![dentist(
                "Thursday 19 November at 14:30",
                "2026-11-19",
                MOVED,
            )],
        )
        .unwrap();
    assert_eq!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM decisions"), 1);
    f.clock.set_str("2026-12-14T12:00:00Z");
    let late = "Dentist appointment with Dr Okafor on Tuesday 5 January at 10:00.";
    let item = keep(&f.engine, late);
    f.engine
        .apply(
            &item,
            vec![dentist("Tuesday 5 January at 10:00", "2027-01-05", late)],
        )
        .unwrap();
    let old = "SELECT COUNT(*) FROM decisions WHERE created_at < '2026-12-01'";
    assert_eq!(sql_count(f.dir.path(), old), 0);
}
