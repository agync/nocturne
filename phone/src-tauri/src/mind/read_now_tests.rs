//! Read now on battery.

use nocturne_contract::ItemStatus;
use nocturne_mind::prompts;
use nocturne_provider::stand_in::Answer;

use super::Pass;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tauri_plugin_nocturne_system::{ReadingAnswer, ReadingNote};

use super::read_now::{PhoneSide, ReadKind, Refused, Stopped};
use super::tests::{World, by_name, world};

/// On battery at `percent`, with battery saving as `saver`.
fn on_battery(w: &World, percent: u8, saver: bool) {
    w.mind
        .host()
        .set_power(Some(false), Some(percent), Some(saver));
}

/// `n` notes kept, the mind turned on, on battery at 80 per cent.
fn waiting(n: usize) -> (World, Vec<String>) {
    let w = world();
    let ids = (0..n)
        .map(|i| {
            w.keep(
                &format!("Note {i}. {}", prompts::EXAMPLE_NOTE),
                &format!("c{i}"),
            )
        })
        .collect();
    w.mind.turn_on(w.settings()).unwrap();
    on_battery(&w, 80, false);
    (w, ids)
}

fn read_now(w: &World, kind: ReadKind) -> Result<(), Refused> {
    w.mind.read_now(kind, w.rt.handle())
}

#[test]
fn on_battery_nothing_is_read_until_read_now() {
    let (w, ids) = waiting(1);
    assert_eq!(w.pass(), Pass::NotCharging);
    assert_eq!(w.status(&ids[0]), ItemStatus::Kept);
    let reading = w.mind.reading(&w.home);
    assert_eq!((reading.done, reading.count), (0, 1));
    assert!(!reading.charging);
    assert_eq!(reading.percent, Some(80));
    assert_eq!(reading.now, None);

    read_now(&w, ReadKind::Now).unwrap();
    assert_eq!(w.mind.reading(&w.home).now, Some(ReadKind::Now));
    assert!(matches!(w.pass(), Pass::Ran { read: 1, .. }));
    assert_ne!(w.status(&ids[0]), ItemStatus::Kept);
    // Nothing is left: Read now ends by itself, and no pass reads until the charger.
    w.pass();
    let reading = w.mind.reading(&w.home);
    assert_eq!(reading.now, None);
    assert_eq!(reading.stopped.map(|s| s.why), Some(Stopped::Done));
    w.keep("One more thing to read.", "c9");
    assert_eq!(w.pass(), Pass::NotCharging);
}

#[test]
fn read_now_reads_but_the_page_waits_for_the_charger() {
    let (w, _) = waiting(1);
    w.clock.advance_days(1);
    read_now(&w, ReadKind::Now).unwrap();
    let pages = || w.home.open_store().unwrap().engine.pages().unwrap().len();
    for _ in 0..5 {
        let pass = w.pass();
        assert!(
            matches!(
                pass,
                Pass::Ran {
                    scheduled: false,
                    ..
                } | Pass::NotCharging
            ),
            "{pass:?}"
        );
    }
    assert_eq!(pages(), 0);
}

#[test]
fn read_now_stops_at_twenty_per_cent_and_read_anyway_runs_down_to_five() {
    let (w, ids) = waiting(8);
    read_now(&w, ReadKind::Now).unwrap();
    assert!(matches!(w.pass(), Pass::Ran { read: 3, .. }));
    on_battery(&w, 20, false);
    assert_eq!(w.pass(), Pass::NotCharging);
    let reading = w.mind.reading(&w.home);
    assert_eq!(reading.now, None);
    let stopped = reading.stopped.unwrap();
    assert_eq!((stopped.why, stopped.done), (Stopped::Low, 3));
    let kept = ids
        .iter()
        .filter(|id| w.status(id) == ItemStatus::Kept)
        .count();
    assert_eq!(kept, 5);
    // At 20 per cent Read now is not offered; Read anyway is.
    assert_eq!(read_now(&w, ReadKind::Now), Err(Refused::Low));
    read_now(&w, ReadKind::Anyway).unwrap();
    assert_eq!(w.mind.reading(&w.home).stopped, None);
    on_battery(&w, 6, false);
    assert!(matches!(w.pass(), Pass::Ran { read: 3, .. }));
    on_battery(&w, 5, false);
    assert_eq!(w.pass(), Pass::NotCharging);
    assert_eq!(
        w.mind.reading(&w.home).stopped.map(|s| s.why),
        Some(Stopped::Floor)
    );
    assert_eq!(read_now(&w, ReadKind::Anyway), Err(Refused::Floor));
    assert_eq!(read_now(&w, ReadKind::Now), Err(Refused::Floor));
}

