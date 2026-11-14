//! The stage scores and the answer-only harness of the real run (`e2e_real.rs`), checked without
//! models: a perfect reading of the seeded day and a scripted mind.

mod common;
mod eval;

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use common::*;
use eval::*;
use nocturne_contract::PageView;
use nocturne_engine::{
    AnswerMeta, BoxFuture, Candidate, ExtractContext, Fact, FakeMind, Mind, MindError, Transcript,
    Understanding,
};
use serde_json::{Value, json};
use tauri::async_runtime::block_on;
use tokio::sync::mpsc;

/// A scripted mind that reports each answer it writes as a raw reply, as the real one does.
struct Reporting(FakeMind);

impl Mind for Reporting {
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
        question: &'a str,
        facts: &'a [Fact],
        out: mpsc::Sender<String>,
    ) -> BoxFuture<'a, Result<AnswerMeta, MindError>> {
        Box::pin(async move {
            let reply: String = facts
                .iter()
                .enumerate()
                .map(|(n, f)| format!("{} [m{}]", f.memory.sentence, n + 1))
                .collect::<Vec<_>>()
                .join(" ");
            tracing::trace!(target: "nocturne::reply", step = "answer", reply = %reply);
            self.0.answer(question, facts, out).await
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

/// The tap, installed once for this test binary. It hears every test's replies, so the tests that
/// read it hold this lock while they run.
static TAPPED: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn tap() -> &'static ReplyTap {
    static TAP: OnceLock<ReplyTap> = OnceLock::new();
    TAP.get_or_init(|| {
        use tracing_subscriber::Layer;
        use tracing_subscriber::layer::SubscriberExt;
        let tap = ReplyTap::default();
        let subscriber = tracing_subscriber::registry().with(tap.clone().with_filter(
            tracing_subscriber::filter::filter_fn(|m| m.target() == REPLY_TARGET),
        ));
        tracing::subscriber::set_global_default(subscriber).expect("one subscriber");
        tap
    })
}

fn fixture_world(
    mind: Arc<dyn Mind>,
) -> (Harness, std::collections::HashMap<&'static str, String>) {
    let h = harness_with_mind(mind.clone());
    let ids = seed_fixture(&h.engine(), &h.clock);
    assert!(block_on(embed_all(&h.engine(), mind.as_ref())) > 0);
    h.clock.set_str(&format!("{DAY}T22:30:00Z"));
    (h, ids)
}

fn ask(h: &Harness, question: &str) -> Asked {
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
    let deltas = deltas_for(&seen, &id);
    let end = deltas.last().unwrap();
    Asked {
        phase: serde_json::to_value(end.phase)
            .unwrap()
            .as_str()
            .unwrap()
            .to_string(),
        text: deltas.iter().filter_map(|d| d.text_delta.clone()).collect(),
        memory_ids: end.memory_ids.clone(),
        cited_items: end.receipts.iter().map(|r| r.item_id.clone()).collect(),
        answered_by: serde_json::to_value(&end.answered_by).unwrap(),
        left_out: serde_json::to_value(end.left_out).unwrap(),
    }
}

#[test]
fn a_perfect_reading_holds_every_gold_word_and_forms_both_moves() {
    let h = harness_with_mind(Arc::new(FakeMind::new()));
    let ids = seed_fixture(&h.engine(), &h.clock);
    assert_eq!(ids.len(), GOLD.len());
    let e = extraction(&h.engine(), &ids);
    assert_eq!(e["found"], e["of"], "{e:#}");
    assert!(e["of"].as_u64().unwrap() >= 40, "{e:#}");
    let l = links(&h.engine(), &ids);
    assert_eq!(l["moves"][0]["linked"], true, "{l:#}");
    assert_eq!(l["moves"][1]["linked"], true, "{l:#}");
    assert!(l["formed"].as_u64().unwrap() >= 2);
}

#[test]
fn coverage_names_what_reading_missed() {
    let row = coverage(
        "boiler",
        &["The boiler service is on Monday 30 November.".to_string()],
    );
    assert_eq!(row["found"], 1);
    assert_eq!(row["missing"], json!(["marek"]));
    let row = coverage(
        "photo",
        &["My parking permit is valid until 30 JUNE 2027.".to_string()],
    );
    assert_eq!(row["missing"], json!(["4471", "c7"]));
    let row = coverage(
        "doc",
        &["Our plot is number 17 | Allotment | 17".to_string()],
    );
    assert_eq!(row["missing"], json!(["1 december", "15 january"]));
    assert!(has_word("plot 17.", "17") && !has_word("in 2017", "17"));
    assert_eq!(coverage("nothing-like-it", &[]), Value::Null);
}

#[test]
fn judging_follows_the_question() {
    let q = |id: &str| QUESTIONS.iter().find(|q| q.id == id).unwrap();
    assert!(judge(
        q("q01"),
        "done",
        "**Marek** is coming on Monday 30 November."
    ));
    assert!(!judge(q("q01"), "done", "Mr Hughes is coming on Monday."));
    assert!(!judge(
        q("q01"),
        "notSure",
        "Marek is coming on 30 November."
    ));
    assert!(judge(q("q07"), "notSure", ""));
    assert!(judge(q("q07"), "nothing", ""));
    assert!(!judge(q("q07"), "done", "It is 0123."));
    let kept_back = Asked {
        phase: "notSure".into(),
        answered_by: json!({"type": "online", "host": "api.example.com"}),
        left_out: json!("sensitive"),
        ..Asked::default()
    };
    assert_eq!(policy(&kept_back), Some("keptBackNotSure"));
    let here = Asked {
        phase: "done".into(),
        answered_by: json!({"type": "computer", "reason": "keptHere"}),
        ..Asked::default()
    };
    assert_eq!(policy(&here), Some("answeredHereAfterKeptBack"));
    assert_eq!(policy(&Asked::default()), None);
}

#[test]
fn the_search_puts_every_source_in_the_first_twelve() {
    let mind: Arc<dyn Mind> = Arc::new(FakeMind::new());
    let (h, ids) = fixture_world(mind.clone());
    let (mut at4, mut at12, mut asked) = (0, 0, 0);
    for q in QUESTIONS.iter().filter(|q| !q.unknown) {
        let wanted: HashSet<String> = q
            .sources
            .iter()
            .filter_map(|k| ids.get(k).cloned())
            .collect();
        let vector =
            block_on(nocturne_app_lib::ask::embed_question(mind.as_ref(), q.text)).unwrap();
        let hits = h.engine().retrieve(q.text, Some(&vector), 12).unwrap();
        let rank = source_rank(&hits, &wanted);
        asked += 1;
        at4 += usize::from(rank.is_some_and(|r| r <= 4));
        at12 += usize::from(rank.is_some());
        assert!(
            rank.is_some(),
            "{}: {:?}",
            q.id,
            hits.iter().map(|h| &h.memory.sentence).collect::<Vec<_>>()
        );
    }
    assert_eq!(at12, asked);
    assert!(at4 <= at12);
}

/// What a run of [`answer_only`] must hold, whichever way the questions were asked.
fn check_runs(result: &Value) {
    let rows = result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2 * QUESTIONS.len());
    let per_run: Vec<u64> = result["correct_per_run"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    assert_eq!(per_run.len(), 2);
    assert_eq!(per_run[0], per_run[1], "frozen facts and a scripted mind");
    assert_eq!(result["median"], per_run[0]);
    for row in rows {
        for key in [
            "run", "id", "phase", "text", "raw_text", "correct", "policy",
        ] {
            assert!(row.get(key).is_some(), "{key} in {row}");
        }
        if row["phase"] == "done" {
            let raw = row["raw_replies"].as_array().unwrap();
            assert_eq!(raw.len(), 1, "{row}");
            assert_eq!(raw[0]["step"], "answer");
            assert!(raw[0]["reply"].as_str().unwrap().contains("[m1]"));
        }
    }
    // The scripted mind says every fact it is given, so the facts decide: the moved dentist
    // appointment, the plot number and the permit number are all there to be said.
    let correct = |id: &str| {
        result["correct_per_question"]
            .as_array()
            .unwrap()
            .iter()
            .find(|q| q["id"] == id)
            .unwrap()["correct"]
            .as_u64()
            .unwrap()
    };
    for id in ["q01", "q02", "q04", "q05", "q10"] {
        assert_eq!(correct(id), 2, "{id}: {result:#}");
    }
}

#[test]
fn the_answer_step_alone_gets_only_the_frozen_facts_and_keeps_the_raw_replies() {
    let _alone = TAPPED.lock().unwrap_or_else(|e| e.into_inner());
    let tap = tap();
    let mind: Arc<dyn Mind> = Arc::new(Reporting(FakeMind::new()));
    let (h, ids) = fixture_world(mind.clone());
    let q = |id: &str| QUESTIONS.iter().find(|q| q.id == id).unwrap();
    let boiler = frozen_facts(&h.engine(), &ids, q("q01"));
    assert_eq!(
        boiler.len(),
        2,
        "the boiler note's two memories, nothing searched"
    );
    assert!(
        boiler
            .iter()
            .all(|f| f.memory.receipts.iter().all(|r| r.item_id == ids["boiler"]))
    );
    let never = frozen_facts(&h.engine(), &ids, q("q07"));
    assert!(!never.is_empty());
    assert!(never.iter().all(|f| {
        f.memory
            .receipts
            .iter()
            .all(|r| r.item_id == ids["cinema"] || r.item_id == ids["club2"])
    }));
    let asked_before = h.state().asks.in_flight();
    let result = answer_only(2, &ids, Some(tap), |q| {
        let facts = frozen_facts(&h.engine(), &ids, q);
        block_on(answer_alone(mind.as_ref(), q.text, &facts))
    });
    assert_eq!(h.state().asks.in_flight(), asked_before);
    check_runs(&result);
}

#[test]
fn the_frozen_store_asked_through_the_app_repeats_and_keeps_the_raw_replies() {
    let _alone = TAPPED.lock().unwrap_or_else(|e| e.into_inner());
    let tap = tap();
    let (h, ids) = fixture_world(Arc::new(Reporting(FakeMind::new())));
    let result = answer_only(2, &ids, Some(tap), |q| ask(&h, q.text));
    check_runs(&result);
}
