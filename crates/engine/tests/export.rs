mod common;

use common::*;
use nocturne_contract::{Edition, Kept, LedgerKind, Source};
use nocturne_engine::{DELETE_PHRASE, Predicate};
use serde_json::Value;

struct World {
    f: Fixture,
    private: String,
    day: &'static str,
}

fn world() -> World {
    let f = fixture();
    let src = tempfile::tempdir().unwrap();
    let photo = match f
        .engine
        .keep_file(
            &write_file(
                src.path(),
                "appointment card.jpg",
                b"JPEG bytes of the card",
            ),
            Source::Screenshot,
        )
        .unwrap()
    {
        Kept::New { item } => item.id,
        _ => unreachable!(),
    };
    f.engine
        .apply(
            &photo,
            vec![cand(
                "Dentist appointment",
                Predicate::Scheduled,
                "Tuesday 14:00",
                "Your dentist appointment is Tuesday at 14:00.",
            )],
        )
        .unwrap();
    let (_, private) = learn(&f.engine, "I take Zebrafish47 for my heart", {
        let mut c = cand(
            "Me",
            Predicate::Takes,
            "Zebrafish47",
            "You take Zebrafish47 for your heart.",
        );
        c.tags = vec!["health".into()];
        c
    });
    f.engine.set_private(&private, true).unwrap();
    let (_, old) = learn(
        &f.engine,
        "Gran lives in Leeds",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    f.engine
        .correct(&old, nocturne_engine::CorrectPatch::object("York"))
        .unwrap();
    let web = keep_as(&f.engine, "web claim", Source::PastedWeb);
    f.engine
        .apply(
            &web,
            vec![cand(
                "Clinic",
                Predicate::Knows,
                "opens at nine",
                "The clinic opens at nine.",
            )],
        )
        .unwrap();
    f.clock.set_str("2026-11-14T06:00:00Z");
    let page = f
        .engine
        .compose_page(date("2026-11-14"), Edition::Screen)
        .unwrap();
    f.engine.save_page(&page).unwrap();
    World {
        f,
        private,
        day: "2026-11-14",
    }
}

#[test]
fn export_writes_readme_memories_pages_originals_and_data() {
    let w = world();
    let out = tempfile::tempdir().unwrap();
    let report = w.f.engine.export_to(out.path()).unwrap();
    let root = std::path::PathBuf::from(&report.path);
    assert_eq!((report.memories, report.pages, report.originals), (4, 1, 1));

    let readme = std::fs::read_to_string(root.join("README.txt")).unwrap();
    assert!(readme.contains("nocturne.json"));

    let md = std::fs::read_to_string(root.join("memories.md")).unwrap();
    let memory_lines: Vec<_> = md.lines().filter(|l| l.starts_with("- ")).collect();
    assert_eq!(memory_lines.len(), 4, "one line per current memory:\n{md}");
    let dentist = memory_lines.iter().find(|l| l.contains("dentist")).unwrap();
    assert!(
        dentist.contains("appointment card.jpg"),
        "with its source: {dentist}"
    );
    assert!(
        memory_lines
            .iter()
            .any(|l| l.contains("Zebrafish47") && l.contains("[private]")),
        "private memories are included and marked"
    );
    assert!(
        memory_lines
            .iter()
            .any(|l| l.contains("clinic") || l.contains("Clinic"))
            && md.contains("[unconfirmed]")
    );
    assert!(
        memory_lines
            .iter()
            .all(|l| !l.starts_with("- Gran lives in Leeds")),
        "earlier versions live in the data file, not the list"
    );

    let page = std::fs::read_to_string(root.join("pages").join(format!("{}.md", w.day))).unwrap();
    assert!(page.starts_with("# "));
    assert!(page.contains("Zebrafish47") && page.contains("[private]"));

    let originals: Vec<_> = std::fs::read_dir(root.join("originals"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(originals.len(), 1);
    assert_eq!(
        std::fs::read(&originals[0]).unwrap(),
        b"JPEG bytes of the card"
    );

    let data: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("nocturne.json")).unwrap())
            .unwrap();
    assert_eq!(
        data["schemaVersion"],
        nocturne_engine::store::latest_version()
    );
    assert_eq!(
        data["memories"].as_array().unwrap().len(),
        5,
        "includes the earlier version"
    );
    assert_eq!(data["items"].as_array().unwrap().len(), 4);
    assert_eq!(data["pages"].as_array().unwrap().len(), 1);
    let private = data["memories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == w.private.as_str())
        .unwrap();
    assert_eq!(private["private"], true);
    assert!(!data.to_string().contains("forgetting"));
}

#[test]
fn a_forgotten_memory_is_not_in_a_later_export() {
    let w = world();
    let p =
        w.f.engine
            .preview_forget(std::slice::from_ref(&w.private), true)
            .unwrap();
    w.f.engine.forget(&p.token).unwrap();
    let out = tempfile::tempdir().unwrap();
    let root = std::path::PathBuf::from(w.f.engine.export_to(out.path()).unwrap().path);
    for entry in ["memories.md", "nocturne.json", "pages/2026-11-14.md"] {
        let text = std::fs::read_to_string(root.join(entry)).unwrap();
        assert!(!text.contains("Zebrafish47"), "{entry}");
    }
}

#[test]
fn delete_everything_needs_the_phrase_and_leaves_an_empty_working_store() {
    let w = world();
    let dir = w.f.dir.path();
    w.f.engine
        .cache(w.day, "af_heart", b"spoken Zebrafish47")
        .unwrap();
    w.f.engine
        .record_download(LedgerKind::Setup, "model", Some("example.org"), 10.0)
        .unwrap();
    {
        let conn = rusqlite::Connection::open(dir.join("nocturne.db")).unwrap();
        conn.execute_batch(
            "INSERT INTO remote_devices(id, name, public_key, home_origin, connection, paired_at) \
               VALUES ('dev1', 'Pixel', x'04', 'https://a.example', 'simple', '2026-11-13T07:00:00.000Z'); \
             INSERT INTO remote_keeps(item_id, device_id, client_id, kept_at) \
               SELECT id, 'dev1', 'c-1', kept_at FROM items LIMIT 1; \
             INSERT INTO remote_events(device_id, action, result, at) \
               VALUES ('dev1', 'keptText', 'ok', '2026-11-13T07:00:00.000Z'); \
             UPDATE remote_state SET wanted_on = 1, sent_bytes = 42, address = 'a.example';",
        )
        .unwrap();
    }
    assert!(w.f.engine.delete_everything("yes").is_err());
    assert!(w.f.engine.delete_everything("Delete everything").is_err());
    assert_eq!(
        w.f.engine.stats().unwrap().memory_count,
        4,
        "nothing happened"
    );

    w.f.engine.delete_everything(DELETE_PHRASE).unwrap();
    for table in [
        "items",
        "memories",
        "memory_sources",
        "entities",
        "pages",
        "page_lines",
        "questions",
        "embeddings",
        "audio_cache",
        "conflicts",
        "ledger",
        "settings",
        "forget_tokens",
        "remote_devices",
        "remote_keeps",
        "remote_events",
    ] {
        assert_eq!(
            sql_count(dir, &format!("SELECT COUNT(*) FROM {table}")),
            0,
            "{table}"
        );
    }
    assert_eq!(
        sql_count(
            dir,
            "SELECT COUNT(*) FROM remote_state \
             WHERE wanted_on = 0 AND sent_bytes = 0 AND address IS NULL"
        ),
        1,
        "Away from home's settings are back to off"
    );
    assert_eq!(std::fs::read_dir(dir.join("originals")).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(dir.join("audio")).unwrap().count(), 0);
    for table in nocturne_engine::store::FTS_TABLES {
        assert_eq!(
            sql_count(
                dir,
                &format!("SELECT COUNT(*) FROM {table} WHERE {table} MATCH 'zebrafish47'")
            ),
            0,
            "{table}"
        );
    }
    for name in ["nocturne.db", "nocturne.db-wal"] {
        let p = dir.join(name);
        if p.exists() {
            let bytes = std::fs::read(p).unwrap().to_ascii_lowercase();
            assert!(!bytes.windows(11).any(|w| w == b"zebrafish47"), "{name}");
            assert!(
                !bytes
                    .windows(7)
                    .any(|w| w == b"leeds\0\0" || w == b"dentist"),
                "{name}"
            );
        }
    }
    // Still usable.
    assert!(w.f.engine.page(None).unwrap().is_none());
    assert!(w.f.engine.retrieve("dentist", None, 5).unwrap().is_empty());
    w.f.engine
        .keep_text("a new beginning", Source::QuickCapture)
        .unwrap();
    assert_eq!(w.f.engine.stats().unwrap().item_count, 1);
    assert_eq!(w.f.engine.settings().unwrap().page_time, "07:00");
}

#[test]
fn ledger_privacy_and_stats() {
    let f = fixture();
    f.engine
        .record_download(LedgerKind::Setup, "Mind", Some("example.org"), 8.4e9)
        .unwrap();
    f.clock.set_str("2026-11-14T07:00:00Z");
    f.engine
        .record_download(LedgerKind::UpdateCheck, "Update check", None, 512.0)
        .unwrap();
    let ledger = f.engine.ledger().unwrap();
    assert_eq!(ledger.len(), 2);
    assert_eq!(
        (ledger[0].kind, ledger[0].bytes),
        (LedgerKind::Setup, 8.4e9)
    );
    assert_eq!(ledger[1].at, "2026-11-14T07:00:00.000Z");
    f.engine.keep_text("a note", Source::Paste).unwrap();
    let p = f.engine.privacy(f.dir.path()).unwrap();
    assert_eq!(p.ledger.len(), 2);
    assert!(!p.sent_measured && p.kept_bytes > 0.0);
    assert!(!p.check_updates && !p.lock);
    let s = f.engine.stats().unwrap();
    assert!(s.integrity_ok);
    assert_eq!((s.item_count, s.memory_count), (1, 0));
    assert_eq!(s.store_version, nocturne_engine::store::latest_version());
}

#[test]
fn export_goes_to_a_fresh_dated_folder_and_never_reuses_a_name() {
    let f = fixture();
    f.engine.keep_text("a note", Source::Paste).unwrap();
    let out = tempfile::tempdir().unwrap();
    let a = std::path::PathBuf::from(f.engine.export_to(out.path()).unwrap().path);
    let b = std::path::PathBuf::from(f.engine.export_to(out.path()).unwrap().path);
    assert_eq!(a.file_name().unwrap(), "Nocturne export 2026-11-13");
    assert_eq!(b.file_name().unwrap(), "Nocturne export 2026-11-13 (1)");
    assert!(a.join("nocturne.json").exists() && b.join("nocturne.json").exists());
}

#[cfg(unix)]
#[test]
fn a_symlink_waiting_in_the_target_folder_is_not_followed() {
    let f = fixture();
    f.engine.keep_text("a note", Source::Paste).unwrap();
    let out = tempfile::tempdir().unwrap();
    let victim = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(victim.path(), out.path().join("Nocturne export 2026-11-13"))
        .unwrap();
    let root = std::path::PathBuf::from(f.engine.export_to(out.path()).unwrap().path);
    assert_eq!(root.file_name().unwrap(), "Nocturne export 2026-11-13 (1)");
    assert_eq!(
        std::fs::read_dir(victim.path()).unwrap().count(),
        0,
        "nothing was written through the link"
    );
}

#[cfg(unix)]
#[test]
fn export_folders_and_files_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    let w = world();
    let out = tempfile::tempdir().unwrap();
    let root = std::path::PathBuf::from(w.f.engine.export_to(out.path()).unwrap().path);
    for d in [root.clone(), root.join("pages"), root.join("originals")] {
        assert_eq!(mode(&d), 0o700, "{d:?}");
    }
    let mut files = vec![
        root.join("README.txt"),
        root.join("memories.md"),
        root.join("nocturne.json"),
    ];
    for sub in ["pages", "originals"] {
        for e in std::fs::read_dir(root.join(sub)).unwrap() {
            files.push(e.unwrap().path());
        }
    }
    assert!(files.len() >= 5);
    for f in files {
        assert_eq!(mode(&f), 0o600, "{f:?}");
    }
}

// ---- export fails closed ----

fn kept_photo(f: &Fixture, bytes: &[u8], name: &str) -> String {
    let src = tempfile::tempdir().unwrap();
    match f
        .engine
        .keep_file(&write_file(src.path(), name, bytes), Source::Screenshot)
        .unwrap()
    {
        Kept::New { item } => item.id,
        _ => unreachable!(),
    }
}

#[test]
fn an_error_inside_the_export_leaves_no_folder_behind() {
    let f = fixture();
    kept_photo(&f, b"JPEG bytes", "card.jpg");
    // Replace the stored original with a directory: opening it works, reading it fails.
    let originals = f.dir.path().join("originals");
    let stored = std::fs::read_dir(&originals)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::remove_file(&stored).unwrap();
    std::fs::create_dir(&stored).unwrap();

    let out = tempfile::tempdir().unwrap();
    assert!(f.engine.export_to(out.path()).is_err());
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
}

#[test]
fn an_item_in_a_pending_forget_that_removes_originals_is_not_exported() {
    let f = fixture();
    let photo = kept_photo(&f, b"JPEG bytes of the secret card", "secret.jpg");
    let plain = keep(&f.engine, "an ordinary note");
    f.engine
        .apply(
            &photo,
            vec![cand("Zed", Predicate::Knows, "x", "Zed knows x.")],
        )
        .unwrap();
    let mem = f.engine.memories(Default::default()).unwrap()[0].id.clone();
    let p = f
        .engine
        .preview_forget(std::slice::from_ref(&mem), true)
        .unwrap();
    // The forget has started and not finished (as when the app stopped part way).
    f.engine.forget_begin_only(&p.token).unwrap();

    let out = tempfile::tempdir().unwrap();
    let root = std::path::PathBuf::from(f.engine.export_to(out.path()).unwrap().path);
    let data: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("nocturne.json")).unwrap())
            .unwrap();
    let ids: Vec<&str> = data["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [plain.as_str()]);
    assert_eq!(
        std::fs::read_dir(root.join("originals")).unwrap().count(),
        0
    );
}

