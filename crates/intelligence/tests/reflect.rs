mod common;

use common::*;
use nocturne_contract::{PeriodKind, Source};
use nocturne_engine::{FakeMind, MindError, WrittenSentence};
use nocturne_intelligence::Wrote;
use nocturne_intelligence::reflect::{MAX_SENTENCES, write, write_stoppable};

const DAY: &str = "2026-11-12";

fn sentence(text: &str, cites: &[usize]) -> WrittenSentence {
    WrittenSentence {
        text: text.into(),
        cites: cites.to_vec(),
    }
}

/// Keep a note and read it, with nothing found in it: a reflection is written from what was read.
fn keep(w: &World, text: &str) -> String {
    let id = common::keep(w, text);
    read(w, &id);
    id
}

fn read(w: &World, id: &str) {
    let p = w
        .engine
        .lease_item(id, std::time::Duration::from_secs(60))
        .unwrap();
    w.engine.apply_candidates(id, &p.lease, vec![]).unwrap();
}

fn day(w: &World) -> nocturne_contract::Period {
    w.engine.period_of(PeriodKind::Day, date(DAY)).unwrap()
}

/// A world on the morning after `DAY`, with two notes kept on it.
fn world_after_a_day(mind: FakeMind) -> (World, Vec<String>) {
    let w = world_at("2026-11-12T10:00:00Z", mind);
    let first = keep(&w, "Dropped Leo at school, then a long walk by the canal");
    w.clock.set_str("2026-11-12T18:30:00Z");
    let second = keep(&w, "Made soup and watched the match");
    w.clock.set_str("2026-11-13T09:00:00Z");
    (w, vec![first, second])
}

#[tokio::test]
async fn a_day_is_written_by_the_mind_and_stored_with_a_receipt_for_each_line() {
    let mind = FakeMind::new().with_reflection(vec![
        sentence(
            "You took Leo to school and then **walked by the canal**.",
            &[0],
        ),
        sentence("In the evening you **made soup**.", &[1]),
    ]);
    let (w, ids) = world_after_a_day(mind);
    let period = day(&w);

    let wrote = write(
        &w.engine,
        w.mind.as_ref(),
        &period,
        nocturne_contract::WrittenWhen::Overnight,
    )
    .await
    .unwrap();
    let Wrote::Day(r) = wrote else {
        panic!("{wrote:?}")
    };
    assert_eq!(r.the_day.len(), 2);
    assert_eq!(r.the_day[0].receipts[0].item_id, ids[0]);
    assert_eq!(r.the_day[1].receipts[0].item_id, ids[1]);
    assert_eq!(r.written, nocturne_contract::WrittenWhen::Overnight);
    assert!(!r.so_far, "the day had ended");
    assert_eq!(r.from_count, 2);
    assert!(w.engine.saved_reflection(&period).unwrap().is_some());
    assert_eq!(w.mind.call_count("reflect"), 1);
}

#[tokio::test]
async fn at_most_six_sentences_are_kept() {
    let many: Vec<WrittenSentence> = (0..10)
        .map(|n| sentence(&format!("Line {n} about **the walk**."), &[0]))
        .collect();
    let (w, _) = world_after_a_day(FakeMind::new().with_reflection(many));
    let period = day(&w);
    let Wrote::Day(r) = write(
        &w.engine,
        w.mind.as_ref(),
        &period,
        nocturne_contract::WrittenWhen::OnDemand,
    )
    .await
    .unwrap() else {
        panic!("a day")
    };
    assert_eq!(r.the_day.len(), MAX_SENTENCES);
    assert_eq!(r.the_day[0].text, "Line 0 about **the walk**.");
}

