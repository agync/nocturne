//! Looking back on the journal: which computers write reflections, what the journal shows for a
//! day or a period, writing on request and stopping it, the overnight run wired into the app, and
//! the journal check run on a scripted mind (the wiring and the scoring, not a size).

mod common;
mod eval;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use eval::journal::*;
use jiff::civil::Date;
use nocturne_app_lib::journal::{WRITES_REFLECTIONS, writes_now, writes_reflections};
use nocturne_contract::{
    CheckInInput, Feeling, FeelingPoint, MemoryFilter, Period, PeriodKind, ReflectionView, Source,
    Tier,
};
use nocturne_engine::{FakeMind, Mind, MindError, WrittenSentence};
use nocturne_intelligence::{FakeHost, SchedStep, TidyStep};
use serde_json::{Value, json};
use tauri::Manager;
use tauri::async_runtime::block_on;

const MORNING: &str = "2026-11-13T09:00:00Z";

fn period(h: &Harness, kind: PeriodKind, day: &str) -> Period {
    h.engine().period_of(kind, day.parse().unwrap()).unwrap()
}

fn arg(p: &Period) -> Value {
    json!({ "period": p })
}

fn world(mind: FakeMind) -> (Harness, Arc<FakeMind>) {
    let mind = Arc::new(mind);
    let h = harness_with_mind(mind.clone() as Arc<dyn Mind>);
    h.clock.set_str(MORNING);
    (h, mind)
}

fn sentence(text: &str, cites: &[usize]) -> WrittenSentence {
    WrittenSentence {
        text: text.into(),
        cites: cites.to_vec(),
    }
}

/// Read everything still waiting, with nothing found in it: a reflection is written from what
/// was read.
fn read_all(h: &Harness) {
    let e = h.engine();
    let waiting = e
        .items(nocturne_contract::ItemFilter {
            since: None,
            status: Some(nocturne_contract::ItemStatus::Kept),
            limit: None,
        })
        .unwrap();
    for item in waiting {
        if let Ok(p) = e.lease_item(&item.id, Duration::from_secs(60)) {
            let _ = e.apply_candidates(&item.id, &p.lease, vec![]);
        }
    }
}

/// Two notes kept on the 12th, then the morning after.
fn keep_a_day(h: &Harness) -> Vec<String> {
    let e = h.engine();
    let mut ids = Vec::new();
    for (at, text) in [
        (
            "2026-11-12T08:10:00Z",
            "Dropped Leo at school, then a walk by the canal",
        ),
        ("2026-11-12T18:20:00Z", "Made soup and watched the match"),
    ] {
        h.clock.set_str(at);
        match e.keep_note(text, Source::Journal, None, false).unwrap() {
            nocturne_contract::Kept::New { item } | nocturne_contract::Kept::Duplicate { item } => {
                ids.push(item.id)
            }
        }
    }
    read_all(h);
    h.clock.set_str(MORNING);
    ids
}

fn kind_of(view: &Value) -> &str {
    view["type"].as_str().unwrap()
}

fn reason_of(view: &Value) -> &str {
    view["reason"].as_str().unwrap()
}

// ---- the gate ----

#[test]
fn only_a_size_that_passed_with_its_own_model_writes_reflections() {
    for tier in [
        Tier::Large,
        Tier::Standard,
        Tier::Small,
        Tier::Tiny,
        Tier::Unsupported,
    ] {
        assert_eq!(
            writes_reflections(tier, false),
            WRITES_REFLECTIONS.contains(&tier),
            "{tier:?}"
        );
        assert!(
            !writes_reflections(tier, true),
            "a model the person chose has not passed the check: {tier:?}"
        );
    }
}

#[test]
fn no_size_has_passed_the_journal_check_yet_and_the_override_decides_for_tests() {
    let h = harness();
    assert!(
        !writes_now(&h.state()),
        "the preset of this computer writes nothing yet"
    );
    h.state().reflecting.set_writes(Some(true));
    assert!(writes_now(&h.state()));
    h.state().reflecting.set_writes(Some(false));
    assert!(!writes_now(&h.state()));
    h.state().reflecting.set_writes(None);
    assert!(!writes_now(&h.state()));
}

