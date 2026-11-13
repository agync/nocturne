mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use nocturne_contract::{CouldntReadReason, ItemStatus, NoticeCode, Source};
use nocturne_engine::{FakeMind, MindError, Predicate, Transcript, Understanding};
use nocturne_intelligence::{Event, Outcome, Step};

fn text_world(text: &str) -> World {
    world(FakeMind::new().with_extract(text, vec![gran()]))
}

// ---- the happy path ----

#[tokio::test]
async fn a_note_goes_kept_reading_understood_and_its_memories_are_embedded() {
    let w = text_world("Gran lives in Leeds");
    let id = keep(&w, "Gran lives in Leeds");
    let step = w.reader.tick().await;
    assert_eq!(
        step,
        Step::Read {
            item: id.clone(),
            outcome: Outcome::Understood
        }
    );
    assert_eq!(
        w.events.statuses(&id),
        [ItemStatus::Reading, ItemStatus::Understood]
    );
    assert_eq!(w.engine.memories(Default::default()).unwrap().len(), 1);
    assert!(w.engine.memories_without_embedding(10).unwrap().is_empty());
    assert_eq!(w.mind.calls(), ["extract", "embed"]);
    assert_eq!(w.reader.tick().await, Step::Idle);
}

#[tokio::test]
async fn a_voice_note_is_transcribed_then_read() {
    let w = world(FakeMind::new());
    let (id, name) = keep_voice(&w, b"audio bytes");
    // Script by the stored name now that it is known.
    let mind = FakeMind::new()
        .with_transcript(
            &name,
            Transcript {
                text: "Gran lives in Leeds".into(),
                language: Some("en".into()),
                duration_ms: 4_000,
                segments: vec![],
            },
        )
        .with_extract("Gran lives in Leeds", vec![gran()]);
    *w.slot.write().unwrap() = Some(Arc::new(mind));
    w.reader.tick().await;
    let item = w.engine.get_item(&id).unwrap();
    assert_eq!(item.status, ItemStatus::Understood);
    assert_eq!(item.text.as_deref(), Some("Gran lives in Leeds"));
}

#[tokio::test]
async fn images_and_documents_are_read_and_a_blank_scan_with_a_title_is_partly_understood() {
    let w = world(FakeMind::new());
    let (photo, photo_name) = keep_photo(&w, b"jpeg", Source::Screenshot);
    let (scan, scan_name) = keep_pdf(&w, b"%PDF scan");
    let mind = FakeMind::new()
        .with_understanding(
            &photo_name,
            Understanding {
                title: "Appointment card".into(),
                text: "Dentist on Tuesday".into(),
                ..Default::default()
            },
        )
        .with_extract("Dentist on Tuesday", vec![gran()])
        .with_understanding(
            &scan_name,
            Understanding {
                title: "Gas bill".into(),
                text: "  ".into(),
                ..Default::default()
            },
        );
    *w.slot.write().unwrap() = Some(Arc::new(mind));
    read_all(&w).await;
    assert_eq!(status(&w, &photo), ItemStatus::Understood);
    assert_eq!(w.engine.get_item(&photo).unwrap().title, "Appointment card");
    let scan = w.engine.get_item(&scan).unwrap();
    assert_eq!(scan.status, ItemStatus::PartlyUnderstood);
    assert_eq!(scan.title, "Gas bill");
}

#[tokio::test]
async fn an_unreadable_scan_without_a_title_is_unsupported() {
    let w = world(FakeMind::new());
    let (scan, name) = keep_pdf(&w, b"%PDF blank");
    let mind = FakeMind::new().with_understanding(&name, Understanding::default());
    *w.slot.write().unwrap() = Some(Arc::new(mind));
    w.reader.tick().await;
    let item = w.engine.get_item(&scan).unwrap();
    assert_eq!(item.status, ItemStatus::CouldntRead);
    assert_eq!(item.reason, Some(CouldntReadReason::Unsupported));
}

