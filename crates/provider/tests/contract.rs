//! What the mind sends to the person's provider, and where: the shape of a reading, an answer
//! and a page; one host and nothing else; nothing at all until it is asked.

use std::sync::{Arc, Mutex};

use nocturne_contract::{CheckOutcome, EntityKind, ItemKind, MemoryView, PageView};
use nocturne_engine::{ExtractContext, Fact, Mind, MindError, OnlineFailure};
use nocturne_mind::{online, prompts, read};
use nocturne_provider::address::{Lookup, OnlineKey, UrlProblem};
use nocturne_provider::stand_in::{Answer, KEY, MODEL, StandIn};
use nocturne_provider::{Exchange, Job, Provider, ProviderMind};
use serde_json::{Value, json};
use tokio::sync::mpsc;

fn provider(stand_in: &StandIn) -> Provider {
    Provider::new(&stand_in.url(), MODEL, OnlineKey::new(KEY.into())).unwrap()
}

fn mind(stand_in: &StandIn) -> ProviderMind {
    ProviderMind::new(provider(stand_in))
}

fn cx() -> ExtractContext {
    ExtractContext {
        item_kind: ItemKind::Text,
        kept_at: "2026-03-02T09:00:00Z".parse().unwrap(),
        today: "2026-03-02".parse().unwrap(),
        known_entities: vec![("Ana".into(), EntityKind::Person)],
    }
}

fn memory(id: &str, sentence: &str, private: bool) -> MemoryView {
    serde_json::from_value(json!({
        "id": id, "sentence": sentence, "subject": "Sam", "entityId": null,
        "predicate": "scheduled", "object": "x", "certainty": "sure", "status": "active",
        "validFrom": null, "validTo": null, "due": null, "learnedAt": "2026-11-10T09:30:00+00:00",
        "lastUsed": null, "letGoAt": null, "keepForever": false, "private": private,
        "was": null, "receipts": [],
    }))
    .unwrap()
}

fn page() -> PageView {
    use nocturne_contract::{Edition, PageVariant};
    PageView {
        date: "2026-11-14".into(),
        page_number: 2,
        edition: Edition::Screen,
        variant: PageVariant::Normal,
        headline: String::new(),
        dek: String::new(),
        needs_you: Vec::new(),
        changed: Vec::new(),
        let_go: Vec::new(),
        upcoming: Vec::new(),
        learned: Vec::new(),
        question: None,
        more_needs_you: 0,
        more_changed: 0,
        kept_count: 4,
        changes_count: 0,
        let_go_count: 0,
        catch_up_days: 0,
        private_skipped: 0,
        pending: 0,
        tidied_from: None,
        tidied_to: None,
        update: None,
        journal_line: None,
        journal_skipped: None,
        calendar_today: Vec::new(),
        more_calendar_today: 0,
        last_night: None,
    }
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or_default()
}

#[tokio::test]
async fn nothing_is_sent_until_the_mind_is_asked() {
    let stand_in = StandIn::start().await;
    let mind = mind(&stand_in);
    assert!(mind.embed(&["a line".into()]).await.is_err());
    assert!(mind.embed_query("a question").await.is_err());
    assert!(mind.speak("words", "voice").await.is_err());
    assert!(matches!(
        mind.read_document(std::path::Path::new("letter.pdf"))
            .await
            .unwrap_err(),
        MindError::NotTaken(_)
    ));
    assert_eq!(mind.extract("   ", &cx()).await.unwrap(), Vec::new());
    let (tx, _rx) = mpsc::channel(4);
    let none = mind.answer("Where?", &[], tx).await.unwrap();
    assert!(none.not_sure);
    assert!(stand_in.seen().is_empty(), "{:?}", stand_in.seen());
}

#[tokio::test]
async fn a_reading_sends_the_item_with_the_reading_prompt_and_schema() {
    let stand_in = StandIn::start().await;
    stand_in.then(Answer::text(prompts::EXAMPLE_REPLY));
    let found = mind(&stand_in)
        .extract(prompts::EXAMPLE_NOTE, &cx())
        .await
        .unwrap();
    assert_eq!(found.len(), 3);
    let chats = stand_in.chats();
    assert_eq!(chats.len(), 1);
    let sent = &chats[0];
    assert_eq!(sent.method, "POST");
    assert_eq!(sent.authorization.as_deref(), Some("Bearer stand-in-key"));
    let body = &sent.body;
    assert_eq!(body["model"], MODEL);
    assert_eq!(body["stream"], false);
    assert_eq!(body["temperature"], 0.0);
    assert_eq!(
        body["max_tokens"],
        json!(read::extract_tokens(prompts::EXAMPLE_NOTE))
    );
    assert_eq!(
        body["response_format"],
        prompts::response_format(&prompts::extraction_schema())
    );
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "user");
    let nonce_free = |s: &str| s.split_whitespace().take(8).collect::<Vec<_>>().join(" ");
    assert_eq!(
        nonce_free(text(&messages[0]["content"])),
        nonce_free(&prompts::extraction_system("n"))
    );
    assert!(text(&messages[1]["content"]).contains(prompts::EXAMPLE_NOTE));
    let keys: Vec<&String> = body.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        [
            "max_tokens",
            "messages",
            "model",
            "response_format",
            "stream",
            "temperature"
        ]
    );
}