// ---- what the journal shows ----

#[test]
fn an_empty_period_has_nothing_to_look_back_on_whatever_else_is_so() {
    let (h, _) = world(FakeMind::new());
    let day = period(&h, PeriodKind::Day, "2026-11-12");
    for open in [None, Some(true)] {
        h.state().reflecting.set_writes(open);
        let v = h.ok("get_reflection", arg(&day));
        assert_eq!(kind_of(&v), "nothing", "{v}");
        assert_eq!(v["period"]["start"], "2026-11-12");
        let v = h.ok("reflect", arg(&day));
        assert_eq!(kind_of(&v), "nothing", "{v}");
    }
    let week = period(&h, PeriodKind::Week, "2026-11-12");
    assert_eq!(kind_of(&h.ok("get_reflection", arg(&week))), "nothing");
}

#[test]
fn without_a_mind_or_while_it_starts_the_day_is_listed_as_not_ready() {
    let h = harness();
    keep_a_day(&h);
    h.state().reflecting.set_writes(Some(true));
    let day = period(&h, PeriodKind::Day, "2026-11-12");
    let v = h.ok("get_reflection", arg(&day));
    assert_eq!(kind_of(&v), "glance", "{v}");
    assert_eq!(reason_of(&v), "notReady");
    assert!(
        !v["glance"]["kept"].as_array().unwrap().is_empty(),
        "the glance still lists what was kept"
    );

    let (h, mind) = world(FakeMind::new());
    keep_a_day(&h);
    h.state().reflecting.set_writes(Some(true));
    h.state().mind.set_loading(true);
    let v = h.ok("reflect", arg(&period(&h, PeriodKind::Day, "2026-11-12")));
    assert_eq!(reason_of(&v), "notReady", "{v}");
    assert_eq!(mind.call_count("reflect"), 0);
}

#[test]
fn a_size_that_has_not_passed_lists_the_day_and_asks_the_mind_nothing() {
    let (h, mind) = world(FakeMind::new());
    keep_a_day(&h);
    let day = period(&h, PeriodKind::Day, "2026-11-12");
    let v = h.ok("get_reflection", arg(&day));
    assert_eq!(
        (kind_of(&v), reason_of(&v)),
        ("glance", "notOnThisComputer")
    );
    let v = h.ok("reflect", arg(&day));
    assert_eq!(
        (kind_of(&v), reason_of(&v)),
        ("glance", "notOnThisComputer")
    );
    assert_eq!(mind.call_count("reflect"), 0);
    assert!(h.engine().saved_reflection(&day.clone()).unwrap().is_none());
}

#[test]
fn a_day_of_only_things_kept_from_elsewhere_is_listed_not_written() {
    let (h, mind) = world(FakeMind::new());
    h.state().reflecting.set_writes(Some(true));
    h.clock.set_str("2026-11-12T10:00:00Z");
    h.engine()
        .keep_text("Screenshot of the school letter", Source::Screenshot)
        .unwrap();
    h.clock.set_str(MORNING);
    let day = period(&h, PeriodKind::Day, "2026-11-12");
    for cmd in ["get_reflection", "reflect"] {
        let v = h.ok(cmd, arg(&day));
        assert_eq!((kind_of(&v), reason_of(&v)), ("glance", "listed"), "{cmd}");
    }
    assert_eq!(mind.call_count("reflect"), 0);
}

#[test]
fn a_day_with_something_of_the_persons_own_can_be_asked_for() {
    let (h, _) = world(FakeMind::new());
    keep_a_day(&h);
    h.state().reflecting.set_writes(Some(true));
    let day = period(&h, PeriodKind::Day, "2026-11-12");
    let v = h.ok("get_reflection", arg(&day));
    assert_eq!(kind_of(&v), "notYet", "{v}");
    assert!(!v["glance"]["kept"].as_array().unwrap().is_empty());
}

