//! What phones bring, through the engine: sends, versions, removals, the page, the journal,
//! deleting and exporting.

use std::sync::Arc;

use jiff::civil::Date;
use nocturne_contract::{
    CalendarBatch, CalendarChange, CalendarEntry, CalendarKind, CameInTag, CheckInInput, Edition,
    Feeling, FeelingPart, FeelingPoint, HourRange, ItemFilter, PhoneCalendar, PhoneDeleteScope,
    PhoneSource, Reading, ReadingBatch, ReadingChange, ReadingKind, ReadingValue, RemoteConnection,
    RowKind, Source, TimeSpan, Versus,
};

use crate::{Engine, ManualClock};

struct Fx {
    dir: tempfile::TempDir,
    engine: Engine,
    clock: Arc<ManualClock>,
    phone: String,
}

/// A store at 05:00 London time on Friday 20 November 2026, before the page time, with one
/// paired phone.
fn fx() -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::at("2026-11-20T05:00:00Z", "Europe/London"));
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
    }
}

fn date(s: &str) -> Date {
    s.parse().unwrap()
}

fn count(f: &Fx, sql: &str) -> i64 {
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn entry(phone_id: &str, title: &str, start: &str, end: &str) -> CalendarEntry {
    CalendarEntry {
        phone_id: phone_id.into(),
        calendar_id: "c1".into(),
        title: title.into(),
        start: start.into(),
        end: end.into(),
        time_zone: Some("Europe/London".into()),
        all_day: false,
        place: None,
        people: Vec::new(),
        repeats: false,
        notes: None,
        repeat_rule: None,
    }
}

fn calendars() -> Vec<PhoneCalendar> {
    vec![PhoneCalendar {
        id: "c1".into(),
        name: "Home".into(),
        colour: None,
        kind: CalendarKind::Ordinary,
    }]
}

fn batch(sync: &str, changes: Vec<CalendarChange>) -> CalendarBatch {
    CalendarBatch {
        sync_id: sync.into(),
        seq: 0,
        last: true,
        window_from: "2026-11-18".into(),
        window_to: "2026-12-04".into(),
        calendars: calendars(),
        changes,
    }
}

fn upsert(e: CalendarEntry) -> CalendarChange {
    CalendarChange::Upsert { entry: e }
}

fn dentist_wed() -> CalendarEntry {
    entry(
        "ev1",
        "Dentist",
        "2026-11-25T09:00:00Z",
        "2026-11-25T09:30:00Z",
    )
}

fn dentist_thu() -> CalendarEntry {
    entry(
        "ev1",
        "Dentist",
        "2026-11-26T09:00:00Z",
        "2026-11-26T09:30:00Z",
    )
}

fn current_memory(f: &Fx, item: &str) -> String {
    f.engine
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT m.id FROM memory_sources s JOIN memories m ON m.id = s.memory_id \
                 WHERE s.item_id = ?1 AND m.superseded_at IS NULL",
                [item],
                |r| r.get::<_, String>(0),
            )?)
        })
        .unwrap()
}

fn record_item(f: &Fx, phone_id: &str) -> String {
    f.engine
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT item_id FROM phone_records WHERE phone_id = ?1",
                [phone_id],
                |r| r.get::<_, String>(0),
            )?)
        })
        .unwrap()
}

fn reading(phone_id: &str, day: &str, start: &str, value: ReadingValue) -> ReadingChange {
    ReadingChange::Upsert {
        reading: Reading {
            phone_id: phone_id.into(),
            day: day.into(),
            start: start.into(),
            end: None,
            source_device: Some("Watch".into()),
            value,
        },
    }
}

fn readings(sync: &str, changes: Vec<ReadingChange>) -> ReadingBatch {
    ReadingBatch {
        sync_id: sync.into(),
        seq: 0,
        last: true,
        changes,
    }
}

fn night(from: &str, to: &str) -> ReadingValue {
    ReadingValue::Sleep {
        asleep: vec![TimeSpan {
            from: from.into(),
            to: to.into(),
        }],
        in_bed_only: false,
    }
}

#[test]
fn a_resend_is_kept_once() {
    let f = fx();
    let r = f
        .engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(dentist_wed())]))
        .unwrap();
    assert_eq!((r.taken, r.unchanged), (1, 0));
    let r = f
        .engine
        .take_calendar(&f.phone, batch("s2", vec![upsert(dentist_wed())]))
        .unwrap();
    assert_eq!((r.taken, r.unchanged), (0, 1));
    assert_eq!(
        count(&f, "SELECT COUNT(*) FROM items WHERE kind = 'calendar'"),
        1
    );
    assert_eq!(count(&f, "SELECT COUNT(*) FROM memories"), 1);
    let item = record_item(&f, "ev1");
    let m = f.engine.get_memory(&current_memory(&f, &item)).unwrap();
    assert_eq!(m.sentence, "Dentist is on Wednesday 25 November at 9:00.");
    assert_eq!(m.due.as_deref(), Some("2026-11-25"));
    let view = f.engine.get_item(&item).unwrap();
    assert_eq!(view.from_device.as_deref(), Some("Pixel"));
}

