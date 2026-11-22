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

fn drinks_claim(text: &str, subject: &str, object: &str) -> nocturne_engine::Candidate {
    nocturne_engine::Candidate {
        subject_kind: Some(nocturne_contract::EntityKind::Thing),
        ..nocturne_engine::Candidate::new(
            subject,
            nocturne_engine::Predicate::Knows,
            object,
            text,
            0.8,
        )
    }
}

/// A voice note said "Trinks" for "Drinks". The person renamed the thing; the tidy then reads
/// a note still unread that says "Trinks", and reads the first note again. Neither may bring
/// the old name back.
#[tokio::test]
async fn the_tidy_keeps_a_name_the_person_chose() {
    let first = "Trinks are 5 pounds";
    let second = "Trinks run out on Sunday";
    let mind = FakeMind::new()
        .with_extract(
            first,
            vec![drinks_claim("Trinks are 5 pounds.", "Trinks", "5 pounds")],
        )
        .with_extract(
            second,
            vec![drinks_claim(
                "Trinks run out on Sunday.",
                "Trinks",
                "run out on Sunday",
            )],
        )
        .with_headline("A quiet night", "Two things kept.");
    let w = world_at("2026-11-12T20:00:00Z", mind);
    let note = keep(&w, first);
    read_all(&w).await;
    let entity = w.engine.entities(None).unwrap().remove(0).id;
    w.engine.rename_entity(&entity, "Drinks").unwrap();
    keep(&w, second);
    w.engine.queue_reread(std::slice::from_ref(&note)).unwrap();

    at(&w, "2026-11-13T03:00:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    let steps = pump(&w, 4).await;
    assert!(finished(&steps), "{steps:?}");
    read_all(&w).await;
    assert!(w.engine.rereads(10).unwrap().is_empty(), "read again");

    let things = w.engine.entities(None).unwrap();
    assert_eq!(things.len(), 1, "{things:?}");
    assert_eq!(things[0].name, "Drinks");
    assert_eq!(things[0].memory_count, 2);
    let sentences: Vec<String> = w
        .engine
        .memories(nocturne_contract::MemoryFilter::default())
        .unwrap()
        .into_iter()
        .map(|m| m.sentence)
        .collect();
    assert!(
        sentences.iter().all(|s| s.starts_with("Drinks ")),
        "{sentences:?}"
    );
}

fn said(subject: &str, object: &str, sentence: &str, quote: &str) -> nocturne_engine::Candidate {
    nocturne_engine::Candidate {
        span: Some(nocturne_contract::Span::Quote { text: quote.into() }),
        ..nocturne_engine::Candidate::new(
            subject,
            nocturne_engine::Predicate::Scheduled,
            object,
            sentence,
            0.9,
        )
    }
}

const BOILER: &str = "Boiler service on Monday 30 November. The engineer is called Marek.";
const MAREK: &str = "The engineer is called Marek.";

fn boiler_mind() -> FakeMind {
    let boiler = said(
        "Boiler service",
        "Monday 30 November",
        "The boiler service is on Monday 30 November.",
        "Boiler service on Monday 30 November",
    );
    let marek = said(
        "Marek",
        "the engineer",
        "The boiler engineer is called Marek.",
        MAREK,
    );
    FakeMind::new()
        .with_extract(BOILER, vec![boiler])
        .with_extract(MAREK, vec![marek])
        .with_headline("A quiet night", "One thing kept.")
}

fn extracts(w: &World) -> usize {
    w.mind.calls().iter().filter(|c| *c == "extract").count()
}

fn sentences_of(w: &World, item: &str) -> Vec<String> {
    w.engine
        .memories(nocturne_contract::MemoryFilter {
            item_id: Some(item.to_string()),
            ..Default::default()
        })
        .unwrap()
        .into_iter()
        .map(|m| m.sentence)
        .collect()
}

/// A note read on battery, at the person's request, gets no second look: the first reading
/// stands, and Marek is left out.
async fn read_on_battery(w: &World, text: &str) -> String {
    w.host.set_ac(Some(false));
    let note = keep(w, text);
    w.reader.read_now(Some(&note)).await;
    read_all(w).await;
    w.host.set_ac(Some(true));
    note
}

async fn night(w: &World, when: &str) {
    at(w, when);
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    assert!(finished(&pump(w, 4).await));
    read_all(w).await;
}

/// An item whose memories leave out a name it gives is read again after the page, a missing
/// line at a time with the whole item in view, and quietly: the name is kept, nothing the
/// first reading kept is lost, and the next night has nothing left to read again.
#[tokio::test]
async fn the_tidy_reads_an_item_again_carefully_where_a_name_was_left_out() {
    let w = world_at("2026-11-12T20:00:00Z", boiler_mind());
    let note = read_on_battery(&w, BOILER).await;
    assert_eq!(extracts(&w), 1, "no second look on battery");
    assert_eq!(w.reader.stats().second_looks, 0);
    assert_eq!(sentences_of(&w, &note).len(), 1);

    night(&w, "2026-11-13T03:00:00Z").await;
    let sentences = sentences_of(&w, &note);
    assert_eq!(sentences.len(), 2, "{sentences:?}");
    assert!(sentences.iter().any(|s| s.contains("Marek")));
    assert!(w.engine.rereads(10).unwrap().is_empty());
    assert!(
        !w.events
            .notices()
            .iter()
            .any(|n| n.code == NoticeCode::ReReading),
        "a careful re-read is not announced"
    );
    assert_eq!(
        extracts(&w),
        3,
        "the note, the note again, and the one line"
    );
    let stats = w.reader.stats();
    assert_eq!((stats.readings, stats.second_looks), (2, 1));

    night(&w, "2026-11-14T03:00:00Z").await;
    assert_eq!(extracts(&w), 3, "nothing is left to read again");
}

/// On mains, the first reading already takes a second look at what it left out.
#[tokio::test]
async fn on_mains_the_first_reading_takes_a_second_look() {
    let w = world_at("2026-11-12T20:00:00Z", boiler_mind());
    let note = keep(&w, BOILER);
    read_all(&w).await;
    assert_eq!(sentences_of(&w, &note).len(), 2);
    assert_eq!(extracts(&w), 2);
    assert_eq!(w.reader.stats().second_looks, 1);
}

/// A careful re-read that fails leaves the item as it was, still marked, and is tried again
/// like any re-read.
#[tokio::test]
async fn a_failed_careful_re_read_keeps_the_item_and_its_mark() {
    let w = world_at("2026-11-12T20:00:00Z", boiler_mind());
    let note = read_on_battery(&w, BOILER).await;
    w.mind
        .fail_always("extract", MindError::Failed("no".into()));
    night(&w, "2026-11-13T03:00:00Z").await;
    assert_eq!(sentences_of(&w, &note).len(), 1, "nothing was taken away");
    assert_eq!(
        w.engine.rereads(10).unwrap(),
        std::slice::from_ref(&note),
        "still marked"
    );
    w.mind.clear_failures("extract");
    at(&w, "2026-11-13T03:30:00Z");
    w.reader.retry_now();
    read_all(&w).await;
    assert_eq!(sentences_of(&w, &note).len(), 2);
    assert!(w.engine.rereads(10).unwrap().is_empty());
}

/// An item already marked for a re-read by a new mind is that re-read's: the tidy never marks it
/// for a careful one.
#[tokio::test]
async fn an_item_marked_by_a_new_mind_is_left_to_that_re_read() {
    let w = world_at("2026-11-12T20:00:00Z", boiler_mind());
    let note = read_on_battery(&w, BOILER).await;
    w.engine.queue_reread(std::slice::from_ref(&note)).unwrap();
    night(&w, "2026-11-13T03:00:00Z").await;
    assert!(
        w.events
            .notices()
            .iter()
            .any(|n| n.code == NoticeCode::ReReading),
        "a new mind's re-read is announced as before"
    );
}

/// The tidy asks for at most eight careful re-reads a night.
#[tokio::test]
async fn careful_re_reads_are_capped_at_eight_a_night() {
    let mut mind = boiler_mind();
    let texts: Vec<String> = (1..=10)
        .map(|n| format!("Locker {n} is next to the door. The key is with Marek."))
        .collect();
    for (n, t) in texts.iter().enumerate() {
        let quote = format!("Locker {} is next to the door", n + 1);
        mind = mind.with_extract(
            t,
            vec![said(
                &format!("Locker {}", n + 1),
                "next to the door",
                &format!("Locker {} is next to the door.", n + 1),
                &quote,
            )],
        );
    }
    let w = world_at("2026-11-12T20:00:00Z", mind);
    w.host.set_ac(Some(false));
    let mut notes = Vec::new();
    for t in &texts {
        let note = keep(&w, t);
        w.reader.read_now(Some(&note)).await;
        notes.push(note);
    }
    read_all(&w).await;
    w.host.set_ac(Some(true));
    let before = extracts(&w);
    assert_eq!(before, 10);
    night(&w, "2026-11-13T03:00:00Z").await;
    // Each re-read is the note and one second look at its missing line.
    assert_eq!(extracts(&w) - before, 8 * 2, "eight a night");
}
/// An item whose memories keep every name and number it gives is never read again.
#[tokio::test]
async fn a_well_read_item_is_not_read_again() {
    let (w, _) = world_with_note("2026-11-12T20:00:00Z", "2026-11-13T03:00:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    assert!(finished(&pump(&w, 4).await));
    read_all(&w).await;
    let extracts = w.mind.calls().iter().filter(|c| *c == "extract").count();
    assert_eq!(extracts, 1);
}

/// An item whose memory a later item replaced is history: it is never read again carefully,
/// however much the current memories leave out.
#[tokio::test]
async fn an_item_with_a_replaced_memory_is_not_read_again() {
    let first = "Dentist with Dr Okafor on Wednesday 18 November at 10:00";
    let second = "The dentist moved to Thursday 19 November at 14:30";
    let appointment = |object: &str, on: &str, quote: &str| nocturne_engine::Candidate {
        valid_from: Some(on.parse().unwrap()),
        span: Some(nocturne_contract::Span::Quote { text: quote.into() }),
        ..nocturne_engine::Candidate::new(
            "Dentist appointment",
            nocturne_engine::Predicate::Scheduled,
            object,
            &format!("The dentist appointment is on {object}."),
            0.9,
        )
    };
    let mind = FakeMind::new()
        .with_extract(
            first,
            vec![appointment(
                "Wednesday 18 November at 10:00",
                "2026-11-18",
                first,
            )],
        )
        .with_extract(
            second,
            vec![appointment(
                "Thursday 19 November at 14:30",
                "2026-11-19",
                second,
            )],
        )
        .with_headline("A quiet night", "Two things kept.");
    let w = world_at("2026-11-12T08:05:00Z", mind);
    let note = keep(&w, first);
    read_all(&w).await;
    at(&w, "2026-11-12T12:00:00Z");
    keep(&w, second);
    read_all(&w).await;
    let current = w
        .engine
        .memories(nocturne_contract::MemoryFilter {
            item_id: Some(note.clone()),
            ..Default::default()
        })
        .unwrap();
    assert!(current.is_empty(), "the Wednesday is history: {current:?}");
    assert_eq!(w.engine.get_item(&note).unwrap().memory_count, 1);
    at(&w, "2026-11-13T03:00:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    assert!(finished(&pump(&w, 4).await));
    read_all(&w).await;
    let extracts = w.mind.calls().iter().filter(|c| *c == "extract").count();
    assert_eq!(
        extracts, 2,
        "Dr Okafor is in no current memory, and still not read again"
    );
}

/// An item with a question open is left for the person to settle: it is not read again.
#[tokio::test]
async fn an_item_with_a_question_open_is_not_read_again() {
    let first = "Dentist on Wednesday 18 November at 10:00";
    let second = "Dentist moved to Thursday 19 November at 14:30 with Dr Okafor";
    let appointment = |object: &str, on: &str, quote: &str| nocturne_engine::Candidate {
        valid_from: Some(on.parse().unwrap()),
        span: Some(nocturne_contract::Span::Quote { text: quote.into() }),
        ..nocturne_engine::Candidate::new(
            "Dentist appointment",
            nocturne_engine::Predicate::Scheduled,
            object,
            &format!("The dentist appointment is on {object}."),
            0.9,
        )
    };
    let mind = FakeMind::new()
        .with_extract(
            first,
            vec![appointment(
                "Wednesday 18 November at 10:00",
                "2026-11-18",
                first,
            )],
        )
        .with_extract(
            second,
            vec![appointment(
                "Thursday 19 November at 14:30",
                "2026-11-19",
                "Dentist moved to Thursday 19 November at 14:30",
            )],
        )
        .with_headline("A quiet night", "Two things kept.");
    let w = world_at("2026-11-12T08:05:00Z", mind);
    read_on_battery(&w, first).await;
    at(&w, "2026-11-12T08:07:00Z");
    let note = read_on_battery(&w, second).await;
    assert_eq!(status(&w, &note), ItemStatus::NeedsYou);
    let before = extracts(&w);
    night(&w, "2026-11-13T03:00:00Z").await;
    assert_eq!(extracts(&w), before, "Dr Okafor waits for the person");
}

/// A private item read again carefully keeps what it says private.
#[tokio::test]
async fn a_private_item_read_again_stays_private() {
    let w = world_at("2026-11-12T20:00:00Z", boiler_mind());
    let note = read_on_battery(&w, BOILER).await;
    w.engine.set_item_private(&note, true).unwrap();
    night(&w, "2026-11-13T03:00:00Z").await;
    let memories = w
        .engine
        .memories(nocturne_contract::MemoryFilter {
            item_id: Some(note.clone()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(memories.len(), 2, "read again");
    assert!(memories.iter().all(|m| m.private), "{memories:?}");
}

// ---- the journal ----

#[tokio::test]
async fn the_night_tidy_clears_every_entry_hold_so_the_night_reads_held_blocks() {
    let w = world_at("2026-11-12T20:00:00Z", mind());
    keep(&w, TEXT);
    // The person was writing until a few minutes ago: the block is still held for ten minutes.
    w.clock.set_str("2026-11-13T02:55:00Z");
    let block = w
        .engine
        .save_entry_text(date("2026-11-12"), None, 0, "Walked home by the canal")
        .unwrap()
        .unwrap();
    w.clock.set_str("2026-11-13T03:00:00Z");
    read_all(&w).await;
    assert_eq!(
        status(&w, &block.item.id),
        ItemStatus::Kept,
        "held: the reader leaves it"
    );

    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    read_all(&w).await;
    assert_eq!(status(&w, &block.item.id), ItemStatus::Understood);
}

// ---- decisions in shadow ----

/// Four appointments, each kept twice on different days: the second of each is logged as a move
/// the rule did not see.
const NOTES: [(&str, &str, &str, &str); 8] = [
    (
        "Dentist on Monday 16 November at 10:00",
        "Dentist",
        "Monday 16 November at 10:00",
        "2026-11-16",
    ),
    (
        "Dentist on Tuesday 24 November at 09:00",
        "Dentist",
        "Tuesday 24 November at 09:00",
        "2026-11-24",
    ),
    (
        "Optician on Monday 16 November at 11:00",
        "Optician",
        "Monday 16 November at 11:00",
        "2026-11-16",
    ),
    (
        "Optician on Tuesday 24 November at 12:00",
        "Optician",
        "Tuesday 24 November at 12:00",
        "2026-11-24",
    ),
    (
        "Physio on Wednesday 18 November at 15:00",
        "Physio",
        "Wednesday 18 November at 15:00",
        "2026-11-18",
    ),
    (
        "Physio on Thursday 26 November at 16:00",
        "Physio",
        "Thursday 26 November at 16:00",
        "2026-11-26",
    ),
    (
        "Vet on Wednesday 18 November at 17:00",
        "Vet",
        "Wednesday 18 November at 17:00",
        "2026-11-18",
    ),
    (
        "Vet on Thursday 26 November at 08:00",
        "Vet",
        "Thursday 26 November at 08:00",
        "2026-11-26",
    ),
];

fn appointment(subject: &str, object: &str, due: &str) -> nocturne_engine::Candidate {
    let mut c = nocturne_engine::Candidate::new(
        &format!("{subject} appointment"),
        nocturne_engine::Predicate::Scheduled,
        object,
        &format!("The {subject} appointment is on {object}."),
        0.9,
    );
    c.due = Some(date(due));
    c
}

/// Eight notes kept in the evening, each read into its own memory, and the clock at 03:00.
fn world_of_notes(mind: FakeMind) -> World {
    let mut mind = mind.with_headline("A quiet night", "Eight things kept.");
    for (text, subject, object, due) in NOTES {
        mind = mind.with_extract(text, vec![appointment(subject, object, due)]);
    }
    let w = world_at("2026-11-12T20:00:00Z", mind);
    for (text, ..) in NOTES {
        keep(&w, text);
    }
    at(&w, "2026-11-13T03:00:00Z");
    w
}

#[tokio::test]
async fn the_night_tidy_asks_the_mind_in_shadow_once_the_page_is_saved() {
    let w = world_of_notes(FakeMind::new().with_decision("Kept appointment", "same"));
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    let steps = pump(&w, 4).await;
    assert!(finished(&steps), "{steps:?}");
    let log = w.engine.decision_log().unwrap();
    assert_eq!(log.len(), 4, "{log:?}");
    for row in &log {
        assert!(!row.rule, "the rule saw no move");
        assert_eq!(row.label.as_deref(), Some("same"));
        assert_eq!(row.verdict, Some(false));
        assert!(row.asked_at.is_some());
        assert!(row.error.is_none());
    }
    assert_eq!(w.mind.call_count("decide"), 4);
    let page = w.engine.page(Some(date("2026-11-13"))).unwrap().unwrap();
    assert_eq!(page.headline, "A quiet night");
}

#[tokio::test]
async fn a_failing_decider_stops_for_the_night_after_three_and_the_page_stands() {
    let w = world_of_notes(FakeMind::new());
    w.mind
        .fail_always("decide", MindError::Failed("no answer".into()));
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    let steps = pump(&w, 4).await;
    assert!(finished(&steps), "{steps:?}");
    assert_eq!(
        w.mind.call_count("decide"),
        3,
        "three failures end the night's asking"
    );
    let log = w.engine.decision_log().unwrap();
    let failed: Vec<_> = log.iter().filter(|r| r.error.is_some()).collect();
    assert_eq!(failed.len(), 3, "{log:?}");
    assert!(failed.iter().all(|r| r.verdict.is_none()));
    assert_eq!(
        w.engine.pending_decisions(10).unwrap().len(),
        1,
        "one waits for the next night"
    );
    assert!(w.engine.page(Some(date("2026-11-13"))).unwrap().is_some());
    for (text, ..) in NOTES {
        let id = w
            .engine
            .items(nocturne_contract::ItemFilter::default())
            .unwrap()
            .into_iter()
            .find(|i| i.text.as_deref() == Some(text))
            .unwrap()
            .id;
        assert_eq!(
            status(&w, &id),
            ItemStatus::Understood,
            "reading is never skipped"
        );
    }
}

#[tokio::test]
async fn a_catch_up_tidy_asks_no_decider() {
    let w = world_three_days_off();
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::CatchUp)
    );
    let steps = pump(&w, 4).await;
    assert!(finished(&steps), "{steps:?}");
    assert_eq!(w.mind.call_count("decide"), 0);
}

#[tokio::test]
async fn shadow_asking_waits_for_another_night_when_the_person_is_back() {
    let w = world_of_notes(FakeMind::new().with_decision("Kept appointment", "same"));
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    w.host.set_idle(Some(Duration::from_secs(30)));
    let steps = pump(&w, 4).await;
    assert!(finished(&steps), "{steps:?}");
    assert_eq!(w.mind.call_count("decide"), 0, "someone is at the keyboard");
    assert_eq!(w.engine.pending_decisions(10).unwrap().len(), 4);
}

#[tokio::test]
async fn shadow_asking_stops_short_of_the_page_time() {
    let w = world_of_notes(FakeMind::new().with_decision("Kept appointment", "same"));
    at(&w, "2026-11-13T06:57:00Z");
    assert_eq!(
        w.scheduler.tick().await,
        SchedStep::Started(TidyKind::Night)
    );
    let steps = pump(&w, 4).await;
    assert!(finished(&steps), "{steps:?}");
    assert_eq!(
        w.mind.call_count("decide"),
        0,
        "the person may open the page soon"
    );
}
