//! The engine additions the intelligence layer relies on: leases given back and renewed,
//! failures counted apart, pages composed and saved to an explicit end, re-reads, the status
//! counts and the scripted fake mind.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use nocturne_contract::{CouldntReadReason, Edition, ItemStatus, PageVariant, Source, TidyReport};
use nocturne_engine::{
    Engine, FailureOutcome, FakeMind, ManualClock, Mind, MindError, Predicate, StatusCounts,
};

const MIN: Duration = Duration::from_secs(60);

fn status(e: &Engine, id: &str) -> ItemStatus {
    e.get_item(id).unwrap().status
}

// ---- leases ----

#[test]
fn a_released_lease_returns_the_item_and_voids_the_token() {
    let f = fixture();
    let id = keep(&f.engine, "a note");
    let p = f.engine.lease_pending(1, MIN).unwrap().remove(0);
    assert_eq!(status(&f.engine, &id), ItemStatus::Reading);
    f.engine.release_lease(&id, &p.lease, true).unwrap();
    assert_eq!(status(&f.engine, &id), ItemStatus::Kept);
    assert!(matches!(
        f.engine.release_lease(&id, &p.lease, true),
        Err(nocturne_engine::Error::LeaseLost)
    ));
    assert!(matches!(
        f.engine.apply_candidates(&id, &p.lease, vec![]),
        Err(nocturne_engine::Error::LeaseLost)
    ));
}

#[test]
fn an_uncounted_release_gives_the_attempt_back() {
    let f = fixture();
    let id = keep(&f.engine, "a note");
    let p = f.engine.lease_pending(1, MIN).unwrap().remove(0);
    assert_eq!(p.attempts, 1);
    f.engine.release_lease(&id, &p.lease, false).unwrap();
    let p = f.engine.lease_pending(1, MIN).unwrap().remove(0);
    assert_eq!(p.attempts, 1, "the first attempt never counted");
    f.engine.release_lease(&id, &p.lease, true).unwrap();
    let p = f.engine.lease_pending(1, MIN).unwrap().remove(0);
    assert_eq!(p.attempts, 2);
}

#[test]
fn a_renewed_lease_outlives_its_first_term() {
    let f = fixture();
    let id = keep(&f.engine, "a note");
    let p = f.engine.lease_pending(1, MIN).unwrap().remove(0);
    f.clock.set_str("2026-11-13T07:00:50Z");
    f.engine.renew_lease(&id, &p.lease, MIN).unwrap();
    f.clock.set_str("2026-11-13T07:01:20Z");
    assert!(f.engine.lease_pending(1, MIN).unwrap().is_empty());
    assert_eq!(status(&f.engine, &id), ItemStatus::Reading);
    f.clock.set_str("2026-11-13T07:02:00Z");
    assert_eq!(f.engine.lease_pending(1, MIN).unwrap().len(), 1);
    assert!(matches!(
        f.engine.renew_lease(&id, &p.lease, MIN),
        Err(nocturne_engine::Error::LeaseLost)
    ));
}

#[test]
fn open_returns_every_item_being_read_even_with_a_live_lease() {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::at("2026-11-13T07:00:00Z", "Europe/London"));
    let id = {
        let engine = Engine::open(dir.path(), clock.clone()).unwrap();
        let id = keep(&engine, "a note");
        engine.lease_pending(1, Duration::from_secs(3600)).unwrap();
        id
    };
    let engine = Engine::open(dir.path(), clock).unwrap();
    assert_eq!(status(&engine, &id), ItemStatus::Kept);
}

