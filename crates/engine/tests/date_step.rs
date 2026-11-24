//! The date step (docs/design/ask-with-tools.md#the-date-step): every phrase of its table, read
//! against a fixed day, at a week's, a month's and a year's turn.

use jiff::civil::{Date, Weekday};
use nocturne_contract::ReadingKind;
use nocturne_engine::date_step;

fn d(s: &str) -> Date {
    s.parse().unwrap()
}

/// The days `question` names on `today`, each range as "from..to".
fn days(question: &str, today: &str) -> Vec<String> {
    date_step(question, d(today), Weekday::Monday)
        .ranges
        .iter()
        .map(|(a, b)| format!("{a}..{b}"))
        .collect()
}

fn one(question: &str, today: &str) -> String {
    let all = days(question, today);
    assert_eq!(all.len(), 1, "{question}: {all:?}");
    all[0].clone()
}

/// Tuesday 24 November 2026, the day of the design's examples.
const TUE: &str = "2026-11-24";

#[test]
fn the_table_on_the_examples_day() {
    for (q, want) in [
        ("What's on today?", "2026-11-24..2026-11-24"),
        ("Anything tonight?", "2026-11-24..2026-11-24"),
        ("What did I do this morning?", "2026-11-24..2026-11-24"),
        ("Am I busy this afternoon?", "2026-11-24..2026-11-24"),
        ("Anything this evening?", "2026-11-24..2026-11-24"),
        ("What's on tomorrow?", "2026-11-25..2026-11-25"),
        ("Am I free tomorrow morning?", "2026-11-25..2026-11-25"),
        ("And the day after tomorrow?", "2026-11-26..2026-11-26"),
        ("What did I do yesterday?", "2026-11-23..2026-11-23"),
        ("How did I sleep last night?", "2026-11-23..2026-11-23"),
        ("What's on Thursday?", "2026-11-26..2026-11-26"),
        ("Am I free on Tuesday?", "2026-11-24..2026-11-24"),
        ("What did I do on Sunday?", "2026-11-22..2026-11-22"),
        ("Who am I seeing on Tuesday?", "2026-11-24..2026-11-24"),
        ("Who did I see on Tuesday?", "2026-11-17..2026-11-17"),
        ("Who was at lunch on Tuesday?", "2026-11-17..2026-11-17"),
        ("What did I do last Tuesday?", "2026-11-17..2026-11-17"),
        ("Anything next Tuesday?", "2026-12-01..2026-12-01"),
        ("What's on this week?", "2026-11-23..2026-11-29"),
        ("How did I sleep this week?", "2026-11-23..2026-11-24"),
        ("What will I do this week?", "2026-11-24..2026-11-29"),
        ("What's on next week?", "2026-11-30..2026-12-06"),
        ("How was I last week?", "2026-11-16..2026-11-22"),
        ("Anything this weekend?", "2026-11-28..2026-11-29"),
        ("Am I busy at the weekend?", "2026-11-28..2026-11-29"),
        ("What's on next weekend?", "2026-12-05..2026-12-06"),
        ("What did I do last weekend?", "2026-11-21..2026-11-22"),
        ("What's on this month?", "2026-11-01..2026-11-30"),
        ("What did I do this month?", "2026-11-01..2026-11-24"),
        ("Anything next month?", "2026-12-01..2026-12-31"),
        ("What happened last month?", "2026-10-01..2026-10-31"),
        ("What's on 25 November?", "2026-11-25..2026-11-25"),
        ("What's on 25th November?", "2026-11-25..2026-11-25"),
        ("What's on November 25?", "2026-11-25..2026-11-25"),
        ("What's on the 25th?", "2026-11-25..2026-11-25"),
        ("What's on the 3rd?", "2026-12-03..2026-12-03"),
        ("What did I do on the 20th?", "2026-11-20..2026-11-20"),
        ("What's on the 2nd?", "2026-12-02..2026-12-02"),
        ("How have I been recently?", "2026-11-18..2026-11-24"),
        ("What have I been doing lately?", "2026-11-18..2026-11-24"),
        ("How were these last few days?", "2026-11-18..2026-11-24"),
        ("What's on the next few days?", "2026-11-24..2026-11-27"),
        ("Anything in the coming days?", "2026-11-24..2026-11-27"),
    ] {
        assert_eq!(one(q, TUE), want, "{q}");
    }
}

#[test]
fn words_not_in_the_table_are_no_time() {
    for q in [
        "When is the dentist?",
        "Who is Leo?",
        "How was Leo's parents' evening?",
        "When is the car's MOT?",
        "Is the march still on?",
        "What is the second thing?",
        "Where did I park?",
    ] {
        assert!(days(q, TUE).is_empty(), "{q}: {:?}", days(q, TUE));
        assert!(!date_step(q, d(TUE), Weekday::Monday).names_time(), "{q}");
    }
}