// ---- writing on request ----

#[test]
fn reflect_writes_saves_and_returns_the_reflection_with_a_receipt_for_each_line() {
    let (h, mind) = world(FakeMind::new().with_reflection(vec![
        sentence("You took Leo to school and **walked by the canal**.", &[0]),
        sentence("In the evening you **made soup**.", &[1]),
    ]));
    let ids = keep_a_day(&h);
    h.state().reflecting.set_writes(Some(true));
    let day = period(&h, PeriodKind::Day, "2026-11-12");

    let v = h.ok("reflect", arg(&day));
    assert_eq!(kind_of(&v), "written", "{v}");
    let lines = v["reflection"]["theDay"].as_array().unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["receipts"][0]["itemId"], ids[0]);
    assert_eq!(lines[1]["receipts"][0]["itemId"], ids[1]);
    assert_eq!(v["reflection"]["written"], "onDemand");
    assert_eq!(v["reflection"]["soFar"], false);
    assert_eq!(mind.call_count("reflect"), 1);

    let again = h.ok("get_reflection", arg(&day));
    assert_eq!(kind_of(&again), "written");
    assert_eq!(again["reflection"]["id"], v["reflection"]["id"]);
}

#[test]
fn a_day_still_going_is_written_as_so_far_and_a_week_comes_back_as_a_recap() {
    let (h, _) = world(FakeMind::new());
    h.state().reflecting.set_writes(Some(true));
    h.clock.set_str("2026-11-13T08:00:00Z");
    h.engine()
        .keep_note("Porridge and the school run", Source::Journal, None, false)
        .unwrap();
    read_all(&h);
    h.clock.set_str("2026-11-13T09:00:00Z");
    let today = period(&h, PeriodKind::Day, "2026-11-13");
    let v = h.ok("reflect", arg(&today));
    assert_eq!(kind_of(&v), "written", "{v}");
    assert_eq!(v["reflection"]["soFar"], true);

    let week = period(&h, PeriodKind::Week, "2026-11-13");
    let v = h.ok("reflect", arg(&week));
    assert_eq!(kind_of(&v), "recap", "{v}");
    assert_eq!(v["recap"]["soFar"], true);
    assert_eq!(v["recap"]["written"], "onDemand");
}

#[test]
fn a_day_the_mind_could_not_cite_comes_back_listed() {
    let (h, _) = world(FakeMind::new().with_reflection(vec![sentence("A lovely day.", &[])]));
    keep_a_day(&h);
    h.state().reflecting.set_writes(Some(true));
    let day = period(&h, PeriodKind::Day, "2026-11-12");
    let v = h.ok("reflect", arg(&day));
    assert_eq!((kind_of(&v), reason_of(&v)), ("glance", "listed"), "{v}");
    assert!(h.engine().saved_reflection(&day).unwrap().is_none());
}

#[test]
fn a_mind_that_fails_is_an_error_the_interface_can_retry() {
    let (h, mind) = world(FakeMind::new());
    keep_a_day(&h);
    h.state().reflecting.set_writes(Some(true));
    mind.fail_next("reflect", MindError::Failed("model".into()));
    let day = period(&h, PeriodKind::Day, "2026-11-12");
    assert_eq!(h.code("reflect", arg(&day)), "internal");
    assert_eq!(kind_of(&h.ok("get_reflection", arg(&day))), "notYet");
    assert_eq!(kind_of(&h.ok("reflect", arg(&day))), "written");
}

#[test]
fn reflect_holds_off_reading_like_an_answer_and_runs_on_battery() {
    let (h, mind) = world(FakeMind::new());
    keep_a_day(&h);
    h.state().reflecting.set_writes(Some(true));
    let host = FakeHost::new();
    host.on_battery(12);
    let services = start_manual(&h, host);
    let gate = mind.gate("reflect");
    let day = period(&h, PeriodKind::Day, "2026-11-12");

    let app = h.app.handle().clone();
    let p = day.clone();
    let call = tauri::async_runtime::spawn(async move {
        nocturne_app_lib::journal::reflect(&app.state(), &p).await
    });
    block_on(async { tokio::time::timeout(Duration::from_secs(10), gate.entered()).await })
        .expect("the mind is asked");
    assert!(services.reader.is_asking(), "the reader waits meanwhile");
    assert_eq!(kind_of(&h.ok("get_reflection", arg(&day))), "writing");
    gate.open();
    let view = block_on(call).unwrap().unwrap();
    assert!(matches!(view, ReflectionView::Written { .. }), "{view:?}");
    assert!(!services.reader.is_asking());
}

