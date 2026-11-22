//! The phone app's own sends against the real computer: the phone's sync (its ledger, diff and
//! batches, from `phone/src-tauri/src/sync`) over the real client and the real listener, into the
//! real engine. The phone's calendar and Health are a stand-in; everything after them is real.
//!
//! `device_bench` (ignored) keeps such a computer running for a debug phone app behind
//! `adb reverse` and prints how many entries and readings came in, never what they say.

#![cfg(unix)]

mod common;

#[allow(dead_code, unused_imports)]
#[path = "../../phone/src-tauri/src/sync/mod.rs"]
mod sync;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::*;
use nocturne_app_lib::remote::controller::RemoteHub;
use nocturne_app_lib::remote::limits::ManualMono;
use nocturne_app_lib::remote::testing;
use nocturne_channel::client::{self, CallError, Connection, OnIdentity, Options, Phone, Router};
use nocturne_channel::phone::{PhoneIdentity, PhonePairing};
use nocturne_contract::{
    CalendarEntry, CameInTag, PhoneCalendar, PhoneFeature, Reading, ReadingKind, ReadingValue,
    RemoteAnswer, RemoteCall, RemoteState,
};
use nocturne_engine::FakeMind;
use nocturne_helpers::{HelperKind, Origin};
use nocturne_intelligence::FakeHost;
use serde_json::json;
use sync::{
    Access, Availability, Changes, Device, FullRead, Granted, Instances, LedgerFile, Link,
    SourceError, SourceStatus, Syncer,
};

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

/// A relay on loopback in front of the listener's socket, where the helper would be.
fn relay(socket: std::path::PathBuf, port: u16) -> u16 {
    let listener = block(tokio::net::TcpListener::bind(("127.0.0.1", port))).unwrap();
    let port = listener.local_addr().unwrap().port();
    tauri::async_runtime::spawn(async move {
        while let Ok((phone, _)) = listener.accept().await {
            let socket = socket.clone();
            tokio::spawn(async move {
                if let Ok(mut computer) = tokio::net::UnixStream::connect(socket).await {
                    let mut phone = phone;
                    let _ = tokio::io::copy_bidirectional(&mut phone, &mut computer).await;
                }
            });
        }
    });
    port
}

struct Computer {
    h: Harness,
    hub: Arc<RemoteHub>,
    mono: Arc<ManualMono>,
    router: Router,
    port: u16,
    _files: tempfile::TempDir,
}

fn computer(port: u16) -> Computer {
    let h = harness_with_mind(Arc::new(FakeMind::new()));
    let files = tempfile::tempdir().unwrap();
    std::fs::write(files.path().join("index.html"), b"<!doctype html>").unwrap();
    let (hub, helpers, mono) =
        testing::install_fakes(h.app.handle(), FakeHost::new(), files.path());
    helpers.install_now(HelperKind::Cloudflared);
    h.ok(
        "start_remote",
        json!({"connection": "simple", "allowDownload": false}),
    );
    wait_for("on", || matches!(hub.state(), RemoteState::On { .. }));
    let Some(Origin::Unix(socket)) = hub.listener_origin() else {
        panic!("a socket listener")
    };
    let port = relay(socket, port);
    Computer {
        h,
        hub,
        mono,
        router: client::loopback_router(port),
        port,
        _files: files,
    }
}

impl Computer {
    fn waiting_for_you(&self) -> bool {
        matches!(
            block(self.hub.view()).pairing.map(|p| p.phase),
            Some(nocturne_contract::PairingPhase::WaitingForYou { .. })
        )
    }

    /// A paired phone with a session open, and its id here.
    fn phone(&self) -> (Phone, String) {
        let offer = self.h.ok("begin_pairing", json!({}));
        let pairing = PhonePairing::scanned(offer["link"].as_str().unwrap(), "Pixel").unwrap();
        let router = self.router.clone();
        let run = tauri::async_runtime::spawn(async move {
            client::pair(pairing, &router, &client::http_client(), |_| {}).await
        });
        wait_for("asked", || self.waiting_for_you());
        self.h.ok(
            "decide_pairing",
            json!({"pairingId": offer["pairingId"], "allow": true}),
        );
        let identity: PhoneIdentity = block(run).unwrap().unwrap();
        let on: OnIdentity = Arc::new(|_| {});
        let phone = Phone::new(identity, self.router.clone(), Options::default(), on);
        let Connection::Connected { hello, .. } = block(phone.connect()) else {
            panic!("no session")
        };
        assert_eq!(
            hello.features,
            vec![
                PhoneFeature::Calendar,
                PhoneFeature::Health,
                PhoneFeature::HoldsMemory
            ]
        );
        (phone, self.device_id())
    }

