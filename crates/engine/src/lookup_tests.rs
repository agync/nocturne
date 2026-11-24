//! The look-ups and the date step's lines, against a store with a paired phone's calendar and
//! Health, and a journal.

use std::sync::Arc;

use jiff::civil::{Date, Weekday};
use nocturne_contract::{
    CalendarBatch, CalendarChange, CalendarEntry, CheckInInput, Feeling, FeelingPart, FeelingPoint,
    PhoneCalendar, Reading, ReadingBatch, ReadingChange, ReadingKind, ReadingValue,
    RemoteConnection, Source, TimeSpan,
};

use super::{LookScope, is_kept_line};
use crate::{Engine, ManualClock, date_step};

struct Fx {
    dir: tempfile::TempDir,
    engine: Engine,
    clock: Arc<ManualClock>,
    phone: String,
}

/// Tuesday 24 November 2026, 21:40 in London, with one paired phone named "Pixel 9".
fn fx() -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::at("2026-11-24T21:40:00Z", "Europe/London"));
    let engine = Engine::open(dir.path(), clock.clone()).unwrap();
    let phone = engine
        .add_device(
            "Pixel 9",
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
    }
}

fn d(s: &str) -> Date {
    s.parse().unwrap()
}

fn entry(id: &str, title: &str, start: &str, end: &str, place: Option<&str>) -> CalendarChange {
    CalendarChange::Upsert {
        entry: CalendarEntry {
            phone_id: id.into(),
            calendar_id: "c1".into(),
            title: title.into(),
            start: start.into(),
            end: end.into(),
            time_zone: Some("Europe/London".into()),
            all_day: false,
            place: place.map(str::to_string),
            people: Vec::new(),
            repeats: false,
            notes: None,
            repeat_rule: None,
        },
    }
}

fn calendar(f: &Fx, sync: &str, changes: Vec<CalendarChange>) {
    f.engine
        .take_calendar(
            &f.phone,
            CalendarBatch {
                sync_id: sync.into(),
                seq: 0,
                last: true,
                window_from: "2026-11-01".into(),
                window_to: "2026-12-31".into(),
                calendars: vec![PhoneCalendar {
                    id: "c1".into(),
                    name: "Home".into(),
                    colour: None,
                    account: None,
                }],
                changes,
            },
        )
        .unwrap();
}

fn health(f: &Fx, sync: &str, changes: Vec<ReadingChange>) {
    f.engine
        .take_readings(
            &f.phone,
            ReadingBatch {
                sync_id: sync.into(),
                seq: 0,
                last: true,
                changes,
            },
        )
        .unwrap();
}

fn night(id: &str, day: &str, from: &str, to: &str) -> ReadingChange {
    ReadingChange::Upsert {
        reading: Reading {
            phone_id: id.into(),
            day: day.into(),
            start: from.into(),
            end: Some(to.into()),
            source_device: Some("Watch".into()),
            value: ReadingValue::Sleep {
                asleep: vec![TimeSpan {
                    from: from.into(),
                    to: to.into(),
                }],
                in_bed_only: false,
            },
        },
    }
}

fn sentences(hits: &[crate::Hit]) -> Vec<String> {
    hits.iter().map(|h| h.memory.sentence.clone()).collect()
}

fn week(f: &Fx) {
    calendar(
        f,
        "s1",
        vec![
            entry(
                "swim",
                "Leo's swimming",
                "2026-11-25T17:00:00Z",
                "2026-11-25T18:00:00Z",
                Some("the leisure centre"),
            ),
            entry(
                "dentist",
                "Dentist",
                "2026-11-25T10:30:00Z",
                "2026-11-25T11:00:00Z",
                Some("Smile Dental"),
            ),
            entry(
                "football",
                "Leo's football",
                "2026-11-28T09:30:00Z",
                "2026-11-28T10:30:00Z",
                None,
            ),
            entry(
                "gone",
                "Haircut",
                "2026-11-25T12:00:00Z",
                "2026-11-25T12:30:00Z",
                None,
            ),
        ],
    );
    calendar(
        f,
        "s2",
        vec![CalendarChange::Removed {
            phone_id: "gone".into(),
        }],
    );
}

