//! What helps an answer when reading was imperfect or something had to stay here: lines of an
//! original that no memory says, the reranking step falling back to the search's own order, and
//! this computer answering when an online "not sure" lacked what was kept back.

mod common;

use std::sync::Arc;

use common::*;
use nocturne_app_lib::online::testing::{MemoryKeychain, StandIn, StandInPort, fake_provider};
use nocturne_contract::{AnsweredBy, AskDelta, AskPhase, LocalReason, Span};
use nocturne_engine::{BoxFuture, Candidate, Hit, Mind, MindError, Predicate, is_source_line};
use nocturne_lemonade::Client;
use nocturne_lemonade::rerank::Reranker;
use serde_json::{Value, json};
use tauri::async_runtime::block_on;

const KEY: &str = "sk-quality-test-0123456789abcdefghijklmn";
const BOILER: &str =
    "Boiler service is booked for Monday 30 November. The engineer is called Marek.";
const MAREK: &str = "The engineer is called Marek.";

fn boiler(h: &Harness) -> (String, String) {
    let c = Candidate::new(
        "Boiler service",
        Predicate::Scheduled,
        "Monday 30 November",
        "The boiler service is booked for Monday 30 November.",
        0.9,
    );
    let (item, mut created) = h.learn_candidate(BOILER, c);
    (item, created.remove(0))
}

fn ask(h: &Harness, question: &str) -> Vec<AskDelta> {
    let seen = h.ask_deltas();
    let id = h
        .ok("ask", json!({"question": question}))
        .as_str()
        .unwrap()
        .to_string();
    wait_for("the answer", || {
        deltas_for(&seen, &id).iter().any(|d| is_terminal(d.phase))
    });
    wait_for("the ask to end", || h.state().asks.in_flight() == 0);
    deltas_for(&seen, &id)
}

#[test]
fn a_line_reading_missed_answers_with_its_original_as_the_receipt() {
    let mind = TestMind::new(AnswerMode::Normal);
    let h = harness_with_mind(mind.clone());
    let (item, memory) = boiler(&h);
    let d = ask(&h, "Who is coming to service the boiler, and when?");
    let given = mind.answered_from.lock().unwrap()[0].clone();
    assert_eq!(given[0].memory.id, memory);
    assert_eq!(given[1].memory.sentence, MAREK);
    assert!(is_source_line(&given[1].memory.id));
    let done = d.last().unwrap();
    assert_eq!(done.phase, AskPhase::Done);
    assert_eq!(
        done.memory_ids,
        std::slice::from_ref(&memory),
        "a line is no memory"
    );
    assert!(done.plain, "bold would stand for memories one for one");
    assert_eq!(done.receipts.len(), 2);
    assert!(done.receipts.iter().all(|r| r.item_id == item));
    assert!(
        done.receipts
            .iter()
            .any(|r| r.span == Some(Span::Quote { text: MAREK.into() }))
    );
    assert!(h.memory(&memory).last_used.is_some());
}

/// A phone's question never gets a line of an original: only memories, which have been checked
/// for what may leave this computer.
#[test]
fn no_line_of_an_original_reaches_a_phone() {
    use nocturne_app_lib::ask::{AskLane, AskOptions, AskSink, start_with};
    let mind = TestMind::new(AnswerMode::Normal);
    let h = harness_with_mind(mind.clone());
    let (_, memory) = boiler(&h);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = h.app.handle().clone();
    let opts = AskOptions {
        public_only: true,
        lane: AskLane::Phone("phone-1".into()),
        sink: AskSink::Channel(tx),
        place: nocturne_contract::AskPlace::Auto,
        help: false,
    };
    block_on(async move {
        start_with(
            &handle,
            "Who is coming to service the boiler, and when?".into(),
            opts,
        )
    });
    let mut got: Vec<AskDelta> = Vec::new();
    wait_for("the phone's answer", || {
        while let Ok(d) = rx.try_recv() {
            got.push(d);
        }
        got.last().is_some_and(|d| is_terminal(d.phase))
    });
    let done = got.last().unwrap();
    assert_eq!(done.phase, AskPhase::Done);
    assert_eq!(done.memory_ids, std::slice::from_ref(&memory));
    let given = mind.answered_from.lock().unwrap()[0].clone();
    assert!(given.iter().all(|f| !is_source_line(&f.memory.id)));
    let text: String = got.iter().filter_map(|x| x.text_delta.clone()).collect();
    assert!(!text.contains("Marek"), "{text}");
    assert!(done.near.iter().all(|n| !n.text.contains("Marek")));
}

