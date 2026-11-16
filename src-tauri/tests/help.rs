//! Help with Nocturne from the ask box: the guide joins answers on this computer, the unsure
//! prompt, the guide's own words on a size that has not passed the help check, and the rules
//! (never a memory, never on a phone). The help check itself runs here on a scripted mind.

mod common;
mod eval;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::*;
use eval::help::*;
use nocturne_app_lib::ask::{AskLane, AskOptions, AskSink, start_with};
use nocturne_contract::{AskDelta, AskPhase, HelpKind, MemoryFilter, PageView};
use nocturne_engine::{
    AnswerMeta, BoxFuture, Candidate, ExtractContext, Fact, FakeMind, Mind, MindError, Transcript,
    Understanding, is_guide_section,
};
use serde_json::json;
use tokio::sync::mpsc;

/// A scripted mind that says each fact it is given, and keeps the reply it would have written,
/// with its markers, and the facts in the order it got them.
#[derive(Default)]
struct Recording {
    mind: FakeMind,
    said: Mutex<Vec<(String, Vec<String>)>>,
}

impl Recording {
    fn last(&self) -> Option<(String, Vec<String>)> {
        self.said.lock().unwrap().last().cloned()
    }
}

impl Mind for Recording {
    fn transcribe<'a>(
        &'a self,
        audio: &'a std::path::Path,
    ) -> BoxFuture<'a, Result<Transcript, MindError>> {
        self.mind.transcribe(audio)
    }

    fn read_image<'a>(
        &'a self,
        image: &'a std::path::Path,
    ) -> BoxFuture<'a, Result<Understanding, MindError>> {
        self.mind.read_image(image)
    }

    fn read_document<'a>(
        &'a self,
        doc: &'a std::path::Path,
    ) -> BoxFuture<'a, Result<Understanding, MindError>> {
        self.mind.read_document(doc)
    }

    fn extract<'a>(
        &'a self,
        text: &'a str,
        cx: &'a ExtractContext,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
        self.mind.extract(text, cx)
    }

    /// No meaning vectors: the guide and the memories are found by their words, so the scripted
    /// run is not swayed by a hash that stands in for meaning.
    fn embed<'a>(&'a self, _: &'a [String]) -> BoxFuture<'a, Result<Vec<Vec<f32>>, MindError>> {
        Box::pin(async { Err(MindError::Unsupported("no vectors here".into())) })
    }

    fn answer<'a>(
        &'a self,
        question: &'a str,
        facts: &'a [Fact],
        out: mpsc::Sender<String>,
    ) -> BoxFuture<'a, Result<AnswerMeta, MindError>> {
        Box::pin(async move {
            let reply = facts
                .iter()
                .enumerate()
                .map(|(n, f)| format!("{} [m{}]", f.memory.sentence, n + 1))
                .collect::<Vec<_>>()
                .join(" ");
            let ids = facts.iter().map(|f| f.memory.id.clone()).collect();
            self.said.lock().unwrap().push((reply, ids));
            self.mind.answer(question, facts, out).await
        })
    }

    fn write_headline<'a>(
        &'a self,
        page: &'a PageView,
    ) -> BoxFuture<'a, Result<(String, String), MindError>> {
        self.mind.write_headline(page)
    }

    fn speak<'a>(
        &'a self,
        text: &'a str,
        voice: &'a str,
    ) -> BoxFuture<'a, Result<Vec<u8>, MindError>> {
        self.mind.speak(text, voice)
    }
}

fn world() -> (Harness, Arc<Recording>, HashMap<&'static str, String>) {
    let mind = Arc::new(Recording::default());
    let h = harness_with_mind(mind.clone());
    let mut ids = HashMap::new();
    for s in SEEDS {
        let (_, id) = h.learn(s.note, s.sentence, s.subject, s.object);
        ids.insert(s.key, id);
    }
    (h, mind, ids)
}

struct Asked {
    end: AskDelta,
    text: String,
    first_sentence: Option<Duration>,
}

fn ask(h: &Harness, question: &str, help: bool) -> Asked {
    let seen = h.ask_deltas();
    let t0 = Instant::now();
    let id = h
        .ok("ask", json!({"question": question, "help": help}))
        .as_str()
        .unwrap()
        .to_string();
    wait_for("the answer", || {
        deltas_for(&seen, &id).iter().any(|d| is_terminal(d.phase))
    });
    wait_for("the ask to end", || h.state().asks.in_flight() == 0);
    let deltas = deltas_for(&seen, &id);
    Asked {
        end: deltas.last().unwrap().clone(),
        text: deltas.iter().filter_map(|d| d.text_delta.clone()).collect(),
        first_sentence: deltas
            .iter()
            .any(|d| d.phase == AskPhase::Streaming)
            .then(|| t0.elapsed()),
    }
}

fn kind(a: &Asked) -> Option<HelpKind> {
    a.end.help.as_ref().map(|h| h.kind)
}