#[test]
fn the_calendar_gives_a_day_s_appointments_in_order_of_time() {
    let f = fx();
    week(&f);
    let found = f
        .engine
        .look_up_calendar(d("2026-11-25"), d("2026-11-25"), LookScope::ALL, 20)
        .unwrap();
    assert_eq!(
        sentences(&found.hits),
        vec![
            "Dentist is on Wednesday 25 November at 10:30, at Smile Dental.",
            "Leo's swimming is on Wednesday 25 November at 17:00, at the leisure centre.",
        ],
        "in order of time, and an entry its calendar dropped is not given"
    );
    assert_eq!(found.more, 0);
    assert!(found.hits.iter().all(|h| !h.memory.receipts.is_empty()));
    let capped = f
        .engine
        .look_up_calendar(d("2026-11-25"), d("2026-11-28"), LookScope::ALL, 2)
        .unwrap();
    assert_eq!((capped.hits.len(), capped.more), (2, 1));
}

#[test]
fn a_memory_let_go_or_replaced_is_never_given() {
    let f = fx();
    week(&f);
    let found = f
        .engine
        .look_up_calendar(d("2026-11-28"), d("2026-11-28"), LookScope::ALL, 20)
        .unwrap();
    let id = found.hits[0].memory.id.clone();
    f.engine
        .write(|tx| {
            tx.execute(
                "UPDATE memories SET status = 'letGo', let_go_at = '2026-11-24T08:00:00Z' \
                 WHERE id = ?1",
                [&id],
            )?;
            Ok(())
        })
        .unwrap();
    let found = f
        .engine
        .look_up_calendar(d("2026-11-28"), d("2026-11-28"), LookScope::ALL, 20)
        .unwrap();
    assert!(found.hits.is_empty());
    f.engine
        .write(|tx| {
            tx.execute(
                "UPDATE memories SET status = 'active', let_go_at = NULL, forgetting = 1 \
                 WHERE id = ?1",
                [&id],
            )?;
            Ok(())
        })
        .unwrap();
    let found = f
        .engine
        .look_up_calendar(d("2026-11-28"), d("2026-11-28"), LookScope::ALL, 20)
        .unwrap();
    assert!(found.hits.is_empty(), "nor one being forgotten");
}

fn checkin(f: &Fx, at: &str, words: &[&str], part: FeelingPart, line: Option<&str>) {
    f.clock.set_str(at);
    f.engine
        .keep_checkin(
            CheckInInput {
                feeling: Feeling {
                    part,
                    point: FeelingPoint {
                        pleasant: -0.5,
                        energy: -0.5,
                    },
                    words: words.iter().map(|w| w.to_string()).collect(),
                },
                line: line.map(str::to_string),
                urge: None,
                replaces: None,
            },
            Source::QuickCapture,
        )
        .unwrap();
}