#[test]
fn an_original_with_no_memory_can_still_answer() {
    let h = harness_with_mind(TestMind::new(AnswerMode::Normal));
    let e = h.engine();
    let item = match e
        .keep_text(
            "Riverside Allotment Society newsletter.\nOur plot is number 17, next to the tap.",
            nocturne_contract::Source::QuickCapture,
        )
        .unwrap()
    {
        nocturne_contract::Kept::New { item } | nocturne_contract::Kept::Duplicate { item } => {
            item.id
        }
    };
    let p = e
        .lease_item(&item, std::time::Duration::from_secs(60))
        .unwrap();
    e.apply_candidates(&item, &p.lease, Vec::new()).unwrap();
    let d = ask(&h, "What is our allotment plot number?");
    let done = d.last().unwrap();
    assert_eq!(done.phase, AskPhase::Done);
    assert!(done.memory_ids.is_empty());
    assert!(!done.receipts.is_empty());
    assert!(done.receipts.iter().all(|r| r.item_id == item));
    let text: String = d.iter().filter_map(|x| x.text_delta.clone()).collect();
    assert!(text.contains("number 17"), "{text}");
}

#[test]
fn the_search_order_stands_when_the_reranking_model_cannot_be_reached() {
    let h = harness_with_mind(TestMind::new(AnswerMode::Normal));
    for n in 0..10 {
        h.learn(
            &format!("note {n}"),
            &format!("The swimming lesson {n} is on Monday."),
            "Swimming",
            &format!("lesson {n}"),
        );
    }
    let hits: Vec<Hit> = h.engine().retrieve("swimming lesson", None, 12).unwrap();
    assert_eq!(hits.len(), 10);
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let reranker = Reranker::new(Client::new(closed, "k"), "bge-reranker-v2-m3-GGUF");
    let kept = block_on(nocturne_app_lib::ask::rerank_with(
        &reranker,
        "swimming lesson",
        hits.clone(),
    ));
    let ids = |v: &[Hit]| v.iter().map(|h| h.memory.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&kept), ids(&hits));
}

/// A mind with an instructed query embedding, which marks the vectors it makes for questions.
struct Instructed(nocturne_engine::FakeMind);

impl Mind for Instructed {
    fn transcribe<'a>(
        &'a self,
        audio: &'a std::path::Path,
    ) -> BoxFuture<'a, Result<nocturne_engine::Transcript, MindError>> {
        self.0.transcribe(audio)
    }

    fn read_image<'a>(
        &'a self,
        image: &'a std::path::Path,
    ) -> BoxFuture<'a, Result<nocturne_engine::Understanding, MindError>> {
        self.0.read_image(image)
    }

    fn read_document<'a>(
        &'a self,
        doc: &'a std::path::Path,
    ) -> BoxFuture<'a, Result<nocturne_engine::Understanding, MindError>> {
        self.0.read_document(doc)
    }

    fn extract<'a>(
        &'a self,
        text: &'a str,
        cx: &'a nocturne_engine::ExtractContext,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
        self.0.extract(text, cx)
    }

    fn embed<'a>(&'a self, texts: &'a [String]) -> BoxFuture<'a, Result<Vec<Vec<f32>>, MindError>> {
        self.0.embed(texts)
    }

    fn embed_query<'a>(&'a self, _: &'a str) -> BoxFuture<'a, Result<Vec<f32>, MindError>> {
        Box::pin(async { Ok(vec![-1.0]) })
    }

    fn answer<'a>(
        &'a self,
        question: &'a str,
        facts: &'a [nocturne_engine::Fact],
        out: tokio::sync::mpsc::Sender<String>,
    ) -> BoxFuture<'a, Result<nocturne_engine::AnswerMeta, MindError>> {
        self.0.answer(question, facts, out)
    }

    fn write_headline<'a>(
        &'a self,
        page: &'a nocturne_contract::PageView,
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

#[test]
fn the_question_is_embedded_as_a_search_query() {
    let q = "When is the plumber coming?";
    let plain = nocturne_engine::FakeMind::new();
    let direct = block_on(plain.embed(&[q.to_string()])).unwrap().remove(0);
    let adapted = block_on(nocturne_app_lib::ask::embed_question(&plain, q)).unwrap();
    assert_eq!(
        adapted, direct,
        "a mind without a query form embeds it as it is"
    );
    let instructed = Instructed(nocturne_engine::FakeMind::new());
    let adapted = block_on(nocturne_app_lib::ask::embed_question(&instructed, q)).unwrap();
    assert_eq!(adapted, vec![-1.0], "the mind's query form is used");
}

// Online answers and what was kept back ----------------------------------------------------

struct Lab {
    h: Harness,
    s: StandIn,
    port: Arc<StandInPort>,
}

fn lab(mind: Arc<dyn Mind>) -> Lab {
    let h = harness_with_mind(mind);
    let s = StandIn::start(&fake_provider());
    let port = Arc::new(StandInPort::new(Some(s.port)));
    nocturne_app_lib::online::manage_hub(
        h.app.handle(),
        port.clone(),
        Arc::new(MemoryKeychain::default()),
    )
    .unwrap()
    .set_confirm(nocturne_app_lib::online::testing::agree(Default::default()));
    // As on the consent step: the person has seen that only what is marked private stays here.
    h.ok("see_online_change", json!({}));
    Lab { h, s, port }
}

impl Lab {
    fn script(&self, body: Value) {
        block_on(async {
            reqwest::Client::new()
                .post(self.s.url("/fake/script"))
                .json(&body)
                .send()
                .await
                .unwrap();
        });
    }

