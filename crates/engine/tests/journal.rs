mod common;

use common::*;
use jiff::SignedDuration;
use nocturne_contract::{
    CheckInInput, DayPart, EntityKind, Feeling, FeelingPart, FeelingPoint, ItemKind, ItemStatus,
    JournalRole, Kept, PeriodKind, RemoteConnection, RowKind, SettingsPatch, Source,
};
use nocturne_engine::{Error, KeepPlace};

fn feeling(part: FeelingPart, words: &[&str]) -> Feeling {
    Feeling {
        part,
        point: FeelingPoint {
            pleasant: 0.5,
            energy: 0.5,
        },
        words: words.iter().map(|w| w.to_string()).collect(),
    }
}

fn input(f: Feeling, line: Option<&str>) -> CheckInInput {
    CheckInInput {
        feeling: f,
        line: line.map(str::to_string),
        urge: None,
        replaces: None,
    }
}

fn after(f: &Fixture, minutes: i64) {
    f.clock.set(
        f.clock
            .now()
            .checked_add(SignedDuration::from_mins(minutes))
            .unwrap(),
    );
}

use nocturne_engine::Clock;

/// Read an item the way the reader does, with nothing found in it.
fn read(f: &Fixture, id: &str) {
    f.engine.apply(id, vec![]).unwrap();
}

#[test]
fn a_note_kept_at_twenty_to_one_in_the_morning_belongs_to_the_evening_before() {
    let f = fixture_at("2026-11-14T00:40:00Z", "Europe/London");
    let id = keep(&f.engine, "Walked home late");
    assert_eq!(f.engine.journal_today().unwrap(), date("2026-11-13"));
    let day = f.engine.journal_day(date("2026-11-13")).unwrap();
    assert!(day.today);
    let row = day
        .rows
        .iter()
        .find(|r| r.item.id == id)
        .expect("on the evening before");
    assert_eq!(row.part, DayPart::AfterMidnight);
    assert_eq!(row.kind, RowKind::Note);
    let next = f.engine.journal_day(date("2026-11-14"));
    assert!(next.is_err(), "the day after has not come");
}

#[test]
fn an_early_page_time_ends_the_day_half_an_hour_before_it() {
    let f = fixture();
    f.engine
        .update_settings(SettingsPatch {
            page_time: Some("02:00".into()),
            ..Default::default()
        })
        .unwrap();
    let (_, end) = f.engine.journal_bounds(date("2026-11-13")).unwrap();
    assert_eq!(end, "2026-11-14T01:30:00Z".parse().unwrap());
    assert_eq!(
        f.engine
            .journal_day_of("2026-11-14T01:00:00Z".parse().unwrap())
            .unwrap(),
        date("2026-11-13")
    );
    assert_eq!(
        f.engine
            .journal_day_of("2026-11-14T01:40:00Z".parse().unwrap())
            .unwrap(),
        date("2026-11-14")
    );
}

#[test]
fn editing_an_unread_block_changes_it_in_place() {
    let f = fixture();
    let day = date("2026-11-13");
    let first = f
        .engine
        .save_entry_text(day, None, 0, "The morning was slow")
        .unwrap()
        .unwrap();
    after(&f, 3);
    let second = f
        .engine
        .save_entry_text(
            day,
            Some(&first.item.id),
            0,
            "The morning was slow but kind",
        )
        .unwrap()
        .unwrap();
    assert_eq!(second.item.id, first.item.id);
    assert_eq!(
        second.item.text.as_deref(),
        Some("The morning was slow but kind")
    );
    assert!(second.earlier.is_empty());
    assert_eq!(f.engine.journal_day(day).unwrap().entry.len(), 1);
    assert_eq!(second.item.journal, Some(JournalRole::Entry));
}

#[test]
fn editing_a_read_block_keeps_the_earlier_version_and_the_old_receipt_still_resolves() {
    let f = fixture();
    let day = date("2026-11-13");
    let first = f
        .engine
        .save_entry_text(day, None, 0, "Gran came for tea")
        .unwrap()
        .unwrap();
    read(&f, &first.item.id);
    after(&f, 20);
    let changed = f
        .engine
        .save_entry_text(day, Some(&first.item.id), 0, "Gran came for tea and stayed")
        .unwrap()
        .unwrap();
    assert_ne!(changed.item.id, first.item.id);
    assert_eq!(changed.earlier.len(), 1);
    assert_eq!(changed.earlier[0].item_id, first.item.id);
    assert_eq!(
        changed.earlier[0].text.as_deref(),
        Some("Gran came for tea")
    );
    assert!(changed.changed_at.is_some());
    assert_eq!(
        f.engine.get_item(&first.item.id).unwrap().text.as_deref(),
        Some("Gran came for tea")
    );
    let view = f.engine.journal_day(day).unwrap();
    assert_eq!(
        view.entry.len(),
        1,
        "the earlier version is history, not a block"
    );
    assert_eq!(view.entry[0].item.id, changed.item.id);
    assert_eq!(
        view.rows
            .iter()
            .filter(|r| r.kind == RowKind::Entry)
            .count(),
        1
    );
}

