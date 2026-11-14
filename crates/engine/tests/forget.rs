mod common;

use common::*;
use nocturne_contract::{Edition, Kept, LineKind, MemoryFilter, Source};
use nocturne_engine::{Error, Predicate};

const SECRET: &str = "zebrafish47";

fn contains_secret(bytes: &[u8]) -> bool {
    let lower: Vec<u8> = bytes.iter().map(u8::to_ascii_lowercase).collect();
    lower.windows(SECRET.len()).any(|w| w == SECRET.as_bytes())
}

fn grep_dir(dir: &std::path::Path) -> Vec<String> {
    let mut hits = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            hits.extend(grep_dir(&path));
        } else if contains_secret(&std::fs::read(&path).unwrap()) {
            hits.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    hits
}

struct World {
    f: Fixture,
    m1: String,
    m1_old: String,
    m2: String,
    note: String,
    photo: String,
    pdf: String,
    day: jiff::civil::Date,
}

fn candidate_for_m1(object: &str) -> nocturne_engine::Candidate {
    let mut c = cand(
        "Dr Zebrafish47",
        Predicate::Knows,
        object,
        &format!("Dr Zebrafish47 sees you {object}."),
    );
    c.subject_kind = Some(nocturne_contract::EntityKind::Person);
    c
}

fn world() -> World {
    let f = fixture();
    let src = tempfile::tempdir().unwrap();
    let id = |k: Kept| match k {
        Kept::New { item } => item.id,
        Kept::Duplicate { item } => item.id,
    };
    // M1 is cited by a note, a photo and a pdf; M2 is cited by the pdf only.
    let note = keep(
        &f.engine,
        "Appointment with Dr Zebrafish47 on Tuesday at 14:00",
    );
    let photo = id(f
        .engine
        .keep_file(
            &write_file(src.path(), "card.jpg", b"JPEG card for Dr Zebrafish47"),
            Source::Screenshot,
        )
        .unwrap());
    let pdf = id(f
        .engine
        .keep_file(
            &write_file(src.path(), "pets.pdf", b"PDF about the cat Pip"),
            Source::Drop,
        )
        .unwrap());
    for item in [&note, &photo, &pdf] {
        f.engine
            .apply(item, vec![candidate_for_m1("on Tuesday")])
            .unwrap();
    }
    let r = f
        .engine
        .apply(
            &pdf,
            vec![
                candidate_for_m1("on Tuesday"),
                cand("Pip", Predicate::Knows, "a cat", "Pip is a cat."),
            ],
        )
        .unwrap();
    let m2 = r.created[0].clone();
    let m1_old = f
        .engine
        .memories(MemoryFilter {
            query: Some("zebrafish47".into()),
            ..Default::default()
        })
        .unwrap()[0]
        .id
        .clone();
    f.clock.set_str("2026-11-13T09:00:00Z");
    let m1 = f
        .engine
        .correct(
            &m1_old,
            nocturne_engine::CorrectPatch::object("on Wednesday"),
        )
        .unwrap();
    f.engine
        .store_embeddings(vec![
            (m1.clone(), vec![1.0, 0.0]),
            (m1_old.clone(), vec![0.9, 0.1]),
            (m2.clone(), vec![0.0, 1.0]),
        ])
        .unwrap();

    f.clock.set_str("2026-11-14T06:00:00Z");
    let day = date("2026-11-14");
    let page = f.engine.compose_page(day, Edition::Screen).unwrap();
    f.engine.save_page(&page).unwrap();
    f.engine
        .cache(
            "2026-11-14",
            "af_heart",
            b"spoken: Dr Zebrafish47 sees you on Wednesday",
        )
        .unwrap();
    World {
        f,
        m1,
        m1_old,
        m2,
        note,
        photo,
        pdf,
        day,
    }
}

