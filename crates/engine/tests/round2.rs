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

#[test]
fn receipts_do_not_carry_the_original_path() {
    let f = fixture();
    let src = write_file(f.dir.path(), "brief.txt", b"The brief");
    let item = kept_item(f.engine.keep_file(&src, Source::Drop).unwrap());
    let p = f
        .engine
        .lease_item(&item.id, std::time::Duration::from_secs(60))
        .unwrap();
    f.engine
        .apply_candidates(
            &item.id,
            &p.lease,
            vec![cand("Brief", Predicate::Knows, "x", "The brief says x.")],
        )
        .unwrap();
    let m = &f.engine.memories(MemoryFilter::default()).unwrap()[0];
    let json = serde_json::to_string(&m.receipts).unwrap();
    assert!(!json.contains("originals"), "{json}");
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
