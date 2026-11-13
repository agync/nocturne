//! The reader, the tidy and the scheduler wired into the app, driven through the mock runtime
//! with a scripted mind and a host the test steers.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use nocturne_contract::{EVENT_ITEM, EngineState, ItemStatus, NoticeCode, PageVariant};
use nocturne_engine::{Candidate, FakeMind, Mind, Predicate};
use nocturne_intelligence::{FakeHost, SchedStep, TidyStep};
use serde_json::{Value, json};
use tauri::async_runtime::block_on;

const TEXT: &str = "Gran lives in Leeds";

fn gran() -> Candidate {
    Candidate::new(
        "Gran",
        Predicate::LivesIn,
        "Leeds",
        "Gran lives in Leeds.",
        0.8,
    )
}

fn mind() -> Arc<FakeMind> {
    Arc::new(
        FakeMind::new()
            .with_extract(TEXT, vec![gran()])
            .with_headline("A quiet night", "One thing kept."),
    )
}

fn world(mind: &Arc<FakeMind>, now: &str) -> Harness {
    let h = harness_with_mind(mind.clone() as Arc<dyn Mind>);
    h.clock.set_str(now);
    h
}

fn keep(h: &Harness) -> String {
    id_of(&h.ok("keep_text", json!({"text": TEXT, "source": "paste"})))
}

fn status_of(h: &Harness, id: &str) -> ItemStatus {
    h.engine().get_item(id).unwrap().status
}

fn status(h: &Harness) -> Value {
    h.ok("get_status", json!({}))
}

#[test]
fn a_kept_note_is_read_in_the_background_and_the_interface_is_told() {
    let h = world(&mind(), "2026-11-13T12:00:00Z");
    let items = h.events(EVENT_ITEM);
    let _services = start_runtime(&h, FakeHost::new());
    let id = keep(&h);
    wait_for("the note to be understood", || {
        status_of(&h, &id) == ItemStatus::Understood
    });
    wait_for("the interface to hear it", || {
        items
            .lock()
            .unwrap()
            .iter()
            .any(|i| i["id"] == id.as_str() && i["status"] == "understood")
    });
    wait_for("the status to settle", || {
        let s = status(&h);
        s["waitingToRead"] == 0 && s["state"] == "resting"
    });
}

#[test]
fn on_battery_a_note_waits_for_the_tidy_until_read_now() {
    let h = world(&mind(), "2026-11-13T12:00:00Z");
    let host = FakeHost::new();
    host.set_ac(Some(false));
    let services = start_runtime(&h, host);
    let id = keep(&h);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(status_of(&h, &id), ItemStatus::Kept, "waits for the tidy");
    assert_eq!(status(&h)["waitingToRead"], 1);

    h.ok("read_now", json!({"id": id}));
    wait_for("the note to be read now", || {
        status_of(&h, &id) == ItemStatus::Understood
    });
    wait_for("the reader to count it", || {
        services.reader.stats().items_read == 1
    });
}