#[test]
fn battery_saving_stops_read_now_and_read_anyway_reads_through_it() {
    let (w, _) = waiting(4);
    read_now(&w, ReadKind::Now).unwrap();
    on_battery(&w, 80, true);
    assert_eq!(w.pass(), Pass::NotCharging);
    assert_eq!(
        w.mind.reading(&w.home).stopped.map(|s| s.why),
        Some(Stopped::Saver)
    );
    assert!(w.mind.reading(&w.home).saver);
    assert_eq!(read_now(&w, ReadKind::Now), Err(Refused::Saver));
    read_now(&w, ReadKind::Anyway).unwrap();
    assert!(matches!(w.pass(), Pass::Ran { read: 3, .. }));
    assert_eq!(w.mind.reading(&w.home).now, Some(ReadKind::Anyway));
}

#[test]
fn the_charger_takes_over_and_stop_ends_it() {
    let (w, _) = waiting(4);
    read_now(&w, ReadKind::Now).unwrap();
    w.mind.host().set_power(Some(true), Some(60), Some(false));
    assert!(matches!(w.pass(), Pass::Ran { read: 3, .. }));
    let reading = w.mind.reading(&w.home);
    assert_eq!(reading.now, None);
    assert_eq!(reading.stopped.map(|s| s.why), Some(Stopped::Charging));
    assert!(reading.charging);
    on_battery(&w, 60, false);
    assert_eq!(w.pass(), Pass::NotCharging);

    read_now(&w, ReadKind::Now).unwrap();
    w.mind.stop_read_now();
    let reading = w.mind.reading(&w.home);
    assert_eq!(reading.now, None);
    assert_eq!(reading.stopped.map(|s| s.why), Some(Stopped::Person));
    assert_eq!(w.pass(), Pass::NotCharging);
}

#[test]
fn turning_off_ends_read_now_and_a_new_start_never_has_it() {
    let (w, _) = waiting(2);
    read_now(&w, ReadKind::Now).unwrap();
    w.mind.turn_off();
    assert_eq!(w.mind.reading(&w.home).now, None);
    w.mind.turn_on(w.settings()).unwrap();
    assert_eq!(w.mind.reading(&w.home).now, None);
    assert_eq!(w.pass(), Pass::NotCharging);
    assert_eq!(read_now(&w, ReadKind::Now), Ok(()));
    let again = w.relaunch();
    again.host().set_power(Some(false), Some(80), Some(false));
    assert_eq!(again.reading(&w.home).now, None);
    assert_eq!(again.pass(&w.home, w.rt.handle()), Pass::NotCharging);
}

#[test]
fn the_limit_ends_read_now_and_holds() {
    use super::status::MindLimit;
    let (w, _) = waiting(2);
    assert!(w.mind.set_limit(MindLimit::Things { count: 1 }));
    read_now(&w, ReadKind::Now).unwrap();
    assert!(matches!(w.pass(), Pass::Ran { read: 1, .. }));
    assert_eq!(w.mind.reading(&w.home).now, None);
    assert_eq!(
        w.mind.reading(&w.home).stopped.map(|s| s.why),
        Some(Stopped::Limit)
    );
    assert_eq!(read_now(&w, ReadKind::Now), Err(Refused::Limit));
    assert_eq!(read_now(&w, ReadKind::Anyway), Err(Refused::Limit));
}

#[test]
fn trouble_ends_read_now_and_read_now_looks_once_more() {
    let (w, _) = waiting(2);
    w.stand_in.always(Answer::Status(503, "{}".into()));
    w.stand_in.models_status(503);
    read_now(&w, ReadKind::Now).unwrap();
    w.pass();
    assert_eq!(w.pass(), Pass::Waiting);
    assert_eq!(
        w.mind.reading(&w.home).stopped.map(|s| s.why),
        Some(Stopped::Trouble)
    );
    let rows = w.mind.sent(1000, None).len();
    let seen = w.stand_in.seen().len();
    assert_eq!(read_now(&w, ReadKind::Now), Err(Refused::NotAnswering));
    assert!(w.mind.reading(&w.home).still_not);
    assert_eq!(w.stand_in.seen().len(), seen + 1, "one look");
    let sent = w.mind.sent(1000, None);
    assert_eq!(sent.len(), rows + 1);
    assert_eq!(sent[0].sent_for, super::SentFor::Check);

    w.stand_in.models_listed();
    w.stand_in.always(Answer::text(prompts::EXAMPLE_REPLY));
    assert_eq!(read_now(&w, ReadKind::Now), Ok(()));
    assert!(!w.mind.reading(&w.home).still_not);
    assert!(matches!(w.pass(), Pass::Ran { read: 1.., .. }));
}

