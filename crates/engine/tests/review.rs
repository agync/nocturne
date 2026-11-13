//! Regression tests for the barrier review. Each one was a bug first.

mod common;

use std::time::Duration;

use common::*;
use nocturne_contract::{Edition, MemoryFilter, MemoryStatus, NeedsYouEntry, Source};
use nocturne_engine::{CorrectPatch, Engine, Error, Predicate, UseKind};

fn all_text(p: &nocturne_contract::PageView) -> String {
    p.needs_you
        .iter()
        .chain(&p.changed)
        .chain(&p.learned)
        .chain(&p.upcoming)
        .chain(&p.let_go)
        .map(|l| l.text.clone())
        .collect::<Vec<_>>()
        .join("|")
}

fn save_day(f: &Fixture, day: &str) -> nocturne_contract::PageView {
    let p = f.engine.compose_page(date(day), Edition::Screen).unwrap();
    f.engine.save_page(&p).unwrap();
    p
}

// ---- 1. a re-read never silently deletes what depends on it -----------------------------------

#[test]
fn reapply_keeps_a_private_memory_cited_on_a_page_and_audio_does_not_read_it() {
    let f = fixture();
    let (item, m) = learn(
        &f.engine,
        "secret note",
        cand(
            "Sam",
            Predicate::Knows,
            "the diagnosis",
            "Sam knows the diagnosis.",
        ),
    );
    f.engine.set_private(&m, true).unwrap();
    f.clock.set_str("2026-11-14T06:00:00Z");
    let day = date("2026-11-14");
    save_day(&f, "2026-11-14");
    let r = f.engine.apply(&item, vec![]).unwrap();
    assert!(
        r.removed.is_empty(),
        "a memory a saved page cites is not deleted"
    );
    let kept = f.engine.get_memory(&m).unwrap();
    assert_eq!(
        kept.status,
        MemoryStatus::Unconfirmed,
        "nothing supports it now"
    );
    assert!(kept.private);
    assert_eq!(kept.receipts.len(), 1, "it keeps a link to the item");
    for edition in [Edition::Audio, Edition::Print] {
        let a = f.engine.page_for(Some(day), edition).unwrap().unwrap();
        assert!(!all_text(&a).contains("diagnosis"), "{edition:?}");
    }
}

#[test]
fn reapply_keeps_keep_forever_used_and_confirmed_memories_but_deletes_the_unused() {
    let f = fixture();
    let (item, pills) = learn(
        &f.engine,
        "pills",
        cand(
            "I",
            Predicate::Takes,
            "metformin 500mg",
            "You take metformin 500mg.",
        ),
    );
    assert!(f.engine.get_memory(&pills).unwrap().keep_forever);
    let r = f.engine.apply(&item, vec![]).unwrap();
    assert!(
        r.removed.is_empty(),
        "keep forever is never deleted by a re-read"
    );
    assert_eq!(
        f.engine.get_memory(&pills).unwrap().status,
        MemoryStatus::Unconfirmed
    );

    let (item, used) = learn(
        &f.engine,
        "n2",
        cand("Leo", Predicate::Knows, "Sam", "Leo knows Sam."),
    );
    f.engine
        .mark_used(std::slice::from_ref(&used), UseKind::Answer)
        .unwrap();
    assert!(f.engine.apply(&item, vec![]).unwrap().removed.is_empty());

    let (item, confirmed) = learn(
        &f.engine,
        "n3",
        cand("Gran", Predicate::Knows, "Pip", "Gran knows Pip."),
    );
    f.engine.confirm(&confirmed, true).unwrap();
    assert!(f.engine.apply(&item, vec![]).unwrap().removed.is_empty());
    assert_eq!(
        f.engine.get_memory(&confirmed).unwrap().status,
        MemoryStatus::Active,
        "the person's word stands"
    );

    let (item, plain) = learn(
        &f.engine,
        "n4",
        cand("Pat", Predicate::Knows, "Max", "Pat knows Max."),
    );
    let r = f.engine.apply(&item, vec![]).unwrap();
    assert_eq!(r.removed, vec![plain.clone()]);
    assert!(f.engine.get_memory(&plain).is_err());
}

