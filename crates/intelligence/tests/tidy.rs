mod common;

use std::time::Duration;

use common::*;
use jiff::Timestamp;
use nocturne_contract::{Edition, ItemStatus, NoticeCode, PageVariant};
use nocturne_engine::{FakeMind, MindError};
use nocturne_intelligence::{Event, SchedStep, TidyKind, TidyStep};

const TEXT: &str = "Gran lives in Leeds";

fn mind() -> FakeMind {
    FakeMind::new()
        .with_extract(TEXT, vec![gran()])
        .with_headline("A quiet night", "One thing kept.")
}

fn at(w: &World, when: &str) {
    w.clock.set_str(when);
}

/// A world at `start` with one note kept at `kept`, then the clock moved to `now`.
fn world_with_note(kept: &str, now: &str) -> (World, String) {
    let w = world_at(kept, mind());
    let id = keep(&w, TEXT);
    at(&w, now);
    (w, id)
}

/// Read, tick, and read again until the scheduler reports `want` or `rounds` run out.
async fn pump(w: &World, rounds: usize) -> Vec<SchedStep> {
    let mut out = Vec::new();
    for _ in 0..rounds {
        read_all(w).await;
        out.push(w.scheduler.tick().await);
    }
    out
}

fn finished(steps: &[SchedStep]) -> bool {
    steps
        .iter()
        .any(|s| matches!(s, SchedStep::Tidy(TidyStep::Finished(_))))
}

// ---- the night tidy ----

#[tokio::test]
async fn the_night_tidy_writes_one_page_and_not_a_second_at_breakfast() {
    let (w, note) = world_with_note("2026-11-12T20:00:00Z", "2026-11-13T00:30:00Z");
    // Before the window: nothing.
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    at(&w, "2026-11-13T03:00:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    let steps = pump(&w, 4).await;
    assert!(finished(&steps), "{steps:?}");

    let page = w.engine.page(Some(date("2026-11-13"))).unwrap().unwrap();
    assert_eq!(page.headline, "A quiet night");
    assert_eq!(page.pending, 0);
    assert_eq!(page.tidied_from.as_deref(), Some("2026-11-13T03:00:00Z"));
    assert!(page.tidied_to.is_some());
    assert_eq!(status(&w, &note), ItemStatus::Understood);
    assert_eq!(w.events.pages().len(), 1);
    assert_eq!(w.events.pages()[0].date, "2026-11-13");
    // The tidy was recorded for Advanced details.
    let report = w.engine.last_tidy().unwrap().unwrap();
    assert_eq!(report.items, 1);
    // Progress ended.
    assert_eq!(
        w.events.all().iter().rev().find_map(|e| match e {
            Event::Progress(p) => Some(p.clone()),
            _ => None,
        }),
        Some(None)
    );
    assert!(w.tidy.progress().borrow().is_none());
    assert_eq!(w.host.inhibitors(), 0, "the machine may sleep again");

    // At 07:05 there is no second page, whatever the scheduler thinks.
    for t in [
        "2026-11-13T05:00:00Z",
        "2026-11-13T07:05:00Z",
        "2026-11-13T07:40:00Z",
    ] {
        at(&w, t);
        let step = w.scheduler.tick().await;
        assert_eq!(step, SchedStep::Idle, "{t}");
    }
    assert_eq!(w.engine.pages().unwrap().len(), 1);
}

#[tokio::test]
async fn the_night_tidy_needs_the_small_hours_mains_and_an_empty_desk() {
    let (w, _) = world_with_note("2026-11-12T20:00:00Z", "2026-11-13T03:00:00Z");
    w.host.set_idle(Some(Duration::from_secs(9 * 60)));
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Idle,
        "someone is at the keyboard"
    );
    w.host.set_idle(Some(Duration::from_secs(10 * 60)));
    w.host.on_battery(80);
    assert_eq!(w.scheduler.tick().await, SchedStep::WaitingForPower);
    assert_eq!(w.scheduler.tick().await, SchedStep::WaitingForPower);
    let n = w.events.notices();
    assert_eq!(n.len(), 1, "the notice is raised once");
    assert_eq!(n[0].code, NoticeCode::TidyPausedBattery);
    w.host.set_ac(Some(true));
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    assert!(
        w.events
            .all()
            .contains(&Event::NoticeCleared(NoticeCode::TidyPausedBattery))
    );
}