#[test]
fn three_mind_failures_give_up_but_leases_alone_never_do() {
    let f = fixture();
    let id = keep(&f.engine, "a note");
    for _ in 0..5 {
        let p = f.engine.lease_pending(1, MIN).unwrap().remove(0);
        f.engine.release_lease(&id, &p.lease, true).unwrap();
    }
    assert_eq!(status(&f.engine, &id), ItemStatus::Kept);
    for round in 1..=3 {
        let p = f.engine.lease_pending(1, MIN).unwrap().remove(0);
        let out = f.engine.record_failure(&id, &p.lease).unwrap();
        if round < 3 {
            assert_eq!(out, FailureOutcome::Retry);
            assert_eq!(status(&f.engine, &id), ItemStatus::Kept);
        } else {
            assert_eq!(out, FailureOutcome::GaveUp);
        }
    }
    let item = f.engine.get_item(&id).unwrap();
    assert_eq!(item.status, ItemStatus::CouldntRead);
    assert_eq!(item.reason, Some(CouldntReadReason::Corrupt));
    // Trying again by hand starts the count over.
    f.engine.retry_item(&id).unwrap();
    let p = f.engine.lease_pending(1, MIN).unwrap().remove(0);
    assert_eq!(
        f.engine.record_failure(&id, &p.lease).unwrap(),
        FailureOutcome::Retry
    );
}

#[test]
fn a_scanned_document_with_a_title_is_partly_understood() {
    let f = fixture();
    let src = tempfile::tempdir().unwrap();
    let kept = f
        .engine
        .keep_file(
            &write_file(src.path(), "scan.pdf", b"%PDF scan"),
            Source::Drop,
        )
        .unwrap();
    let nocturne_contract::Kept::New { item } = kept else {
        unreachable!()
    };
    let p = f.engine.lease_pending(1, MIN).unwrap().remove(0);
    f.engine
        .mark_partly(&item.id, &p.lease, " Gas bill ")
        .unwrap();
    let got = f.engine.get_item(&item.id).unwrap();
    assert_eq!(got.status, ItemStatus::PartlyUnderstood);
    assert_eq!(got.title, "Gas bill");
}

// ---- re-reads ----

#[test]
fn a_reread_mark_survives_a_restart_and_a_returned_lease_restores_the_status() {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::at("2026-11-13T07:00:00Z", "Europe/London"));
    let (waiting, read) = {
        let engine = Engine::open(dir.path(), clock.clone()).unwrap();
        let waiting = keep(&engine, "not read yet");
        let (read, _) = learn(
            &engine,
            "Gran lives in Leeds",
            cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
        );
        // Only items that were read can be read again.
        assert_eq!(
            engine
                .queue_reread(&[waiting.clone(), read.clone()])
                .unwrap(),
            1
        );
        (waiting, read)
    };
    let engine = Engine::open(dir.path(), clock).unwrap();
    assert_eq!(engine.rereads(10).unwrap(), std::slice::from_ref(&read));

    let p = engine.lease_item(&read, MIN).unwrap();
    assert!(engine.rereads(10).unwrap().is_empty(), "being read now");
    engine.release_lease(&read, &p.lease, false).unwrap();
    assert_eq!(status(&engine, &read), ItemStatus::Understood);
    assert_eq!(engine.rereads(10).unwrap(), std::slice::from_ref(&read));

    // Applying again clears the mark.
    let p = engine.lease_item(&read, MIN).unwrap();
    engine.apply_candidates(&read, &p.lease, vec![]).unwrap();
    assert!(engine.rereads(10).unwrap().is_empty());
    assert_eq!(status(&engine, &waiting), ItemStatus::Kept);
}

#[test]
fn an_interrupted_reread_is_understood_again_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::at("2026-11-13T07:00:00Z", "Europe/London"));
    let read = {
        let engine = Engine::open(dir.path(), clock.clone()).unwrap();
        let (read, _) = learn(
            &engine,
            "Gran lives in Leeds",
            cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
        );
        engine.queue_reread(std::slice::from_ref(&read)).unwrap();
        engine.lease_item(&read, MIN).unwrap();
        read
    };
    let engine = Engine::open(dir.path(), clock).unwrap();
    assert_eq!(status(&engine, &read), ItemStatus::Understood);
}