#[test]
fn a_line_citing_an_unknown_memory_fails_closed_for_audio_and_print() {
    let f = fixture();
    learn(
        &f.engine,
        "n",
        cand("Leo", Predicate::Knows, "Sam", "Leo knows Sam."),
    );
    f.clock.set_str("2026-11-14T06:00:00Z");
    let day = date("2026-11-14");
    save_day(&f, "2026-11-14");
    assert!(
        all_text(
            &f.engine
                .page_for(Some(day), Edition::Audio)
                .unwrap()
                .unwrap()
        )
        .contains("Leo")
    );
    // The row disappears behind the engine's back.
    let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn.execute("DELETE FROM memories", []).unwrap();
    drop(conn);
    for edition in [Edition::Audio, Edition::Print] {
        let a = f.engine.page_for(Some(day), edition).unwrap().unwrap();
        assert!(!all_text(&a).contains("Leo"), "{edition:?} fails closed");
    }
}

// ---- 2. due dates ------------------------------------------------------------------------------

#[test]
fn a_due_memory_does_not_decay_before_its_date_and_an_undated_one_still_shows() {
    let f = fixture();
    let mut c = cand(
        "car tax",
        Predicate::Due,
        "1 April 2027",
        "Car tax is due on 1 April 2027.",
    );
    c.due = Some(date("2027-04-01"));
    let (_, m) = learn(&f.engine, "car tax", c);
    let (_, undated) = learn(
        &f.engine,
        "mot",
        cand("MOT", Predicate::Due, "soon", "The MOT is due soon."),
    );
    // Undated: like an undated promise, it is listed, after the dated ones, with an empty date.
    f.clock.set_str("2026-11-14T07:00:00Z");
    let ny = f.engine.needs_you().unwrap();
    let last = ny.last().unwrap();
    assert!(
        matches!(last, NeedsYouEntry::Due { memory, due, .. } if memory.id == undated && due.is_empty())
    );
    let page = f
        .engine
        .compose_page(date("2026-11-14"), Edition::Screen)
        .unwrap();
    assert_eq!(page.needs_you.last().unwrap().on, None);

    // A deadline never fades before its date (the undated one decays like anything else).
    f.clock.set_str("2027-03-25T07:00:00Z");
    f.engine.decay().unwrap();
    assert_eq!(
        f.engine.get_memory(&m).unwrap().status,
        MemoryStatus::Active,
        "a deadline never fades before its date"
    );
    let ny = f.engine.needs_you().unwrap();
    assert!(ny.iter().any(|e| matches!(e, NeedsYouEntry::Due { memory, due, .. } if memory.id == m && due == "2027-04-01")));
}

// ---- 3. audio staleness ------------------------------------------------------------------------

#[test]
fn resaving_a_page_drops_its_audio_and_a_stale_stamp_is_refused() {
    let f = fixture();
    let (_, m) = learn(
        &f.engine,
        "zebra",
        cand("Zed", Predicate::Knows, "zebrafish", "Zed knows zebrafish."),
    );
    f.clock.set_str("2026-11-14T05:00:00Z");
    let p = save_day(&f, "2026-11-14");
    f.engine
        .cache("2026-11-14", "v", b"Zed knows zebrafish")
        .unwrap();
    assert!(f.engine.cached_audio("2026-11-14", "v").unwrap().is_some());
    let mut p2 = p.clone();
    p2.learned.clear();
    p2.changed.clear();
    p2.needs_you.clear();
    f.engine.save_page(&p2).unwrap();
    assert!(
        f.engine.cached_audio("2026-11-14", "v").unwrap().is_none(),
        "a re-save drops the audio of its date"
    );
    assert_eq!(
        std::fs::read_dir(f.dir.path().join("audio"))
            .unwrap()
            .count(),
        0
    );

    // A stamp taken before a change is refused.
    let stamp = f.engine.audio_stamp("2026-11-14");
    f.engine.set_private(&m, true).unwrap();
    assert!(
        f.engine
            .cache_audio("2026-11-14", "v", b"stale", stamp)
            .unwrap()
            .is_none()
    );
    let stamp = f.engine.audio_stamp("2026-11-14");
    assert!(
        f.engine
            .cache_audio("2026-11-14", "v", b"fresh", stamp)
            .unwrap()
            .is_some()
    );
    for change in 0..4 {
        let stamp = f.engine.audio_stamp("2026-11-14");
        match change {
            0 => {
                f.engine.correct(&m, CorrectPatch::object("zebra")).unwrap();
            }
            1 => {
                save_day(&f, "2026-11-14");
            }
            2 => {
                let item = f.engine.items(Default::default()).unwrap().remove(0).id;
                f.engine.apply(&item, vec![]).unwrap();
            }
            _ => {
                let preview = f.engine.preview_forget(std::slice::from_ref(&m), false);
                if let Ok(p) = preview {
                    let _ = f.engine.forget(&p.token);
                }
            }
        }
        assert!(
            f.engine
                .cache_audio("2026-11-14", "v", b"x", stamp)
                .unwrap()
                .is_none(),
            "change {change}"
        );
    }
}