#[test]
fn an_export_that_ran_across_delete_everything_is_discarded() {
    let f = fixture();
    keep(&f.engine, "a note");
    kept_photo(&f, b"JPEG bytes", "card.jpg");
    let out = tempfile::tempdir().unwrap();
    let fired = std::cell::Cell::new(false);
    let err = f
        .engine
        .export_to_with(out.path(), &|done, _| {
            if done == 1 && !fired.get() {
                fired.set(true);
                f.engine.delete_everything(DELETE_PHRASE).unwrap();
            }
        })
        .unwrap_err();
    assert!(fired.get());
    assert!(matches!(err, nocturne_engine::Error::Busy), "{err:?}");
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
}

#[test]
fn an_export_that_ran_across_a_finishing_forget_is_discarded() {
    let f = fixture();
    let (_, mem) = learn(
        &f.engine,
        "Zed note",
        cand("Zed", Predicate::Knows, "x", "Zed knows x."),
    );
    let p = f
        .engine
        .preview_forget(std::slice::from_ref(&mem), true)
        .unwrap();
    f.engine.forget_begin_only(&p.token).unwrap();
    keep(&f.engine, "another note");
    let out = tempfile::tempdir().unwrap();
    let fired = std::cell::Cell::new(false);
    let err = f
        .engine
        .export_to_with(out.path(), &|done, _| {
            if done == 1 && !fired.get() {
                fired.set(true);
                // Finishing the pending job (as the next open does) removes items and files.
                f.engine.resume_forgets_for_test().unwrap();
            }
        })
        .unwrap_err();
    assert!(fired.get());
    assert!(matches!(err, nocturne_engine::Error::Busy), "{err:?}");
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
}