#[test]
fn empty_text_deletes_an_unread_block_and_is_refused_on_a_read_one() {
    let f = fixture();
    let day = date("2026-11-13");
    let unread = f
        .engine
        .save_entry_text(day, None, 0, "Scratch")
        .unwrap()
        .unwrap();
    assert!(
        f.engine
            .save_entry_text(day, Some(&unread.item.id), 0, "  ")
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        f.engine.get_item(&unread.item.id),
        Err(Error::NotFound(_))
    ));

    let kept = f
        .engine
        .save_entry_text(day, None, 0, "Keep this")
        .unwrap()
        .unwrap();
    read(&f, &kept.item.id);
    assert!(matches!(
        f.engine.save_entry_text(day, Some(&kept.item.id), 0, ""),
        Err(Error::Invalid(_))
    ));
    assert!(f.engine.get_item(&kept.item.id).is_ok());
    assert!(
        f.engine
            .save_entry_text(day, None, 1, "")
            .unwrap()
            .is_none()
    );
}

#[test]
fn held_blocks_are_not_pending_until_the_hold_passes_or_read_entry_now() {
    let f = fixture();
    let day = date("2026-11-13");
    let block = f
        .engine
        .save_entry_text(day, None, 0, "Still writing")
        .unwrap()
        .unwrap();
    assert!(f.engine.pending_items(10).unwrap().is_empty());
    after(&f, 9);
    assert!(f.engine.pending_items(10).unwrap().is_empty());
    after(&f, 2);
    assert_eq!(f.engine.pending_items(10).unwrap()[0].id, block.item.id);

    let again = f
        .engine
        .save_entry_text(day, Some(&block.item.id), 0, "Still writing more")
        .unwrap()
        .unwrap();
    assert!(
        f.engine.pending_items(10).unwrap().is_empty(),
        "a save holds it again"
    );
    f.engine.read_entry_now(day).unwrap();
    assert_eq!(f.engine.pending_items(10).unwrap()[0].id, again.item.id);

    let other = f
        .engine
        .save_entry_text(day, None, 1, "Another")
        .unwrap()
        .unwrap();
    assert!(
        f.engine
            .pending_items(10)
            .unwrap()
            .iter()
            .all(|p| p.id != other.item.id)
    );
    assert_eq!(f.engine.release_entry_holds().unwrap(), 1);
    assert_eq!(f.engine.pending_items(10).unwrap().len(), 2);
}

#[test]
fn a_quick_note_with_a_feeling_shows_the_feeling_on_its_row_and_no_separate_checkin_row() {
    let f = fixture();
    let kept = f
        .engine
        .keep_note(
            "Lunch with Priya",
            Source::QuickCapture,
            Some(feeling(FeelingPart::Settled, &["calm"])),
            false,
        )
        .unwrap();
    let Kept::New { item } = kept else {
        panic!("a new note")
    };
    assert_eq!(item.journal, None, "an ordinary note");
    let day = f.engine.journal_day(date("2026-11-13")).unwrap();
    assert_eq!(day.rows.len(), 1);
    let row = &day.rows[0];
    assert_eq!(row.item.id, item.id);
    let shown = row.feeling.as_ref().expect("its feeling");
    assert_eq!(shown.about.as_deref(), Some(item.id.as_str()));
    assert_eq!(shown.feeling.words, ["calm"]);
    assert_eq!(shown.day, "2026-11-13");
    assert_eq!(
        day.checkins.len(),
        1,
        "the check-in still counts as how the day felt"
    );
    assert_eq!(row.move_to.as_deref(), Some("2026-11-12"));
}

#[test]
fn a_feeling_with_a_note_kept_before_belongs_to_today_not_the_old_note_day() {
    let f = fixture_at("2026-11-12T10:00:00Z", "Europe/London");
    let first = keep(&f.engine, "Lunch with Priya");
    f.clock.set_str("2026-11-13T10:00:00Z");
    let kept = f
        .engine
        .keep_note(
            "Lunch with Priya",
            Source::QuickCapture,
            Some(feeling(FeelingPart::Settled, &["calm"])),
            false,
        )
        .unwrap();
    let Kept::Duplicate { item } = kept else {
        panic!("the same note again")
    };
    assert_eq!(item.id, first);
    let yesterday = f.engine.journal_day(date("2026-11-12")).unwrap();
    assert!(yesterday.checkins.is_empty());
    let today = f.engine.journal_day(date("2026-11-13")).unwrap();
    assert_eq!(today.checkins.len(), 1);
}

