mod common;

use std::time::Duration;

use common::*;
use nocturne_contract::{
    CouldntReadReason, HeardBy, ItemFilter, ItemKind, ItemStatus, Kept, Source, Trust,
};
use nocturne_engine::{RecordOutcome, Transcript, Understanding};

fn new_item(k: Kept) -> nocturne_contract::ItemView {
    match k {
        Kept::New { item } => item,
        Kept::Duplicate { .. } => panic!("expected a new item"),
    }
}

#[test]
fn keep_text_returns_new_with_status_kept_and_touches_nothing_else() {
    let f = fixture();
    let item = new_item(
        f.engine
            .keep_text("Dentist on Tuesday at 14:00", Source::QuickCapture)
            .unwrap(),
    );
    assert_eq!(item.status, ItemStatus::Kept);
    assert_eq!(item.kind, ItemKind::Text);
    assert_eq!(item.title, "Dentist on Tuesday at 14:00");
    assert_eq!(item.trust, Trust::Trusted);
    assert_eq!(item.kept_at, "2026-11-13T07:00:00.000Z");
    assert_eq!(item.text.as_deref(), Some("Dentist on Tuesday at 14:00"));
    // Capture is one insert: no memories, no pages, nothing read.
    assert_eq!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM memories"), 0);
    assert_eq!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM items"), 1);
}

#[test]
fn rekeeping_the_same_bytes_is_a_duplicate_and_changes_nothing() {
    // Review focus 3.
    let f = fixture();
    let first = new_item(
        f.engine
            .keep_text("Bins go out on Thursday", Source::Paste)
            .unwrap(),
    );
    f.clock.advance_days(2);
    match f
        .engine
        .keep_text("Bins go out on Thursday", Source::QuickCapture)
        .unwrap()
    {
        Kept::Duplicate { item } => {
            assert_eq!(item.id, first.id);
            assert_eq!(item.kept_at, first.kept_at);
            assert_eq!(item.source, Source::Paste);
        }
        Kept::New { .. } => panic!("expected a duplicate"),
    }
    assert_eq!(f.engine.items(ItemFilter::default()).unwrap().len(), 1);
}