#[test]
fn a_changed_entry_replaces_and_the_page_says_it_moved() {
    let f = fx();
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(dentist_wed())]))
        .unwrap();
    let first = record_item(&f, "ev1");
    let old = current_memory(&f, &first);
    f.clock.set_str("2026-11-20T05:10:00Z");
    let r = f
        .engine
        .take_calendar(&f.phone, batch("s2", vec![upsert(dentist_thu())]))
        .unwrap();
    assert_eq!(r.taken, 1);
    let second = record_item(&f, "ev1");
    assert_ne!(first, second);
    let replaces: Option<String> = f
        .engine
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT json_extract(metadata, '$.replaces') FROM items WHERE id = ?1",
                [&second],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(replaces.as_deref(), Some(first.as_str()));
    let new = current_memory(&f, &second);
    let superseded_by: Option<String> = f
        .engine
        .read(|conn| {
            Ok(conn.query_row(
                "SELECT superseded_by FROM memories WHERE id = ?1",
                [&old],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(
        superseded_by.as_deref(),
        Some(new.as_str()),
        "the old day is history"
    );
    assert_eq!(
        count(&f, "SELECT COUNT(*) FROM conflicts"),
        0,
        "the calendar is not asked about"
    );
    let page = f
        .engine
        .compose_page(date("2026-11-20"), Edition::Screen)
        .unwrap();
    let texts: Vec<&str> = page.changed.iter().map(|l| l.text.as_str()).collect();
    assert!(
        texts.contains(
            &"Dentist moved from Wednesday 25 November to Thursday 26 November, in your calendar."
        ),
        "{texts:?}"
    );
    let came = f
        .engine
        .came_in(&f.phone, PhoneSource::Calendar, None, None, 10)
        .unwrap();
    let tags: Vec<CameInTag> = came.iter().map(|c| c.tag).collect();
    assert_eq!(tags, vec![CameInTag::Changed, CameInTag::New]);
}

#[test]
fn a_removed_entry_closes_its_memory_and_stays() {
    let f = fx();
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(dentist_thu())]))
        .unwrap();
    let item = record_item(&f, "ev1");
    let mem = current_memory(&f, &item);
    f.clock.set_str("2026-11-20T05:10:00Z");
    let r = f
        .engine
        .take_calendar(
            &f.phone,
            batch(
                "s2",
                vec![CalendarChange::Removed {
                    phone_id: "ev1".into(),
                }],
            ),
        )
        .unwrap();
    assert_eq!(r.taken, 1);
    let m = f.engine.get_memory(&mem).unwrap();
    assert_eq!(
        m.valid_to.as_deref(),
        Some("2026-11-20"),
        "closed, not deleted"
    );
    assert!(f.engine.get_item(&item).is_ok(), "the item stays");
    let came = f
        .engine
        .came_in(&f.phone, PhoneSource::Calendar, None, None, 10)
        .unwrap();
    assert_eq!(came[0].tag, CameInTag::Removed);
    let (today, _) = f.engine.calendar_today(date("2026-11-26")).unwrap();
    assert!(today.is_empty(), "not on the page");
    let page = f
        .engine
        .compose_page(date("2026-11-20"), Edition::Screen)
        .unwrap();
    let texts: Vec<&str> = page.changed.iter().map(|l| l.text.as_str()).collect();
    assert!(
        texts.contains(&"Dentist on Thursday 26 November was cancelled in your calendar."),
        "{texts:?}"
    );
    assert!(
        page.upcoming.is_empty(),
        "a cancelled appointment is not this week"
    );
    // Sent again later, it is on again.
    f.engine
        .take_calendar(&f.phone, batch("s3", vec![upsert(dentist_thu())]))
        .unwrap();
    assert_eq!(
        f.engine.calendar_today(date("2026-11-26")).unwrap().0.len(),
        1
    );
}

#[test]
fn an_entry_outside_the_window_is_not_removed() {
    let f = fx();
    let old = entry(
        "ev0",
        "Swim",
        "2026-11-16T18:00:00Z",
        "2026-11-16T19:00:00Z",
    );
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(old)]))
        .unwrap();
    let r = f
        .engine
        .take_calendar(
            &f.phone,
            batch(
                "s2",
                vec![CalendarChange::Removed {
                    phone_id: "ev0".into(),
                }],
            ),
        )
        .unwrap();
    assert_eq!((r.taken, r.unchanged), (0, 1));
    assert_eq!(
        count(
            &f,
            "SELECT COUNT(*) FROM phone_records WHERE removed_at IS NOT NULL"
        ),
        0
    );
}

#[test]
fn one_send_takes_at_most_its_cap() {
    let f = fx();
    let mut taken = 0;
    let mut too_many = false;
    for b in 0..6 {
        let changes = (0..100)
            .filter(|i| b * 100 + i <= 500)
            .map(|i| {
                let n = b * 100 + i;
                upsert(entry(
                    &format!("e{n}"),
                    &format!("Thing {n}"),
                    "2026-11-21T10:00:00Z",
                    "2026-11-21T11:00:00Z",
                ))
            })
            .collect();
        let mut cb = batch("big", changes);
        cb.seq = b as u32;
        let r = f.engine.take_calendar(&f.phone, cb).unwrap();
        taken += r.taken;
        too_many |= r.too_many;
    }
    assert_eq!(taken, super::CALENDAR_PER_SEND);
    assert!(too_many);
    assert_eq!(
        count(&f, "SELECT COUNT(*) FROM conflicts"),
        0,
        "two entries on one day are two appointments"
    );
    let (lines, more) = f.engine.calendar_today(date("2026-11-21")).unwrap();
    assert_eq!((lines.len(), more), (5, 495));
}

#[test]
fn a_stopped_source_takes_nothing() {
    let f = fx();
    f.engine
        .set_source_taking(&f.phone, PhoneSource::Calendar, false)
        .unwrap();
    let r = f
        .engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(dentist_wed())]))
        .unwrap();
    assert!(r.stopped);
    assert_eq!(r.taken, 0);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM items"), 0);
    let view = f.engine.phone_sources().unwrap();
    let cal = view.phones[0].calendar.as_ref().unwrap();
    assert!(!cal.taking);
    assert!(!view.phones[0].in_browser);
}

#[test]
fn a_removed_phone_cannot_send() {
    let f = fx();
    f.engine.revoke_device(&f.phone).unwrap();
    assert!(
        f.engine
            .take_calendar(&f.phone, batch("s1", vec![upsert(dentist_wed())]))
            .is_err()
    );
}

#[test]
fn bad_entries_are_refused_one_by_one() {
    let f = fx();
    let mut blank = dentist_wed();
    blank.phone_id = "ev2".into();
    blank.title = "  ".into();
    let mut backwards = dentist_wed();
    backwards.phone_id = "ev3".into();
    backwards.end = "2026-11-24T09:00:00Z".into();
    let r = f
        .engine
        .take_calendar(
            &f.phone,
            batch(
                "s1",
                vec![upsert(blank), upsert(backwards), upsert(dentist_wed())],
            ),
        )
        .unwrap();
    assert_eq!((r.taken, r.refused), (1, 2));
}