#[test]
fn stopping_cancels_the_write_and_stores_nothing() {
    let (h, mind) = world(FakeMind::new());
    keep_a_day(&h);
    h.state().reflecting.set_writes(Some(true));
    let gate = mind.gate("reflect");
    let day = period(&h, PeriodKind::Day, "2026-11-12");
    let app = h.app.handle().clone();
    let p = day.clone();
    let call = tauri::async_runtime::spawn(async move {
        nocturne_app_lib::journal::reflect(&app.state(), &p).await
    });
    block_on(async { tokio::time::timeout(Duration::from_secs(10), gate.entered()).await })
        .expect("the mind is asked");
    assert!(h.state().reflecting.is_writing(&day));

    h.ok("stop_reflecting", arg(&day));
    let view = block_on(call).unwrap().unwrap();
    assert!(matches!(view, ReflectionView::NotYet { .. }), "{view:?}");
    assert!(!h.state().reflecting.is_writing(&day));
    assert!(h.engine().saved_reflection(&day).unwrap().is_none());
    gate.open();
    assert_eq!(kind_of(&h.ok("get_reflection", arg(&day))), "notYet");
    // Stopping what is not being written is not an error.
    h.ok("stop_reflecting", arg(&day));
}

#[test]
fn a_write_cancelled_from_outside_does_not_leave_the_period_writing() {
    let (h, mind) = world(FakeMind::new());
    keep_a_day(&h);
    h.state().reflecting.set_writes(Some(true));
    let gate = mind.gate("reflect");
    let day = period(&h, PeriodKind::Day, "2026-11-12");
    let app = h.app.handle().clone();
    let p = day.clone();
    let call = tauri::async_runtime::spawn(async move {
        nocturne_app_lib::journal::reflect(&app.state(), &p).await
    });
    block_on(async { tokio::time::timeout(Duration::from_secs(10), gate.entered()).await })
        .expect("the mind is asked");
    assert!(h.state().reflecting.is_writing(&day));

    call.abort();
    let _ = block_on(call);
    assert!(!h.state().reflecting.is_writing(&day));
    assert_eq!(kind_of(&h.ok("get_reflection", arg(&day))), "notYet");
}

#[test]
fn deleting_during_a_write_leaves_nothing_stored() {
    let (h, mind) = world(FakeMind::new());
    keep_a_day(&h);
    h.state().reflecting.set_writes(Some(true));
    let day = period(&h, PeriodKind::Day, "2026-11-12");
    h.ok("reflect", arg(&day));
    assert!(h.engine().saved_reflection(&day).unwrap().is_some());

    let gate = mind.gate("reflect");
    let app = h.app.handle().clone();
    let p = day.clone();
    let call = tauri::async_runtime::spawn(async move {
        nocturne_app_lib::journal::reflect(&app.state(), &p).await
    });
    block_on(async { tokio::time::timeout(Duration::from_secs(10), gate.entered()).await })
        .expect("the mind is asked");
    h.ok("delete_reflection", arg(&day));
    gate.open();
    let _ = block_on(call);
    assert!(h.engine().saved_reflection(&day).unwrap().is_none());
}

