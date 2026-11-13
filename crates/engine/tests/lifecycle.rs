mod common;

use common::*;
use nocturne_contract::{LettingGo, MemoryFilter, MemoryStatus, SettingsPatch};
use nocturne_engine::{Predicate, UseKind};

fn status(f: &Fixture, id: &str) -> MemoryStatus {
    f.engine.get_memory(id).unwrap().status
}

fn jam(f: &Fixture) -> String {
    learn(
        &f.engine,
        "Leo likes jam",
        cand("Leo", Predicate::Prefers, "jam", "Leo likes jam."),
    )
    .1
}

#[test]
fn unused_for_30_days_fades_and_for_90_is_let_go_and_decay_is_idempotent() {
    let f = fixture();
    let id = jam(&f);
    f.clock.advance_days(29);
    assert_eq!(f.engine.decay().unwrap().faded, 0);
    assert_eq!(status(&f, &id), MemoryStatus::Active);
    f.clock.advance_days(1);
    let r = f.engine.decay().unwrap();
    assert_eq!((r.faded, r.let_go), (1, 0));
    assert_eq!(status(&f, &id), MemoryStatus::Fading);
    assert_eq!(f.engine.decay().unwrap(), Default::default(), "idempotent");
    f.clock.advance_days(59);
    assert_eq!(f.engine.decay().unwrap().let_go, 0);
    f.clock.advance_days(1);
    let r = f.engine.decay().unwrap();
    assert_eq!((r.faded, r.let_go), (0, 1));
    let m = f.engine.get_memory(&id).unwrap();
    assert_eq!(m.status, MemoryStatus::LetGo);
    assert!(m.let_go_at.is_some());
    assert_eq!(f.engine.decay().unwrap(), Default::default());
}

#[test]
fn answer_page_and_confirm_count_as_use_but_browsing_does_not() {
    let f = fixture();
    let a = jam(&f);
    let b = learn(
        &f.engine,
        "n2",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    )
    .1;
    let c = learn(
        &f.engine,
        "n3",
        cand("Pat", Predicate::Decided, "to move", "Pat decided to move."),
    )
    .1;
    let d = learn(
        &f.engine,
        "n4",
        cand("Sam", Predicate::Knows, "Max", "Sam knows Max."),
    )
    .1;
    f.clock.advance_days(40);
    // Browsing and searching are not use.
    f.engine.memories(MemoryFilter::default()).unwrap();
    f.engine.get_memory(&d).unwrap();
    f.engine.decay().unwrap();
    for id in [&a, &b, &c, &d] {
        assert_eq!(status(&f, id), MemoryStatus::Fading);
    }
    f.engine.mark_used(&[a.clone()], UseKind::Answer).unwrap();
    f.engine.mark_used(&[b.clone()], UseKind::Page).unwrap();
    f.engine.confirm(&c, true).unwrap();
    for id in [&a, &b, &c] {
        assert_eq!(
            status(&f, id),
            MemoryStatus::Active,
            "use moves fading back to active"
        );
    }
    assert_eq!(status(&f, &d), MemoryStatus::Fading);
    f.clock.advance_days(29);
    f.engine.decay().unwrap();
    assert_eq!(
        status(&f, &a),
        MemoryStatus::Active,
        "the clock restarted at use"
    );
    f.clock.advance_days(1);
    f.engine.decay().unwrap();
    assert_eq!(status(&f, &a), MemoryStatus::Fading);
}

#[test]
fn use_brings_a_let_go_memory_back() {
    let f = fixture();
    let id = jam(&f);
    f.clock.advance_days(100);
    f.engine.decay().unwrap();
    assert_eq!(status(&f, &id), MemoryStatus::LetGo);
    f.engine.mark_used(&[id.clone()], UseKind::Answer).unwrap();
    let m = f.engine.get_memory(&id).unwrap();
    assert_eq!(m.status, MemoryStatus::Active);
    assert_eq!(m.let_go_at, None);
}

#[test]
fn a_scheduled_memory_dated_four_months_ahead_never_fades_before_its_date() {
    let f = fixture();
    let mut c = cand(
        "Holiday",
        Predicate::Scheduled,
        "Lake District",
        "You are in the Lake District in March.",
    );
    c.valid_from = Some(date("2027-03-13"));
    let id = learn(&f.engine, "holiday booked", c).1;
    for days in [31, 61, 91, 119] {
        f.clock.set_str("2026-11-13T07:00:00Z");
        f.clock.advance_days(days);
        f.engine.decay().unwrap();
        assert_eq!(status(&f, &id), MemoryStatus::Active, "day {days}");
    }
    // On the day itself it has not passed yet.
    f.clock.set_str("2027-03-13T12:00:00Z");
    f.engine.decay().unwrap();
    assert_eq!(status(&f, &id), MemoryStatus::Active);
    // After the date, the idle clock starts at the end of that day.
    f.clock.set_str("2027-04-12T12:00:00Z");
    f.engine.decay().unwrap();
    assert_eq!(status(&f, &id), MemoryStatus::Active, "29 days after");
    f.clock.set_str("2027-04-14T12:00:00Z");
    f.engine.decay().unwrap();
    assert_eq!(status(&f, &id), MemoryStatus::Fading);
}

