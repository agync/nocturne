mod common;

use common::{fixture, keep};
use nocturne_contract::{RemoteAction, RemoteConnection, RemoteResult};

const KEY: &[u8] = &[7, 1, 9, 4, 4, 250, 3, 3, 8];

#[test]
fn an_arrival_takes_the_computers_pairings_and_the_phone_becomes_one_of_them() {
    let phone = fixture();
    let local = phone.engine.ensure_local_device("This phone").unwrap();
    let item = keep(&phone.engine, "Bought a ticket");
    phone.engine.link_remote_keep(&item, &local, "c1").unwrap();

    let computer = fixture();
    let paired = computer
        .engine
        .add_device(
            "Pixel",
            KEY,
            "https://home.example",
            RemoteConnection::Simple,
            "a",
        )
        .unwrap()
        .id;
    let other = computer
        .engine
        .add_device(
            "Tablet",
            KEY,
            "https://home.example",
            RemoteConnection::Simple,
            "b",
        )
        .unwrap()
        .id;
    computer
        .engine
        .update_remote_state(|s| {
            s.wanted_on = true;
            s.machine_name = Some("Desk".into());
        })
        .unwrap();
    computer
        .engine
        .log_remote_event(Some(&other), RemoteAction::KeptText, RemoteResult::Ok, 3.0)
        .unwrap();

    phone
        .engine
        .settle_arrival(computer.dir.path(), &paired)
        .unwrap();

    let e = &phone.engine;
    assert_eq!(e.local_device().unwrap(), None, "no own row is left");
    let mut names: Vec<String> = e
        .devices(false)
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    names.sort();
    assert_eq!(names, ["Pixel", "Tablet"]);
    assert_eq!(e.device(&paired).unwrap().unwrap().public_key, KEY);
    assert_eq!(e.remote_keep(&paired, "c1").unwrap(), Some(item.clone()));
    assert_eq!(
        e.get_item(&item).unwrap().from_device.as_deref(),
        Some("Pixel")
    );
    let state = e.remote_state().unwrap();
    assert!(state.wanted_on);
    assert_eq!(state.machine_name.as_deref(), Some("Desk"));
    assert_eq!(e.remote_events(10).unwrap().len(), 1);
    assert_eq!(
        computer.engine.devices(false).unwrap().len(),
        2,
        "the store in use is only read"
    );
}

#[test]
fn an_arrival_that_cannot_rehome_changes_nothing() {
    let phone = fixture();
    phone.engine.ensure_local_device("This phone").unwrap();
    let computer = fixture();
    computer
        .engine
        .update_remote_state(|s| s.machine_name = Some("Desk".into()))
        .unwrap();

    assert!(
        phone
            .engine
            .settle_arrival(computer.dir.path(), "missing")
            .is_err()
    );
    let e = &phone.engine;
    assert!(e.local_device().unwrap().is_some());
    assert_eq!(e.remote_state().unwrap().machine_name, None);
    assert!(e.devices(true).unwrap().is_empty());

    // The temporary copies went with the failure: a second try starts clean.
    let paired = computer
        .engine
        .add_device(
            "Pixel",
            KEY,
            "https://home.example",
            RemoteConnection::Simple,
            "a",
        )
        .unwrap()
        .id;
    e.settle_arrival(computer.dir.path(), &paired).unwrap();
    assert_eq!(e.devices(false).unwrap().len(), 1);
}

#[test]
fn pairings_the_arriving_store_carries_let_nobody_in() {
    let phone = fixture();
    phone.engine.ensure_local_device("This phone").unwrap();
    let stray = phone
        .engine
        .add_device(
            "Stray",
            KEY,
            "https://elsewhere.example",
            RemoteConnection::Simple,
            "x",
        )
        .unwrap()
        .id;
    let computer = fixture();
    let paired = computer
        .engine
        .add_device(
            "Pixel",
            KEY,
            "https://home.example",
            RemoteConnection::Simple,
            "a",
        )
        .unwrap()
        .id;
    phone
        .engine
        .settle_arrival(computer.dir.path(), &paired)
        .unwrap();
    let live: Vec<String> = phone
        .engine
        .devices(false)
        .unwrap()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(
        live,
        vec![paired.clone()],
        "only the computer's own pairings"
    );
    let all = phone.engine.devices(true).unwrap();
    let kept = all
        .iter()
        .find(|d| d.id == stray)
        .expect("kept for receipts");
    assert!(kept.public_key.is_empty());
}

