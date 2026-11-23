//! A phone's things brought into a computer that already holds a memory: what travels, what is
//! kept once, and that all of it is added in one step or none of it.

mod common;

use common::{date, fixture, keep, sql_count, write_file};
use nocturne_contract::{
    CheckInInput, Feeling, FeelingPart, FeelingPoint, ItemStatus, Kept, RemoteConnection, Source,
};
use nocturne_engine::{BroughtIn, Engine, KeepPlace};

const KEY: &[u8] = &[7, 1, 9, 4];
const PHONE: &str = "Ana\u{2019}s Pixel";

fn pair(computer: &Engine) -> String {
    computer
        .add_device(
            PHONE,
            KEY,
            "https://home.example",
            RemoteConnection::Simple,
            "a",
        )
        .unwrap()
        .id
}

fn feeling() -> Feeling {
    Feeling {
        part: FeelingPart::Settled,
        point: FeelingPoint {
            pleasant: 0.5,
            energy: 0.5,
        },
        words: vec!["settled".into()],
    }
}

fn checkin(e: &Engine, line: &str) -> String {
    e.keep_checkin(
        CheckInInput {
            feeling: feeling(),
            line: Some(line.into()),
            urge: None,
            replaces: None,
        },
        Source::Phone,
    )
    .unwrap()
    .id
}

fn item_count(e: &Engine) -> u32 {
    e.stats().unwrap().item_count
}

#[test]
fn the_phones_things_journal_and_check_ins_are_added_and_wait_to_be_read() {
    let phone = fixture();
    let computer = fixture();
    keep(&computer.engine, "The gate code is 4512.");
    let device = pair(&computer.engine);

    let note = keep(&phone.engine, "The heron feeder is refilled on Thursday.");
    let photo = write_file(phone.dir.path(), "heron.png", b"\x89PNG\r\n\x1a\nheron");
    let Kept::New { item: photo } = phone.engine.keep_file(&photo, Source::Phone).unwrap() else {
        panic!("a new photo")
    };
    let day = date("2026-11-13");
    phone
        .engine
        .save_entry_text(day, None, 0, "Walked to the river before work.")
        .unwrap();
    phone
        .engine
        .keep_good_thing(day, "Tea in the garden", None, Source::Phone)
        .unwrap();
    checkin(&phone.engine, "A slow morning");

    let brought = computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    assert_eq!(
        brought,
        BroughtIn {
            things: 2,
            entries: 2,
            checkins: 1,
            same: 0,
            already: 0,
        }
    );
    let e = &computer.engine;
    assert_eq!(item_count(e), 1 + item_count(&phone.engine));

    let came = e.get_item(&note).unwrap();
    assert_eq!(came.status, ItemStatus::Kept, "it waits to be read as new");
    assert_eq!(came.from_device.as_deref(), Some(PHONE));
    assert!(
        e.pending_items(50).unwrap().iter().any(|p| p.id == note),
        "it is in the queue"
    );
    let found = e.find_items("heron feeder", 10).unwrap();
    assert!(
        found.iter().any(|f| f.receipt.item_id == note),
        "finding it by its words works at once"
    );
    assert!(
        e.find_unread("heron feeder", 10)
            .unwrap()
            .iter()
            .any(|f| f.receipt.item_id == note),
        "and it is among what waits to be read"
    );

    let picture = e.get_item(&photo.id).unwrap();
    assert!(picture.original_path.is_some(), "the original came with it");
    let journal = e.journal_day(day).unwrap();
    assert_eq!(journal.entry.len(), 1);
    assert_eq!(journal.good.len(), 1);
    assert_eq!(journal.checkins.len(), 1);
    assert_eq!(
        sql_count(computer.dir.path(), "SELECT COUNT(*) FROM memories"),
        0,
        "nothing a mind worked out on the phone comes"
    );
}

#[test]
fn bringing_the_same_memory_in_twice_adds_nothing_the_second_time() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    keep(&phone.engine, "Bins out on Tuesday.");
    checkin(&phone.engine, "Tired");
    let first = computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    let count = item_count(&computer.engine);
    let again = computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    assert_eq!(item_count(&computer.engine), count);
    assert_eq!(
        again,
        BroughtIn {
            already: first.things + first.checkins,
            ..BroughtIn::default()
        }
    );
}