#[test]
fn forget_leaves_no_trace() {
    // Review focus 1: FTS rows, vectors, sources, history, cached audio, page lines, raw bytes.
    let w = world();
    let f = &w.f;
    let dir = f.dir.path();
    assert!(
        !grep_dir(dir).is_empty(),
        "the fixture really contains the secret"
    );
    let page = f.engine.page(Some(w.day)).unwrap().unwrap();
    assert!(
        page.learned
            .iter()
            .chain(&page.changed)
            .any(|l| l.memory_ids.contains(&w.m1))
    );

    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&w.m1), true)
        .unwrap();
    assert_eq!(
        preview.memories.len(),
        2,
        "the memory and its earlier version"
    );
    assert!(preview.memories.iter().any(|r| r.memory_id == w.m1_old));
    assert_eq!(preview.pages_affected, 1);
    let by_id = |id: &str| preview.originals.iter().find(|o| o.item_id == id).unwrap();
    assert!(by_id(&w.note).removable);
    assert!(by_id(&w.photo).removable);
    assert!(
        !by_id(&w.pdf).removable,
        "another memory still depends on the pdf"
    );
    assert_eq!(by_id(&w.pdf).other_memories, 1);
    // Previewing changes nothing.
    assert_eq!(f.engine.memories(MemoryFilter::default()).unwrap().len(), 2);

    let report = f.engine.forget(&preview.token).unwrap().report;
    assert_eq!(report.memories_removed, 2);
    assert_eq!(report.originals_removed, 2);
    assert_eq!(report.pages_redacted, 1);

    let q = |sql: &str| sql_count(dir, sql);
    // Memories, history, sources, vectors, entity.
    assert_eq!(
        q(
            "SELECT COUNT(*) FROM memories WHERE sentence LIKE '%Zebrafish47%' OR object LIKE '%Zebrafish47%'"
        ),
        0
    );
    assert_eq!(q("SELECT COUNT(*) FROM memories"), 1);
    assert_eq!(q("SELECT COUNT(*) FROM memory_sources"), 1);
    assert_eq!(q("SELECT COUNT(*) FROM embeddings"), 1);
    assert_eq!(
        q("SELECT COUNT(*) FROM entities WHERE name LIKE '%Zebrafish47%'"),
        0
    );
    // Search rows.
    for table in nocturne_engine::store::FTS_TABLES {
        assert_eq!(
            q(&format!(
                "SELECT COUNT(*) FROM {table} WHERE {table} MATCH 'zebrafish47'"
            )),
            0,
            "{table}"
        );
    }
    // Originals: the note and the photo go, the pdf stays.
    assert_eq!(q("SELECT COUNT(*) FROM items"), 1);
    assert!(f.engine.get_item(&w.pdf).is_ok());
    assert!(f.engine.get_item(&w.note).is_err() && f.engine.get_item(&w.photo).is_err());
    assert_eq!(std::fs::read_dir(dir.join("originals")).unwrap().count(), 1);
    // Cached audio.
    assert_eq!(q("SELECT COUNT(*) FROM audio_cache"), 0);
    assert!(
        f.engine
            .cached_audio("2026-11-14", "af_heart")
            .unwrap()
            .is_none()
    );
    assert_eq!(std::fs::read_dir(dir.join("audio")).unwrap().count(), 0);
    // Page lines: redacted, the rest of the page stays.
    assert_eq!(q("SELECT COUNT(*) FROM page_lines WHERE forgotten = 1"), 1);
    assert_eq!(
        q("SELECT COUNT(*) FROM page_lines WHERE forgotten = 1 AND text != ''"),
        0
    );
    assert_eq!(
        q(&format!(
            "SELECT COUNT(*) FROM page_line_memories WHERE memory_id IN ('{}', '{}')",
            w.m1, w.m1_old
        )),
        0
    );
    let page = f.engine.page(Some(w.day)).unwrap().unwrap();
    let all: Vec<_> = page
        .needs_you
        .iter()
        .chain(&page.changed)
        .chain(&page.learned)
        .chain(&page.upcoming)
        .chain(&page.let_go)
        .collect();
    let redacted: Vec<_> = all.iter().filter(|l| l.forgotten).collect();
    assert_eq!(redacted.len(), 1);
    assert!(
        redacted[0].text.is_empty()
            && redacted[0].receipts.is_empty()
            && redacted[0].memory_ids.is_empty()
    );
    assert!(
        all.iter()
            .any(|l| !l.forgotten && l.memory_ids.contains(&w.m2)),
        "the rest of the page stays"
    );
    assert!(
        !page.headline.to_lowercase().contains(SECRET) && !page.dek.to_lowercase().contains(SECRET)
    );
    assert_eq!(page.headline, "Saturday 14 November");
    // Raw bytes: the database, its write-ahead log and shared memory, originals and audio.
    for name in ["nocturne.db", "nocturne.db-wal", "nocturne.db-shm"] {
        let p = dir.join(name);
        if p.exists() {
            assert!(
                !contains_secret(&std::fs::read(&p).unwrap()),
                "{name} still holds the secret"
            );
        }
    }
    assert!(grep_dir(dir).is_empty(), "{:?}", grep_dir(dir));
    let wal = dir.join("nocturne.db-wal");
    if wal.exists() {
        assert_eq!(
            std::fs::metadata(wal).unwrap().len(),
            0,
            "the log is truncated"
        );
    }
    // The other memory is untouched.
    assert_eq!(
        f.engine.get_memory(&w.m2).unwrap().sentence,
        "Pip is a cat."
    );
    // Even after reopening.
    let again = nocturne_engine::Engine::open(dir, f.clock.clone()).unwrap();
    assert_eq!(again.memories(MemoryFilter::default()).unwrap().len(), 1);
}