/// What was renamed and moved is exported as it stands: former names and where a memory was
/// first filed are there, and a thing merged into another is not listed on its own.
#[test]
fn the_export_has_former_names_and_moves_and_leaves_out_merged_in_things() {
    use nocturne_contract::{EntityKind, RefileTarget};
    let f = fixture();
    let claim = |subject: &str, object: &str| nocturne_engine::Candidate {
        subject_kind: Some(EntityKind::Thing),
        ..cand(
            subject,
            Predicate::Knows,
            object,
            &format!("{subject}: {object}."),
        )
    };
    let (_, a) = learn(
        &f.engine,
        "Trinks are 5 pounds",
        claim("Trinks", "5 pounds"),
    );
    let (_, b) = learn(&f.engine, "Drinks are cold", claim("Drinks", "cold"));
    let (_, c) = learn(&f.engine, "Pops are red", claim("Pops", "red"));
    let ent = |m: &str| f.engine.get_memory(m).unwrap().entity_id.unwrap();
    let (ea, eb) = (ent(&a), ent(&b));
    f.engine.rename_entity(&eb, "Cold drinks").unwrap();
    f.engine.merge_entities(&ea, &eb).unwrap();
    f.engine
        .refile_memory(
            &c,
            RefileTarget::Existing {
                entity_id: eb.clone(),
            },
        )
        .unwrap();

    let out = tempfile::tempdir().unwrap();
    let root = std::path::PathBuf::from(f.engine.export_to(out.path()).unwrap().path);
    let data: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("nocturne.json")).unwrap())
            .unwrap();
    let entities = data["entities"].as_array().unwrap();
    assert!(
        entities.iter().all(|e| e["id"] != ea),
        "merged in: {entities:?}"
    );
    let kept = entities.iter().find(|e| e["id"] == eb).unwrap();
    assert_eq!(kept["name"], "Cold drinks");
    let former: Vec<&str> = kept["formerNames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    assert!(
        former.contains(&"Drinks") && former.contains(&"Trinks"),
        "{former:?}"
    );
    let moved = data["memories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == c)
        .unwrap();
    assert_eq!(moved["movedFrom"], "Pops");
    assert_eq!(moved["entityId"], eb);
}

