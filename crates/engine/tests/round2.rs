//! Round two: originals on item views, item filters, entity search, undo, pasted bytes, item
//! flags, snooze, prioritise and the export callback.

mod common;

use common::*;
use nocturne_contract::{EntityKind, ItemFilter, Kept, MemoryFilter, Source};
use nocturne_engine::Predicate;

fn kept_item(k: Kept) -> nocturne_contract::ItemView {
    match k {
        Kept::New { item } | Kept::Duplicate { item } => item,
    }
}

// ---- original_path ----

#[test]
fn item_views_name_the_stored_original_when_it_exists() {
    let f = fixture();
    let src = write_file(f.dir.path(), "scan.png", b"\x89PNG pretend image");
    let kept = kept_item(f.engine.keep_file(&src, Source::Drop).unwrap());
    let originals = f.dir.path().join("originals");

    let path = std::path::PathBuf::from(kept.original_path.clone().expect("path on Kept::New"));
    assert!(path.is_absolute());
    assert_eq!(path.parent().unwrap(), originals);
    assert_eq!(path.extension().unwrap(), "png");
    assert_eq!(path.file_stem().unwrap().len(), 64);
    assert!(path.exists());
    assert_eq!(std::fs::read(&path).unwrap(), b"\x89PNG pretend image");

    let got = f.engine.get_item(&kept.id).unwrap();
    assert_eq!(got.original_path, kept.original_path);
    let listed = f.engine.items(ItemFilter::default()).unwrap();
    assert_eq!(listed[0].original_path, kept.original_path);

    // Keeping the same file again is a duplicate that names the same original.
    let again = kept_item(f.engine.keep_file(&src, Source::Drop).unwrap());
    assert_eq!(again.original_path, kept.original_path);
}

#[test]
fn item_views_have_no_original_path_for_notes_or_missing_files() {
    let f = fixture();
    let note = keep(&f.engine, "just a note");
    assert_eq!(f.engine.get_item(&note).unwrap().original_path, None);

    let src = write_file(f.dir.path(), "card.jpg", b"jpeg bytes");
    let kept = kept_item(f.engine.keep_file(&src, Source::Drop).unwrap());
    let path = kept.original_path.clone().unwrap();
    std::fs::remove_file(&path).unwrap();
    let got = f.engine.get_item(&kept.id).unwrap();
    assert_eq!(got.original_path, None);
    assert!(got.has_original, "the row still says it was a file");
}

// ---- MemoryFilter.item_id ----

