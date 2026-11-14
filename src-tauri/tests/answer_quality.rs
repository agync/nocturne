//! What helps an answer when reading was imperfect: lines of an original that no memory says.

mod common;

use std::sync::Arc;

use common::*;
use nocturne_app_lib::online::testing::{MemoryKeychain, StandIn, StandInPort, fake_provider};
use nocturne_contract::{AnsweredBy, AskDelta, AskPhase, Span};
use nocturne_engine::{Candidate, Mind, Predicate, is_source_line};
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

// Online answers ----------------------------------------------------

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
            json!({"settings": {"phones": true, "sendSensitive": false,
                                "limit": {"kind": "questions", "questions": 200, "usd": null}}}),
        );
        assert_eq!(view["state"]["type"], "on", "{view}");
    }
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
