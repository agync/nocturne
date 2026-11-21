//! Decisions against the fake engine: the `/v1/systemone` wire, the decider over it and the mind
//! answering from a fixed list.

mod common;

use std::time::Duration;

use common::Fake;
use nocturne_contract::Tier;
use nocturne_engine::decide::{Ask, Backend, Decider, Decision, Question};
use nocturne_engine::{Mind, MindError};
use nocturne_lemonade::client::{SystemOneKind, SystemOneQuestion, SystemOneRequest};
use nocturne_lemonade::decide::SystemOneDecider;
use nocturne_lemonade::{LemonadeMind, RuntimeError, Supervisor, SupervisorOptions};
use serde_json::{Value, json};

const GGUF: &str = "Decision-Lite-GGUF";

fn only(mut v: Vec<Value>) -> Value {
    assert_eq!(v.len(), 1, "{v:?}");
    v.remove(0)
}

fn decision(ask: Ask) -> Decision {
    Decision {
        state: "Dentist moved to Friday.".into(),
        question: Question {
            instructions: "What happened to the appointment?".into(),
            ask,
        },
    }
}

fn choice() -> Ask {
    Ask::Choice(vec![
        ("moved".into(), "Now at another time".into()),
        ("same".into(), "Unchanged".into()),
    ])
}

fn score() -> Ask {
    Ask::Score(vec!["low".into(), "mid".into(), "high".into()])
}

async fn script_systemone(fake: &Fake, items: Value) {
    fake.http
        .post(fake.url("/fake/script"))
        .bearer_auth("k3y")
        .json(&json!({ "systemone": items }))
        .send()
        .await
        .unwrap();
}

fn decider(fake: &Fake, model: &str) -> SystemOneDecider {
    SystemOneDecider::new(fake.client.clone(), model, Backend::LlamaCpp)
}

#[tokio::test]
async fn systemone_sends_the_questions_as_the_engine_takes_them() {
    let fake = Fake::started(Some("downloaded=all")).await;
    let req = SystemOneRequest {
        model: GGUF.into(),
        state: "Dentist moved to Friday.".into(),
        questions: [(
            "q".to_string(),
            SystemOneQuestion {
                kind: SystemOneKind::Choice,
                instructions: "What happened?".into(),
                criteria: Some(json!({"moved": "Now at another time", "same": null})),
            },
        )]
        .into(),
    };
    let reply = fake.client.systemone(&req).await.unwrap();
    let sent = only(fake.seen("/v1/systemone").await);
    assert_eq!(sent["method"], "POST");
    assert_eq!(
        sent["body"],
        json!({
            "model": GGUF,
            "state": "Dentist moved to Friday.",
            "questions": {"q": {
                "type": "choice",
                "instructions": "What happened?",
                "criteria": {"moved": "Now at another time", "same": null},
            }},
        })
    );
    let a = &reply.answers["q"];
    assert_eq!(a.choice.as_deref(), Some("moved"));
    assert_eq!(a.probabilities.as_ref().unwrap()["moved"], 0.6);
}

#[tokio::test]
async fn yes_no_is_a_noul_question_read_as_yes_and_no() {
    let fake = Fake::started(Some("downloaded=all")).await;
    let d = decider(&fake, GGUF);
    let a = d.decide(&decision(Ask::YesNo)).await.unwrap();
    let sent = only(fake.seen("/v1/systemone").await);
    assert_eq!(sent["body"]["questions"]["q"]["type"], "noul");
    assert!(sent["body"]["questions"]["q"].get("criteria").is_none());
    assert_eq!((a.label.as_str(), a.by.as_str()), ("yes", GGUF));
    assert_eq!((a.p("yes"), a.p("no")), (0.5, 0.5));

    script_systemone(&fake, json!([{"answer": {"type": "noul", "noul": 0.2}}])).await;
    let a = d.decide(&decision(Ask::YesNo)).await.unwrap();
    assert_eq!(a.label, "no");
    assert!((a.p("yes") - 0.2).abs() < 1e-9 && (a.p("no") - 0.8).abs() < 1e-9);
    assert_eq!(d.backend(), Backend::LlamaCpp);
}

