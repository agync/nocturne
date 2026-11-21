//! The sends with the phone's sources and the computer as fakes.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::time::Duration;

use nocturne_channel::client::{CallError, Reach};
use nocturne_contract::{
    CalendarChange, CalendarEntry, CalendarKind, PhoneCalendar, PhoneError, PhoneFeature, Reading,
    ReadingChange, ReadingKind, ReadingValue, RemoteAnswer, RemoteCall, SyncReceipt,
};

use crate::sync::ledger::SentEntry;
use crate::sync::{
    Access, Availability, Changes, Device, FullRead, Granted, Instances, LedgerFile, Link,
    SourceError, SourceStatus, Syncer, batches, calendar, hash, health,
};

const BOTH: [PhoneFeature; 2] = [PhoneFeature::Calendar, PhoneFeature::Health];

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

fn cal(id: &str, kind: CalendarKind) -> PhoneCalendar {
    PhoneCalendar {
        id: id.into(),
        name: format!("Calendar {id}"),
        colour: Some("#3F51B5".into()),
        kind,
    }
}

fn entry(id: &str, day: &str, calendar: &str) -> CalendarEntry {
    CalendarEntry {
        phone_id: id.into(),
        calendar_id: calendar.into(),
        title: format!("Appointment {id}"),
        start: format!("{day}T09:00:00Z"),
        end: format!("{day}T09:30:00Z"),
        time_zone: Some("Europe/London".into()),
        all_day: false,
        place: None,
        people: vec![],
        repeats: false,
        notes: Some("Call link: example".into()),
        repeat_rule: None,
    }
}

fn steps(day: &str, total: u32) -> Reading {
    Reading {
        phone_id: day.into(),
        day: day.into(),
        start: format!("{day}T00:00:00Z"),
        end: None,
        source_device: None,
        value: ReadingValue::Steps { total },
    }
}

fn pressure(id: &str, day: &str) -> Reading {
    Reading {
        phone_id: id.into(),
        day: day.into(),
        start: format!("{day}T08:00:00Z"),
        end: None,
        source_device: Some("This phone".into()),
        value: ReadingValue::BloodPressure {
            top: 120.0,
            bottom: 80.0,
        },
    }
}

struct FakeDevice {
    access: Mutex<Access>,
    calendars: Mutex<Vec<PhoneCalendar>>,
    window: Mutex<(String, String)>,
    entries: Mutex<Vec<CalendarEntry>>,
    asked_notes: Mutex<Option<bool>>,
    granted: Mutex<Granted>,
    readings: Mutex<Vec<Reading>>,
    changes: Mutex<Option<Changes>>,
    reads: Mutex<Vec<u32>>,
}

impl FakeDevice {
    fn new() -> FakeDevice {
        FakeDevice {
            access: Mutex::new(Access::Granted),
            calendars: Mutex::new(vec![
                cal("1", CalendarKind::Ordinary),
                cal("2", CalendarKind::Holidays),
                cal("3", CalendarKind::Birthdays),
            ]),
            window: Mutex::new(("2026-11-19".into(), "2026-12-05".into())),
            entries: Mutex::new(vec![]),
            asked_notes: Mutex::new(None),
            granted: Mutex::new(Granted {
                kinds: vec![ReadingKind::Steps, ReadingKind::BloodPressure],
                history: false,
                distance: false,
            }),
            readings: Mutex::new(vec![]),
            changes: Mutex::new(None),
            reads: Mutex::new(vec![]),
        }
    }
}