#[tokio::test]
async fn a_reading_with_invalid_memories_is_asked_again_for_those_alone() {
    let stand_in = StandIn::start().await;
    let bad = r#"{"memories": [{"span": {"quote": "Dr Patel"}, "sentence": "", "subject": ""}]}"#;
    stand_in.then(Answer::text(bad));
    stand_in.then(Answer::text(prompts::EXAMPLE_REPLY));
    let found = mind(&stand_in)
        .extract(prompts::EXAMPLE_NOTE, &cx())
        .await
        .unwrap();
    assert_eq!(found.len(), 3);
    let chats = stand_in.chats();
    assert_eq!(chats.len(), 2);
    let again = chats[1].body["messages"].as_array().unwrap();
    assert_eq!(again.len(), 4);
    assert_eq!(again[2]["role"], "assistant");
    assert_eq!(again[2]["content"], bad);
    assert!(text(&again[3]["content"]).starts_with("That was not valid: memory 1:"));
}

#[tokio::test]
async fn a_refused_schema_is_dropped_once_and_for_all() {
    let stand_in = StandIn::start().await;
    stand_in.then(Answer::Status(
        400,
        "response_format is not supported".into(),
    ));
    stand_in.always(Answer::text(prompts::EXAMPLE_REPLY));
    let mind = mind(&stand_in);
    mind.extract(prompts::EXAMPLE_NOTE, &cx()).await.unwrap();
    mind.extract(prompts::EXAMPLE_NOTE, &cx()).await.unwrap();
    let chats = stand_in.chats();
    assert_eq!(chats.len(), 3);
    assert!(chats[0].body.get("response_format").is_some());
    assert!(chats[1].body.get("response_format").is_none());
    assert!(chats[2].body.get("response_format").is_none());
}

#[tokio::test]
async fn an_answer_sends_the_question_with_its_numbered_lines_and_keeps_a_receipt() {
    let stand_in = StandIn::start().await;
    stand_in.then(Answer::text("The bins go out on Tuesday [m1]."));
    let record: Arc<Mutex<Vec<(Job, Exchange)>>> = Arc::default();
    let mind = mind(&stand_in).with_record({
        let record = record.clone();
        Arc::new(move |job, exchange| record.lock().unwrap().push((job, exchange.clone())))
    });
    let facts = vec![
        Fact::from(memory("mem_1", "The bins go out on Tuesday.", false)),
        // Marked private: on the person's own home it goes like anything else.
        Fact::from(memory("mem_2", "Sam's key is under the blue pot.", true)),
    ];
    let (tx, mut rx) = mpsc::channel(16);
    let meta = mind
        .answer("When do the bins go out?", &facts, tx)
        .await
        .unwrap();
    assert!(!meta.not_sure);
    assert_eq!(meta.cited, vec!["mem_1".to_string()]);
    let mut shown = String::new();
    while let Ok(part) = rx.try_recv() {
        shown.push_str(&part);
    }
    assert!(shown.contains("Tuesday"), "{shown}");

    let chats = stand_in.chats();
    assert_eq!(chats.len(), 1);
    let body = &chats[0].body;
    assert_eq!(body["model"], MODEL);
    assert_eq!(body["temperature"], json!(0.2f32));
    assert_eq!(body["max_tokens"], online::MAX_TOKENS);
    assert!(body.get("response_format").is_none());
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    let user = text(&messages[1]["content"]);
    assert!(user.contains("When do the bins go out?"));
    assert!(user.contains("The bins go out on Tuesday."));
    assert!(user.contains("Sam's key is under the blue pot."));

    let receipt = meta.online.expect("a receipt");
    assert_eq!(receipt.status, Some(200));
    assert_eq!(receipt.sent_ids, vec!["mem_1", "mem_2"]);
    assert_eq!(receipt.line_hashes.len(), 2);
    assert_eq!(receipt.bytes_up, chats[0].bytes as u64);
    assert!(receipt.bytes_down > 0);
    assert_eq!(receipt.cost_usd, Some(0.0001));
    let record = record.lock().unwrap();
    assert_eq!(record.len(), 1);
    assert_eq!(record[0].0, Job::Answer);
    assert_eq!(record[0].1.bytes_up, receipt.bytes_up);
}