#[test]
fn a_refused_key_ends_read_now_and_is_not_read_through() {
    let (w, _) = waiting(2);
    w.stand_in.always(Answer::Status(401, "{}".into()));
    read_now(&w, ReadKind::Now).unwrap();
    w.pass();
    w.pass();
    assert_eq!(w.mind.reading(&w.home).now, None);
    assert_eq!(
        w.mind.reading(&w.home).stopped.map(|s| s.why),
        Some(Stopped::Trouble)
    );
    assert_eq!(read_now(&w, ReadKind::Now), Err(Refused::Trouble));
}

#[test]
fn an_iphone_pause_keeps_read_now_and_says_after_how_many() {
    let (w, _) = waiting(4);
    read_now(&w, ReadKind::Now).unwrap();
    w.pass();
    w.mind.paused_away();
    let reading = w.mind.reading(&w.home);
    assert_eq!(reading.now, Some(ReadKind::Now));
    assert_eq!(reading.away, Some(3));
    assert!(matches!(w.pass(), Pass::Ran { read: 1, .. }));
    // A new Read now forgets the pause.
    w.mind.stop_read_now();
    read_now(&w, ReadKind::Now).unwrap();
    assert_eq!(w.mind.reading(&w.home).away, None);
}

#[test]
fn the_screens_read_read_now_as_mind_ts_says() {
    let (w, _) = waiting(4);
    read_now(&w, ReadKind::Anyway).unwrap();
    w.pass();
    on_battery(&w, 4, false);
    w.pass();
    let json = serde_json::to_value(w.mind.reading(&w.home)).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "done": 3, "count": 4, "charging": false, "percent": 4, "saver": false,
            "now": null, "stopped": { "why": "floor", "done": 3 }, "away": null,
            "stillNot": false, "quiet": false
        })
    );
    let refused = serde_json::to_value(Refused::NotAnswering).unwrap();
    assert_eq!(refused, serde_json::json!("notAnswering"));
    assert_eq!(
        serde_json::to_value(ReadKind::Anyway).unwrap(),
        serde_json::json!("anyway")
    );
}

/// The phone's side in a test: its power in turn (the last repeats), the notes it was shown, and
/// the answers it gives in turn (then none pressed).
#[derive(Default)]
struct FakePhone {
    power: Mutex<VecDeque<u8>>,
    shown: Mutex<Vec<ReadingNote>>,
    answers: Mutex<VecDeque<ReadingAnswer>>,
}

impl FakePhone {
    fn at(levels: &[u8]) -> Arc<FakePhone> {
        Arc::new(FakePhone {
            power: Mutex::new(levels.iter().copied().collect()),
            ..FakePhone::default()
        })
    }

    fn answer(&self, answer: ReadingAnswer) {
        self.answers.lock().unwrap().push_back(answer);
    }

    fn last(&self) -> Option<ReadingNote> {
        self.shown.lock().unwrap().last().cloned()
    }

    fn ended(&self) -> Vec<String> {
        self.shown
            .lock()
            .unwrap()
            .iter()
            .filter_map(|n| match n {
                ReadingNote::Ended { why, .. } => Some(why.clone()),
                ReadingNote::Reading { .. } => None,
            })
            .collect()
    }
}

impl PhoneSide for FakePhone {
    fn power(&self) -> (Option<bool>, Option<u8>, Option<bool>) {
        let mut levels = self.power.lock().unwrap();
        let level = if levels.len() > 1 {
            levels.pop_front()
        } else {
            levels.front().copied()
        };
        (Some(false), level, Some(false))
    }

    fn show(&self, note: &ReadingNote) -> Option<ReadingAnswer> {
        self.shown.lock().unwrap().push(note.clone());
        let answer = self.answers.lock().unwrap().pop_front();
        Some(answer.unwrap_or(ReadingAnswer {
            notes: true,
            ..ReadingAnswer::default()
        }))
    }
}

/// `n` waiting, the phone at `levels`, Read now pressed and shown by the phone.
fn shown(n: usize, levels: &[u8]) -> (World, Arc<FakePhone>) {
    let (w, _) = waiting(n);
    let phone = FakePhone::at(levels);
    w.mind.use_phone(phone.clone());
    w.mind.feel_power();
    read_now(&w, ReadKind::Now).unwrap();
    w.mind.tell_phone_from(&w.home);
    assert!(matches!(phone.last(), Some(ReadingNote::Reading { .. })));
    (w, phone)
}

