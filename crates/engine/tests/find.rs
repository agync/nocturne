mod common;

use common::{fixture, keep};
use jiff::SignedDuration;
use nocturne_contract::{
    CheckInInput, Feeling, FeelingPart, FeelingPoint, ItemKind, RemoteConnection, Source,
};
use nocturne_engine::{Clock, Engine, FIND_CAP, FoundItem, KeepPlace};

fn ids(found: &[FoundItem]) -> Vec<&str> {
    found.iter().map(|f| f.receipt.item_id.as_str()).collect()
}

fn checkin(engine: &Engine, line: &str) -> String {
    engine
        .keep_checkin(
            CheckInInput {
                feeling: Feeling {
                    part: FeelingPart::Settled,
                    point: FeelingPoint {
                        pleasant: 0.5,
                        energy: 0.5,
                    },
                    words: vec!["calm".into()],
                },
                line: Some(line.into()),
                urge: None,
                replaces: None,
            },
            Source::Phone,
        )
        .unwrap()
        .id
}

#[test]
fn an_unread_note_is_found_with_its_words_marked() {
    let f = fixture();
    let id = keep(&f.engine, "The plumber is coming on Tuesday morning");
    keep(&f.engine, "Buy oat milk");

    let found = f.engine.find_items("plumber", 10).unwrap();
    assert_eq!(ids(&found), [id.as_str()]);
    let hit = &found[0];
    assert_eq!(hit.receipt.kind, ItemKind::Text);
    assert!(!hit.private);
    assert!(!hit.receipt.unconfirmed);
    let marked: Vec<&str> = hit
        .excerpt
        .iter()
        .filter(|p| p.matched)
        .map(|p| p.text.as_str())
        .collect();
    assert_eq!(marked, ["plumber"]);
    let whole: String = hit.excerpt.iter().map(|p| p.text.as_str()).collect();
    assert_eq!(whole, "The plumber is coming on Tuesday morning");
}

#[test]
fn word_endings_do_not_hide_a_match() {
    let f = fixture();
    let id = keep(&f.engine, "The plumbers were late");
    assert_eq!(
        ids(&f.engine.find_items("plumber", 10).unwrap()),
        [id.as_str()]
    );
    assert_eq!(
        ids(&f.engine.find_items("PLUMBERS late?", 10).unwrap()),
        [id.as_str()]
    );
}

#[test]
fn a_private_item_is_found_and_says_so() {
    let f = fixture();
    let id = keep(&f.engine, "My doctor said the results are fine");
    f.engine.set_item_private(&id, true).unwrap();

    let found = f.engine.find_items("doctor", 10).unwrap();
    assert_eq!(ids(&found), [id.as_str()]);
    assert!(found[0].private);
}

#[test]
fn a_check_in_is_never_found() {
    let f = fixture();
    checkin(&f.engine, "Lantern evening, feeling steady");
    assert!(f.engine.find_items("lantern", 10).unwrap().is_empty());

    let note = keep(&f.engine, "Lantern festival is in March");
    assert_eq!(
        ids(&f.engine.find_items("lantern", 10).unwrap()),
        [note.as_str()]
    );
}

#[test]
fn a_journal_entry_is_found() {
    let f = fixture();
    let day = "2026-11-13".parse().unwrap();
    let entry = f
        .engine
        .save_entry_text(day, None, 0, "Gran came for tea")
        .unwrap()
        .unwrap();
    let found = f.engine.find_items("gran", 10).unwrap();
    assert_eq!(ids(&found), [entry.item.id.as_str()]);
    assert_eq!(found[0].receipt.source, Source::Journal);
}

#[test]
fn a_forgotten_item_is_never_found_not_even_while_it_is_going() {
    let f = fixture();
    let gone = keep(&f.engine, "Walnut tree pruning in January");
    let going = keep(&f.engine, "Walnut bread recipe from Gran");
    let stays = keep(&f.engine, "Walnut oil is in the cupboard");
    assert_eq!(f.engine.find_items("walnut", 10).unwrap().len(), 3);

    f.engine.unkeep(&gone).unwrap();
    f.engine.unkeep_begin_only(&going).unwrap();

    assert_eq!(
        ids(&f.engine.find_items("walnut", 10).unwrap()),
        [stays.as_str()]
    );
}

#[test]
fn a_reading_from_a_phone_is_not_a_thing_kept() {
    let f = fixture();
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    conn.execute(
        "INSERT INTO items(id, kind, source, trust, sha256, title, kept_at, status, text) \
         VALUES ('r1', 'reading', 'phone', 'trusted', 'aa', 'Steps', '2026-11-13T07:00:00Z', \
         'understood', 'Steps today 8421')",
        [],
    )
    .unwrap();
    drop(conn);
    // The write above bypassed the engine's own connections, so reopen to be sure it is seen.
    let engine = Engine::open(f.dir.path(), f.clock.clone()).unwrap();
    assert!(engine.find_items("steps", 10).unwrap().is_empty());
}