#[test]
fn time_zones_and_clock_changes() {
    let f = fx();
    let mut ny = entry("ny", "Call", "2026-11-21T15:00:00Z", "2026-11-21T16:00:00Z");
    ny.time_zone = Some("America/New_York".into());
    // Clocks went back at 01:00 UTC on 25 October: 00:30 UTC is still 01:30 summer time.
    let edge = entry(
        "dst",
        "Early",
        "2026-10-24T23:30:00Z",
        "2026-10-25T00:30:00Z",
    );
    let mut window = batch("s1", vec![upsert(ny), upsert(edge)]);
    window.window_from = "2026-10-20".into();
    f.engine.take_calendar(&f.phone, window).unwrap();
    let (lines, _) = f.engine.calendar_today(date("2026-11-21")).unwrap();
    assert_eq!(lines[0].zone.as_deref(), Some("New York"));
    let (lines, _) = f.engine.calendar_today(date("2026-10-25")).unwrap();
    assert_eq!(lines.len(), 1, "00:30 summer time on the 25th");
    assert_eq!(lines[0].zone, None);
}

#[test]
fn all_day_entries_come_first() {
    let f = fx();
    let mut holiday = entry("hd", "Day off", "2026-11-23", "2026-11-24");
    holiday.all_day = true;
    let meeting = entry(
        "mt",
        "Meeting",
        "2026-11-23T08:00:00Z",
        "2026-11-23T09:00:00Z",
    );
    f.engine
        .take_calendar(
            &f.phone,
            batch("s1", vec![upsert(meeting), upsert(holiday)]),
        )
        .unwrap();
    let (lines, _) = f.engine.calendar_today(date("2026-11-23")).unwrap();
    let titles: Vec<&str> = lines.iter().map(|l| l.title.as_str()).collect();
    assert_eq!(titles, vec!["Day off", "Meeting"]);
    assert_eq!(lines[0].calendar, "Home");
    assert_eq!(lines[0].phone, "Pixel");
}

#[test]
fn calendar_and_readings_are_not_kept_things() {
    let f = fx();
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(dentist_wed())]))
        .unwrap();
    f.engine
        .take_readings(
            &f.phone,
            readings(
                "h1",
                vec![reading(
                    "st1",
                    "2026-11-19",
                    "2026-11-19T00:00:00Z",
                    ReadingValue::Steps { total: 8000 },
                )],
            ),
        )
        .unwrap();
    assert!(
        f.engine.items(ItemFilter::default()).unwrap().is_empty(),
        "not in Recently kept"
    );
    assert!(f.engine.journal_days().unwrap().is_empty());
    let page = f
        .engine
        .compose_page(date("2026-11-20"), Edition::Screen)
        .unwrap();
    assert_eq!((page.kept_count, page.pending), (0, 0));
    assert_eq!(
        count(
            &f,
            "SELECT COUNT(*) FROM items WHERE kind = 'reading' AND status = 'understood'"
        ),
        1,
        "a reading is never waiting to be read"
    );
    assert_eq!(
        count(&f, "SELECT COUNT(*) FROM memories"),
        1,
        "readings make no memories"
    );
}

#[test]
fn past_appointments_are_not_said_goodbye_to() {
    let f = fx();
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(dentist_wed())]))
        .unwrap();
    let mem = current_memory(&f, &record_item(&f, "ev1"));
    f.engine
        .write(|tx| {
            tx.execute(
                "UPDATE memories SET status = 'letGo', let_go_at = ?2 WHERE id = ?1",
                rusqlite::params![mem, "2026-11-20T05:00:00.000Z"],
            )?;
            Ok(())
        })
        .unwrap();
    let page = f
        .engine
        .compose_page(date("2026-11-20"), Edition::Screen)
        .unwrap();
    assert!(page.let_go.is_empty());
    assert_eq!(page.let_go_count, 0);
}

#[test]
fn a_private_entry_stays_off_every_edition_but_the_screen() {
    let f = fx();
    let other = entry(
        "ev2",
        "Lunch",
        "2026-11-20T12:00:00Z",
        "2026-11-20T13:00:00Z",
    );
    let secret = entry(
        "ev3",
        "Clinic",
        "2026-11-20T15:00:00Z",
        "2026-11-20T16:00:00Z",
    );
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(other), upsert(secret)]))
        .unwrap();
    f.engine
        .set_item_private(&record_item(&f, "ev3"), true)
        .unwrap();
    let day = date("2026-11-20");
    let screen = f.engine.compose_page(day, Edition::Screen).unwrap();
    assert_eq!(screen.calendar_today.len(), 2);
    for edition in [Edition::Phone, Edition::Print, Edition::Audio] {
        let page = f.engine.compose_page(day, edition).unwrap();
        let titles: Vec<&str> = page
            .calendar_today
            .iter()
            .map(|l| l.title.as_str())
            .collect();
        assert_eq!(titles, vec!["Lunch"], "{edition:?}");
        assert_eq!(page.more_calendar_today, 0);
    }
    // Saved, the page keeps none of it, and every edition reads it again.
    f.engine.save_page(&screen).unwrap();
    assert_eq!(
        count(
            &f,
            "SELECT COUNT(*) FROM pages WHERE body LIKE '%\"calendarToday\":[{%' \
             OR body LIKE '%\"moreCalendarToday\":2%'"
        ),
        0
    );
    let phone = f
        .engine
        .page_for(Some(day), Edition::Phone)
        .unwrap()
        .unwrap();
    let titles: Vec<&str> = phone
        .calendar_today
        .iter()
        .map(|l| l.title.as_str())
        .collect();
    assert_eq!(titles, vec!["Lunch"]);
    let phone = crate::phone_view(phone).unwrap();
    assert_eq!(phone.calendar_today.len(), 1);
    assert_eq!(
        f.engine
            .page(Some(day))
            .unwrap()
            .unwrap()
            .calendar_today
            .len(),
        2
    );
    // Turned off, nothing shows.
    f.engine
        .set_phone_page_options(nocturne_contract::PhonePageOptions {
            calendar_today: false,
            sleep_last_night: true,
        })
        .unwrap();
    assert!(
        f.engine
            .page(Some(day))
            .unwrap()
            .unwrap()
            .calendar_today
            .is_empty()
    );
}

