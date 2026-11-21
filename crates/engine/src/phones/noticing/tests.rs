//! Noticing, through the engine: what a recap says went together, and when it says nothing.

use std::sync::Arc;

use jiff::civil::Date;
use nocturne_contract::{
    CheckInInput, Feeling, FeelingPart, FeelingPoint, HourRange, ItemKind, Kept, Noticed,
    NoticedMeasure, NoticedTopic, PeriodKind, PhoneDeleteScope, PhoneSource, Reading, ReadingBatch,
    ReadingChange, ReadingKind, ReadingValue, RemoteConnection, Source, TimeSpan, WrittenWhen,
};

use crate::{Engine, ManualClock};

struct Fx {
    dir: tempfile::TempDir,
    engine: Engine,
    clock: Arc<ManualClock>,
    phone: String,
    sends: std::cell::Cell<u32>,
}

/// A store on Monday 16 November 2026, the week of 9 to 15 November behind it, with one phone.
fn fx() -> Fx {
    fx_in("Europe/London")
}

fn fx_in(zone: &str) -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::at("2026-11-16T08:00:00Z", zone));
    let engine = Engine::open(dir.path(), clock.clone()).unwrap();
    let phone = engine
        .add_device(
            "Pixel",
            &[7; 32],
            "https://example.test",
            RemoteConnection::Simple,
            "example.test",
        )
        .unwrap()
        .id;
    Fx {
        dir,
        engine,
        clock,
        phone,
        sends: std::cell::Cell::new(0),
    }
}

fn day(n: u8) -> String {
    format!("2026-11-{n:02}")
}

fn date(n: u8) -> Date {
    day(n).parse().unwrap()
}

fn reading(phone_id: &str, n: u8, value: ReadingValue) -> ReadingChange {
    ReadingChange::Upsert {
        reading: Reading {
            phone_id: phone_id.into(),
            day: day(n),
            start: format!("{}T07:00:00Z", day(n)),
            end: None,
            source_device: Some("Watch".into()),
            value,
        },
    }
}

fn asleep(minutes: u32) -> ReadingValue {
    ReadingValue::Sleep {
        asleep: vec![TimeSpan {
            from: "2026-11-01T00:00:00Z".into(),
            to: format!("2026-11-01T{:02}:{:02}:00Z", minutes / 60, minutes % 60),
        }],
        in_bed_only: false,
    }
}

fn send(f: &Fx, changes: Vec<ReadingChange>) {
    f.sends.set(f.sends.get() + 1);
    f.engine
        .take_readings(
            &f.phone,
            ReadingBatch {
                sync_id: format!("s{}", f.sends.get()),
                seq: 0,
                last: true,
                changes,
            },
        )
        .unwrap();
}

fn sleep_on(f: &Fx, minutes: &[(u8, u32)]) {
    send(
        f,
        minutes
            .iter()
            .map(|(n, m)| reading(&format!("sl{n}"), *n, asleep(*m)))
            .collect(),
    );
}

fn steps_on(f: &Fx, steps: &[(u8, u32)]) {
    send(
        f,
        steps
            .iter()
            .map(|(n, s)| reading(&format!("st{n}"), *n, ReadingValue::Steps { total: *s }))
            .collect(),
    );
}

fn walk_on(f: &Fx, days: &[u8]) {
    send(
        f,
        days.iter()
            .map(|n| {
                reading(
                    &format!("wk{n}"),
                    *n,
                    ReadingValue::Workout {
                        kind: "Walk".into(),
                        length_s: 2400,
                        distance_m: None,
                    },
                )
            })
            .collect(),
    );
}

/// A note on the day naming the Hendersons.
fn write_about_hendersons(f: &Fx, days: &[u8]) {
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO entities(id, kind, name, key, role) \
         VALUES ('ent_hen', 'person', 'the Hendersons', 'the hendersons', NULL)",
        [],
    )
    .unwrap();
    for n in days {
        f.clock.set_str(&format!("{}T09:00:00Z", day(*n)));
        let item = match f
            .engine
            .keep_text(
                &format!("The Hendersons rang on the {n}th"),
                Source::QuickCapture,
            )
            .unwrap()
        {
            Kept::New { item } | Kept::Duplicate { item } => item.id,
        };
        conn.execute(
            "INSERT INTO mentions(id, item_id, entity_id, name, start, end, ord, state, new, \
             created_at) VALUES (?1, ?2, 'ent_hen', 'the Hendersons', 4, 18, 0, 'found', 0, \
             '2026-11-13T07:00:00.000Z')",
            rusqlite::params![format!("men_{item}"), item],
        )
        .unwrap();
    }
}