impl Device for FakeDevice {
    fn calendar_access(&self) -> Access {
        *self.access.lock().unwrap()
    }
    fn calendars(&self) -> Result<Vec<PhoneCalendar>, SourceError> {
        Ok(self.calendars.lock().unwrap().clone())
    }
    fn instances(
        &self,
        back: u32,
        ahead: u32,
        ids: &[String],
        notes: bool,
    ) -> Result<Instances, SourceError> {
        assert_eq!((back, ahead), (2, 14));
        *self.asked_notes.lock().unwrap() = Some(notes);
        let (from, to) = self.window.lock().unwrap().clone();
        Ok(Instances {
            window_from: from,
            window_to: to,
            entries: self
                .entries
                .lock()
                .unwrap()
                .iter()
                .filter(|e| ids.contains(&e.calendar_id))
                .cloned()
                .collect(),
        })
    }
    fn health_availability(&self) -> Availability {
        Availability::Available
    }
    fn health_granted(&self) -> Result<Granted, SourceError> {
        Ok(self.granted.lock().unwrap().clone())
    }
    fn health_read(&self, kinds: &[ReadingKind], days: u32) -> Result<FullRead, SourceError> {
        self.reads.lock().unwrap().push(days);
        Ok(FullRead {
            readings: self
                .readings
                .lock()
                .unwrap()
                .iter()
                .filter(|r| kinds.contains(&health::kind_of(&r.value)))
                .cloned()
                .collect(),
            token: format!("t{}", self.reads.lock().unwrap().len()),
            from_day: "2026-10-23".into(),
        })
    }
    fn health_changes(
        &self,
        _kinds: &[ReadingKind],
        token: &str,
        refresh_days: u32,
    ) -> Result<Changes, SourceError> {
        assert!(token.starts_with('t'));
        assert_eq!(refresh_days, 2);
        Ok(self.changes.lock().unwrap().take().unwrap_or_default())
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Take,
    Away,
    Stopped,
    /// The computer takes this many changes in the send, then says too many.
    Cap(u32),
    /// Busy this many times, then takes.
    Busy(u32),
}

struct FakeLink {
    mode: Mutex<Mode>,
    calls: Mutex<Vec<RemoteCall>>,
    taken: Mutex<u32>,
    pauses: Mutex<u32>,
}

impl FakeLink {
    fn new(mode: Mode) -> FakeLink {
        FakeLink {
            mode: Mutex::new(mode),
            calls: Mutex::new(vec![]),
            taken: Mutex::new(0),
            pauses: Mutex::new(0),
        }
    }

    fn set(&self, mode: Mode) {
        *self.mode.lock().unwrap() = mode;
    }