#[tokio::test]
async fn an_answer_whose_receipt_is_not_a_line_sent_is_not_sure() {
    let stand_in = StandIn::start().await;
    stand_in.then(Answer::text("The bins go out on Friday [m7]."));
    let facts = vec![Fact::from(memory(
        "mem_1",
        "The bins go out on Tuesday.",
        false,
    ))];
    let (tx, _rx) = mpsc::channel(16);
    let meta = mind(&stand_in).answer("Bins?", &facts, tx).await.unwrap();
    assert!(meta.not_sure);
    assert!(meta.online.is_some());
}

#[tokio::test]
async fn a_failed_answer_says_why() {
    let stand_in = StandIn::start().await;
    for (status, want) in [
        (401, OnlineFailure::KeyRefused),
        (402, OnlineFailure::NoCredit),
        (404, OnlineFailure::ModelMissing),
        (429, OnlineFailure::Busy),
        (503, OnlineFailure::Other(503)),
    ] {
        stand_in.then(Answer::Status(status, "{}".into()));
        let facts = vec![Fact::from(memory(
            "mem_1",
            "The bins go out on Tuesday.",
            false,
        ))];
        let (tx, _rx) = mpsc::channel(16);
        let err = mind(&stand_in)
            .answer("Bins?", &facts, tx)
            .await
            .unwrap_err();
        assert_eq!(err, MindError::Online(want), "{status}");
    }
}

#[tokio::test]
async fn the_page_sends_its_lines_for_a_headline() {
    let stand_in = StandIn::start().await;
    stand_in.then(Answer::text(
        r#"{"headline": "A quiet Friday", "dek": "Four things kept."}"#,
    ));
    let (headline, dek) = mind(&stand_in).write_headline(&page()).await.unwrap();
    assert_eq!(
        (headline.as_str(), dek.as_str()),
        ("A quiet Friday", "Four things kept.")
    );
    let chats = stand_in.chats();
    assert_eq!(chats.len(), 1);
    let body = &chats[0].body;
    assert_eq!(body["temperature"], json!(0.4f32));
    assert_eq!(body["max_tokens"], 200);
    assert_eq!(
        body["response_format"],
        prompts::response_format(&prompts::headline_schema())
    );
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "user");
}

#[tokio::test]
async fn a_photo_goes_with_its_image_and_waits_when_photos_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let photo = dir.path().join("receipt.png");
    let img = image::RgbImage::from_pixel(4, 4, image::Rgb([200, 10, 10]));
    img.save(&photo).unwrap();

    let stand_in = StandIn::start().await;
    stand_in.then(Answer::text(
        r#"{"title": "Receipt", "text": "Total 12.40", "summary": "A shop receipt.", "language": "en"}"#,
    ));
    let mind = mind(&stand_in);
    let u = mind.read_image(&photo).await.unwrap();
    assert_eq!(
        (u.title.as_str(), u.text.as_str()),
        ("Receipt", "Total 12.40")
    );
    let sent = &stand_in.chats()[0].body;
    let parts = sent["messages"][1]["content"].as_array().unwrap();
    assert_eq!(parts[0]["text"], prompts::IMAGE_ASK);
    assert!(text(&parts[1]["image_url"]["url"]).starts_with("data:image/png;base64,"));

    let refusing = StandIn::start().await;
    refusing.always(Answer::Status(
        400,
        "this model does not take images".into(),
    ));
    let mind = self::mind(&refusing);
    // One refused photo waits; it may be that photo alone.
    let err = mind.read_image(&photo).await.unwrap_err();
    assert!(matches!(err, MindError::NotTaken(_)), "{err:?}");
    assert_eq!(
        refusing.chats().len(),
        2,
        "once by schema, once by the prompt"
    );
    // A second in a row: the provider takes none, and none is sent again.
    assert!(matches!(
        mind.read_image(&photo).await.unwrap_err(),
        MindError::NotTaken(_)
    ));
    let asked = refusing.chats().len();
    assert_eq!(
        asked, 4,
        "the schema is still asked for: the retry without it failed too"
    );
    assert!(matches!(
        mind.read_image(&photo).await.unwrap_err(),
        MindError::NotTaken(_)
    ));
    assert_eq!(refusing.chats().len(), asked, "not asked again");
}