#[test]
fn memories_filter_by_source_item() {
    let f = fixture();
    let (a, ma) = learn(
        &f.engine,
        "note a",
        cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming."),
    );
    let (b, mb) = learn(
        &f.engine,
        "note b",
        cand("Mia", Predicate::Prefers, "chess", "Mia likes chess."),
    );
    let only_a = f
        .engine
        .memories(MemoryFilter {
            item_id: Some(a.clone()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(only_a.len(), 1);
    assert_eq!(only_a[0].id, ma);

    // A memory with two sources shows under both items.
    let c = keep(&f.engine, "note c");
    f.engine
        .apply(
            &c,
            vec![cand("Mia", Predicate::Prefers, "chess", "Mia likes chess.")],
        )
        .unwrap();
    for item in [&b, &c] {
        let got = f
            .engine
            .memories(MemoryFilter {
                item_id: Some(item.clone()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, mb);
    }

    // Superseded memories are history, not current.
    let new = f
        .engine
        .correct(&ma, nocturne_engine::CorrectPatch::object("rowing"))
        .unwrap();
    let after = f
        .engine
        .memories(MemoryFilter {
            item_id: Some(a),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, new);

    // An unknown item matches nothing.
    assert!(
        f.engine
            .memories(MemoryFilter {
                item_id: Some("itm_nope".into()),
                ..Default::default()
            })
            .unwrap()
            .is_empty()
    );
}

// ---- entities_matching ----

fn set_role(f: &Fixture, name: &str, role: &str) {
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    conn.execute(
        "UPDATE entities SET role = ?2 WHERE name = ?1",
        rusqlite::params![name, role],
    )
    .unwrap();
}

fn person(name: &str, pred: Predicate, obj: &str, sentence: &str) -> nocturne_engine::Candidate {
    let mut c = cand(name, pred, obj, sentence);
    c.subject_kind = Some(EntityKind::Person);
    c
}

#[test]
fn entities_matching_searches_name_and_role_and_ranks_by_memory_count() {
    let f = fixture();
    learn(
        &f.engine,
        "n1",
        person("Leo", Predicate::Prefers, "swimming", "Leo likes swimming."),
    );
    learn(
        &f.engine,
        "n2",
        person("Leo", Predicate::Knows, "Sam", "Leo knows Sam."),
    );
    learn(
        &f.engine,
        "n3",
        person("Leona Park", Predicate::Prefers, "tea", "Leona likes tea."),
    );
    learn(
        &f.engine,
        "n4",
        person("Mrs Hale", Predicate::Knows, "Leo", "Mrs Hale knows Leo."),
    );
    let mut place = cand("Pool", Predicate::Knows, "open", "The pool is open.");
    place.subject_kind = Some(EntityKind::Place);
    learn(&f.engine, "n5", place);
    set_role(&f, "Mrs Hale", "Leo's TEACHER");

    let names = |kind, q: Option<&str>| -> Vec<String> {
        f.engine
            .entities_matching(kind, q)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect()
    };
    // Name substring, case-insensitive, most memories first.
    assert_eq!(names(None, Some("LEO")), ["Leo", "Leona Park", "Mrs Hale"]);
    // Role substring.
    assert_eq!(names(None, Some("teacher")), ["Mrs Hale"]);
    // Kind narrows.
    assert_eq!(names(Some(EntityKind::Place), Some("o")), ["Pool"]);
    assert_eq!(
        names(Some(EntityKind::Place), Some("leo")),
        Vec::<String>::new()
    );
    // No query: everything of the kind, ties by name.
    assert_eq!(
        names(Some(EntityKind::Person), None),
        ["Leo", "Leona Park", "Mrs Hale"]
    );
    assert_eq!(names(None, Some("   ")).len(), 4);
    // The wrapper keeps its old order guarantee of finding the same entities.
    assert_eq!(
        f.engine.entities(Some(EntityKind::Person)).unwrap().len(),
        3
    );
    // Non-ASCII folds too.
    learn(
        &f.engine,
        "n6",
        person("Zoë", Predicate::Knows, "x", "Zoë knows x."),
    );
    assert_eq!(names(None, Some("ZOË")), ["Zoë"]);
}

// ---- keep_bytes ----

#[test]
fn keep_bytes_stores_a_private_original_and_follows_keep_file() {
    let f = fixture();
    let item = kept_item(
        f.engine
            .keep_bytes(b"\x89PNG pasted", "PNG", Source::Paste)
            .unwrap(),
    );
    assert_eq!(item.kind, nocturne_contract::ItemKind::Image);
    assert_eq!(item.trust, nocturne_contract::Trust::Trusted);
    assert_eq!(item.status, nocturne_contract::ItemStatus::Kept);
    assert!(item.has_original);
    assert_eq!(item.file_name.as_deref(), Some("Pasted image.png"));
    let path = std::path::PathBuf::from(item.original_path.clone().unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), b"\x89PNG pasted");
    assert_eq!(path.parent().unwrap(), f.dir.path().join("originals"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    // No temporary file is left behind.
    for e in std::fs::read_dir(f.dir.path().join("originals")).unwrap() {
        let name = e.unwrap().file_name().to_string_lossy().into_owned();
        assert!(!name.starts_with(".tmp-"), "{name}");
    }

    // The same bytes again are a duplicate, whatever the source or a leading dot.
    match f
        .engine
        .keep_bytes(b"\x89PNG pasted", ".png", Source::Screenshot)
        .unwrap()
    {
        Kept::Duplicate { item: d } => assert_eq!(d.id, item.id),
        Kept::New { .. } => panic!("expected a duplicate"),
    }
    assert_eq!(f.engine.items(ItemFilter::default()).unwrap().len(), 1);

    // It is read like any file: the queue hands out the stored original.
    let leased = f
        .engine
        .lease_pending(1, std::time::Duration::from_secs(60))
        .unwrap();
    assert_eq!(leased[0].id, item.id);
    assert_eq!(leased[0].original.as_deref(), Some(path.as_path()));
    assert_eq!(leased[0].mime.as_deref(), Some("image/png"));
}

#[test]
fn keep_bytes_takes_kind_from_the_extension_and_trust_from_the_source() {
    let f = fixture();
    let rec = kept_item(
        f.engine
            .keep_bytes(b"audio bytes", "m4a", Source::Recording)
            .unwrap(),
    );
    assert_eq!(rec.kind, nocturne_contract::ItemKind::Audio);
    assert_eq!(rec.file_name.as_deref(), Some("Recording.m4a"));
    let agent = kept_item(
        f.engine
            .keep_bytes(b"agent image", "jpg", Source::Agent)
            .unwrap(),
    );
    assert_eq!(agent.trust, nocturne_contract::Trust::Untrusted);
}

#[test]
fn keep_bytes_rejects_unsupported_kinds_and_empty_input_without_writing() {
    let f = fixture();
    for (bytes, ext) in [
        (&b"MZ"[..], "exe"),
        (b"x", ""),
        (b"x", "png/../x"),
        (b"", "png"),
    ] {
        let err = f.engine.keep_bytes(bytes, ext, Source::Paste).unwrap_err();
        assert!(
            matches!(err, nocturne_engine::Error::Invalid(_)),
            "{ext}: {err:?}"
        );
    }
    assert_eq!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM items"), 0);
    assert_eq!(
        std::fs::read_dir(f.dir.path().join("originals"))
            .unwrap()
            .count(),
        0
    );
}

// ---- prioritize ----

fn lease_ids(f: &Fixture, n: usize) -> Vec<String> {
    f.engine
        .lease_pending(n, std::time::Duration::from_secs(60))
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect()
}

#[test]
fn a_prioritised_item_is_leased_first_and_the_latest_request_goes_first() {
    let f = fixture();
    let a = keep(&f.engine, "note a");
    f.clock.advance_days(1);
    let b = keep(&f.engine, "note b");
    f.clock.advance_days(1);
    let c = keep(&f.engine, "note c");
    f.engine.prioritize(&c).unwrap();
    assert_eq!(f.engine.pending_items(3).unwrap()[0].id, c);
    f.engine.prioritize(&b).unwrap();
    assert_eq!(lease_ids(&f, 3), [b.clone(), c.clone(), a.clone()]);

    assert!(matches!(
        f.engine.prioritize("itm_nope"),
        Err(nocturne_engine::Error::NotFound(_))
    ));
}

#[test]
fn priority_does_not_outlive_a_lease() {
    let f = fixture();
    let a = keep(&f.engine, "note a");
    f.clock.advance_days(1);
    let b = keep(&f.engine, "note b");
    f.engine.prioritize(&b).unwrap();
    assert_eq!(lease_ids(&f, 1), vec![b.clone()]);
    // The lease runs out; b is waiting again, in its ordinary place.
    f.clock.advance_days(1);
    assert_eq!(lease_ids(&f, 2), [a, b]);
}

#[test]
fn retry_item_accepts_waiting_items_and_puts_them_first() {
    let f = fixture();
    let a = keep(&f.engine, "note a");
    f.clock.advance_days(1);
    let b = keep(&f.engine, "note b");
    f.engine.retry_item(&b).unwrap();
    assert_eq!(f.engine.pending_items(2).unwrap()[0].id, b);
    assert_eq!(
        f.engine.get_item(&b).unwrap().status,
        nocturne_contract::ItemStatus::Kept
    );

    // An unreadable one is queued again, also first.
    let p = f
        .engine
        .lease_item(&a, std::time::Duration::from_secs(60))
        .unwrap();
    f.engine
        .fail_item(&a, &p.lease, nocturne_contract::CouldntReadReason::Corrupt)
        .unwrap();
    f.engine.retry_item(&a).unwrap();
    assert_eq!(f.engine.pending_items(2).unwrap()[0].id, a);

    assert!(matches!(
        f.engine.retry_item("itm_nope"),
        Err(nocturne_engine::Error::NotFound(_))
    ));
}

// ---- migration 3 ----

#[test]
fn a_store_at_version_two_upgrades_in_place() {
    let dir = tempfile::tempdir().unwrap();
    {
        let conn = rusqlite::Connection::open(dir.path().join("nocturne.db")).unwrap();
        for (version, sql) in nocturne_engine::store::migrations() {
            if version <= 2 {
                conn.execute_batch(sql).unwrap();
                conn.pragma_update(None, "user_version", version).unwrap();
            }
        }
        conn.execute(
            "INSERT INTO items(id, kind, source, trust, sha256, title, kept_at, status) \
             VALUES ('itm_old', 'text', 'quickCapture', 'trusted', 'abc', 't', \
             '2026-11-01T00:00:00.000Z', 'kept')",
            [],
        )
        .unwrap();
    }
    let clock = std::sync::Arc::new(nocturne_engine::ManualClock::at(
        "2026-11-13T07:00:00Z",
        "Europe/London",
    ));
    let engine = nocturne_engine::Engine::open(dir.path(), clock).unwrap();
    assert_eq!(engine.get_item("itm_old").unwrap().title, "t");
    let conn = rusqlite::Connection::open(dir.path().join("nocturne.db")).unwrap();
    let v: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, nocturne_engine::store::latest_version());
    assert!(v >= 3);
    for (table, column) in [("items", "priority"), ("memories", "snoozed_until")] {
        let n: u32 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"),
                [column],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "{table}.{column}");
    }
}

// ---- snooze ----

fn unconfirmed(f: &Fixture, text: &str, subject: &str) -> String {
    let item = keep_as(&f.engine, text, Source::PastedWeb);
    let r = f
        .engine
        .apply(
            &item,
            vec![cand(
                subject,
                Predicate::Knows,
                "x",
                &format!("{subject} knows x."),
            )],
        )
        .unwrap();
    assert_eq!(r.unconfirmed.len(), 1);
    r.created[0].clone()
}

fn snoozed_count(f: &Fixture) -> i64 {
    sql_count(
        f.dir.path(),
        "SELECT COUNT(*) FROM memories WHERE snoozed_until IS NOT NULL",
    )
}

#[test]
fn a_snoozed_memory_leaves_needs_you_until_its_date() {
    let f = fixture();
    let m = unconfirmed(&f, "web one", "Alpha");
    unconfirmed(&f, "web two", "Beta");
    assert_eq!(f.engine.needs_you().unwrap().len(), 2);
    assert_eq!(f.engine.needs_you_public().unwrap().len(), 2);

    f.engine.snooze(&m, 3).unwrap();
    assert_eq!(f.engine.needs_you().unwrap().len(), 1);
    assert_eq!(f.engine.needs_you_public().unwrap().len(), 1);

    // Still hidden on the last snoozed day, back on the date itself.
    f.clock.advance_days(2);
    assert_eq!(f.engine.needs_you().unwrap().len(), 1);
    f.clock.advance_days(1);
    assert_eq!(f.engine.needs_you().unwrap().len(), 2);
    assert_eq!(f.engine.needs_you_public().unwrap().len(), 2);

    // Zero days is no snooze. An unknown memory is refused.
    f.engine.snooze(&m, 0).unwrap();
    assert_eq!(f.engine.needs_you().unwrap().len(), 2);
    assert!(matches!(
        f.engine.snooze("mem_nope", 1),
        Err(nocturne_engine::Error::NotFound(_))
    ));
}

#[test]
fn snooze_counts_local_days() {
    // 23:30 in London is already tomorrow in Auckland: the date is the local one.
    let f = fixture_at("2026-11-13T23:30:00Z", "Pacific/Auckland");
    let m = unconfirmed(&f, "web one", "Alpha");
    f.engine.snooze(&m, 1).unwrap();
    let until: String = {
        let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
        conn.query_row("SELECT snoozed_until FROM memories", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(until, "2026-11-15");
}

#[test]
fn confirming_or_correcting_clears_a_snooze() {
    let f = fixture();
    let m = unconfirmed(&f, "web one", "Alpha");
    f.engine.snooze(&m, 30).unwrap();
    assert_eq!(snoozed_count(&f), 1);
    f.engine.confirm(&m, false).unwrap();
    assert_eq!(snoozed_count(&f), 0);
    assert_eq!(f.engine.needs_you().unwrap().len(), 1);

    f.engine.snooze(&m, 30).unwrap();
    assert!(f.engine.needs_you().unwrap().is_empty());
    let new = f
        .engine
        .correct(&m, nocturne_engine::CorrectPatch::object("y"))
        .unwrap();
    assert_eq!(snoozed_count(&f), 0);
    // The corrected memory is confirmed, so it no longer waits; the old one is history.
    assert!(f.engine.needs_you().unwrap().is_empty());
    assert!(f.engine.snooze(&m, 1).is_err(), "history cannot be snoozed");
    let _ = new;
}

#[test]
fn a_new_conflict_clears_the_snooze_on_both_sides() {
    let f = fixture();
    let (_, a) = learn(
        &f.engine,
        "Dentist Tuesday",
        cand(
            "The dentist appointment",
            Predicate::Scheduled,
            "Tuesday 14:00",
            "The dentist appointment is Tuesday at 14:00.",
        ),
    );
    f.engine.snooze(&a, 30).unwrap();
    assert_eq!(snoozed_count(&f), 1);
    let item_b = keep(&f.engine, "Dentist moved");
    let r = f
        .engine
        .apply(
            &item_b,
            vec![cand(
                "dentist appointment",
                Predicate::Scheduled,
                "Thursday 10:00",
                "The dentist appointment is Thursday at 10:00.",
            )],
        )
        .unwrap();
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(snoozed_count(&f), 0);
    assert_eq!(f.engine.needs_you().unwrap().len(), 1);

    // A snoozed side hides the choice until its date.
    f.engine.snooze(&a, 2).unwrap();
    assert!(f.engine.needs_you().unwrap().is_empty());
    assert!(f.engine.needs_you_public().unwrap().is_empty());
    f.clock.advance_days(2);
    assert_eq!(f.engine.needs_you().unwrap().len(), 1);
}

#[test]
fn a_snoozed_memory_stays_off_the_morning_page_until_its_date() {
    let f = fixture();
    let m = unconfirmed(&f, "web one", "Alpha");
    f.engine.snooze(&m, 2).unwrap();
    let page = f
        .engine
        .compose_page(date("2026-11-14"), nocturne_contract::Edition::Screen)
        .unwrap();
    assert!(page.needs_you.is_empty());
    let page = f
        .engine
        .compose_page(date("2026-11-15"), nocturne_contract::Edition::Screen)
        .unwrap();
    assert_eq!(page.needs_you.len(), 1);
}

// ---- item private and keep forever ----

fn two_memories_from_one_item(f: &Fixture) -> (String, String, String) {
    let item = keep(&f.engine, "Leo swims and Mia plays chess");
    let r = f
        .engine
        .apply(
            &item,
            vec![
                cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming."),
                cand("Mia", Predicate::Prefers, "chess", "Mia likes chess."),
            ],
        )
        .unwrap();
    (item, r.created[0].clone(), r.created[1].clone())
}

#[test]
fn an_items_private_flag_reaches_every_memory_from_it_and_their_history() {
    let f = fixture();
    let (item, m1, m2) = two_memories_from_one_item(&f);
    let (_, other) = learn(
        &f.engine,
        "unrelated",
        cand("Zed", Predicate::Knows, "x", "Zed knows x."),
    );
    // An earlier version of m1 is covered through the chain.
    let m1b = f
        .engine
        .correct(&m1, nocturne_engine::CorrectPatch::object("rowing"))
        .unwrap();

    f.engine.set_item_private(&item, true).unwrap();
    assert!(f.engine.get_item(&item).unwrap().private);
    assert!(f.engine.get_memory(&m1b).unwrap().private);
    assert!(f.engine.get_memory(&m2).unwrap().private);
    assert!(!f.engine.get_memory(&other).unwrap().private);
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM memories WHERE private = 0 AND subject IN ('Leo', 'Mia')"
        ),
        0
    );

    f.engine.set_item_private(&item, false).unwrap();
    assert!(!f.engine.get_item(&item).unwrap().private);
    assert!(!f.engine.get_memory(&m2).unwrap().private);
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM memories WHERE private = 1"
        ),
        0
    );

    assert!(matches!(
        f.engine.set_item_private("itm_nope", true),
        Err(nocturne_engine::Error::NotFound(_))
    ));
}

#[test]
fn making_an_item_private_removes_cached_audio_of_pages_citing_it() {
    let f = fixture();
    let (item, m1, _) = two_memories_from_one_item(&f);
    f.clock.set_str("2026-11-14T06:00:00Z");
    let page = f
        .engine
        .compose_page(date("2026-11-14"), nocturne_contract::Edition::Screen)
        .unwrap();
    f.engine.save_page(&page).unwrap();
    f.engine
        .cache("2026-11-14", "af_heart", b"spoken Leo")
        .unwrap();
    assert!(
        f.engine
            .cached_audio("2026-11-14", "af_heart")
            .unwrap()
            .is_some()
    );

    f.engine.set_item_private(&item, true).unwrap();
    assert!(
        f.engine
            .cached_audio("2026-11-14", "af_heart")
            .unwrap()
            .is_none()
    );
    let audio = f
        .engine
        .page_for(Some(date("2026-11-14")), nocturne_contract::Edition::Audio)
        .unwrap()
        .unwrap();
    assert!(
        !audio
            .learned
            .iter()
            .chain(&audio.changed)
            .any(|l| l.memory_ids.contains(&m1))
    );
}

#[test]
fn an_items_keep_forever_flag_reaches_every_current_memory_from_it() {
    let f = fixture();
    let (item, m1, m2) = two_memories_from_one_item(&f);
    let (_, other) = learn(
        &f.engine,
        "unrelated",
        cand("Zed", Predicate::Knows, "x", "Zed knows x."),
    );
    f.engine.set_item_keep_forever(&item, true).unwrap();
    assert!(f.engine.get_memory(&m1).unwrap().keep_forever);
    assert!(f.engine.get_memory(&m2).unwrap().keep_forever);
    assert!(!f.engine.get_memory(&other).unwrap().keep_forever);
    f.engine.set_item_keep_forever(&item, false).unwrap();
    assert!(!f.engine.get_memory(&m1).unwrap().keep_forever);
    assert!(matches!(
        f.engine.set_item_keep_forever("itm_nope", true),
        Err(nocturne_engine::Error::NotFound(_))
    ));
}

// ---- export progress ----

#[test]
fn export_reports_progress_for_every_file_and_returns_the_contract_report() {
    let f = fixture();
    let (_, m) = learn(
        &f.engine,
        "Leo swims",
        cand("Leo", Predicate::Prefers, "swimming", "Leo likes swimming."),
    );
    let _ = m;
    f.engine
        .keep_bytes(b"photo bytes", "jpg", Source::Paste)
        .unwrap();
    f.clock.set_str("2026-11-14T06:00:00Z");
    let page = f
        .engine
        .compose_page(date("2026-11-14"), nocturne_contract::Edition::Screen)
        .unwrap();
    f.engine.save_page(&page).unwrap();

    let out = tempfile::tempdir().unwrap();
    let calls = std::cell::RefCell::new(Vec::<(u32, u32)>::new());
    let report: nocturne_contract::ExportReport = f
        .engine
        .export_to_with(out.path(), &|done, total| {
            calls.borrow_mut().push((done, total))
        })
        .unwrap();
    let calls = calls.into_inner();

    // README, memories.md, nocturne.json, one page, one original.
    assert_eq!((report.memories, report.pages, report.originals), (1, 1, 1));
    assert_eq!(calls.first(), Some(&(0, 5)));
    assert_eq!(calls.last(), Some(&(5, 5)));
    assert!(
        calls
            .windows(2)
            .all(|w| w[0].0 <= w[1].0 && w[0].1 == w[1].1)
    );
    assert_eq!(calls.iter().filter(|c| **c == (5, 5)).count(), 1);

    let root = std::path::PathBuf::from(&report.path);
    assert!(root.starts_with(out.path()));
    let written: usize = ["memories.md", "README.txt", "nocturne.json"]
        .iter()
        .filter(|n| root.join(n).exists())
        .count()
        + std::fs::read_dir(root.join("pages")).unwrap().count()
        + std::fs::read_dir(root.join("originals")).unwrap().count();
    assert_eq!(written, 5, "progress counts files written");

    // Where the store keeps an original is not part of the person's data.
    let json = std::fs::read_to_string(root.join("nocturne.json")).unwrap();
    let data: serde_json::Value = serde_json::from_str(&json).unwrap();
    for item in data["items"].as_array().unwrap() {
        assert!(item["originalPath"].is_null());
    }
    assert!(!json.contains(f.dir.path().to_str().unwrap()));
}

#[test]
fn export_to_is_the_same_export_without_a_callback() {
    let f = fixture();
    f.engine.keep_text("a note", Source::Paste).unwrap();
    let out = tempfile::tempdir().unwrap();
    let r = f.engine.export_to(out.path()).unwrap();
    assert_eq!((r.memories, r.pages, r.originals), (0, 0, 0));
    assert!(std::path::Path::new(&r.path).join("nocturne.json").exists());
    // An empty store still reports (0, 3) then (3, 3).
    let calls = std::cell::RefCell::new(Vec::new());
    f.engine
        .export_to_with(out.path(), &|d, t| calls.borrow_mut().push((d, t)))
        .unwrap();
    let calls = calls.into_inner();
    assert_eq!((calls[0], *calls.last().unwrap()), ((0, 3), (3, 3)));
}

// ---- unkeep ----

const SECRET: &str = "zebrafish47";

fn dir_mentions_secret(dir: &std::path::Path) -> Vec<String> {
    let mut hits = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            hits.extend(dir_mentions_secret(&path));
        } else {
            let lower: Vec<u8> = std::fs::read(&path)
                .unwrap()
                .iter()
                .map(u8::to_ascii_lowercase)
                .collect();
            if lower.windows(SECRET.len()).any(|w| w == SECRET.as_bytes()) {
                hits.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
    }
    hits
}

fn secret(object: &str) -> nocturne_engine::Candidate {
    let mut c = cand(
        "Dr Zebrafish47",
        Predicate::Knows,
        object,
        &format!("Dr Zebrafish47 sees you {object}."),
    );
    c.subject_kind = Some(EntityKind::Person);
    c
}

fn kept_id(k: Kept) -> String {
    kept_item(k).id
}

#[test]
fn unkeep_removes_the_item_its_original_text_and_memories_without_a_trace() {
    let f = fixture();
    let photo = kept_id(
        f.engine
            .keep_bytes(b"JPEG card for Dr Zebrafish47", "jpg", Source::Paste)
            .unwrap(),
    );
    let lease = f
        .engine
        .lease_item(&photo, std::time::Duration::from_secs(60))
        .unwrap();
    f.engine
        .record_understanding(
            &photo,
            &lease.lease,
            nocturne_engine::Understanding {
                title: "Appointment card".into(),
                text: "Dr Zebrafish47 on Tuesday".into(),
                summary: Some("A card from Dr Zebrafish47".into()),
                language: Some("en".into()),
                pages: None,
            },
        )
        .unwrap();
    let r = f
        .engine
        .apply_candidates(&photo, &lease.lease, vec![secret("on Tuesday")])
        .unwrap();
    let mem = r.created[0].clone();
    f.engine
        .store_embeddings(vec![(mem.clone(), vec![1.0, 0.0])])
        .unwrap();
    let original =
        std::path::PathBuf::from(f.engine.get_item(&photo).unwrap().original_path.unwrap());
    assert!(original.exists());
    assert!(
        !dir_mentions_secret(f.dir.path()).is_empty(),
        "the fixture holds the secret"
    );
    // Another item that is not touched.
    let other = keep(&f.engine, "Bins go out on Thursday");

    f.engine.unkeep(&photo).unwrap();

    assert!(matches!(
        f.engine.get_item(&photo),
        Err(nocturne_engine::Error::NotFound(_))
    ));
    assert!(f.engine.get_item(&other).is_ok());
    assert!(!original.exists());
    assert_eq!(
        std::fs::read_dir(f.dir.path().join("originals"))
            .unwrap()
            .count(),
        0
    );
    assert!(f.engine.get_memory(&mem).is_err());
    assert!(
        f.engine
            .memories(MemoryFilter::default())
            .unwrap()
            .is_empty()
    );
    assert!(f.engine.entities(None).unwrap().is_empty());
    let q = |sql: &str| sql_count(f.dir.path(), sql);
    assert_eq!(q("SELECT COUNT(*) FROM items"), 1);
    assert_eq!(q("SELECT COUNT(*) FROM memories"), 0);
    assert_eq!(q("SELECT COUNT(*) FROM memory_sources"), 0);
    assert_eq!(q("SELECT COUNT(*) FROM embeddings"), 0);
    assert_eq!(q("SELECT COUNT(*) FROM forget_jobs"), 0);
    assert_eq!(
        q("SELECT COUNT(*) FROM items_fts WHERE items_fts MATCH 'zebrafish47'"),
        0
    );
    assert_eq!(
        q("SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH 'zebrafish47'"),
        0
    );
    assert!(
        dir_mentions_secret(f.dir.path()).is_empty(),
        "{:?}",
        dir_mentions_secret(f.dir.path())
    );
    // The same bytes can be kept again as a new item.
    assert!(matches!(
        f.engine
            .keep_bytes(b"JPEG card for Dr Zebrafish47", "jpg", Source::Paste)
            .unwrap(),
        Kept::New { .. }
    ));
}

#[test]
fn unkeep_is_allowed_for_ten_minutes_only() {
    let f = fixture();
    let a = keep(&f.engine, "note a");
    f.clock.set_str("2026-11-13T07:10:00Z");
    f.engine.unkeep(&a).unwrap();
    assert!(f.engine.get_item(&a).is_err());

    let b = keep(&f.engine, "note b");
    f.clock.set_str("2026-11-13T07:20:01Z");
    assert!(matches!(
        f.engine.unkeep(&b),
        Err(nocturne_engine::Error::TooLate)
    ));
    assert!(
        f.engine.get_item(&b).is_ok(),
        "a refused undo changes nothing"
    );

    assert!(matches!(
        f.engine.unkeep("itm_nope"),
        Err(nocturne_engine::Error::NotFound(_))
    ));
}

#[test]
fn a_memory_with_another_source_stays_and_loses_only_this_link() {
    let f = fixture();
    let (older, m) = learn(&f.engine, "Dr Zebrafish47 on Tuesday", secret("on Tuesday"));
    let newer = keep(&f.engine, "Dr Zebrafish47 again, Tuesday");
    f.engine.apply(&newer, vec![secret("on Tuesday")]).unwrap();
    assert_eq!(f.engine.get_memory(&m).unwrap().receipts.len(), 2);

    f.engine.unkeep(&newer).unwrap();

    let got = f.engine.get_memory(&m).unwrap();
    assert_eq!(got.receipts.len(), 1);
    assert_eq!(got.receipts[0].item_id, older);
    assert!(f.engine.get_item(&older).is_ok());
    assert!(f.engine.get_item(&newer).is_err());
    assert_eq!(
        f.engine.entities(None).unwrap().len(),
        1,
        "the person is still known"
    );
    assert_eq!(f.engine.get_item(&older).unwrap().memory_count, 1);
}

#[test]
fn a_memory_left_with_only_untrusted_sources_goes_back_to_unconfirmed() {
    let f = fixture();
    let (web, m) = {
        let item = keep_as(&f.engine, "web page: Clinic opens at 9", Source::PastedWeb);
        f.engine
            .apply(
                &item,
                vec![cand(
                    "Clinic",
                    Predicate::Knows,
                    "opens at 9",
                    "The clinic opens at 9.",
                )],
            )
            .unwrap();
        let m = f.engine.memories(MemoryFilter::default()).unwrap()[0]
            .id
            .clone();
        (item, m)
    };
    let note = keep(&f.engine, "I rang: clinic opens at 9");
    let r = f
        .engine
        .apply(
            &note,
            vec![cand(
                "Clinic",
                Predicate::Knows,
                "opens at 9",
                "The clinic opens at 9.",
            )],
        )
        .unwrap();
    assert!(r.promoted.contains(&m));
    assert_eq!(
        f.engine.get_memory(&m).unwrap().status,
        nocturne_contract::MemoryStatus::Active
    );

    f.engine.unkeep(&note).unwrap();
    let got = f.engine.get_memory(&m).unwrap();
    assert_eq!(got.status, nocturne_contract::MemoryStatus::Unconfirmed);
    assert_eq!(got.receipts[0].item_id, web);
}

#[test]
fn a_memory_the_item_replaced_comes_back() {
    let f = fixture();
    let mut old = cand("Leo", Predicate::LivesIn, "Leeds", "Leo lives in Leeds.");
    old.valid_from = Some(date("2020-01-01"));
    let (_, m_old) = learn(&f.engine, "Leo lives in Leeds", old);
    let mut new = cand("Leo", Predicate::LivesIn, "York", "Leo lives in York.");
    new.valid_from = Some(date("2026-11-01"));
    let (moved, m_new) = learn(&f.engine, "Leo moved to York", new);
    let now = f.engine.get_memory(&m_old).unwrap();
    assert_eq!(now.valid_to.as_deref(), Some("2026-11-01"));

    f.engine.unkeep(&moved).unwrap();

    assert!(f.engine.get_memory(&m_new).is_err());
    let back = f.engine.get_memory(&m_old).unwrap();
    assert_eq!(back.valid_to, None);
    let current = f.engine.memories(MemoryFilter::default()).unwrap();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].id, m_old);
}

#[test]
fn unkeep_redacts_pages_and_drops_audio_and_keeps_shared_lines_free_of_the_item() {
    let f = fixture();
    let (_, shared) = learn(
        &f.engine,
        "Mia likes chess",
        cand("Mia", Predicate::Prefers, "chess", "Mia likes chess."),
    );
    let (item, sole) = learn(&f.engine, "Dr Zebrafish47 on Tuesday", secret("on Tuesday"));
    // The item also backs the shared memory, so its receipt appears on that line.
    f.engine
        .apply(
            &item,
            vec![
                secret("on Tuesday"),
                cand("Mia", Predicate::Prefers, "chess", "Mia likes chess."),
            ],
        )
        .unwrap();
    f.clock.set_str("2026-11-14T06:00:00Z");
    let page = f
        .engine
        .compose_page(date("2026-11-14"), nocturne_contract::Edition::Screen)
        .unwrap();
    f.engine.save_page(&page).unwrap();
    f.engine
        .cache("2026-11-14", "af_heart", b"spoken Zebrafish47")
        .unwrap();
    f.clock.set_str("2026-11-14T06:05:00Z");
    // The undo window has passed for the real kept_at, so move the item's time forward for
    // this scenario by keeping inside the window: re-stamp it.
    {
        let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
        conn.execute("UPDATE items SET kept_at = '2026-11-14T06:01:00.000Z'", [])
            .unwrap();
    }

    let stale = f
        .engine
        .compose_page(date("2026-11-14"), nocturne_contract::Edition::Screen)
        .unwrap();
    f.engine.unkeep(&item).unwrap();

    assert!(
        f.engine
            .cached_audio("2026-11-14", "af_heart")
            .unwrap()
            .is_none()
    );
    let saved = f.engine.page(Some(date("2026-11-14"))).unwrap().unwrap();
    let json = serde_json::to_string(&saved).unwrap();
    assert!(!json.to_lowercase().contains(SECRET), "{json}");
    assert!(!json.contains(&item));
    assert!(!json.contains(&sole));
    assert!(
        saved
            .learned
            .iter()
            .chain(&saved.changed)
            .all(|l| l.forgotten || !l.memory_ids.contains(&shared))
    );
    assert!(f.engine.get_memory(&shared).is_ok());
    // A page composed before the undo cannot bring any of it back.
    f.engine.save_page(&stale).unwrap();
    let saved = f.engine.page(Some(date("2026-11-14"))).unwrap().unwrap();
    let json = serde_json::to_string(&saved).unwrap();
    assert!(!json.to_lowercase().contains(SECRET), "{json}");
    assert!(!json.contains(&item));
    assert!(dir_mentions_secret(f.dir.path()).is_empty());
}

#[test]
fn an_unkeep_that_was_interrupted_is_finished_on_open_and_hides_everything_at_once() {
    let f = fixture();
    let photo = kept_id(
        f.engine
            .keep_bytes(b"JPEG card for Dr Zebrafish47", "jpg", Source::Paste)
            .unwrap(),
    );
    let r = f.engine.apply(&photo, vec![secret("on Tuesday")]).unwrap();
    let mem = r.created[0].clone();
    let original =
        std::path::PathBuf::from(f.engine.get_item(&photo).unwrap().original_path.unwrap());

    f.engine.unkeep_begin_only(&photo).unwrap();
    // Hidden from the moment the first phase commits.
    assert!(f.engine.get_memory(&mem).is_err());
    assert!(
        f.engine
            .memories(MemoryFilter::default())
            .unwrap()
            .is_empty()
    );
    assert!(
        f.engine
            .lease_pending(5, std::time::Duration::from_secs(60))
            .unwrap()
            .is_empty()
    );
    assert!(
        f.engine
            .lease_item(&photo, std::time::Duration::from_secs(60))
            .is_err()
    );
    assert_eq!(
        sql_count(f.dir.path(), "SELECT COUNT(*) FROM forget_jobs"),
        1
    );

    let clock = f.clock.clone();
    let dir = f.dir.path().to_path_buf();
    drop(f.engine);
    let engine = nocturne_engine::Engine::open(&dir, clock).unwrap();
    assert!(engine.get_item(&photo).is_err());
    assert!(!original.exists());
    assert_eq!(sql_count(&dir, "SELECT COUNT(*) FROM forget_jobs"), 0);
    assert_eq!(sql_count(&dir, "SELECT COUNT(*) FROM memories"), 0);
    assert!(dir_mentions_secret(&dir).is_empty());
}

#[test]
fn a_worker_holding_the_read_loses_it_when_the_keep_is_undone() {
    let f = fixture();
    let item = keep(&f.engine, "Dr Zebrafish47 on Tuesday");
    let lease = f
        .engine
        .lease_item(&item, std::time::Duration::from_secs(60))
        .unwrap();
    f.engine.unkeep(&item).unwrap();
    let err = f
        .engine
        .apply_candidates(&item, &lease.lease, vec![secret("on Tuesday")])
        .unwrap_err();
    assert!(
        matches!(
            err,
            nocturne_engine::Error::NotFound(_) | nocturne_engine::Error::LeaseLost
        ),
        "{err:?}"
    );
    assert!(
        f.engine
            .memories(MemoryFilter::default())
            .unwrap()
            .is_empty()
    );
}

// ---- review fixes ----

fn restamp(f: &Fixture, id: &str, at: &str) {
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    conn.execute("UPDATE items SET kept_at = ?2 WHERE id = ?1", [id, at])
        .unwrap();
}

fn private_count(f: &Fixture, where_sql: &str) -> i64 {
    sql_count(
        f.dir.path(),
        &format!("SELECT COUNT(*) FROM memories WHERE private = 1 AND {where_sql}"),
    )
}

#[test]
fn what_a_private_item_says_is_private_when_it_is_read_later() {
    let f = fixture();
    let y = keep(&f.engine, "Therapy notes");
    f.engine.set_item_private(&y, true).unwrap();
    let r = f
        .engine
        .apply(
            &y,
            vec![cand("Leo", Predicate::Knows, "x", "Leo sees a therapist.")],
        )
        .unwrap();
    assert!(f.engine.get_memory(&r.created[0]).unwrap().private);
    assert!(f.engine.get_item(&y).unwrap().private);
}

#[test]
fn a_private_item_that_agrees_with_a_public_memory_makes_it_private_and_drops_audio() {
    let f = fixture();
    let (_, m) = learn(
        &f.engine,
        "Mia note",
        cand("Mia", Predicate::Prefers, "chess", "Mia likes chess."),
    );
    f.clock.set_str("2026-11-14T06:00:00Z");
    let page = f
        .engine
        .compose_page(date("2026-11-14"), nocturne_contract::Edition::Screen)
        .unwrap();
    f.engine.save_page(&page).unwrap();
    f.engine
        .cache("2026-11-14", "af_heart", b"spoken Mia")
        .unwrap();
    let y = keep(&f.engine, "private diary");
    f.engine.set_item_private(&y, true).unwrap();
    f.engine
        .apply(
            &y,
            vec![cand("Mia", Predicate::Prefers, "chess", "Mia likes chess.")],
        )
        .unwrap();
    assert!(f.engine.get_memory(&m).unwrap().private);
    assert!(
        f.engine
            .cached_audio("2026-11-14", "af_heart")
            .unwrap()
            .is_none()
    );
}

#[test]
fn an_items_flags_reach_memories_it_lost_to_a_choice() {
    let f = fixture();
    let (_, a) = learn(
        &f.engine,
        "Dentist Tuesday",
        cand(
            "The dentist appointment",
            Predicate::Scheduled,
            "Tuesday 14:00",
            "The dentist appointment is Tuesday at 14:00.",
        ),
    );
    let y = keep(&f.engine, "Dentist moved");
    let r = f
        .engine
        .apply(
            &y,
            vec![cand(
                "dentist appointment",
                Predicate::Scheduled,
                "Thursday 10:00",
                "The dentist appointment is Thursday at 10:00.",
            )],
        )
        .unwrap();
    let b = r.created[0].clone();
    // The person keeps the old time; Y's own memory becomes history.
    f.engine.resolve_which(&r.conflicts[0], &a).unwrap();
    f.clock.set_str("2026-11-14T06:00:00Z");
    let page = f
        .engine
        .compose_page(date("2026-11-14"), nocturne_contract::Edition::Screen)
        .unwrap();
    f.engine.save_page(&page).unwrap();
    f.engine.cache("2026-11-14", "af_heart", b"spoken").unwrap();

    f.engine.set_item_private(&y, true).unwrap();
    assert_eq!(private_count(&f, &format!("id = '{b}'")), 1);
    assert!(
        f.engine
            .cached_audio("2026-11-14", "af_heart")
            .unwrap()
            .is_none()
    );
    f.engine.set_item_keep_forever(&y, true).unwrap();
    assert_eq!(
        sql_count(
            f.dir.path(),
            &format!("SELECT keep_forever FROM memories WHERE id = '{b}'")
        ),
        1
    );
}

#[test]
fn turning_an_items_private_flag_off_spares_what_another_source_or_version_still_holds() {
    let f = fixture();
    let (_, shared) = learn(
        &f.engine,
        "Mia note",
        cand("Mia", Predicate::Prefers, "chess", "Mia likes chess."),
    );
    let y = keep(&f.engine, "private diary");
    f.engine
        .apply(
            &y,
            vec![
                cand("Mia", Predicate::Prefers, "chess", "Mia likes chess."),
                cand("Leo", Predicate::Knows, "x", "Leo knows x."),
            ],
        )
        .unwrap();
    f.engine.set_item_private(&y, true).unwrap();
    let only_y = f
        .engine
        .memories(MemoryFilter {
            query: Some("Leo".into()),
            ..Default::default()
        })
        .unwrap()[0]
        .id
        .clone();
    assert!(f.engine.get_memory(&shared).unwrap().private);
    assert!(f.engine.get_memory(&only_y).unwrap().private);

    f.engine.set_item_private(&y, false).unwrap();
    assert!(!f.engine.get_item(&y).unwrap().private);
    assert!(!f.engine.get_memory(&only_y).unwrap().private);
    assert!(
        f.engine.get_memory(&shared).unwrap().private,
        "another source's memory stays private"
    );
}

fn page_with_question(f: &Fixture, y: &str) -> nocturne_contract::PageView {
    let page = f
        .engine
        .compose_page(date("2026-11-14"), nocturne_contract::Edition::Screen)
        .unwrap();
    f.engine.save_page(&page).unwrap();
    let q = page.question.clone().expect("a question");
    assert_eq!(q.receipt.as_ref().unwrap().item_id, y, "{q:?}");
    page
}

#[test]
fn unkeep_clears_a_daily_question_that_cites_the_item_and_fixes_the_counts() {
    let f = fixture();
    // Page 1 first, so the next page asks a yes/no question.
    let p = f
        .engine
        .compose_page(date("2026-11-13"), nocturne_contract::Edition::Screen)
        .unwrap();
    f.engine.save_page(&p).unwrap();
    let y = keep(&f.engine, "Zebrafish47 clinic letter");
    let r = f
        .engine
        .apply(
            &y,
            vec![cand("Mia", Predicate::Prefers, "chess", "Mia likes chess.")],
        )
        .unwrap();
    let m = r.created[0].clone();
    f.clock.set_str("2026-11-13T08:00:00Z");
    let x = keep(&f.engine, "Mia note");
    f.engine
        .apply(
            &x,
            vec![cand("Mia", Predicate::Prefers, "chess", "Mia likes chess.")],
        )
        .unwrap();
    f.clock.set_str("2026-11-14T06:00:00Z");
    let page = page_with_question(&f, &y);
    assert_eq!(page.kept_count, 2);
    restamp(&f, &y, "2026-11-14T05:59:00.000Z");

    f.engine.unkeep(&y).unwrap();

    assert!(f.engine.get_memory(&m).is_ok());
    let saved = f.engine.page(Some(date("2026-11-14"))).unwrap().unwrap();
    let json = serde_json::to_string(&saved).unwrap();
    assert!(saved.question.is_none(), "{:?}", saved.question);
    assert!(!json.contains(&y), "{json}");
    assert!(!json.to_lowercase().contains("zebrafish47"), "{json}");
    // The window [07:00 on the 13th, 06:00 on the 14th) held both items; now it holds one.
    assert_eq!(saved.kept_count, 1);
    assert!(saved.dek.contains("read 1 thing "), "{}", saved.dek);
    // A page composed before the undo cannot bring the question back.
    f.engine.save_page(&page).unwrap();
    let again = f.engine.page(Some(date("2026-11-14"))).unwrap().unwrap();
    assert!(again.question.is_none());
}

#[test]
fn an_undone_item_taken_by_the_morning_read_is_not_counted_pending() {
    let f = fixture();
    let p = f
        .engine
        .compose_page(date("2026-11-13"), nocturne_contract::Edition::Screen)
        .unwrap();
    f.engine.save_page(&p).unwrap();
    let a = keep(&f.engine, "waiting note");
    f.clock.set_str("2026-11-14T06:00:00Z");
    restamp(&f, &a, "2026-11-14T05:58:00.000Z");
    let page = f
        .engine
        .compose_page(date("2026-11-14"), nocturne_contract::Edition::Screen)
        .unwrap();
    assert_eq!((page.kept_count, page.pending), (1, 1));
    f.engine.save_page(&page).unwrap();
    f.engine.unkeep(&a).unwrap();
    let saved = f.engine.page(Some(date("2026-11-14"))).unwrap().unwrap();
    assert_eq!((saved.kept_count, saved.pending), (0, 0));
}

#[test]
fn unkeep_takes_the_item_row_and_lists_its_original_in_the_first_phase() {
    let f = fixture();
    let photo = kept_id(
        f.engine
            .keep_bytes(b"JPEG card for Dr Zebrafish47", "jpg", Source::Paste)
            .unwrap(),
    );
    let original =
        std::path::PathBuf::from(f.engine.get_item(&photo).unwrap().original_path.unwrap());
    f.engine.unkeep_begin_only(&photo).unwrap();
    assert!(f.engine.get_item(&photo).is_err());
    assert_eq!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM items"), 0);
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM items_fts WHERE items_fts MATCH 'zebrafish47'"
        ),
        0
    );
    let listed: String = {
        let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
        conn.query_row("SELECT files FROM forget_jobs", [], |r| r.get(0))
            .unwrap()
    };
    assert!(listed.contains(original.file_name().unwrap().to_str().unwrap()));
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM forget_jobs WHERE kind = 'unkeep'"
        ),
        1
    );
}