#[tokio::test]
async fn a_choice_sends_its_descriptions_and_reads_probabilities_by_option() {
    let fake = Fake::started(Some("downloaded=all")).await;
    let d = decider(&fake, GGUF);
    let a = d.decide(&decision(choice())).await.unwrap();
    let sent = only(fake.seen("/v1/systemone").await);
    assert_eq!(sent["body"]["questions"]["q"]["type"], "choice");
    assert_eq!(
        sent["body"]["questions"]["q"]["criteria"],
        json!({"moved": "Now at another time", "same": "Unchanged"})
    );
    assert_eq!(a.label, "moved");
    assert_eq!((a.p("moved"), a.p("same")), (0.6, 0.4));

    script_systemone(
        &fake,
        json!([{"answer": {"type": "choice", "choice": "same",
            "probabilities": {"moved": 0.1, "same": 0.9}}}]),
    )
    .await;
    let a = d.decide(&decision(choice())).await.unwrap();
    assert_eq!((a.label.as_str(), a.p("same")), ("same", 0.9));
}

#[tokio::test]
async fn a_score_maps_level_indexes_back_to_the_levels() {
    let fake = Fake::started(Some("downloaded=all")).await;
    let d = decider(&fake, GGUF);
    script_systemone(
        &fake,
        json!([{"answer": {"type": "score", "score": 1.6,
            "probabilities": {"0": 0.1, "1": 0.2, "2": 0.7}}}]),
    )
    .await;
    let a = d.decide(&decision(score())).await.unwrap();
    let sent = only(fake.seen("/v1/systemone").await);
    assert_eq!(sent["body"]["questions"]["q"]["type"], "score");
    assert_eq!(
        sent["body"]["questions"]["q"]["criteria"],
        json!(["low", "mid", "high"])
    );
    assert_eq!(a.label, "high");
    assert_eq!((a.p("low"), a.p("mid"), a.p("high")), (0.1, 0.2, 0.7));
}

#[tokio::test]
async fn the_engine_refuses_what_it_cannot_serve() {
    let fake = Fake::started(Some("downloaded=all")).await;
    let ask = |model: &str| SystemOneRequest {
        model: model.into(),
        state: "x".into(),
        questions: [(
            "q".to_string(),
            SystemOneQuestion {
                kind: SystemOneKind::Noul,
                instructions: "Is it?".into(),
                criteria: None,
            },
        )]
        .into(),
    };
    assert!(matches!(
        fake.client.systemone(&ask("Qwen3.5-2B-GGUF")).await,
        Err(RuntimeError::Rejected { status: 400, .. })
    ));
    assert!(matches!(
        fake.client.systemone(&ask("no-such-model")).await,
        Err(RuntimeError::NotFound(_))
    ));
    for body in [
        json!({"model": GGUF, "questions": {"q": {"type": "noul", "instructions": "i"}}}),
        json!({"model": GGUF, "state": "x"}),
    ] {
        let r = fake
            .http
            .post(fake.url("/v1/systemone"))
            .bearer_auth("k3y")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400, "{body}");
    }
}

#[tokio::test]
async fn pull_registered_sends_the_registration_and_lists_the_model() {
    let fake = Fake::started(None).await;
    let job = fake
        .client
        .pull_registered(
            "user.decider",
            "owner/repo:model-q4.gguf",
            "llamacpp",
            &["classification", "systemone"],
        )
        .await
        .unwrap();
    assert_eq!(job.id, "model:user.decider");
    let sent = only(fake.seen("/v1/pull").await);
    assert_eq!(
        sent["body"],
        json!({
            "model_name": "user.decider",
            "checkpoint": "owner/repo:model-q4.gguf",
            "recipe": "llamacpp",
            "labels": ["classification", "systemone"],
            "stream": true,
            "subscribe": false,
        })
    );
    let listed = fake.client.models().await.unwrap();
    let m = listed.iter().find(|m| m.id == "user.decider").unwrap();
    assert_eq!(m.recipe, "llamacpp");
    assert_eq!(m.labels, ["classification", "systemone"]);
}

#[tokio::test]
async fn a_decider_never_pulls_a_model_that_is_not_on_disk() {
    let fake = Fake::started(None).await;
    let d = decider(&fake, GGUF);
    assert!(matches!(
        d.decide(&decision(Ask::YesNo)).await,
        Err(MindError::Unavailable)
    ));
    assert!(fake.seen("/v1/systemone").await.is_empty());
    assert!(fake.seen("/v1/pull").await.is_empty());
    let stats = fake.peek("/fake/stats").await;
    assert_eq!(stats["pulls"], 0);
    assert_eq!(stats["fetched_on_use"], json!([]));
}

