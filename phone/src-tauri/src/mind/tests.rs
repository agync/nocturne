use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use nocturne_contract::{CheckOutcome, ItemStatus, RemoteAnswer, RemoteCall};
use nocturne_engine::ManualClock;
use nocturne_mind::prompts;
use nocturne_provider::address::OnlineKey;
use nocturne_provider::stand_in::{Answer, KEY, MODEL, StandIn};
use tokio::runtime::Runtime;

use super::{Chosen, Keys, MemoryKeys, Pass, PhoneMind, ProviderSettings};
use crate::home::{Home, dispatch};

/// Keys shared with the test, so it can see what is held.
struct Shared(Arc<MemoryKeys>);

impl Keys for Shared {
    fn keep(&self, key: OnlineKey) {
        self.0.keep(key)
    }
    fn key(&self) -> Option<OnlineKey> {
        self.0.key()
    }
    fn forget(&self) {
        self.0.forget()
    }
}

struct World {
    _dir: tempfile::TempDir,
    clock: Arc<ManualClock>,
    rt: Runtime,
    home: Home,
    mind: PhoneMind,
    keys: Arc<MemoryKeys>,
    stand_in: StandIn,
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::at("2026-11-22T09:00:00Z", "Europe/London"));
    let home = Home::new(
        dir.path().to_path_buf(),
        clock.clone(),
        Box::new(|_: &Path| {}),
    );
    home.start_alone("Pixel", false).unwrap();
    let keys = Arc::new(MemoryKeys::default());
    let mind = PhoneMind::new(dir.path().to_path_buf(), Box::new(Shared(keys.clone())));
    mind.follow(&home);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let stand_in = rt.block_on(StandIn::start());
    stand_in.always(Answer::text(prompts::EXAMPLE_REPLY));
    World {
        _dir: dir,
        clock,
        rt,
        home,
        mind,
        keys,
        stand_in,
    }
}

impl World {
    fn keep(&self, text: &str, client_id: &str) -> String {
        let call = RemoteCall::KeepText {
            text: text.into(),
            client_id: client_id.into(),
            feeling: None,
            good: None,
            captured_at: None,
            web: false,
        };
        match dispatch::answer(&self.home.open_store().unwrap(), call) {
            RemoteAnswer::Kept { kept } => kept.item.id,
            other => panic!("not kept: {other:?}"),
        }
    }

    fn status(&self, id: &str) -> ItemStatus {
        let open = self.home.open_store().unwrap();
        open.engine.get_item(id).unwrap().status
    }

    fn settings(&self) -> ProviderSettings {
        ProviderSettings {
            url: self.stand_in.url(),
            key: KEY.into(),
            model: MODEL.into(),
        }
    }

    fn pass(&self) -> Pass {
        self.mind.pass(&self.home, self.rt.handle())
    }
}

#[test]
fn a_mind_never_turned_on_reaches_nothing_and_reads_nothing() {
    let w = world();
    let id = w.keep(prompts::EXAMPLE_NOTE, "c1");
    assert_eq!(w.mind.chosen(), Chosen::Off);
    for _ in 0..3 {
        assert_eq!(w.pass(), Pass::Off);
    }
    assert_eq!(w.status(&id), ItemStatus::Kept);
    assert!(w.stand_in.seen().is_empty());
}

#[test]
fn turned_on_it_reads_what_was_kept_through_the_one_host() {
    let w = world();
    let id = w.keep(prompts::EXAMPLE_NOTE, "c1");
    w.mind.turn_on(w.settings()).unwrap();
    let host = format!("127.0.0.1:{}", w.stand_in.addr.port());
    assert_eq!(
        w.mind.chosen(),
        Chosen::On {
            host,
            model: MODEL.into()
        }
    );
    assert!(matches!(w.pass(), Pass::Ran { read: 1, .. }));
    assert_eq!(w.status(&id), ItemStatus::Understood);
    let seen = w.stand_in.seen();
    assert!(!seen.is_empty());
    assert!(seen.iter().all(|s| s.path == "/v1/chat/completions"
        && s.authorization.as_deref() == Some("Bearer stand-in-key")));
    assert!(seen.iter().any(|s| {
        s.body["messages"][1]["content"]
            .as_str()
            .is_some_and(|c| c.contains(prompts::EXAMPLE_NOTE))
    }));
}

#[test]
fn after_turning_off_nothing_goes_and_the_key_is_gone() {
    let w = world();
    w.keep(prompts::EXAMPLE_NOTE, "c1");
    w.mind.turn_on(w.settings()).unwrap();
    assert!(w.keys.key().is_some());
    w.pass();
    let before = w.stand_in.seen().len();
    w.mind.turn_off();
    assert!(w.keys.key().is_none());
    assert_eq!(w.mind.chosen(), Chosen::Off);
    let id = w.keep("Sam's dentist moved to Thursday at 9.", "c2");
    for _ in 0..3 {
        assert_eq!(w.pass(), Pass::Off);
    }
    assert_eq!(w.status(&id), ItemStatus::Kept);
    assert_eq!(w.stand_in.seen().len(), before);
}

