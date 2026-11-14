//! Answers over two sides, with the memories Tiny stored: an open which-is-right question about
//! book club, and a dentist appointment a later note says moved. A mind that gives one side,
//! as Tiny did, is shown both, each with its receipt.

mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use nocturne_contract::{AskDelta, AskPhase, PageView, Source};
use nocturne_engine::{
    AnswerMeta, BoxFuture, Candidate, ExtractContext, Fact, FakeMind, Mind, MindError, Predicate,
    Transcript, Understanding, is_source_line,
};
use serde_json::json;
use tokio::sync::mpsc;

const CLUB1: &str = "The book club meets on the first Tuesday of the month at the library.";
const CLUB2: &str =
    "The book club meets on the second Wednesday of the month at the community hall.";
const WED: &str = "The dentist appointment with Dr Okafor is on Wednesday 18 November at 10:00.";
const NEW_LINE: &str = "Dr Okafor's receptionist called: it is now Thursday 19 November at 14:30, not Wednesday 18 November.";
const MOVED: &str = "The dentist appointment has moved. Dr Okafor's receptionist called: it is now Thursday 19 November at 14:30, not Wednesday 18 November.";

/// Answers as Tiny did: the first fact with another side, or else the one that says something
/// moved, in its own words, citing it alone. It keeps the facts it was given.
struct OneSide(FakeMind, std::sync::Mutex<Vec<Vec<Fact>>>);

impl Mind for OneSide {
    fn transcribe<'a>(&'a self, audio: &'a Path) -> BoxFuture<'a, Result<Transcript, MindError>> {
        self.0.transcribe(audio)
    }
    fn read_image<'a>(
        &'a self,
        image: &'a Path,
    ) -> BoxFuture<'a, Result<Understanding, MindError>> {
        self.0.read_image(image)
    }
    fn read_document<'a>(
        &'a self,
        doc: &'a Path,
    ) -> BoxFuture<'a, Result<Understanding, MindError>> {
        self.0.read_document(doc)
    }
    fn extract<'a>(
        &'a self,
        text: &'a str,
        cx: &'a ExtractContext,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
        self.0.extract(text, cx)
    }
    fn embed<'a>(&'a self, texts: &'a [String]) -> BoxFuture<'a, Result<Vec<Vec<f32>>, MindError>> {
        self.0.embed(texts)
    }
    fn answer<'a>(
        &'a self,
        _question: &'a str,
        facts: &'a [Fact],
        out: mpsc::Sender<String>,
    ) -> BoxFuture<'a, Result<AnswerMeta, MindError>> {
        Box::pin(async move {
            self.1.lock().unwrap().push(facts.to_vec());
            let fact = facts
                .iter()
                .find(|f| !f.disputed_by.is_empty() && !is_source_line(&f.memory.id))
                .or_else(|| facts.iter().find(|f| f.memory.sentence.contains("moved")))
                .unwrap_or(&facts[0]);
            let _ = out.send(fact.memory.sentence.clone()).await;
            Ok(AnswerMeta {
                cited: vec![fact.memory.id.clone()],
                ..Default::default()
            })
        })
    }
    fn write_headline<'a>(
        &'a self,
        page: &'a PageView,
    ) -> BoxFuture<'a, Result<(String, String), MindError>> {
        self.0.write_headline(page)
    }
    fn speak<'a>(
        &'a self,
        text: &'a str,
        voice: &'a str,
    ) -> BoxFuture<'a, Result<Vec<u8>, MindError>> {
        self.0.speak(text, voice)
    }
}

fn harness() -> Harness {
    harness_with_mind(Arc::new(OneSide(FakeMind::new(), Default::default())))
}

/// Keep a note at `at` and apply what was read from it; returns the item and what it made.
fn note(h: &Harness, at: &str, text: &str, read: Vec<Candidate>) -> (String, Vec<String>) {
    h.clock.set_str(at);
    let e = h.engine();
    let item = match e.keep_text(text, Source::QuickCapture).unwrap() {
        nocturne_contract::Kept::New { item } | nocturne_contract::Kept::Duplicate { item } => {
            item.id
        }
    };
    let lease = e.lease_item(&item, Duration::from_secs(60)).unwrap();
    let report = e.apply_candidates(&item, &lease.lease, read).unwrap();
    (item, report.created)
}

fn ask(h: &Harness, question: &str) -> (String, AskDelta) {
    let seen = h.ask_deltas();
    let id = h
        .ok("ask", json!({"question": question}))
        .as_str()
        .unwrap()
        .to_string();
    wait_for("the answer", || {
        deltas_for(&seen, &id).iter().any(|d| is_terminal(d.phase))
    });
    let deltas = deltas_for(&seen, &id);
    let text = deltas.iter().filter_map(|d| d.text_delta.clone()).collect();
    (text, deltas.last().unwrap().clone())
}