fn check_in(f: &Fx, n: u8, part: FeelingPart) -> String {
    f.clock.set_str(&format!("{}T18:00:00Z", day(n)));
    f.engine
        .keep_checkin(
            CheckInInput {
                feeling: Feeling {
                    part,
                    point: FeelingPoint {
                        pleasant: 0.5,
                        energy: 0.0,
                    },
                    words: vec!["calm".into()],
                },
                line: None,
                urge: None,
                replaces: None,
            },
            Source::Journal,
        )
        .unwrap()
        .id
}

fn week(f: &Fx) -> nocturne_contract::Recap {
    week_at(f, "2026-11-16T08:00:00Z")
}

fn week_at(f: &Fx, now: &str) -> nocturne_contract::Recap {
    f.clock.set_str(now);
    let p = f.engine.period_of(PeriodKind::Week, date(12)).unwrap();
    f.engine.compose_recap(&p, WrittenWhen::Overnight).unwrap()
}

fn noticed(f: &Fx) -> Vec<Noticed> {
    week(f).health.map(|h| h.noticed).unwrap_or_default()
}

/// Nights of 9 to 15 November: 360 minutes on the given days, 480 on the others.
fn nights_with(short: &[u8], short_minutes: u32) -> Vec<(u8, u32)> {
    (9..=15)
        .map(|n| {
            (
                n,
                if short.contains(&n) {
                    short_minutes
                } else {
                    480
                },
            )
        })
        .collect()
}

#[test]
fn shorter_nights_before_days_with_a_name_are_noticed_with_both_sides_cited() {
    let f = fx();
    write_about_hendersons(&f, &[10, 11, 12]);
    sleep_on(&f, &nights_with(&[10, 11, 12], 360));
    let lines = noticed(&f);
    assert_eq!(lines.len(), 1);
    let n = &lines[0];
    assert_eq!(n.measure, NoticedMeasure::Sleep);
    assert!(matches!(&n.topic, NoticedTopic::Name { with } if with.name == "the Hendersons"));
    assert_eq!((n.days, n.held, n.other_days), (3, 3, 4));
    assert_eq!((n.difference, n.more), (120, false));
    assert_eq!(n.body.len(), 3, "a night for each day counted");
    assert!(
        n.body
            .iter()
            .all(|r| r.kind == ItemKind::Reading && r.label == "Sleep")
    );
    assert_eq!(n.journal.len(), 3, "an entry for each day counted");
    assert!(n.journal.iter().all(|r| r.kind == ItemKind::Text));
}

#[test]
fn a_receipt_stands_on_the_day_of_the_reading() {
    let f = fx();
    write_about_hendersons(&f, &[10, 11, 12]);
    sleep_on(&f, &nights_with(&[10, 11, 12], 360));
    let n = &noticed(&f)[0];
    let days: Vec<&str> = n.body.iter().map(|r| &r.kept_at[..10]).collect();
    assert_eq!(days, ["2026-11-10", "2026-11-11", "2026-11-12"]);
}

#[test]
fn the_edges_of_what_is_shown() {
    // (days written about, minutes on those nights, nights on other days, shown)
    let cases: &[(&[u8], u32, &[u8], bool)] = &[
        (&[10, 11, 12], 360, &[9, 13, 14, 15], true),
        (&[10, 11, 12], 450, &[9, 13, 14, 15], true),
        (&[10, 11, 12], 451, &[9, 13, 14, 15], false),
        (&[10, 11], 360, &[9, 13, 14, 15], false),
        (&[10, 11, 12], 360, &[9, 13, 14], true),
        (&[10, 11, 12], 360, &[9, 13], false),
        (&[10, 11, 12, 13, 14], 360, &[9, 15], false),
    ];
    for (named, minutes, others, shown) in cases {
        let f = fx();
        write_about_hendersons(&f, named);
        let mut nights: Vec<(u8, u32)> = named.iter().map(|n| (*n, *minutes)).collect();
        nights.extend(others.iter().map(|n| (*n, 480)));
        sleep_on(&f, &nights);
        assert_eq!(
            !noticed(&f).is_empty(),
            *shown,
            "named {named:?} at {minutes}, others {others:?}"
        );
    }
}

