mod common;

use common::{cand, fixture, keep, learn};
use jiff::Timestamp;
use nocturne_contract::{AutoOff, RemoteAction, RemoteConnection, RemoteResult};
use nocturne_engine::{Engine, Predicate};

const KEY: &[u8] = &[7, 1, 9, 4, 4, 250, 3, 3, 8];

fn pair(engine: &Engine, name: &str, origin: &str) -> String {
    engine
        .add_device(name, KEY, origin, RemoteConnection::Simple, "addr-a")
        .unwrap()
        .id
}

fn ts(s: &str) -> Timestamp {
    s.parse().unwrap()
}

#[test]
fn devices_round_trip() {
    let f = fixture();
    let d = f
        .engine
        .add_device(
            "Phone A",
            KEY,
            "https://home.example",
            RemoteConnection::MostPrivate,
            "addr-a",
        )
        .unwrap();
    assert_eq!(d.id.len(), 22);
    assert!(
        d.id.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    );
    assert_eq!(d.name, "Phone A");
    assert_eq!(d.public_key, KEY);
    assert_eq!(d.connection, RemoteConnection::MostPrivate);
    assert_eq!(d.address.as_deref(), Some("addr-a"));
    assert_eq!(d.paired_at, ts("2026-11-13T07:00:00Z"));
    assert_eq!(d.last_used_at, None);
    assert_eq!(f.engine.device(&d.id).unwrap(), Some(d.clone()));
    assert_eq!(f.engine.device("missing").unwrap(), None);

    f.clock.set_str("2026-11-13T07:30:00Z");
    let other = pair(&f.engine, "Phone B", "https://home.example");
    assert_ne!(other, d.id);

    f.clock.set_str("2026-11-13T08:00:00Z");
    f.engine.touch_device(&d.id, "addr-b").unwrap();
    let touched = f.engine.device(&d.id).unwrap().unwrap();
    assert_eq!(touched.last_used_at, Some(ts("2026-11-13T08:00:00Z")));
    assert_eq!(touched.address.as_deref(), Some("addr-b"));

    let renamed = f.engine.rename_device(&d.id, "Phone C").unwrap();
    assert_eq!(renamed.name, "Phone C");
    assert!(f.engine.rename_device("missing", "x").is_err());

    assert!(f.engine.revoke_device(&d.id).unwrap());
    assert!(!f.engine.revoke_device(&d.id).unwrap());
    assert!(!f.engine.revoke_device("missing").unwrap());
    assert!(f.engine.rename_device(&d.id, "again").is_err());
    f.engine.touch_device(&d.id, "addr-c").unwrap();
    let revoked = f.engine.device(&d.id).unwrap().unwrap();
    assert_eq!(revoked.revoked_at, Some(ts("2026-11-13T08:00:00Z")));
    assert_eq!(revoked.address.as_deref(), Some("addr-b"));

    assert_eq!(f.engine.devices(false).unwrap().len(), 1);
    let all = f.engine.devices(true).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, d.id);
    assert_eq!(f.engine.revoke_all_devices().unwrap(), 1);
    assert!(f.engine.devices(false).unwrap().is_empty());
}

#[test]
fn home_origins_only_unrevoked() {
    let f = fixture();
    let a = pair(&f.engine, "A", "https://one.example");
    pair(&f.engine, "B", "https://one.example");
    pair(&f.engine, "C", "https://two.example");
    assert_eq!(
        f.engine.home_origins().unwrap(),
        ["https://one.example", "https://two.example"]
    );
    f.engine.revoke_device(&a).unwrap();
    assert_eq!(f.engine.home_origins().unwrap().len(), 2);
    f.engine.revoke_all_devices().unwrap();
    assert!(f.engine.home_origins().unwrap().is_empty());
}

#[test]
fn revoked_device_keeps_its_name_in_receipts() {
    let f = fixture();
    let device = pair(&f.engine, "Phone A", "https://home.example");
    let (item, memory) = learn(
        &f.engine,
        "Dentist on Thursday at 10:00.",
        cand(
            "dentist",
            Predicate::Happened,
            "Thursday 10:00",
            "Your dentist is Thursday.",
        ),
    );
    f.engine.link_remote_keep(&item, &device, "c1").unwrap();
    let receipts = f.engine.get_memory(&memory).unwrap().receipts;
    assert_eq!(receipts[0].from_device.as_deref(), Some("Phone A"));

    f.engine.revoke_device(&device).unwrap();
    let receipts = f.engine.get_memory(&memory).unwrap().receipts;
    assert_eq!(receipts[0].from_device.as_deref(), Some("Phone A"));
}