// ---- 4. privacy follows the version chain -------------------------------------------------------

#[test]
fn private_after_a_correction_covers_the_old_page_line() {
    let f = fixture();
    let (_, m) = learn(
        &f.engine,
        "dx",
        cand(
            "Sam",
            Predicate::Knows,
            "the diagnosis",
            "Sam knows the diagnosis.",
        ),
    );
    f.clock.set_str("2026-11-14T06:00:00Z");
    let day = date("2026-11-14");
    save_day(&f, "2026-11-14");
    f.engine
        .cache("2026-11-14", "v", b"Sam knows the diagnosis")
        .unwrap();
    let m2 = f
        .engine
        .correct(&m, CorrectPatch::object("the full diagnosis"))
        .unwrap();
    f.engine.set_private(&m2, true).unwrap();
    let a = f
        .engine
        .page_for(Some(day), Edition::Audio)
        .unwrap()
        .unwrap();
    assert!(!all_text(&a).contains("diagnosis"));
    assert!(
        f.engine.cached_audio("2026-11-14", "v").unwrap().is_none(),
        "audio citing any version goes"
    );
    // The earlier version is private too, in both directions.
    let history = f.engine.history(&m).unwrap();
    assert_eq!(history.len(), 2);
    assert!(history.iter().all(|h| h.private));
    assert_eq!(history[0].id, m2, "newest first");
    // And the other way round.
    f.engine.set_private(&m, false).unwrap();
    assert!(f.engine.history(&m2).unwrap().iter().all(|h| !h.private));
}

// ---- 5. files are removed even across a crash ----------------------------------------------------

#[test]
fn files_with_no_row_are_swept_at_open_and_a_started_forget_remembers_its_audio() {
    let f = fixture();
    let src = tempfile::tempdir().unwrap();
    let kept = f
        .engine
        .keep_file(
            &write_file(src.path(), "a.pdf", b"kept bytes"),
            Source::Drop,
        )
        .unwrap();
    let (_, m) = learn(
        &f.engine,
        "zebra",
        cand("Zed", Predicate::Knows, "zebrafish", "Zed knows zebrafish."),
    );
    f.clock.set_str("2026-11-14T06:00:00Z");
    save_day(&f, "2026-11-14");
    let audio = f
        .engine
        .cache("2026-11-14", "v", b"Zed knows zebrafish")
        .unwrap();

    // Leftovers a crash could leave.
    let originals = f.dir.path().join("originals");
    std::fs::write(originals.join(".tmp-halfcopied"), b"half").unwrap();
    std::fs::write(originals.join("0123456789abcdef.jpg"), b"orphan original").unwrap();
    std::fs::write(
        f.dir.path().join("audio").join("orphan.audio"),
        b"orphan audio",
    )
    .unwrap();

    // A forget that started: its audio is recorded in the job before anything is removed.
    let preview = f.engine.preview_forget(&[m], false).unwrap();
    f.engine.forget_begin_only(&preview.token).unwrap();
    let recorded: String = {
        let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
        conn.query_row("SELECT files FROM forget_jobs", [], |r| r.get(0))
            .unwrap()
    };
    assert!(recorded.contains(audio.file_name().unwrap().to_str().unwrap()));
    assert!(!audio.exists(), "shredded after the commit");

    let reopened = Engine::open(f.dir.path(), f.clock.clone()).unwrap();
    assert_eq!(
        sql_count(f.dir.path(), "SELECT COUNT(*) FROM forget_jobs"),
        0
    );
    let names: Vec<_> = std::fs::read_dir(&originals)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let kept_id = match kept {
        nocturne_contract::Kept::New { item } => item.id,
        _ => unreachable!(),
    };
    assert_eq!(
        names.len(),
        1,
        "only the original with a row stays: {names:?}"
    );
    assert!(reopened.get_item(&kept_id).unwrap().has_original);
    assert_eq!(
        std::fs::read_dir(f.dir.path().join("audio"))
            .unwrap()
            .count(),
        0
    );
}