#[tokio::test]
async fn long_text_is_extracted_in_chunks_and_merged() {
    let para = |c: char| c.to_string().repeat(4_000);
    let text = format!("{}\n\n{}\n\n{}", para('a'), para('b'), para('c'));
    let mut mind = FakeMind::new();
    for c in ['a', 'b', 'c'] {
        mind = mind.with_extract(&para(c), vec![gran()]);
    }
    let w = world(mind);
    let id = keep(&w, &text);
    w.reader.tick().await;
    assert_eq!(status(&w, &id), ItemStatus::Understood);
    assert_eq!(
        w.mind.call_count("extract"),
        3,
        "6,000 characters at most each"
    );
    assert_eq!(
        w.engine.memories(Default::default()).unwrap().len(),
        1,
        "the same claim from three chunks is one memory"
    );
}

// ---- power policy ----

#[tokio::test]
async fn on_battery_new_items_wait_but_voice_notes_and_read_now_go() {
    let w = text_world("a note");
    w.host.on_battery(80);
    let note = keep(&w, "a note");
    assert_eq!(w.reader.tick().await, Step::Idle);
    assert_eq!(status(&w, &note), ItemStatus::Kept);

    let (voice, name) = keep_voice(&w, b"voice");
    let mind = FakeMind::new()
        .with_transcript(
            &name,
            Transcript {
                text: "hello".into(),
                ..Default::default()
            },
        )
        .with_extract("a note", vec![gran()]);
    *w.slot.write().unwrap() = Some(Arc::new(mind));
    let step = w.reader.tick().await;
    assert!(
        matches!(step, Step::Read { ref item, .. } if *item == voice),
        "{step:?}"
    );
    assert_eq!(w.reader.tick().await, Step::Idle, "the note still waits");

    w.reader.read_now(Some(&note));
    let step = w.reader.tick().await;
    assert!(matches!(step, Step::Read { ref item, outcome: Outcome::Understood } if *item == note));
}

#[tokio::test]
async fn read_now_for_everything_ignores_power_until_the_queue_is_empty() {
    let w = text_world("a note");
    w.host.on_battery(50);
    let a = keep(&w, "a note");
    let b = keep(&w, "another");
    w.reader.read_now(None);
    read_all(&w).await;
    assert_eq!(status(&w, &a), ItemStatus::Understood);
    assert_eq!(status(&w, &b), ItemStatus::Understood);
    // The request is spent: a new item waits again.
    let c = keep(&w, "third");
    assert_eq!(w.reader.tick().await, Step::Idle);
    assert_eq!(status(&w, &c), ItemStatus::Kept);
}

#[tokio::test]
async fn unknown_power_counts_as_mains() {
    let w = text_world("a note");
    w.host.set_ac(None);
    let id = keep(&w, "a note");
    w.reader.tick().await;
    assert_eq!(status(&w, &id), ItemStatus::Understood);
}

#[tokio::test]
async fn read_now_goes_before_new_items() {
    let w = text_world("x");
    let first = keep(&w, "first");
    let second = keep(&w, "second");
    w.reader.read_now(Some(&second));
    let step = w.reader.tick().await;
    assert!(
        matches!(step, Step::Read { ref item, .. } if *item == second),
        "{step:?}"
    );
    let _ = first;
}

// ---- failures ----

async fn one_failure(err: MindError) -> (World, String, Step) {
    let w = world(FakeMind::new());
    let id = keep(&w, "a note");
    w.mind.fail_next("extract", err);
    let step = w.reader.tick().await;
    (w, id, step)
}

#[tokio::test]
async fn too_long_unsupported_and_language_are_final() {
    for (err, reason) in [
        (MindError::TooLong, CouldntReadReason::TooLong),
        (
            MindError::Unsupported("x".into()),
            CouldntReadReason::Unsupported,
        ),
        (
            MindError::Language("fr".into()),
            CouldntReadReason::Language,
        ),
    ] {
        let (w, id, step) = one_failure(err).await;
        assert_eq!(
            step,
            Step::Read {
                item: id.clone(),
                outcome: Outcome::CouldntRead(reason)
            }
        );
        let item = w.engine.get_item(&id).unwrap();
        assert_eq!(item.status, ItemStatus::CouldntRead);
        assert_eq!(item.reason, Some(reason));
        assert_eq!(w.engine.memories(Default::default()).unwrap().len(), 0);
    }
}