#[test]
fn remote_keep_dedupes_by_client_id() {
    let f = fixture();
    let a = pair(&f.engine, "A", "https://home.example");
    let b = pair(&f.engine, "B", "https://home.example");
    let item = keep(&f.engine, "first");
    let second = keep(&f.engine, "second");
    assert_eq!(f.engine.remote_keep(&a, "c1").unwrap(), None);
    f.engine.link_remote_keep(&item, &a, "c1").unwrap();
    f.engine.link_remote_keep(&second, &a, "c1").unwrap();
    assert_eq!(f.engine.remote_keep(&a, "c1").unwrap(), Some(item.clone()));
    assert_eq!(f.engine.remote_keep(&b, "c1").unwrap(), None);
    assert_eq!(f.engine.remote_keep(&a, "c2").unwrap(), None);
    assert_eq!(f.engine.remote_keep_owner(&second).unwrap(), None);
    assert_eq!(
        f.engine.remote_keep_client(&item).unwrap().as_deref(),
        Some("c1")
    );
    assert_eq!(f.engine.remote_keep_client(&second).unwrap(), None);
    assert_eq!(
        f.engine.remote_keep_owner(&item).unwrap(),
        Some((a, ts("2026-11-13T07:00:00Z")))
    );
}

#[test]
fn unkeep_cascades_remote_keep() {
    let f = fixture();
    let device = pair(&f.engine, "A", "https://home.example");
    let item = keep(&f.engine, "take me back");
    f.engine.link_remote_keep(&item, &device, "c1").unwrap();
    f.engine.unkeep(&item).unwrap();
    assert_eq!(
        common::sql_count(f.dir.path(), "SELECT COUNT(*) FROM remote_keeps"),
        0
    );
    assert_eq!(f.engine.remote_keep(&device, "c1").unwrap(), None);
    assert!(f.engine.device(&device).unwrap().is_some());
}

#[test]
fn events_collapse_repeats_into_count() {
    let f = fixture();
    let a = pair(&f.engine, "A", "https://home.example");
    let log = |action, result| {
        f.engine
            .log_remote_event(Some(&a), action, result, 10.0)
            .unwrap()
    };
    log(RemoteAction::Asked, RemoteResult::Ok);
    f.clock.set_str("2026-11-13T07:00:50Z");
    log(RemoteAction::Asked, RemoteResult::Ok);
    f.clock.set_str("2026-11-13T07:01:40Z");
    log(RemoteAction::Asked, RemoteResult::Ok);
    f.clock.set_str("2026-11-13T07:01:45Z");
    log(RemoteAction::Asked, RemoteResult::Failed);
    log(RemoteAction::Asked, RemoteResult::Failed);
    f.engine
        .log_remote_event(None, RemoteAction::Asked, RemoteResult::Failed, 0.0)
        .unwrap();

    let events = f.engine.remote_events(10).unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].device_name, None);
    assert_eq!(events[1].count, 2);
    assert_eq!(events[1].result, RemoteResult::Failed);
    assert_eq!(events[1].device_name.as_deref(), Some("A"));
    assert_eq!(events[2].count, 3);
    assert_eq!(events[2].at, "2026-11-13T07:01:40.000Z");
    assert_eq!(f.engine.remote_events(1).unwrap().len(), 1);
    assert_eq!(
        common::sql_count(
            f.dir.path(),
            "SELECT CAST(SUM(bytes) AS INTEGER) FROM remote_events"
        ),
        50
    );
}

#[test]
fn events_pruned_after_90_days() {
    let f = fixture();
    let a = pair(&f.engine, "A", "https://home.example");
    f.engine
        .log_remote_event(Some(&a), RemoteAction::Asked, RemoteResult::Ok, 0.0)
        .unwrap();
    f.clock.advance_days(60);
    f.engine
        .log_remote_event(Some(&a), RemoteAction::ReadPage, RemoteResult::Ok, 0.0)
        .unwrap();
    f.clock.advance_days(40);
    let cutoff = f.engine.clock_now() - jiff::SignedDuration::from_hours(90 * 24);
    assert_eq!(f.engine.prune_remote_events(cutoff).unwrap(), 1);
    let left = f.engine.remote_events(10).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].action, RemoteAction::ReadPage);
}

#[test]
fn item_view_says_kept_from_device() {
    let f = fixture();
    let device = pair(&f.engine, "Phone A", "https://home.example");
    let from_phone = keep(&f.engine, "from the phone");
    let local = keep(&f.engine, "at the desk");
    f.engine
        .link_remote_keep(&from_phone, &device, "c1")
        .unwrap();
    let items = f.engine.items(Default::default()).unwrap();
    let by_id = |id: &str| items.iter().find(|i| i.id == id).unwrap();
    assert_eq!(by_id(&from_phone).from_device.as_deref(), Some("Phone A"));
    assert_eq!(by_id(&local).from_device, None);
    f.engine.rename_device(&device, "Phone B").unwrap();
    f.engine.revoke_device(&device).unwrap();
    let items = f.engine.items(Default::default()).unwrap();
    assert_eq!(
        items
            .iter()
            .find(|i| i.id == from_phone)
            .unwrap()
            .from_device
            .as_deref(),
        Some("Phone B")
    );
}