#[test]
fn the_journal_gives_check_ins_in_the_person_s_words_and_unread_writing() {
    let f = fx();
    checkin(
        &f,
        "2026-11-23T21:10:00Z",
        &["drained"],
        FeelingPart::Low,
        Some("Leo up twice again."),
    );
    checkin(&f, "2026-11-23T08:00:00Z", &[], FeelingPart::Settled, None);
    f.clock.set_str("2026-11-24T21:40:00Z");
    f.engine
        .keep_good_thing(
            d("2026-11-23"),
            "Lunch with Priya",
            None,
            Source::QuickCapture,
        )
        .unwrap();
    f.engine
        .save_entry_text(d("2026-11-23"), None, 0, "Leo had a nightmare.")
        .unwrap();
    let found = f
        .engine
        .look_up_journal(d("2026-11-23"), d("2026-11-23"), LookScope::ALL, 20)
        .unwrap();
    let said = sentences(&found.hits);
    assert!(
        said.contains(
            &"Check-in, Monday 23 November at 21:10: drained (Low). \"Leo up twice again.\""
                .to_string()
        ),
        "{said:?}"
    );
    assert!(
        said.contains(&"Check-in, Monday 23 November at 8:00: Settled.".to_string()),
        "{said:?}"
    );
    assert!(
        said.contains(&"Good thing, Monday 23 November: Lunch with Priya".to_string()),
        "{said:?}"
    );
    assert!(
        said.contains(&"Journal, Monday 23 November: Leo had a nightmare.".to_string()),
        "{said:?}"
    );
    assert!(found.hits.iter().all(|h| is_kept_line(&h.memory.id)));
    assert!(
        found
            .hits
            .iter()
            .all(|h| h.memory.receipts.len() == 1 && !h.memory.receipts[0].item_id.is_empty())
    );
    // A companion phone gets no line of an original, only the check-ins.
    let companion = f
        .engine
        .look_up_journal(d("2026-11-23"), d("2026-11-23"), LookScope::COMPANION, 20)
        .unwrap();
    assert_eq!(companion.hits.len(), 2, "{:?}", sentences(&companion.hits));
    assert!(
        sentences(&companion.hits)
            .iter()
            .all(|s| s.starts_with("Check-in"))
    );
    // Nothing from another day.
    let other = f
        .engine
        .look_up_journal(d("2026-11-20"), d("2026-11-22"), LookScope::ALL, 20)
        .unwrap();
    assert!(other.hits.is_empty());
}

#[test]
fn health_gives_one_line_per_night_in_the_health_wording() {
    let f = fx();
    health(
        &f,
        "h1",
        vec![
            night(
                "n1",
                "2026-11-23",
                "2026-11-22T23:50:00Z",
                "2026-11-23T06:30:00Z",
            ),
            night(
                "n2",
                "2026-11-24",
                "2026-11-23T23:00:00Z",
                "2026-11-24T06:55:00Z",
            ),
            night(
                "n3",
                "2026-11-22",
                "2026-11-21T23:00:00Z",
                "2026-11-22T07:00:00Z",
            ),
        ],
    );
    health(
        &f,
        "h2",
        vec![ReadingChange::Removed {
            phone_id: "n3".into(),
            kind: ReadingKind::Sleep,
        }],
    );
    let found = f
        .engine
        .look_up_health(
            ReadingKind::Sleep,
            d("2026-11-22"),
            d("2026-11-24"),
            LookScope::ALL,
            31,
        )
        .unwrap();
    assert_eq!(
        sentences(&found.hits),
        vec![
            "Night to Monday 23 November: 6 h 40 min asleep (Health on Pixel 9).",
            "Night to Tuesday 24 November: 7 h 55 min asleep (Health on Pixel 9).",
        ],
        "a reading removed in Health is not given"
    );
    assert_eq!(f.engine.reading_kinds().unwrap(), vec![ReadingKind::Sleep]);
}

#[test]
fn the_date_step_fetches_the_days_a_question_names() {
    let f = fx();
    week(&f);
    health(
        &f,
        "h1",
        vec![night(
            "n1",
            "2026-11-23",
            "2026-11-22T23:50:00Z",
            "2026-11-23T06:30:00Z",
        )],
    );
    let today = d("2026-11-24");
    let lines = |q: &str| {
        let step = date_step(q, today, Weekday::Monday);
        sentences(
            &f.engine
                .date_step_lines(&step, today, LookScope::ALL, 12)
                .unwrap(),
        )
    };
    assert_eq!(
        lines("What's on tomorrow?"),
        vec![
            "Dentist is on Wednesday 25 November at 10:30, at Smile Dental.",
            "Leo's swimming is on Wednesday 25 November at 17:00, at the leisure centre.",
        ]
    );
    assert_eq!(
        lines("Anything this weekend?"),
        vec!["Leo's football is on Saturday 28 November at 9:30."]
    );
    assert_eq!(
        lines("How did I sleep this week?"),
        vec!["Night to Monday 23 November: 6 h 40 min asleep (Health on Pixel 9)."]
    );
    assert!(lines("When is the dentist?").is_empty());
}