#[tokio::test]
async fn sentences_that_cite_nothing_or_something_not_there_are_not_kept() {
    let (w, _) = world_after_a_day(FakeMind::new().with_reflection(vec![
        sentence("You felt wonderful.", &[]),
        sentence("You went to **Paris**.", &[7]),
        sentence("You **made soup**.", &[1]),
    ]));
    let period = day(&w);
    let Wrote::Day(r) = write(
        &w.engine,
        w.mind.as_ref(),
        &period,
        nocturne_contract::WrittenWhen::OnDemand,
    )
    .await
    .unwrap() else {
        panic!("a day")
    };
    assert_eq!(r.the_day.len(), 1);
    assert_eq!(r.the_day[0].text, "You **made soup**.");
}

#[tokio::test]
async fn a_day_with_no_sentence_that_cites_anything_is_listed_and_stores_nothing() {
    let (w, _) = world_after_a_day(
        FakeMind::new()
            .with_reflection(vec![sentence("A good day.", &[]), sentence("Quiet.", &[])]),
    );
    let period = day(&w);
    let wrote = write(
        &w.engine,
        w.mind.as_ref(),
        &period,
        nocturne_contract::WrittenWhen::OnDemand,
    )
    .await
    .unwrap();
    assert_eq!(wrote, Wrote::Listed);
    assert!(w.engine.saved_reflection(&period).unwrap().is_none());
}

#[tokio::test]
async fn a_write_stopped_before_its_save_stores_nothing_and_keeps_the_earlier_one() {
    let mind = FakeMind::new();
    let gate = mind.gate("reflect");
    let (w, _) = world_after_a_day(mind);
    let period = day(&w);
    gate.open();
    let Wrote::Day(first) = write(
        &w.engine,
        w.mind.as_ref(),
        &period,
        nocturne_contract::WrittenWhen::Overnight,
    )
    .await
    .unwrap() else {
        panic!("a day")
    };

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let late = stop.clone();
    let held = w.mind.gate("reflect");
    let engine = w.engine.clone();
    let mind = w.mind.clone();
    let p = period.clone();
    let task = tokio::spawn(async move {
        write_stoppable(
            &engine,
            mind.as_ref(),
            &p,
            nocturne_contract::WrittenWhen::OnDemand,
            late,
        )
        .await
    });
    held.entered().await;
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    held.open();
    let got = task.await.unwrap();
    assert!(
        matches!(
            got,
            Err(nocturne_intelligence::ReflectError::Engine(
                nocturne_engine::Error::Cancelled
            ))
        ),
        "{got:?}"
    );
    let Some(ReflectionView::Written { reflection }) = w.engine.saved_reflection(&period).unwrap()
    else {
        panic!("the earlier one stays")
    };
    assert_eq!(reflection.id, first.id);
}

#[tokio::test]
async fn a_day_where_more_than_half_the_sentences_were_lost_is_listed() {
    let (w, _) = world_after_a_day(
        FakeMind::new().with_reflection_written(vec![sentence("You **made soup**.", &[1])], 3),
    );
    let period = day(&w);
    let wrote = write(
        &w.engine,
        w.mind.as_ref(),
        &period,
        nocturne_contract::WrittenWhen::OnDemand,
    )
    .await
    .unwrap();
    assert_eq!(wrote, Wrote::Listed);
    assert!(w.engine.saved_reflection(&period).unwrap().is_none());
}

#[tokio::test]
async fn a_day_where_half_or_more_of_the_sentences_survived_is_written() {
    let (w, _) = world_after_a_day(FakeMind::new().with_reflection_written(
        vec![
            sentence("You **made soup**.", &[1]),
            sentence("You **walked by the canal**.", &[0]),
        ],
        3,
    ));
    let period = day(&w);
    let wrote = write(
        &w.engine,
        w.mind.as_ref(),
        &period,
        nocturne_contract::WrittenWhen::OnDemand,
    )
    .await
    .unwrap();
    let Wrote::Day(r) = wrote else {
        panic!("{wrote:?}")
    };
    assert_eq!(r.the_day.len(), 2);
}