#[test]
fn a_night_belongs_to_the_day_it_ends_on() {
    let f = fx();
    f.engine
        .take_readings(
            &f.phone,
            readings(
                "h1",
                vec![reading(
                    "n1",
                    "2026-11-20",
                    "2026-11-19T23:50:00Z",
                    night("2026-11-19T23:50:00Z", "2026-11-20T06:30:00Z"),
                )],
            ),
        )
        .unwrap();
    let n = f.engine.last_night(date("2026-11-20")).unwrap().unwrap();
    assert_eq!(n.asleep_min, 400);
    assert_eq!((n.awake_count, n.phone.as_str()), (0, "Pixel"));
    assert_eq!(n.versus, None, "no usual before two weeks");
    assert!(f.engine.last_night(date("2026-11-19")).unwrap().is_none());
    // A night held back from phones (readings cannot be marked private, so only by hand here)
    // stays off every edition but the screen.
    let set_private = |on: bool| {
        f.engine
            .write(|tx| {
                tx.execute(
                    "UPDATE items SET private = ?2 WHERE id = ?1",
                    rusqlite::params![n.item_id, on],
                )?;
                Ok(())
            })
            .unwrap()
    };
    set_private(true);
    for edition in [Edition::Phone, Edition::Print] {
        let page = f.engine.compose_page(date("2026-11-20"), edition).unwrap();
        assert!(page.last_night.is_none(), "{edition:?}");
    }
    set_private(false);
    assert!(
        f.engine.set_item_private(&n.item_id, true).is_err(),
        "a reading cannot be marked private"
    );
    let screen = f
        .engine
        .compose_page(date("2026-11-20"), Edition::Screen)
        .unwrap();
    assert!(screen.last_night.is_some());
    f.engine.save_page(&screen).unwrap();
    assert_eq!(
        count(
            &f,
            "SELECT COUNT(*) FROM pages WHERE body LIKE '%lastNight%'"
        ),
        0,
        "the night is never stored with the page"
    );
    // Never read aloud; on the screen and the phone.
    let day = date("2026-11-20");
    assert!(
        f.engine
            .compose_page(day, Edition::Audio)
            .unwrap()
            .last_night
            .is_none()
    );
    assert!(
        f.engine
            .compose_page(day, Edition::Phone)
            .unwrap()
            .last_night
            .is_some()
    );
    // The journal: "Woke", and the first check-in carries the night.
    f.clock.set_str("2026-11-20T10:00:00Z");
    f.engine
        .keep_checkin(
            CheckInInput {
                feeling: Feeling {
                    part: FeelingPart::LitUp,
                    point: FeelingPoint {
                        pleasant: 0.5,
                        energy: 0.5,
                    },
                    words: Vec::new(),
                },
                line: None,
                urge: None,
                replaces: None,
            },
            Source::QuickCapture,
        )
        .unwrap();
    let j = f.engine.journal_day(day).unwrap();
    assert!(j.rows.iter().any(|r| r.kind == RowKind::Woke));
    let after = j.checkins[0].after_sleep.as_ref().unwrap();
    assert_eq!(
        (after.minutes, after.item_id.as_str()),
        (400, n.item_id.as_str())
    );
    assert!(j.health.unwrap().night.is_some());
}

#[test]
fn the_usual_leaves_out_removed_readings() {
    let f = fx();
    let mut changes = Vec::new();
    for i in 0..14 {
        let d = date("2026-11-20")
            .checked_sub(jiff::Span::new().days(i))
            .unwrap();
        changes.push(reading(
            &format!("rhr{i}"),
            &d.to_string(),
            &format!("{d}T08:00:00Z"),
            ReadingValue::RestingHeartRate {
                bpm: 50.0 + i as f64,
            },
        ));
    }
    f.engine
        .take_readings(&f.phone, readings("h1", changes))
        .unwrap();
    let s = f
        .engine
        .health_series(
            ReadingKind::RestingHeartRate,
            date("2026-11-07"),
            date("2026-11-20"),
        )
        .unwrap();
    assert_eq!(s.points.len(), 14);
    assert!(s.usual.is_some());
    f.engine
        .take_readings(
            &f.phone,
            readings(
                "h2",
                vec![ReadingChange::Removed {
                    phone_id: "rhr0".into(),
                    kind: ReadingKind::RestingHeartRate,
                }],
            ),
        )
        .unwrap();
    let s = f
        .engine
        .health_series(
            ReadingKind::RestingHeartRate,
            date("2026-11-07"),
            date("2026-11-20"),
        )
        .unwrap();
    assert_eq!(s.points.len(), 13);
    assert_eq!(s.usual, None, "13 days give no usual");
    let rows = f
        .engine
        .readings(
            ReadingKind::RestingHeartRate,
            date("2026-11-07"),
            date("2026-11-20"),
        )
        .unwrap();
    assert_eq!(rows.len(), 14, "the list keeps it");
    assert!(rows[0].removed, "newest first, marked removed");
}

#[test]
fn a_short_night_is_less_than_usual() {
    let f = fx();
    let mut changes = Vec::new();
    for i in 1..=20 {
        let d = date("2026-11-20")
            .checked_sub(jiff::Span::new().days(i))
            .unwrap();
        let prev = d.yesterday().unwrap();
        changes.push(reading(
            &format!("n{i}"),
            &d.to_string(),
            &format!("{prev}T23:00:00Z"),
            night(&format!("{prev}T23:00:00Z"), &format!("{d}T07:00:00Z")),
        ));
    }
    changes.push(reading(
        "short",
        "2026-11-20",
        "2026-11-20T02:00:00Z",
        night("2026-11-20T02:00:00Z", "2026-11-20T06:00:00Z"),
    ));
    f.engine
        .take_readings(&f.phone, readings("h1", changes))
        .unwrap();
    let n = f.engine.last_night(date("2026-11-20")).unwrap().unwrap();
    assert_eq!(n.versus, Some(Versus::Less));
    assert_eq!(n.usual_bed.as_deref(), Some("23:00"));
    assert_eq!(n.usual_wake.as_deref(), Some("07:00"));
}