#[test]
fn promises_with_a_future_date_wait_and_undated_promises_decay_normally() {
    let f = fixture();
    let mut dated = cand(
        "Call",
        Predicate::Promised,
        "Gran",
        "You promised to call Gran.",
    );
    dated.due = Some(date("2027-01-10"));
    let dated = learn(&f.engine, "p1", dated).1;
    let undated = learn(
        &f.engine,
        "p2",
        cand(
            "Mend",
            Predicate::Promised,
            "the fence",
            "You promised to mend the fence.",
        ),
    )
    .1;
    f.clock.advance_days(60);
    f.engine.decay().unwrap();
    assert_eq!(status(&f, &dated), MemoryStatus::Active);
    assert_eq!(status(&f, &undated), MemoryStatus::Fading);
}

#[test]
fn keep_forever_and_safety_memories_never_fade() {
    let f = fixture();
    let jam = jam(&f);
    let dose = learn(
        &f.engine,
        "Leo takes metformin",
        cand("Leo", Predicate::Takes, "metformin", "Leo takes metformin."),
    )
    .1;
    f.engine.set_keep_forever(&jam, true).unwrap();
    f.clock.advance_days(400);
    f.engine.decay().unwrap();
    assert_eq!(status(&f, &jam), MemoryStatus::Active);
    assert_eq!(status(&f, &dose), MemoryStatus::Active);
    // The person can change it per memory.
    f.engine.set_keep_forever(&dose, false).unwrap();
    f.engine.decay().unwrap();
    assert_eq!(status(&f, &dose), MemoryStatus::LetGo);
}

#[test]
fn restore_returns_let_go_to_active_with_last_used_now() {
    let f = fixture();
    let id = jam(&f);
    let other = learn(
        &f.engine,
        "n2",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    )
    .1;
    f.clock.advance_days(100);
    f.engine.decay().unwrap();
    f.clock.set_str("2027-03-01T08:00:00Z");
    f.engine.restore(&[id.clone()]).unwrap();
    let m = f.engine.get_memory(&id).unwrap();
    assert_eq!(m.status, MemoryStatus::Active);
    assert_eq!(m.last_used.as_deref(), Some("2027-03-01T08:00:00.000Z"));
    assert_eq!(m.let_go_at, None);
    assert_eq!(
        status(&f, &other),
        MemoryStatus::LetGo,
        "only the named memory returns"
    );
    // It does not instantly fade again.
    f.engine.decay().unwrap();
    assert_eq!(status(&f, &id), MemoryStatus::Active);
    // Restoring a fading one works too, and an active one is left alone.
    f.engine.restore(&[id]).unwrap();
}

#[test]
fn the_letting_go_setting_scales_the_thresholds() {
    for (pace, fade, gone) in [(LettingGo::Gentle, 45, 120), (LettingGo::Brisk, 14, 45)] {
        let f = fixture();
        f.engine
            .update_settings(SettingsPatch {
                letting_go: Some(pace),
                ..Default::default()
            })
            .unwrap();
        let id = jam(&f);
        f.clock.advance_days(fade - 1);
        f.engine.decay().unwrap();
        assert_eq!(status(&f, &id), MemoryStatus::Active, "{pace:?}");
        f.clock.advance_days(1);
        f.engine.decay().unwrap();
        assert_eq!(status(&f, &id), MemoryStatus::Fading, "{pace:?}");
        f.clock.advance_days(gone - fade);
        f.engine.decay().unwrap();
        assert_eq!(status(&f, &id), MemoryStatus::LetGo, "{pace:?}");
    }
}

#[test]
fn decay_handles_more_than_one_chunk() {
    let f = fixture();
    let item = keep(&f.engine, "a long list");
    let cands: Vec<_> = (0..450)
        .map(|i| {
            cand(
                &format!("Thing {i}"),
                Predicate::Knows,
                "x",
                &format!("Thing {i} is known."),
            )
        })
        .collect();
    f.engine.apply_candidates(&item, cands).unwrap();
    f.clock.advance_days(100);
    let r = f.engine.decay().unwrap();
    assert_eq!(r.let_go, 450);
    let letgo = f
        .engine
        .memories(MemoryFilter {
            status: Some(MemoryStatus::LetGo),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(letgo.len(), 450);
}

#[test]
fn settings_round_trip_and_validate() {
    let f = fixture();
    let s = f.engine.settings().unwrap();
    assert_eq!(s.page_time, "07:00");
    let s = f
        .engine
        .update_settings(SettingsPatch {
            page_time: Some("06:30".into()),
            reminders: Some(false),
            ..Default::default()
        })
        .unwrap();
    assert_eq!((s.page_time.as_str(), s.reminders), ("06:30", false));
    assert_eq!(f.engine.settings().unwrap(), s);
    assert!(
        f.engine
            .update_settings(SettingsPatch {
                page_time: Some("25:00".into()),
                ..Default::default()
            })
            .is_err()
    );
    assert!(
        f.engine
            .update_settings(SettingsPatch {
                page_time: Some("7:00".into()),
                ..Default::default()
            })
            .is_err()
    );
}