fn club(object: &str, sentence: &str) -> Candidate {
    Candidate::new("Book club", Predicate::Scheduled, object, sentence, 0.9)
}

/// The two book club notes, the second kept later; returns their items and memories.
fn book_club(h: &Harness) -> ((String, String), (String, String)) {
    let (first, a) = note(
        h,
        "2026-11-12T09:00:00Z",
        "Book club meets on the first Tuesday of the month at the library.",
        vec![club("first Tuesday of the month", CLUB1)],
    );
    let (second, b) = note(
        h,
        "2026-11-12T16:10:00Z",
        "Sam says book club is actually on the second Wednesday of the month at the community hall.",
        vec![club("second Wednesday of the month", CLUB2)],
    );
    assert_eq!(h.engine().open_disputes(&a).unwrap().len(), 1);
    ((first, a[0].clone()), (second, b[0].clone()))
}

#[test]
fn an_open_question_answered_with_one_side_gets_the_other() {
    let h = harness();
    let ((first, a), (second, b)) = book_club(&h);
    let (text, done) = ask(&h, "When does book club meet?");
    assert_eq!(done.phase, AskPhase::Done);
    assert_eq!(
        text,
        format!("{CLUB1} A later note says: {CLUB2} It is not settled which is right.")
    );
    assert_eq!(done.memory_ids, [a, b]);
    let items: Vec<&str> = done.receipts.iter().map(|r| r.item_id.as_str()).collect();
    assert_eq!(items, [first.as_str(), second.as_str()]);
}

/// What Tiny stored from the move note in two runs of three: the move with no new day, and the
/// new day read as when the receptionist called.
fn tiny_move_reading() -> Vec<Candidate> {
    vec![
        Candidate::new(
            "Dentist appointment",
            Predicate::Other("moved".into()),
            "moved",
            "The dentist appointment has been moved.",
            0.9,
        ),
        Candidate::new(
            "Dr Okafor's receptionist",
            Predicate::Other("called".into()),
            "at 14:30 on Thursday 19 November",
            "Dr Okafor's receptionist called at 14:30 on Thursday 19 November.",
            0.9,
        ),
        Candidate::new(
            "The call",
            Predicate::Happened,
            "Thursday 19 November at 14:30",
            "The call was made on Thursday 19 November at 14:30.",
            0.9,
        ),
    ]
}

/// The first dentist note, then the move note as Tiny read it; returns the old memory and the
/// move note's item.
fn dentist(h: &Harness) -> (String, String) {
    let mut wed = Candidate::new(
        "Dentist appointment",
        Predicate::Scheduled,
        "Wednesday 18 November at 10:00",
        WED,
        0.9,
    );
    wed.due = Some("2026-11-18".parse().unwrap());
    let (_, old) = note(
        h,
        "2026-11-12T08:05:00Z",
        "Dentist appointment with Dr Okafor on Wednesday 18 November at 10:00.",
        vec![wed],
    );
    let (moved, _) = note(h, "2026-11-12T12:00:00Z", MOVED, tiny_move_reading());
    (old[0].clone(), moved)
}

/// The mind is given the move as a move: the note's line in the old appointment's place, with
/// the old value as `was`, and no open question. Answered from the memory that says it moved,
/// the line that says where is added, its receipt the note.
#[test]
fn a_moved_appointment_reaches_the_mind_as_a_move() {
    let mind = Arc::new(OneSide(FakeMind::new(), Default::default()));
    let h = harness_with_mind(mind.clone());
    let (old, moved) = dentist(&h);
    let (text, done) = ask(&h, "When is my dentist appointment?");
    let given = mind.1.lock().unwrap()[0].clone();
    assert!(
        given
            .iter()
            .all(|f| f.memory.id != old && f.disputed_by.is_empty())
    );
    let line = given
        .iter()
        .find(|f| is_source_line(&f.memory.id))
        .expect("the note's line");
    assert_eq!(line.memory.sentence, NEW_LINE);
    assert_eq!(
        given
            .iter()
            .filter(|f| f.memory.sentence == NEW_LINE)
            .count(),
        1,
        "a line the search offered is not offered twice"
    );
    assert_eq!(
        line.memory.was.as_deref(),
        Some("Wednesday 18 November at 10:00")
    );
    assert_eq!(done.phase, AskPhase::Done);
    assert_eq!(
        text,
        format!("The dentist appointment has been moved. The later note says: {NEW_LINE}")
    );
    assert!(done.plain);
    assert!(done.receipts.iter().all(|r| r.item_id == moved));
}