#[tokio::test]
async fn a_thing_the_provider_refuses_is_that_things_failure_and_the_rest_is_read() {
    let stand_in = StandIn::start().await;
    stand_in.then(Answer::Status(413, "too large".into()));
    stand_in.always(Answer::text(prompts::EXAMPLE_REPLY));
    let mind = mind(&stand_in);
    let err = mind.extract("A very long note.", &cx()).await.unwrap_err();
    assert!(matches!(err, MindError::Failed(_)), "{err:?}");
    assert_eq!(
        mind.extract(prompts::EXAMPLE_NOTE, &cx())
            .await
            .unwrap()
            .len(),
        3
    );
    let chats = stand_in.chats();
    assert_eq!(chats.len(), 2);
    assert!(
        chats[1].body.get("response_format").is_some(),
        "the schema was not blamed"
    );

    let wordy = StandIn::start().await;
    wordy.always(Answer::Status(
        400,
        "This model's maximum context length is exceeded".into(),
    ));
    let err = self::mind(&wordy)
        .extract("A note.", &cx())
        .await
        .unwrap_err();
    assert!(matches!(err, MindError::NotTaken(_)), "{err:?}");

    for status in [401, 402, 429, 503] {
        let away = StandIn::start().await;
        away.always(Answer::Status(status, "{}".into()));
        let err = self::mind(&away)
            .extract("A note.", &cx())
            .await
            .unwrap_err();
        assert!(matches!(err, MindError::Online(_)), "{status}: {err:?}");
    }
}

#[tokio::test]
async fn stopping_drops_a_request_under_way() {
    let stand_in = StandIn::start().await;
    stand_in.always(Answer::Hang);
    let mind = Arc::new(mind(&stand_in));
    let asking = tokio::spawn({
        let mind = mind.clone();
        async move { mind.extract("A note.", &cx()).await }
    });
    while stand_in.chats().is_empty() {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    mind.provider().stop();
    let err = tokio::time::timeout(std::time::Duration::from_secs(5), asking)
        .await
        .expect("dropped at once")
        .unwrap()
        .unwrap_err();
    assert_eq!(err, MindError::Online(OnlineFailure::Refused));
    assert_eq!(stand_in.chats().len(), 1);
}

#[tokio::test]
async fn nothing_leaves_once_stopped() {
    let stand_in = StandIn::start().await;
    let mind = mind(&stand_in);
    mind.provider().stop();
    let facts = vec![Fact::from(memory(
        "mem_1",
        "The bins go out on Tuesday.",
        false,
    ))];
    let (tx, _rx) = mpsc::channel(16);
    assert!(mind.answer("Bins?", &facts, tx).await.is_err());
    assert!(mind.extract(prompts::EXAMPLE_NOTE, &cx()).await.is_err());
    assert!(stand_in.seen().is_empty());
}

#[tokio::test]
async fn a_redirect_is_never_followed() {
    let stand_in = StandIn::start().await;
    let elsewhere = StandIn::start().await;
    stand_in.always(Answer::Redirect(format!(
        "{}/chat/completions",
        elsewhere.url()
    )));
    let facts = vec![Fact::from(memory(
        "mem_1",
        "The bins go out on Tuesday.",
        false,
    ))];
    let (tx, _rx) = mpsc::channel(16);
    let err = mind(&stand_in)
        .answer("Bins?", &facts, tx)
        .await
        .unwrap_err();
    assert_eq!(err, MindError::Online(OnlineFailure::Refused));
    assert_eq!(stand_in.chats().len(), 1);
    assert!(elsewhere.seen().is_empty());
}

fn stub(addr: &'static str) -> Lookup {
    Arc::new(move |_: String, _: u16| Box::pin(async move { Ok(vec![addr.parse().unwrap()]) }))
}

#[tokio::test]
async fn a_name_on_the_own_network_that_points_outside_it_is_never_sent_to_over_http() {
    let stand_in = StandIn::start().await;
    let url = format!("http://gpubox:{}/v1", stand_in.addr.port());
    let outside = Provider::new(&url, MODEL, OnlineKey::new(KEY.into()))
        .unwrap()
        .with_lookup(stub("8.8.8.8"));
    let (outcome, made) = outside.check().await;
    assert_eq!(outcome, CheckOutcome::AddressNotAllowed);
    assert!(made.is_empty());
    assert!(stand_in.seen().is_empty());
}

/// A resolver whose answer changes on every call: the addresses in turn, then the last.
fn changing(answers: &'static [&'static str], calls: Arc<Mutex<usize>>) -> Lookup {
    Arc::new(move |_: String, _: u16| {
        let n = {
            let mut calls = calls.lock().unwrap();
            *calls += 1;
            *calls - 1
        };
        let addr = answers[n.min(answers.len() - 1)];
        Box::pin(async move { Ok(vec![addr.parse().unwrap()]) })
    })
}