#[test]
fn more_sleep_is_said_as_more_and_steps_have_their_own_edge() {
    // (steps on written-about days, steps on the others, shown, more)
    let cases = [
        (10_000u32, 8000u32, true, true),
        (9_999, 8000, false, true),
        (6_000, 8000, true, false),
        (6_001, 8000, false, false),
    ];
    for (with, without, shown, more) in cases {
        let f = fx();
        write_about_hendersons(&f, &[10, 11, 12]);
        let steps: Vec<(u8, u32)> = (9..=15)
            .map(|n| {
                (
                    n,
                    if (10..=12).contains(&n) {
                        with
                    } else {
                        without
                    },
                )
            })
            .collect();
        steps_on(&f, &steps);
        let lines = noticed(&f);
        assert_eq!(!lines.is_empty(), shown, "{with} against {without}");
        if shown {
            assert_eq!(lines[0].measure, NoticedMeasure::Steps);
            assert_eq!(lines[0].more, more);
            assert_eq!(lines[0].difference, with.abs_diff(without));
        }
    }
}

#[test]
fn a_pattern_must_hold_on_three_of_the_days() {
    let f = fx();
    write_about_hendersons(&f, &[10, 11, 12]);
    // On average 47 minutes less, but only two of the three nights were shorter.
    let mut nights = nights_with(&[], 0);
    for (n, m) in &mut nights {
        *m = match *n {
            10 | 11 => 300,
            12 => 700,
            _ => 480,
        };
    }
    sleep_on(&f, &nights);
    assert!(noticed(&f).is_empty());
}

#[test]
fn heart_rate_blood_pressure_and_weight_are_never_used() {
    let f = fx();
    write_about_hendersons(&f, &[10, 11, 12]);
    let mut changes = Vec::new();
    for n in 9..=15u8 {
        let high = (10..=12).contains(&n);
        changes.push(reading(
            &format!("hr{n}"),
            n,
            ReadingValue::HeartRate {
                hours: vec![HourRange {
                    start: format!("{}T08:00:00Z", day(n)),
                    low: 50.0,
                    high: 90.0,
                    middle: if high { 95.0 } else { 60.0 },
                }],
                count: 100,
            },
        ));
        changes.push(reading(
            &format!("rhr{n}"),
            n,
            ReadingValue::RestingHeartRate {
                bpm: if high { 80.0 } else { 55.0 },
            },
        ));
        changes.push(reading(
            &format!("bp{n}"),
            n,
            ReadingValue::BloodPressure {
                top: if high { 150.0 } else { 115.0 },
                bottom: if high { 95.0 } else { 75.0 },
            },
        ));
        changes.push(reading(
            &format!("wt{n}"),
            n,
            ReadingValue::Weight {
                value: if high { 90.0 } else { 70.0 },
                unit: "kg".into(),
            },
        ));
    }
    send(&f, changes);
    let recap = week(&f);
    let health = recap.health.expect("resting heart rate is shown");
    assert!(health.noticed.is_empty(), "only those four kinds came in");
    assert!(health.resting.is_some() && health.sleep.is_none() && health.steps.is_none());

    // With a night and a day's steps as well, the lines cite those and nothing else.
    sleep_on(&f, &nights_with(&[10, 11, 12], 360));
    for line in noticed(&f) {
        let labels: Vec<&str> = line.body.iter().map(|r| r.label.as_str()).collect();
        assert!(
            labels
                .iter()
                .all(|l| ["Sleep", "Steps", "Walk"].contains(l))
        );
    }
}