#[test]
fn the_day_shows_workouts_and_from_health() {
    let f = fx();
    f.engine
        .take_readings(
            &f.phone,
            readings(
                "h1",
                vec![
                    reading(
                        "w1",
                        "2026-11-19",
                        "2026-11-19T16:00:00Z",
                        ReadingValue::Workout {
                            kind: "Walk".into(),
                            length_s: 2520,
                            distance_m: Some(3100.0),
                        },
                    ),
                    reading(
                        "hr1",
                        "2026-11-19",
                        "2026-11-19T00:00:00Z",
                        ReadingValue::HeartRate {
                            hours: vec![
                                HourRange {
                                    start: "2026-11-19T08:00:00Z".into(),
                                    low: 52.0,
                                    high: 90.0,
                                    middle: 70.0,
                                },
                                HourRange {
                                    start: "2026-11-19T16:00:00Z".into(),
                                    low: 60.0,
                                    high: 148.0,
                                    middle: 78.0,
                                },
                            ],
                            count: 200,
                        },
                    ),
                    reading(
                        "bp1",
                        "2026-11-19",
                        "2026-11-19T07:30:00Z",
                        ReadingValue::BloodPressure {
                            top: 128.0,
                            bottom: 82.0,
                        },
                    ),
                ],
            ),
        )
        .unwrap();
    let j = f.engine.journal_day(date("2026-11-19")).unwrap();
    assert!(j.rows.iter().any(|r| r.kind == RowKind::Workout));
    let h = j.health.unwrap();
    assert_eq!(h.workouts[0].kind, "Walk");
    let heart = h.heart.unwrap();
    assert_eq!((heart.low, heart.high, heart.middle), (52.0, 148.0, 74.0));
    assert_eq!(h.pressure.len(), 1);
    assert!(h.steps.is_none() && h.night.is_none());
}

fn two_entries_and_a_change(f: &Fx) {
    let swim = entry(
        "ev9",
        "Swim",
        "2026-11-22T10:00:00Z",
        "2026-11-22T11:00:00Z",
    );
    f.engine
        .take_calendar(
            &f.phone,
            batch("s1", vec![upsert(dentist_wed()), upsert(swim)]),
        )
        .unwrap();
    f.engine
        .take_calendar(&f.phone, batch("s2", vec![upsert(dentist_thu())]))
        .unwrap();
}

#[test]
fn deleting_what_came_in_previews_then_applies() {
    let f = fx();
    two_entries_and_a_change(&f);
    f.engine
        .take_readings(
            &f.phone,
            readings(
                "h1",
                vec![reading(
                    "st1",
                    "2026-11-19",
                    "2026-11-19T00:00:00Z",
                    ReadingValue::Steps { total: 8000 },
                )],
            ),
        )
        .unwrap();
    let scope = PhoneDeleteScope {
        device_id: f.phone.clone(),
        source: PhoneSource::Calendar,
        kinds: None,
        before: None,
        item_id: None,
    };
    let p = f.engine.preview_phone_delete(scope).unwrap();
    assert_eq!((p.items, p.versions, p.memories), (2, 1, 3));
    f.engine.delete_phone_data(&p.token, true).unwrap();
    assert_eq!(
        count(&f, "SELECT COUNT(*) FROM items WHERE kind = 'calendar'"),
        0
    );
    assert_eq!(count(&f, "SELECT COUNT(*) FROM memories"), 0);
    assert_eq!(
        count(
            &f,
            "SELECT COUNT(*) FROM phone_records WHERE source = 'calendar'"
        ),
        0
    );
    assert_eq!(
        count(&f, "SELECT COUNT(*) FROM items WHERE kind = 'reading'"),
        1,
        "health stays"
    );
    assert!(
        !f.engine
            .source_taking(&f.phone, PhoneSource::Calendar)
            .unwrap()
    );
    // A token is single use.
    assert!(f.engine.delete_phone_data(&p.token, false).is_err());
}

#[test]
fn deleting_readings_by_kind_and_day() {
    let f = fx();
    f.engine
        .take_readings(
            &f.phone,
            readings(
                "h1",
                vec![
                    reading(
                        "st1",
                        "2026-11-10",
                        "2026-11-10T00:00:00Z",
                        ReadingValue::Steps { total: 8000 },
                    ),
                    reading(
                        "st2",
                        "2026-11-19",
                        "2026-11-19T00:00:00Z",
                        ReadingValue::Steps { total: 9000 },
                    ),
                    reading(
                        "w1",
                        "2026-11-10",
                        "2026-11-10T16:00:00Z",
                        ReadingValue::Weight {
                            value: 70.0,
                            unit: "kg".into(),
                        },
                    ),
                ],
            ),
        )
        .unwrap();
    let p = f
        .engine
        .preview_phone_delete(PhoneDeleteScope {
            device_id: f.phone.clone(),
            source: PhoneSource::Health,
            kinds: Some(vec![ReadingKind::Steps]),
            before: Some("2026-11-15".into()),
            item_id: None,
        })
        .unwrap();
    assert_eq!((p.items, p.memories), (1, 0));
    f.engine.delete_phone_data(&p.token, false).unwrap();
    assert_eq!(
        count(&f, "SELECT COUNT(*) FROM items WHERE kind = 'reading'"),
        2
    );
    assert!(
        f.engine
            .source_taking(&f.phone, PhoneSource::Health)
            .unwrap()
    );
}