// ---- 7. a log that cannot be truncated is retried ----------------------------------------------

#[test]
fn a_busy_reader_makes_the_scrub_incomplete_and_it_is_retried() {
    let f = fixture();
    let (_, m) = learn(
        &f.engine,
        "zebra",
        cand("Zed", Predicate::Knows, "zebrafish", "Zed knows zebrafish."),
    );
    // A reader that holds an old snapshot blocks the truncate.
    let reader = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
    reader
        .execute_batch("BEGIN; SELECT COUNT(*) FROM memories;")
        .unwrap();
    let preview = f.engine.preview_forget(&[m], false).unwrap();
    let outcome = f.engine.forget(&preview.token).unwrap();
    assert_eq!(
        outcome.report.memories_removed, 1,
        "the forget itself completed"
    );
    assert!(!outcome.fully_scrubbed);
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM settings WHERE key = '_truncatePending'"
        ),
        1
    );
    reader.execute_batch("COMMIT;").unwrap();
    assert!(f.engine.retry_scrub().unwrap());
    assert_eq!(
        sql_count(
            f.dir.path(),
            "SELECT COUNT(*) FROM settings WHERE key = '_truncatePending'"
        ),
        0
    );
    let wal = f.dir.path().join("nocturne.db-wal");
    if wal.exists() {
        assert_eq!(std::fs::metadata(wal).unwrap().len(), 0);
    }
}

// ---- 8. leases ----------------------------------------------------------------------------------

#[test]
fn a_lease_token_is_required_and_dies_with_the_lease() {
    let f = fixture();
    let id = keep(&f.engine, "a note");
    let first = f
        .engine
        .lease_pending(1, Duration::from_secs(60))
        .unwrap()
        .remove(0);
    assert!(!first.lease.is_empty());
    assert!(matches!(
        f.engine.apply_candidates(&id, "wrong", vec![]),
        Err(Error::LeaseLost)
    ));
    assert!(matches!(
        f.engine.apply_candidates(&id, "", vec![]),
        Err(Error::LeaseLost)
    ));
    assert!(matches!(
        f.engine
            .fail_item(&id, "wrong", nocturne_contract::CouldntReadReason::Corrupt),
        Err(Error::LeaseLost)
    ));
    // The lease runs out and another worker takes the item.
    f.clock.set_str("2026-11-13T07:05:00Z");
    let second = f
        .engine
        .lease_pending(1, Duration::from_secs(60))
        .unwrap()
        .remove(0);
    assert_ne!(first.lease, second.lease);
    assert!(
        matches!(
            f.engine.apply_candidates(&id, &first.lease, vec![]),
            Err(Error::LeaseLost)
        ),
        "the old worker is refused"
    );
    f.engine
        .apply_candidates(&id, &second.lease, vec![])
        .unwrap();
    assert!(
        matches!(
            f.engine.apply_candidates(&id, &second.lease, vec![]),
            Err(Error::LeaseLost)
        ),
        "applying ends the lease"
    );
}

#[test]
fn an_item_in_a_forget_cannot_be_leased_or_applied() {
    let f = fixture();
    let (item, m) = learn(
        &f.engine,
        "n",
        cand("Zed", Predicate::Knows, "zebrafish", "Zed knows zebrafish."),
    );
    let lease = f
        .engine
        .lease_item(&item, Duration::from_secs(600))
        .unwrap()
        .lease;
    let preview = f.engine.preview_forget(&[m], true).unwrap();
    f.engine.forget_begin_only(&preview.token).unwrap();
    assert!(matches!(
        f.engine.apply_candidates(&item, &lease, vec![]),
        Err(Error::LeaseLost)
    ));
    assert!(matches!(
        f.engine.lease_item(&item, Duration::from_secs(60)),
        Err(Error::LeaseLost)
    ));
}

// ---- 9. conflicts when a memory becomes active ---------------------------------------------------