#[tokio::test]
async fn a_day_of_only_things_kept_from_elsewhere_is_listed_without_asking_the_mind() {
    let w = world_at("2026-11-12T10:00:00Z", FakeMind::new());
    let letter = id_of(
        w.engine
            .keep_text("Screenshot of the school letter", Source::Screenshot)
            .unwrap(),
    );
    read(&w, &letter);
    w.clock.set_str("2026-11-13T09:00:00Z");
    let period = day(&w);
    let wrote = write(
        &w.engine,
        w.mind.as_ref(),
        &period,
        nocturne_contract::WrittenWhen::OnDemand,
    )
    .await
    .unwrap();
    assert_eq!(wrote, Wrote::Listed);
    assert_eq!(w.mind.call_count("reflect"), 0);
}

#[tokio::test]
async fn a_mind_that_fails_stores_nothing_and_says_so() {
    let (w, _) = world_after_a_day(FakeMind::new());
    w.mind.fail_next("reflect", MindError::Unavailable);
    let period = day(&w);
    let err = write(
        &w.engine,
        w.mind.as_ref(),
        &period,
        nocturne_contract::WrittenWhen::OnDemand,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        nocturne_intelligence::ReflectError::Mind(MindError::Unavailable)
    ));
    assert!(w.engine.saved_reflection(&period).unwrap().is_none());
}

#[tokio::test]
async fn a_week_is_composed_without_the_mind_and_an_empty_week_is_listed() {
    let (w, _) = world_after_a_day(FakeMind::new());
    let week = w.engine.period_of(PeriodKind::Week, date(DAY)).unwrap();
    let Wrote::Recap(recap) = write(
        &w.engine,
        w.mind.as_ref(),
        &week,
        nocturne_contract::WrittenWhen::Overnight,
    )
    .await
    .unwrap() else {
        panic!("a recap")
    };
    assert_eq!(recap.period, week);
    assert!(recap.days >= 1);
    assert!(w.engine.saved_reflection(&week).unwrap().is_some());
    assert_eq!(w.mind.call_count("reflect"), 0, "a recap needs no mind");

    let quiet = w
        .engine
        .period_of(PeriodKind::Week, date("2026-09-02"))
        .unwrap();
    let wrote = write(
        &w.engine,
        w.mind.as_ref(),
        &quiet,
        nocturne_contract::WrittenWhen::Overnight,
    )
    .await
    .unwrap();
    assert_eq!(wrote, Wrote::Listed);
    assert!(w.engine.saved_reflection(&quiet).unwrap().is_none());
}

#[tokio::test]
async fn writing_is_not_using_what_was_read() {
    let w = world_at(
        "2026-11-12T10:00:00Z",
        FakeMind::new().with_extract("Gran lives in Leeds", vec![gran()]),
    );
    common::keep(&w, "Gran lives in Leeds");
    read_all(&w).await;
    w.clock.set_str("2026-11-13T09:00:00Z");
    let before = w
        .engine
        .memories(nocturne_contract::MemoryFilter::default())
        .unwrap();
    assert_eq!(before.len(), 1);
    let period = day(&w);
    for p in [
        period,
        w.engine.period_of(PeriodKind::Week, date(DAY)).unwrap(),
    ] {
        write(
            &w.engine,
            w.mind.as_ref(),
            &p,
            nocturne_contract::WrittenWhen::OnDemand,
        )
        .await
        .unwrap();
    }
    let after = w
        .engine
        .memories(nocturne_contract::MemoryFilter::default())
        .unwrap();
    assert_eq!(after[0].last_used, before[0].last_used);
    assert_eq!(after[0].last_used, None);
}

// ---- the morning run ----

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use nocturne_contract::ReflectionView;
use nocturne_intelligence::{SchedStep, TidyKind, TidyStep};

const NOTE: &str = "Dropped Leo at school, then a long walk by the canal";

fn open_gate(w: &World) -> Arc<AtomicBool> {
    let gate = Arc::new(AtomicBool::new(true));
    let g = gate.clone();
    w.scheduler
        .set_reflecting(w.slot.clone(), Arc::new(move || g.load(Ordering::SeqCst)));
    gate
}