fn sections(a: &Asked) -> Vec<String> {
    a.end
        .help
        .as_ref()
        .map(|h| h.sections.iter().map(|s| s.id.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn a_confident_match_shows_the_guides_own_words_until_the_size_passes() {
    let (h, _, _) = world();
    let a = ask(&h, "How do I get Nocturne on my phone?", false);
    assert_eq!(kind(&a), Some(HelpKind::Words), "{:?}", a.end);
    let help = a.end.help.as_ref().unwrap();
    assert_eq!(
        sections(&a),
        [
            "guide:away-from-home#turn-it-on",
            "guide:away-from-home#add-a-phone"
        ]
    );
    assert!(!help.opening.is_empty());
    assert!(help.sections.iter().all(|s| !s.steps.is_empty()));
    assert!(help.cited.is_empty(), "nothing was written from the guide");
    assert!(!help.online_on);
    // The memory answer is the normal one, beside the guide's words.
    assert!(
        a.end.memory_ids.iter().all(|id| !is_guide_section(id)),
        "{:?}",
        a.end
    );
}

#[test]
fn a_confident_match_on_a_size_that_passed_is_written_from_the_guide_and_what_was_kept() {
    let (h, mind, ids) = world();
    h.state().help.set_writes(Some(true));
    let a = ask(&h, "How do I get rid of the photo of Gran's letter?", false);
    assert_eq!(a.end.phase, AskPhase::Done, "{:?}", a.end);
    assert_eq!(kind(&a), Some(HelpKind::Written));
    let help = a.end.help.as_ref().unwrap();
    assert!(sections(&a).contains(&"guide:your-data#let-go-bring-back-forget".to_string()));
    assert!(a.end.memory_ids.contains(&ids["gran_letter"]));
    assert!(
        a.end.memory_ids.iter().all(|id| !is_guide_section(id)),
        "the actions act only on memories"
    );
    assert!(help.cited.iter().any(|id| is_guide_section(id)));
    assert!(help.cited.contains(&ids["gran_letter"]));
    // Guide receipts are the help's sections; every receipt on the delta is a kept original.
    let letter = h.memory(&ids["gran_letter"]);
    assert!(
        a.end
            .receipts
            .iter()
            .all(|r| letter.receipts.iter().any(|x| x.item_id == r.item_id)
                || !r.item_id.starts_with("guide"))
    );
    let (_, facts) = mind.last().unwrap();
    assert!(
        is_guide_section(&facts[0]),
        "the guide's sections come first among the facts: {facts:?}"
    );
    assert!(a.text.contains("Delete this…"), "{}", a.text);
}

#[test]
fn an_unsure_match_keeps_the_memory_answer_and_offers_the_guide() {
    let (h, _, _) = world();
    let a = ask(&h, "The three words on my phone", false);
    assert_eq!(kind(&a), Some(HelpKind::Unsure), "{:?}", a.end);
    assert_eq!(sections(&a).len(), 1);
    // Asked again for the guide's answer, the nearest section answers.
    h.state().help.set_writes(Some(true));
    let again = ask(&h, "The three words on my phone", true);
    assert_eq!(kind(&again), Some(HelpKind::Written), "{:?}", again.end);
    assert_eq!(again.end.phase, AskPhase::Done);
}

#[test]
fn a_memory_question_that_shares_words_with_the_guide_gets_no_help() {
    let (h, _, ids) = world();
    h.state().help.set_writes(Some(true));
    let a = ask(&h, "What is Gran's new phone number?", false);
    assert_eq!(a.end.phase, AskPhase::Done);
    assert!(
        !matches!(kind(&a), Some(HelpKind::Written | HelpKind::Words)),
        "{:?}",
        a.end.help
    );
    assert!(a.end.memory_ids.contains(&ids["gran_phone"]));
    for q in [
        "What is Gran's new address?",
        "Can you tell me when the bins go out?",
        "Can I see Gran's phone number?",
        "When did Leo say he lost his phone?",
        "Did Gran remind me about the party?",
        "Can Leo connect the printer?",
        "What's the wifi passphrase?",
        "Where's the backup drive?",
        "How do I get rid of the ants?",
    ] {
        let a = ask(&h, q, false);
        assert!(
            !matches!(kind(&a), Some(HelpKind::Written | HelpKind::Words)),
            "{q}: {:?}",
            a.end.help
        );
    }
}

#[test]
fn the_guide_is_never_a_memory() {
    let (h, _, _) = world();
    h.state().help.set_writes(Some(true));
    let e = h.engine();
    let all = || {
        e.memories(MemoryFilter {
            entity_id: None,
            item_id: None,
            status: None,
            query: None,
            limit: None,
        })
        .unwrap()
    };
    let (memories, items) = (all(), e.stats().unwrap());
    let used: Vec<Option<String>> = memories.iter().map(|m| m.last_used.clone()).collect();
    let question = "How do I take Nocturne off my computer?";
    let mut cited: Vec<String> = Vec::new();
    for q in [question, "Where has the little icon by the clock gone?"] {
        let a = ask(&h, q, false);
        cited.extend(a.end.memory_ids.clone());
        assert_eq!(kind(&a), Some(HelpKind::Written), "{q}: {:?}", a.end);
        assert!(a.end.memory_ids.iter().all(|id| !is_guide_section(id)));
        for r in &a.end.receipts {
            assert!(
                e.get_item(&r.item_id).is_ok(),
                "a receipt is something kept"
            );
        }
    }
    let after = all();
    assert_eq!(after.len(), memories.len(), "nothing was kept");
    for (m, before) in after.iter().zip(&used) {
        if !cited.contains(&m.id) {
            assert_eq!(
                &m.last_used, before,
                "citing the guide is no use of anything"
            );
        }
    }
    assert!(after.iter().all(|m| !is_guide_section(&m.id)));
    assert_eq!(e.stats().unwrap(), items);
    let hits = e.retrieve("Add a phone Away from home", None, 12).unwrap();
    assert!(hits.iter().all(|h| !is_guide_section(&h.memory.id)));
}

#[test]
fn a_paired_phone_never_gets_help() {
    let (h, _, _) = world();
    h.state().help.set_writes(Some(true));
    let (tx, mut rx) = mpsc::unbounded_channel::<AskDelta>();
    let handle = h.app.handle().clone();
    let opts = AskOptions {
        public_only: true,
        lane: AskLane::Phone("phone-1".into()),
        sink: AskSink::Channel(tx),
        place: nocturne_contract::AskPlace::Auto,
        help: true,
    };
    tauri::async_runtime::block_on(async move {
        start_with(&handle, "How do I get Nocturne on my phone?".into(), opts)
    });
    let deltas = tauri::async_runtime::block_on(async {
        let mut out = Vec::new();
        while let Ok(Some(d)) = tokio::time::timeout(Duration::from_secs(60), rx.recv()).await {
            let end = is_terminal(d.phase);
            out.push(d);
            if end {
                break;
            }
        }
        out
    });
    assert!(deltas.last().is_some_and(|d| is_terminal(d.phase)));
    assert!(deltas.iter().all(|d| d.help.is_none()), "{deltas:?}");
}

#[test]
fn the_help_check_passes_on_the_scripted_mind() {
    let (h, mind, ids) = world();
    h.state().help.set_writes(Some(true));
    let report = run_check(RUNS, &ids, |text| {
        let before = mind.said.lock().unwrap().len();
        let a = ask(&h, text, false);
        let said = match mind.said.lock().unwrap().get(before) {
            Some((reply, facts)) => cited_sentences(reply, facts),
            None => Vec::new(),
        };
        let (help, sections) = help_of(&serde_json::to_value(&a.end.help).unwrap());
        HelpAnswer {
            phase: format!("{:?}", a.end.phase),
            help,
            sections,
            memory_ids: a.end.memory_ids.clone(),
            said,
            first_sentence: a.first_sentence,
        }
    });
    assert_eq!(report["pass"], true, "{report:#}");
}

#[test]
fn the_check_fails_an_answer_that_blends() {
    let guide: HashMap<String, String> = [(
        "guide:a#b".to_string(),
        "1. Choose Add a phone. 2. Compare the three words.".to_string(),
    )]
    .into();
    let kept: HashMap<String, String> = [(
        "mem_1".to_string(),
        "Gran's new phone number is 07700 900123.".to_string(),
    )]
    .into();
    let ids: HashMap<&'static str, String> = HashMap::new();
    let controls = ["Add a phone"];
    let src = Sources {
        guide: &guide,
        kept: &kept,
        ids: &ids,
        controls: &controls,
    };
    let run = |said: Vec<(String, Vec<String>)>| {
        let mut run = HashMap::new();
        run.insert(
            "h01",
            HelpAnswer {
                help: "written".into(),
                said,
                ..HelpAnswer::default()
            },
        );
        score(&[run], &src)
    };
    let one = |s: &str, id: &str| vec![(s.to_string(), vec![id.to_string()])];
    let blended = run(one("Choose Add Gran's phone.", "guide:a#b"));
    assert_eq!(blended["runs"][0]["checks"]["noBlending"]["sentences"], 1);
    let steps_in_memory = run(one("Choose Add a phone for 07700 900123.", "mem_1"));
    assert_eq!(
        steps_in_memory["runs"][0]["checks"]["noBlending"]["sentences"],
        1
    );
    let misspelt = run(one("Choose Add A Phone.", "guide:a#b"));
    assert_eq!(misspelt["runs"][0]["checks"]["faithfulSteps"]["misses"], 1);
    let clean = run(one("Choose Add a phone.", "guide:a#b"));
    assert_eq!(clean["runs"][0]["checks"]["noBlending"]["sentences"], 0);
    assert_eq!(clean["runs"][0]["checks"]["faithfulSteps"]["misses"], 0);
}

#[test]
fn markers_are_read_per_sentence() {
    let facts = vec!["guide:a#b".to_string(), "mem_1".to_string()];
    let said = cited_sentences(
        "Choose **Add a phone** [m1]. Gran's number is 07700 900123. [m2] Nothing here.",
        &facts,
    );
    assert_eq!(said.len(), 3, "{said:?}");
    assert_eq!(said[0].1, ["guide:a#b"]);
    assert_eq!(said[1].1, ["mem_1"]);
    assert!(said[2].1.is_empty());
}