#[test]
fn a_week_s_turn() {
    // Sunday 29 November: this week is still Monday 23 to today.
    assert_eq!(
        one("What's on this week?", "2026-11-29"),
        "2026-11-23..2026-11-29"
    );
    assert_eq!(
        one("What's on tomorrow?", "2026-11-29"),
        "2026-11-30..2026-11-30"
    );
    assert_eq!(
        one("What's on next week?", "2026-11-29"),
        "2026-11-30..2026-12-06"
    );
    // On a Saturday "this weekend" is this one.
    assert_eq!(
        one("Anything this weekend?", "2026-11-28"),
        "2026-11-28..2026-11-29"
    );
    assert_eq!(
        one("Anything this weekend?", "2026-11-29"),
        "2026-11-28..2026-11-29"
    );
    // Monday 30 November: "last Monday" is a week ago, and "Monday" is today.
    assert_eq!(
        one("What did I do last Monday?", "2026-11-30"),
        "2026-11-23..2026-11-23"
    );
    assert_eq!(
        one("What's on Monday?", "2026-11-30"),
        "2026-11-30..2026-11-30"
    );
    assert_eq!(
        one("What did I do on Monday?", "2026-11-30"),
        "2026-11-23..2026-11-23"
    );
}

#[test]
fn a_month_s_and_a_year_s_turn() {
    assert_eq!(
        one("What's on tomorrow?", "2026-11-30"),
        "2026-12-01..2026-12-01"
    );
    assert_eq!(
        one("What did I do yesterday?", "2026-12-01"),
        "2026-11-30..2026-11-30"
    );
    assert_eq!(
        one("What's on tomorrow?", "2026-12-31"),
        "2027-01-01..2027-01-01"
    );
    assert_eq!(
        one("What did I do yesterday?", "2027-01-01"),
        "2026-12-31..2026-12-31"
    );
    assert_eq!(
        one("Anything next month?", "2026-12-10"),
        "2027-01-01..2027-01-31"
    );
    assert_eq!(
        one("What happened last month?", "2027-01-10"),
        "2026-12-01..2026-12-31"
    );
    assert_eq!(
        one("What's on next week?", "2026-12-30"),
        "2027-01-04..2027-01-10"
    );
    // A date in early January asked in December is the coming one.
    assert_eq!(
        one("What's on 3 January?", "2026-12-20"),
        "2027-01-03..2027-01-03"
    );
    // One in December asked in January is the one just gone.
    assert_eq!(
        one("What did I do on 20 December?", "2027-01-05"),
        "2026-12-20..2026-12-20"
    );
    // The nearest within six months, either way.
    assert_eq!(
        one("What did I do on 25 August?", TUE),
        "2026-08-25..2026-08-25"
    );
    assert_eq!(one("What's on 25 February?", TUE), "2027-02-25..2027-02-25");
    // A day that is not one is no day at all.
    assert!(days("What's on 31 November?", TUE).is_empty());
}

#[test]
fn the_week_starts_on_the_device_s_day() {
    let step = date_step("What's on this week?", d(TUE), Weekday::Sunday);
    assert_eq!(step.ranges, vec![(d("2026-11-22"), d("2026-11-28"))]);
}

#[test]
fn a_question_naming_a_kind_of_reading_fetches_it() {
    let step = date_step("How did I sleep this week?", d(TUE), Weekday::Monday);
    assert_eq!(step.kinds, vec![ReadingKind::Sleep]);
    let step = date_step("How many steps yesterday?", d(TUE), Weekday::Monday);
    assert_eq!(step.kinds, vec![ReadingKind::Steps]);
    let step = date_step(
        "What was my blood pressure and weight last week?",
        d(TUE),
        Weekday::Monday,
    );
    assert_eq!(
        step.kinds,
        vec![ReadingKind::BloodPressure, ReadingKind::Weight]
    );
    let step = date_step("How was my heart yesterday?", d(TUE), Weekday::Monday);
    assert_eq!(
        step.kinds,
        vec![ReadingKind::RestingHeartRate, ReadingKind::HeartRate]
    );
    let step = date_step("Where did I run yesterday?", d(TUE), Weekday::Monday);
    assert!(step.kinds.is_empty(), "'run' is not in the list; 'ran' is");
    let step = date_step("When I ran yesterday", d(TUE), Weekday::Monday);
    assert_eq!(step.kinds, vec![ReadingKind::Workout]);
}

#[test]
fn several_times_in_one_question_each_count_once() {
    assert_eq!(
        days("What did I do yesterday, and what's on tomorrow?", TUE),
        vec!["2026-11-23..2026-11-23", "2026-11-25..2026-11-25"]
    );
    assert_eq!(days("Today or today?", TUE), vec!["2026-11-24..2026-11-24"]);
}