fn period(w: &World, kind: PeriodKind, day: &str) -> nocturne_contract::Period {
    w.engine.period_of(kind, date(day)).unwrap()
}

fn saved(w: &World, kind: PeriodKind, day: &str) -> Option<ReflectionView> {
    w.engine.saved_reflection(&period(w, kind, day)).unwrap()
}

fn first_line(w: &World, day: &str) -> String {
    match saved(w, PeriodKind::Day, day) {
        Some(ReflectionView::Written { reflection }) => reflection.the_day[0].text.clone(),
        other => panic!("no reflection for {day}: {other:?}"),
    }
}

/// Tick until the tidy has saved its page (reading what is unread between ticks).
async fn until_page(w: &World) -> Vec<SchedStep> {
    let mut steps = Vec::new();
    for _ in 0..8 {
        read_all(w).await;
        let step = w.scheduler.tick().await;
        let done = matches!(step, SchedStep::Tidy(TidyStep::Finished(_)));
        steps.push(step);
        if done {
            break;
        }
    }
    steps
}

/// The evening of the 12th with a note kept, then a night tidy at 01:30 that wrote the page
/// before the day's end at 04:00.
async fn night_tidy_before_the_boundary(mind: FakeMind) -> World {
    let w = world_at("2026-11-12T18:00:00Z", mind);
    keep(&w, NOTE);
    w.clock.set_str("2026-11-13T01:30:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    let steps = until_page(&w).await;
    assert!(
        steps
            .iter()
            .any(|s| matches!(s, SchedStep::Tidy(TidyStep::Finished(_)))),
        "{steps:?}"
    );
    w
}

#[tokio::test]
async fn the_morning_run_writes_the_day_that_ended_once_the_boundary_has_passed() {
    let mind = FakeMind::new()
        .with_headline("A quiet night", "One thing kept.")
        .with_reflection(vec![sentence("You **walked by the canal**.", &[0])]);
    let w = night_tidy_before_the_boundary(mind).await;
    open_gate(&w);

    w.clock.set_str("2026-11-13T03:30:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Idle,
        "the day ends at 04:00"
    );
    assert!(saved(&w, PeriodKind::Day, DAY).is_none());

    w.clock.set_str("2026-11-13T04:10:00Z");
    let step = w.scheduler.tick().await;
    assert_eq!(
        step,
        SchedStep::Reflected(vec![period(&w, PeriodKind::Day, DAY)])
    );
    assert_eq!(first_line(&w, DAY), "You **walked by the canal**.");
    let Some(ReflectionView::Written { reflection }) = saved(&w, PeriodKind::Day, DAY) else {
        panic!("written")
    };
    assert_eq!(
        reflection.written,
        nocturne_contract::WrittenWhen::Overnight
    );
    assert!(!reflection.so_far);

    // Written once, not again at the next look.
    w.clock.set_str("2026-11-13T04:20:00Z");
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    assert_eq!(w.mind.call_count("reflect"), 1);
}

#[tokio::test]
async fn the_morning_run_waits_for_mains_and_an_empty_desk_like_the_night_tidy() {
    let w = night_tidy_before_the_boundary(FakeMind::new().with_headline("H", "d")).await;
    open_gate(&w);
    w.clock.set_str("2026-11-13T04:10:00Z");

    w.host.set_idle(Some(Duration::from_secs(5 * 60)));
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    w.host.set_idle(Some(Duration::from_secs(3600)));
    w.host.on_battery(90);
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    assert!(saved(&w, PeriodKind::Day, DAY).is_none());
    assert_eq!(w.mind.call_count("reflect"), 0);

    w.host.set_ac(Some(true));
    assert!(matches!(w.scheduler.tick().await, SchedStep::Reflected(_)));
}

#[tokio::test]
async fn nothing_is_written_while_the_gate_is_closed() {
    let w = night_tidy_before_the_boundary(FakeMind::new().with_headline("H", "d")).await;
    let gate = open_gate(&w);
    gate.store(false, Ordering::SeqCst);
    w.clock.set_str("2026-11-13T04:10:00Z");
    for _ in 0..3 {
        assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    }
    assert_eq!(w.mind.call_count("reflect"), 0);
    assert!(saved(&w, PeriodKind::Day, DAY).is_none());

    gate.store(true, Ordering::SeqCst);
    assert!(matches!(w.scheduler.tick().await, SchedStep::Reflected(_)));
}

#[tokio::test]
async fn a_scheduler_with_no_gate_set_writes_nothing() {
    let w = night_tidy_before_the_boundary(FakeMind::new().with_headline("H", "d")).await;
    w.clock.set_str("2026-11-13T04:10:00Z");
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    assert_eq!(w.mind.call_count("reflect"), 0);
}

#[tokio::test]
async fn a_missing_mind_is_skipped_quietly_and_tried_again_at_the_next_tick() {
    let w = night_tidy_before_the_boundary(FakeMind::new().with_headline("H", "d")).await;
    open_gate(&w);
    w.clock.set_str("2026-11-13T04:10:00Z");
    w.mind_away();
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    assert!(w.events.notices().is_empty(), "no notice for it");
    w.mind_back();
    assert!(matches!(w.scheduler.tick().await, SchedStep::Reflected(_)));
}

#[tokio::test]
async fn a_failed_write_is_not_tried_again_at_every_tick() {
    let w = night_tidy_before_the_boundary(FakeMind::new().with_headline("H", "d")).await;
    open_gate(&w);
    w.clock.set_str("2026-11-13T04:10:00Z");
    w.mind
        .fail_next("reflect", MindError::Failed("model".into()));
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    assert_eq!(w.mind.call_count("reflect"), 1, "it waits a while");
    w.clock.set_str("2026-11-13T04:40:00Z");
    assert!(matches!(w.scheduler.tick().await, SchedStep::Reflected(_)));
}

#[tokio::test]
async fn a_day_that_cannot_be_written_is_listed_and_not_asked_for_again() {
    let mind = FakeMind::new()
        .with_headline("H", "d")
        .with_reflection(vec![sentence("A good day.", &[])]);
    let w = night_tidy_before_the_boundary(mind).await;
    open_gate(&w);
    w.clock.set_str("2026-11-13T04:10:00Z");
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    w.clock.set_str("2026-11-13T04:20:00Z");
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    assert_eq!(w.mind.call_count("reflect"), 1);
    assert!(saved(&w, PeriodKind::Day, DAY).is_none());
}

#[tokio::test]
async fn the_week_is_composed_after_the_days_it_ends_with() {
    // The week of Monday 2 November ends on Sunday the 8th, and its last day on the 9th at 04:00.
    let w = world_at(
        "2026-11-08T12:00:00Z",
        FakeMind::new().with_headline("H", "d"),
    );
    keep(&w, "Sunday lunch at Gran's");
    w.clock.set_str("2026-11-09T01:30:00Z");
    open_gate(&w);
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    until_page(&w).await;
    w.clock.set_str("2026-11-09T04:10:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Reflected(vec![
            period(&w, PeriodKind::Day, "2026-11-08"),
            period(&w, PeriodKind::Week, "2026-11-08"),
        ])
    );
    assert!(matches!(
        saved(&w, PeriodKind::Week, "2026-11-08"),
        Some(ReflectionView::Recap { .. })
    ));
}