#[test]
fn the_same_thing_kept_on_both_is_kept_once_and_lists_both_places() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    let here = keep(&computer.engine, "Call the plumber about the boiler.");
    let there = keep(&phone.engine, "Call the plumber about the boiler.");
    phone.engine.set_item_private(&there, true).unwrap();

    let brought = computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    assert_eq!((brought.things, brought.same), (0, 1));
    let e = &computer.engine;
    assert_eq!(item_count(e), 1);
    let kept = e.get_item(&here).unwrap();
    assert!(kept.private, "private when either is private");
    assert_eq!(kept.from_device, None, "the one here stays as it was");
    let phone_kept = phone.engine.get_item(&there).unwrap().kept_at;
    assert_eq!(
        kept.also_kept_on,
        vec![nocturne_contract::KeptElsewhere {
            name: PHONE.into(),
            at: Some(phone_kept)
        }],
        "its page lists the other place, with when"
    );
    // Brought in again, the place is listed once.
    computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    assert_eq!(e.get_item(&here).unwrap().also_kept_on.len(), 1);
}

#[test]
fn a_private_mark_travels_and_stays() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    let secret = keep(&phone.engine, "The safe is behind the mirror.");
    phone.engine.set_item_private(&secret, true).unwrap();
    computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    assert!(computer.engine.get_item(&secret).unwrap().private);
}

#[test]
fn what_the_phone_sent_as_a_companion_is_not_added_again() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    let local = phone.engine.ensure_local_device("This phone").unwrap();

    // Sent to the computer as a companion, and kept on the phone under the same keep.
    let sent = keep(&computer.engine, "Pick up the parcel from the depot.");
    computer
        .engine
        .link_remote_keep(&sent, &device, "c-7")
        .unwrap();
    phone
        .engine
        .keep_note_item(
            "Pick up the parcel from the depot (slip in the hall).",
            Source::Phone,
            false,
            &KeepPlace {
                day: None,
                phone: Some((&local, "c-7")),
            },
        )
        .unwrap();

    let brought = computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    assert_eq!(brought.already, 1, "{brought:?}");
    assert_eq!(item_count(&computer.engine), 1);
}

#[test]
fn a_phone_keep_carries_its_keep_so_a_later_send_is_kept_once() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    let local = phone.engine.ensure_local_device("This phone").unwrap();
    phone
        .engine
        .keep_note_item(
            "Renew the library books.",
            Source::Phone,
            false,
            &KeepPlace {
                day: None,
                phone: Some((&local, "c-9")),
            },
        )
        .unwrap();
    computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    let item = computer
        .engine
        .remote_keep(&device, "c-9")
        .unwrap()
        .expect("the keep came with it");
    assert_eq!(
        computer
            .engine
            .get_item(&item)
            .unwrap()
            .from_device
            .as_deref(),
        Some(PHONE)
    );
}

#[test]
fn forgotten_things_and_the_calendar_do_not_travel() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    let local = phone.engine.ensure_local_device("This phone").unwrap();
    let gone = keep(&phone.engine, "Something I took back.");
    phone.engine.unkeep(&gone).unwrap();
    let forgetting = keep(&phone.engine, "Something being forgotten.");
    phone.engine.unkeep_begin_only(&forgetting).unwrap();
    let stays = keep(&phone.engine, "Something that stays.");
    let batch = nocturne_contract::CalendarBatch {
        sync_id: "s1".into(),
        seq: 0,
        last: true,
        window_from: "2026-11-11".into(),
        window_to: "2026-11-27".into(),
        calendars: vec![nocturne_contract::PhoneCalendar {
            id: "c1".into(),
            name: "Home".into(),
            colour: None,
            account: None,
        }],
        changes: vec![nocturne_contract::CalendarChange::Upsert {
            entry: nocturne_contract::CalendarEntry {
                phone_id: "e1".into(),
                calendar_id: "c1".into(),
                title: "Dentist".into(),
                start: "2026-11-13T10:00:00Z".into(),
                end: "2026-11-13T10:30:00Z".into(),
                time_zone: None,
                all_day: false,
                place: None,
                people: Vec::new(),
                repeats: false,
                notes: None,
                repeat_rule: None,
            },
        }],
    };
    phone.engine.take_calendar(&local, batch).unwrap();

    let brought = computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    assert_eq!(brought.things, 1, "{brought:?}");
    let e = &computer.engine;
    assert!(e.get_item(&stays).is_ok());
    assert!(e.get_item(&gone).is_err());
    assert!(e.get_item(&forgetting).is_err());
    assert_eq!(
        sql_count(
            computer.dir.path(),
            "SELECT COUNT(*) FROM items WHERE kind = 'calendar'"
        ),
        0,
        "the phone sends its calendar again"
    );
}