#[test]
fn corroboration_and_confirmation_check_for_conflicts() {
    let f = fixture();
    learn(
        &f.engine,
        "Gran lives in Leeds",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    let web = keep_as(&f.engine, "web says Hull", Source::PastedWeb);
    let r = f
        .engine
        .apply(
            &web,
            vec![cand(
                "Gran",
                Predicate::LivesIn,
                "Hull",
                "Gran lives in Hull.",
            )],
        )
        .unwrap();
    assert!(r.conflicts.is_empty(), "untrusted never conflicts");
    let hull = r.created[0].clone();
    // A trusted independent note agrees with the web claim: it becomes active, and now competes.
    let note = keep(&f.engine, "Gran told me she lives in Hull");
    let r = f
        .engine
        .apply(
            &note,
            vec![cand(
                "Gran",
                Predicate::LivesIn,
                "Hull",
                "Gran lives in Hull.",
            )],
        )
        .unwrap();
    assert_eq!(r.promoted, vec![hull.clone()]);
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(
        f.engine.get_item(&note).unwrap().status,
        nocturne_contract::ItemStatus::NeedsYou
    );
    assert!(
        f.engine
            .needs_you()
            .unwrap()
            .iter()
            .any(|e| matches!(e, NeedsYouEntry::Which { .. }))
    );

    // The same through a person's confirmation.
    let g = fixture();
    learn(
        &g.engine,
        "Gran lives in Leeds",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    let agent = keep_as(&g.engine, "agent: Hull", Source::Agent);
    let r = g
        .engine
        .apply(
            &agent,
            vec![cand(
                "Gran",
                Predicate::LivesIn,
                "Hull",
                "Gran lives in Hull.",
            )],
        )
        .unwrap();
    g.engine.confirm(&r.created[0], true).unwrap();
    assert!(
        g.engine
            .needs_you()
            .unwrap()
            .iter()
            .any(|e| matches!(e, NeedsYouEntry::Which { .. }))
    );
}

// ---- 10. correct with a patch -------------------------------------------------------------------

#[test]
fn a_correction_can_change_the_sentence_and_the_dates() {
    let f = fixture();
    let mut c = cand(
        "Trip form",
        Predicate::Due,
        "Friday",
        "The trip form is due Friday.",
    );
    c.due = Some(date("2026-11-14"));
    c.valid_from = Some(date("2026-11-01"));
    let (_, m) = learn(&f.engine, "form", c);
    let new = f
        .engine
        .correct(
            &m,
            CorrectPatch {
                object: "Monday".into(),
                sentence: Some("The trip form is now due on Monday 17th.".into()),
                due: Some(Some(date("2026-11-17"))),
                valid_from: Some(None),
                valid_to: None,
            },
        )
        .unwrap();
    let got = f.engine.get_memory(&new).unwrap();
    assert_eq!(got.sentence, "The trip form is now due on Monday 17th.");
    assert_eq!(got.due.as_deref(), Some("2026-11-17"));
    assert_eq!(got.valid_from, None);
    // Default: the object is replaced inside the sentence.
    let again = f
        .engine
        .correct(&new, CorrectPatch::object("Tuesday"))
        .unwrap();
    assert_eq!(
        f.engine.get_memory(&again).unwrap().due.as_deref(),
        Some("2026-11-17"),
        "untouched when not given"
    );
}

// ---- 11. vectors --------------------------------------------------------------------------------

#[test]
fn rewording_on_a_reread_drops_the_old_vector() {
    let f = fixture();
    let (item, m) = learn(
        &f.engine,
        "n",
        cand("Leo", Predicate::Knows, "Sam", "Leo knows Sam."),
    );
    f.engine
        .store_embeddings(vec![(m.clone(), vec![1.0, 0.0])])
        .unwrap();
    assert!(f.engine.memories_without_embedding(10).unwrap().is_empty());
    let r = f
        .engine
        .apply(
            &item,
            vec![cand(
                "Leo",
                Predicate::Knows,
                "Sam",
                "Leo has known Sam for years.",
            )],
        )
        .unwrap();
    assert_eq!(r.reworded, vec![m.clone()]);
    assert_eq!(
        f.engine.memories_without_embedding(10).unwrap(),
        vec![(m, "Leo has known Sam for years.".to_string())]
    );
    assert!(
        f.engine
            .retrieve("sam", Some(&[1.0, 0.0]), 5)
            .unwrap()
            .iter()
            .all(|h| h.score < 0.02),
        "found by words only"
    );
}

// ---- 12. a page saved early ends at now ------------------------------------------------------------

#[test]
fn a_page_saved_before_its_time_leaves_later_items_for_the_next_page() {
    let f = fixture_at("2026-11-13T20:00:00Z", "Europe/London");
    save_day(&f, "2026-11-14"); // page one, written the night before it is due
    f.clock.set_str("2026-11-14T05:00:00Z");
    let early = keep(&f.engine, "kept at five");
    f.engine.apply(&early, vec![]).unwrap();
    f.clock.set_str("2026-11-14T05:30:00Z");
    let p = f
        .engine
        .compose_page(date("2026-11-14"), Edition::Screen)
        .unwrap();
    f.engine.save_page(&p).unwrap();
    f.clock.set_str("2026-11-14T06:30:00Z");
    let later = keep(&f.engine, "kept at half six, before the page time");
    f.engine.apply(&later, vec![]).unwrap();
    f.clock.set_str("2026-11-15T07:00:00Z");
    let next = f
        .engine
        .compose_page(date("2026-11-15"), Edition::Screen)
        .unwrap();
    assert_eq!(
        next.kept_count, 1,
        "the item kept after the early save is not lost"
    );
}

// ---- 13. the public facade ----------------------------------------------------------------------

#[test]
fn needs_you_public_leaves_private_out_and_values_serialise() {
    let f = fixture();
    let (_, secret) = learn(
        &f.engine,
        "s",
        cand(
            "Call doctor",
            Predicate::Promised,
            "the doctor",
            "You promised to call the doctor.",
        ),
    );
    learn(
        &f.engine,
        "o",
        cand(
            "Call Gran",
            Predicate::Promised,
            "Gran",
            "You promised to call Gran.",
        ),
    );
    f.engine.set_private(&secret, true).unwrap();
    assert_eq!(f.engine.needs_you().unwrap().len(), 2);
    let public = f.engine.needs_you_public().unwrap();
    assert_eq!(public.len(), 1);
    assert!(serde_json::to_string(&public).unwrap().contains("Gran"));
    assert!(!serde_json::to_string(&public).unwrap().contains("doctor"));

    assert_eq!(f.engine.next_page_date().unwrap(), date("2026-11-13"));
    f.clock.set_str("2026-11-13T08:00:00Z");
    save_day(&f, "2026-11-13");
    assert_eq!(f.engine.next_page_date().unwrap(), date("2026-11-14"));

    serde_json::to_string(&f.engine.stats().unwrap()).unwrap();
    let hits = f.engine.retrieve("gran", None, 3).unwrap();
    let json = serde_json::to_string(&hits).unwrap();
    let back: Vec<nocturne_engine::Hit> = serde_json::from_str(&json).unwrap();
    assert_eq!(back, hits);
    serde_json::to_string(&f.engine.pending_items(5).unwrap()).unwrap();
    assert_eq!(f.engine.memories(MemoryFilter::default()).unwrap().len(), 2);
}

#[test]
fn which_lines_carry_the_memory_subject() {
    use common::{cand, fixture, keep, learn};
    use nocturne_engine::Predicate;
    let f = fixture();
    learn(
        &f.engine,
        "Dentist Tuesday 14:00",
        cand(
            "The dentist appointment",
            Predicate::Scheduled,
            "Tuesday 14:00",
            "Your dentist appointment is Tuesday at 14:00.",
        ),
    );
    let item_b = keep(&f.engine, "Dentist moved to Thursday 10:00");
    f.engine
        .apply(
            &item_b,
            vec![cand(
                "dentist appointment",
                Predicate::Scheduled,
                "Thursday 10:00",
                "Your dentist appointment is Thursday at 10:00.",
            )],
        )
        .unwrap();
    let day = f.engine.next_page_date().unwrap();
    let page = f
        .engine
        .compose_page(day, nocturne_contract::Edition::Screen)
        .unwrap();
    let which = page
        .needs_you
        .iter()
        .find(|l| l.kind == nocturne_contract::LineKind::Which)
        .expect("a which line");
    assert!(
        which
            .subject
            .as_deref()
            .is_some_and(|s| s.contains("dentist"))
    );
}