#[test]
fn keep_file_copies_to_originals_by_hash_and_dedupes() {
    let f = fixture();
    let src = tempfile::tempdir().unwrap();
    let a = write_file(src.path(), "card.jpg", b"not really a jpeg");
    let b = write_file(src.path(), "copy of card.jpg", b"not really a jpeg");
    let item = new_item(f.engine.keep_file(&a, Source::Drop).unwrap());
    assert_eq!(item.kind, ItemKind::Image);
    assert!(item.has_original);
    assert_eq!(item.file_name.as_deref(), Some("card.jpg"));
    let sha = {
        use sha2::{Digest, Sha256};
        Sha256::digest(b"not really a jpeg")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let stored = f.dir.path().join("originals").join(format!("{sha}.jpg"));
    assert_eq!(std::fs::read(&stored).unwrap(), b"not really a jpeg");
    assert!(matches!(
        f.engine.keep_file(&b, Source::Drop).unwrap(),
        Kept::Duplicate { .. }
    ));
    // No temp files left behind.
    let names: Vec<_> = std::fs::read_dir(f.dir.path().join("originals"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
}

/// Spec D9: what a paired phone keeps is the person's own, trusted and labelled by its source.
#[test]
fn phone_keeps_are_trusted() {
    let f = fixture();
    let item = new_item(
        f.engine
            .keep_text("Bin day moved to Thursday", Source::Phone)
            .unwrap(),
    );
    assert_eq!(item.source, Source::Phone);
    assert_eq!(item.trust, Trust::Trusted);
    let again = f.engine.get_item(&item.id).unwrap();
    assert_eq!((again.source, again.trust), (Source::Phone, Trust::Trusted));
}

#[test]
fn trust_follows_the_source_table() {
    let f = fixture();
    let cases = [
        (Source::QuickCapture, Trust::Trusted),
        (Source::Drop, Trust::Trusted),
        (Source::Paste, Trust::Trusted),
        (Source::PastedWeb, Trust::Untrusted),
        (Source::WatchedFolder, Trust::Trusted),
        (Source::Screenshot, Trust::Trusted),
        (Source::Recording, Trust::Trusted),
        (Source::Agent, Trust::Untrusted),
        (Source::Setup, Trust::Trusted),
        (Source::Phone, Trust::Trusted),
    ];
    for (i, (source, trust)) in cases.into_iter().enumerate() {
        assert_eq!(nocturne_engine::trust_for(source), trust);
        let item = new_item(f.engine.keep_text(&format!("note {i}"), source).unwrap());
        assert_eq!(item.trust, trust, "{source:?}");
    }
    let web = f.engine.items(ItemFilter::default()).unwrap();
    let pasted = web.iter().find(|i| i.source == Source::PastedWeb).unwrap();
    assert_eq!(pasted.kind, ItemKind::Web);
}

#[test]
fn kind_follows_extension() {
    let f = fixture();
    let src = tempfile::tempdir().unwrap();
    let table: &[(&str, ItemKind)] = &[
        ("a.jpg", ItemKind::Image),
        ("a.png", ItemKind::Image),
        ("a.HEIC", ItemKind::Image),
        ("a.webp", ItemKind::Image),
        ("a.m4a", ItemKind::Audio),
        ("a.mp3", ItemKind::Audio),
        ("a.wav", ItemKind::Audio),
        ("a.ogg", ItemKind::Audio),
        ("a.opus", ItemKind::Audio),
        ("a.webm", ItemKind::Audio),
        ("a.pdf", ItemKind::Document),
        ("a.txt", ItemKind::Document),
        ("a.md", ItemKind::Document),
        ("a.docx", ItemKind::Document),
        // Unknown types are still kept; the mind decides whether it can read them.
        ("a.xyz", ItemKind::Document),
        ("noext", ItemKind::Document),
    ];
    for (i, (name, kind)) in table.iter().enumerate() {
        let p = write_file(src.path(), name, format!("bytes {i}").as_bytes());
        let item = new_item(f.engine.keep_file(&p, Source::Drop).unwrap());
        assert_eq!(item.kind, *kind, "{name}");
        assert_eq!(item.status, ItemStatus::Kept, "{name}");
    }
}

#[test]
fn words_a_phone_heard_are_marked_and_a_mind_replaces_them() {
    let f = fixture();
    let src = tempfile::tempdir().unwrap();
    let note = new_item(
        f.engine
            .keep_file(
                &write_file(src.path(), "note.wav", b"audio"),
                Source::Recording,
            )
            .unwrap(),
    );
    assert_eq!(f.engine.get_item(&note.id).unwrap().heard_by, None);
    let words = |text: &str| Transcript {
        text: text.into(),
        duration_ms: 1000,
        ..Default::default()
    };
    let lease = || {
        f.engine
            .lease_item(&note.id, Duration::from_secs(60))
            .unwrap()
            .lease
    };

    let l = lease();
    f.engine
        .record_phone_transcript(&note.id, &l, words("the purple giraffe"))
        .unwrap();
    f.engine.release_lease(&note.id, &l, false).unwrap();
    let heard = f.engine.get_item(&note.id).unwrap();
    assert_eq!(heard.text.as_deref(), Some("the purple giraffe"));
    assert_eq!(heard.heard_by, Some(HeardBy::Phone));
    assert_eq!(heard.status, ItemStatus::Kept, "a mind still reads it");
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM items WHERE json_extract(metadata, '$.heardBy') = 'phone'"
        ),
        1
    );

    let l = lease();
    f.engine
        .record_transcript(&note.id, &l, words("the purple giraffes"))
        .unwrap();
    let read = f.engine.get_item(&note.id).unwrap();
    assert_eq!(read.text.as_deref(), Some("the purple giraffes"));
    assert_eq!(read.heard_by, None, "the phone's mark goes with its words");
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM items WHERE json_type(metadata, '$.heardBy') IS NOT NULL"
        ),
        0,
        "the key is gone, not null"
    );
}

