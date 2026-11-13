//! The HTTP client against the fake engine.

mod common;

use std::time::Duration;

use futures_util::StreamExt;
use nocturne_lemonade::RuntimeError;
use nocturne_lemonade::client::{Action, ChatRequest, Client, LoadOptions, Message, Timeouts};
use serde::Deserialize;
use serde_json::{Value, json};

use common::Fake;

fn only(mut v: Vec<Value>) -> Value {
    assert_eq!(v.len(), 1, "{v:?}");
    v.remove(0)
}

#[tokio::test]
async fn each_call_sends_the_right_method_path_body_and_key() {
    let fake = Fake::started(Some("downloaded=all")).await;
    let c = &fake.client;

    c.health().await.unwrap();
    let r = only(fake.seen("/v1/health").await);
    assert_eq!(
        (r["method"].as_str(), r["authorised"].as_bool()),
        (Some("GET"), Some(true))
    );

    c.system_info().await.unwrap();
    assert_eq!(only(fake.seen("/v1/system-info").await)["method"], "GET");

    let models = c.models().await.unwrap();
    let r = only(fake.seen("/v1/models").await);
    assert_eq!(r["query"], "show_all=true");
    assert!(models.iter().any(|m| m.id == "Whisper-Tiny" && m.present()));

    let job = c.pull("Whisper-Tiny").await.unwrap();
    assert_eq!(job.id, "model:Whisper-Tiny");
    assert!(job.finished(), "already downloaded");
    let r = only(fake.seen("/v1/pull").await);
    assert_eq!(r["method"], "POST");
    assert_eq!(
        r["body"],
        json!({"model_name": "Whisper-Tiny", "stream": true, "subscribe": false})
    );

    c.install("llamacpp", "vulkan").await.unwrap();
    let r = only(fake.seen("/v1/install").await);
    assert_eq!(
        r["body"],
        json!({"recipe": "llamacpp", "backend": "vulkan", "stream": true, "subscribe": false})
    );

    c.downloads().await.unwrap();
    assert_eq!(only(fake.seen("/v1/downloads").await)["method"], "GET");

    assert!(
        c.control("model:nothing", Action::Remove)
            .await
            .unwrap()
            .is_none()
    );
    let r = only(fake.seen("/v1/downloads/control").await);
    assert_eq!(
        r["body"],
        json!({"id": "model:nothing", "action": "remove"})
    );

    let opts = LoadOptions {
        ctx_size: Some(8192),
        llamacpp_backend: Some("vulkan".into()),
        whispercpp_backend: None,
    };
    c.load("Qwen3.5-9B-GGUF", &opts).await.unwrap();
    let r = only(fake.seen("/v1/load").await);
    assert_eq!(
        r["body"],
        json!({"model_name": "Qwen3.5-9B-GGUF", "ctx_size": 8192, "save_options": true, "llamacpp_backend": "vulkan"})
    );
    assert_eq!(
        c.health().await.unwrap().model_loaded.as_deref(),
        Some("Qwen3.5-9B-GGUF")
    );
    c.unload("Qwen3.5-9B-GGUF").await.unwrap();
    assert_eq!(
        only(fake.seen("/v1/unload").await)["body"],
        json!({"model_name": "Qwen3.5-9B-GGUF"})
    );

    c.set(&json!({"offline": true})).await.unwrap();
    assert_eq!(
        only(fake.seen("/internal/set").await)["body"],
        json!({"offline": true})
    );
    assert_eq!(c.config().await.unwrap()["offline"], true);

    let vecs = c
        .embeddings("Qwen3-Embedding-0.6B-GGUF", &["a".into(), "bb".into()])
        .await
        .unwrap();
    assert_eq!(vecs.len(), 2);
    assert_ne!(vecs[0], vecs[1]);
    let r = only(fake.seen("/v1/embeddings").await);
    assert_eq!(
        r["body"],
        json!({"model": "Qwen3-Embedding-0.6B-GGUF", "input": ["a", "bb"]})
    );

    let wav = c.speech("kokoro-v1", "bf_emma", "hello").await.unwrap();
    assert_eq!(&wav[..4], b"RIFF");
    let r = only(fake.seen("/v1/audio/speech").await);
    assert_eq!(
        r["body"],
        json!({"model": "kokoro-v1", "voice": "bf_emma", "input": "hello", "response_format": "wav"})
    );
}