#[test]
fn tidy_anyway_sets_the_battery_rule_aside() {
    let h = world(&mind(), "2026-11-12T20:00:00Z");
    let id = keep(&h);
    let host = FakeHost::new();
    host.set_ac(Some(false));
    host.set_percent(Some(20));
    let services = start_runtime(&h, host);
    h.clock.set_str("2026-11-13T03:00:00Z");

    assert_eq!(
        block_on(services.scheduler.tick()),
        SchedStep::WaitingForPower
    );
    wait_for("the battery notice", || {
        status(&h)["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["code"] == "tidyPausedBattery")
    });

    h.ok("tidy_anyway", json!({}));
    let mut finished = false;
    for _ in 0..40 {
        if matches!(
            block_on(services.scheduler.tick()),
            SchedStep::Tidy(TidyStep::Finished(_))
        ) {
            finished = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(finished, "the tidy ran although the battery is low");
    assert_eq!(status_of(&h, &id), ItemStatus::Understood);
    assert_eq!(
        h.ok("get_page", json!({"date": "2026-11-13"}))["headline"],
        "A quiet night"
    );
}

#[test]
fn a_page_being_written_says_so_until_it_is_saved() {
    let m = mind();
    let gate = m.gate("extract");
    let h = world(&m, "2026-11-12T20:00:00Z");
    let host = FakeHost::new();
    host.set_ac(Some(false));
    let id = keep(&h);
    let services = start_runtime(&h, host.clone());
    h.clock.set_str("2026-11-13T03:00:00Z");
    host.set_ac(Some(true));

    assert!(matches!(
        block_on(services.scheduler.tick()),
        SchedStep::Started(_)
    ));
    block_on(async { tokio::time::timeout(Duration::from_secs(10), gate.entered()).await })
        .expect("the tidy reads the note");

    let page = h.ok("get_page", json!({}));
    assert_eq!(
        page["variant"],
        serde_json::to_value(PageVariant::BeingWritten).unwrap()
    );
    assert!(page["pending"].as_u64().unwrap() >= 1, "{page}");
    assert_eq!(
        h.ok("get_status", json!({}))["tidy"]["total"],
        1,
        "the status carries the progress"
    );
    assert_eq!(
        status(&h)["state"],
        serde_json::to_value(EngineState::Tidying).unwrap()
    );

    gate.open();
    let mut finished = false;
    for _ in 0..100 {
        if matches!(
            block_on(services.scheduler.tick()),
            SchedStep::Tidy(TidyStep::Finished(_))
        ) {
            finished = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(finished);
    assert_eq!(status_of(&h, &id), ItemStatus::Understood);
    let page = h.ok("get_page", json!({}));
    assert_ne!(page["variant"], "beingWritten");
    assert_eq!(page["headline"], "A quiet night");
    wait_for("the tidy to end in the status", || {
        status(&h)["tidy"].is_null()
    });
}

#[test]
fn pausing_stops_the_scheduler_and_the_reader_and_resuming_restarts_them() {
    let h = world(&mind(), "2026-11-13T12:00:00Z");
    let services = start_runtime(&h, FakeHost::new());
    h.ok("pause", json!({"until": "2026-11-13T18:00:00Z"}));
    assert_eq!(block_on(services.scheduler.tick()), SchedStep::Paused);

    let id = keep(&h);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        status_of(&h, &id),
        ItemStatus::Kept,
        "nothing is read while paused"
    );
    assert_eq!(status(&h)["state"], "paused");

    h.ok("resume", json!({}));
    wait_for("the note to be read after resuming", || {
        status_of(&h, &id) == ItemStatus::Understood
    });
    assert_ne!(block_on(services.scheduler.tick()), SchedStep::Paused);
}

#[test]
fn a_question_holds_the_reader_back_for_its_whole_answer() {
    let slow = TestMind::new(AnswerMode::Slow(Duration::from_millis(400)));
    let h = harness_with_mind(slow);
    h.clock.set_str("2026-11-13T12:00:00Z");
    h.learn(
        "dentist note",
        "The dentist is on Thursday.",
        "Dentist",
        "Thursday",
    );
    let services = start_runtime(&h, FakeHost::new());
    let seen = h.ask_deltas();
    let id = h
        .ok("ask", json!({"question": "When is the dentist?"}))
        .as_str()
        .unwrap()
        .to_string();
    wait_for("the ask to begin", || services.reader.is_asking());
    wait_for("the answer", || {
        deltas_for(&seen, &id).iter().any(|d| is_terminal(d.phase))
    });
    wait_for("the reader to be free again", || {
        !services.reader.is_asking()
    });
}

#[test]
fn the_status_counts_what_waits_without_a_cap() {
    let h = harness();
    let e = h.engine();
    for n in 0..1_005 {
        e.keep_text(&format!("note {n}"), nocturne_contract::Source::Paste)
            .unwrap();
    }
    assert_eq!(status(&h)["waitingToRead"], 1_005);
}

#[test]
fn quitting_stops_the_reader_and_leaves_what_is_unread_in_the_queue() {
    let h = world(&mind(), "2026-11-13T12:00:00Z");
    let host = FakeHost::new();
    host.set_ac(Some(false));
    let services = start_runtime(&h, host);
    let id = keep(&h);

    let started = std::time::Instant::now();
    h.state().run_exit_hooks();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(services.token().is_cancelled());
    assert_eq!(status_of(&h, &id), ItemStatus::Kept);
}

#[test]
fn a_notice_the_scheduler_raises_reaches_the_status_and_clears_with_it() {
    let h = world(&mind(), "2026-11-12T20:00:00Z");
    keep(&h);
    let host = FakeHost::new();
    host.set_ac(Some(false));
    host.set_percent(Some(20));
    let services = start_runtime(&h, host.clone());
    h.clock.set_str("2026-11-13T03:00:00Z");
    block_on(services.scheduler.tick());
    wait_for("the notice", || {
        has_notice(&status(&h), NoticeCode::TidyPausedBattery)
    });
    // Back on mains: the notice goes.
    host.set_ac(Some(true));
    h.clock.set_str("2026-11-13T12:00:00Z");
    block_on(services.scheduler.tick());
    wait_for("the notice to clear", || {
        !has_notice(&status(&h), NoticeCode::TidyPausedBattery)
    });
}

fn has_notice(status: &Value, code: NoticeCode) -> bool {
    let want = serde_json::to_value(code).unwrap();
    status["notices"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["code"] == want)
}

/// Quitting with a note half read gives it back, and the reading is not counted against it.
#[test]
fn quitting_in_the_middle_of_a_note_gives_it_back_uncounted() {
    let m = mind();
    let gate = m.gate("extract");
    let h = world(&m, "2026-11-13T12:00:00Z");
    let services = start_runtime(&h, FakeHost::new());
    let id = keep(&h);
    block_on(async { tokio::time::timeout(Duration::from_secs(10), gate.entered()).await })
        .expect("the note is being read");
    assert_eq!(status_of(&h, &id), ItemStatus::Reading);

    h.state().run_exit_hooks();
    assert!(services.token().is_cancelled());
    assert_eq!(status_of(&h, &id), ItemStatus::Kept);
    let leased = h
        .engine()
        .lease_item(&id, Duration::from_secs(60))
        .expect("the lease was let go");
    assert_eq!(leased.attempts, 1, "the half read was not counted");
}