#[tokio::test]
async fn three_failures_give_up_with_a_notice_after_a_wait_between_tries() {
    let w = world(FakeMind::new());
    let id = keep(&w, "a note");
    w.mind
        .fail_always("extract", MindError::Failed("boom".into()));
    assert!(matches!(
        w.reader.tick().await,
        Step::Read {
            outcome: Outcome::Retry,
            ..
        }
    ));
    // The item waits before another try, so a hiccup does not use all three at once.
    assert_eq!(w.reader.tick().await, Step::Idle);
    w.clock.set_str("2026-11-13T09:01:00Z");
    assert!(matches!(
        w.reader.tick().await,
        Step::Read {
            outcome: Outcome::Retry,
            ..
        }
    ));
    w.clock.set_str("2026-11-13T09:02:00Z");
    assert!(matches!(
        w.reader.tick().await,
        Step::Read {
            outcome: Outcome::GaveUp,
            ..
        }
    ));
    let item = w.engine.get_item(&id).unwrap();
    assert_eq!(item.status, ItemStatus::CouldntRead);
    assert_eq!(item.reason, Some(CouldntReadReason::Corrupt));
    let notices = w.events.notices();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].code, NoticeCode::CouldntRead);
    assert_eq!(notices[0].item_id.as_deref(), Some(id.as_str()));
}

#[tokio::test]
async fn an_unavailable_mind_gives_the_item_back_uncounted_and_backs_off() {
    let w = world(FakeMind::new());
    let id = keep(&w, "a note");
    w.mind.fail_always("extract", MindError::Unavailable);
    let waits = [30, 60, 120, 300, 300];
    let mut now = "2026-11-13T09:00:00Z".parse::<jiff::Timestamp>().unwrap();
    for wait in waits {
        w.clock.set(now);
        let step = w.reader.tick().await;
        assert!(
            matches!(
                step,
                Step::Read {
                    outcome: Outcome::Returned,
                    ..
                }
            ),
            "{step:?}"
        );
        assert_eq!(status(&w, &id), ItemStatus::Kept);
        // Still backing off one second before the wait is over.
        w.clock.set(now + jiff::SignedDuration::from_secs(wait - 1));
        assert!(matches!(w.reader.tick().await, Step::Backoff { .. }));
        now += jiff::SignedDuration::from_secs(wait);
    }
    // None of it counted against the item: its next lease is its first attempt.
    w.clock.set(now);
    w.mind.clear_failures("extract");
    w.reader.tick().await;
    assert_eq!(status(&w, &id), ItemStatus::Understood);
    assert!(w.reader.unavailable_since().is_none(), "success clears it");
}

#[tokio::test]
async fn busy_and_out_of_memory_back_off_too_and_out_of_memory_is_reported() {
    let (w, id, step) = one_failure(MindError::Busy).await;
    assert!(matches!(
        step,
        Step::Read {
            outcome: Outcome::Returned,
            ..
        }
    ));
    assert_eq!(status(&w, &id), ItemStatus::Kept);
    assert!(matches!(w.reader.tick().await, Step::Backoff { .. }));
    assert!(!w.events.all().contains(&Event::OutOfMemory));

    let (w, id, step) = one_failure(MindError::OutOfMemory).await;
    assert!(matches!(
        step,
        Step::Read {
            outcome: Outcome::Returned,
            ..
        }
    ));
    assert_eq!(status(&w, &id), ItemStatus::Kept);
    assert!(w.events.all().contains(&Event::OutOfMemory));
    // Three out-of-memory loads never use up the item.
    for i in 1..=3 {
        w.mind.fail_next("extract", MindError::OutOfMemory);
        w.clock.advance_days(i);
        w.reader.tick().await;
    }
    w.clock.advance_days(10);
    w.reader.tick().await;
    assert_eq!(status(&w, &id), ItemStatus::Understood);
}