#[test]
fn turning_off_tells_the_phone_to_stop_showing_the_reading() {
    let (w, phone) = shown(4, &[80]);
    w.mind.turn_off();
    assert_eq!(phone.ended(), ["off"]);
    assert_eq!(w.mind.read_now_on(), None);
    // Nothing more is shown after.
    w.mind.tell_phone_from(&w.home);
    assert_eq!(phone.shown.lock().unwrap().len(), 2);

    let (w, phone) = shown(4, &[80]);
    w.mind.turn_on(w.settings()).unwrap();
    assert_eq!(phone.ended(), ["off"], "a mind chosen again ends it too");

    let (w, phone) = shown(4, &[80]);
    w.mind.erase();
    assert_eq!(phone.ended(), ["off"]);
}

#[test]
fn stop_where_the_phone_shows_it_ends_the_pass_before_the_next_thing() {
    let (w, phone) = shown(6, &[80]);
    // Before the first thing nothing was pressed; before the second, Stop.
    phone.answer(ReadingAnswer::default());
    phone.answer(ReadingAnswer {
        stop: true,
        ..ReadingAnswer::default()
    });
    assert!(matches!(w.pass(), Pass::Ran { read: 1, .. }));
    let reading = w.mind.reading(&w.home);
    assert_eq!(reading.now, None);
    assert_eq!(reading.stopped.map(|s| s.why), Some(Stopped::Person));
    assert_eq!(phone.ended(), ["person"]);
}

#[test]
fn the_floor_mid_pass_stops_before_the_next_thing() {
    let (w, phone) = shown(6, &[80, 80, 5]);
    read_now(&w, ReadKind::Anyway).unwrap();
    assert!(matches!(w.pass(), Pass::Ran { read: 1, .. }));
    assert_eq!(
        w.mind.reading(&w.home).stopped.map(|s| s.why),
        Some(Stopped::Floor)
    );
    assert_eq!(phone.ended(), ["floor"]);
}

#[test]
fn the_phones_own_time_limit_is_not_the_persons_stop() {
    let (w, phone) = shown(4, &[80]);
    phone.answer(ReadingAnswer {
        timeout: true,
        ..ReadingAnswer::default()
    });
    w.mind.tell_phone_from(&w.home);
    assert_eq!(
        w.mind.reading(&w.home).stopped.map(|s| s.why),
        Some(Stopped::Timeout)
    );
    match phone.last() {
        Some(ReadingNote::Ended { why, title, .. }) => {
            assert_eq!(why, "timeout");
            assert!(title.is_some(), "it says so");
        }
        other => panic!("not ended: {other:?}"),
    }
}

#[test]
fn read_now_and_stop_wake_the_passes_at_once() {
    let (w, _) = waiting(2);
    let woken = |w: &World| {
        w.rt.block_on(async {
            tokio::time::timeout(std::time::Duration::from_millis(20), w.mind.woken())
                .await
                .is_ok()
        })
    };
    // Turning on woke them; that is taken, and nothing else has.
    assert!(woken(&w));
    assert!(!woken(&w));
    read_now(&w, ReadKind::Now).unwrap();
    assert!(woken(&w));
    w.mind.stop_read_now();
    assert!(woken(&w));
}

#[test]
fn read_now_judges_the_battery_as_the_phone_says_it_now() {
    let (w, _) = waiting(2);
    w.mind.use_phone(FakePhone::at(&[15]));
    w.mind.feel_power();
    assert_eq!(read_now(&w, ReadKind::Now), Err(Refused::Low));
}

#[test]
fn read_now_is_refused_with_the_mind_off_or_this_phone_offline() {
    let w = world();
    on_battery(&w, 80, false);
    assert_eq!(read_now(&w, ReadKind::Now), Err(Refused::Off));

    let w = world();
    let (online, settings) = by_name(&w, true);
    online.store(false, std::sync::atomic::Ordering::SeqCst);
    w.mind.turn_on(settings).unwrap();
    w.keep("Waiting while offline.", "c9");
    w.pass();
    on_battery(&w, 80, false);
    assert!(matches!(
        w.mind.status(),
        super::status::MindStatus::Offline { .. }
    ));
    assert_eq!(read_now(&w, ReadKind::Now), Err(Refused::Offline));
}