    fn calendar_changes(&self) -> Vec<CalendarChange> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter_map(|c| match c {
                RemoteCall::SendCalendar { batch } => Some(batch.changes.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn reading_changes(&self) -> Vec<ReadingChange> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter_map(|c| match c {
                RemoteCall::SendReadings { batch } => Some(batch.changes.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn clear(&self) {
        self.calls.lock().unwrap().clear();
    }

    fn answer(&self, call: &RemoteCall) -> Result<RemoteAnswer, CallError> {
        let n = match call {
            RemoteCall::SendCalendar { batch } => batch.changes.len() as u32,
            RemoteCall::SendReadings { batch } => batch.changes.len() as u32,
            _ => 0,
        };
        let receipt = |taken: u32, refused: u32, stopped: bool, too_many: bool| {
            Ok(RemoteAnswer::Synced {
                receipt: SyncReceipt {
                    taken,
                    unchanged: 0,
                    refused,
                    stopped,
                    too_many,
                },
            })
        };
        let mut mode = self.mode.lock().unwrap();
        match *mode {
            Mode::Take => receipt(n, 0, false, false),
            Mode::Away => Err(CallError::Connection {
                state: Reach::Asleep,
            }),
            Mode::Stopped => receipt(0, 0, true, false),
            Mode::Cap(cap) => {
                let mut taken = self.taken.lock().unwrap();
                let fits = n.min(cap.saturating_sub(*taken));
                *taken += fits;
                receipt(fits, n - fits, false, fits < n)
            }
            Mode::Busy(0) => {
                *mode = Mode::Take;
                receipt(n, 0, false, false)
            }
            Mode::Busy(left) => {
                *mode = Mode::Busy(left - 1);
                Ok(RemoteAnswer::Error {
                    code: PhoneError::TooMany,
                })
            }
        }
    }
}

impl Link for FakeLink {
    fn send(
        &self,
        call: RemoteCall,
    ) -> impl std::future::Future<Output = Result<RemoteAnswer, CallError>> + Send {
        let answer = self.answer(&call);
        self.calls.lock().unwrap().push(call);
        async move { answer }
    }

    fn pause(&self, _how_long: Duration) -> impl std::future::Future<Output = ()> + Send {
        *self.pauses.lock().unwrap() += 1;
        async {}
    }
}

fn syncer() -> (tempfile::TempDir, Syncer) {
    let dir = tempfile::tempdir().unwrap();
    let s = Syncer::new(LedgerFile::open(dir.path()));
    (dir, s)
}

fn bring_calendar(s: &Syncer) {
    s.ledger.update(|l| l.calendar.settings.bring = true);
}

fn bring_health(s: &Syncer) {
    s.ledger.update(|l| l.health.settings.bring = true);
}

fn sync(s: &Syncer, d: &FakeDevice, l: &FakeLink) -> crate::sync::SyncReport {
    block(s.sync(d, l, &BOTH, "2026-11-21T08:00:00Z".into())).unwrap()
}

fn ids(changes: &[CalendarChange]) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = changes
        .iter()
        .map(|c| match c {
            CalendarChange::Upsert { entry } => (entry.phone_id.clone(), true),
            CalendarChange::Removed { phone_id } => (phone_id.clone(), false),
        })
        .collect();
    out.sort();
    out
}

#[test]
fn new_changed_and_removed_entries_go_once() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = vec![
        entry("a", "2026-11-21", "1"),
        entry("b", "2026-11-23", "1"),
        entry("c", "2026-11-24", "1"),
    ];
    let link = FakeLink::new(Mode::Take);
    let report = sync(&s, &d, &link);
    assert_eq!(report.calendar.unwrap().taken, 3);
    assert_eq!(link.calendar_changes().len(), 3);

    // Nothing changed: nothing goes.
    link.clear();
    let report = sync(&s, &d, &link);
    assert_eq!(report.calendar, None);
    assert!(link.calls.lock().unwrap().is_empty());

    // One moved, one deleted on the phone.
    link.clear();
    {
        let mut e = d.entries.lock().unwrap();
        e[1].start = "2026-11-23T10:00:00Z".into();
        e.remove(2);
    }
    sync(&s, &d, &link);
    assert_eq!(
        ids(&link.calendar_changes()),
        vec![("b".into(), true), ("c".into(), false)]
    );
    let l = s.ledger.get();
    assert!(l.calendar.pending.is_empty());
    assert_eq!(l.last_sent.unwrap().calendar_changes, 2);
}

#[test]
fn leaving_the_window_by_time_is_not_removed() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = vec![
        entry("old", "2026-11-19", "1"),
        entry("edge", "2026-11-20", "1"),
        entry("far", "2026-12-05", "1"),
    ];
    let link = FakeLink::new(Mode::Take);
    sync(&s, &d, &link);

    // Two days later: the window moved on, and the first two aged out of it.
    *d.window.lock().unwrap() = ("2026-11-21".into(), "2026-12-07".into());
    d.entries.lock().unwrap().retain(|e| e.phone_id == "far");
    link.clear();
    sync(&s, &d, &link);
    assert!(link.calendar_changes().is_empty());
    let sent = s.ledger.get().calendar.sent;
    assert_eq!(sent.keys().collect::<Vec<_>>(), vec!["far"]);
}

#[test]
fn an_entry_on_the_far_edge_counts_once_it_is_inside() {
    let sent: BTreeMap<String, SentEntry> = [(
        "far".to_string(),
        SentEntry {
            hash: "x".into(),
            day: "2026-12-05".into(),
            calendar: "1".into(),
        },
    )]
    .into();
    let brought: BTreeSet<String> = ["1".to_string()].into();
    let on_edge = calendar::diff(&sent, vec![], ("2026-11-19", "2026-12-05"), &brought, false);
    assert!(on_edge.changes.is_empty());
    assert!(on_edge.forget.is_empty());
    // A day from the edge still counts as the edge: a time zone apart must never remove it.
    let near = calendar::diff(&sent, vec![], ("2026-11-20", "2026-12-06"), &brought, false);
    assert!(near.changes.is_empty() && near.forget.is_empty());
    let inside = calendar::diff(&sent, vec![], ("2026-11-21", "2026-12-07"), &brought, false);
    assert_eq!(
        inside.changes.values().collect::<Vec<_>>(),
        vec![&CalendarChange::Removed {
            phone_id: "far".into()
        }]
    );
}

#[test]
fn holidays_and_birthdays_start_off() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = vec![
        entry("a", "2026-11-21", "1"),
        entry("h", "2026-11-22", "2"),
        entry("b", "2026-11-23", "3"),
    ];
    let link = FakeLink::new(Mode::Take);
    sync(&s, &d, &link);
    assert_eq!(ids(&link.calendar_changes()), vec![("a".into(), true)]);
    match &link.calls.lock().unwrap()[0] {
        RemoteCall::SendCalendar { batch } => {
            assert_eq!(batch.seq, 0);
            assert!(batch.last);
            assert_eq!(batch.calendars, vec![cal("1", CalendarKind::Ordinary)]);
        }
        other => panic!("not a calendar send: {other:?}"),
    }
    assert_eq!(s.sources(&d, &BOTH).calendars, 1);

    // Turned on by hand, the holidays go too.
    s.ledger
        .update(|l| l.calendar.settings.calendars.insert("2".into(), true));
    link.clear();
    sync(&s, &d, &link);
    assert_eq!(ids(&link.calendar_changes()), vec![("h".into(), true)]);
}

#[test]
fn a_calendar_turned_off_is_let_go_without_removing() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = vec![entry("a", "2026-11-21", "1")];
    let link = FakeLink::new(Mode::Take);
    sync(&s, &d, &link);
    s.ledger
        .update(|l| l.calendar.settings.calendars.insert("1".into(), false));
    link.clear();
    sync(&s, &d, &link);
    assert!(link.calendar_changes().is_empty());
    assert!(s.ledger.get().calendar.sent.is_empty());
}

#[test]
fn notes_off_strips_the_text() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = vec![entry("a", "2026-11-21", "1")];
    let link = FakeLink::new(Mode::Take);
    sync(&s, &d, &link);
    assert_eq!(*d.asked_notes.lock().unwrap(), Some(false));
    match &link.calendar_changes()[0] {
        CalendarChange::Upsert { entry } => assert_eq!(entry.notes, None),
        other => panic!("{other:?}"),
    }