#[test]
fn entries_for_the_same_day_on_both_are_both_kept_in_time_order() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    let day = date("2026-11-13");
    // On the phone at seven, on the computer at nine.
    phone
        .engine
        .save_entry_text(day, None, 0, "Early swim, cold water.")
        .unwrap();
    computer.clock.set("2026-11-13T09:00:00Z".parse().unwrap());
    computer
        .engine
        .save_entry_text(day, None, 0, "Long meeting about the move.")
        .unwrap();
    checkin(&computer.engine, "Busy");
    checkin(&phone.engine, "Bright");

    let brought = computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    assert_eq!((brought.entries, brought.checkins), (1, 1));
    let journal = computer.engine.journal_day(day).unwrap();
    let texts: Vec<_> = journal
        .entry
        .iter()
        .map(|b| (b.block, b.item.text.clone().unwrap_or_default()))
        .collect();
    assert_eq!(
        texts,
        vec![
            (0, "Early swim, cold water.".to_string()),
            (1, "Long meeting about the move.".to_string()),
        ],
        "nothing is joined into one entry"
    );
    assert_eq!(journal.checkins.len(), 2);
}

#[test]
fn an_earlier_version_comes_with_the_one_that_replaced_it_and_counts_once() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    let first = checkin(&phone.engine, "Wobbly");
    let second = phone
        .engine
        .keep_checkin(
            CheckInInput {
                feeling: feeling(),
                line: Some("Better after lunch".into()),
                urge: None,
                replaces: Some(first.clone()),
            },
            Source::Phone,
        )
        .unwrap()
        .id;
    let brought = computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    assert_eq!(brought.checkins, 1);
    assert_eq!(
        sql_count(
            computer.dir.path(),
            &format!(
                "SELECT COUNT(*) FROM items WHERE id = '{second}' AND \
                 json_extract(metadata, '$.replaces') = '{first}'"
            )
        ),
        1
    );
}

#[test]
fn nothing_comes_through_a_phone_that_is_not_paired() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    computer.engine.revoke_device(&device).unwrap();
    keep(&phone.engine, "A note.");
    assert!(computer.engine.bring_in(phone.dir.path(), &device).is_err());
    assert_eq!(item_count(&computer.engine), 0);
}

#[test]
fn a_damaged_original_name_adds_nothing() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    keep(&phone.engine, "A note that would travel.");
    let photo = write_file(phone.dir.path(), "p.png", b"\x89PNG\r\n\x1a\nx");
    let Kept::New { item } = phone.engine.keep_file(&photo, Source::Phone).unwrap() else {
        panic!("a new photo")
    };
    let conn = rusqlite::Connection::open(phone.dir.path().join("nocturne.db")).unwrap();
    conn.execute(
        "UPDATE items SET ext = '../../escape' WHERE id = ?1",
        [&item.id],
    )
    .unwrap();
    assert!(computer.engine.bring_in(phone.dir.path(), &device).is_err());
    assert_eq!(item_count(&computer.engine), 0, "none of it was added");
}

#[test]
fn a_photo_in_an_entry_on_the_phone_keeps_its_place_when_it_is_a_plain_thing_here() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    let bytes: &[u8] = b"\x89PNG\r\n\x1a\nthe river at dawn";
    let here = write_file(computer.dir.path(), "river.png", bytes);
    let Kept::New { item: here } = computer.engine.keep_file(&here, Source::Drop).unwrap() else {
        panic!("a new photo here")
    };
    let day = date("2026-11-13");
    let there = write_file(phone.dir.path(), "river.png", bytes);
    let Kept::New { item: there } = phone.engine.keep_file(&there, Source::Phone).unwrap() else {
        panic!("a new photo on the phone")
    };
    phone
        .engine
        .save_entry_text(day, None, 0, "Walked to the river before work.")
        .unwrap();
    phone.engine.place_in_entry(&there.id, day, 1).unwrap();

    let brought = computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    assert_eq!(brought.same, 1, "{brought:?}");
    let journal = computer.engine.journal_day(day).unwrap();
    let blocks: Vec<(&str, u32)> = journal
        .entry
        .iter()
        .map(|b| (b.item.id.as_str(), b.block))
        .collect();
    assert_eq!(blocks.len(), 2, "{blocks:?}");
    assert!(
        blocks.contains(&(here.id.as_str(), 1)),
        "the photo here takes the phone's place in the entry: {blocks:?}"
    );
}