    /// Questions sent, without the connection check's.
    fn questions(&self) -> Vec<Value> {
        let all: Vec<Value> = self.port.own(block_on(async {
            reqwest::Client::new()
                .get(self.s.url("/fake/requests"))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap()
        }));
        all.into_iter()
            .filter(|r| r["path"] == "/v1/chat/completions")
            .filter(|c| {
                c["body"]["messages"]
                    .as_array()
                    .is_some_and(|m| m.len() == 2)
            })
            .collect()
    }

    fn turn_on(&self) {
        let out = self.h.ok(
            "check_online",
            json!({"form": {"baseUrl": "https://api.example.com/v1", "key": KEY,
                            "model": "stand-in-model"}}),
        );
        assert_eq!(out["type"], "works", "{out}");
        let view = self.h.ok(
            "turn_on_online",
            json!({"settings": {"phones": true,
                                "limit": {"kind": "questions", "questions": 200, "usd": null}}}),
        );
        assert_eq!(view["state"]["type"], "on", "{view}");
    }
}

fn here(reason: LocalReason) -> Option<AnsweredBy> {
    Some(AnsweredBy::Computer {
        reason: Some(reason),
    })
}

/// The dentist note, and a medicine the person marked private.
fn dentist_and_pills(l: &Lab) -> String {
    l.h.learn(
        "the dentist note",
        "The dentist is on Thursday.",
        "Dentist",
        "Thursday",
    );
    let c = Candidate::new(
        "Dentist",
        Predicate::Takes,
        "pills",
        "The dentist gave PILLMARK pills.",
        0.9,
    );
    let id = l.h.learn_candidate("pills", c).1.remove(0);
    l.h.engine().set_private(&id, true).unwrap();
    id
}

/// A private line goes with the question like any other, so a provider that is not sure has
/// had everything this computer has, and the question is not handed back.
#[test]
fn not_sure_online_with_a_private_line_sent_stays_not_sure() {
    let mind = TestMind::new(AnswerMode::Normal);
    let l = lab(mind.clone());
    dentist_and_pills(&l);
    l.turn_on();
    l.script(json!({"reply": "NOT_SURE"}));
    let d = ask(&l.h, "What did the dentist give me?");
    let done = d.last().unwrap();
    assert_eq!(done.phase, AskPhase::NotSure);
    assert_eq!(
        done.answered_by,
        Some(AnsweredBy::Online {
            host: "api.example.com".into()
        })
    );
    assert_eq!(done.left_out, None);
    let questions = l.questions();
    assert_eq!(questions.len(), 1, "asked online once, never again");
    assert!(
        serde_json::to_string(&questions)
            .unwrap()
            .contains("PILLMARK"),
        "the private line went with the question"
    );
    assert!(mind.answered_from.lock().unwrap().is_empty());
}

#[test]
fn not_sure_online_without_lines_that_stayed_here_is_answered_here() {
    let mind = TestMind::new(AnswerMode::Normal);
    let l = lab(mind.clone());
    boiler(&l.h);
    l.turn_on();
    l.script(json!({"reply": "NOT_SURE"}));
    let d = ask(&l.h, "Who is coming to service the boiler, and when?");
    let done = d.last().unwrap();
    assert_eq!(done.phase, AskPhase::Done);
    assert_eq!(done.answered_by, here(LocalReason::NotSureOnline));
    assert!(done.send_id.is_some());
    let sent = serde_json::to_string(&l.questions()).unwrap();
    assert!(!sent.contains("Marek"), "{sent}");
    let given = mind.answered_from.lock().unwrap()[0].clone();
    assert!(given.iter().any(|f| f.memory.sentence == MAREK));
    let text: String = d.iter().filter_map(|x| x.text_delta.clone()).collect();
    assert!(text.contains("Marek"), "{text}");
}

#[test]
fn not_sure_online_with_nothing_kept_back_stays_not_sure() {
    let mind = TestMind::new(AnswerMode::Normal);
    let l = lab(mind.clone());
    l.h.learn(
        "the dentist note",
        "The dentist is on Thursday.",
        "Dentist",
        "Thursday",
    );
    l.turn_on();
    l.script(json!({"reply": "NOT_SURE"}));
    let d = ask(&l.h, "When is the dentist?");
    assert_eq!(d.last().unwrap().phase, AskPhase::NotSure);
    assert!(mind.answered_from.lock().unwrap().is_empty());
}

#[test]
fn a_line_of_an_original_never_goes_online() {
    let l = lab(TestMind::new(AnswerMode::Normal));
    boiler(&l.h);
    l.turn_on();
    l.script(json!({"reply": "The boiler service is on **Monday** [m1]."}));
    let d = ask(&l.h, "Who is coming to service the boiler, and when?");
    let done = d.last().unwrap();
    assert_eq!(
        done.answered_by,
        Some(AnsweredBy::Online {
            host: "api.example.com".into()
        })
    );
    assert_eq!(done.left_out, None);
    let sent = serde_json::to_string(&l.questions()).unwrap();
    assert!(sent.contains("Monday 30 November"));
    assert!(!sent.contains("Marek"), "{sent}");
}