#[test]
fn a_write_for_a_period_already_being_written_is_not_started_twice() {
    let (h, mind) = world(FakeMind::new());
    keep_a_day(&h);
    h.state().reflecting.set_writes(Some(true));
    let gate = mind.gate("reflect");
    let day = period(&h, PeriodKind::Day, "2026-11-12");
    let app = h.app.handle().clone();
    let p = day.clone();
    let first = tauri::async_runtime::spawn(async move {
        nocturne_app_lib::journal::reflect(&app.state(), &p).await
    });
    block_on(async { tokio::time::timeout(Duration::from_secs(10), gate.entered()).await })
        .expect("the mind is asked");
    assert_eq!(kind_of(&h.ok("reflect", arg(&day))), "writing");
    gate.open();
    block_on(first).unwrap().unwrap();
    assert_eq!(mind.call_count("reflect"), 1);
}

#[test]
fn deleting_a_reflection_removes_the_writing_and_never_an_entry() {
    let (h, _) = world(FakeMind::new());
    let ids = keep_a_day(&h);
    h.state().reflecting.set_writes(Some(true));
    let day = period(&h, PeriodKind::Day, "2026-11-12");
    h.ok("reflect", arg(&day));
    assert!(h.engine().saved_reflection(&day).unwrap().is_some());

    h.ok("delete_reflection", arg(&day));
    assert!(h.engine().saved_reflection(&day).unwrap().is_none());
    for id in ids {
        assert!(h.engine().get_item(&id).is_ok(), "the entry is untouched");
    }
    assert_eq!(kind_of(&h.ok("get_reflection", arg(&day))), "notYet");
    // Writing it again is allowed.
    assert_eq!(kind_of(&h.ok("reflect", arg(&day))), "written");
}

#[test]
fn looking_back_is_not_use() {
    let (h, _) = world(FakeMind::new());
    h.state().reflecting.set_writes(Some(true));
    h.clock.set_str("2026-11-12T09:00:00Z");
    let (item, _) = h.learn(
        "Gran lives in Leeds",
        "Gran lives in Leeds.",
        "Gran",
        "Leeds",
    );
    h.clock.set_str(MORNING);
    let all = || {
        h.engine()
            .memories(MemoryFilter::default())
            .unwrap()
            .into_iter()
            .map(|m| (m.id, m.last_used))
            .collect::<Vec<_>>()
    };
    let before = all();
    assert_eq!(before.len(), 1);
    for kind in [PeriodKind::Day, PeriodKind::Week, PeriodKind::Month] {
        let p = period(&h, kind, "2026-11-12");
        h.ok("get_reflection", arg(&p));
        h.ok("reflect", arg(&p));
        h.ok("get_reflection", arg(&p));
    }
    assert!(
        h.engine()
            .saved_reflection(&period(&h, PeriodKind::Day, "2026-11-12"))
            .unwrap()
            .is_some()
    );
    assert_eq!(all(), before, "looking back is not use");
    assert!(h.engine().get_item(&item).is_ok());
}

// ---- overnight, wired into the app ----

fn seed_the_12th(h: &Harness) {
    h.clock.set_str("2026-11-12T20:00:00Z");
    h.engine()
        .keep_note("Soup and the match", Source::Journal, None, false)
        .unwrap();
}