#[test]
fn a_crash_after_the_memories_were_purged_is_finished_on_open() {
    let f = fixture();
    let photo = kept_id(
        f.engine
            .keep_bytes(b"JPEG card for Dr Zebrafish47", "jpg", Source::Paste)
            .unwrap(),
    );
    let r = f.engine.apply(&photo, vec![secret("on Tuesday")]).unwrap();
    assert_eq!(r.created.len(), 1);
    let original =
        std::path::PathBuf::from(f.engine.get_item(&photo).unwrap().original_path.unwrap());
    f.engine.unkeep_begin_only(&photo).unwrap();
    {
        // Phase 2 had purged the memories; the app stopped before the clean-up.
        let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
        conn.execute("DELETE FROM memories", []).unwrap();
    }
    let clock = f.clock.clone();
    let dir = f.dir.path().to_path_buf();
    drop(f.engine);
    let _engine = nocturne_engine::Engine::open(&dir, clock).unwrap();
    assert_eq!(sql_count(&dir, "SELECT COUNT(*) FROM forget_jobs"), 0);
    assert_eq!(sql_count(&dir, "SELECT COUNT(*) FROM entities"), 0);
    assert!(!original.exists());
}

#[test]
fn unkeep_reports_what_it_removed() {
    let f = fixture();
    let photo = kept_id(
        f.engine
            .keep_bytes(b"JPEG card for Dr Zebrafish47", "jpg", Source::Paste)
            .unwrap(),
    );
    f.engine.apply(&photo, vec![secret("on Tuesday")]).unwrap();
    let out = f.engine.unkeep(&photo).unwrap();
    assert_eq!(out.report.memories_removed, 1);
    assert_eq!(out.report.originals_removed, 1);
    let note = keep(&f.engine, "a note");
    let out = f.engine.unkeep(&note).unwrap();
    assert_eq!(
        (out.report.memories_removed, out.report.originals_removed),
        (0, 0)
    );
}