#[test]
fn today_is_the_journal_s_day_in_the_device_s_zone() {
    let f = fx();
    let t = f.engine.ask_today().unwrap();
    assert_eq!(
        (t.day, t.hour, t.minute, t.zone.as_str(), t.week_start),
        (d("2026-11-24"), 21, 40, "Europe/London", Weekday::Monday)
    );
    // After midnight, before the day turns: still the evening before.
    f.clock.set_str("2026-11-25T01:30:00Z");
    let t = f.engine.ask_today().unwrap();
    assert_eq!((t.day, t.hour, t.minute), (d("2026-11-24"), 1, 30));
    // In a zone ahead of London it is already Wednesday morning there.
    f.clock
        .set_tz(jiff::tz::TimeZone::get("Pacific/Auckland").unwrap());
    f.clock.set_str("2026-11-24T21:40:00Z");
    let t = f.engine.ask_today().unwrap();
    assert_eq!(
        (t.day, t.hour, t.zone.as_str()),
        (d("2026-11-25"), 10, "Pacific/Auckland")
    );
    // And behind it, still Tuesday afternoon.
    f.clock
        .set_tz(jiff::tz::TimeZone::get("America/New_York").unwrap());
    let t = f.engine.ask_today().unwrap();
    assert_eq!((t.day, t.hour), (d("2026-11-24"), 16));
}

/// Every look-up only reads: the store is byte for byte the same after each of them.
#[test]
fn look_ups_only_read() {
    let f = fx();
    week(&f);
    checkin(
        &f,
        "2026-11-23T21:10:00Z",
        &["drained"],
        FeelingPart::Low,
        None,
    );
    f.clock.set_str("2026-11-24T21:40:00Z");
    f.engine.checkpoint_truncate().unwrap();
    let bytes = |name: &str| std::fs::read(f.dir.path().join(name)).unwrap_or_default();
    let before = (bytes("nocturne.db"), bytes("nocturne.db-wal"));
    let (a, b) = (d("2026-11-20"), d("2026-11-30"));
    f.engine.look_up_calendar(a, b, LookScope::ALL, 20).unwrap();
    f.engine.look_up_journal(a, b, LookScope::ALL, 20).unwrap();
    f.engine
        .look_up_health(ReadingKind::Sleep, a, b, LookScope::ALL, 31)
        .unwrap();
    f.engine
        .look_up_words("dentist swimming", LookScope::ALL, 12)
        .unwrap();
    f.engine.reading_kinds().unwrap();
    f.engine.ask_today().unwrap();
    let step = date_step("What did I do last week?", d("2026-11-24"), Weekday::Monday);
    f.engine
        .date_step_lines(&step, d("2026-11-24"), LookScope::ALL, 12)
        .unwrap();
    assert!(before == (bytes("nocturne.db"), bytes("nocturne.db-wal")));
}