#[tokio::test]
async fn no_mind_means_no_reading_and_the_item_waits() {
    let w = text_world("a note");
    *w.slot.write().unwrap() = None;
    let id = keep(&w, "a note");
    assert_eq!(w.reader.tick().await, Step::NoMind);
    assert!(w.reader.unavailable_since().is_some());
    assert_eq!(status(&w, &id), ItemStatus::Kept);
}

// ---- the mind goes away, an ask pre-empts, a lease is lost ----

#[tokio::test]
async fn a_mind_that_goes_away_mid_item_leaves_it_kept_with_nothing_applied() {
    let w = text_world("Gran lives in Leeds");
    let id = keep(&w, "Gran lives in Leeds");
    let gate = w.mind.gate("extract");
    let reader = w.reader.clone();
    let task = tokio::spawn(async move { reader.tick().await });
    gate.entered().await;
    assert_eq!(status(&w, &id), ItemStatus::Reading);
    // lemond restarts: the call fails when it is let go.
    w.mind.set_down(true);
    gate.open();
    let step = task.await.unwrap();
    assert!(
        matches!(
            step,
            Step::Read {
                outcome: Outcome::Returned,
                ..
            }
        ),
        "{step:?}"
    );
    assert_eq!(status(&w, &id), ItemStatus::Kept);
    assert_eq!(w.engine.memories(Default::default()).unwrap().len(), 0);
    assert_eq!(
        w.events.statuses(&id),
        [ItemStatus::Reading, ItemStatus::Kept]
    );
    // Later, the mind is back and the item is read.
    w.mind.set_down(false);
    w.clock.advance_days(1);
    w.reader.tick().await;
    assert_eq!(status(&w, &id), ItemStatus::Understood);
}

#[tokio::test]
async fn an_ask_pre_empts_the_item_in_flight_and_holds_the_mind() {
    let w = text_world("Gran lives in Leeds");
    let id = keep(&w, "Gran lives in Leeds");
    let gate = w.mind.gate("extract");
    let reader = w.reader.clone();
    let task = tokio::spawn(async move { reader.tick().await });
    gate.entered().await;
    let ask = w.reader.ask_begin();
    let step = task.await.unwrap();
    assert!(
        matches!(
            step,
            Step::Read {
                outcome: Outcome::Returned,
                ..
            }
        ),
        "{step:?}"
    );
    assert_eq!(status(&w, &id), ItemStatus::Kept);
    assert_eq!(w.reader.tick().await, Step::Asking);
    drop(ask);
    gate.open();
    w.reader.tick().await;
    assert_eq!(status(&w, &id), ItemStatus::Understood);
    // The pre-empted read never counted: this was the item's first attempt.
    assert_eq!(w.engine.memories(Default::default()).unwrap().len(), 1);
}

#[tokio::test]
async fn an_item_undone_mid_read_is_dropped_silently() {
    let w = text_world("Gran lives in Leeds");
    let id = keep(&w, "Gran lives in Leeds");
    let gate = w.mind.gate("extract");
    let reader = w.reader.clone();
    let task = tokio::spawn(async move { reader.tick().await });
    gate.entered().await;
    w.engine.unkeep(&id).unwrap();
    gate.open();
    let step = task.await.unwrap();
    assert!(
        matches!(
            step,
            Step::Read {
                outcome: Outcome::Lost,
                ..
            }
        ),
        "{step:?}"
    );
    assert_eq!(w.engine.memories(Default::default()).unwrap().len(), 0);
    // Only the "reading" announcement was ever sent.
    assert_eq!(w.events.statuses(&id), [ItemStatus::Reading]);
}

#[tokio::test(start_paused = true)]
async fn the_lease_is_renewed_every_five_minutes_while_the_mind_works() {
    let w = text_world("Gran lives in Leeds");
    let id = keep(&w, "Gran lives in Leeds");
    w.mind.set_latency(Duration::from_secs(12 * 60));
    let step = w.reader.tick().await;
    assert_eq!(
        step,
        Step::Read {
            item: id,
            outcome: Outcome::Understood
        }
    );
    assert_eq!(w.reader.stats().renewals, 2, "at 5 and at 10 minutes");
}