/// The Sunday-lunch world of the week test, with the day's reflection held at the mind until
/// `act` has run, so that a second period (the week) is still to come.
async fn hold_the_first_period_while(act: impl FnOnce(&World)) -> (World, SchedStep) {
    let mind = FakeMind::new().with_headline("H", "d");
    let gate = mind.gate("reflect");
    let w = world_at("2026-11-08T12:00:00Z", mind);
    keep(&w, "Sunday lunch at Gran's");
    w.clock.set_str("2026-11-09T01:30:00Z");
    open_gate(&w);
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    until_page(&w).await;
    w.clock.set_str("2026-11-09T04:10:00Z");
    let scheduler = w.scheduler.clone();
    let tick = tokio::spawn(async move { scheduler.tick().await });
    for _ in 0..2000 {
        if w.mind.call_count("reflect") == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(w.mind.call_count("reflect"), 1, "the day is being written");
    act(&w);
    gate.open();
    let step = tick.await.unwrap();
    (w, step)
}

#[tokio::test]
async fn a_second_period_is_not_written_once_the_desk_is_in_use_again() {
    let (w, step) = hold_the_first_period_while(|w| {
        w.host.set_idle(Some(Duration::from_secs(5)));
    })
    .await;
    assert_eq!(
        step,
        SchedStep::Reflected(vec![period(&w, PeriodKind::Day, "2026-11-08")])
    );
    assert!(saved(&w, PeriodKind::Week, "2026-11-08").is_none());
}

#[tokio::test]
async fn a_second_period_is_not_written_once_the_battery_is_low() {
    let (w, step) = hold_the_first_period_while(|w| w.host.on_battery(10)).await;
    assert_eq!(
        step,
        SchedStep::Reflected(vec![period(&w, PeriodKind::Day, "2026-11-08")])
    );
    assert!(saved(&w, PeriodKind::Week, "2026-11-08").is_none());
}

#[tokio::test]
async fn a_second_period_is_not_written_once_the_page_time_has_passed() {
    let (w, step) = hold_the_first_period_while(|w| w.clock.set_str("2026-11-09T09:00:00Z")).await;
    assert_eq!(
        step,
        SchedStep::Reflected(vec![period(&w, PeriodKind::Day, "2026-11-08")])
    );
    assert!(saved(&w, PeriodKind::Week, "2026-11-08").is_none());
}

#[tokio::test]
async fn a_tidy_that_starts_after_the_boundary_writes_the_reflection_before_its_page() {
    let w = world_at(
        "2026-11-12T18:00:00Z",
        FakeMind::new().with_headline("H", "d"),
    );
    keep(&w, NOTE);
    w.clock.set_str("2026-11-13T05:00:00Z");
    open_gate(&w);
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    until_page(&w).await;
    assert!(saved(&w, PeriodKind::Day, DAY).is_some());
    let calls = w.mind.calls();
    let reflect = calls.iter().position(|c| c == "reflect").unwrap();
    let headline = calls.iter().position(|c| c == "write_headline").unwrap();
    assert!(reflect < headline, "{calls:?}");
}

// ---- the catch-up ----

#[tokio::test]
async fn a_missed_night_writes_missed_days_newest_first_before_the_page_and_stops_at_seven_days() {
    let mind = FakeMind::new()
        .with_headline("A late page", "Three days.")
        .with_reflection(vec![sentence("Newest.", &[0])])
        .with_reflection(vec![sentence("Middle.", &[0])])
        .with_reflection(vec![sentence("Oldest.", &[0])]);
    let w = world_at("2026-11-02T10:00:00Z", mind);
    // Too far back to be written: kept on the 2nd, looked at on the 13th.
    keep(&w, "Notes from the first week of the month");
    for (day, text) in [
        ("2026-11-10", "Tuesday: dentist, then soup"),
        ("2026-11-11", "Wednesday: the long walk"),
        ("2026-11-12", "Thursday: Leo's concert"),
    ] {
        w.clock.set_str(&format!("{day}T12:00:00Z"));
        keep(&w, text);
    }
    // The laptop was off; it wakes at nine in the morning of the 13th.
    w.clock.set_str("2026-11-13T09:00:00Z");
    open_gate(&w);

    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::CatchUp)
    );
    let steps = until_page(&w).await;
    assert!(
        steps
            .iter()
            .any(|s| matches!(s, SchedStep::Tidy(TidyStep::Finished(_)))),
        "{steps:?}"
    );
    assert_eq!(first_line(&w, "2026-11-12"), "Newest.");
    assert_eq!(first_line(&w, "2026-11-11"), "Middle.");
    assert_eq!(first_line(&w, "2026-11-10"), "Oldest.");
    assert!(saved(&w, PeriodKind::Day, "2026-11-02").is_none());

    // Before the page: the mind wrote reflections, then the headline.
    let calls = w.mind.calls();
    let last_reflect = calls.iter().rposition(|c| c == "reflect").unwrap();
    let headline = calls.iter().position(|c| c == "write_headline").unwrap();
    assert!(last_reflect < headline, "{calls:?}");
    assert_eq!(w.mind.call_count("reflect"), 3);
}