#[test]
fn a_regular_forget_job_is_not_an_unkeep_job() {
    let f = fixture();
    let (_, m) = learn(
        &f.engine,
        "Zed note",
        cand("Zed", Predicate::Knows, "x", "Zed knows x."),
    );
    let p = f.engine.preview_forget(&[m], false).unwrap();
    f.engine.forget_begin_only(&p.token).unwrap();
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM forget_jobs WHERE kind = 'forget'"
        ),
        1
    );
}

#[test]
fn undoing_the_keep_that_promoted_a_memory_removes_the_choice_the_promotion_made() {
    let f = fixture();
    let x = keep_as(&f.engine, "web: dentist Tuesday", Source::PastedWeb);
    let rx = f
        .engine
        .apply(
            &x,
            vec![cand(
                "The dentist appointment",
                Predicate::Scheduled,
                "Tuesday 14:00",
                "The dentist appointment is Tuesday at 14:00.",
            )],
        )
        .unwrap();
    let mx = rx.created[0].clone();
    learn(
        &f.engine,
        "dentist thursday",
        cand(
            "The dentist appointment",
            Predicate::Scheduled,
            "Thursday 10:00",
            "The dentist appointment is Thursday at 10:00.",
        ),
    );
    let y = keep(&f.engine, "rang: dentist Tuesday");
    let ry = f
        .engine
        .apply(
            &y,
            vec![cand(
                "The dentist appointment",
                Predicate::Scheduled,
                "Tuesday 14:00",
                "The dentist appointment is Tuesday at 14:00.",
            )],
        )
        .unwrap();
    assert_eq!(ry.conflicts.len(), 1, "the promotion made a choice");
    f.engine.unkeep(&y).unwrap();
    assert_eq!(
        f.engine.get_memory(&mx).unwrap().status,
        nocturne_contract::MemoryStatus::Unconfirmed
    );
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM conflicts WHERE resolved_at IS NULL"
        ),
        0
    );
}

