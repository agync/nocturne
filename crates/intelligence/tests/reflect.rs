mod common;

use common::*;
use nocturne_contract::{PeriodKind, Source};
use nocturne_engine::{FakeMind, MindError, WrittenSentence};
use nocturne_intelligence::Wrote;
use nocturne_intelligence::reflect::{MAX_SENTENCES, write};

const DAY: &str = "2026-11-12";

fn sentence(text: &str, cites: &[usize]) -> WrittenSentence {
    WrittenSentence {
        text: text.into(),
        cites: cites.to_vec(),
    }
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
async fn a_day_of_only_things_kept_from_elsewhere_is_listed_without_asking_the_mind() {
    let w = world_at("2026-11-12T10:00:00Z", FakeMind::new());
    w.engine
        .keep_text("Screenshot of the school letter", Source::Screenshot)
        .unwrap();
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
    keep(&w, "Gran lives in Leeds");
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