/// The gate before a kept line goes online, or to the phone's own mind: only a line whose item
/// a look-up would give now. Each way of being gone fails closed.
#[test]
fn a_kept_line_goes_only_while_its_item_is_kept() {
    let f = fx();
    checkin(
        &f,
        "2026-11-23T21:10:00Z",
        &["drained"],
        FeelingPart::Low,
        None,
    );
    f.clock.set_str("2026-11-24T21:40:00Z");
    let found = f
        .engine
        .look_up_journal(d("2026-11-23"), d("2026-11-23"), LookScope::ALL, 20)
        .unwrap();
    let id = found.hits[0].memory.id.clone();
    let item = super::kept_line_item(&id).unwrap().to_string();
    let goes = |f: &Fx| {
        let verdict = f.engine.online_verdicts(std::slice::from_ref(&id)).unwrap()[0];
        let may = f.engine.mind_may_read(std::slice::from_ref(&id)).unwrap()[0];
        assert_eq!(verdict.is_none(), may, "both gates agree");
        may
    };
    assert!(goes(&f), "a kept line goes");
    let sql = |f: &Fx, sql: &str| {
        let item = item.clone();
        let sql = sql.to_string();
        f.engine
            .write(move |tx| {
                tx.execute(&sql, [&item])?;
                Ok(())
            })
            .unwrap();
    };
    for (break_it, mend) in [
        (
            "UPDATE items SET forget_epoch = 1 WHERE id = ?1",
            "UPDATE items SET forget_epoch = 0 WHERE id = ?1",
        ),
        (
            "INSERT INTO forget_jobs(ids, items, include_originals) \
             VALUES ('[]', '[\"' || ?1 || '\"]', 1)",
            "DELETE FROM forget_jobs WHERE instr(items, ?1) > 0",
        ),
        (
            "INSERT INTO forgotten_claims(item_id, claim) VALUES (?1, 'x')",
            "DELETE FROM forgotten_claims WHERE item_id = ?1",
        ),
        (
            "INSERT INTO items(id, kind, source, trust, sha256, title, kept_at, status, metadata) \
             VALUES ('newer', 'checkin', 'quickCapture', 'trusted', 'h-newer', 'x', \
             '2026-11-24T10:00:00Z', 'understood', json_object('replaces', ?1))",
            "DELETE FROM items WHERE id = 'newer' AND ?1 IS NOT NULL",
        ),
        (
            "INSERT INTO phone_records(device_id, source, phone_id, item_id, version, removed_at, \
             came_in_at) VALUES ('d', 'health', 'p', ?1, 1, '2026-11-24T10:00:00Z', \
             '2026-11-24T09:00:00Z')",
            "DELETE FROM phone_records WHERE item_id = ?1",
        ),
    ] {
        sql(&f, break_it);
        assert!(!goes(&f), "{break_it}");
        sql(&f, mend);
        assert!(goes(&f), "{mend}");
    }
    // An earlier version it replaced being forgotten keeps the newer one here too.
    let old = "older-version";
    let item2 = item.clone();
    f.engine
        .write(move |tx| {
            tx.execute(
                "INSERT INTO items(id, kind, source, trust, sha256, title, kept_at, status, \
                 forget_epoch, metadata) VALUES (?1, 'checkin', 'quickCapture', 'trusted', \
                 'h-old', 'x', '2026-11-23T09:00:00Z', 'understood', 1, '{}')",
                [old],
            )?;
            tx.execute(
                "UPDATE items SET metadata = json_set(metadata, '$.replaces', ?2) WHERE id = ?1",
                [&item2, old],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(!goes(&f), "a forgotten earlier version");
    // An id that is no line of anything kept never goes.
    let gone = format!("{}missing", super::KEPT_LINE);
    assert!(!f.engine.mind_may_read(&[gone.clone()]).unwrap()[0]);
    assert!(f.engine.online_verdicts(&[gone]).unwrap()[0].is_some());
}

/// Ask with no mind on a phone alone still runs the date step: tomorrow's appointments are found
/// though no word of the question is in them.
#[test]
fn finding_with_no_mind_lists_the_days_a_question_names() {
    let f = fx();
    week(&f);
    let found = f.engine.find_with_days("What's on tomorrow?", 30).unwrap();
    let words: Vec<String> = found
        .iter()
        .map(|i| i.excerpt.iter().map(|p| p.text.as_str()).collect())
        .collect();
    assert_eq!(
        words,
        vec![
            "Dentist is on Wednesday 25 November at 10:30, at Smile Dental.",
            "Leo's swimming is on Wednesday 25 November at 17:00, at the leisure centre.",
        ]
    );
    assert!(
        found
            .iter()
            .all(|i| i.receipt.calendar.as_deref() == Some("Home"))
    );
    assert!(f.engine.find_with_days("xylophone", 30).unwrap().is_empty());
}