    // Turned on, the notes go as a change.
    s.ledger.update(|l| l.calendar.settings.notes = true);
    link.clear();
    sync(&s, &d, &link);
    assert_eq!(*d.asked_notes.lock().unwrap(), Some(true));
    match &link.calendar_changes()[0] {
        CalendarChange::Upsert { entry } => {
            assert_eq!(entry.notes.as_deref(), Some("Call link: example"))
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn batches_keep_to_the_count_and_the_size() {
    let small: Vec<u32> = (0..250).collect();
    let (parts, dropped) = batches(small, 200);
    assert_eq!(
        parts.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![100, 100, 50]
    );
    assert!(dropped.is_empty());

    let big: Vec<String> = (0..10).map(|_| "x".repeat(20_000)).collect();
    let (parts, _) = batches(big, 200);
    assert_eq!(parts.len(), 4);
    for p in &parts {
        assert!(serde_json::to_vec(p).unwrap().len() + 200 <= 65_536);
    }

    let (parts, dropped) = batches(vec!["x".repeat(70_000)], 200);
    assert!(parts.is_empty());
    assert_eq!(dropped.len(), 1);
}

#[test]
fn one_send_goes_in_batches_under_one_id() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = (0..230)
        .map(|i| entry(&format!("e{i:03}"), "2026-11-22", "1"))
        .collect();
    let link = FakeLink::new(Mode::Take);
    let report = sync(&s, &d, &link);
    assert_eq!(report.calendar.unwrap().taken, 230);
    let calls = link.calls.lock().unwrap();
    let batches: Vec<_> = calls
        .iter()
        .map(|c| match c {
            RemoteCall::SendCalendar { batch } => batch.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(batches.len(), 3);
    assert!(batches.iter().all(|b| b.sync_id == batches[0].sync_id));
    assert_eq!(
        batches.iter().map(|b| (b.seq, b.last)).collect::<Vec<_>>(),
        vec![(0, false), (1, false), (2, true)]
    );
    assert!(!batches[0].calendars.is_empty());
    assert!(batches[1].calendars.is_empty());
    assert!(nocturne_channel::crypto::check_id(&batches[0].sync_id).is_ok());
}

#[test]
fn too_many_keeps_the_rest_waiting() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = (0..150)
        .map(|i| entry(&format!("e{i:03}"), "2026-11-22", "1"))
        .collect();
    let link = FakeLink::new(Mode::Cap(100));
    let report = sync(&s, &d, &link).calendar.unwrap();
    assert!(report.too_many);
    assert_eq!(report.taken, 100);
    let l = s.ledger.get();
    assert!(l.calendar.too_many);
    // The batch the computer cut short is not counted as sent.
    assert_eq!(l.calendar.sent.len(), 100);
    assert_eq!(l.calendar.pending.len(), 50);
    assert!(s.sources(&d, &BOTH).calendar_too_many);
}

#[test]
fn a_send_keeps_to_what_the_computer_takes_at_once() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = (0..520)
        .map(|i| entry(&format!("e{i:03}"), "2026-11-22", "1"))
        .collect();
    let link = FakeLink::new(Mode::Take);
    sync(&s, &d, &link);
    assert_eq!(link.calendar_changes().len(), 500);
    assert!(s.ledger.get().calendar.too_many);
    link.clear();
    sync(&s, &d, &link);
    assert_eq!(link.calendar_changes().len(), 20);
    assert!(!s.ledger.get().calendar.too_many);
}

#[test]
fn while_away_changes_wait_and_only_the_latest_goes() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    bring_health(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = vec![entry("a", "2026-11-22", "1")];
    *d.readings.lock().unwrap() = vec![steps("2026-11-20", 4000)];
    let link = FakeLink::new(Mode::Away);
    let report = sync(&s, &d, &link);
    assert_eq!(report.calendar, None);
    assert_eq!(report.health, None);
    assert_eq!(s.ledger.get().health.pending.len(), 1);

    // Still away: the entry moved and the day's steps grew.
    d.entries.lock().unwrap()[0].start = "2026-11-22T11:00:00Z".into();
    *d.changes.lock().unwrap() = Some(Changes {
        readings: vec![steps("2026-11-20", 6000)],
        removed: vec![],
        token: Some("t9".into()),
        ..Changes::default()
    });
    sync(&s, &d, &link);
    assert_eq!(s.ledger.get().health.token.as_deref(), Some("t9"));

    // Back: one version of each.
    link.set(Mode::Take);
    link.clear();
    sync(&s, &d, &link);
    let cal = link.calendar_changes();
    assert_eq!(cal.len(), 1);
    match &cal[0] {
        CalendarChange::Upsert { entry } => assert_eq!(entry.start, "2026-11-22T11:00:00Z"),
        other => panic!("{other:?}"),
    }
    let readings = link.reading_changes();
    assert_eq!(readings.len(), 1);
    match &readings[0] {
        ReadingChange::Upsert { reading } => {
            assert_eq!(reading.value, ReadingValue::Steps { total: 6000 })
        }
        other => panic!("{other:?}"),
    }
    assert!(s.ledger.get().health.pending.is_empty());
}

#[test]
fn stopped_on_the_computer_takes_nothing_and_says_so() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = vec![entry("a", "2026-11-22", "1")];
    let link = FakeLink::new(Mode::Stopped);
    let report = sync(&s, &d, &link);
    assert!(report.calendar.unwrap().stopped);
    assert_eq!(
        s.sources(&d, &BOTH).calendar,
        SourceStatus::StoppedOnComputer
    );
    assert_eq!(s.ledger.get().calendar.pending.len(), 1);

    // Turned back on there: the next send learns it and what waited goes.
    link.set(Mode::Take);
    link.clear();
    sync(&s, &d, &link);
    assert_eq!(link.calendar_changes().len(), 1);
    assert_eq!(s.sources(&d, &BOTH).calendar, SourceStatus::On);
}

#[test]
fn a_busy_computer_is_tried_again_after_a_pause() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = vec![entry("a", "2026-11-22", "1")];
    let link = FakeLink::new(Mode::Busy(2));
    let report = sync(&s, &d, &link);
    assert_eq!(report.calendar.unwrap().taken, 1);
    assert_eq!(*link.pauses.lock().unwrap(), 2);
}

#[test]
fn statuses_follow_the_phone_and_the_computer() {
    let (_dir, s) = syncer();
    let d = FakeDevice::new();
    let st = s.sources(&d, &BOTH);
    assert_eq!(st.calendar, SourceStatus::NotBrought);
    assert_eq!(st.health, SourceStatus::NotBrought);
    assert_eq!(
        s.sources(&d, &[]).calendar,
        SourceStatus::NeedsNewerComputer
    );
    *d.access.lock().unwrap() = Access::Blocked;
    assert_eq!(s.sources(&d, &BOTH).calendar, SourceStatus::OffInSettings);
    bring_calendar(&s);
    *d.access.lock().unwrap() = Access::Refused;
    assert_eq!(s.sources(&d, &BOTH).calendar, SourceStatus::OffInSettings);
    *d.access.lock().unwrap() = Access::Granted;
    assert_eq!(s.sources(&d, &BOTH).calendar, SourceStatus::On);

    bring_health(&s);
    let st = s.sources(&d, &BOTH);
    assert_eq!(st.health, SourceStatus::On);
    assert_eq!(st.kinds, 2);
    d.granted.lock().unwrap().kinds.clear();
    assert_eq!(s.sources(&d, &BOTH).health, SourceStatus::OffInSettings);
}

#[test]
fn a_calendar_taken_back_sends_nothing() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = vec![entry("a", "2026-11-22", "1")];
    *d.access.lock().unwrap() = Access::Blocked;
    let link = FakeLink::new(Mode::Take);
    assert_eq!(sync(&s, &d, &link).calendar, None);
    assert!(link.calls.lock().unwrap().is_empty());
}