#[test]
fn an_export_that_ran_across_an_unkeep_is_discarded() {
    let f = fixture();
    let ids: Vec<String> = ["alpha.txt", "bravo.txt", "charlie.txt"]
        .iter()
        .enumerate()
        .map(|(i, n)| {
            f.clock.set_str(&format!("2026-11-13T07:0{i}:00Z"));
            let p = write_file(f.dir.path(), n, n.as_bytes());
            kept_id(f.engine.keep_file(&p, Source::Drop).unwrap())
        })
        .collect();
    f.clock.set_str("2026-11-13T07:05:00Z");
    let out = tempfile::tempdir().unwrap();
    let fired = std::cell::Cell::new(false);
    let err = f
        .engine
        .export_to_with(out.path(), &|done, _| {
            if done == 1 && !fired.get() {
                fired.set(true);
                f.engine.unkeep(&ids[1]).unwrap();
            }
        })
        .unwrap_err();
    assert!(fired.get());
    assert!(matches!(err, nocturne_engine::Error::Busy), "{err:?}");
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
    // Left alone, the export is whole: each copy is named for and holds its own item.
    let r = f.engine.export_to(out.path()).unwrap();
    let root = std::path::PathBuf::from(&r.path);
    let data: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("nocturne.json")).unwrap())
            .unwrap();
    assert_eq!(data["items"].as_array().unwrap().len(), 2);
    for item in data["items"].as_array().unwrap() {
        let copy = root.join(item["export"].as_str().unwrap());
        assert_eq!(
            std::fs::read(&copy).unwrap(),
            item["fileName"].as_str().unwrap().as_bytes()
        );
    }
}