#[tokio::test]
async fn the_fake_answers_both_url_prefixes_but_the_client_uses_v1() {
    let fake = Fake::started(None).await;
    let r = fake
        .http
        .get(fake.url("/api/v1/health"))
        .bearer_auth("k3y")
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success());
}

fn request(model: &str, text: &str) -> ChatRequest {
    ChatRequest::new(model, vec![Message::user(text)])
}

#[tokio::test]
async fn chat_streams_deltas_in_order_and_skips_reasoning() {
    let fake = Fake::started(None).await;
    fake.script(json!([{"deltas": ["Hel", "lo ", "there"], "reasoning": "hmm"}]))
        .await;
    let mut stream = fake.client.chat(&request("m", "hi")).await.unwrap();
    let mut got = Vec::new();
    while let Some(item) = stream.next().await {
        got.push(item.unwrap());
    }
    assert_eq!(got, ["Hel", "lo ", "there"]);
    let r = only(fake.seen("/v1/chat/completions").await);
    assert_eq!(r["body"]["stream"], true);
    assert_eq!(r["body"]["enable_thinking"], false);
    assert_eq!(r["body"]["messages"][0]["role"], "user");
}

#[tokio::test]
async fn a_cut_stream_is_an_error_after_the_deltas() {
    let fake = Fake::started(None).await;
    fake.script(json!([{"deltas": ["one ", "two "], "cut": true}]))
        .await;
    let mut stream = fake.client.chat(&request("m", "hi")).await.unwrap();
    let mut texts = Vec::new();
    let mut failure = None;
    while let Some(item) = stream.next().await {
        match item {
            Ok(t) => texts.push(t),
            Err(e) => failure = Some(e),
        }
    }
    assert_eq!(texts, ["one ", "two "]);
    assert_eq!(failure, Some(RuntimeError::StreamCut));
}

#[tokio::test]
async fn chat_complete_and_chat_json() {
    #[derive(Deserialize, PartialEq, Debug)]
    struct Out {
        n: u32,
    }
    let fake = Fake::started(None).await;
    fake.script(json!([
        {"content": "plain words"},
        {"content": "{\"n\": 7}"},
        {"content": "```json\n{\"n\": 8}\n```"},
        {"content": "not json at all"},
    ]))
    .await;
    let c = &fake.client;
    assert_eq!(
        c.chat_complete(&request("m", "x")).await.unwrap(),
        "plain words"
    );
    let schema = json!({"type": "object", "properties": {"n": {"type": "integer"}}});
    let a: Out = c.chat_json(&request("m", "x"), &schema).await.unwrap();
    assert_eq!(a, Out { n: 7 });
    let b: Out = c.chat_json(&request("m", "x"), &schema).await.unwrap();
    assert_eq!(b, Out { n: 8 });
    let bad = c.chat_json::<Out>(&request("m", "x"), &schema).await;
    assert!(matches!(bad, Err(RuntimeError::BadResponse(_))), "{bad:?}");
    let seen = fake.seen("/v1/chat/completions").await;
    assert_eq!(seen[1]["body"]["response_format"]["type"], "json_schema");
    assert_eq!(
        seen[1]["body"]["response_format"]["json_schema"]["schema"],
        schema
    );
}

#[tokio::test]
async fn transcribe_sends_the_file_as_multipart() {
    let fake = Fake::started(None).await;
    let wav = fake.dirs.root.join("note.wav");
    std::fs::write(&wav, vec![7u8; 1234]).unwrap();
    let t = fake
        .client
        .transcribe(&wav, "Whisper-Small", Some("en"))
        .await
        .unwrap();
    assert_eq!(t.text.trim(), "hello world");
    assert_eq!(t.language.as_deref(), Some("en"));
    assert_eq!(t.segments.len(), 2);
    assert_eq!(t.segments[1].end, 3.2);
    let entry = fake
        .peek("/fake/requests")
        .await
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["multipart"] == true)
        .cloned()
        .unwrap();
    assert_eq!(entry["file_name"], "note.wav");
    assert_eq!(entry["file_len"], 1234);
    assert_eq!(entry["fields"]["model"], "Whisper-Small");
    assert_eq!(entry["fields"]["response_format"], "verbose_json");
    assert_eq!(entry["fields"]["language"], "en");
    let posts = fake.seen("/v1/audio/transcriptions").await;
    assert!(
        posts[0]["content_type"]
            .as_str()
            .unwrap()
            .starts_with("multipart/form-data")
    );
}