fn run_until_page(h: &Harness, services: &nocturne_app_lib::runtime::Services) -> bool {
    let _ = h;
    for _ in 0..80 {
        if matches!(
            block_on(services.scheduler.tick()),
            SchedStep::Tidy(TidyStep::Finished(_))
        ) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn the_night_writes_the_day_before_the_page_when_this_computer_may() {
    let (h, _) = world(FakeMind::new().with_headline("A quiet night", "One thing."));
    h.state().reflecting.set_writes(Some(true));
    seed_the_12th(&h);
    let services = start_manual(&h, FakeHost::new());
    h.clock.set_str("2026-11-13T05:00:00Z");
    assert!(run_until_page(&h, &services));
    let day = period(&h, PeriodKind::Day, "2026-11-12");
    let Some(ReflectionView::Written { reflection }) = h.engine().saved_reflection(&day).unwrap()
    else {
        panic!("the night wrote it")
    };
    assert_eq!(
        reflection.written,
        nocturne_contract::WrittenWhen::Overnight
    );
}

#[test]
fn the_night_writes_nothing_while_this_computer_has_not_passed_the_check() {
    let (h, mind) = world(FakeMind::new().with_headline("A quiet night", "One thing."));
    seed_the_12th(&h);
    let services = start_manual(&h, FakeHost::new());
    h.clock.set_str("2026-11-13T05:00:00Z");
    assert!(run_until_page(&h, &services));
    assert_eq!(mind.call_count("reflect"), 0);
    let day = period(&h, PeriodKind::Day, "2026-11-12");
    assert!(h.engine().saved_reflection(&day).unwrap().is_none());
}

// ---- the journal check ----

/// Seed a day as the engine would keep it, and return the evidence the check judges against.
fn seed(h: &Harness, d: &SeedDay) {
    let e = h.engine();
    let day: Date = d.day.parse().unwrap();
    for (n, k) in d.kept.iter().enumerate() {
        h.clock.set_str(&format!("{}T{}:00Z", d.day, k.at));
        match k.kind {
            Kind::Entry => {
                e.save_entry_text(day, None, n as u32, k.text).unwrap();
            }
            Kind::Note => {
                e.keep_note(k.text, Source::Journal, None, false).unwrap();
            }
            Kind::Good => {
                e.keep_good_thing(day, k.text, None, Source::Journal)
                    .unwrap();
            }
        }
    }
    for c in d.checkins {
        h.clock.set_str(&format!("{}T{}:00Z", d.day, c.at));
        e.keep_checkin(
            CheckInInput {
                feeling: Feeling {
                    part: c.part,
                    point: FeelingPoint {
                        pleasant: 0.5,
                        energy: 0.2,
                    },
                    words: c.words.iter().map(|w| w.to_string()).collect(),
                },
                line: c.line.map(str::to_string),
                urge: None,
                replaces: None,
            },
            Source::Journal,
        )
        .unwrap();
    }
    read_all(h);
    h.clock.set_str(MORNING);
}

fn evidence(h: &Harness, d: &SeedDay) -> Evidence {
    let e = h.engine();
    let p = period(h, PeriodKind::Day, d.day);
    let mut ev = Evidence {
        items: e
            .reflection_input(&p)
            .unwrap()
            .items
            .into_iter()
            .map(|i| (i.id, i.text))
            .collect(),
        checkin_words: Vec::new(),
    };
    for c in e.glance(&p).unwrap().felt {
        ev.checkin_words.extend(c.feeling.words.clone());
        ev.items.push((c.id, c.line.unwrap_or_default()));
    }
    ev
}

fn scripted_mind() -> FakeMind {
    let mut mind = FakeMind::new();
    for _ in 0..RUNS {
        for d in DAYS {
            mind = mind.with_reflection(
                d.script
                    .iter()
                    .map(|(text, cites)| sentence(text, cites))
                    .collect(),
            );
        }
    }
    mind
}

#[test]
fn the_journal_check_passes_on_the_scripted_mind() {
    let (h, mind) = world(scripted_mind());
    h.state().reflecting.set_writes(Some(true));
    for d in DAYS {
        seed(&h, d);
    }
    let report = run_check(RUNS, |d| {
        let p = period(&h, PeriodKind::Day, d.day);
        let v = h.ok("reflect", arg(&p));
        assert_eq!(kind_of(&v), "written", "{}: {v}", d.id);
        let lines = v["reflection"]["theDay"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| Line {
                text: l["text"].as_str().unwrap().to_string(),
                receipts: l["receipts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| r["itemId"].as_str().unwrap().to_string())
                    .collect(),
            })
            .collect();
        (evidence(&h, d), lines)
    });
    assert_eq!(mind.call_count("reflect"), RUNS * DAYS.len());
    assert_eq!(report.runs, RUNS);
    assert_eq!(report.days, DAYS.len());
    assert_eq!(
        report.sentences,
        RUNS * DAYS.iter().map(|d| d.script.len()).sum::<usize>(),
        "every scripted sentence was kept"
    );
    assert!(report.passes(), "{:#?}", report.failures);
}

fn evidence_of_day_one() -> Evidence {
    Evidence {
        items: vec![
            (
                "i1".into(),
                "Dropped Leo at school, then a walk with Priya. I felt tired.".into(),
            ),
            ("c1".into(), "Quiet evening".into()),
        ],
        checkin_words: vec!["relieved".into()],
    }
}

fn line(text: &str, receipts: &[&str]) -> Line {
    Line {
        text: text.into(),
        receipts: receipts.iter().map(|r| r.to_string()).collect(),
    }
}

#[test]
fn the_scoring_fails_what_each_check_is_there_to_catch() {
    let ev = evidence_of_day_one();
    let fails = |text: &str, receipts: &[&str]| judge(&ev, &line(text, receipts));

    assert_eq!(fails("You **dropped Leo at school**.", &["i1"]), []);
    assert_eq!(fails("You were with Priya.", &["i1"]), []);
    assert_eq!(fails("You felt tired.", &["i1"]), [], "a quote supports it");
    assert_eq!(
        fails("You were relieved.", &["i1"]),
        [],
        "a check-in word supports it"
    );

    assert_eq!(fails("You went for a walk.", &[]), [Check::Receipts]);
    assert_eq!(
        fails("You went for a walk.", &["nope"]),
        [Check::Receipts],
        "a receipt must be an item of that day"
    );
    assert_eq!(
        fails("You were anxious all day.", &["i1"]),
        [Check::NoFeelingWithoutSupport]
    );
    for advice in [
        "You should rest more.",
        "Try to sleep earlier.",
        "Remember to call Priya.",
    ] {
        assert!(
            fails(advice, &["i1"]).contains(&Check::NoAdvice),
            "{advice}"
        );
    }
    assert_eq!(
        fails("You met Marek at the station.", &["i1"]),
        [Check::NoInventedPeople]
    );
    assert_eq!(
        fails("Gran was there too.", &["i1"]),
        [Check::NoInventedPeople],
        "a name at the start of a sentence counts too"
    );
}

#[test]
fn a_size_passes_only_when_every_sentence_of_every_run_does() {
    let ok = |_: &SeedDay| {
        (
            evidence_of_day_one(),
            vec![line("You **dropped Leo at school**.", &["i1"])],
        )
    };
    assert!(run_check(2, ok).passes());
    let mut n = 0;
    let one_bad = |d: &SeedDay| {
        n += 1;
        let text = if n == 5 {
            "You should rest."
        } else {
            "You **dropped Leo at school**."
        };
        let _ = d;
        (evidence_of_day_one(), vec![line(text, &["i1"])])
    };
    let report = run_check(2, one_bad);
    assert!(!report.passes());
    assert!(report.failed(Check::NoAdvice));
    assert!(!report.failed(Check::Receipts));
    let empty = run_check(1, |_| (evidence_of_day_one(), Vec::new()));
    assert!(!empty.passes(), "a run that wrote nothing has not passed");
}

#[test]
fn files_kept_into_the_entry_go_on_when_one_cannot() {
    let h = harness();
    h.clock.set_str(MORNING);
    let src = tempfile::tempdir().unwrap();
    let paths = [
        write_file(src.path(), "allotment.jpg", b"jpeg of the allotment"),
        write_file(src.path(), "letter.pdf", b"%PDF a letter"),
        write_file(src.path(), "gate.jpg", b"jpeg of the gate"),
    ];
    h.state().allow_paths(paths.clone());
    let kept = h.ok(
        "keep_files",
        json!({
            "paths": paths.iter().map(|p| p.to_str().unwrap()).collect::<Vec<_>>(),
            "source": "journal",
            "day": "2026-11-13",
            "block": 0,
        }),
    );
    assert_eq!(kept.as_array().unwrap().len(), 2, "{kept}");
    let day = h
        .engine()
        .journal_day("2026-11-13".parse().unwrap())
        .unwrap();
    let blocks: Vec<u32> = day.entry.iter().map(|b| b.block).collect();
    assert_eq!(blocks, [0, 1]);
}