#[tokio::test]
async fn unknown_idle_and_unknown_power_count_as_away_and_mains_in_the_small_hours() {
    let (w, _) = world_with_note("2026-11-12T20:00:00Z", "2026-11-13T03:00:00Z");
    w.host.set_idle(None);
    w.host.set_ac(None);
    w.host.set_percent(None);
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
}

#[tokio::test]
async fn the_window_follows_local_time_and_the_page_time() {
    // Summer: 07:00 BST is 06:00 UTC; the window opens at 00:00 UTC.
    let w = world_at("2026-07-01T00:00:00Z", mind());
    at(&w, "2026-06-30T23:30:00Z");
    keep(&w, TEXT);
    at(&w, "2026-06-30T23:45:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Idle,
        "00:45 local is before 01:00"
    );
    at(&w, "2026-07-01T00:10:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
}

#[tokio::test]
async fn pausing_stops_the_tidy_until_the_pause_ends() {
    let (w, _) = world_with_note("2026-11-12T20:00:00Z", "2026-11-13T03:00:00Z");
    w.scheduler
        .set_paused_until(Some("2026-11-13T03:10:00Z".parse::<Timestamp>().unwrap()));
    assert_eq!(w.scheduler.tick().await, SchedStep::Paused);
    assert_eq!(w.reader.tick().await, nocturne_intelligence::Step::Paused);
    at(&w, "2026-11-13T03:11:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
}

// ---- catch-up ----

/// A page saved for 2026-11-10 by hand, then three days with a note on the 12th.
fn world_three_days_off() -> World {
    let w = world_at("2026-11-09T20:00:00Z", mind());
    keep(&w, TEXT);
    at(&w, "2026-11-10T07:00:00Z");
    let page = w
        .engine
        .compose_page(date("2026-11-10"), Edition::Screen)
        .unwrap();
    w.engine.save_page(&page).unwrap();
    at(&w, "2026-11-12T20:00:00Z");
    keep(&w, "Leo swims on Tuesdays");
    at(&w, "2026-11-13T09:00:00Z");
    w
}

#[tokio::test]
async fn three_days_off_give_one_catch_up_page_started_at_launch() {
    let w = world_three_days_off();
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::CatchUp)
    );
    assert!(
        w.events
            .notices()
            .iter()
            .any(|n| n.code == NoticeCode::CatchingUp)
    );
    let steps = pump(&w, 4).await;
    assert!(finished(&steps), "{steps:?}");
    assert!(
        w.events
            .all()
            .contains(&Event::NoticeCleared(NoticeCode::CatchingUp))
    );
    let page = w.engine.page(Some(date("2026-11-13"))).unwrap().unwrap();
    assert_eq!(page.variant, PageVariant::CatchUp);
    assert_eq!(page.catch_up_days, 2);
    assert_eq!(w.engine.pages().unwrap().len(), 2, "one page, not three");
    for t in [
        "2026-11-13T09:10:00Z",
        "2026-11-13T12:00:00Z",
        "2026-11-14T00:30:00Z",
    ] {
        at(&w, t);
        assert_eq!(w.scheduler.tick().await, SchedStep::Idle, "{t}");
    }
    assert_eq!(w.engine.pages().unwrap().len(), 2);
}

#[tokio::test]
async fn catch_up_on_a_low_battery_waits_but_tidy_anyway_goes() {
    let w = world_three_days_off();
    w.host.on_battery(25);
    assert_eq!(w.scheduler.tick().await, SchedStep::WaitingForPower);
    w.tidy.tidy_anyway();
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::CatchUp)
    );
}

#[tokio::test]
async fn a_catch_up_is_looked_for_every_five_minutes_not_every_tick() {
    let w = world_at("2026-11-13T06:00:00Z", mind());
    // Somebody is at the keyboard, so the night tidy keeps out of the way.
    w.host.set_idle(Some(Duration::from_secs(30)));
    keep(&w, TEXT);
    at(&w, "2026-11-13T06:59:00Z");
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    // The page time passes, but the last look was moments ago. (Steps stay under a minute so
    // the manual clock never looks like a sleep.)
    let first: Timestamp = "2026-11-13T06:59:00Z".parse().unwrap();
    for k in 1..=5 {
        w.clock.set(first + jiff::SignedDuration::from_secs(50 * k));
        assert_eq!(w.scheduler.tick().await, SchedStep::Idle, "step {k}");
    }
    w.clock.set(first + jiff::SignedDuration::from_secs(300));
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::CatchUp)
    );
}