#[test]
fn health_reads_whole_then_only_what_changed() {
    let (_dir, s) = syncer();
    bring_health(&s);
    let d = FakeDevice::new();
    *d.readings.lock().unwrap() = vec![
        steps("2026-11-20", 4000),
        steps("2026-11-21", 900),
        pressure("bp-1", "2026-11-20"),
    ];
    let link = FakeLink::new(Mode::Take);
    let report = sync(&s, &d, &link);
    assert_eq!(report.health.unwrap().taken, 3);
    assert_eq!(*d.reads.lock().unwrap(), vec![30]);

    // The day's steps grow in Health too, as a read again would find.
    d.readings.lock().unwrap()[1] = steps("2026-11-21", 1500);
    // A reading deleted in Health comes as an id; it goes as removed with its kind.
    *d.changes.lock().unwrap() = Some(Changes {
        readings: vec![steps("2026-11-21", 1500)],
        removed: vec!["bp-1".into(), "never-sent".into()],
        token: Some("t5".into()),
        ..Changes::default()
    });
    link.clear();
    sync(&s, &d, &link);
    let mut got = link.reading_changes();
    got.sort_by_key(|c| format!("{c:?}"));
    assert_eq!(
        got,
        vec![
            ReadingChange::Removed {
                phone_id: "bp-1".into(),
                kind: ReadingKind::BloodPressure
            },
            ReadingChange::Upsert {
                reading: steps("2026-11-21", 1500)
            },
        ]
    );
    // "never-sent" matched nothing: the daily kinds were read again.
    assert_eq!(*d.reads.lock().unwrap(), vec![30, 30]);
}