#[test]
fn workout_days_checked_in_as_settled_are_noticed() {
    let f = fx();
    walk_on(&f, &[9, 10, 11, 12]);
    sleep_on(&f, &nights_with(&[], 480));
    let settled: Vec<String> = [9, 10, 11]
        .iter()
        .map(|n| check_in(&f, *n, FeelingPart::Settled))
        .collect();
    check_in(&f, 12, FeelingPart::Low);
    for n in [13, 14, 15] {
        check_in(&f, n, FeelingPart::Low);
    }
    let lines = noticed(&f);
    let line = lines
        .iter()
        .find(|l| l.measure == NoticedMeasure::Workout)
        .expect("a line about workouts");
    assert_eq!(
        line.topic,
        NoticedTopic::Felt {
            part: FeelingPart::Settled
        }
    );
    assert_eq!((line.held, line.days, line.other_days), (3, 4, 3));
    assert_eq!(line.body.len(), 4, "a receipt for each workout");
    assert!(line.body.iter().all(|r| r.label == "Walk"));
    let cited: Vec<&str> = line.journal.iter().map(|r| r.item_id.as_str()).collect();
    assert_eq!(cited.len(), 3);
    assert!(settled.iter().all(|id| cited.contains(&id.as_str())));
}

#[test]
fn a_part_felt_as_often_without_workouts_is_not_noticed() {
    let f = fx();
    walk_on(&f, &[9, 10, 11]);
    sleep_on(&f, &nights_with(&[], 480));
    for n in 9..=15 {
        check_in(&f, n, FeelingPart::Settled);
    }
    assert!(
        noticed(&f)
            .iter()
            .all(|l| l.measure != NoticedMeasure::Workout)
    );
}

#[test]
fn no_sentence_while_the_switch_is_off_and_it_comes_back_with_it() {
    let f = fx();
    write_about_hendersons(&f, &[10, 11, 12]);
    sleep_on(&f, &nights_with(&[10, 11, 12], 360));
    f.engine.set_noticing(false).unwrap();
    let recap = week(&f);
    assert!(recap.health.as_ref().is_some_and(|h| h.noticed.is_empty()));
    assert!(recap.health.is_some(), "From Health is not the switch's");
    f.engine.set_noticing(true).unwrap();
    assert_eq!(noticed(&f).len(), 1);
}

#[test]
fn a_reading_removed_in_health_leaves_the_sentence_and_may_take_it_out() {
    let f = fx();
    write_about_hendersons(&f, &[10, 11, 12]);
    sleep_on(&f, &nights_with(&[10, 11, 12], 360));
    assert_eq!(noticed(&f).len(), 1);
    send(
        &f,
        vec![ReadingChange::Removed {
            phone_id: "sl11".into(),
            kind: ReadingKind::Sleep,
        }],
    );
    assert!(noticed(&f).is_empty(), "two nights are not enough");
    let health = week(&f).health.unwrap();
    assert_eq!(health.nights.len(), 6, "the removed night is not drawn");
}

#[test]
fn a_saved_recap_is_worked_out_again_when_opened_and_keeps_none_of_it() {
    let f = fx();
    write_about_hendersons(&f, &[10, 11, 12]);
    sleep_on(&f, &nights_with(&[10, 11, 12], 360));
    walk_on(&f, &[10, 11]);
    let recap = week(&f);
    assert_eq!(recap.health.as_ref().unwrap().workouts.len(), 2);
    f.engine.save_recap(&recap).unwrap();
    let body: String = rusqlite::Connection::open(f.dir.path().join("nocturne.db"))
        .unwrap()
        .query_row("SELECT body FROM reflections", [], |r| r.get(0))
        .unwrap();
    assert!(!body.contains("\"health\"") && !body.contains("\"noticed\""));
    let Some(nocturne_contract::ReflectionView::Recap { recap: opened }) =
        f.engine.saved_reflection(&recap.period).unwrap()
    else {
        panic!("a recap")
    };
    assert_eq!(opened.health, recap.health);
    // Deleted in Health after it was saved: gone when it is opened next.
    send(
        &f,
        vec![ReadingChange::Removed {
            phone_id: "wk10".into(),
            kind: ReadingKind::Workout,
        }],
    );
    let Some(nocturne_contract::ReflectionView::Recap { recap: opened }) =
        f.engine.saved_reflection(&recap.period).unwrap()
    else {
        panic!("a recap")
    };
    assert_eq!(opened.health.unwrap().workouts.len(), 1);
}