/// A phone gets no line of an original: the memory that says it moved stands in, with the old
/// value as `was`.
#[test]
fn on_a_phone_the_move_is_the_memory_that_says_so() {
    use nocturne_app_lib::ask::{AskLane, AskOptions, AskSink, start_with};
    let mind = Arc::new(OneSide(FakeMind::new(), Default::default()));
    let h = harness_with_mind(mind.clone());
    let (old, _) = dentist(&h);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = h.app.handle().clone();
    let opts = AskOptions {
        public_only: true,
        lane: AskLane::Phone("phone-1".into()),
        sink: AskSink::Channel(tx),
        place: nocturne_contract::AskPlace::Auto,
    };
    tauri::async_runtime::block_on(async move {
        start_with(&handle, "When is my dentist appointment?".into(), opts)
    });
    let mut got: Vec<AskDelta> = Vec::new();
    wait_for("the phone's answer", || {
        while let Ok(d) = rx.try_recv() {
            got.push(d);
        }
        got.last().is_some_and(|d| is_terminal(d.phase))
    });
    let given = mind.1.lock().unwrap()[0].clone();
    assert!(
        given
            .iter()
            .all(|f| !is_source_line(&f.memory.id) && f.memory.id != old)
    );
    let notice = given
        .iter()
        .find(|f| f.memory.sentence == "The dentist appointment has been moved.")
        .unwrap();
    assert_eq!(
        notice.memory.was.as_deref(),
        Some("Wednesday 18 November at 10:00")
    );
    assert_eq!(got.last().unwrap().phase, AskPhase::Done);
}

#[cfg(feature = "online-stand-in")]
mod online {
    use super::*;
    use nocturne_app_lib::online::testing::{MemoryKeychain, StandIn, StandInPort, fake_provider};

    /// The first dentist note, then a move note that says only that it moved; returns the old
    /// memory.
    fn dentist_notice_only(h: &Harness) -> String {
        let mut wed = Candidate::new(
            "Dentist appointment",
            Predicate::Scheduled,
            "Wednesday 18 November at 10:00",
            WED,
            0.9,
        );
        wed.due = Some("2026-11-18".parse().unwrap());
        let (_, old) = note(
            h,
            "2026-11-12T08:05:00Z",
            "Dentist appointment with Dr Okafor on Wednesday 18 November at 10:00.",
            vec![wed],
        );
        note(
            h,
            "2026-11-12T12:00:00Z",
            "The dentist appointment has been moved.",
            vec![tiny_move_reading().remove(0)],
        );
        old[0].clone()
    }

    /// Online answers on, against the stand-in provider replying `reply`.
    pub struct Online {
        s: StandIn,
        port: Arc<StandInPort>,
    }

    pub fn turn_on(h: &Harness, reply: &str) -> Online {
        let s = StandIn::start(&fake_provider());
        let port = Arc::new(StandInPort::new(Some(s.port)));
        nocturne_app_lib::online::manage_hub(
            h.app.handle(),
            port.clone(),
            Arc::new(MemoryKeychain::default()),
        )
        .unwrap()
        .set_confirm(nocturne_app_lib::online::testing::agree(Default::default()));
        let out = h.ok(
            "check_online",
            json!({"form": {"baseUrl": "https://api.example.com/v1",
                            "key": "sk-links-test-0123456789abcdefghijklmn",
                            "model": "stand-in-model"}}),
        );
        assert_eq!(out["type"], "works", "{out}");
        h.ok(
            "turn_on_online",
            json!({"settings": {"phones": true, "sendSensitive": false,
                                "limit": {"kind": "questions", "questions": 200, "usd": null}}}),
        );
        let online = Online { s, port };
        tauri::async_runtime::block_on(async {
            reqwest::Client::new()
                .post(online.s.url("/fake/script"))
                .json(&json!({ "reply": reply }))
                .send()
                .await
                .unwrap();
        });
        online
    }

    impl Online {
        /// Everything the provider was sent, as text.
        pub fn sent(&self) -> String {
            let all: Vec<serde_json::Value> =
                self.port.own(tauri::async_runtime::block_on(async {
                    reqwest::Client::new()
                        .get(self.s.url("/fake/requests"))
                        .send()
                        .await
                        .unwrap()
                        .json()
                        .await
                        .unwrap()
                }));
            serde_json::to_string(&all).unwrap()
        }
    }

    /// With the other side private and kept back, the provider's answer gives one side and this
    /// computer adds the other. What was kept back never goes out.
    #[test]
    fn an_online_answer_gets_the_side_kept_back_added_here() {
        let h = harness();
        let ((_, a), (_, b)) = book_club(&h);
        h.engine().set_private(&b, true).unwrap();
        let online = turn_on(&h, &format!("{CLUB1} [m1]"));
        let (text, done) = ask(&h, "When does book club meet?");
        assert_eq!(done.phase, AskPhase::Done);
        assert!(matches!(
            done.answered_by,
            Some(nocturne_contract::AnsweredBy::Online { .. })
        ));
        assert_eq!(
            text,
            format!("{CLUB1} A later note says: {CLUB2} It is not settled which is right.")
        );
        assert_eq!(done.memory_ids, [a, b]);
        assert_eq!(
            done.left_out,
            Some(nocturne_contract::LeftOut::Private),
            "the mark says part stayed here"
        );
        assert!(!online.sent().contains("community hall"));
        let log = h.ok("list_online_sends", json!({"limit": 10, "before": null}));
        assert!(!log.to_string().contains("community hall"), "{log}");
    }