#[test]
fn delete_everything_clears_remote_rows() {
    let f = fixture();
    let device = pair(&f.engine, "A", "https://home.example");
    let item = keep(&f.engine, "kept from a phone");
    f.engine.link_remote_keep(&item, &device, "c1").unwrap();
    f.engine
        .log_remote_event(Some(&device), RemoteAction::KeptText, RemoteResult::Ok, 5.0)
        .unwrap();
    f.engine
        .update_remote_state(|s| {
            s.connection = RemoteConnection::MostPrivate;
            s.wanted_on = true;
            s.keep_awake = true;
            s.auto_off = AutoOff::Never;
            s.machine_name = Some("desk".into());
        })
        .unwrap();
    f.engine.add_sent_to_phones(99).unwrap();

    f.engine
        .delete_everything(nocturne_engine::DELETE_PHRASE)
        .unwrap();

    let dir = f.dir.path();
    for table in ["remote_devices", "remote_keeps", "remote_events"] {
        assert_eq!(
            common::sql_count(dir, &format!("SELECT COUNT(*) FROM {table}")),
            0,
            "{table}"
        );
    }
    let state = f.engine.remote_state().unwrap();
    assert_eq!(state.connection, RemoteConnection::Simple);
    assert!(!state.wanted_on);
    assert!(!state.keep_awake);
    assert_eq!(state.auto_off, AutoOff::Unused30Days);
    assert_eq!(state.sent_bytes, 0.0);
    assert_eq!(state.machine_name, None);
}

#[test]
fn remote_events_hold_no_content() {
    let f = fixture();
    let marker = "zebra-marker-4471";
    let device = pair(&f.engine, "A", "https://home.example");
    let item = keep(&f.engine, &format!("note about {marker}"));
    f.engine.link_remote_keep(&item, &device, marker).unwrap();
    f.engine
        .log_remote_event(
            Some(&device),
            RemoteAction::KeptText,
            RemoteResult::Ok,
            22.0,
        )
        .unwrap();
    f.engine
        .log_remote_event(
            Some(&device),
            RemoteAction::Asked,
            RemoteResult::Refused,
            0.0,
        )
        .unwrap();

    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    let mut stmt = conn.prepare("SELECT * FROM remote_events").unwrap();
    let cols = stmt.column_count();
    let mut rows = stmt.query([]).unwrap();
    let mut seen = 0;
    while let Some(row) = rows.next().unwrap() {
        seen += 1;
        for i in 0..cols {
            let bytes: Vec<u8> = match row.get_ref(i).unwrap() {
                rusqlite::types::ValueRef::Text(t) | rusqlite::types::ValueRef::Blob(t) => {
                    t.to_vec()
                }
                other => format!("{other:?}").into_bytes(),
            };
            assert!(!bytes.windows(marker.len()).any(|w| w == marker.as_bytes()));
            assert!(!bytes.windows(KEY.len()).any(|w| w == KEY));
        }
    }
    assert_eq!(seen, 2);
}

#[test]
fn update_remote_state_round_trips_every_field() {
    let f = fixture();
    let defaults = f.engine.remote_state().unwrap();
    assert_eq!(defaults.connection, RemoteConnection::Simple);
    assert_eq!(defaults.auto_off, AutoOff::Unused30Days);
    let t = |s| Some(ts(s));
    let written = f
        .engine
        .update_remote_state(|s| {
            s.connection = RemoteConnection::MostPrivate;
            s.wanted_on = true;
            s.auto_off = AutoOff::AfterWeek;
            s.keep_awake = true;
            s.sent_bytes = 1234.5;
            s.first_on_at = t("2026-11-01T00:00:00Z");
            s.on_since = t("2026-11-02T00:00:00Z");
            s.last_on_at = t("2026-11-03T00:00:00Z");
            s.last_off_at = t("2026-11-04T00:00:00Z");
            s.address = Some("addr-z".into());
            s.address_since = t("2026-11-05T00:00:00Z");
            s.machine_name = Some("desk".into());
        })
        .unwrap();
    assert_eq!(f.engine.remote_state().unwrap(), written);
    assert_eq!(written.sent_bytes, 1234.5);
    assert_eq!(written.machine_name.as_deref(), Some("desk"));
    f.engine.add_sent_to_phones(10).unwrap();
    assert_eq!(f.engine.remote_state().unwrap().sent_bytes, 1244.5);
}