#[tokio::test]
async fn a_fastflowlm_decider_reports_its_backend() {
    let fake = Fake::started(Some("downloaded=all,npu=ready")).await;
    let d = SystemOneDecider::new(fake.client.clone(), "d3-lite-FLM", Backend::Flm);
    let a = d.decide(&decision(choice())).await.unwrap();
    assert_eq!((d.backend(), a.by.as_str()), (Backend::Flm, "d3-lite-FLM"));
}

#[tokio::test]
async fn a_question_the_engine_could_not_answer_is_a_failure() {
    let fake = Fake::started(Some("downloaded=all")).await;
    let d = decider(&fake, GGUF);
    script_systemone(
        &fake,
        json!([
            {"answer": {"type": "choice", "error": "max_length_exceeded", "message": "too long"}},
            {"answer": {"type": "choice", "choice": "elsewhere",
                "probabilities": {"elsewhere": 1.0}}},
            {"status": 500, "error": "backend down"},
        ]),
    )
    .await;
    for _ in 0..3 {
        assert!(matches!(
            d.decide(&decision(choice())).await,
            Err(MindError::Failed(_))
        ));
    }
}

struct Rig {
    sup: Supervisor,
    mind: LemonadeMind,
    _tmp: tempfile::TempDir,
}

async fn rig() -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    common::set_mode(&dirs, "downloaded=all");
    let sup = Supervisor::start(
        dirs,
        SupervisorOptions {
            health_interval: Duration::from_millis(200),
            ..SupervisorOptions::default()
        },
    )
    .await
    .unwrap();
    let tier = Tier::Standard;
    let picks =
        nocturne_lemonade::selection::picks(&nocturne_lemonade::selection::Selection::preset(tier));
    let mind = LemonadeMind::new(sup.client(), picks, tier);
    Rig {
        sup,
        mind,
        _tmp: tmp,
    }
}

async fn script_chat(r: &Rig, content: &str) {
    let e = r.sup.client().endpoint().unwrap();
    reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/fake/script", e.port))
        .bearer_auth(e.key.clone())
        .json(&json!({"chat": [{"content": content}]}))
        .send()
        .await
        .unwrap();
}

async fn chats(r: &Rig) -> Vec<Value> {
    let e = r.sup.client().endpoint().unwrap();
    let all: Value = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{}/fake/requests", e.port))
        .bearer_auth(e.key.clone())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    all.as_array()
        .unwrap()
        .iter()
        .filter(|q| q["path"] == "/v1/chat/completions")
        .cloned()
        .collect()
}

#[tokio::test]
async fn the_mind_answers_with_a_label_from_a_fixed_list() {
    let r = rig().await;
    script_chat(&r, r#"{"answer": "same"}"#).await;
    let a = r.mind.decide(&decision(choice())).await.unwrap();
    assert_eq!(a.label, "same");
    assert_eq!(a.p("same"), 1.0);

    let sent = only(chats(&r).await);
    let body = &sent["body"];
    assert_eq!(body["enable_thinking"], false);
    assert_eq!(body["temperature"], 0.0);
    let format = &body["response_format"]["json_schema"]["schema"];
    assert_eq!(
        format["properties"]["answer"]["enum"],
        json!(["moved", "same"])
    );
    assert_eq!(format["required"], json!(["answer"]));
    assert_eq!(format["additionalProperties"], false);
    let prompt = body["messages"].to_string();
    for needle in [
        "Dentist moved to Friday.",
        "What happened to the appointment?",
        "moved: Now at another time",
        "same: Unchanged",
    ] {
        assert!(prompt.contains(needle), "{needle} missing from {prompt}");
    }
    assert!(!prompt.contains("systemone"));
}

#[tokio::test]
async fn the_mind_reads_yes_and_no_and_scores_from_their_labels() {
    let r = rig().await;
    script_chat(&r, r#"{"answer": "no"}"#).await;
    let a = r.mind.decide(&decision(Ask::YesNo)).await.unwrap();
    assert_eq!(a.label, "no");
    script_chat(&r, r#"{"answer": "mid"}"#).await;
    let a = r.mind.decide(&decision(score())).await.unwrap();
    assert_eq!(a.label, "mid");
}

#[tokio::test]
async fn the_mind_fails_on_an_answer_outside_the_list() {
    let r = rig().await;
    script_chat(&r, r#"{"answer": "perhaps"}"#).await;
    assert!(matches!(
        r.mind.decide(&decision(choice())).await,
        Err(MindError::Failed(_))
    ));
}