#[test]
fn forgetting_without_originals_keeps_the_items() {
    let w = world();
    let preview =
        w.f.engine
            .preview_forget(std::slice::from_ref(&w.m1), false)
            .unwrap();
    let r = w.f.engine.forget(&preview.token).unwrap().report;
    assert_eq!(r.originals_removed, 0);
    assert!(w.f.engine.get_item(&w.note).is_ok());
    assert!(w.f.engine.get_item(&w.photo).is_ok());
}

#[test]
fn tokens_are_single_use_and_go_stale() {
    let w = world();
    let e = &w.f.engine;
    assert!(matches!(e.forget("nope"), Err(Error::StaleToken)));

    // Reused.
    let p = e
        .preview_forget(std::slice::from_ref(&w.m2), false)
        .unwrap();
    e.forget(&p.token).unwrap();
    assert!(matches!(e.forget(&p.token), Err(Error::StaleToken)));

    // Changed since the preview: the memory was used.
    let p = e
        .preview_forget(std::slice::from_ref(&w.m1), false)
        .unwrap();
    w.f.clock.set_str("2026-11-14T06:01:00Z");
    e.mark_used(
        std::slice::from_ref(&w.m1),
        nocturne_engine::UseKind::Answer,
    )
    .unwrap();
    assert!(matches!(e.forget(&p.token), Err(Error::StaleToken)));
    assert!(
        e.get_memory(&w.m1).is_ok(),
        "a refused forget removes nothing"
    );

    // Expired after ten minutes.
    let p = e
        .preview_forget(std::slice::from_ref(&w.m1), false)
        .unwrap();
    w.f.clock.set_str("2026-11-14T06:12:00Z");
    assert!(matches!(e.forget(&p.token), Err(Error::StaleToken)));
    assert!(e.get_memory(&w.m1).is_ok());

    // A fresh one works.
    let p = e
        .preview_forget(std::slice::from_ref(&w.m1), false)
        .unwrap();
    e.forget(&p.token).unwrap();
    assert!(e.get_memory(&w.m1).is_err());
    assert!(e.preview_forget(&["mem_missing".into()], false).is_err());
}