#[tokio::test]
async fn the_address_judged_is_the_address_connected_to() {
    let stand_in = StandIn::start().await;
    let url = format!("http://gpubox:{}/v1", stand_in.addr.port());
    let calls = Arc::new(Mutex::new(0));
    let lookup = changing(&["8.8.8.8", "127.0.0.1", "169.254.169.254"], calls.clone());
    let make = || {
        Provider::new(&url, MODEL, OnlineKey::new(KEY.into()))
            .unwrap()
            .with_lookup(lookup.clone())
    };
    // Outside the own network over plain HTTP: no address is handed over, nothing leaves.
    let (outcome, made) = make().check().await;
    assert_eq!(outcome, CheckOutcome::AddressNotAllowed);
    assert!(made.is_empty());
    assert!(stand_in.seen().is_empty());
    assert_eq!(
        *calls.lock().unwrap(),
        1,
        "one lookup for the judging and the connection"
    );
    // The next answer is on this device: that one address is the one connected to.
    let provider = make();
    let (listed, _) = provider.models().await.unwrap();
    assert_eq!(listed, vec![MODEL.to_string()]);
    assert_eq!(stand_in.seen().len(), 1);
    assert_eq!(*calls.lock().unwrap(), 2);
    // Never a provider, whatever the name said before.
    let (outcome, _) = make().check().await;
    assert_eq!(outcome, CheckOutcome::AddressNotAllowed);
    assert_eq!(stand_in.seen().len(), 1);
}

#[tokio::test]
async fn a_name_that_cannot_be_looked_up_is_not_reached() {
    let stand_in = StandIn::start().await;
    let url = format!("http://gpubox:{}/v1", stand_in.addr.port());
    let failing: Lookup = Arc::new(|_: String, _: u16| {
        Box::pin(async { Err(std::io::Error::other("no such name")) })
    });
    let provider = Provider::new(&url, MODEL, OnlineKey::new(KEY.into()))
        .unwrap()
        .with_lookup(failing);
    let (outcome, made) = provider.check().await;
    assert_eq!(outcome, CheckOutcome::NoInternet);
    assert!(made.is_empty());
    assert!(stand_in.seen().is_empty());
}

#[tokio::test]
async fn the_check_looks_at_the_list_then_asks_one_short_question() {
    let stand_in = StandIn::start().await;
    let (outcome, made) = provider(&stand_in).check().await;
    assert_eq!(
        outcome,
        CheckOutcome::Works {
            host: format!("127.0.0.1:{}", stand_in.addr.port()),
            model: MODEL.into()
        }
    );
    assert_eq!(made.len(), 2);
    let seen = stand_in.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(
        (seen[0].method.as_str(), seen[0].path.as_str()),
        ("GET", "/v1/models")
    );
    assert_eq!(seen[1].path, "/v1/chat/completions");
    assert_eq!(
        seen[1].body["messages"],
        json!([{"role": "user", "content": online::CHECK_QUESTION}])
    );
    assert_eq!(seen[1].body["max_tokens"], 8);

    let missing = StandIn::start().await;
    missing.models(&["another-model"]);
    let (outcome, _) = provider(&missing).check().await;
    assert_eq!(outcome, CheckOutcome::ModelMissing);
    assert_eq!(missing.chats().len(), 0);

    let refused = StandIn::start().await;
    refused.models_status(401);
    assert_eq!(provider(&refused).check().await.0, CheckOutcome::KeyRefused);

    let unlisted = StandIn::start().await;
    unlisted.models_status(404);
    let (outcome, made) = provider(&unlisted).check().await;
    assert!(matches!(outcome, CheckOutcome::Works { .. }), "{outcome:?}");
    assert_eq!(made.len(), 2);
}

#[test]
fn an_address_off_the_rules_makes_no_provider() {
    let key = || OnlineKey::new(KEY.into());
    assert_eq!(
        Provider::new("http://api.example.com/v1", MODEL, key()).unwrap_err(),
        UrlProblem::NotHttps
    );
    assert_eq!(
        Provider::new("https://metadata.google.internal/v1", MODEL, key()).unwrap_err(),
        UrlProblem::NotAllowed
    );
}