#[test]
fn the_best_match_comes_first() {
    let f = fixture();
    let plain = keep(&f.engine, "Please remember the thermostat today");
    let best = keep(&f.engine, "Thermostat thermostat thermostat");

    let found = f.engine.find_items("thermostat", 10).unwrap();
    assert_eq!(ids(&found), [best.as_str(), plain.as_str()]);
}

#[test]
fn equal_matches_are_newest_first() {
    let f = fixture();
    let mut made = Vec::new();
    for i in 0..3 {
        made.push(keep(&f.engine, &format!("Bus pass number {i}")));
        f.clock.set(
            f.clock
                .now()
                .checked_add(SignedDuration::from_hours(1))
                .unwrap(),
        );
    }
    let found = f.engine.find_items("bus pass", 10).unwrap();
    let got = ids(&found);
    let want: Vec<&str> = made.iter().rev().map(String::as_str).collect();
    assert_eq!(got, want);
}

#[test]
fn a_search_returns_at_most_the_cap_and_at_most_the_limit() {
    let f = fixture();
    for i in 0..(FIND_CAP + 5) {
        keep(&f.engine, &format!("Parking ticket {i}"));
    }
    assert_eq!(f.engine.find_items("parking", 100).unwrap().len(), FIND_CAP);
    assert_eq!(f.engine.find_items("parking", 4).unwrap().len(), 4);
    assert!(f.engine.find_items("parking", 0).unwrap().is_empty());
}

#[test]
fn a_query_with_no_words_finds_nothing() {
    let f = fixture();
    keep(&f.engine, "Anything at all");
    assert!(f.engine.find_items("", 10).unwrap().is_empty());
    assert!(f.engine.find_items("  ?! ", 10).unwrap().is_empty());
    assert!(
        f.engine
            .find_items("nothing matches this", 10)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_long_note_gives_an_excerpt_around_the_match() {
    let f = fixture();
    let filler = "alpha beta gamma delta ".repeat(30);
    keep(
        &f.engine,
        &format!("{filler}the saffron jar is empty {filler}"),
    );
    let found = f.engine.find_items("saffron", 5).unwrap();
    let whole: String = found[0].excerpt.iter().map(|p| p.text.as_str()).collect();
    assert!(whole.contains("saffron jar"));
    assert!(whole.starts_with('…') && whole.ends_with('…'));
    assert!(whole.chars().count() < 400);
}

#[test]
fn a_kept_thing_from_a_paired_phone_names_it_and_the_phones_own_row_does_not() {
    let f = fixture();
    let paired = f
        .engine
        .add_device(
            "Pixel",
            &[7; 8],
            "https://h.test",
            RemoteConnection::Simple,
            "a",
        )
        .unwrap()
        .id;
    let local = f.engine.ensure_local_device("This phone").unwrap();
    for (device, client, text) in [(&paired, "c1", "Ticket one"), (&local, "c2", "Ticket two")] {
        f.engine
            .keep_note_item(
                text,
                Source::Phone,
                false,
                &KeepPlace {
                    day: None,
                    phone: Some((device, client)),
                },
            )
            .unwrap();
    }
    let found = f.engine.find_items("ticket", 10).unwrap();
    let from: Vec<(String, Option<String>)> = found
        .iter()
        .map(|f| (f.receipt.label.clone(), f.receipt.from_device.clone()))
        .collect();
    assert!(
        from.contains(&("Ticket one".into(), Some("Pixel".into()))),
        "{from:?}"
    );
    assert!(from.contains(&("Ticket two".into(), None)), "{from:?}");
}

#[test]
fn a_found_thing_maps_to_the_contract_type() {
    let f = fixture();
    keep(&f.engine, "The plumber is coming on Tuesday morning");
    let found = f.engine.find_items("plumber", 10).unwrap().remove(0);
    let receipt = found.receipt.clone();
    let mapped: nocturne_contract::FoundItem = found.into();
    assert_eq!(mapped.receipt, receipt);
    assert!(!mapped.private);
    let whole: String = mapped.excerpt.iter().map(|p| p.text.as_str()).collect();
    assert_eq!(whole, "The plumber is coming on Tuesday morning");
    assert!(
        mapped
            .excerpt
            .iter()
            .any(|p| p.matched && p.text == "plumber")
    );
}