#[test]
fn forgetting_one_side_of_a_conflict_removes_the_conflict() {
    let f = fixture();
    let (_, a) = learn(
        &f.engine,
        "Tuesday",
        cand(
            "Dentist appointment",
            Predicate::Scheduled,
            "Tuesday",
            "Dentist is Tuesday.",
        ),
    );
    let item = keep(&f.engine, "Thursday");
    let r = f
        .engine
        .apply(
            &item,
            vec![cand(
                "Dentist appointment",
                Predicate::Scheduled,
                "Thursday",
                "Dentist is Thursday.",
            )],
        )
        .unwrap();
    assert_eq!(f.engine.needs_you().unwrap().len(), 1);
    let p = f
        .engine
        .preview_forget(&[r.created[0].clone()], false)
        .unwrap();
    f.engine.forget(&p.token).unwrap();
    assert!(f.engine.needs_you().unwrap().is_empty());
    assert_eq!(sql_count(f.dir.path(), "SELECT COUNT(*) FROM conflicts"), 0);
    assert!(f.engine.get_memory(&a).is_ok());
}

#[test]
fn making_a_memory_private_removes_cached_audio_of_pages_citing_it() {
    let w = world();
    let e = &w.f.engine;
    assert!(e.cached_audio("2026-11-14", "af_heart").unwrap().is_some());
    e.set_private(&w.m1, true).unwrap();
    assert!(e.cached_audio("2026-11-14", "af_heart").unwrap().is_none());
    // The audio and print editions of the saved page leave the line out; the screen keeps it.
    let screen = e.page_for(Some(w.day), Edition::Screen).unwrap().unwrap();
    let audio = e.page_for(Some(w.day), Edition::Audio).unwrap().unwrap();
    let cites = |p: &nocturne_contract::PageView| {
        p.needs_you
            .iter()
            .chain(&p.changed)
            .chain(&p.learned)
            .any(|l| l.memory_ids.contains(&w.m1))
    };
    assert!(cites(&screen));
    assert!(!cites(&audio));
    assert!(audio.private_skipped >= 1);
    let _ = LineKind::Learned;
}

#[test]
fn a_page_that_cites_a_memory_only_through_the_daily_question_is_redacted() {
    let f = fixture();
    let (_, m) = learn(
        &f.engine,
        "n",
        cand(
            "Zebrafish47",
            Predicate::Prefers,
            "swimming",
            "Zebrafish47 likes swimming.",
        ),
    );
    f.clock.set_str("2026-11-14T06:00:00Z");
    let p1 = compose_save(&f, "2026-11-14");
    assert!(p1.question.is_some());
    f.clock.set_str("2026-11-14T20:00:00Z");
    let p2 = compose_save(&f, "2026-11-15");
    let q = p2.question.clone().unwrap();
    assert_eq!(
        q.memory_id.as_deref(),
        Some(m.as_str()),
        "the question names the memory"
    );
    assert!(
        lines(&p2).iter().all(|l| !l.memory_ids.contains(&m)),
        "no line cites it"
    );
    f.engine
        .cache(
            "2026-11-15",
            "af_heart",
            b"spoken: Zebrafish47 likes swimming",
        )
        .unwrap();
    f.engine
        .cache(
            "2026-11-14",
            "af_heart",
            b"spoken: Zebrafish47 likes swimming",
        )
        .unwrap();

    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&m), true)
        .unwrap();
    assert_eq!(preview.pages_affected, 2, "the question page counts too");
    f.engine.forget(&preview.token).unwrap();

    let after = f.engine.page(Some(date("2026-11-15"))).unwrap().unwrap();
    assert!(after.question.is_none(), "the question is redacted");
    assert!(
        !serde_json::to_string(&after)
            .unwrap()
            .to_lowercase()
            .contains(SECRET)
    );
    assert!(
        f.engine
            .cached_audio("2026-11-15", "af_heart")
            .unwrap()
            .is_none()
    );
    assert!(
        f.engine
            .cached_audio("2026-11-14", "af_heart")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM questions WHERE memory_id IS NOT NULL"
        ),
        0
    );
    assert!(
        grep_dir(f.dir.path()).is_empty(),
        "{:?}",
        grep_dir(f.dir.path())
    );
    for name in ["nocturne.db", "nocturne.db-wal"] {
        let p = f.dir.path().join(name);
        if p.exists() {
            assert!(!contains_secret(&std::fs::read(p).unwrap()), "{name}");
        }
    }
}