#[test]
fn a_recap_counts_workouts_by_receipt_and_leaves_pressure_and_weight_out() {
    let f = fx();
    walk_on(&f, &[10, 11, 12, 13]);
    send(
        &f,
        vec![
            reading(
                "sw1",
                14,
                ReadingValue::Workout {
                    kind: "Swim".into(),
                    length_s: 1800,
                    distance_m: None,
                },
            ),
            reading(
                "bp1",
                14,
                ReadingValue::BloodPressure {
                    top: 120.0,
                    bottom: 80.0,
                },
            ),
            reading(
                "wt1",
                14,
                ReadingValue::Weight {
                    value: 70.0,
                    unit: "kg".into(),
                },
            ),
        ],
    );
    let health = week(&f).health.unwrap();
    let kinds: Vec<&str> = health.workouts.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(kinds, ["Walk", "Walk", "Walk", "Walk", "Swim"]);
    assert!(health.workouts.iter().all(|r| r.kind == ItemKind::Reading));
    assert!(health.sleep.is_none() && health.steps.is_none() && health.resting.is_none());
}

#[test]
fn no_health_section_when_nothing_came_in() {
    let f = fx();
    write_about_hendersons(&f, &[10, 11, 12]);
    assert!(week(&f).health.is_none());
}

#[test]
fn the_delete_preview_counts_the_sentences_it_would_change() {
    let f = fx();
    write_about_hendersons(&f, &[10, 11, 12]);
    sleep_on(&f, &nights_with(&[10, 11, 12], 360));
    let recap = week(&f);
    let lines = recap.health.as_ref().unwrap().noticed.clone();
    assert_eq!(lines.len(), 1);
    f.engine.save_recap(&recap).unwrap();
    let scope = |item: &str| PhoneDeleteScope {
        device_id: f.phone.clone(),
        source: PhoneSource::Health,
        kinds: None,
        before: None,
        item_id: Some(item.to_string()),
    };
    // A night the sentence cites.
    let cited = lines[0].body[0].item_id.clone();
    let preview = f.engine.preview_phone_delete(scope(&cited)).unwrap();
    assert_eq!(preview.noticing, 1);
    // A night on another day leaves what it says as it is, with three days still compared.
    let other = f
        .engine
        .readings(ReadingKind::Sleep, date(15), date(15))
        .unwrap()[0]
        .item_id
        .clone();
    assert_eq!(
        f.engine
            .preview_phone_delete(scope(&other))
            .unwrap()
            .noticing,
        0
    );
    // Applying the first delete takes the sentence out.
    f.engine.delete_phone_data(&preview.token, false).unwrap();
    assert!(noticed(&f).is_empty());
}

#[test]
fn a_nap_does_not_make_a_second_night_and_the_longest_is_the_receipt() {
    let f = fx();
    write_about_hendersons(&f, &[10, 11, 12]);
    let mut changes: Vec<ReadingChange> = nights_with(&[10, 11, 12], 360)
        .iter()
        .map(|(n, m)| reading(&format!("sl{n}"), *n, asleep(*m)))
        .collect();
    // A nap that starts later in the day than the night.
    changes.push(ReadingChange::Upsert {
        reading: Reading {
            phone_id: "nap10".into(),
            day: day(10),
            start: format!("{}T14:00:00Z", day(10)),
            end: None,
            source_device: Some("Watch".into()),
            value: asleep(40),
        },
    });
    send(&f, changes);
    let recap = week(&f);
    let health = recap.health.unwrap();
    assert_eq!(health.nights.len(), 7, "one reading for each night");
    let mut days: Vec<&str> = health.nights.iter().map(|n| n.day.as_str()).collect();
    days.dedup();
    assert_eq!(days.len(), 7);
    let night = health.nights.iter().find(|n| n.day == day(10)).unwrap();
    assert!(
        matches!(&night.value, ReadingValue::Sleep { asleep, .. } if asleep[0].to.contains("T06:00"))
    );
    let line = &health.noticed[0];
    assert!(line.body.iter().any(|r| r.item_id == night.item_id));
    assert_eq!(line.body.len(), 3);
}