#[test]
fn removing_a_phone_can_delete_what_it_brought() {
    let f = fx();
    two_entries_and_a_change(&f);
    f.engine.delete_brought(&f.phone).unwrap();
    assert_eq!(count(&f, "SELECT COUNT(*) FROM items"), 0);
    // Stopped first, so nothing comes in before the phone is removed.
    let r = f
        .engine
        .take_calendar(&f.phone, batch("late", vec![upsert(dentist_wed())]))
        .unwrap();
    assert!(r.stopped);
    f.engine.revoke_device(&f.phone).unwrap();
    f.engine.forget_phone_sources(&f.phone).unwrap();
    assert_eq!(
        count(&f, "SELECT COUNT(*) FROM phone_sources"),
        0,
        "no calendar names left"
    );
    assert!(
        f.engine.phone_sources().unwrap().phones.is_empty(),
        "the phone is not listed"
    );
}

#[test]
fn delete_everything_clears_what_phones_brought() {
    let f = fx();
    two_entries_and_a_change(&f);
    f.engine.delete_everything(crate::DELETE_PHRASE).unwrap();
    for table in ["phone_records", "phone_sources", "items"] {
        assert_eq!(
            count(&f, &format!("SELECT COUNT(*) FROM {table}")),
            0,
            "{table}"
        );
    }
}

#[test]
fn export_writes_a_calendar_per_phone_and_a_table_per_kind() {
    let f = fx();
    let mut e = dentist_wed();
    e.place = Some("High Street, Leeds".into());
    e.people = vec!["Sam".into()];
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(e)]))
        .unwrap();
    f.engine
        .take_readings(
            &f.phone,
            readings(
                "h1",
                vec![
                    reading(
                        "bp1",
                        "2026-11-19",
                        "2026-11-19T07:30:00Z",
                        ReadingValue::BloodPressure {
                            top: 128.0,
                            bottom: 82.0,
                        },
                    ),
                    reading(
                        "bp2",
                        "2026-11-19",
                        "2026-11-19T19:30:00Z",
                        ReadingValue::BloodPressure {
                            top: 121.0,
                            bottom: 79.0,
                        },
                    ),
                ],
            ),
        )
        .unwrap();
    f.engine
        .take_readings(
            &f.phone,
            readings(
                "h2",
                vec![ReadingChange::Removed {
                    phone_id: "bp2".into(),
                    kind: ReadingKind::BloodPressure,
                }],
            ),
        )
        .unwrap();
    let out = tempfile::tempdir().unwrap();
    let report = f.engine.export_to(out.path()).unwrap();
    let root = std::path::PathBuf::from(report.path);
    let ics = std::fs::read_to_string(root.join("calendar/Pixel.ics")).unwrap();
    let item = record_item(&f, "ev1");
    let expected = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Nocturne//Export//EN\r\nBEGIN:VEVENT\r\n\
         UID:ev1\r\nDTSTAMP:20261120T050000Z\r\nDTSTART:20261125T090000Z\r\n\
         DTEND:20261125T093000Z\r\nSUMMARY:Dentist\r\nLOCATION:High Street\\, Leeds\r\n\
         DESCRIPTION:With Sam\r\nCATEGORIES:Home\r\nX-NOCTURNE-ITEM:{item}\r\nEND:VEVENT\r\n\
         END:VCALENDAR\r\n"
    );
    assert_eq!(ics, expected);
    let csv = std::fs::read_to_string(root.join("health/Blood pressure.csv")).unwrap();
    assert_eq!(
        csv,
        "day,time,value,unit,source,removed\r\n\
         2026-11-19,07:30,128 over 82,mmHg,\"Watch, through Health on Pixel\",\r\n\
         2026-11-19,19:30,121 over 79,mmHg,\"Watch, through Health on Pixel\",removed in Health\r\n"
    );
}

#[test]
fn page_options_and_noticing_are_kept() {
    let f = fx();
    let v = f.engine.phone_sources().unwrap();
    assert!(v.page.calendar_today && v.page.sleep_last_night && v.noticing);
    assert!(v.phones[0].in_browser, "a phone that brought nothing");
    f.engine.set_noticing(false).unwrap();
    f.engine
        .set_phone_page_options(nocturne_contract::PhonePageOptions {
            calendar_today: true,
            sleep_last_night: false,
        })
        .unwrap();
    let v = f.engine.phone_sources().unwrap();
    assert!(!v.noticing && !v.page.sleep_last_night && v.page.calendar_today);
    // The settings view still reads.
    assert_eq!(f.engine.settings().unwrap().page_time, "07:00");
}

fn second_phone(f: &Fx) -> String {
    f.engine
        .add_device(
            "Tablet",
            &[8; 32],
            "https://example.test",
            RemoteConnection::Simple,
            "example.test",
        )
        .unwrap()
        .id
}

#[test]
fn a_private_entry_stays_private_when_it_changes() {
    let f = fx();
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(dentist_wed())]))
        .unwrap();
    f.engine
        .set_item_private(&record_item(&f, "ev1"), true)
        .unwrap();
    f.engine
        .take_calendar(&f.phone, batch("s2", vec![upsert(dentist_thu())]))
        .unwrap();
    let second = record_item(&f, "ev1");
    assert!(f.engine.get_item(&second).unwrap().private);
    let phone = f
        .engine
        .compose_page(date("2026-11-26"), Edition::Phone)
        .unwrap();
    assert!(phone.calendar_today.is_empty());
    let m = f.engine.get_memory(&current_memory(&f, &second)).unwrap();
    assert!(m.private);
}