#[test]
fn an_expired_token_reads_whole_and_finds_what_went() {
    let (_dir, s) = syncer();
    bring_health(&s);
    let d = FakeDevice::new();
    *d.readings.lock().unwrap() = vec![steps("2026-11-20", 4000), pressure("bp-1", "2026-11-20")];
    let link = FakeLink::new(Mode::Take);
    sync(&s, &d, &link);
    d.readings.lock().unwrap().pop();
    *d.changes.lock().unwrap() = Some(Changes {
        expired: true,
        ..Changes::default()
    });
    link.clear();
    sync(&s, &d, &link);
    assert_eq!(*d.reads.lock().unwrap(), vec![30, 30]);
    assert_eq!(
        link.reading_changes(),
        vec![ReadingChange::Removed {
            phone_id: "bp-1".into(),
            kind: ReadingKind::BloodPressure
        }]
    );
}

#[test]
fn older_readings_read_a_year_when_allowed() {
    let (_dir, s) = syncer();
    s.ledger.update(|l| {
        l.health.settings.bring = true;
        l.health.settings.older_readings = true;
    });
    let d = FakeDevice::new();
    let link = FakeLink::new(Mode::Take);
    sync(&s, &d, &link);
    // Not allowed yet: 30 days.
    assert_eq!(*d.reads.lock().unwrap(), vec![30]);
    d.granted.lock().unwrap().history = true;
    sync(&s, &d, &link);
    assert_eq!(*d.reads.lock().unwrap(), vec![30, 365]);
}

#[test]
fn only_chosen_and_allowed_kinds_are_read() {
    let kinds = health::kinds_to_read(
        &[ReadingKind::Weight, ReadingKind::Steps, ReadingKind::Sleep],
        &[
            ReadingKind::Steps,
            ReadingKind::Weight,
            ReadingKind::HeartRate,
        ],
    );
    assert_eq!(kinds, vec![ReadingKind::Steps, ReadingKind::Weight]);
    assert!(
        !crate::sync::ledger::HealthSettings::default()
            .kinds
            .contains(&ReadingKind::Weight)
    );
}