#[test]
fn a_choice_without_its_key_reaches_nothing() {
    let w = world();
    w.mind.turn_on(w.settings()).unwrap();
    let again = PhoneMind::new(w._dir.path().to_path_buf(), Box::new(MemoryKeys::default()));
    assert!(matches!(again.chosen(), Chosen::NeedsKey { .. }));
    w.keep(prompts::EXAMPLE_NOTE, "c1");
    assert_eq!(again.pass(&w.home, w.rt.handle()), Pass::Off);
    assert!(w.stand_in.seen().is_empty());
}

#[test]
fn an_address_off_the_rules_is_never_reached_or_kept() {
    let w = world();
    let settings = ProviderSettings {
        url: "http://api.example.com/v1".into(),
        key: KEY.into(),
        model: MODEL.into(),
    };
    assert_eq!(
        w.rt.block_on(PhoneMind::check(&settings)),
        CheckOutcome::NotHttps
    );
    assert!(w.mind.turn_on(settings).is_err());
    assert_eq!(w.mind.chosen(), Chosen::Off);
    assert!(w.keys.key().is_none());
    assert_eq!(
        w.rt.block_on(PhoneMind::check(&w.settings())),
        CheckOutcome::Works {
            host: format!("127.0.0.1:{}", w.stand_in.addr.port()),
            model: MODEL.into()
        }
    );
    assert_eq!(w.stand_in.seen().len(), 2);
}

#[test]
fn the_reader_lets_go_when_the_store_does() {
    let w = world();
    w.mind.turn_on(w.settings()).unwrap();
    w.pass();
    assert!(w.mind.thinking.lock().unwrap().is_some());
    {
        let _only = w.home.exclusive();
        w.home.close();
    }
    assert!(w.mind.thinking.lock().unwrap().is_none());
    w.keep(prompts::EXAMPLE_NOTE, "c1");
    assert!(matches!(w.pass(), Pass::Ran { read: 1, .. }));
}

/// Runs a pass on its own thread until the stand-in has a request in hand, then `then`.
fn while_a_request_hangs(w: &World, then: impl FnOnce()) -> Pass {
    w.stand_in.always(Answer::Hang);
    std::thread::scope(|scope| {
        let passing = scope.spawn(|| w.pass());
        let began = std::time::Instant::now();
        while w.stand_in.chats().is_empty() {
            assert!(began.elapsed() < Duration::from_secs(10), "no request came");
            std::thread::sleep(Duration::from_millis(10));
        }
        then();
        passing.join().unwrap()
    })
}

#[test]
fn a_store_wanted_mid_reading_is_given_way_to_at_once() {
    let w = world();
    let id = w.keep(prompts::EXAMPLE_NOTE, "c1");
    w.mind.turn_on(w.settings()).unwrap();
    let pass = while_a_request_hangs(&w, || {
        let began = std::time::Instant::now();
        let _only = w.home.exclusive();
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "{:?}",
            began.elapsed()
        );
    });
    assert_eq!(
        pass,
        Pass::Ran {
            read: 0,
            scheduled: false
        }
    );
    assert_eq!(w.status(&id), ItemStatus::Kept);
}

#[test]
fn turning_off_mid_reading_ends_the_request_and_never_waits_for_the_pass() {
    let w = world();
    let id = w.keep(prompts::EXAMPLE_NOTE, "c1");
    w.mind.turn_on(w.settings()).unwrap();
    let began = std::time::Instant::now();
    while_a_request_hangs(&w, || {
        let off = std::time::Instant::now();
        w.mind.turn_off();
        assert!(off.elapsed() < Duration::from_secs(1));
    });
    assert!(began.elapsed() < Duration::from_secs(15));
    assert_eq!(w.stand_in.chats().len(), 1);
    assert_eq!(w.status(&id), ItemStatus::Kept);
    assert_eq!(w.pass(), Pass::Off);
    assert_eq!(w.stand_in.seen().len(), 1);
}

#[test]
fn a_store_wanted_while_the_scheduler_writes_is_given_way_to_at_once() {
    let w = world();
    w.keep(prompts::EXAMPLE_NOTE, "c1");
    w.mind.turn_on(w.settings()).unwrap();
    while matches!(w.pass(), Pass::Ran { read: 1.., .. }) {}
    // A day on, the page is due, and nothing answers.
    w.clock.advance_days(1);
    w.stand_in.always(Answer::Hang);
    let asked = w.stand_in.chats().len();
    std::thread::scope(|scope| {
        let passing = scope.spawn(|| {
            for _ in 0..10 {
                if w.stand_in.chats().len() > asked {
                    break;
                }
                w.pass();
            }
        });
        let began = std::time::Instant::now();
        while w.stand_in.chats().len() == asked {
            assert!(
                began.elapsed() < Duration::from_secs(10),
                "nothing asked again"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let wanted = std::time::Instant::now();
        drop(w.home.exclusive());
        assert!(
            wanted.elapsed() < Duration::from_secs(5),
            "{:?}",
            wanted.elapsed()
        );
        passing.join().unwrap();
    });
    // What was being written was dropped unsaved; all is made anew at the next pass.
    assert!(w.mind.thinking.lock().unwrap().is_none());
}