    fn device_id(&self) -> String {
        self.h.engine().devices(false).unwrap()[0].id.clone()
    }

    fn came_in(&self, device: &str, source: &str) -> Vec<nocturne_contract::CameInRow> {
        serde_json::from_value(self.h.ok(
            "list_came_in",
            json!({"deviceId": device, "source": source, "before": null, "limit": 1000}),
        ))
        .unwrap()
    }
}

/// The phone's calendar and Health, as the plugins would give them.
struct Stand {
    entries: Mutex<Vec<CalendarEntry>>,
    readings: Mutex<Vec<Reading>>,
    changes: Mutex<Option<Changes>>,
}

impl Stand {
    fn new() -> Stand {
        Stand {
            entries: Mutex::new(vec![]),
            readings: Mutex::new(vec![]),
            changes: Mutex::new(None),
        }
    }
}

const FROM: &str = "2026-11-11";
const TO: &str = "2026-11-27";

impl Device for Stand {
    fn calendar_access(&self) -> Access {
        Access::Granted
    }
    fn calendars(&self) -> Result<Vec<PhoneCalendar>, SourceError> {
        Ok(vec![
            PhoneCalendar {
                id: "1".into(),
                name: "Home".into(),
                colour: Some("#3F51B5".into()),
            },
            PhoneCalendar {
                id: "2".into(),
                name: "Holidays in United Kingdom".into(),
                colour: None,
            },
        ])
    }
    fn instances(
        &self,
        _back: u32,
        _ahead: u32,
        ids: &[String],
        _notes: bool,
    ) -> Result<Instances, SourceError> {
        Ok(Instances {
            window_from: FROM.into(),
            window_to: TO.into(),
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
        Ok(Granted {
            kinds: vec![
                ReadingKind::Sleep,
                ReadingKind::HeartRate,
                ReadingKind::Steps,
            ],
            history: false,
            distance: false,
            refusals_hidden: false,
        })
    }
    fn health_read(&self, _kinds: &[ReadingKind], _days: u32) -> Result<FullRead, SourceError> {
        Ok(FullRead {
            readings: self.readings.lock().unwrap().clone(),
            token: "t1".into(),
            from_day: "2026-10-15".into(),
        })
    }
    fn health_changes(
        &self,
        _kinds: &[ReadingKind],
        _token: &str,
        _refresh: u32,
    ) -> Result<Changes, SourceError> {
        Ok(self.changes.lock().unwrap().take().unwrap_or_default())
    }
}

/// The real client; a pause moves the computer's clock instead of waiting.
struct Wire {
    phone: Phone,
    mono: Arc<ManualMono>,
    pauses: AtomicUsize,
}

impl Link for Wire {
    fn send(
        &self,
        call: RemoteCall,
    ) -> impl std::future::Future<Output = Result<RemoteAnswer, CallError>> + Send {
        let phone = self.phone.clone();
        async move { phone.call(call).await }
    }

    fn pause(&self, how_long: Duration) -> impl std::future::Future<Output = ()> + Send {
        self.pauses.fetch_add(1, Ordering::SeqCst);
        self.mono.advance(how_long);
        async {}
    }
}

fn entry(id: &str, title: &str, day: &str, calendar: &str) -> CalendarEntry {
    CalendarEntry {
        phone_id: id.into(),
        calendar_id: calendar.into(),
        title: title.into(),
        start: format!("{day}T09:00:00Z"),
        end: format!("{day}T09:30:00Z"),
        time_zone: Some("Europe/London".into()),
        all_day: false,
        place: None,
        people: vec!["Sam".into()],
        repeats: false,
        notes: Some("Dial-in: 0123".into()),
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

const TAKES: [PhoneFeature; 2] = [PhoneFeature::Calendar, PhoneFeature::Health];

fn bring_both(s: &Syncer) {
    s.ledger.update(|l| {
        l.calendar.settings.bring = true;
        l.health.settings.bring = true;
    });
}

#[test]
fn the_phone_sends_what_changed_and_the_computer_keeps_it() {
    let c = computer(0);
    let (phone, device) = c.phone();
    let dir = tempfile::tempdir().unwrap();
    let s = Syncer::new(LedgerFile::open(dir.path()));
    bring_both(&s);
    s.ledger
        .update(|l| l.calendar.settings.calendars.insert("2".into(), false));
    let stand = Stand::new();
    *stand.entries.lock().unwrap() = vec![
        entry("dentist", "Dentist", "2026-11-13", "1"),
        entry("swim", "Swim club", "2026-11-14", "1"),
        entry("bank-holiday", "Bank holiday", "2026-11-16", "2"),
    ];
    *stand.readings.lock().unwrap() = vec![steps("2026-11-12", 8412), steps("2026-11-13", 900)];
    let wire = Wire {
        phone: phone.clone(),
        mono: c.mono.clone(),
        pauses: AtomicUsize::new(0),
    };
    let report = block(s.sync(&stand, &wire, &TAKES, "now".into())).unwrap();
    assert_eq!(
        report.calendar.unwrap().taken,
        2,
        "a calendar turned off stays home"
    );
    assert_eq!(report.health.unwrap().taken, 2);

    let e = c.h.engine();
    let today: Vec<String> = e
        .calendar_today("2026-11-13".parse().unwrap())
        .unwrap()
        .0
        .into_iter()
        .map(|l| l.title)
        .collect();
    assert_eq!(today, vec!["Dentist".to_string()]);
    let view = e.phone_sources().unwrap();
    let mine = view.phones.iter().find(|p| p.device_id == device).unwrap();
    let cal = mine.calendar.as_ref().unwrap();
    assert_eq!(cal.calendars, 1, "only the calendars brought");
    assert_eq!(
        mine.health.as_ref().unwrap().kinds,
        vec![ReadingKind::Steps]
    );
    let came = c.came_in(&device, "calendar");
    assert_eq!(came.len(), 2);

    // Nothing changed: nothing goes, and the computer is not asked.
    let again = block(s.sync(&stand, &wire, &TAKES, "now".into())).unwrap();
    assert_eq!((again.calendar, again.health), (None, None));

    // The swim is cancelled on the phone, the dentist moves to Sunday, and today's steps grow.
    // A one-off keeps its id when it moves, as the plugin gives it (from its event alone).
    {
        let mut entries = stand.entries.lock().unwrap();
        entries.retain(|e| e.phone_id != "swim");
        entries[0].start = "2026-11-15T10:00:00Z".into();
        entries[0].end = "2026-11-15T10:30:00Z".into();
    }
    *stand.changes.lock().unwrap() = Some(Changes {
        readings: vec![steps("2026-11-13", 4100)],
        token: Some("t2".into()),
        ..Changes::default()
    });
    let report = block(s.sync(&stand, &wire, &TAKES, "now".into())).unwrap();
    assert_eq!(report.calendar.unwrap().taken, 2);
    assert_eq!(report.health.unwrap().taken, 1);
    let came = c.came_in(&device, "calendar");
    let dentist: Vec<CameInTag> = came
        .iter()
        .filter(|r| r.title == "Dentist")
        .map(|r| r.tag)
        .collect();
    // The test clock stands still, so both versions came in at the same moment.
    assert_eq!(dentist.len(), 2);
    assert!(
        dentist.contains(&CameInTag::Changed),
        "a move is a change, never removed and new"
    );
    assert!(!dentist.contains(&CameInTag::Removed));
    let removed: Vec<&str> = came
        .iter()
        .filter(|r| r.tag == CameInTag::Removed)
        .map(|r| r.title.as_str())
        .collect();
    assert_eq!(removed, vec!["Swim club"]);
    let on = |day: &str| -> Vec<String> {
        e.calendar_today(day.parse().unwrap())
            .unwrap()
            .0
            .into_iter()
            .map(|l| l.title)
            .collect()
    };
    assert!(on("2026-11-13").is_empty());
    assert_eq!(on("2026-11-15"), vec!["Dentist".to_string()]);
    assert_eq!(
        s.sources(&stand, &TAKES)
            .last_sent
            .unwrap()
            .calendar_changes,
        2
    );
}

#[test]
fn a_large_send_goes_in_batches_and_waits_out_the_limit() {
    let c = computer(0);
    let (phone, device) = c.phone();
    let dir = tempfile::tempdir().unwrap();
    let s = Syncer::new(LedgerFile::open(dir.path()));
    bring_both(&s);
    let stand = Stand::new();
    *stand.entries.lock().unwrap() = (0..120)
        .map(|i| {
            entry(
                &format!("e{i:03}"),
                &format!("Meeting {i}"),
                "2026-11-20",
                "1",
            )
        })
        .collect();
    // 200 days of heart rate, full: more batches than the computer takes in a minute.
    *stand.readings.lock().unwrap() = (0..200).map(heart_day).collect();
    let wire = Wire {
        phone,
        mono: c.mono.clone(),
        pauses: AtomicUsize::new(0),
    };
    let report = block(s.sync(&stand, &wire, &TAKES, "now".into())).unwrap();
    assert_eq!(report.calendar.unwrap().taken, 120);
    assert_eq!(report.health.unwrap().taken, 200);
    let view = c.h.engine().phone_sources().unwrap();
    let mine = view.phones.iter().find(|p| p.device_id == device).unwrap();
    assert_eq!(mine.calendar.as_ref().unwrap().upcoming, 120);
    let l = s.ledger.get();
    assert!(l.calendar.pending.is_empty() && l.health.pending.is_empty());
    assert!(
        wire.pauses.load(Ordering::SeqCst) > 0,
        "the computer said wait"
    );
}

fn heart_day(n: u32) -> Reading {
    let day = jiff::civil::date(2026, 11, 13)
        .checked_sub(jiff::Span::new().days(i64::from(n)))
        .unwrap()
        .to_string();
    Reading {
        phone_id: day.clone(),
        day: day.clone(),
        start: format!("{day}T00:00:00Z"),
        end: None,
        source_device: Some("Watch".into()),
        value: ReadingValue::HeartRate {
            hours: (0..48)
                .map(|h| nocturne_contract::HourRange {
                    start: format!("{day}T{:02}:{:02}:00Z", h / 2, (h % 2) * 30),
                    low: 52.0,
                    high: 148.0,
                    middle: 74.0,
                })
                .collect(),
            count: 400,
        },
    }
}

#[test]
fn stopped_on_the_computer_waits_until_it_is_back() {
    let c = computer(0);
    let (phone, device) = c.phone();
    let dir = tempfile::tempdir().unwrap();
    let s = Syncer::new(LedgerFile::open(dir.path()));
    bring_both(&s);
    let stand = Stand::new();
    *stand.entries.lock().unwrap() = vec![entry("dentist", "Dentist", "2026-11-13", "1")];
    let wire = Wire {
        phone,
        mono: c.mono.clone(),
        pauses: AtomicUsize::new(0),
    };
    // The calendar has to have been offered before it can be stopped.
    block(s.sync(&stand, &wire, &TAKES, "now".into())).unwrap();
    c.h.ok(
        "set_phone_source",
        json!({"deviceId": device, "source": "calendar", "taking": false}),
    );
    stand.entries.lock().unwrap()[0].title = "Dentist, moved".into();
    let report = block(s.sync(&stand, &wire, &TAKES, "now".into())).unwrap();
    assert!(report.calendar.unwrap().stopped);
    assert_eq!(
        s.sources(&stand, &TAKES).calendar,
        SourceStatus::StoppedOnComputer
    );
    c.h.ok(
        "set_phone_source",
        json!({"deviceId": device, "source": "calendar", "taking": true}),
    );
    let report = block(s.sync(&stand, &wire, &TAKES, "now".into())).unwrap();
    assert_eq!(report.calendar.unwrap().taken, 1);
    assert_eq!(s.sources(&stand, &TAKES).calendar, SourceStatus::On);
}

/// A computer on stand-ins for a debug phone app behind `adb reverse`, with a fresh data folder:
/// it allows the pairing once asked, then prints how many calendar entries and readings came in
/// from the phone, never what they say.
#[test]
#[ignore = "runs a computer for a phone; see docs/contribution/building.md"]
fn device_bench() {
    let port: u16 = std::env::var("NOCTURNE_PHONE_BENCH_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8799);
    let secs: u64 = std::env::var("NOCTURNE_PHONE_BENCH_SECS")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(900);
    let c = computer(port);
    let offer = c.h.ok("begin_pairing", json!({}));
    println!("bench on 127.0.0.1:{}", c.port);
    println!("link: {}", offer["link"].as_str().unwrap());
    println!("address: {}", testing::TEST_HOST);
    println!("code: {}", offer["typedCode"].as_str().unwrap());
    let until = std::time::Instant::now() + Duration::from_secs(secs);
    let mut allowed = false;
    let mut last = (usize::MAX, usize::MAX, usize::MAX);
    while std::time::Instant::now() < until {
        if !allowed && c.waiting_for_you() {
            std::thread::sleep(Duration::from_secs(8));
            c.h.ok(
                "decide_pairing",
                json!({"pairingId": offer["pairingId"], "allow": true}),
            );
            allowed = true;
            println!("pairing allowed");
        }
        if allowed && let Some(d) = c.h.engine().devices(false).unwrap().first() {
            let calendar = c.came_in(&d.id, "calendar");
            let removed = calendar
                .iter()
                .filter(|r| r.tag == CameInTag::Removed)
                .count();
            let health = c.came_in(&d.id, "health").len();
            if (calendar.len(), removed, health) != last {
                last = (calendar.len(), removed, health);
                println!(
                    "came in: {} calendar ({removed} removed), {health} health",
                    calendar.len()
                );
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}