#[tokio::test]
async fn an_empty_store_does_not_write_a_first_page_at_launch() {
    let w = world_at("2026-11-13T09:00:00Z", mind());
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    keep(&w, TEXT);
    at(&w, "2026-11-13T10:00:00Z");
    // Kept after this morning's page time: it belongs to tomorrow's page.
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    assert!(w.engine.pages().unwrap().is_empty());
}

// ---- sleep ----

#[tokio::test]
async fn sleeping_mid_tidy_resumes_without_a_second_page() {
    let (w, note) = world_with_note("2026-11-13T02:00:00Z", "2026-11-13T03:00:00Z");
    let gate = w.mind.gate("extract");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    let reader = w.reader.clone();
    let reading = tokio::spawn(async move { reader.tick().await });
    gate.entered().await;
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Tidy(TidyStep::Running { done: 0, total: 1 })
    );
    assert_eq!(w.host.inhibitors(), 1, "kept awake while it works on mains");

    // The lid closes; two hours pass; the lid opens.
    at(&w, "2026-11-13T05:00:00Z");
    w.host.resume();
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Tidy(TidyStep::Running { done: 0, total: 1 })
    );
    gate.open();
    reading.await.unwrap();
    assert_eq!(status(&w, &note), ItemStatus::Understood);
    let step = w.scheduler.tick().await;
    assert!(
        matches!(step, SchedStep::Tidy(TidyStep::Finished(_))),
        "{step:?}"
    );
    for t in ["2026-11-13T05:30:00Z", "2026-11-13T07:30:00Z"] {
        at(&w, t);
        assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    }
    assert_eq!(w.engine.pages().unwrap().len(), 1);
    assert_eq!(w.events.pages().len(), 1);
    assert_eq!(w.host.inhibitors(), 0);
}

#[tokio::test]
async fn a_clock_that_ran_ahead_of_the_monotonic_clock_counts_as_waking() {
    let w = world_three_days_off();
    // Nothing missed at launch if a page for this morning exists.
    let page = w
        .engine
        .compose_page(date("2026-11-13"), Edition::Screen)
        .unwrap();
    w.engine.save_page(&page).unwrap();
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    // The next day's page is missed while the machine sleeps through the page time.
    at(&w, "2026-11-13T22:00:00Z");
    keep(&w, "late note");
    at(&w, "2026-11-14T10:00:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::CatchUp)
    );
}

// ---- battery ----

#[tokio::test]
async fn a_tidy_waits_for_power_below_forty_percent_and_goes_on_when_told() {
    let (w, note) = world_with_note("2026-11-13T02:00:00Z", "2026-11-13T03:00:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    assert_eq!(w.host.inhibitors(), 0, "nothing held before the first tick");
    w.host.on_battery(30);
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Tidy(TidyStep::PausedBattery { done: 0, total: 1 })
    );
    assert!(
        w.events
            .notices()
            .iter()
            .any(|n| n.code == NoticeCode::TidyPausedBattery)
    );
    assert_eq!(w.host.inhibitors(), 0);
    // On battery the reader does not touch what the tidy waits for.
    assert_eq!(w.reader.tick().await, nocturne_intelligence::Step::Idle);
    assert_eq!(status(&w, &note), ItemStatus::Kept);

    w.tidy.tidy_anyway();
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Tidy(TidyStep::Running { done: 0, total: 1 })
    );
    assert!(
        w.events
            .all()
            .contains(&Event::NoticeCleared(NoticeCode::TidyPausedBattery))
    );
    read_all(&w).await;
    assert_eq!(status(&w, &note), ItemStatus::Understood);
    let step = w.scheduler.tick().await;
    assert!(
        matches!(step, SchedStep::Tidy(TidyStep::Finished(_))),
        "{step:?}"
    );
}

