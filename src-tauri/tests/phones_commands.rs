//! From your phones through the window commands: what phones brought, the page options,
//! stopping a source, the health pages, deleting what came in and removing a phone with it.

mod common;

use common::*;
use nocturne_app_lib::remote::testing;
use nocturne_contract::{
    CalendarBatch, CalendarChange, CalendarEntry, PhoneDeletePreview, PhoneSourcesView, Reading,
    ReadingBatch, ReadingChange, ReadingValue, RemoteConnection,
};
use nocturne_intelligence::FakeHost;
use serde_json::json;

fn paired(h: &Harness) -> String {
    h.engine()
        .add_device(
            "Pixel",
            &[7; 32],
            "https://example.test",
            RemoteConnection::Simple,
            "example.test",
        )
        .unwrap()
        .id
}

fn bring(h: &Harness, device: &str) {
    let e = h.engine();
    e.take_calendar(
        device,
        CalendarBatch {
            sync_id: "s1".into(),
            seq: 0,
            last: true,
            window_from: "2026-11-11".into(),
            window_to: "2026-11-27".into(),
            calendars: Vec::new(),
            changes: vec![CalendarChange::Upsert {
                entry: CalendarEntry {
                    phone_id: "ev1".into(),
                    calendar_id: "c1".into(),
                    title: "Dentist".into(),
                    start: "2026-11-16T09:00:00Z".into(),
                    end: "2026-11-16T09:30:00Z".into(),
                    time_zone: None,
                    all_day: false,
                    place: None,
                    people: Vec::new(),
                    repeats: false,
                    notes: None,
                },
            }],
        },
    )
    .unwrap();
    e.take_readings(
        device,
        ReadingBatch {
            sync_id: "h1".into(),
            seq: 0,
            last: true,
            changes: vec![ReadingChange::Upsert {
                reading: Reading {
                    phone_id: "st1".into(),
                    day: "2026-11-12".into(),
                    start: "2026-11-12T00:00:00Z".into(),
                    end: None,
                    source_device: None,
                    value: ReadingValue::Steps { total: 8412 },
                },
            }],
        },
    )
    .unwrap();
}

#[test]
fn from_your_phones_lists_what_each_phone_brings() {
    let h = harness();
    let device = paired(&h);
    let v: PhoneSourcesView = serde_json::from_value(h.ok("get_phone_sources", json!({}))).unwrap();
    assert!(v.phones[0].in_browser);
    bring(&h, &device);
    let v: PhoneSourcesView = serde_json::from_value(h.ok("get_phone_sources", json!({}))).unwrap();
    let phone = &v.phones[0];
    assert!(!phone.in_browser);
    assert_eq!(phone.calendar.as_ref().unwrap().upcoming, 1);
    assert_eq!(phone.health.as_ref().unwrap().kinds.len(), 1);

    let v: PhoneSourcesView = serde_json::from_value(h.ok(
        "set_phone_source",
        json!({"deviceId": device, "source": "calendar", "taking": false}),
    ))
    .unwrap();
    assert!(!v.phones[0].calendar.as_ref().unwrap().taking);
    let v: PhoneSourcesView = serde_json::from_value(h.ok(
        "set_phone_page_options",
        json!({"options": {"calendarToday": false, "sleepLastNight": true}}),
    ))
    .unwrap();
    assert!(!v.page.calendar_today);
    h.ok("set_noticing", json!({"on": false}));
    let v: PhoneSourcesView = serde_json::from_value(h.ok("get_phone_sources", json!({}))).unwrap();
    assert!(!v.noticing);
    assert_eq!(
        h.code(
            "set_phone_source",
            json!({"deviceId": "nobody", "source": "calendar", "taking": false})
        ),
        "notFound"
    );
}

#[test]
fn what_came_in_and_the_health_pages() {
    let h = harness();
    let device = paired(&h);
    bring(&h, &device);
    let rows = h.ok(
        "list_came_in",
        json!({"deviceId": device, "source": "calendar", "before": null, "limit": 10}),
    );
    assert_eq!(rows[0]["title"], "Dentist");
    assert_eq!(rows[0]["tag"], "new");
    let series = h.ok(
        "get_health_series",
        json!({"kind": "steps", "from": "2026-11-01", "to": "2026-11-13"}),
    );
    assert_eq!(series["points"][0]["value"], 8412.0);
    assert!(series["usual"].is_null());
    let list = h.ok(
        "list_readings",
        json!({"kind": "steps", "from": "2026-11-01", "to": "2026-11-13"}),
    );
    assert_eq!(list[0]["value"]["total"], 8412);
    assert_eq!(
        h.code(
            "list_readings",
            json!({"kind": "steps", "from": "soon", "to": "2026-11-13"})
        ),
        "invalid"
    );
}

#[test]
fn delete_what_came_in_through_a_preview() {
    let h = harness();
    let device = paired(&h);
    bring(&h, &device);
    let p: PhoneDeletePreview = serde_json::from_value(h.ok(
        "preview_phone_delete",
        json!({"scope": {"deviceId": device, "source": "calendar", "kinds": null, "before": null}}),
    ))
    .unwrap();
    assert_eq!((p.items, p.memories), (1, 1));
    h.ok(
        "delete_phone_data",
        json!({"token": p.token, "alsoStop": true}),
    );
    assert!(
        h.engine()
            .calendar_today("2026-11-16".parse().unwrap())
            .unwrap()
            .0
            .is_empty()
    );
    assert_eq!(
        h.code(
            "delete_phone_data",
            json!({"token": p.token, "alsoStop": false})
        ),
        "tokenExpired"
    );
}

#[test]
fn removing_a_phone_keeps_or_deletes_what_it_brought() {
    let h = harness();
    let files = tempfile::tempdir().unwrap();
    let _fakes = testing::install_fakes(h.app.handle(), FakeHost::new(), files.path());
    let kept = paired(&h);
    bring(&h, &kept);
    h.ok("remove_device", json!({"id": kept}));
    assert_eq!(
        h.engine()
            .calendar_today("2026-11-16".parse().unwrap())
            .unwrap()
            .0
            .len(),
        1,
        "what it brought stays"
    );
    let v: PhoneSourcesView = serde_json::from_value(h.ok("get_phone_sources", json!({}))).unwrap();
    assert_eq!(
        v.phones.len(),
        1,
        "a removed phone that brought something is still listed"
    );
    h.engine()
        .preview_phone_delete(nocturne_contract::PhoneDeleteScope {
            device_id: kept.clone(),
            source: nocturne_contract::PhoneSource::Calendar,
            kinds: None,
            before: None,
        })
        .unwrap();

    let gone = h
        .engine()
        .add_device(
            "Tablet",
            &[8; 32],
            "https://example.test",
            RemoteConnection::Simple,
            "example.test",
        )
        .unwrap()
        .id;
    bring(&h, &gone);
    h.ok(
        "remove_device",
        json!({"id": gone, "alsoDeleteBrought": true}),
    );
    let lines = h
        .engine()
        .calendar_today("2026-11-16".parse().unwrap())
        .unwrap()
        .0;
    assert_eq!(lines.len(), 1, "only the first phone's stays");
    assert_eq!(lines[0].phone, "Pixel");
}