#[tokio::test]
async fn the_catch_up_writes_nothing_while_the_gate_is_closed() {
    let w = world_at(
        "2026-11-12T12:00:00Z",
        FakeMind::new().with_headline("H", "d"),
    );
    keep(&w, NOTE);
    w.clock.set_str("2026-11-13T09:00:00Z");
    let gate = open_gate(&w);
    gate.store(false, Ordering::SeqCst);
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::CatchUp)
    );
    until_page(&w).await;
    assert_eq!(w.mind.call_count("reflect"), 0);
    assert!(saved(&w, PeriodKind::Day, DAY).is_none());
}

#[tokio::test]
async fn the_catch_up_on_battery_writes_above_forty_per_cent_and_leaves_it_below() {
    let w = world_at(
        "2026-11-12T12:00:00Z",
        FakeMind::new().with_headline("H", "d"),
    );
    keep(&w, NOTE);
    w.clock.set_str("2026-11-13T09:00:00Z");
    open_gate(&w);
    w.host.on_battery(55);
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::CatchUp)
    );
    until_page(&w).await;
    assert!(saved(&w, PeriodKind::Day, DAY).is_some());

    let low = world_at(
        "2026-11-12T12:00:00Z",
        FakeMind::new().with_headline("H", "d"),
    );
    keep(&low, NOTE);
    low.clock.set_str("2026-11-13T09:00:00Z");
    open_gate(&low);
    low.host.on_battery(30);
    assert_eq!(low.scheduler.tick().await, SchedStep::WaitingForPower);
    assert_eq!(low.mind.call_count("reflect"), 0);
}