#[tokio::test]
async fn forty_percent_is_enough_and_power_coming_back_resumes() {
    let (w, note) = world_with_note("2026-11-13T02:00:00Z", "2026-11-13T03:00:00Z");
    w.scheduler.tick().await;
    w.host.on_battery(39);
    assert!(matches!(
        w.scheduler.tick().await,
        SchedStep::Tidy(TidyStep::PausedBattery { .. })
    ));
    w.host.set_ac(Some(true));
    assert!(matches!(
        w.scheduler.tick().await,
        SchedStep::Tidy(TidyStep::Running { .. })
    ));
    assert_eq!(w.host.inhibitors(), 1);
    read_all(&w).await;
    assert_eq!(status(&w, &note), ItemStatus::Understood);
}

// ---- the page while it is written ----

#[tokio::test]
async fn while_the_tidy_runs_the_page_is_being_written_with_progress() {
    let (w, note) = world_with_note("2026-11-13T02:00:00Z", "2026-11-13T03:00:00Z");
    w.scheduler.tick().await;
    let live = w
        .engine
        .compose_page(date("2026-11-13"), Edition::Screen)
        .unwrap();
    assert_eq!(live.variant, PageVariant::BeingWritten);
    assert_eq!(live.pending, 1);
    let progress = w.tidy.progress().borrow().clone().unwrap();
    assert_eq!((progress.done, progress.total), (0, 1));
    read_all(&w).await;
    let step = w.scheduler.tick().await;
    assert!(matches!(step, SchedStep::Tidy(TidyStep::Finished(_))));
    let saved = w.engine.page(Some(date("2026-11-13"))).unwrap().unwrap();
    assert_ne!(saved.variant, PageVariant::BeingWritten);
    assert_eq!(status(&w, &note), ItemStatus::Understood);
}

#[tokio::test]
async fn the_headline_falls_back_to_the_template_when_the_mind_errors() {
    let (w, _) = world_with_note("2026-11-13T02:00:00Z", "2026-11-13T03:00:00Z");
    w.mind
        .fail_next("write_headline", MindError::Failed("no".into()));
    w.scheduler.tick().await;
    pump(&w, 3).await;
    let page = w.engine.page(Some(date("2026-11-13"))).unwrap().unwrap();
    assert_eq!(page.headline, "Friday 13 November");
}

// ---- deadlines and the backlog ----

#[tokio::test]
async fn twenty_minutes_without_a_mind_saves_the_page_with_the_backlog_and_saves_it_once_more() {
    let (w, note) = world_with_note("2026-11-13T02:00:00Z", "2026-11-13T03:00:00Z");
    w.mind_away();
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    assert_eq!(w.reader.tick().await, nocturne_intelligence::Step::NoMind);
    at(&w, "2026-11-13T03:19:30Z");
    assert!(matches!(
        w.scheduler.tick().await,
        SchedStep::Tidy(TidyStep::Running { .. })
    ));
    at(&w, "2026-11-13T03:20:00Z");
    let step = w.scheduler.tick().await;
    assert!(
        matches!(step, SchedStep::Tidy(TidyStep::Finished(_))),
        "{step:?}"
    );
    let page = w.engine.page(Some(date("2026-11-13"))).unwrap().unwrap();
    assert_eq!(page.pending, 1);
    assert_ne!(page.variant, PageVariant::BeingWritten);
    assert_eq!(
        page.headline, "Friday 13 November",
        "no mind, no written headline"
    );
    assert_eq!(status(&w, &note), ItemStatus::Kept);

    // The mind comes back, the backlog is read, and nobody has opened the page.
    w.mind_back();
    at(&w, "2026-11-13T03:30:00Z");
    w.reader.retry_now();
    read_all(&w).await;
    assert_eq!(status(&w, &note), ItemStatus::Understood);
    let step = w.scheduler.tick().await;
    assert!(
        matches!(step, SchedStep::Tidy(TidyStep::Resaved(_))),
        "{step:?}"
    );
    let page = w.engine.page(Some(date("2026-11-13"))).unwrap().unwrap();
    assert_eq!(page.pending, 0);
    assert_eq!(page.headline, "A quiet night");
    assert_eq!(w.events.pages().len(), 2);
    assert!(
        !w.engine.can_resave_page(date("2026-11-13")).unwrap(),
        "once only"
    );
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
}