#[test]
fn the_ledger_survives_a_restart() {
    let (dir, s) = syncer();
    bring_calendar(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = vec![entry("a", "2026-11-22", "1")];
    sync(&s, &d, &FakeLink::new(Mode::Take));
    drop(s);
    let again = Syncer::new(LedgerFile::open(dir.path()));
    let l = again.ledger.get();
    assert!(l.calendar.settings.bring);
    assert_eq!(
        l.calendar.sent["a"].hash,
        hash(&{
            let mut e = entry("a", "2026-11-22", "1");
            e.notes = None;
            e
        })
    );
    // A damaged ledger forgets what was sent, never what the person chose.
    std::fs::write(dir.path().join("sync.json"), b"{broken").unwrap();
    let damaged = LedgerFile::open(dir.path()).get();
    assert!(damaged.calendar.settings.bring);
    assert!(damaged.calendar.sent.is_empty());
}

#[test]
fn while_stopped_only_a_check_goes_and_nothing_is_read() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    bring_health(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = vec![entry("a", "2026-11-22", "1")];
    *d.readings.lock().unwrap() = vec![steps("2026-11-20", 4000)];
    let link = FakeLink::new(Mode::Stopped);
    sync(&s, &d, &link);
    let token = s.ledger.get().health.token.clone();
    *d.asked_notes.lock().unwrap() = None;
    *d.changes.lock().unwrap() = Some(Changes {
        readings: vec![steps("2026-11-21", 10)],
        token: Some("t9".into()),
        ..Changes::default()
    });
    link.clear();
    let report = sync(&s, &d, &link);
    assert!(report.calendar.unwrap().stopped && report.health.unwrap().stopped);
    assert!(link.calendar_changes().is_empty() && link.reading_changes().is_empty());
    assert_eq!(link.calls.lock().unwrap().len(), 2, "one check for each");
    assert_eq!(
        *d.asked_notes.lock().unwrap(),
        None,
        "the calendar was not read"
    );
    assert!(d.changes.lock().unwrap().is_some(), "Health was not read");
    assert_eq!(s.ledger.get().health.token, token);
}

#[test]
fn the_earliest_go_first_when_there_are_too_many() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    let d = FakeDevice::new();
    // Ids against the days, so an order by id would cut the wrong ones.
    *d.entries.lock().unwrap() = (0..510)
        .map(|i| {
            let mut e = entry(&format!("e{:03}", 509 - i), "2026-11-22", "1");
            e.start = format!("2026-11-22T{:02}:{:02}:00Z", i / 60, i % 60);
            e
        })
        .collect();
    let link = FakeLink::new(Mode::Take);
    sync(&s, &d, &link);
    let sent = link.calendar_changes();
    assert_eq!(sent.len(), 500);
    let starts: Vec<String> = sent
        .iter()
        .map(|c| match c {
            CalendarChange::Upsert { entry } => entry.start.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    let mut sorted = starts.clone();
    sorted.sort();
    assert_eq!(starts, sorted);
    assert_eq!(starts[0], "2026-11-22T00:00:00Z");
}

#[test]
fn an_entry_too_large_to_send_is_not_tried_again() {
    let (_dir, s) = syncer();
    s.ledger.update(|l| {
        l.calendar.settings.bring = true;
        l.calendar.settings.notes = true;
    });
    let d = FakeDevice::new();
    let mut big = entry("big", "2026-11-22", "1");
    big.notes = Some("n".repeat(70_000));
    *d.entries.lock().unwrap() = vec![big, entry("a", "2026-11-22", "1")];
    let link = FakeLink::new(Mode::Take);
    sync(&s, &d, &link);
    assert_eq!(ids(&link.calendar_changes()), vec![("a".into(), true)]);
    link.clear();
    assert_eq!(sync(&s, &d, &link).calendar, None);
    assert!(link.calls.lock().unwrap().is_empty());
}

#[test]
fn waiting_changes_follow_what_is_still_brought() {
    let (_dir, s) = syncer();
    bring_health(&s);
    let d = FakeDevice::new();
    *d.readings.lock().unwrap() = vec![steps("2026-11-20", 4000), pressure("bp-1", "2026-11-20")];
    let link = FakeLink::new(Mode::Away);
    sync(&s, &d, &link);
    assert_eq!(s.ledger.get().health.pending.len(), 2);
    s.ledger
        .update(|l| l.health.settings.kinds.retain(|k| *k != ReadingKind::Steps));
    link.set(Mode::Take);
    link.clear();
    sync(&s, &d, &link);
    let sent = link.reading_changes();
    assert_eq!(sent.len(), 1);
    assert!(
        sent.iter()
            .all(|c| matches!(c, ReadingChange::Upsert { reading } if reading.phone_id == "bp-1"))
    );

    let mut l = crate::sync::ledger::CalendarLedger::default();
    l.pending.insert(
        "a".into(),
        CalendarChange::Upsert {
            entry: entry("a", "2026-11-22", "1"),
        },
    );
    l.pending.insert(
        "b".into(),
        CalendarChange::Upsert {
            entry: entry("b", "2026-11-22", "2"),
        },
    );
    calendar::prune(&mut l, |c| c == "2");
    assert_eq!(l.pending.keys().collect::<Vec<_>>(), vec!["b"]);
}

#[test]
fn a_send_stops_at_once_when_turned_off() {
    let link = FakeLink::new(Mode::Take);
    let asked = std::sync::atomic::AtomicU32::new(0);
    let out = block(crate::sync::send_all(
        &link,
        vec![vec![1u32], vec![2], vec![3]],
        |_, _, _| RemoteCall::GetStatus,
        || asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 1,
        |_, _| {},
    ));
    assert_eq!(out.ended, crate::sync::Sent::Off);
    assert_eq!(link.calls.lock().unwrap().len(), 1);
}

#[test]
fn distance_allowed_or_not_reads_whole_again() {
    let (_dir, s) = syncer();
    bring_health(&s);
    let d = FakeDevice::new();
    let link = FakeLink::new(Mode::Take);
    sync(&s, &d, &link);
    sync(&s, &d, &link);
    assert_eq!(*d.reads.lock().unwrap(), vec![30]);
    d.granted.lock().unwrap().distance = true;
    sync(&s, &d, &link);
    assert_eq!(*d.reads.lock().unwrap(), vec![30, 30]);
    assert_eq!(
        s.sources(&d, &BOTH).health_from.as_deref(),
        Some("2026-10-23")
    );
}

#[test]
fn a_day_read_again_and_empty_was_deleted_in_health() {
    let (_dir, s) = syncer();
    bring_health(&s);
    let d = FakeDevice::new();
    *d.readings.lock().unwrap() = vec![steps("2026-11-20", 4000), steps("2026-11-21", 900)];
    let link = FakeLink::new(Mode::Take);
    sync(&s, &d, &link);
    *d.changes.lock().unwrap() = Some(Changes {
        readings: vec![steps("2026-11-20", 4000)],
        token: Some("t5".into()),
        refreshed: [(
            ReadingKind::Steps,
            vec!["2026-11-20".to_string(), "2026-11-21".to_string()],
        )]
        .into(),
        ..Changes::default()
    });
    link.clear();
    sync(&s, &d, &link);
    assert_eq!(
        link.reading_changes(),
        vec![ReadingChange::Removed {
            phone_id: "2026-11-21".into(),
            kind: ReadingKind::Steps
        }]
    );
}

#[test]
fn a_deletion_that_matches_nothing_reads_the_days_again() {
    let (_dir, s) = syncer();
    bring_health(&s);
    let d = FakeDevice::new();
    *d.readings.lock().unwrap() = vec![steps("2026-11-01", 4000), steps("2026-11-02", 900)];
    let link = FakeLink::new(Mode::Take);
    sync(&s, &d, &link);
    // One of the day's step records went: Health says only its id.
    d.readings.lock().unwrap().remove(0);
    *d.changes.lock().unwrap() = Some(Changes {
        removed: vec!["some-steps-record".into()],
        token: Some("t5".into()),
        ..Changes::default()
    });
    link.clear();
    sync(&s, &d, &link);
    assert_eq!(*d.reads.lock().unwrap(), vec![30, 30]);
    assert_eq!(
        link.reading_changes(),
        vec![ReadingChange::Removed {
            phone_id: "2026-11-01".into(),
            kind: ReadingKind::Steps
        }]
    );
    assert_eq!(s.ledger.get().health.token.as_deref(), Some("t5"));
}

#[test]
fn each_source_says_when_it_last_sent() {
    let (_dir, s) = syncer();
    bring_calendar(&s);
    bring_health(&s);
    let d = FakeDevice::new();
    *d.entries.lock().unwrap() = vec![entry("a", "2026-11-22", "1")];
    *d.readings.lock().unwrap() = vec![steps("2026-11-20", 4000), steps("2026-11-21", 10)];
    sync(&s, &d, &FakeLink::new(Mode::Take));
    let st = s.sources(&d, &BOTH);
    assert_eq!(st.calendar_last_sent.unwrap().count, 1);
    assert_eq!(st.health_last_sent.unwrap().count, 2);
}