// ---- privacy gaps ----

#[test]
fn a_reread_of_a_private_item_makes_its_memories_private_even_when_nothing_is_reported() {
    let f = fixture();
    let y = keep(&f.engine, "private diary");
    let c = || cand("Leo", Predicate::Knows, "x", "Leo knows x.");
    let r = f.engine.apply(&y, vec![c()]).unwrap();
    let m = r.created[0].clone();
    f.engine.set_item_private(&y, true).unwrap();
    assert!(f.engine.get_memory(&m).unwrap().private);
    // The memory is made public by hand, then the same item is read again: the re-read refreshes
    // the same memory and reports neither a creation nor a reinforcement.
    f.engine.set_private(&m, false).unwrap();
    let again = f.engine.apply(&y, vec![c()]).unwrap();
    assert!(again.created.is_empty() && again.reinforced.is_empty());
    assert!(f.engine.get_memory(&m).unwrap().private);
}

#[test]
fn an_export_that_ran_across_a_forget_is_discarded() {
    let f = fixture();
    let (_, m) = learn(
        &f.engine,
        "Zed note",
        cand("Zed", Predicate::Knows, "x", "Zed knows x."),
    );
    f.engine.keep_bytes(b"photo", "jpg", Source::Paste).unwrap();
    let out = tempfile::tempdir().unwrap();
    let fired = std::cell::Cell::new(false);
    let err = f
        .engine
        .export_to_with(out.path(), &|done, _| {
            if done == 1 && !fired.get() {
                fired.set(true);
                let p = f
                    .engine
                    .preview_forget(std::slice::from_ref(&m), false)
                    .unwrap();
                f.engine.forget(&p.token).unwrap();
            }
        })
        .unwrap_err();
    assert!(matches!(err, nocturne_engine::Error::Busy), "{err:?}");
    assert_eq!(
        std::fs::read_dir(out.path()).unwrap().count(),
        0,
        "nothing is left behind"
    );
    // Nothing changing, the same export succeeds.
    assert!(f.engine.export_to(out.path()).is_ok());
}

