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
        .apply_candidates(
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
    f.engine.correct(&old, "York").unwrap();
    let web = keep_as(&f.engine, "web claim", Source::PastedWeb);
    f.engine
        .apply_candidates(
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
    assert_eq!((report.memories, report.pages, report.originals), (4, 1, 1));

    let readme = std::fs::read_to_string(out.path().join("README.txt")).unwrap();
    assert!(readme.contains("nocturne.json"));

    let md = std::fs::read_to_string(out.path().join("memories.md")).unwrap();
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

    let page =
        std::fs::read_to_string(out.path().join("pages").join(format!("{}.md", w.day))).unwrap();
    assert!(page.starts_with("# "));
    assert!(page.contains("Zebrafish47") && page.contains("[private]"));

    let originals: Vec<_> = std::fs::read_dir(out.path().join("originals"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(originals.len(), 1);
    assert_eq!(
        std::fs::read(&originals[0]).unwrap(),
        b"JPEG bytes of the card"
    );

    let data: Value =
        serde_json::from_str(&std::fs::read_to_string(out.path().join("nocturne.json")).unwrap())
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
            .preview_forget(&[w.private.clone()], true)
            .unwrap();
    w.f.engine.forget(&p.token).unwrap();
    let out = tempfile::tempdir().unwrap();
    w.f.engine.export_to(out.path()).unwrap();
    for entry in ["memories.md", "nocturne.json", "pages/2026-11-14.md"] {
        let text = std::fs::read_to_string(out.path().join(entry)).unwrap();
        assert!(!text.contains("Zebrafish47"), "{entry}");
    }
}

#[test]
fn delete_everything_needs_the_phrase_and_leaves_an_empty_working_store() {
    let w = world();
    let dir = w.f.dir.path();
    w.f.engine
        .cache_audio(w.day, "af_heart", b"spoken Zebrafish47")
        .unwrap();
    w.f.engine
        .record_download(LedgerKind::Setup, "model", Some("example.org"), 10.0)
        .unwrap();
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
    ] {
        assert_eq!(
            sql_count(dir, &format!("SELECT COUNT(*) FROM {table}")),
            0,
            "{table}"
        );
    }
    assert_eq!(std::fs::read_dir(dir.join("originals")).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(dir.join("audio")).unwrap().count(), 0);
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
