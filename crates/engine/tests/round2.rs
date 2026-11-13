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