fn stale_page_after_forget(include_originals: bool) -> String {
    let f = fixture();
    let (x, shared) = learn(
        &f.engine,
        "Mia from x",
        cand("Mia", Predicate::Prefers, "chess", "Mia likes chess."),
    );
    let _ = x;
    let y = keep(&f.engine, "Mia and Leo from y");
    f.engine
        .apply(
            &y,
            vec![
                cand("Mia", Predicate::Prefers, "chess", "Mia likes chess."),
                cand("Leo", Predicate::Knows, "x", "Leo knows x."),
            ],
        )
        .unwrap();
    let leo = f
        .engine
        .memories(MemoryFilter {
            query: Some("Leo".into()),
            ..Default::default()
        })
        .unwrap()[0]
        .id
        .clone();
    f.clock.set_str("2026-11-14T06:00:00Z");
    let stale = f
        .engine
        .compose_page(date("2026-11-14"), nocturne_contract::Edition::Screen)
        .unwrap();
    let p = f
        .engine
        .preview_forget(std::slice::from_ref(&leo), include_originals)
        .unwrap();
    assert!(p.originals.iter().any(|o| o.item_id == y));
    f.engine.forget_begin_only(&p.token).unwrap();
    f.engine.save_page(&stale).unwrap();
    let saved = f.engine.page(Some(date("2026-11-14"))).unwrap().unwrap();
    let line_id = stale
        .learned
        .iter()
        .chain(&stale.changed)
        .find(|l| l.memory_ids.contains(&shared))
        .expect("a line for the shared memory")
        .id
        .clone();
    let line = saved
        .learned
        .iter()
        .chain(&saved.changed)
        .find(|l| l.id == line_id)
        .expect("the line is still on the page");
    serde_json::to_string(&(line.forgotten, line.receipts.len())).unwrap()
}

