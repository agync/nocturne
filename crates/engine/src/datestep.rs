//! The date step of Ask (docs/design/ask-with-tools.md#the-date-step): a small, fixed reading of
//! a question in English for the days it names, such as "tomorrow", "last Tuesday" or "this
//! weekend", and the kinds of health reading it names. No mind is involved and nothing is
//! guessed: words not in its table are not a time.

use jiff::ToSpan;
use jiff::civil::{Date, Weekday};
use nocturne_contract::ReadingKind;

/// What a question names: the days, each range with both ends included, in the order the
/// question names them, and the kinds of reading.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DateStep {
    pub ranges: Vec<(Date, Date)>,
    pub kinds: Vec<ReadingKind>,
}

impl DateStep {
    /// Whether the question names a time.
    pub fn names_time(&self) -> bool {
        !self.ranges.is_empty()
    }
}

const WEEKDAYS: [(&str, Weekday); 7] = [
    ("monday", Weekday::Monday),
    ("tuesday", Weekday::Tuesday),
    ("wednesday", Weekday::Wednesday),
    ("thursday", Weekday::Thursday),
    ("friday", Weekday::Friday),
    ("saturday", Weekday::Saturday),
    ("sunday", Weekday::Sunday),
];

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// Words that put a question in the past tense.
const PAST: [&str; 9] = [
    "did", "was", "were", "went", "had", "said", "saw", "met", "slept",
];
/// Words that put it in the future tense.
const FUTURE: [&str; 5] = ["will", "ll", "going", "shall", "gonna"];