// ---- pages ----

#[test]
fn a_page_composed_to_an_end_covers_only_what_was_kept_before_it() {
    let f = fixture_at("2026-11-13T02:00:00Z", "Europe/London");
    f.clock.set_str("2026-11-13T03:00:00Z");
    keep(&f.engine, "kept at three");
    f.clock.set_str("2026-11-13T05:00:00Z");
    keep(&f.engine, "kept at five");
    f.clock.set_str("2026-11-13T05:30:00Z");

    let day = date("2026-11-13");
    let end = "2026-11-13T04:00:00Z".parse().unwrap();
    let page = f
        .engine
        .compose_page_until(day, end, Edition::Screen)
        .unwrap();
    assert_eq!(page.kept_count, 1);
    assert_eq!(page.pending, 1, "the item before the end is still unread");
    assert_ne!(page.variant, PageVariant::BeingWritten);
    // The plain composition counts what is kept so far and says it is being written.
    let live = f.engine.compose_page(day, Edition::Screen).unwrap();
    assert_eq!(live.variant, PageVariant::BeingWritten);

    f.engine.save_page_until(&page, end).unwrap();
    // The window ended at the explicit end: the next page starts there.
    let next = f
        .engine
        .compose_page(date("2026-11-14"), Edition::Screen)
        .unwrap();
    assert_eq!(next.kept_count, 1, "only the item kept at five");
}

#[test]
fn a_page_can_be_saved_again_once_and_only_while_unopened() {
    let f = fixture_at("2026-11-13T03:00:00Z", "Europe/London");
    keep(&f.engine, "kept at three");
    let day = date("2026-11-13");
    let end = "2026-11-13T03:30:00Z".parse().unwrap();
    assert!(!f.engine.can_resave_page(day).unwrap(), "nothing saved yet");
    let page = f
        .engine
        .compose_page_until(day, end, Edition::Screen)
        .unwrap();
    f.engine.save_page_until(&page, end).unwrap();
    assert!(f.engine.can_resave_page(day).unwrap());
    assert!(f.engine.resave_page(&page, end).unwrap());
    assert!(!f.engine.can_resave_page(day).unwrap(), "once only");
    assert!(!f.engine.resave_page(&page, end).unwrap());

    let other = fixture_at("2026-11-13T03:00:00Z", "Europe/London");
    keep(&other.engine, "kept at three");
    let page = other
        .engine
        .compose_page_until(day, end, Edition::Screen)
        .unwrap();
    other.engine.save_page_until(&page, end).unwrap();
    assert!(other.engine.page_opened(day).unwrap(), "the first open");
    assert!(
        !other.engine.page_opened(day).unwrap(),
        "later opens do not count"
    );
    assert!(!other.engine.resave_page(&page, end).unwrap());
}

#[test]
fn the_last_tidy_is_remembered() {
    let f = fixture();
    assert_eq!(f.engine.last_tidy().unwrap(), None);
    let report = TidyReport {
        from: "2026-11-13T01:00:00Z".into(),
        to: "2026-11-13T01:20:00Z".into(),
        items: 4,
        merged: 1,
        let_go: 2,
    };
    f.engine.record_tidy(report.clone()).unwrap();
    assert_eq!(f.engine.last_tidy().unwrap(), Some(report));
    // It is not one of the person's settings.
    assert_eq!(f.engine.settings().unwrap().page_time, "07:00");
}

// ---- counts ----