#[test]
fn taking_back_a_note_takes_back_the_feeling_kept_with_it() {
    let f = fixture();
    let Kept::New { item } = f
        .engine
        .keep_note(
            "Lunch with Priya",
            Source::QuickCapture,
            Some(feeling(FeelingPart::Settled, &["calm"])),
            false,
        )
        .unwrap()
    else {
        panic!("a new note")
    };
    f.engine.unkeep(&item.id).unwrap();
    let day = f.engine.journal_day(date("2026-11-13")).unwrap();
    assert!(day.rows.is_empty());
    assert!(day.checkins.is_empty());
}

#[test]
fn good_things_are_a_note_marked_good_and_a_fourth_is_refused() {
    let f = fixture();
    let day = date("2026-11-13");
    for text in [
        "Sam fixed the gate",
        "Sam fixed the gate",
        "Sun on the table",
    ] {
        f.engine
            .keep_good_thing(day, text, None, Source::Journal)
            .unwrap();
        after(&f, 1);
    }
    assert!(matches!(
        f.engine
            .keep_note("A fourth", Source::QuickCapture, None, true),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        f.engine
            .keep_good_thing(day, "A fourth", None, Source::Journal),
        Err(Error::Invalid(_))
    ));
    let view = f.engine.journal_day(day).unwrap();
    assert_eq!(view.good.len(), 3);
    assert!(view.rows.iter().all(|r| r.kind == RowKind::Good));
    assert_eq!(view.good[0].text, "Sam fixed the gate");
    assert_eq!(
        view.good[1].text, "Sam fixed the gate",
        "identical words never collapse"
    );
    // Another day has room.
    f.engine
        .keep_good_thing(date("2026-11-12"), "Dry socks", None, Source::Journal)
        .unwrap();
}

#[test]
fn what_a_phones_keep_makes_beside_its_item_is_made_once_and_found_by_the_keep() {
    let f = fixture();
    let device = f
        .engine
        .add_device(
            "Pixel",
            &[7, 1, 9],
            "https://home.example",
            RemoteConnection::Simple,
            "a",
        )
        .unwrap()
        .id;
    let place = KeepPlace {
        phone: Some((&device, "c-1")),
        ..Default::default()
    };
    let other = KeepPlace {
        phone: Some((&device, "c-2")),
        ..Default::default()
    };
    let day = date("2026-11-13");
    let kept = f
        .engine
        .keep_note_item("Sam fixed the gate", Source::Phone, false, &place)
        .unwrap();
    let Kept::New { item } = &kept else {
        panic!("a new note")
    };
    assert_eq!(
        f.engine.remote_keep(&device, "c-1").unwrap().as_deref(),
        Some(item.id.as_str()),
        "the note is the phone's as soon as it is kept"
    );
    assert!(!f.engine.keep_is_marked(&place, true, true).unwrap());
    let first = f
        .engine
        .feel_kept(
            &kept,
            feeling(FeelingPart::LitUp, &["glad"]),
            Source::Phone,
            &place,
        )
        .unwrap();
    let again = f
        .engine
        .feel_kept(
            &kept,
            feeling(FeelingPart::Low, &["flat"]),
            Source::Phone,
            &place,
        )
        .unwrap();
    assert_eq!(first.id, again.id, "sent twice, felt once");
    let thing = f
        .engine
        .keep_good_thing_at(day, "Sam fixed the gate", None, Source::Phone, &place)
        .unwrap();
    let thing_again = f
        .engine
        .keep_good_thing_at(day, "Sam fixed the gate", None, Source::Phone, &place)
        .unwrap();
    assert_eq!(thing.id, thing_again.id);
    let view = f.engine.journal_day(day).unwrap();
    assert_eq!((view.checkins.len(), view.good.len()), (1, 1));
    assert!(f.engine.keep_is_marked(&place, true, true).unwrap());
    assert!(!f.engine.keep_is_marked(&other, true, false).unwrap());
    assert_eq!(
        f.engine.keep_records(&place).unwrap(),
        vec![first.id.clone(), thing.id.clone()]
    );
    assert!(f.engine.keep_records(&other).unwrap().is_empty());
    assert!(
        f.engine
            .keep_records(&KeepPlace::default())
            .unwrap()
            .is_empty()
    );

    // A keep that already made its good thing does not count against the day's three.
    for text in ["One", "Two"] {
        f.engine
            .keep_good_thing(day, text, None, Source::Journal)
            .unwrap();
    }
    assert!(f.engine.good_room(day, &place).unwrap());
    assert!(!f.engine.good_room(day, &other).unwrap());
}