#[test]
fn the_most_steps_of_two_phones_is_the_day_and_its_receipt() {
    let f = fx();
    write_about_hendersons(&f, &[10, 11, 12]);
    let mut changes: Vec<ReadingChange> = (9..=15u8)
        .map(|n| {
            let low = (10..=12).contains(&n);
            reading(
                &format!("a{n}"),
                n,
                ReadingValue::Steps {
                    total: if low { 6000 } else { 8000 },
                },
            )
        })
        .collect();
    // Another phone counts a few of the same steps, later in the day.
    for n in 9..=15u8 {
        changes.push(ReadingChange::Upsert {
            reading: Reading {
                phone_id: format!("b{n}"),
                day: day(n),
                start: format!("{}T20:00:00Z", day(n)),
                end: None,
                source_device: Some("Phone".into()),
                value: ReadingValue::Steps { total: 100 },
            },
        });
    }
    send(&f, changes);
    let line = noticed(&f).remove(0);
    assert_eq!(
        (line.measure, line.difference),
        (NoticedMeasure::Steps, 2000)
    );
    assert!(line.body.iter().all(|r| r.item_id.len() > 1));
    let big: Vec<String> = f
        .engine
        .readings(ReadingKind::Steps, date(10), date(12))
        .unwrap()
        .into_iter()
        .filter(|r| matches!(r.value, ReadingValue::Steps { total } if total == 6000))
        .map(|r| r.item_id)
        .collect();
    assert!(line.body.iter().all(|r| big.contains(&r.item_id)));
}

#[test]
fn a_period_still_going_leaves_out_today() {
    let f = fx();
    write_about_hendersons(&f, &[10, 11, 12]);
    sleep_on(&f, &nights_with(&[10, 11, 12], 360));
    // It is Thursday the 12th: that day is not over, and the nights before it are too few.
    let now = week_at(&f, "2026-11-12T12:00:00Z").health.unwrap();
    assert!(now.noticed.is_empty());
    // On the 13th the 12th counts, but only the 9th is set against it: two days are missing.
    assert!(
        week_at(&f, "2026-11-13T12:00:00Z")
            .health
            .unwrap()
            .noticed
            .is_empty()
    );
    // On the 15th the 14th is the last day counted: the 9th, 13th and 14th are set against it.
    let line = week_at(&f, "2026-11-15T12:00:00Z").health.unwrap().noticed;
    assert_eq!(line.len(), 1);
    assert_eq!(line[0].other_days, 3);
    // Once the week is over every night counts.
    let line = week(&f).health.unwrap().noticed;
    assert_eq!(line[0].other_days, 4);
}

#[test]
fn a_receipt_names_its_day_in_the_computers_zone() {
    let f = fx_in("Pacific/Auckland");
    write_about_hendersons(&f, &[10, 11, 12]);
    sleep_on(&f, &nights_with(&[10, 11, 12], 360));
    let line = noticed(&f).remove(0);
    let days: Vec<&str> = line.body.iter().map(|r| &r.kept_at[..10]).collect();
    assert_eq!(days, ["2026-11-10", "2026-11-11", "2026-11-12"]);
    assert!(line.body.iter().all(|r| r.kept_at.ends_with("+13:00")));
}

#[test]
fn a_health_problem_never_costs_the_recap() {
    let f = fx();
    write_about_hendersons(&f, &[10, 11, 12]);
    sleep_on(&f, &nights_with(&[10, 11, 12], 360));
    // Health that cannot be read leaves the recap without it, not without the recap.
    f.engine
        .write(|tx| {
            tx.execute_batch("DROP TABLE phone_records")?;
            Ok(())
        })
        .unwrap();
    let recap = week(&f);
    assert!(recap.days > 0);
    assert!(recap.health.is_none());
}

mod pure {
    use std::collections::BTreeMap;