#[test]
fn deleting_keeps_what_the_persons_own_keeps_also_say() {
    let f = fx();
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(dentist_wed())]))
        .unwrap();
    let item = record_item(&f, "ev1");
    let mem = current_memory(&f, &item);
    // A letter says the same appointment: one memory with both receipts.
    let letter = match f
        .engine
        .keep_text("Dentist on Wednesday at nine", Source::QuickCapture)
        .unwrap()
    {
        nocturne_contract::Kept::New { item } | nocturne_contract::Kept::Duplicate { item } => {
            item.id
        }
    };
    let lease = f
        .engine
        .lease_item(&letter, std::time::Duration::from_secs(60))
        .unwrap()
        .lease;
    let mut c = crate::Candidate::new(
        "Dentist",
        crate::Predicate::Scheduled,
        "Wednesday 25 November at 9:00",
        "Dentist is on Wednesday 25 November at 9:00.",
        0.9,
    );
    c.due = Some(date("2026-11-25"));
    f.engine.apply_candidates(&letter, &lease, vec![c]).unwrap();
    assert_eq!(f.engine.get_memory(&mem).unwrap().receipts.len(), 2);
    let p = f
        .engine
        .preview_phone_delete(PhoneDeleteScope {
            device_id: f.phone.clone(),
            source: PhoneSource::Calendar,
            kinds: None,
            before: None,
            item_id: None,
        })
        .unwrap();
    assert_eq!((p.items, p.memories, p.shared_memories), (1, 0, 1));
    f.engine.delete_phone_data(&p.token, false).unwrap();
    let kept = f.engine.get_memory(&mem).unwrap();
    let from: Vec<&str> = kept.receipts.iter().map(|r| r.item_id.as_str()).collect();
    assert_eq!(from, vec![letter.as_str()], "only the phone's receipt went");
    assert!(f.engine.get_item(&item).is_err(), "the entry went");
    assert_eq!(
        count(
            &f,
            "SELECT COUNT(*) FROM phone_sources WHERE calendars != '[]'"
        ),
        0,
        "a whole-calendar delete forgets the calendars' names"
    );
}

#[test]
fn one_reading_can_be_deleted() {
    let f = fx();
    f.engine
        .take_readings(
            &f.phone,
            readings(
                "h1",
                vec![
                    reading(
                        "st1",
                        "2026-11-18",
                        "2026-11-18T00:00:00Z",
                        ReadingValue::Steps { total: 1 },
                    ),
                    reading(
                        "st2",
                        "2026-11-19",
                        "2026-11-19T00:00:00Z",
                        ReadingValue::Steps { total: 2 },
                    ),
                ],
            ),
        )
        .unwrap();
    let one = record_item(&f, "steps/st1");
    let p = f
        .engine
        .preview_phone_delete(PhoneDeleteScope {
            device_id: f.phone.clone(),
            source: PhoneSource::Health,
            kinds: None,
            before: None,
            item_id: Some(one.clone()),
        })
        .unwrap();
    assert_eq!(p.items, 1);
    f.engine.delete_phone_data(&p.token, false).unwrap();
    assert_eq!(
        count(&f, "SELECT COUNT(*) FROM items WHERE kind = 'reading'"),
        1
    );
    assert!(f.engine.get_item(&one).is_err());
}

#[test]
fn a_cancel_on_one_phone_leaves_what_another_still_says() {
    let f = fx();
    let other = second_phone(&f);
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(dentist_wed())]))
        .unwrap();
    f.engine
        .take_calendar(&other, batch("s1", vec![upsert(dentist_wed())]))
        .unwrap();
    let mem = current_memory(&f, &record_item(&f, "ev1"));
    f.engine
        .take_calendar(
            &f.phone,
            batch(
                "s2",
                vec![CalendarChange::Removed {
                    phone_id: "ev1".into(),
                }],
            ),
        )
        .unwrap();
    assert_eq!(f.engine.get_memory(&mem).unwrap().valid_to, None);
}

#[test]
fn an_unread_earlier_version_is_not_left_waiting() {
    let f = fx();
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(dentist_wed())]))
        .unwrap();
    let first = record_item(&f, "ev1");
    f.engine
        .take_calendar(&f.phone, batch("s2", vec![upsert(dentist_thu())]))
        .unwrap();
    assert_eq!(
        f.engine.get_item(&first).unwrap().status,
        nocturne_contract::ItemStatus::Understood
    );
    assert_eq!(f.engine.status_counts().unwrap().waiting_to_read, 0);
}

#[test]
fn a_title_already_read_is_not_read_again() {
    let f = fx();
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(dentist_wed())]))
        .unwrap();
    let first = record_item(&f, "ev1");
    f.engine
        .write(|tx| {
            tx.execute(
                "UPDATE items SET status = 'understood' WHERE id = ?1",
                [&first],
            )?;
            Ok(())
        })
        .unwrap();
    let mut again = dentist_thu();
    again.phone_id = "ev2".into();
    f.engine
        .take_calendar(&f.phone, batch("s2", vec![upsert(again)]))
        .unwrap();
    assert_eq!(
        f.engine.get_item(&record_item(&f, "ev2")).unwrap().status,
        nocturne_contract::ItemStatus::Understood
    );
}

#[test]
fn renaming_a_calendar_is_not_a_change_to_its_entries() {
    let f = fx();
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(dentist_wed())]))
        .unwrap();
    let mut renamed = batch("s2", vec![upsert(dentist_wed())]);
    renamed.calendars[0].name = "Family".into();
    let r = f.engine.take_calendar(&f.phone, renamed).unwrap();
    assert_eq!((r.taken, r.unchanged), (0, 1));
    let (lines, _) = f.engine.calendar_today(date("2026-11-25")).unwrap();
    assert_eq!(lines[0].calendar, "Family");
}

#[test]
fn an_entry_over_several_days_is_on_each() {
    let f = fx();
    let mut away = entry("aw", "Away", "2026-11-23", "2026-11-26");
    away.all_day = true;
    let late = entry(
        "lt",
        "Late shift",
        "2026-11-27T22:00:00Z",
        "2026-11-28T01:00:00Z",
    );
    let to_midnight = entry(
        "mn",
        "Party",
        "2026-11-29T20:00:00Z",
        "2026-11-30T00:00:00Z",
    );
    f.engine
        .take_calendar(
            &f.phone,
            batch("s1", vec![upsert(away), upsert(late), upsert(to_midnight)]),
        )
        .unwrap();
    let on = |d: &str| -> Vec<String> {
        f.engine
            .calendar_today(date(d))
            .unwrap()
            .0
            .into_iter()
            .map(|l| l.title)
            .collect()
    };
    assert_eq!(on("2026-11-24"), vec!["Away"]);
    assert_eq!(on("2026-11-25"), vec!["Away"]);
    assert!(
        on("2026-11-26").is_empty(),
        "an all-day end is not a day it is on"
    );
    assert_eq!(on("2026-11-28"), vec!["Late shift"]);
    assert!(on("2026-11-30").is_empty(), "ending at midnight");
}