#[tokio::test]
async fn a_page_the_person_has_opened_is_not_written_again() {
    let (w, note) = world_with_note("2026-11-13T02:00:00Z", "2026-11-13T03:00:00Z");
    w.mind_away();
    w.scheduler.tick().await;
    w.reader.tick().await;
    at(&w, "2026-11-13T03:21:00Z");
    w.scheduler.tick().await;
    assert!(w.engine.page_opened(date("2026-11-13")).unwrap());
    w.mind_back();
    w.reader.retry_now();
    read_all(&w).await;
    assert_eq!(status(&w, &note), ItemStatus::Understood);
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    assert_eq!(w.events.pages().len(), 1);
    assert_eq!(
        w.engine
            .page(Some(date("2026-11-13")))
            .unwrap()
            .unwrap()
            .pending,
        1
    );
}

#[tokio::test]
async fn the_page_time_is_the_deadline_for_a_night_tidy() {
    let (w, note) = world_with_note("2026-11-13T02:00:00Z", "2026-11-13T03:00:00Z");
    let gate = w.mind.gate("extract");
    w.scheduler.tick().await;
    let reader = w.reader.clone();
    let reading = tokio::spawn(async move { reader.tick().await });
    gate.entered().await;
    at(&w, "2026-11-13T06:59:00Z");
    assert!(matches!(
        w.scheduler.tick().await,
        SchedStep::Tidy(TidyStep::Running { .. })
    ));
    at(&w, "2026-11-13T07:00:00Z");
    let step = w.scheduler.tick().await;
    assert!(
        matches!(step, SchedStep::Tidy(TidyStep::Finished(_))),
        "{step:?}"
    );
    let page = w.engine.page(Some(date("2026-11-13"))).unwrap().unwrap();
    assert_eq!(page.pending, 1);
    assert_eq!(status(&w, &note), ItemStatus::Reading);
    gate.open();
    reading.await.unwrap();
}

#[tokio::test]
async fn the_reader_going_away_mid_tidy_does_not_make_a_second_page_or_double_mark_use() {
    let (w, _) = world_with_note("2026-11-13T02:00:00Z", "2026-11-13T03:00:00Z");
    w.scheduler.tick().await;
    w.mind.fail_next("extract", MindError::Unavailable);
    let step = w.reader.tick().await;
    assert!(matches!(
        step,
        nocturne_intelligence::Step::Read {
            outcome: nocturne_intelligence::Outcome::Returned,
            ..
        }
    ));
    at(&w, "2026-11-13T03:02:00Z");
    pump(&w, 4).await;
    assert_eq!(w.engine.pages().unwrap().len(), 1);
}

#[tokio::test]
async fn a_two_minute_jump_of_the_wall_clock_is_a_wake_and_checks_for_a_missed_page() {
    let w = world_at("2026-11-13T06:00:00Z", mind());
    w.host.set_idle(Some(Duration::from_secs(30)));
    keep(&w, TEXT);
    at(&w, "2026-11-13T06:59:00Z");
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    // The lid was closed through the page time: only two minutes of the monotonic clock
    // passed, but the wall clock did not.
    at(&w, "2026-11-13T07:01:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::CatchUp)
    );
}

#[tokio::test]
async fn pausing_lets_the_machine_sleep() {
    let (w, _) = world_with_note("2026-11-13T02:00:00Z", "2026-11-13T03:00:00Z");
    w.scheduler.tick().await;
    w.scheduler.tick().await;
    assert_eq!(w.host.inhibitors(), 1);
    w.scheduler
        .set_paused_until(Some("2026-11-13T04:00:00Z".parse::<Timestamp>().unwrap()));
    assert_eq!(w.scheduler.tick().await, SchedStep::Paused);
    assert_eq!(w.host.inhibitors(), 0);
}

// ---- a drain that cannot finish, a fresh install, a slow mind ----

/// A catch-up for a note kept just before the page time, started just after it.
async fn catch_up_with_a_fresh_note() -> (World, String) {
    let w = world_at("2026-11-13T06:58:00Z", mind());
    w.host.set_idle(Some(Duration::from_secs(30)));
    let note = keep(&w, TEXT);
    at(&w, "2026-11-13T06:59:00Z");
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    at(&w, "2026-11-13T07:05:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::CatchUp)
    );
    (w, note)
}