#[tokio::test]
async fn statuses_map_to_error_variants() {
    let fake = Fake::started(None).await;
    let wrong = Client::new(fake.port, "not-the-key");
    assert_eq!(
        wrong.health().await.unwrap_err(),
        RuntimeError::Unauthorized
    );

    let missing = fake
        .client
        .load("No-Such-Model", &LoadOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(missing, RuntimeError::NotFound(_)), "{missing:?}");

    let rejected = fake.client.set(&json!({})).await.unwrap_err();
    assert!(
        matches!(rejected, RuntimeError::Rejected { status: 400, .. }),
        "{rejected:?}"
    );

    fake.script(json!([{"status": 500, "error": "boom"}])).await;
    let server = fake
        .client
        .chat_complete(&request("m", "x"))
        .await
        .unwrap_err();
    assert_eq!(
        server,
        RuntimeError::Server {
            status: 500,
            message: "boom".into()
        }
    );

    fake.client.set(&json!({"offline": true})).await.unwrap();
    assert_eq!(
        fake.client.pull("Whisper-Tiny").await.unwrap_err(),
        RuntimeError::Offline
    );
}

#[tokio::test]
async fn no_engine_means_unavailable() {
    let closed = Client::new(common::CLOSED_PORT, "k");
    assert_eq!(closed.live().await.unwrap_err(), RuntimeError::Unavailable);
    assert_eq!(
        closed.health().await.unwrap_err(),
        RuntimeError::Unavailable
    );
}

#[tokio::test]
async fn a_silent_server_times_out() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let client = Client::with_timeouts(
        port,
        "k",
        Timeouts {
            connect: Duration::from_millis(200),
            request: Duration::from_millis(300),
            idle: Duration::from_millis(300),
            ..Timeouts::default()
        },
    );
    assert_eq!(client.health().await.unwrap_err(), RuntimeError::Timeout);
    drop(listener);
}