#[test]
fn delete_everything_resets_online_and_reindex() {
    let w = world();
    let dir = w.f.dir.path();
    {
        let conn = rusqlite::Connection::open(dir.join("nocturne.db")).unwrap();
        conn.execute_batch(
            "UPDATE online_state SET wanted_on = 1, set_up = 1, base_url = 'https://a.example/v1', \
               model = 'm', send_sensitive = 1, sent_bytes = 42, key_storage = 'file'; \
             INSERT INTO online_sends(at, kind, host, model, question, template, result) \
               VALUES ('2026-11-13T07:00:00.000Z', 'question', 'a.example', 'm', \
                 'When are the bins?', '[]', 'answered'); \
             INSERT INTO embeddings_next(memory_id, model, dim, vec) \
               SELECT id, 'other', 2, x'0000803f00000000' FROM memories LIMIT 1; \
             UPDATE reindex_state SET model = 'other', from_model = 'old', total = 4, \
               phase = 'running';",
        )
        .unwrap();
    }
    w.f.engine.delete_everything(DELETE_PHRASE).unwrap();
    for table in ["online_sends", "embeddings_next"] {
        assert_eq!(
            sql_count(dir, &format!("SELECT COUNT(*) FROM {table}")),
            0,
            "{table}"
        );
    }
    assert_eq!(
        sql_count(
            dir,
            "SELECT COUNT(*) FROM online_state WHERE wanted_on = 0 AND set_up = 0 \
             AND base_url IS NULL AND model IS NULL AND send_sensitive = 0 AND phones = 1 \
             AND sent_bytes = 0 AND key_storage IS NULL AND limit_questions = 200"
        ),
        1,
        "Online answers is back to off, one row"
    );
    assert_eq!(
        sql_count(
            dir,
            "SELECT COUNT(*) FROM reindex_state WHERE model IS NULL AND phase IS NULL AND total = 0"
        ),
        1,
        "no re-read under way, one row"
    );
}