#[tokio::test]
async fn pausing_gives_back_the_item_and_stops_reading() {
    let w = text_world("Gran lives in Leeds");
    let id = keep(&w, "Gran lives in Leeds");
    let gate = w.mind.gate("extract");
    let reader = w.reader.clone();
    let task = tokio::spawn(async move { reader.tick().await });
    gate.entered().await;
    w.reader.set_paused(true);
    let step = task.await.unwrap();
    assert!(matches!(
        step,
        Step::Read {
            outcome: Outcome::Returned,
            ..
        }
    ));
    assert_eq!(status(&w, &id), ItemStatus::Kept);
    assert_eq!(w.reader.tick().await, Step::Paused);
    w.reader.set_paused(false);
    gate.open();
    w.reader.tick().await;
    assert_eq!(status(&w, &id), ItemStatus::Understood);
}

#[tokio::test]
async fn shutdown_returns_the_item_in_flight() {
    let w = text_world("Gran lives in Leeds");
    let id = keep(&w, "Gran lives in Leeds");
    let gate = w.mind.gate("extract");
    let stop = tokio_util::sync::CancellationToken::new();
    let reader = w.reader.clone();
    let token = stop.clone();
    let run = tokio::spawn(async move { reader.run(token).await });
    gate.entered().await;
    stop.cancel();
    run.await.unwrap();
    assert_eq!(status(&w, &id), ItemStatus::Kept);
    assert_eq!(w.engine.get_item(&id).unwrap().status, ItemStatus::Kept);
}

// ---- re-reads ----

#[tokio::test]
async fn re_reads_wait_for_everything_else_and_for_mains() {
    let w = text_world("Gran lives in Leeds");
    let old = keep(&w, "Gran lives in Leeds");
    w.reader.tick().await;
    assert_eq!(status(&w, &old), ItemStatus::Understood);
    w.engine.queue_reread(std::slice::from_ref(&old)).unwrap();

    w.host.on_battery(90);
    assert_eq!(w.reader.tick().await, Step::Idle, "not on battery");
    w.host.set_ac(Some(true));
    let fresh = keep(&w, "another note");
    // The new item first, then the re-read.
    let first = w.reader.tick().await;
    assert!(
        matches!(first, Step::Read { ref item, .. } if *item == fresh),
        "{first:?}"
    );
    let second = w.reader.tick().await;
    assert!(
        matches!(second, Step::Read { ref item, outcome: Outcome::Understood } if *item == old),
        "{second:?}"
    );
    assert!(w.engine.rereads(10).unwrap().is_empty());
    assert!(
        w.events
            .notices()
            .iter()
            .any(|n| n.code == NoticeCode::ReReading)
    );
    assert_eq!(w.reader.tick().await, Step::Idle);
    assert!(
        w.events
            .all()
            .contains(&Event::NoticeCleared(NoticeCode::ReReading))
    );
    let _ = Predicate::Knows;
}

#[tokio::test]
async fn a_re_read_that_fails_never_takes_away_what_was_understood() {
    let w = text_world("Gran lives in Leeds");
    let id = keep(&w, "Gran lives in Leeds");
    w.reader.tick().await;
    w.engine.queue_reread(std::slice::from_ref(&id)).unwrap();
    for err in [
        MindError::Unsupported("x".into()),
        MindError::TooLong,
        MindError::Failed("boom".into()),
    ] {
        w.mind.fail_next("extract", err);
        w.clock.advance_days(1);
        let step = w.reader.tick().await;
        assert!(
            matches!(
                step,
                Step::Read {
                    outcome: Outcome::Returned,
                    ..
                }
            ),
            "{step:?}"
        );
        assert_eq!(status(&w, &id), ItemStatus::Understood);
        assert_eq!(w.engine.rereads(10).unwrap().len(), 1, "still marked");
    }
    assert_eq!(w.engine.memories(Default::default()).unwrap().len(), 1);
}
