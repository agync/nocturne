//! A phone app's calendar and health sends, end to end: a Rust phone over the real listener, a
//! fake helper and the real app on the mock runtime.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use nocturne_app_lib::remote::controller::RemoteHub;
use nocturne_app_lib::remote::limits::ManualMono;
use nocturne_app_lib::remote::protocol::*;
use nocturne_app_lib::remote::testing::client::{Addr, Mode, Opened, Pairing, PhoneClient};
use nocturne_app_lib::remote::testing::{self, FakeHelpers, PhoneKeys, PhonePair};
use nocturne_contract::{
    CalendarBatch, CalendarChange, CalendarEntry, PhoneCalendar, PhoneError, PhoneFeature,
    PhoneSource, Reading, ReadingBatch, ReadingChange, ReadingValue, RemoteAction, RemoteAnswer,
    RemoteCall, RemoteState, SyncReceipt, TimeSpan,
};
use nocturne_engine::FakeMind;
use nocturne_helpers::{HelperKind, Origin};
use nocturne_intelligence::FakeHost;
use serde_json::json;

struct Phone {
    h: Harness,
    hub: Arc<RemoteHub>,
    #[allow(dead_code)]
    helpers: Arc<FakeHelpers>,
    mono: Arc<ManualMono>,
    _files: tempfile::TempDir,
}

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

fn on() -> Phone {
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
    Phone {
        h,
        hub,
        helpers,
        mono,
        _files: files,
    }
}

impl Phone {
    fn addr(&self) -> Addr {
        match self.hub.listener_origin().unwrap() {
            Origin::Unix(p) => Addr::Unix(p),
            Origin::Tcp { port, key } => Addr::Keyed { port, host: key },
        }
    }

    fn pair(&self, name: &str) -> PhoneKeys {
        let offer = self.h.ok("begin_pairing", json!({}));
        let run = block(Pairing::start(
            &self.addr(),
            PhonePair::scanned(offer["link"].as_str().unwrap(), name),
            Mode::WebSocket,
        ))
        .unwrap();
        wait_for("asked", || {
            matches!(
                block(self.hub.view()).pairing.map(|p| p.phase),
                Some(nocturne_contract::PairingPhase::WaitingForYou { .. })
            )
        });
        self.h.ok(
            "decide_pairing",
            json!({"pairingId": offer["pairingId"], "allow": true}),
        );
        block(run.outcome()).unwrap().unwrap()
    }

    /// A paired phone's session, the computer's hello and the phone's id here.
    fn session(&self, name: &str) -> (Box<PhoneClient>, Vec<PhoneFeature>, String) {
        let keys = self.pair(name);
        let (client, hello) = match block(PhoneClient::open(&self.addr(), &keys, Mode::WebSocket)) {
            Opened::Session(c, hello) => (c, hello),
            other => panic!("no session: {other:?}"),
        };
        let id = self
            .h
            .engine()
            .devices(false)
            .unwrap()
            .into_iter()
            .find(|d| d.name == name)
            .unwrap()
            .id;
        (client, hello.features, id)
    }
}

fn call(c: &mut PhoneClient, call: RemoteCall) -> RemoteAnswer {
    block(c.call(call)).unwrap()
}

fn receipt(a: &RemoteAnswer) -> SyncReceipt {
    match a {
        RemoteAnswer::Synced { receipt } => *receipt,
        other => panic!("not a receipt: {other:?}"),
    }
}

fn error_of(a: &RemoteAnswer) -> Option<PhoneError> {
    match a {
        RemoteAnswer::Error { code } => Some(*code),
        _ => None,
    }
}

/// An appointment on the harness's day, Friday 13 November 2026.
fn entry(phone_id: &str, title: &str, hour: u8) -> CalendarEntry {
    CalendarEntry {
        phone_id: phone_id.into(),
        calendar_id: "c1".into(),
        title: title.into(),
        start: format!("2026-11-13T{hour:02}:00:00Z"),
        end: format!("2026-11-13T{hour:02}:30:00Z"),
        time_zone: None,
        all_day: false,
        place: None,
        people: Vec::new(),
        repeats: false,
        notes: None,
        repeat_rule: None,
    }
}

fn send_calendar(sync: &str, entries: Vec<CalendarEntry>) -> RemoteCall {
    RemoteCall::SendCalendar {
        batch: CalendarBatch {
            sync_id: sync.into(),
            seq: 0,
            last: true,
            window_from: "2026-11-11".into(),
            window_to: "2026-11-27".into(),
            calendars: vec![PhoneCalendar {
                id: "c1".into(),
                name: "Home".into(),
                colour: None,
            }],
            changes: entries
                .into_iter()
                .map(|entry| CalendarChange::Upsert { entry })
                .collect(),
        },
    }
}

#[test]
fn the_computer_says_it_takes_calendar_and_health() {
    let p = on();
    let (_, features, _) = p.session("Pixel");
    assert_eq!(features, vec![PhoneFeature::Calendar, PhoneFeature::Health]);
}