    /// The line of a note that says where the appointment went stays here; the provider's
    /// answer from the memory that says it moved gets it added on this computer, and the mark
    /// says something stayed here. Neither the request nor the Sent log has it.
    #[test]
    fn a_line_added_here_to_an_online_answer_is_marked_and_never_logged() {
        let h = harness();
        dentist(&h);
        let online = turn_on(&h, "The dentist appointment has been moved. [m1]");
        let (text, done) = ask(&h, "When is my dentist appointment?");
        assert!(matches!(
            done.answered_by,
            Some(nocturne_contract::AnsweredBy::Online { .. })
        ));
        assert!(text.ends_with(NEW_LINE), "{text}");
        assert_eq!(done.left_out, Some(nocturne_contract::LeftOut::Private));
        assert!(!online.sent().contains("14:30"));
        let log = h.ok("list_online_sends", json!({"limit": 10, "before": null}));
        assert!(!log.to_string().contains("14:30"), "{log}");
    }

    /// The old value a move carries here as `was` is another memory's: it never goes out, and
    /// the Sent log shows the memory exactly as it went.
    #[test]
    fn the_old_value_a_move_carries_never_goes_online() {
        let h = harness();
        dentist_notice_only(&h);
        let e = h.engine();
        let hits = e
            .retrieve("When is my dentist appointment?", None, 12)
            .unwrap();
        assert_eq!(
            e.later_moves(&hits).unwrap().len(),
            1,
            "a move, carrying `was`"
        );
        let online = turn_on(&h, "The dentist appointment has been moved. [m1]");
        ask(&h, "When is my dentist appointment?");
        let sent = online.sent();
        assert!(sent.contains("has been moved"), "{sent}");
        assert!(
            !sent.contains("18 November") && !sent.contains("10:00"),
            "{sent}"
        );
        let log = h.ok("list_online_sends", json!({"limit": 10, "before": null}));
        let lines = log[0]["lines"].as_array().unwrap();
        let line = lines
            .iter()
            .find(|l| l["type"] == "memory")
            .expect("the memory line");
        assert_eq!(line["state"], "same", "{log}");
        assert!(
            line["text"].as_str().unwrap().contains("has been moved"),
            "{log}"
        );
    }

    /// An old value that is a health detail in an earlier version stays here with sensitive
    /// details off, though the move that carries it goes out.
    #[test]
    fn a_sensitive_old_value_is_held_back_with_the_switch_off() {
        let h = harness();
        let mut tue = Candidate::new(
            "Dentist appointment",
            Predicate::Scheduled,
            "Tuesday 17 November at 9:00",
            "The dentist appointment is on Tuesday 17 November at 9:00.",
            0.9,
        );
        tue.due = Some("2026-11-17".parse().unwrap());
        tue.tags = vec!["health".into()];
        note(
            &h,
            "2026-11-11T09:00:00Z",
            "Dentist appointment Tuesday 17 November at 9:00.",
            vec![tue],
        );
        let mut wed = Candidate::new(
            "Dentist appointment",
            Predicate::Scheduled,
            "Wednesday 18 November at 10:00",
            "The dentist appointment moved to Wednesday 18 November at 10:00.",
            0.9,
        );
        wed.due = Some("2026-11-18".parse().unwrap());
        wed.moved = true;
        let (_, made) = note(
            &h,
            "2026-11-12T08:05:00Z",
            "The dentist appointment moved to Wednesday 18 November at 10:00.",
            vec![wed],
        );
        assert!(
            h.memory(&made[0]).was.is_some(),
            "the health version is history"
        );
        note(
            &h,
            "2026-11-12T12:00:00Z",
            "The dentist appointment has been moved.",
            vec![tiny_move_reading().remove(0)],
        );
        let e = h.engine();
        let hits = e
            .retrieve("When is my dentist appointment?", None, 12)
            .unwrap();
        assert_eq!(
            e.later_moves(&hits).unwrap().len(),
            1,
            "a move, carrying `was`"
        );
        let online = turn_on(&h, "The dentist appointment has been moved. [m1]");
        ask(&h, "When is my dentist appointment?");
        let sent = online.sent();
        assert!(sent.contains("has been moved"), "{sent}");
        assert!(
            !sent.contains("18 November") && !sent.contains("17 November"),
            "{sent}"
        );
    }
}