#[test]
fn status_counts_agree_with_needs_you() {
    let f = fixture();
    let (_, due) = learn(&f.engine, "Pay the gas bill", {
        let mut c = cand("Gas bill", Predicate::Due, "pay it", "Pay the gas bill.");
        c.due = Some(date("2026-11-14"));
        c
    });
    learn(
        &f.engine,
        "Promised Leo a bike",
        cand(
            "Leo",
            Predicate::Promised,
            "a bike",
            "You promised Leo a bike.",
        ),
    );
    let (_, snoozed) = learn(
        &f.engine,
        "Promised Mia a book",
        cand(
            "Mia",
            Predicate::Promised,
            "a book",
            "You promised Mia a book.",
        ),
    );
    f.engine.snooze(&snoozed, 3).unwrap();
    // An untrusted source leaves its memory unconfirmed.
    let web = keep_as(&f.engine, "a web page", Source::PastedWeb);
    f.engine
        .apply(
            &web,
            vec![cand(
                "Cafe",
                Predicate::Knows,
                "open late",
                "The cafe is open late.",
            )],
        )
        .unwrap();
    // A conflict.
    learn(
        &f.engine,
        "Gran lives in Leeds",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    let (_, york) = learn(
        &f.engine,
        "Gran lives in York now",
        cand("Gran", Predicate::LivesIn, "York", "Gran lives in York."),
    );
    // One waiting, one that could not be read.
    keep(&f.engine, "waiting");
    let bad = keep(&f.engine, "unreadable");
    let p = f.engine.lease_item(&bad, MIN).unwrap();
    f.engine
        .fail_item(&bad, &p.lease, CouldntReadReason::Unsupported)
        .unwrap();

    let counts = f.engine.status_counts().unwrap();
    assert_eq!(
        counts.needs_you as usize,
        f.engine.needs_you().unwrap().len()
    );
    assert!(counts.needs_you >= 4, "{counts:?}");
    assert_eq!(counts.unconfirmed, 1);
    assert_eq!(counts.couldnt_read, 1);
    assert_eq!(counts.waiting_to_read, 1);

    // Ticking one off, and a forget in progress, are counted the same way.
    f.engine.mark_done(&due).unwrap();
    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&york), true)
        .unwrap();
    f.engine.forget_begin_only(&preview.token).unwrap();
    let counts = f.engine.status_counts().unwrap();
    assert_eq!(
        counts.needs_you as usize,
        f.engine.needs_you().unwrap().len()
    );
}

#[test]
fn status_counts_do_not_load_rows_or_stop_at_a_cap() {
    let f = fixture();
    for i in 0..1500 {
        f.engine
            .keep_text(&format!("note number {i}"), Source::Paste)
            .unwrap();
    }
    assert_eq!(
        f.engine.status_counts().unwrap(),
        StatusCounts {
            needs_you: 0,
            waiting_to_read: 1500,
            couldnt_read: 0,
            unconfirmed: 0
        }
    );
}

// ---- the scripted fake mind ----

#[tokio::test]
async fn scripted_errors_apply_per_call_in_order() {
    let mind = FakeMind::new();
    mind.fail_next("embed", MindError::Busy);
    mind.fail_next("embed", MindError::OutOfMemory);
    let texts = vec!["a".to_string()];
    assert_eq!(mind.embed(&texts).await.unwrap_err(), MindError::Busy);
    assert_eq!(
        mind.embed(&texts).await.unwrap_err(),
        MindError::OutOfMemory
    );
    assert!(mind.embed(&texts).await.is_ok());
    mind.fail_always("embed", MindError::TooLong);
    assert_eq!(mind.embed(&texts).await.unwrap_err(), MindError::TooLong);
    assert_eq!(mind.embed(&texts).await.unwrap_err(), MindError::TooLong);
    mind.clear_failures("embed");
    assert!(mind.embed(&texts).await.is_ok());
    assert_eq!(mind.call_count("embed"), 6);
}

#[tokio::test]
async fn the_runtime_down_switch_flips_while_running() {
    let mind = FakeMind::new();
    let texts = vec!["a".to_string()];
    assert!(mind.embed(&texts).await.is_ok());
    mind.set_down(true);
    assert_eq!(
        mind.embed(&texts).await.unwrap_err(),
        MindError::Unavailable
    );
    mind.set_down(false);
    assert!(mind.embed(&texts).await.is_ok());
}