#[tokio::test]
async fn a_stalled_stream_is_cut_after_the_idle_limit() {
    let fake = Fake::started(None).await;
    fake.script(json!([{"deltas": ["a", "b", "c"], "delay_ms": 600}]))
        .await;
    let client = Client::with_timeouts(
        fake.port,
        "k3y",
        Timeouts {
            connect: Duration::from_secs(2),
            request: Duration::from_secs(120),
            idle: Duration::from_millis(200),
            ..Timeouts::default()
        },
    );
    let outcome = match client.chat(&request("m", "x")).await {
        Err(e) => e,
        Ok(mut stream) => loop {
            match stream.next().await {
                Some(Err(e)) => break e,
                Some(Ok(_)) => {}
                None => panic!("the stall went unnoticed"),
            }
        },
    };
    assert!(
        matches!(outcome, RuntimeError::Timeout | RuntimeError::StreamCut),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn shutdown_stops_the_engine() {
    let mut fake = Fake::started(None).await;
    fake.client.shutdown().await.unwrap();
    assert!(fake.wait_exit(Duration::from_secs(5)).is_some());
}

#[tokio::test]
async fn json_calls_get_their_own_overall_limit() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let client = Client::with_timeouts(
        port,
        "k",
        Timeouts {
            request: Duration::from_secs(30),
            json: Duration::from_millis(300),
            ..Timeouts::default()
        },
    );
    let started = std::time::Instant::now();
    let err = client
        .chat_json::<Value>(&request("m", "x"), &json!({"type": "object"}))
        .await
        .unwrap_err();
    assert_eq!(err, RuntimeError::Timeout);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    drop(listener);
}

#[tokio::test]
async fn the_first_word_gets_longer_than_the_words_after_it() {
    let fake = Fake::started(None).await;
    fake.script(
        json!([{"deltas": ["a", "b"], "delay_ms": 500}, {"deltas": ["a", "b"], "delay_ms": 500}]),
    )
    .await;
    let patient_start = Client::with_timeouts(
        fake.port,
        "k3y",
        Timeouts {
            first_token: Duration::from_secs(5),
            idle: Duration::from_secs(2),
            ..Timeouts::default()
        },
    );
    let mut stream = patient_start.chat(&request("m", "x")).await.unwrap();
    let mut words = Vec::new();
    while let Some(item) = stream.next().await {
        words.push(item.unwrap());
    }
    assert_eq!(
        words,
        ["a", "b"],
        "500 ms gaps are fine when the limits are 5 s and 2 s"
    );

    let impatient_start = Client::with_timeouts(
        fake.port,
        "k3y",
        Timeouts {
            first_token: Duration::from_millis(150),
            idle: Duration::from_secs(2),
            ..Timeouts::default()
        },
    );
    let outcome = match impatient_start.chat(&request("m", "x")).await {
        Err(e) => e,
        Ok(mut stream) => loop {
            match stream.next().await {
                Some(Err(e)) => break e,
                Some(Ok(_)) => {}
                None => panic!("the slow start went unnoticed"),
            }
        },
    };
    assert!(
        matches!(outcome, RuntimeError::Timeout | RuntimeError::StreamCut),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_model_is_busy_while_it_answers_and_idle_after() {
    let fake = Fake::started(Some("downloaded=all")).await;
    let c = &fake.client;
    let model = "Qwen3.5-9B-GGUF";
    assert!(c.activity(model).is_none());
    let req = ChatRequest::new(model, vec![Message::user("hello")]);
    let mut stream = c.chat(&req).await.unwrap();
    assert_eq!(c.activity(model).unwrap().busy, 1);
    while stream.next().await.is_some() {}
    drop(stream);
    let after = c.activity(model).unwrap();
    assert_eq!(after.busy, 0);
    c.embeddings("Qwen3-Embedding-0.6B-GGUF", &["a".into()])
        .await
        .unwrap();
    assert_eq!(c.activity("Qwen3-Embedding-0.6B-GGUF").unwrap().busy, 0);
    assert!(
        c.activity(model).unwrap().last <= c.activity("Qwen3-Embedding-0.6B-GGUF").unwrap().last
    );
}

#[tokio::test]
async fn a_saving_load_keeps_the_other_saved_options() {
    let fake = Fake::started(Some("downloaded=all")).await;
    let c = &fake.client;
    let mut saved = serde_json::Map::new();
    saved.insert("evict_idle_timeout".into(), json!(180));
    saved.insert("ctx_size".into(), json!(4096));
    c.set_saved_options(vec![("Qwen3.5-2B-GGUF", saved)]);
    let opts = LoadOptions {
        ctx_size: Some(8192),
        ..LoadOptions::default()
    };
    c.load("Qwen3.5-2B-GGUF", &opts).await.unwrap();
    assert_eq!(
        only(fake.seen("/v1/load").await)["body"],
        json!({"model_name": "Qwen3.5-2B-GGUF", "ctx_size": 8192, "save_options": true, "evict_idle_timeout": 180})
    );
}

#[tokio::test]
async fn a_model_that_runs_alone_unloads_the_others_first() {
    let fake = Fake::started(Some("downloaded=all")).await;
    let c = &fake.client;
    c.load("Qwen3.5-2B-GGUF", &LoadOptions::default())
        .await
        .unwrap();
    let _ = c.speech("kokoro-v1", "bf_emma", "Hello.").await;
    assert!(
        fake.seen("/v1/unload").await.is_empty(),
        "speaking alone makes no room: the caller asks once per page"
    );
    assert!(
        !c.make_room("kokoro-v1").await,
        "nothing has to run alone yet"
    );
    c.set_alone(vec!["kokoro-v1".into()]);
    assert!(c.make_room("kokoro-v1").await);
    let unloaded = fake.seen("/v1/unload").await;
    assert_eq!(
        only(unloaded)["body"],
        json!({"model_name": "Qwen3.5-2B-GGUF"})
    );
}