#[test]
fn a_calendar_send_is_kept_and_on_the_phones_page() {
    let p = on();
    let (mut c, _, device) = p.session("Pixel");
    let r = receipt(&call(
        &mut c,
        send_calendar("s1", vec![entry("ev1", "Dentist", 9)]),
    ));
    assert_eq!((r.taken, r.stopped), (1, false));
    let r = receipt(&call(
        &mut c,
        send_calendar("s2", vec![entry("ev1", "Dentist", 9)]),
    ));
    assert_eq!((r.taken, r.unchanged), (0, 1), "a resend is kept once");
    let RemoteAnswer::Page { page } = call(&mut c, RemoteCall::GetPage { date: None }) else {
        panic!("no page");
    };
    assert_eq!(page.calendar_today.len(), 1);
    assert_eq!(page.calendar_today[0].phone, "Pixel");
    let events = p.h.engine().remote_events(10).unwrap();
    let sent = events
        .iter()
        .find(|e| e.action == RemoteAction::SentCalendar)
        .expect("noted");
    assert_eq!(sent.count, 1, "counts only");
    let view = p.h.engine().phone_sources().unwrap();
    let phone = view
        .phones
        .iter()
        .find(|ph| ph.device_id == device)
        .unwrap();
    assert!(phone.calendar.as_ref().unwrap().taking);
}

#[test]
fn a_private_calendar_entry_never_reaches_the_phone() {
    let p = on();
    let (mut c, _, _) = p.session("Pixel");
    call(
        &mut c,
        send_calendar(
            "s1",
            vec![entry("ev1", "Lunch", 12), entry("ev2", "Clinic", 15)],
        ),
    );
    let e = p.h.engine();
    let secret = e
        .calendar_today("2026-11-13".parse().unwrap())
        .unwrap()
        .0
        .into_iter()
        .find(|l| l.title == "Clinic")
        .unwrap()
        .item_id;
    e.set_item_private(&secret, true).unwrap();
    for saved in [false, true] {
        if saved {
            p.h.save_todays_page();
        }
        let answer = call(&mut c, RemoteCall::GetPage { date: None });
        let RemoteAnswer::Page { page } = &answer else {
            panic!("the page still arrives: {answer:?}");
        };
        let titles: Vec<&str> = page
            .calendar_today
            .iter()
            .map(|l| l.title.as_str())
            .collect();
        assert_eq!(titles, vec!["Lunch"]);
        assert_eq!(page.more_calendar_today, 0);
        let text = serde_json::to_string(&answer).unwrap();
        assert!(!text.contains(&secret) && !text.contains("Clinic"));
    }
    let RemoteAnswer::JournalDay { day } = call(
        &mut c,
        RemoteCall::GetJournalDay {
            date: Some("2026-11-13".into()),
        },
    ) else {
        panic!("no day");
    };
    let text = serde_json::to_string(&day).unwrap();
    assert!(!text.contains(&secret) && !text.contains("Clinic"));
    assert!(text.contains("Lunch"), "the public one is a calendar row");
    assert_eq!(
        error_of(&call(&mut c, RemoteCall::GetItem { id: secret })),
        Some(PhoneError::NotFound)
    );
}

#[test]
fn a_stopped_source_answers_stopped() {
    let p = on();
    let (mut c, _, device) = p.session("Pixel");
    p.h.ok(
        "set_phone_source",
        json!({"deviceId": device, "source": "health", "taking": false}),
    );
    let r = receipt(&call(
        &mut c,
        RemoteCall::SendReadings {
            batch: ReadingBatch {
                sync_id: "h1".into(),
                seq: 0,
                last: true,
                changes: vec![ReadingChange::Upsert {
                    reading: Reading {
                        phone_id: "n1".into(),
                        day: "2026-11-13".into(),
                        start: "2026-11-12T23:00:00Z".into(),
                        end: None,
                        source_device: None,
                        value: ReadingValue::Sleep {
                            asleep: vec![TimeSpan {
                                from: "2026-11-12T23:00:00Z".into(),
                                to: "2026-11-13T06:00:00Z".into(),
                            }],
                            in_bed_only: false,
                        },
                    },
                }],
            },
        },
    ));
    assert!(r.stopped);
    assert_eq!(r.taken, 0);
    assert!(
        p.h.engine()
            .last_night("2026-11-13".parse().unwrap())
            .unwrap()
            .is_none()
    );
    assert!(
        p.h.engine()
            .source_taking(&device, PhoneSource::Calendar)
            .unwrap(),
        "only that source"
    );
}

#[test]
fn sends_are_limited_and_checked() {
    let p = on();
    let (mut c, _, _) = p.session("Pixel");
    let big: Vec<CalendarEntry> = (0..=MAX_SYNC_BATCH)
        .map(|i| entry(&format!("e{i}"), "x", 10))
        .collect();
    assert_eq!(
        error_of(&call(&mut c, send_calendar("big", big))),
        Some(PhoneError::TooLarge)
    );
    let mut long = entry("long", "x", 10);
    long.notes = Some("n".repeat(MAX_SYNC_JSON));
    assert_eq!(
        error_of(&call(&mut c, send_calendar("long", vec![long]))),
        Some(PhoneError::TooLarge)
    );
    assert_eq!(
        error_of(&call(&mut c, send_calendar("not an id!", Vec::new()))),
        Some(PhoneError::Invalid)
    );
    // The two refused for size and the one refused as invalid took no token.
    let mut limited = 0;
    for i in 0..LIMIT_SYNCS + 2 {
        let a = call(&mut c, send_calendar(&format!("s{i}"), Vec::new()));
        if error_of(&a) == Some(PhoneError::TooMany) {
            limited += 1;
        }
    }
    assert_eq!(limited, 2);
    p.mono.advance(Duration::from_secs(60));
    assert!(matches!(
        call(&mut c, send_calendar("later", Vec::new())),
        RemoteAnswer::Synced { .. }
    ));
}
