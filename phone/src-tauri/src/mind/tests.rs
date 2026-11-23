use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use nocturne_contract::{
    AnsweredBy, AskDelta, AskPhase, CheckOutcome, ItemStatus, RemoteAnswer, RemoteCall,
};
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
    fn keep(&self, key: OnlineKey) -> bool {
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
    let mind = PhoneMind::new(
        dir.path().to_path_buf(),
        Box::new(Shared(keys.clone())),
        clock.clone(),
    );
    mind.follow(&home);
    mind.host().set_charging(Some(true));
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
    let again = PhoneMind::new(
        w._dir.path().to_path_buf(),
        Box::new(MemoryKeys::default()),
        w.clock.clone(),
    );
    again.host().set_charging(Some(true));
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
        w.rt.block_on(w.mind.check(&settings)),
        CheckOutcome::NotHttps
    );
    assert!(w.mind.turn_on(settings).is_err());
    assert_eq!(w.mind.chosen(), Chosen::Off);
    assert!(w.keys.key().is_none());
    assert_eq!(
        w.rt.block_on(w.mind.check(&w.settings())),
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

#[test]
fn nothing_is_read_unless_the_phone_is_known_to_charge() {
    let w = world();
    let id = w.keep(prompts::EXAMPLE_NOTE, "c1");
    w.mind.turn_on(w.settings()).unwrap();
    for charging in [None, Some(false)] {
        w.mind.host().set_charging(charging);
        assert_eq!(w.pass(), Pass::NotCharging);
    }
    assert_eq!(w.status(&id), ItemStatus::Kept);
    assert!(w.stand_in.seen().is_empty());
    w.mind.host().set_charging(Some(true));
    assert!(matches!(w.pass(), Pass::Ran { read: 1, .. }));
}

#[test]
fn the_first_page_waits_for_the_next_morning() {
    let w = world();
    w.keep(prompts::EXAMPLE_NOTE, "c1");
    // A day on, this morning's page is missed, and the mind is turned on.
    w.clock.advance_days(1);
    w.mind.turn_on(w.settings()).unwrap();
    let pages = || w.home.open_store().unwrap().engine.pages().unwrap().len();
    for _ in 0..5 {
        assert!(matches!(
            w.pass(),
            Pass::Ran {
                scheduled: false,
                ..
            }
        ));
    }
    assert_eq!(pages(), 0);
    w.clock.advance_days(1);
    for _ in 0..10 {
        if pages() > 0 {
            break;
        }
        w.pass();
    }
    assert_eq!(pages(), 1);
}

#[test]
fn a_key_in_the_safe_place_turns_the_mind_on_again_at_the_next_start() {
    let w = world();
    w.mind.turn_on(w.settings()).unwrap();
    let again = PhoneMind::new(
        w._dir.path().to_path_buf(),
        Box::new(Shared(w.keys.clone())),
        w.clock.clone(),
    );
    assert!(matches!(again.chosen(), Chosen::On { .. }));
    again.turn_off();
    assert!(w.keys.key().is_none());
}

/// A safe place that keeps nothing.
struct Refusing;

impl Keys for Refusing {
    fn keep(&self, _: OnlineKey) -> bool {
        false
    }
    fn key(&self) -> Option<OnlineKey> {
        None
    }
    fn forget(&self) {}
}

#[test]
fn a_key_the_safe_place_refuses_lasts_until_the_app_closes() {
    let w = world();
    let refusing = PhoneMind::new(
        w._dir.path().to_path_buf(),
        Box::new(Refusing),
        w.clock.clone(),
    );
    refusing.turn_on(w.settings()).unwrap();
    assert!(refusing.is_on());
    let again = PhoneMind::new(
        w._dir.path().to_path_buf(),
        Box::new(Refusing),
        w.clock.clone(),
    );
    assert!(matches!(again.chosen(), Chosen::NeedsKey { .. }));
}

/// A safe place that holds what it already had and refuses anything new.
struct Stuck(Arc<MemoryKeys>);

impl Keys for Stuck {
    fn keep(&self, _: OnlineKey) -> bool {
        false
    }
    fn key(&self) -> Option<OnlineKey> {
        self.0.key()
    }
    fn forget(&self) {
        self.0.forget()
    }
}

#[test]
fn a_new_choice_whose_key_is_refused_leaves_no_older_key_behind() {
    let w = world();
    w.mind.turn_on(w.settings()).unwrap();
    let other = w.rt.block_on(StandIn::start());
    let stuck = PhoneMind::new(
        w._dir.path().to_path_buf(),
        Box::new(Stuck(w.keys.clone())),
        w.clock.clone(),
    );
    let settings = ProviderSettings {
        url: other.url(),
        key: "another-key".into(),
        model: MODEL.into(),
    };
    stuck.turn_on(settings).unwrap();
    assert!(w.keys.key().is_none());
    let again = PhoneMind::new(
        w._dir.path().to_path_buf(),
        Box::new(Shared(w.keys.clone())),
        w.clock.clone(),
    );
    assert!(matches!(again.chosen(), Chosen::NeedsKey { .. }));
}

#[test]
fn a_key_given_for_another_address_is_never_used() {
    let w = world();
    w.mind.turn_on(w.settings()).unwrap();
    let other = w.rt.block_on(StandIn::start());
    let wrong = OnlineKey::new(format!("{}:{KEY}", "0".repeat(64)));
    w.keys.keep(wrong);
    let again = PhoneMind::new(
        w._dir.path().to_path_buf(),
        Box::new(Shared(w.keys.clone())),
        w.clock.clone(),
    );
    again.host().set_charging(Some(true));
    assert!(matches!(again.chosen(), Chosen::NeedsKey { .. }));
    w.keep(prompts::EXAMPLE_NOTE, "c1");
    assert_eq!(again.pass(&w.home, w.rt.handle()), Pass::Off);
    assert!(w.stand_in.seen().is_empty() && other.seen().is_empty());
}

#[test]
fn only_a_pass_that_read_its_fill_or_wrote_has_more_to_do() {
    let ran = |read, scheduled| Pass::Ran { read, scheduled };
    assert!(ran(super::FEW, false).more());
    assert!(ran(0, true).more());
    assert!(!ran(1, false).more());
    for idle in [Pass::Off, Pass::NotCharging, Pass::Paused, Pass::NotReady] {
        assert!(!idle.more());
    }
}

#[test]
fn the_sent_list_matches_what_the_provider_received() {
    let w = world();
    assert!(matches!(
        w.rt.block_on(w.mind.check(&w.settings())),
        CheckOutcome::Works { .. }
    ));
    w.keep(prompts::EXAMPLE_NOTE, "c1");
    w.mind.turn_on(w.settings()).unwrap();
    while matches!(w.pass(), Pass::Ran { read: 1.., .. }) {}
    let seen = w.stand_in.seen();
    let sent = w.mind.sent(1000, None);
    assert_eq!(sent.len(), seen.len());
    let host = format!("127.0.0.1:{}", w.stand_in.addr.port());
    assert!(sent.iter().all(|s| s.host == host && s.status == Some(200)));
    let checks = sent.iter().filter(|s| s.sent_for == super::SentFor::Check);
    assert_eq!(checks.count(), 2);
    assert_eq!(sent.last().unwrap().sent_for, super::SentFor::Check);
}

#[test]
fn deleting_everything_takes_the_key_the_choice_and_the_sent_list() {
    let w = world();
    w.rt.block_on(w.mind.check(&w.settings()));
    w.mind.turn_on(w.settings()).unwrap();
    assert!(!w.mind.sent(10, None).is_empty());
    w.mind.erase();
    assert!(w.keys.key().is_none());
    assert_eq!(w.mind.chosen(), Chosen::Off);
    assert!(w.mind.sent(10, None).is_empty());
}

impl World {
    fn ask(&self, question: &str) -> Vec<AskDelta> {
        let mut steps = Vec::new();
        self.mind
            .ask(&self.home, self.rt.handle(), question, &mut |d| {
                steps.push(d)
            });
        steps
    }
}

#[test]
fn a_question_is_answered_online_with_its_receipts() {
    let w = world();
    let id = w.keep(prompts::EXAMPLE_NOTE, "c1");
    w.mind.turn_on(w.settings()).unwrap();
    while matches!(w.pass(), Pass::Ran { read: 1.., .. }) {}
    let asked = w.stand_in.chats().len();
    w.stand_in.then(Answer::text("Ana lives in **Hull** [m1]."));
    let steps = w.ask("Where does Ana live?");
    let last = steps.last().unwrap();
    assert_eq!(last.phase, AskPhase::Done, "{steps:?}");
    assert!(last.receipts.iter().all(|r| r.item_id == id));
    assert!(!last.receipts.is_empty());
    assert_eq!(last.memory_ids.len(), 1);
    assert!(matches!(&last.answered_by, Some(AnsweredBy::Online { .. })));
    let words: String = steps.iter().filter_map(|d| d.text_delta.clone()).collect();
    assert!(words.contains("Hull"), "{words}");
    let chats = w.stand_in.chats();
    assert_eq!(chats.len(), asked + 1);
    let sent = w.mind.sent(1, None);
    assert_eq!(sent[0].sent_for, super::SentFor::Answer);
}

#[test]
fn an_answer_that_cites_nothing_is_not_sure() {
    let w = world();
    w.keep(prompts::EXAMPLE_NOTE, "c1");
    w.mind.turn_on(w.settings()).unwrap();
    while matches!(w.pass(), Pass::Ran { read: 1.., .. }) {}
    w.stand_in.then(Answer::text("Ana lives in Leeds."));
    let steps = w.ask("Where does Ana live?");
    let last = steps.last().unwrap();
    assert_eq!(last.phase, AskPhase::NotSure);
    assert!(!last.near.is_empty());
}

#[test]
fn with_the_mind_off_a_question_reaches_nothing() {
    let w = world();
    w.keep(prompts::EXAMPLE_NOTE, "c1");
    assert_eq!(
        w.ask("Where does Ana live?").last().unwrap().phase,
        AskPhase::Unavailable
    );
    let call = w
        .mind
        .answer_call(&w.home, w.rt.handle(), "Where does Ana live?");
    assert!(matches!(call, RemoteAnswer::Error { .. }));
    assert!(w.stand_in.seen().is_empty());
}

/// Keeps the ids of items it hears of.
#[derive(Default)]
struct Heard(std::sync::Mutex<Vec<String>>);

impl nocturne_intelligence::Events for Heard {
    fn item_changed(&self, item: &nocturne_contract::ItemView) {
        self.0.lock().unwrap().push(item.id.clone());
    }
}

#[test]
fn the_screens_hear_of_each_thing_read() {
    let w = world();
    let heard = Arc::new(Heard::default());
    w.mind.tell(heard.clone());
    let id = w.keep(prompts::EXAMPLE_NOTE, "c1");
    w.mind.turn_on(w.settings()).unwrap();
    w.pass();
    assert!(heard.0.lock().unwrap().contains(&id));
}

/// Whether a request the stand-in received after the first `from` carried `words`.
fn reached(w: &World, from: usize, words: &str) -> bool {
    w.stand_in.chats()[from..]
        .iter()
        .any(|c| c.body.to_string().contains(words))
}

#[test]
fn a_memory_being_forgotten_never_reaches_the_provider() {
    let w = world();
    w.keep(prompts::EXAMPLE_NOTE, "c1");
    w.mind.turn_on(w.settings()).unwrap();
    while matches!(w.pass(), Pass::Ran { read: 1.., .. }) {}
    let e = w.home.open_store().unwrap().engine;
    let ana: Vec<String> = e
        .retrieve("Ana Hull", None, 12)
        .unwrap()
        .into_iter()
        .filter(|h| h.memory.sentence.contains("Hull"))
        .map(|h| h.memory.id)
        .collect();
    assert!(!ana.is_empty());
    let preview = e.preview_forget(&ana, false).unwrap();
    e.forget_begin_only(&preview.token).unwrap();
    let asked = w.stand_in.chats().len();
    let steps = w.ask("Ana Hull");
    let phase = steps.last().unwrap().phase;
    assert!(
        matches!(phase, AskPhase::NotSure | AskPhase::Nothing),
        "{steps:?}"
    );
    assert_eq!(w.stand_in.chats().len(), asked);
    assert!(!reached(&w, asked, "has lived in Hull"));
}

#[test]
fn a_private_memory_still_goes_to_the_mind_the_person_chose() {
    let w = world();
    let id = w.keep(prompts::EXAMPLE_NOTE, "c1");
    w.mind.turn_on(w.settings()).unwrap();
    while matches!(w.pass(), Pass::Ran { read: 1.., .. }) {}
    let e = w.home.open_store().unwrap().engine;
    e.set_item_private(&id, true).unwrap();
    let asked = w.stand_in.chats().len();
    w.stand_in.then(Answer::text("Ana lives in Hull [m1]."));
    w.ask("Where does Ana live?");
    assert!(reached(&w, asked, "has lived in Hull"));
}

#[test]
fn a_voice_note_waits_for_the_phones_own_reader() {
    use nocturne_contract::{ItemKind, ItemStatus};
    let w = world();
    let open = w.home.open_store().unwrap();
    let wav = w._dir.path().join("note.wav");
    std::fs::write(&wav, b"RIFF....WAVE").unwrap();
    let kept = open
        .engine
        .keep_file(&wav, nocturne_contract::Source::Recording)
        .unwrap();
    let nocturne_contract::Kept::New { item } = kept else {
        panic!("kept before");
    };
    assert_eq!(item.kind, ItemKind::Audio);
    w.mind.turn_on(w.settings()).unwrap();
    w.pass();
    assert_eq!(w.status(&item.id), ItemStatus::Kept);
    assert!(w.stand_in.chats().is_empty());
    // The phone's reader has had its turn and could not decode it: the mind may take it.
    assert!(w.home.set_language("en-GB"));
    w.home.hear_waiting(&NoWords);
    w.pass();
    assert!(!w.stand_in.chats().is_empty());
}

/// A speech reader that is ready and hears nothing.
struct NoWords;

impl crate::home::voice::Reader for NoWords {
    fn ready(&self, _: &str) -> bool {
        true
    }
    fn read(&self, _: &Path, _: &str) -> crate::home::voice::Heard {
        crate::home::voice::Heard::Nothing
    }
}

#[test]
fn with_a_limit_of_three_things_the_fourth_waits() {
    use super::status::{MindLimit, MindReading, MindStatus};
    let w = world();
    let ids: Vec<String> = (0..4)
        .map(|n| {
            w.keep(
                &format!("Note {n}. {}", prompts::EXAMPLE_NOTE),
                &format!("c{n}"),
            )
        })
        .collect();
    assert!(w.mind.set_limit(MindLimit::Things { count: 3 }));
    w.mind.turn_on(w.settings()).unwrap();
    for _ in 0..5 {
        w.pass();
    }
    let waiting = ids
        .iter()
        .filter(|id| w.status(id) == ItemStatus::Kept)
        .count();
    assert_eq!(waiting, 1);
    assert_eq!(w.pass(), Pass::Paused);
    assert_eq!(w.mind.month().count, 3);
    assert_eq!(w.mind.reading(&w.home), MindReading { done: 3, count: 4 });
    assert!(matches!(w.mind.status(), MindStatus::Paused { until, .. } if until == "2026-12-01"));
    let asked = w.stand_in.chats().len();
    assert_eq!(w.ask("Ana").last().unwrap().phase, AskPhase::Unavailable);
    assert_eq!(w.stand_in.chats().len(), asked);
    // A new month starts the count again.
    w.clock.advance_days(10);
    assert!(matches!(w.pass(), Pass::Ran { read: 1, .. }));
}

#[test]
fn a_service_that_stops_answering_is_told_until_it_answers_again() {
    use super::status::MindStatus;
    let w = world();
    w.keep(prompts::EXAMPLE_NOTE, "c1");
    w.mind.turn_on(w.settings()).unwrap();
    assert!(matches!(w.mind.status(), MindStatus::Fine { .. }));
    w.stand_in.always(Answer::Status(401, "{}".into()));
    w.pass();
    assert!(matches!(w.mind.status(), MindStatus::KeyRefused { .. }));
    w.stand_in.always(Answer::Status(503, "{}".into()));
    w.mind.turn_on(w.settings()).unwrap();
    w.pass();
    assert!(matches!(w.mind.status(), MindStatus::Unreachable { .. }));
    w.mind.turn_off();
    assert_eq!(w.mind.status(), MindStatus::Off);
}

#[test]
fn the_screens_read_the_sent_list_and_the_month_as_mind_ts_says() {
    use super::status::{MindLimit, MindSent};
    let w = world();
    w.rt.block_on(w.mind.check(&w.settings()));
    let sent: Vec<MindSent> = w
        .mind
        .sent(10, None)
        .into_iter()
        .map(MindSent::from)
        .collect();
    let json = serde_json::to_value(&sent[0]).unwrap();
    assert_eq!(json["kind"], "check");
    assert_eq!(json["id"], "2");
    assert!(json["bytes"].as_u64().unwrap() > 0);
    assert!(json.get("words").is_some());
    let limit: MindLimit =
        serde_json::from_value(serde_json::json!({"kind": "amount", "usd": 5.0})).unwrap();
    assert_eq!(limit, MindLimit::Amount { usd: 5.0 });
    w.mind.set_limit(limit);
    let month = serde_json::to_value(w.mind.month()).unwrap();
    assert_eq!(
        month,
        serde_json::json!({"count": 0, "limit": {"kind": "amount", "usd": 5.0}})
    );
    assert_eq!(
        serde_json::to_value(MindLimit::None).unwrap(),
        serde_json::json!({"kind": "none"})
    );
}