#[test]
fn the_entrys_own_page_and_what_came_in() {
    let f = fx();
    let mut wed = dentist_wed();
    wed.repeats = true;
    wed.repeat_rule = Some("FREQ=WEEKLY;BYDAY=WE".into());
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(wed)]))
        .unwrap();
    f.clock.set_str("2026-11-20T05:10:00Z");
    f.engine
        .take_calendar(&f.phone, batch("s2", vec![upsert(dentist_thu())]))
        .unwrap();
    let item = f.engine.get_item(&record_item(&f, "ev1")).unwrap();
    let c = item.calendar.unwrap();
    assert_eq!((c.calendar.as_str(), c.phone.as_str()), ("Home", "Pixel"));
    assert_eq!(c.was.unwrap().start, "2026-11-25T09:00:00Z");
    assert!(!c.removed);
    let came = f
        .engine
        .came_in(&f.phone, PhoneSource::Calendar, None, None, 1)
        .unwrap();
    assert_eq!(came.len(), 1);
    assert_eq!(came[0].start.as_deref(), Some("2026-11-26T09:00:00Z"));
    assert_eq!(came[0].calendar.as_deref(), Some("Home"));
    let older = f
        .engine
        .came_in(
            &f.phone,
            PhoneSource::Calendar,
            Some(&came[0].came_in_at),
            None,
            10,
        )
        .unwrap();
    assert_eq!(older.len(), 1);
    assert_eq!(older[0].tag, CameInTag::New);
}

#[test]
fn repeat_rules_in_words() {
    use super::read::repeat_words;
    assert_eq!(
        repeat_words("FREQ=WEEKLY;BYDAY=FR").as_deref(),
        Some("Every Friday")
    );
    assert_eq!(
        repeat_words("RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE").as_deref(),
        Some("Every other Monday and Wednesday")
    );
    assert_eq!(repeat_words("FREQ=DAILY").as_deref(), Some("Every day"));
    assert_eq!(repeat_words("FREQ=MONTHLY;BYMONTHDAY=3"), None);
}

#[test]
fn blood_pressure_has_a_usual_for_each_number() {
    let f = fx();
    let mut changes = Vec::new();
    for i in 0..14 {
        let d = date("2026-11-20")
            .checked_sub(jiff::Span::new().days(i))
            .unwrap();
        changes.push(reading(
            &format!("bp{i}"),
            &d.to_string(),
            &format!("{d}T08:00:00Z"),
            ReadingValue::BloodPressure {
                top: 120.0 + i as f64,
                bottom: 80.0 + i as f64,
            },
        ));
    }
    f.engine
        .take_readings(&f.phone, readings("h1", changes))
        .unwrap();
    let s = f
        .engine
        .health_series(
            ReadingKind::BloodPressure,
            date("2026-11-07"),
            date("2026-11-20"),
        )
        .unwrap();
    let u = s.pressure_usual.unwrap();
    assert_eq!((u.top.low, u.bottom.low), (123.25, 83.25));
}

#[test]
fn delete_this_day_lists_what_phones_brought() {
    let f = fx();
    let today = entry("td", "Swim", "2026-11-19T18:00:00Z", "2026-11-19T19:00:00Z");
    f.engine
        .take_calendar(&f.phone, batch("s1", vec![upsert(today)]))
        .unwrap();
    f.engine
        .take_readings(
            &f.phone,
            readings(
                "h1",
                vec![reading(
                    "st1",
                    "2026-11-19",
                    "2026-11-19T00:00:00Z",
                    ReadingValue::Steps { total: 3 },
                )],
            ),
        )
        .unwrap();
    let j = f.engine.journal_day(date("2026-11-19")).unwrap();
    let b = j.from_phones.unwrap();
    assert_eq!(b.calendar[0].title, "Swim");
    assert_eq!(b.health[0].title, "Steps");
    assert_eq!(b.health[0].phone, "Pixel");
}

#[test]
fn what_came_in_pages_through_one_big_send() {
    let f = fx();
    let changes = (0..7)
        .map(|n| {
            upsert(entry(
                &format!("p{n}"),
                &format!("Thing {n}"),
                "2026-11-21T10:00:00Z",
                "2026-11-21T11:00:00Z",
            ))
        })
        .collect();
    f.engine
        .take_calendar(&f.phone, batch("s1", changes))
        .unwrap();
    let mut seen = Vec::new();
    let mut cursor: Option<(String, String)> = None;
    loop {
        let page = f
            .engine
            .came_in(
                &f.phone,
                PhoneSource::Calendar,
                cursor.as_ref().map(|c| c.0.as_str()),
                cursor.as_ref().map(|c| c.1.as_str()),
                3,
            )
            .unwrap();
        if page.is_empty() {
            break;
        }
        let last = page.last().unwrap();
        cursor = Some((last.came_in_at.clone(), last.item_id.clone()));
        seen.extend(page.into_iter().map(|r| r.item_id));
    }
    assert_eq!(seen.len(), 7, "every row of the send, each once");
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 7);
}

#[test]
fn a_reading_names_the_phone_it_came_from() {
    let f = fx();
    f.engine
        .take_readings(
            &f.phone,
            readings(
                "h1",
                vec![reading(
                    "st1",
                    "2026-11-19",
                    "2026-11-19T00:00:00Z",
                    ReadingValue::Steps { total: 5 },
                )],
            ),
        )
        .unwrap();
    let rows = f
        .engine
        .readings(ReadingKind::Steps, date("2026-11-19"), date("2026-11-19"))
        .unwrap();
    assert_eq!(rows[0].device_id, f.phone);
}