#[test]
fn a_capture_instant_gives_its_day_never_a_future_one_or_one_over_a_week_old() {
    let f = fixture();
    let on = |at: Option<&str>| f.engine.capture_day(at).unwrap();
    assert_eq!(on(None), date("2026-11-13"));
    assert_eq!(on(Some("2026-11-12T15:00:00Z")), date("2026-11-12"));
    assert_eq!(
        on(Some("2026-11-12T16:00:00+01:00")),
        date("2026-11-12"),
        "an offset is read"
    );
    assert_eq!(on(Some("2026-11-06T09:00:00Z")), date("2026-11-06"));
    let a_week_back = f
        .engine
        .journal_day_of(
            f.clock
                .now()
                .checked_sub(SignedDuration::from_hours(7 * 24))
                .unwrap(),
        )
        .unwrap();
    assert_eq!(on(Some("2026-10-01T12:00:00Z")), a_week_back);
    assert_eq!(on(Some("2026-11-14T12:00:00Z")), date("2026-11-13"));
    for bad in ["", "yesterday", "2026-11-12", "12:00"] {
        assert_eq!(on(Some(bad)), date("2026-11-13"), "{bad:?} is now");
    }
}

#[test]
fn a_phones_keep_made_earlier_belongs_to_the_day_it_was_made() {
    let f = fixture();
    let device = f
        .engine
        .add_device(
            "Pixel",
            &[7, 1, 9],
            "https://home.example",
            RemoteConnection::Simple,
            "a",
        )
        .unwrap()
        .id;
    let (yesterday, today) = (date("2026-11-12"), date("2026-11-13"));
    let place = KeepPlace {
        day: Some(yesterday),
        phone: Some((&device, "c-1")),
    };

    let kept = f
        .engine
        .keep_note_item("Walked home late", Source::Phone, false, &place)
        .unwrap();
    let Kept::New { item } = &kept else {
        panic!("a new note")
    };
    f.engine
        .feel_kept(
            &kept,
            feeling(FeelingPart::Low, &["tired"]),
            Source::Phone,
            &place,
        )
        .unwrap();
    let past = f.engine.journal_day(yesterday).unwrap();
    assert_eq!(past.rows.len(), 1);
    assert_eq!(past.rows[0].item.id, item.id);
    assert_eq!(past.checkins.len(), 1, "its feeling is on the same day");
    let now = f.engine.journal_day(today).unwrap();
    assert!(now.rows.is_empty() && now.checkins.is_empty());

    let good = KeepPlace {
        phone: Some((&device, "c-2")),
        ..place
    };
    f.engine
        .keep_note_item("Dry socks", Source::Phone, true, &good)
        .unwrap();
    assert_eq!(f.engine.journal_day(yesterday).unwrap().good.len(), 1);
    assert!(f.engine.journal_day(today).unwrap().good.is_empty());

    f.engine
        .keep_checkin_at(
            input(feeling(FeelingPart::Settled, &["calm"]), None),
            Source::Phone,
            &KeepPlace {
                day: Some(yesterday),
                phone: None,
            },
        )
        .unwrap();
    assert_eq!(f.engine.journal_day(yesterday).unwrap().checkins.len(), 2);
    assert!(f.engine.journal_day(today).unwrap().checkins.is_empty());

    // A note that was kept before stays where it is; only the new feeling takes the day.
    let before = keep(&f.engine, "Bought milk");
    let again = f
        .engine
        .keep_note_item(
            "Bought milk",
            Source::Phone,
            false,
            &KeepPlace {
                day: Some(yesterday),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(matches!(&again, Kept::Duplicate { item } if item.id == before));
    assert!(
        f.engine
            .journal_day(yesterday)
            .unwrap()
            .rows
            .iter()
            .all(|r| r.item.id != before)
    );

    // The day it was kept on needs no mark.
    let same = f
        .engine
        .keep_note_item(
            "Same day",
            Source::Phone,
            false,
            &KeepPlace {
                day: Some(today),
                ..Default::default()
            },
        )
        .unwrap();
    let Kept::New { item } = same else {
        panic!("a new note")
    };
    assert_eq!(
        common::sql_count(
            f.dir.path(),
            &format!(
                "SELECT COUNT(*) FROM items WHERE id = '{}' AND json_extract(metadata, '$.journal_day') IS NULL",
                item.id
            )
        ),
        1
    );
}

#[test]
fn a_good_thing_may_be_a_photo_alone_and_a_note_alone_may_not_be_empty() {
    let f = fixture();
    let day = date("2026-11-13");
    let Kept::New { item: photo } = f
        .engine
        .keep_bytes(b"png-bytes", "png", Source::Journal)
        .unwrap()
    else {
        panic!("new photo")
    };
    assert!(
        f.engine
            .keep_good_thing(day, "", None, Source::Journal)
            .is_err()
    );
    let good = f
        .engine
        .keep_good_thing(day, "", Some(&photo.id), Source::Journal)
        .unwrap();
    assert_eq!(good.text, "");
    assert_eq!(
        good.photo.as_ref().map(|p| p.id.as_str()),
        Some(photo.id.as_str())
    );
}

#[test]
fn journal_items_cannot_be_marked_private() {
    let f = fixture();
    let day = date("2026-11-13");
    let block = f
        .engine
        .save_entry_text(day, None, 0, "Private thoughts")
        .unwrap()
        .unwrap();
    assert!(matches!(
        f.engine.set_item_private(&block.item.id, true),
        Err(Error::Invalid(_))
    ));
    let ordinary = keep(&f.engine, "A plain note");
    f.engine.set_item_private(&ordinary, true).unwrap();
}

#[test]
fn move_to_day_moves_a_note_to_the_previous_days_timeline() {
    let f = fixture();
    let note = keep(&f.engine, "Dinner was good yesterday");
    let yesterday = date("2026-11-12");
    f.engine.move_to_day(&note, yesterday).unwrap();
    assert!(
        f.engine
            .journal_day(date("2026-11-13"))
            .unwrap()
            .rows
            .is_empty()
    );
    let day = f.engine.journal_day(yesterday).unwrap();
    assert_eq!(day.rows.len(), 1);
    assert_eq!(day.rows[0].item.id, note);
    assert_eq!(
        day.rows[0].move_to.as_deref(),
        Some("2026-11-13"),
        "it can be moved back"
    );
    f.engine.move_to_day(&note, date("2026-11-13")).unwrap();
    assert_eq!(
        f.engine.journal_day(date("2026-11-13")).unwrap().rows.len(),
        1
    );
    assert!(f.engine.move_to_day(&note, date("2026-11-20")).is_err());

    let block = f
        .engine
        .save_entry_text(date("2026-11-13"), None, 0, "Entry text")
        .unwrap()
        .unwrap();
    assert!(matches!(
        f.engine.move_to_day(&block.item.id, yesterday),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn a_checkin_without_a_line_is_understood_at_once_and_one_with_a_line_waits_to_be_read() {
    let f = fixture();
    let bare = f
        .engine
        .keep_checkin(
            input(feeling(FeelingPart::LitUp, &["excited"]), None),
            Source::Journal,
        )
        .unwrap();
    let item = f.engine.get_item(&bare.id).unwrap();
    assert_eq!(item.kind, ItemKind::Checkin);
    assert_eq!(item.status, ItemStatus::Understood);
    assert_eq!(item.journal, Some(JournalRole::Checkin));
    assert_eq!(bare.day, "2026-11-13");

    let mut with_line = input(
        feeling(FeelingPart::Low, &["tired"]),
        Some("Long day with Gran"),
    );
    with_line.urge = Some("Go for a walk".into());
    let written = f.engine.keep_checkin(with_line, Source::Phone).unwrap();
    let item = f.engine.get_item(&written.id).unwrap();
    assert_eq!(item.status, ItemStatus::Kept);
    assert_eq!(
        item.text.as_deref(),
        Some("Long day with Gran\nGo for a walk")
    );
    assert_eq!(item.source, Source::Phone);
    assert_eq!(written.urge.as_deref(), Some("Go for a walk"));

    let day = f.engine.journal_day(date("2026-11-13")).unwrap();
    assert_eq!(day.checkins.len(), 2);
    assert_eq!(
        day.rows
            .iter()
            .filter(|r| r.kind == RowKind::Checkin)
            .count(),
        2
    );
    assert_eq!(day.reading, 1);
}

#[test]
fn changing_a_checkin_keeps_its_day_and_hides_the_old_one() {
    let f = fixture();
    let first = f
        .engine
        .keep_checkin(
            input(feeling(FeelingPart::Low, &["tired"]), None),
            Source::Journal,
        )
        .unwrap();
    f.clock.set_str("2026-11-14T09:00:00Z");
    let mut change = input(feeling(FeelingPart::Settled, &["calm"]), None);
    change.replaces = Some(first.id.clone());
    let second = f.engine.keep_checkin(change, Source::Journal).unwrap();
    assert_eq!(second.day, "2026-11-13");
    let day = f.engine.journal_day(date("2026-11-13")).unwrap();
    assert_eq!(day.checkins.len(), 1);
    assert_eq!(day.checkins[0].id, second.id);
    assert!(
        f.engine
            .keep_checkin(
                CheckInInput {
                    replaces: Some(first.id),
                    ..input(feeling(FeelingPart::Low, &[]), None)
                },
                Source::Journal
            )
            .is_err()
    );
}

#[test]
fn a_new_version_of_a_left_out_block_or_checkin_stays_left_out() {
    let f = fixture();
    let day = date("2026-11-13");
    let block = f
        .engine
        .save_entry_text(day, None, 0, "First words")
        .unwrap()
        .unwrap();
    read(&f, &block.item.id);
    f.engine.set_left_out(&block.item.id, true).unwrap();
    let edited = f
        .engine
        .save_entry_text(day, Some(&block.item.id), 0, "Second words")
        .unwrap()
        .unwrap();
    assert_ne!(edited.item.id, block.item.id);
    let left_out = |id: &str| -> bool {
        rusqlite::Connection::open(f.dir.path().join("nocturne.db"))
            .unwrap()
            .query_row("SELECT left_out FROM items WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap()
    };
    assert!(left_out(&edited.item.id));

    let first = f
        .engine
        .keep_checkin(
            input(feeling(FeelingPart::Low, &["tired"]), None),
            Source::Journal,
        )
        .unwrap();
    f.engine.set_left_out(&first.id, true).unwrap();
    let mut change = input(feeling(FeelingPart::Settled, &["calm"]), None);
    change.replaces = Some(first.id.clone());
    let second = f.engine.keep_checkin(change, Source::Journal).unwrap();
    assert!(second.left_out);
}

#[test]
fn a_feeling_takes_up_to_three_words_inside_the_field() {
    let f = fixture();
    let many = feeling(FeelingPart::Low, &["a", "b", "c", "d"]);
    assert!(
        f.engine
            .keep_checkin(input(many, None), Source::Journal)
            .is_err()
    );
    let mut off = feeling(FeelingPart::Low, &[]);
    off.point.energy = 1.5;
    assert!(
        f.engine
            .keep_checkin(input(off, None), Source::Journal)
            .is_err()
    );
}

#[test]
fn top_feeling_words_orders_by_use_and_is_empty_without_history() {
    let f = fixture();
    assert!(
        f.engine
            .top_feeling_words(FeelingPart::Settled)
            .unwrap()
            .is_empty()
    );
    for words in [
        &["calm", "content"][..],
        &["calm"],
        &["calm", "content", "relieved"],
        &["relieved"],
        &["content", "mellow", "serene"],
    ] {
        f.engine
            .keep_checkin(
                input(feeling(FeelingPart::Settled, words), None),
                Source::Journal,
            )
            .unwrap();
    }
    f.engine
        .keep_checkin(
            input(feeling(FeelingPart::Low, &["tired"]), None),
            Source::Journal,
        )
        .unwrap();
    assert_eq!(
        f.engine.top_feeling_words(FeelingPart::Settled).unwrap(),
        ["calm", "content", "relieved", "mellow"]
    );
    assert_eq!(
        f.engine.top_feeling_words(FeelingPart::Low).unwrap(),
        ["tired"]
    );
    assert!(
        f.engine
            .top_feeling_words(FeelingPart::WoundUp)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn trying_an_idea_is_noted_once_on_the_checkin() {
    let f = fixture();
    let c = f
        .engine
        .keep_checkin(
            input(feeling(FeelingPart::WoundUp, &["tense"]), None),
            Source::Journal,
        )
        .unwrap();
    f.engine.mark_tried(&c.id, "slow-breathing").unwrap();
    f.engine.mark_tried(&c.id, "walk").unwrap();
    f.engine.mark_tried(&c.id, "walk").unwrap();
    let day = f.engine.journal_day(date("2026-11-13")).unwrap();
    assert_eq!(day.checkins[0].tried, ["slow-breathing", "walk"]);
    let note = keep(&f.engine, "Not a check-in");
    assert!(f.engine.mark_tried(&note, "walk").is_err());
}

#[test]
fn photos_and_voice_notes_become_blocks_of_the_entry_and_a_photo_can_be_described() {
    let f = fixture();
    let day = date("2026-11-13");
    let Kept::New { item: photo } = f
        .engine
        .keep_bytes(b"jpeg", "jpg", Source::Journal)
        .unwrap()
    else {
        panic!("new")
    };
    let Kept::New { item: voice } = f
        .engine
        .keep_bytes(b"webm", "webm", Source::Journal)
        .unwrap()
    else {
        panic!("new")
    };
    f.engine
        .save_entry_text(day, None, 0, "Words first")
        .unwrap();
    let block = f.engine.place_in_entry(&photo.id, day, 1).unwrap();
    assert_eq!(block.block, 1);
    assert_eq!(block.item.journal, Some(JournalRole::Entry));
    f.engine.place_in_entry(&voice.id, day, 2).unwrap();
    assert!(
        f.engine.place_in_entry(&photo.id, day, 3).is_err(),
        "placed once"
    );
    f.engine
        .describe_photo(&photo.id, "The allotment in rain")
        .unwrap();
    let view = f.engine.journal_day(day).unwrap();
    assert_eq!(view.entry.len(), 3);
    assert_eq!(view.entry[1].alt.as_deref(), Some("The allotment in rain"));
    assert!(view.entry_saved_at.is_some());
    assert!(view.entry_read_at.is_none(), "not read yet");
    f.engine.describe_photo(&photo.id, " ").unwrap();
    assert_eq!(f.engine.journal_day(day).unwrap().entry[1].alt, None);
}

#[test]
fn a_voice_note_kept_into_the_entry_is_a_held_block_from_the_start() {
    let f = fixture();
    let day = date("2026-11-13");
    let Kept::New { item } = f
        .engine
        .keep_bytes_in_entry(b"wav-voice", "wav", Source::Journal, day, 1)
        .unwrap()
    else {
        panic!("new")
    };
    assert_eq!(item.journal, Some(JournalRole::Entry));
    assert!(
        f.engine.pending_items(10).unwrap().is_empty(),
        "held like a saved block, never an ordinary item waiting to be read"
    );
    assert_eq!(f.engine.journal_day(day).unwrap().entry[0].block, 1);

    let file = write_file(f.dir.path(), "allotment.jpg", b"jpeg-allotment");
    let Kept::New { item: photo } = f.engine.keep_file(&file, Source::Drop).unwrap() else {
        panic!("new")
    };
    let Kept::Duplicate { item: placed } = f
        .engine
        .keep_file_in_entry(&file, Source::Journal, day, 2)
        .unwrap()
    else {
        panic!("the same bytes")
    };
    assert_eq!(placed.id, photo.id);
    assert_eq!(placed.journal, Some(JournalRole::Entry));
    assert!(
        f.engine
            .keep_bytes_in_entry(b"wav-voice", "wav", Source::Journal, day, 3)
            .is_err(),
        "already in the entry"
    );
    assert!(
        f.engine
            .keep_bytes_in_entry(b"wav-later", "wav", Source::Journal, date("2026-11-14"), 0)
            .is_err()
    );
    assert_eq!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM items"), 2);
}

#[test]
fn leaving_an_item_out_is_reversible_and_marked_on_its_row() {
    let f = fixture();
    let note = keep(&f.engine, "A thing to leave out");
    f.engine.set_left_out(&note, true).unwrap();
    assert!(f.engine.journal_day(date("2026-11-13")).unwrap().rows[0].left_out);
    f.engine.set_left_out(&note, false).unwrap();
    assert!(!f.engine.journal_day(date("2026-11-13")).unwrap().rows[0].left_out);
}

#[test]
fn the_first_use_card_shows_until_dismissed_and_things_elsewhere_are_background() {
    let f = fixture();
    let day = date("2026-11-13");
    assert!(f.engine.journal_day(day).unwrap().first_use);
    f.engine.dismiss_journal_intro().unwrap();
    assert!(!f.engine.journal_day(day).unwrap().first_use);
    let file = write_file(f.dir.path(), "letter.txt", b"School trip form");
    f.engine.keep_file(&file, Source::WatchedFolder).unwrap();
    let rows = f.engine.journal_day(day).unwrap().rows;
    assert_eq!(rows[0].kind, RowKind::Elsewhere);
}

#[test]
fn days_and_periods_list_what_was_kept_newest_first() {
    let f = fixture();
    keep(&f.engine, "Friday");
    f.clock.set_str("2026-11-10T10:00:00Z");
    keep(&f.engine, "Tuesday");
    f.clock.set_str("2026-10-01T10:00:00Z");
    keep(&f.engine, "October");
    f.clock.set_str("2026-11-13T07:00:00Z");
    assert_eq!(
        f.engine.journal_days().unwrap(),
        [date("2026-11-13"), date("2026-11-10"), date("2026-10-01")]
    );
    let weeks = f.engine.periods(PeriodKind::Week).unwrap();
    assert_eq!(weeks.len(), 2, "the Tuesday and the Friday share a week");
    assert_eq!(weeks[0].period.start, "2026-11-09");
    assert_eq!(weeks[0].period.end, "2026-11-15");
    assert!(!weeks[0].written);
    let months = f.engine.periods(PeriodKind::Month).unwrap();
    assert_eq!(months.len(), 2);
    assert_eq!(months[1].period.start, "2026-10-01");
    assert_eq!(months[1].period.end, "2026-10-31");
    let year = f
        .engine
        .period_of(PeriodKind::Year, date("2026-11-13"))
        .unwrap();
    assert_eq!(
        (year.start.as_str(), year.end.as_str()),
        ("2026-01-01", "2026-12-31")
    );
    let week = f
        .engine
        .period_of(PeriodKind::Week, date("2026-11-15"))
        .unwrap();
    assert_eq!(
        week.start, "2026-11-09",
        "a Sunday belongs to the week before it"
    );
}

fn mention(f: &Fixture, item: &str, entity: &str, name: &str, new: bool) {
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO entities(id, kind, name, key, role) VALUES (?1, 'person', ?2, ?3, 'Grandmother')",
        rusqlite::params![entity, name, name.to_lowercase()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO mentions(id, item_id, entity_id, name, start, end, ord, state, new, created_at) \
         VALUES (?1, ?2, ?3, ?4, 0, ?5, 0, 'found', ?6, '2026-11-13T07:00:00.000Z')",
        rusqlite::params![
            format!("men_{item}_{entity}"),
            item,
            entity,
            name,
            name.len() as u32,
            new
        ],
    )
    .unwrap();
}

#[test]
fn names_found_in_the_day_are_listed_once_with_the_items_that_mention_them() {
    let f = fixture();
    let a = keep(&f.engine, "Gran came by");
    let b = keep(&f.engine, "Gran stayed for tea");
    mention(&f, &a, "ent_gran", "Gran", true);
    mention(&f, &b, "ent_gran", "Gran", false);
    let day = f.engine.journal_day(date("2026-11-13")).unwrap();
    assert_eq!(day.people.len(), 1);
    let gran = &day.people[0];
    assert_eq!(gran.name, "Gran");
    assert_eq!(gran.kind, EntityKind::Person);
    assert!(gran.new);
    assert_eq!(gran.item_ids.len(), 2);
    assert_eq!(day.rows[0].mentions[0].name, "Gran");
    assert!(day.same_name.is_empty());
    assert_eq!(
        f.engine.entity_journal_days("ent_gran").unwrap(),
        [date("2026-11-13")]
    );
}

#[test]
fn the_evening_reminder_is_off_until_turned_on_and_takes_a_time_of_day() {
    let f = fixture();
    let off = f.engine.evening_reminder().unwrap();
    assert!(!off.on);
    assert_eq!(off.time, "21:00");
    assert_eq!(f.engine.settings().unwrap().evening_reminder, Some(off));

    let on = nocturne_contract::EveningReminder {
        on: true,
        time: "20:30".into(),
    };
    let view = f
        .engine
        .update_settings(SettingsPatch {
            evening_reminder: Some(on.clone()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(view.evening_reminder, Some(on.clone()));
    assert_eq!(f.engine.evening_reminder().unwrap(), on);
    assert_eq!(
        f.engine.settings().unwrap().page_time,
        "07:00",
        "nothing else changed"
    );

    let bad = nocturne_contract::EveningReminder {
        on: true,
        time: "25:99".into(),
    };
    assert!(
        f.engine
            .update_settings(SettingsPatch {
                evening_reminder: Some(bad),
                ..Default::default()
            })
            .is_err()
    );
    assert_eq!(f.engine.evening_reminder().unwrap(), on);
}

#[test]
fn a_private_photo_cannot_be_placed_in_the_journal() {
    let f = fixture();
    let day = date("2026-11-13");
    let Kept::New { item: photo } = f
        .engine
        .keep_bytes(b"jpeg-bytes", "jpg", Source::Journal)
        .unwrap()
    else {
        panic!("new")
    };
    f.engine.set_item_private(&photo.id, true).unwrap();
    assert!(matches!(
        f.engine.place_in_entry(&photo.id, day, 1),
        Err(Error::Invalid(_))
    ));
    assert_eq!(f.engine.get_item(&photo.id).unwrap().journal, None);
}