#[test]
fn long_items_are_kept_and_flagged_only_after_understanding_reports_length() {
    let f = fixture();
    let src = tempfile::tempdir().unwrap();
    let audio = new_item(
        f.engine
            .keep_file(
                &write_file(src.path(), "long.m4a", b"audio"),
                Source::Recording,
            )
            .unwrap(),
    );
    // Capture itself accepted it.
    assert_eq!(audio.status, ItemStatus::Kept);
    let t = |ms| Transcript {
        text: "x".into(),
        duration_ms: ms,
        ..Default::default()
    };
    let lease = |id: &str| {
        f.engine
            .lease_item(id, Duration::from_secs(60))
            .unwrap()
            .lease
    };
    let l = lease(&audio.id);
    assert_eq!(
        f.engine
            .record_transcript(&audio.id, &l, t(2 * 60 * 60 * 1000))
            .unwrap(),
        RecordOutcome::Recorded
    );
    assert_eq!(
        f.engine.get_item(&audio.id).unwrap().status,
        ItemStatus::Reading
    );
    assert_eq!(
        f.engine
            .record_transcript(&audio.id, &l, t(2 * 60 * 60 * 1000 + 1))
            .unwrap(),
        RecordOutcome::TooLong
    );
    let got = f.engine.get_item(&audio.id).unwrap();
    assert_eq!(got.status, ItemStatus::CouldntRead);
    assert_eq!(got.reason, Some(CouldntReadReason::TooLong));
    assert!(
        matches!(
            f.engine.record_transcript(&audio.id, &l, t(1)),
            Err(nocturne_engine::Error::LeaseLost)
        ),
        "an item that could not be read takes no more readings"
    );

    let doc = new_item(
        f.engine
            .keep_file(&write_file(src.path(), "big.pdf", b"pdf"), Source::Drop)
            .unwrap(),
    );
    let u = |pages| Understanding {
        text: "t".into(),
        pages: Some(pages),
        ..Default::default()
    };
    let l = lease(&doc.id);
    assert_eq!(
        f.engine.record_understanding(&doc.id, &l, u(100)).unwrap(),
        RecordOutcome::Recorded
    );
    assert_eq!(
        f.engine.record_understanding(&doc.id, &l, u(101)).unwrap(),
        RecordOutcome::TooLong
    );
    let got = f.engine.get_item(&doc.id).unwrap();
    assert_eq!(
        (got.status, got.reason),
        (ItemStatus::CouldntRead, Some(CouldntReadReason::TooLong))
    );
    assert_eq!(got.pages, Some(101));

    f.engine.retry_item(&doc.id).unwrap();
    assert_eq!(f.engine.get_item(&doc.id).unwrap().status, ItemStatus::Kept);
}

#[test]
fn leases_are_exclusive_and_expire() {
    let f = fixture();
    for i in 0..3 {
        f.engine
            .keep_text(&format!("note {i}"), Source::Paste)
            .unwrap();
    }
    assert_eq!(f.engine.pending_items(10).unwrap().len(), 3);
    let first = f.engine.lease_pending(2, Duration::from_secs(60)).unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(
        f.engine.get_item(&first[0].id).unwrap().status,
        ItemStatus::Reading
    );
    let second = f.engine.lease_pending(10, Duration::from_secs(60)).unwrap();
    assert_eq!(second.len(), 1, "leased items are not handed out twice");
    assert!(
        f.engine
            .lease_pending(10, Duration::from_secs(60))
            .unwrap()
            .is_empty()
    );

    f.clock.set_str("2026-11-13T07:02:00Z");
    let again = f.engine.lease_pending(10, Duration::from_secs(60)).unwrap();
    assert_eq!(again.len(), 3, "expired leases go back in the queue");
    assert!(again.iter().all(|p| p.attempts == 2));
}

#[test]
fn open_returns_abandoned_reads_to_the_queue() {
    let f = fixture();
    f.engine.keep_text("note", Source::Paste).unwrap();
    f.engine.lease_pending(1, Duration::from_secs(60)).unwrap();
    f.clock.set_str("2026-11-13T09:00:00Z");
    let reopened = nocturne_engine::Engine::open(f.dir.path(), f.clock.clone()).unwrap();
    assert_eq!(reopened.pending_items(10).unwrap().len(), 1);
}

#[test]
fn empty_text_is_not_kept() {
    let f = fixture();
    assert!(f.engine.keep_text("  \n ", Source::Paste).is_err());
}

#[cfg(unix)]
#[test]
fn the_data_directory_and_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    let parent = tempfile::tempdir().unwrap();
    let dir = parent.path().join("data");
    let clock = std::sync::Arc::new(nocturne_engine::ManualClock::at(
        "2026-11-13T07:00:00Z",
        "UTC",
    ));
    let engine = nocturne_engine::Engine::open(&dir, clock).unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = write_file(src.path(), "a.pdf", b"pdf bytes");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    engine.keep_text("a note", Source::Paste).unwrap();
    engine.keep_file(&file, Source::Drop).unwrap();

    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&dir.join("originals")), 0o700);
    assert_eq!(mode(&dir.join("nocturne.db")), 0o600);
    for suffix in ["-wal", "-shm"] {
        let p = dir.join(format!("nocturne.db{suffix}"));
        if p.exists() {
            assert_eq!(mode(&p), 0o600, "{suffix}");
        }
    }
    for entry in std::fs::read_dir(dir.join("originals")).unwrap() {
        assert_eq!(mode(&entry.unwrap().path()), 0o600);
    }
}