fn compose_save(f: &Fixture, day: &str) -> nocturne_contract::PageView {
    let p = f.engine.compose_page(date(day), Edition::Screen).unwrap();
    f.engine.save_page(&p).unwrap();
    p
}

fn lines(p: &nocturne_contract::PageView) -> Vec<&nocturne_contract::PageLine> {
    p.needs_you
        .iter()
        .chain(&p.changed)
        .chain(&p.learned)
        .chain(&p.upcoming)
        .chain(&p.let_go)
        .collect()
}

#[test]
fn a_forget_hides_everything_at_once_and_a_crash_between_phases_is_finished_on_open() {
    let w = world();
    let day = w.day;
    let dir = w.f.dir.path().to_path_buf();
    let e = &w.f.engine;
    // A page composed before the forget, saved after it began.
    let stale = e.compose_page(day, Edition::Screen).unwrap();
    assert!(lines(&stale).iter().any(|l| l.memory_ids.contains(&w.m1)));

    let preview = e.preview_forget(std::slice::from_ref(&w.m1), true).unwrap();
    e.forget_begin_only(&preview.token).unwrap();

    // From this moment nothing shows the memories, though the app has "stopped" before purging.
    let listed = e.memories(Default::default()).unwrap();
    assert!(listed.iter().all(|m| m.id != w.m1 && m.id != w.m1_old));
    assert!(e.get_memory(&w.m1).is_err());
    assert!(
        e.retrieve("zebrafish47 Wednesday", Some(&[1.0, 0.0]), 5)
            .unwrap()
            .iter()
            .all(|h| h.memory.id == w.m2)
    );
    assert!(e.needs_you().unwrap().is_empty());
    let composed = e.compose_page(day, Edition::Screen).unwrap();
    assert!(
        lines(&composed)
            .iter()
            .all(|l| !l.memory_ids.contains(&w.m1))
    );
    assert!(e.cached_audio("2026-11-14", "af_heart").unwrap().is_none());
    let saved = e.page(Some(day)).unwrap().unwrap();
    assert!(lines(&saved).iter().all(|l| !l.memory_ids.contains(&w.m1)));
    // The stale page cannot bring the lines back.
    e.save_page(&stale).unwrap();
    let saved = e.page(Some(day)).unwrap().unwrap();
    assert!(
        !serde_json::to_string(&saved)
            .unwrap()
            .to_lowercase()
            .contains(SECRET)
    );
    assert!(!saved.headline.to_lowercase().contains(SECRET));
    assert_eq!(sql_count(&dir, "SELECT COUNT(*) FROM forget_jobs"), 1);
    assert_eq!(
        sql_count(&dir, "SELECT COUNT(*) FROM memories WHERE forgetting = 1"),
        2
    );

    // Reopening finishes the job, including the originals the person asked to remove.
    let clock = w.f.clock.clone();
    let Fixture {
        dir: tmp, engine, ..
    } = w.f;
    drop(engine);
    let reopened = nocturne_engine::Engine::open(tmp.path(), clock).unwrap();
    assert_eq!(sql_count(&dir, "SELECT COUNT(*) FROM forget_jobs"), 0);
    assert_eq!(sql_count(&dir, "SELECT COUNT(*) FROM memories"), 1);
    assert_eq!(sql_count(&dir, "SELECT COUNT(*) FROM items"), 1);
    assert!(reopened.get_item(&w.pdf).is_ok());
    assert!(grep_dir(&dir).is_empty(), "{:?}", grep_dir(&dir));
    for name in ["nocturne.db", "nocturne.db-wal"] {
        let p = dir.join(name);
        if p.exists() {
            assert!(!contains_secret(&std::fs::read(p).unwrap()), "{name}");
        }
    }
}