    use jiff::civil::Date;
    use nocturne_contract::{FeelingPart, ItemKind, NoticedMeasure, NoticedTopic, Receipt, Source};

    use super::super::{Body, Counted, Topic, notice};

    fn receipt(id: &str) -> Receipt {
        Receipt {
            item_id: id.into(),
            kind: ItemKind::Reading,
            source: Source::Phone,
            label: "Sleep".into(),
            kept_at: "2026-11-10T12:00:00Z".into(),
            duration_ms: None,
            span: None,
            unconfirmed: false,
            from_device: None,
        }
    }

    fn d(n: u8) -> Date {
        format!("2026-11-{n:02}").parse().unwrap()
    }

    fn counted(values: &[(u8, i64)]) -> BTreeMap<Date, Counted> {
        values
            .iter()
            .map(|(n, v)| {
                (
                    d(*n),
                    Counted {
                        value: *v,
                        receipt: receipt(&format!("r{n}")),
                    },
                )
            })
            .collect()
    }

    fn topic(days: &[u8]) -> Topic {
        Topic {
            kind: NoticedTopic::Good,
            days: days
                .iter()
                .map(|n| (d(*n), vec![receipt(&format!("e{n}"))]))
                .collect(),
        }
    }

    #[test]
    fn the_thresholds_are_exact_for_sleep_and_steps() {
        // (with values, without values, shown): sleep 30 minutes, steps 2,000.
        let sleep = |with: i64, shown: bool| {
            let body = Body {
                sleep: counted(&[
                    (1, with),
                    (2, with),
                    (3, with),
                    (4, 500),
                    (5, 500),
                    (6, 500),
                ]),
                ..Body::default()
            };
            let lines = notice(&[topic(&[1, 2, 3])], &body);
            assert_eq!(lines.len() == 1, shown, "sleep {with}");
        };
        sleep(470, true);
        sleep(471, false);
        sleep(530, true);
        sleep(529, false);

        // A fractional average: 3 days 1,999 and 1,999 and 2,000 against 4,000 each.
        let body = Body {
            steps: counted(&[
                (1, 1999),
                (2, 2000),
                (3, 2000),
                (4, 4000),
                (5, 4000),
                (6, 4000),
            ]),
            ..Body::default()
        };
        let lines = notice(&[topic(&[1, 2, 3])], &body);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].measure, NoticedMeasure::Steps);
        assert_eq!(lines[0].difference, 2000);
    }

    #[test]
    fn at_most_three_sentences_the_widest_first() {
        let body = Body {
            sleep: counted(&[
                (1, 300),
                (2, 300),
                (3, 300),
                (4, 300),
                (5, 480),
                (6, 480),
                (7, 480),
                (8, 480),
            ]),
            ..Body::default()
        };
        let topics = [topic(&[1, 2, 3]), topic(&[1, 2, 3, 4])];
        let lines = notice(&topics, &body);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].days, 4);
        assert!(notice(&[], &body).is_empty());
    }

    #[test]
    fn a_felt_part_with_no_workouts_in_the_week_says_nothing() {
        let body = Body {
            sleep: counted(&[(1, 480), (2, 480), (3, 480), (4, 480)]),
            ..Body::default()
        };
        let t = Topic {
            kind: NoticedTopic::Felt {
                part: FeelingPart::Settled,
            },
            days: [(d(1), vec![receipt("c1")])].into_iter().collect(),
        };
        assert!(notice(&[t], &body).is_empty());
    }
}

/// The rules over the shared case table, which the mock engine's own port is tested against.
mod table {
    use std::collections::BTreeMap;

    use jiff::civil::Date;
    use nocturne_contract::{
        EntityKind, FeelingPart, ItemKind, NameRef, NoticedMeasure, NoticedTopic, Receipt, Source,
    };
    use serde::Deserialize;

    use super::super::{Body, Counted, Topic, notice};

    const CASES: &str = include_str!("cases.json");

    #[derive(Deserialize)]
    struct File {
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        topics: Vec<TopicIn>,
        sleep: BTreeMap<String, i64>,
        steps: BTreeMap<String, i64>,
        workouts: Vec<u8>,
        expect: Vec<Expect>,
    }