#[tokio::test(start_paused = true)]
async fn latency_is_tokio_time() {
    let mind = Arc::new(FakeMind::new());
    mind.set_latency(Duration::from_secs(30));
    let started = tokio::time::Instant::now();
    let texts = vec!["a".to_string()];
    mind.embed(&texts).await.unwrap();
    assert!(started.elapsed() >= Duration::from_secs(30));
}

#[tokio::test]
async fn a_gate_holds_a_call_mid_flight_until_opened() {
    let mind = Arc::new(FakeMind::new());
    let gate = mind.gate("extract");
    let m = mind.clone();
    let task = tokio::spawn(async move {
        let cx = nocturne_engine::ExtractContext {
            item_kind: nocturne_contract::ItemKind::Text,
            kept_at: "2026-11-13T07:00:00Z".parse().unwrap(),
            today: date("2026-11-13"),
            known_entities: vec![],
        };
        m.extract("text", &cx).await
    });
    gate.entered().await;
    assert!(!task.is_finished());
    gate.open();
    assert!(task.await.unwrap().is_ok());
}

// ---- forgetting and re-reads ----

fn zebra() -> nocturne_engine::Candidate {
    cand(
        "Me",
        Predicate::Knows,
        "Zebrafish47",
        "You know about Zebrafish47.",
    )
}

#[test]
fn a_forget_during_an_in_flight_reread_stops_the_result_from_bringing_the_memory_back() {
    let f = fixture();
    let (item, mem) = learn(&f.engine, "I know about Zebrafish47", zebra());
    f.engine.queue_reread(std::slice::from_ref(&item)).unwrap();
    // The re-read is under way: its lease was taken before the forget.
    let p = f.engine.lease_item(&item, MIN).unwrap();
    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&mem), false)
        .unwrap();
    f.engine.forget(&preview.token).unwrap();
    // The mind's answer arrives after the forget finished and its job row is gone.
    let err = f
        .engine
        .apply_candidates(&item, &p.lease, vec![zebra()])
        .unwrap_err();
    assert!(matches!(err, nocturne_engine::Error::LeaseLost), "{err:?}");
    assert!(f.engine.memories(Default::default()).unwrap().is_empty());
    // The item keeps its text and the person's choice: it is not marked for another re-read.
    assert!(f.engine.rereads(10).unwrap().is_empty());
}

#[test]
fn an_item_something_was_forgotten_from_is_never_re_read_automatically() {
    let f = fixture();
    let (item, mem) = learn(&f.engine, "I know about Zebrafish47", zebra());
    let other = learn(
        &f.engine,
        "Gran lives in Leeds",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    )
    .0;
    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&mem), false)
        .unwrap();
    f.engine.forget(&preview.token).unwrap();
    // Not queued, and an item that is not tombstoned still is.
    assert_eq!(
        f.engine
            .queue_reread(&[item.clone(), other.clone()])
            .unwrap(),
        1
    );
    assert_eq!(f.engine.rereads(10).unwrap(), [other]);
    // Even if a reader raced between listing and leasing, the lease is refused.
    assert!(matches!(
        f.engine.lease_item(&item, MIN),
        Err(nocturne_engine::Error::LeaseLost)
    ));
}

