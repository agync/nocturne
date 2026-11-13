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