/// The words that name a kind of reading, and the kinds they fetch.
const KINDS: [(&[&str], &[ReadingKind]); 8] = [
    (&["sleep"], &[ReadingKind::Sleep]),
    (&["slept"], &[ReadingKind::Sleep]),
    (&["steps"], &[ReadingKind::Steps]),
    (&["walked"], &[ReadingKind::Steps]),
    (
        &["heart"],
        &[ReadingKind::RestingHeartRate, ReadingKind::HeartRate],
    ),
    (&["blood", "pressure"], &[ReadingKind::BloodPressure]),
    (&["weight"], &[ReadingKind::Weight]),
    (&["workout"], &[ReadingKind::Workout]),
];
/// Further words for a workout.
const WORKOUT: [&str; 2] = ["workouts", "ran"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tense {
    Past,
    Future,
    Neither,
}

fn words(question: &str) -> Vec<String> {
    question
        .to_lowercase()
        .replace(['\u{2019}', '\''], " ")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

fn weekday(word: &str) -> Option<Weekday> {
    WEEKDAYS.iter().find(|(w, _)| *w == word).map(|(_, d)| *d)
}

fn month(word: &str) -> Option<i8> {
    MONTHS.iter().position(|m| *m == word).map(|i| i as i8 + 1)
}

/// "25", "25th", "1st", "2nd", "3rd" as a day of the month.
fn day_number(word: &str) -> Option<i8> {
    let digits = word
        .strip_suffix("st")
        .or_else(|| word.strip_suffix("nd"))
        .or_else(|| word.strip_suffix("rd"))
        .or_else(|| word.strip_suffix("th"))
        .unwrap_or(word);
    let n: i8 = digits.parse().ok()?;
    (1..=31).contains(&n).then_some(n)
}

fn add(day: Date, days: i64) -> Date {
    day.checked_add(days.days()).unwrap_or(day)
}

/// The first day of the week that holds `day`.
fn week_of(day: Date, start: Weekday) -> Date {
    add(day, -i64::from(day.weekday().since(start)))
}

/// The next `wd` from `today`, today included.
fn coming(today: Date, wd: Weekday) -> Date {
    add(today, i64::from(wd.since(today.weekday())))
}

/// The last `wd` before `today`.
fn before(today: Date, wd: Weekday) -> Date {
    let back = i64::from(today.weekday().since(wd));
    add(today, if back == 0 { -7 } else { -back })
}

/// The first `wd` after `today`.
fn after(today: Date, wd: Weekday) -> Date {
    let on = i64::from(wd.since(today.weekday()));
    add(today, if on == 0 { 7 } else { on })
}

/// The Saturday and Sunday that are coming, or this one on a Saturday or Sunday.
fn weekend(today: Date) -> (Date, Date) {
    let sat = match today.weekday() {
        Weekday::Sunday => add(today, -1),
        _ => coming(today, Weekday::Saturday),
    };
    (sat, add(sat, 1))
}

fn month_range(first: Date) -> (Date, Date) {
    (first, first.last_of_month())
}

/// The `month`/`day` nearest to `today` within six months, if that is a day.
fn nearest(today: Date, month: i8, day: i8) -> Option<Date> {
    let low = today.checked_sub(6.months()).ok()?;
    let high = today.checked_add(6.months()).ok()?;
    [today.year() - 1, today.year(), today.year() + 1]
        .into_iter()
        .filter_map(|y| Date::new(y, month, day).ok())
        .filter(|d| *d >= low && *d <= high)
        .min_by_key(|d| {
            (
                today.until(*d).map_or(i32::MAX, |s| s.get_days().abs()),
                *d < today,
            )
        })
}

/// "the 25th": the 25th of this month, the next or the last, whichever is nearest.
fn nearest_day(today: Date, day: i8) -> Option<Date> {
    let first = today.first_of_month();
    [-1i64, 0, 1]
        .into_iter()
        .filter_map(|m| first.checked_add(m.months()).ok())
        .filter_map(|f| Date::new(f.year(), f.month(), day).ok())
        .min_by_key(|d| {
            (
                today.until(*d).map_or(i32::MAX, |s| s.get_days().abs()),
                *d < today,
            )
        })
}

/// The days and kinds of reading `question` names, read on `today` with weeks starting on
/// `week_start`.
pub fn date_step(question: &str, today: Date, week_start: Weekday) -> DateStep {
    let w = words(question);
    let at = |i: usize| w.get(i).map(String::as_str).unwrap_or("");
    let tense = if w.iter().any(|x| PAST.contains(&x.as_str())) {
        Tense::Past
    } else if w.iter().any(|x| FUTURE.contains(&x.as_str())) {
        Tense::Future
    } else {
        Tense::Neither
    };
    let mut step = DateStep::default();
    let mut push = |r: (Date, Date)| {
        if !step.ranges.contains(&r) {
            step.ranges.push(r);
        }
    };
    let one = |d: Date| (d, d);
    let mut i = 0;
    while i < w.len() {
        let word = at(i);
        let next = at(i + 1);
        let mut used = 1;
        match word {
            "the" if next == "day" && at(i + 2) == "after" && at(i + 3) == "tomorrow" => {
                push(one(add(today, 2)));
                used = 4;
            }
            "today" | "tonight" => push(one(today)),
            "this" if matches!(next, "morning" | "afternoon" | "evening") => {
                push(one(today));
                used = 2;
            }
            "tomorrow" => push(one(add(today, 1))),
            "yesterday" => push(one(add(today, -1))),
            "last" | "next" | "this" => {
                let last = word == "last";
                let this = word == "this";
                if let Some(wd) = weekday(next) {
                    push(one(match word {
                        "last" => before(today, wd),
                        "next" => after(today, wd),
                        _ if tense == Tense::Past => before(today, wd),
                        _ => coming(today, wd),
                    }));
                    used = 2;
                } else if next == "night" && last {
                    push(one(add(today, -1)));
                    used = 2;
                } else if next == "week" {
                    let monday = week_of(today, week_start);
                    let sunday = add(monday, 6);
                    push(match word {
                        "last" => (add(monday, -7), add(monday, -1)),
                        "next" => (add(monday, 7), add(monday, 13)),
                        _ => match tense {
                            Tense::Past => (monday, today),
                            Tense::Future => (today, sunday),
                            Tense::Neither => (monday, sunday),
                        },
                    });
                    used = 2;
                } else if next == "weekend" {
                    let (sat, sun) = weekend(today);
                    push(match word {
                        // The weekend before this one: on a Sunday, not the day before.
                        "last" => {
                            let sat = before(today, Weekday::Saturday);
                            let sat = if today.weekday() == Weekday::Sunday {
                                add(sat, -7)
                            } else {
                                sat
                            };
                            (sat, add(sat, 1))
                        }
                        "next" => (add(sat, 7), add(sun, 7)),
                        _ if tense == Tense::Past && today.weekday() != Weekday::Saturday => {
                            let sat = before(today, Weekday::Saturday);
                            (sat, add(sat, 1))
                        }
                        _ => (sat, sun),
                    });
                    used = 2;
                } else if next == "month" {
                    let first = today.first_of_month();
                    push(match word {
                        "last" => month_range(first.checked_sub(1.month()).unwrap_or(first)),
                        "next" => month_range(first.checked_add(1.month()).unwrap_or(first)),
                        _ if tense == Tense::Past => (first, today),
                        _ => month_range(first),
                    });
                    used = 2;
                } else if (last || this) && next == "few" && at(i + 2) == "days" {
                    push((add(today, -6), today));
                    used = 3;
                } else if word == "next" && next == "few" && at(i + 2) == "days" {
                    push((today, add(today, 3)));
                    used = 3;
                }
            }
            "these" if next == "last" && at(i + 2) == "few" && at(i + 3) == "days" => {
                push((add(today, -6), today));
                used = 4;
            }
            "at" if next == "the" && at(i + 2) == "weekend" => {
                let (sat, sun) = weekend(today);
                push(
                    if tense == Tense::Past && today.weekday() != Weekday::Saturday {
                        let sat = before(today, Weekday::Saturday);
                        (sat, add(sat, 1))
                    } else {
                        (sat, sun)
                    },
                );
                used = 3;
            }
            "recently" | "lately" => push((add(today, -6), today)),
            "coming" if next == "days" => {
                push((today, add(today, 3)));
                used = 2;
            }
            _ => {
                if let Some(wd) = weekday(word) {
                    push(one(if tense == Tense::Past {
                        before(today, wd)
                    } else {
                        coming(today, wd)
                    }));
                } else if let Some(n) = day_number(word) {
                    // "25 November", "25th November", "25th of November", "the 25th".
                    let of = usize::from(next == "of");
                    if let Some(m) = month(at(i + 1 + of)) {
                        if let Some(d) = nearest(today, m, n) {
                            push(one(d));
                        }
                        used = 2 + of;
                    } else if i > 0
                        && at(i - 1) == "the"
                        && word.len() > 2
                        && !word.chars().all(|c| c.is_ascii_digit())
                        && let Some(d) = nearest_day(today, n)
                    {
                        push(one(d));
                    }
                } else if let Some(m) = month(word)
                    && let Some(n) = day_number(next)
                {
                    // "November 25", "November 25th".
                    if let Some(d) = nearest(today, m, n) {
                        push(one(d));
                    }
                    used = 2;
                }
            }
        }
        i += used;
    }
    for (i, word) in w.iter().enumerate() {
        let mut found: &[ReadingKind] = &[];
        for (names, kinds) in KINDS {
            if names.len() == 1 && names[0] == word {
                found = kinds;
            } else if names.len() == 2 && names[0] == word && at(i + 1) == names[1] {
                found = kinds;
            }
        }
        if WORKOUT.contains(&word.as_str()) {
            found = &[ReadingKind::Workout];
        }
        for k in found {
            if !step.kinds.contains(k) {
                step.kinds.push(*k);
            }
        }
    }
    step
}