#[tokio::test]
async fn a_page_already_written_still_gets_its_missed_reflection_when_the_desk_is_empty() {
    // The night tidy ran at 01:30 and the laptop slept through 04:00 to 07:00: it wakes at nine.
    let w = night_tidy_before_the_boundary(FakeMind::new().with_headline("H", "d")).await;
    open_gate(&w);
    w.clock.set_str("2026-11-13T09:00:00Z");
    w.host.resume();
    w.host.set_idle(Some(Duration::from_secs(60)));
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Idle,
        "someone is at the keyboard"
    );
    w.host.set_idle(Some(Duration::from_secs(11 * 60)));
    w.clock.set_str("2026-11-13T09:20:00Z");
    assert!(matches!(w.scheduler.tick().await, SchedStep::Reflected(_)));
    assert!(saved(&w, PeriodKind::Day, DAY).is_some());
}

#[tokio::test]
async fn the_morning_run_never_marks_a_memory_used() {
    let mind = FakeMind::new()
        .with_headline("H", "d")
        .with_extract(NOTE, vec![gran()]);
    let w = world_at("2026-11-12T18:00:00Z", mind);
    common::keep(&w, NOTE);
    w.clock.set_str("2026-11-13T01:30:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    until_page(&w).await;
    let used = || -> Vec<Option<String>> {
        w.engine
            .memories(nocturne_contract::MemoryFilter::default())
            .unwrap()
            .into_iter()
            .map(|m| m.last_used)
            .collect()
    };
    let before = used();
    assert_eq!(before.len(), 1);

    open_gate(&w);
    w.clock.set_str("2026-11-13T04:10:00Z");
    assert!(matches!(w.scheduler.tick().await, SchedStep::Reflected(_)));
    assert_eq!(used(), before, "looking back is not use");
}