#[test]
fn a_forget_here_that_is_not_done_never_swallows_the_phones_copy() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    let bytes: &[u8] = b"\x89PNG\r\n\x1a\nthe allotment shed";
    let here = write_file(computer.dir.path(), "shed.png", bytes);
    let Kept::New { item: here } = computer.engine.keep_file(&here, Source::Drop).unwrap() else {
        panic!("a new photo here")
    };
    let there = write_file(phone.dir.path(), "shed.png", bytes);
    let Kept::New { item: there } = phone.engine.keep_file(&there, Source::Phone).unwrap() else {
        panic!("a new photo on the phone")
    };
    computer.engine.unkeep_begin_only(&here.id).unwrap();

    let busy = computer.engine.bring_in(phone.dir.path(), &device);
    assert!(
        matches!(busy, Err(nocturne_engine::Error::Busy)),
        "{busy:?}"
    );
    let stray = std::fs::read_dir(computer.dir.path().join("originals"))
        .unwrap()
        .flatten()
        .filter(|f| f.file_name().to_string_lossy().starts_with(".tmp-"))
        .count();
    assert_eq!(stray, 0, "no copy is left behind");

    computer.engine.resume_forgets_for_test().unwrap();
    let brought = computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    assert_eq!(brought.things, 1, "{brought:?}");
    let came = computer.engine.get_item(&there.id).unwrap();
    assert!(
        came.original_path
            .as_deref()
            .is_some_and(|p| std::path::Path::new(p).is_file()),
        "the phone's copy came with its original"
    );
}

fn heron() -> nocturne_engine::Candidate {
    common::cand(
        "heron",
        nocturne_engine::Predicate::Prefers,
        "the feeder",
        "The heron prefers the feeder.",
    )
}

/// Forget the memory `memory` on `e`, leaving the thing it came from; `begin_only` stops part way.
fn forget_memory(e: &Engine, memory: &str, originals: bool, begin_only: bool) {
    let preview = e.preview_forget(&[memory.to_string()], originals).unwrap();
    if begin_only {
        e.forget_begin_only(&preview.token).unwrap();
    } else {
        e.forget(&preview.token).unwrap();
    }
}

fn says_heron(e: &Engine, item: &str) -> bool {
    use common::EngineExt;
    !e.apply(item, vec![heron()]).unwrap().created.is_empty()
}

#[test]
fn what_the_person_forgot_on_the_phone_is_never_read_again_on_the_computer() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    let (note, memory) = common::learn(&phone.engine, "The heron prefers the feeder.", heron());
    forget_memory(&phone.engine, &memory, false, false);

    let brought = computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    assert_eq!(brought.things, 1, "the thing itself stays and travels");
    let e = &computer.engine;
    assert_eq!(e.get_item(&note).unwrap().status, ItemStatus::Kept);
    assert_eq!(
        sql_count(computer.dir.path(), "SELECT COUNT(*) FROM forgotten_claims"),
        1
    );
    assert!(
        e.find_items("heron feeder", 10).unwrap().is_empty(),
        "nor is it found by its words, as after a forget here"
    );
    assert!(
        !says_heron(e, &note),
        "the forgotten claim is not said again"
    );
}

#[test]
fn a_forget_on_the_phone_that_is_not_done_still_holds_on_the_computer() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    // Forgetting the memory alone: the thing stays, and what was forgotten comes with it.
    let (stays, memory) = common::learn(&phone.engine, "The heron prefers the feeder.", heron());
    forget_memory(&phone.engine, &memory, false, true);
    // Forgetting the memory with its original: the thing goes, so it does not come.
    let (goes, memory) = common::learn(
        &phone.engine,
        "The fox prefers the shed.",
        common::cand(
            "fox",
            nocturne_engine::Predicate::Prefers,
            "the shed",
            "The fox prefers the shed.",
        ),
    );
    forget_memory(&phone.engine, &memory, true, true);

    let brought = computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    assert_eq!(brought.things, 1, "{brought:?}");
    let e = &computer.engine;
    assert!(
        e.get_item(&goes).is_err(),
        "a thing being forgotten does not come"
    );
    assert!(e.get_item(&stays).is_ok(), "a thing a forget leaves comes");
    assert!(!says_heron(e, &stays));
}

#[test]
fn a_forget_on_the_phone_holds_for_the_same_thing_already_here() {
    let phone = fixture();
    let computer = fixture();
    let device = pair(&computer.engine);
    let here = keep(&computer.engine, "The heron prefers the feeder.");
    let (_, memory) = common::learn(&phone.engine, "The heron prefers the feeder.", heron());
    forget_memory(&phone.engine, &memory, false, false);

    let brought = computer.engine.bring_in(phone.dir.path(), &device).unwrap();
    assert_eq!(brought.same, 1, "{brought:?}");
    assert!(
        !says_heron(&computer.engine, &here),
        "the one here never says what was forgotten on the phone"
    );
}