    #[derive(Deserialize)]
    struct TopicIn {
        #[serde(rename = "type")]
        kind: String,
        name: Option<String>,
        part: Option<String>,
        days: Vec<u8>,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct Expect {
        measure: String,
        topic: String,
        days: u32,
        held: u32,
        other_days: u32,
        difference: u32,
        more: bool,
        body: usize,
        journal: usize,
    }

    fn receipt(id: String) -> Receipt {
        Receipt {
            item_id: id,
            kind: ItemKind::Reading,
            source: Source::Phone,
            label: "x".into(),
            kept_at: "2026-11-10T12:00:00+00:00".into(),
            duration_ms: None,
            span: None,
            unconfirmed: false,
            from_device: None,
        }
    }

    fn day(n: u8) -> Date {
        format!("2026-11-{n:02}").parse().unwrap()
    }

    fn counted(prefix: &str, values: &BTreeMap<String, i64>) -> BTreeMap<Date, Counted> {
        values
            .iter()
            .map(|(d, v)| {
                let n: u8 = d.parse().unwrap();
                (
                    day(n),
                    Counted {
                        value: *v,
                        receipt: receipt(format!("{prefix}-{n}")),
                    },
                )
            })
            .collect()
    }

    fn part(p: &str) -> FeelingPart {
        match p {
            "woundUp" => FeelingPart::WoundUp,
            "litUp" => FeelingPart::LitUp,
            "low" => FeelingPart::Low,
            _ => FeelingPart::Settled,
        }
    }

    fn summary(n: &nocturne_contract::Noticed) -> Expect {
        Expect {
            measure: match n.measure {
                NoticedMeasure::Sleep => "sleep",
                NoticedMeasure::Steps => "steps",
                NoticedMeasure::Workout => "workout",
            }
            .into(),
            topic: match &n.topic {
                NoticedTopic::Name { with } => format!("name:{}", with.name),
                NoticedTopic::Felt { part } => format!(
                    "felt:{}",
                    match part {
                        FeelingPart::WoundUp => "woundUp",
                        FeelingPart::LitUp => "litUp",
                        FeelingPart::Low => "low",
                        FeelingPart::Settled => "settled",
                    }
                ),
                NoticedTopic::Good => "good".into(),
            },
            days: n.days,
            held: n.held,
            other_days: n.other_days,
            difference: n.difference,
            more: n.more,
            body: n.body.len(),
            journal: n.journal.len(),
        }
    }

    #[test]
    fn every_case_says_what_the_table_says() {
        let file: File = serde_json::from_str(CASES).unwrap();
        for case in file.cases {
            let mut workouts: BTreeMap<Date, Vec<Receipt>> = BTreeMap::new();
            for n in &case.workouts {
                workouts
                    .entry(day(*n))
                    .or_default()
                    .push(receipt(format!("workout-{n}")));
            }
            let body = Body {
                sleep: counted("sleep", &case.sleep),
                steps: counted("steps", &case.steps),
                workouts,
            };
            let topics: Vec<Topic> = case
                .topics
                .iter()
                .map(|t| {
                    let label = t.name.clone().or(t.part.clone()).unwrap_or_default();
                    Topic {
                        kind: match t.kind.as_str() {
                            "name" => NoticedTopic::Name {
                                with: NameRef {
                                    entity_id: label.clone(),
                                    name: label.clone(),
                                    kind: EntityKind::Person,
                                    role: None,
                                    new: false,
                                    item_ids: Vec::new(),
                                },
                            },
                            "felt" => NoticedTopic::Felt {
                                part: part(t.part.as_deref().unwrap_or("settled")),
                            },
                            _ => NoticedTopic::Good,
                        },
                        days: t
                            .days
                            .iter()
                            .map(|n| (day(*n), vec![receipt(format!("{label}-entry-{n}"))]))
                            .collect(),
                    }
                })
                .collect();
            let got: Vec<Expect> = notice(&topics, &body).iter().map(summary).collect();
            assert_eq!(got, case.expect, "{}", case.name);
        }
    }
}