#[tokio::test]
async fn an_item_undone_during_a_catch_up_does_not_hold_the_tidy_forever() {
    let (w, note) = catch_up_with_a_fresh_note().await;
    assert_eq!(w.host.inhibitors(), 0);
    w.engine.unkeep(&note).unwrap();
    let step = w.scheduler.tick().await;
    assert!(
        matches!(
            step,
            SchedStep::Tidy(TidyStep::Finished(_) | TidyStep::Skipped)
        ),
        "{step:?}"
    );
    assert!(!w.tidy.running().await);
    assert_eq!(w.host.inhibitors(), 0);
}

#[tokio::test]
async fn delete_everything_during_a_catch_up_ends_the_tidy_too() {
    let (w, _) = catch_up_with_a_fresh_note().await;
    w.scheduler.tick().await;
    assert_eq!(w.host.inhibitors(), 1);
    w.engine
        .delete_everything(nocturne_engine::DELETE_PHRASE)
        .unwrap();
    let step = w.scheduler.tick().await;
    assert!(
        matches!(
            step,
            SchedStep::Tidy(TidyStep::Finished(_) | TidyStep::Skipped)
        ),
        "{step:?}"
    );
    assert!(!w.tidy.running().await);
    assert_eq!(w.host.inhibitors(), 0);
}

#[tokio::test]
async fn a_catch_up_saves_its_page_after_two_hours_whatever_is_unread() {
    let (w, note) = catch_up_with_a_fresh_note().await;
    let gate = w.mind.gate("extract");
    let reader = w.reader.clone();
    let reading = tokio::spawn(async move { reader.tick().await });
    gate.entered().await;
    at(&w, "2026-11-13T08:59:00Z");
    assert!(matches!(
        w.scheduler.tick().await,
        SchedStep::Tidy(TidyStep::Running { .. })
    ));
    at(&w, "2026-11-13T09:05:00Z");
    let step = w.scheduler.tick().await;
    assert!(
        matches!(step, SchedStep::Tidy(TidyStep::Finished(_))),
        "{step:?}"
    );
    assert_eq!(
        w.engine
            .page(Some(date("2026-11-13")))
            .unwrap()
            .unwrap()
            .pending,
        1
    );
    assert_eq!(status(&w, &note), ItemStatus::Reading);
    gate.open();
    reading.await.unwrap();
}

#[tokio::test]
async fn a_fresh_install_writes_no_page_overnight_until_something_was_kept() {
    let w = world_at("2026-11-13T03:00:00Z", mind());
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    at(&w, "2026-11-13T03:10:00Z");
    assert_eq!(w.scheduler.tick().await, SchedStep::Idle);
    assert!(w.engine.pages().unwrap().is_empty());
    assert!(w.events.pages().is_empty());

    // The same night with a note kept the evening before does write one.
    let w = world_at("2026-11-12T21:00:00Z", mind());
    keep(&w, TEXT);
    at(&w, "2026-11-13T03:00:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
}

#[tokio::test]
async fn a_slow_mind_filling_in_vectors_does_not_freeze_pause_or_status() {
    let (w, note) = world_with_note("2026-11-13T02:00:00Z", "2026-11-13T03:00:00Z");
    // The note is read but its vector is not stored: the tidy will fill it in.
    w.mind.fail_next("embed", MindError::Failed("later".into()));
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    read_all(&w).await;
    assert_eq!(status(&w, &note), ItemStatus::Understood);
    assert_eq!(w.engine.memories_without_embedding(10).unwrap().len(), 1);

    let gate = w.mind.gate("embed");
    let scheduler = w.scheduler.clone();
    let finishing = tokio::spawn(async move { scheduler.tick().await });
    gate.entered().await;
    // The page is being written, the mind is slow: pausing and asking still answer at once.
    let running = tokio::time::timeout(Duration::from_secs(2), w.tidy.running()).await;
    assert_eq!(running, Ok(true));
    w.scheduler
        .set_paused_until(Some("2026-11-13T04:00:00Z".parse::<Timestamp>().unwrap()));
    let step = tokio::time::timeout(Duration::from_secs(2), w.scheduler.tick()).await;
    assert_eq!(step, Ok(SchedStep::Paused));
    gate.open();
    let step = finishing.await.unwrap();
    assert!(
        matches!(step, SchedStep::Tidy(TidyStep::Finished(_))),
        "{step:?}"
    );
    assert!(w.engine.memories_without_embedding(10).unwrap().is_empty());
}