#[test]
fn a_retake_brings_what_changed_on_the_computer_since() {
    let phone = fixture();
    phone.engine.ensure_local_device("This phone").unwrap();
    let computer = fixture();
    let add = |name: &str| {
        computer
            .engine
            .add_device(
                name,
                KEY,
                "https://home.example",
                RemoteConnection::Simple,
                "a",
            )
            .unwrap()
            .id
    };
    let paired = add("Pixel");
    let tablet = add("Tablet");
    phone
        .engine
        .settle_arrival(computer.dir.path(), &paired)
        .unwrap();

    computer.engine.revoke_device(&tablet).unwrap();
    let watch = add("Watch");
    computer
        .engine
        .update_remote_state(|s| s.machine_name = Some("Desk".into()))
        .unwrap();
    phone.engine.retake_computer(computer.dir.path()).unwrap();

    let mut live: Vec<String> = phone
        .engine
        .devices(false)
        .unwrap()
        .into_iter()
        .map(|d| d.id)
        .collect();
    live.sort();
    let mut want = vec![paired, watch];
    want.sort();
    assert_eq!(
        live, want,
        "the removed phone stays removed, the new one is there"
    );
    assert_eq!(
        phone.engine.remote_state().unwrap().machine_name.as_deref(),
        Some("Desk")
    );
}

#[test]
fn a_store_in_use_a_version_behind_still_settles() {
    let phone = fixture();
    phone.engine.ensure_local_device("This phone").unwrap();
    let computer = fixture();
    let paired = computer
        .engine
        .add_device(
            "Pixel",
            KEY,
            "https://home.example",
            RemoteConnection::Simple,
            "a",
        )
        .unwrap()
        .id;
    // The store in use lacks a column the arriving one has, as one not yet migrated would.
    let conn = rusqlite::Connection::open(computer.dir.path().join("nocturne.db")).unwrap();
    conn.execute_batch("ALTER TABLE remote_devices DROP COLUMN agreed_build;")
        .unwrap();
    drop(conn);
    phone
        .engine
        .settle_arrival(computer.dir.path(), &paired)
        .unwrap();
    assert_eq!(
        phone.engine.device(&paired).unwrap().unwrap().public_key,
        KEY
    );
}

#[test]
fn a_leaving_memory_names_where_its_things_were_kept() {
    let phone = fixture();
    phone.engine.ensure_local_device("Phone").unwrap();
    let restored = keep(&phone.engine, "From an older backup");
    assert_eq!(phone.engine.name_unnamed_items("Desk").unwrap(), 1);
    assert_eq!(phone.engine.name_unnamed_items("Another name").unwrap(), 0);
    assert_eq!(
        phone
            .engine
            .get_item(&restored)
            .unwrap()
            .from_device
            .as_deref(),
        Some("Desk")
    );
}

#[test]
fn a_phones_things_arrive_named_after_the_phone_as_paired_here() {
    let phone = fixture();
    let local = phone.engine.ensure_local_device("Phone").unwrap();
    let typed = keep(&phone.engine, "Bought a ticket");
    phone.engine.link_remote_keep(&typed, &local, "c1").unwrap();
    let unlinked = keep(&phone.engine, "Kept with no keep link");
    let computer = fixture();
    let paired = computer
        .engine
        .add_device(
            "Pixel 9",
            KEY,
            "https://home.example",
            RemoteConnection::Simple,
            "a",
        )
        .unwrap()
        .id;
    phone
        .engine
        .settle_arrival(computer.dir.path(), &paired)
        .unwrap();
    for id in [&typed, &unlinked] {
        assert_eq!(
            phone.engine.get_item(id).unwrap().from_device.as_deref(),
            Some("Pixel 9")
        );
    }
}