#[test]
fn a_stale_page_may_cite_an_item_the_forget_keeps() {
    // The item also says the shared memory, so even a forget with originals keeps it, and the
    // line for the shared memory stays with both receipts.
    assert_eq!(stale_page_after_forget(true), "[false,2]");
    assert_eq!(stale_page_after_forget(false), "[false,2]");
}

#[test]
fn a_store_at_version_three_with_a_pending_unkeep_job_is_marked_on_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("nocturne.db");
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        for (version, sql) in nocturne_engine::store::migrations() {
            if version <= 3 {
                conn.execute_batch(sql).unwrap();
                conn.pragma_update(None, "user_version", version).unwrap();
            }
        }
        conn.execute_batch(
            "INSERT INTO forget_jobs(id, ids, items, include_originals) VALUES \
               (1, '[]', '[\"itm_one\"]', 1), \
               (2, '[\"mem_a\"]', '[\"itm_a\", \"itm_b\"]', 1), \
               (3, '[\"mem_b\"]', '[\"itm_c\"]', 0);",
        )
        .unwrap();
    }
    drop(nocturne_engine::store::open(&db).unwrap());
    let conn = rusqlite::Connection::open(&db).unwrap();
    let kinds: Vec<(i64, String)> = conn
        .prepare("SELECT id, kind FROM forget_jobs ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        kinds,
        [
            (1, "unkeep".to_string()),
            (2, "forget".to_string()),
            (3, "forget".to_string())
        ]
    );
}