#[test]
fn a_re_read_the_person_asked_for_still_cannot_say_the_forgotten_thing_again() {
    let f = fixture();
    let (item, mem) = learn(&f.engine, "I know about Zebrafish47", zebra());
    let other = cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds.");
    f.engine.apply(&item, vec![zebra(), other.clone()]).unwrap();
    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&mem), false)
        .unwrap();
    f.engine.forget(&preview.token).unwrap();
    let p = f.engine.lease_item_by_request(&item, MIN).unwrap();
    f.engine
        .apply_candidates(&item, &p.lease, vec![zebra(), other])
        .unwrap();
    let sentences: Vec<String> = f
        .engine
        .memories(Default::default())
        .unwrap()
        .into_iter()
        .map(|m| m.sentence)
        .collect();
    assert_eq!(sentences, ["Gran lives in Leeds."]);
    // The store keeps the item's own text, but no trace of the forgotten claim's wording.
    let db = std::fs::read(f.dir.path().join("nocturne.db")).unwrap();
    assert!(!String::from_utf8_lossy(&db).contains("You know about Zebrafish47"));
}

#[test]
fn a_forgotten_claim_is_recognised_in_other_words() {
    let f = fixture();
    let (item, mem) = learn(
        &f.engine,
        "My cat is called Biscuit",
        cand(
            "Me",
            Predicate::Other("Owns".into()),
            "a cat called Biscuit",
            "You have a cat called Biscuit.",
        ),
    );
    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&mem), false)
        .unwrap();
    f.engine.forget(&preview.token).unwrap();
    let p = f.engine.lease_item_by_request(&item, MIN).unwrap();
    // "I" for "Me", a different case in the relation, trailing punctuation and a leading "the".
    let again = cand(
        "I",
        Predicate::Other("owns".into()),
        "The cat called Biscuit.",
        "You own a cat called Biscuit.",
    );
    f.engine
        .apply_candidates(&item, &p.lease, vec![again])
        .unwrap();
    assert!(f.engine.memories(Default::default()).unwrap().is_empty());
}

#[test]
fn giving_a_lease_back_works_while_a_forget_is_under_way() {
    let f = fixture();
    let (item, mem) = learn(&f.engine, "I know about Zebrafish47", zebra());
    f.engine.queue_reread(std::slice::from_ref(&item)).unwrap();
    let p = f.engine.lease_item(&item, MIN).unwrap();
    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&mem), false)
        .unwrap();
    // The forget has started and its job row is still there.
    f.engine.forget_begin_only(&preview.token).unwrap();
    assert!(matches!(
        f.engine.apply_candidates(&item, &p.lease, vec![zebra()]),
        Err(nocturne_engine::Error::LeaseLost)
    ));
    f.engine.release_lease(&item, &p.lease, false).unwrap();
    assert_eq!(status(&f.engine, &item), ItemStatus::Understood);
}

#[test]
fn a_reread_that_is_too_long_gives_the_item_back_as_it_was() {
    let f = fixture();
    let (item, _) = learn(&f.engine, "I know about Zebrafish47", zebra());
    f.engine.queue_reread(std::slice::from_ref(&item)).unwrap();
    let p = f.engine.lease_item(&item, MIN).unwrap();
    let out = f
        .engine
        .record_understanding(
            &item,
            &p.lease,
            nocturne_engine::Understanding {
                text: "long".into(),
                pages: Some(150),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(out, nocturne_engine::RecordOutcome::TooLong);
    f.engine.release_lease(&item, &p.lease, false).unwrap();
    assert_eq!(status(&f.engine, &item), ItemStatus::Understood);

    // A first reading of the same size is still final.
    let fresh = keep(&f.engine, "a long one");
    let p = f.engine.lease_item(&fresh, MIN).unwrap();
    f.engine
        .record_understanding(
            &fresh,
            &p.lease,
            nocturne_engine::Understanding {
                text: "long".into(),
                pages: Some(150),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(status(&f.engine, &fresh), ItemStatus::CouldntRead);
}

#[test]
fn a_reread_mark_can_be_cleared() {
    let f = fixture();
    let (item, _) = learn(&f.engine, "I know about Zebrafish47", zebra());
    f.engine.queue_reread(std::slice::from_ref(&item)).unwrap();
    f.engine.clear_reread(std::slice::from_ref(&item)).unwrap();
    assert!(f.engine.rereads(10).unwrap().is_empty());
}
