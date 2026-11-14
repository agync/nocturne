//! The reranking client against a small stand-in for the engine's `/v1/models` and
//! `/v1/rerank`, on loopback.

use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use nocturne_lemonade::Client;
use nocturne_lemonade::rerank::Reranker;
use serde_json::{Value, json};

const MODEL: &str = "bge-reranker-v2-m3-GGUF";
const KEY: &str = "per-launch-key";

#[derive(Default)]
struct Seen {
    reranks: Vec<(Option<String>, Value)>,
}

struct Engine {
    seen: Arc<Mutex<Seen>>,
    reply: Value,
    downloaded: bool,
}

async fn start(reply: Value, downloaded: bool) -> (Client, Arc<Mutex<Seen>>) {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let state = Arc::new(Engine {
        seen: seen.clone(),
        reply,
        downloaded,
    });
    let app = Router::new()
        .route(
            "/v1/models",
            get(|State(s): State<Arc<Engine>>| async move {
                Json(json!({"data": [
                    {"id": MODEL, "downloaded": s.downloaded,
                     "checkpoint_status": [{"required": true, "present": s.downloaded}]},
                    {"id": "Qwen3-Embedding-0.6B-GGUF", "downloaded": true}
                ]}))
            }),
        )
        .route(
            "/v1/rerank",
            post(
                |State(s): State<Arc<Engine>>, headers: HeaderMap, Json(body): Json<Value>| async move {
                    let auth = headers
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    s.seen.lock().unwrap().reranks.push((auth, body));
                    Json(s.reply.clone())
                },
            ),
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (Client::new(port, KEY), seen)
}

fn docs() -> Vec<String> {
    [
        "The plumber comes on Friday.",
        "Gran lives in York.",
        "The boiler engineer is Marek.",
    ]
    .map(String::from)
    .to_vec()
}

#[tokio::test]
async fn scores_come_back_in_the_order_of_the_documents() {
    // The engine sends its results best first; each carries the place of its document.
    let (client, seen) = start(
        json!({"model": MODEL, "object": "list", "results": [
            {"index": 2, "relevance_score": 0.91},
            {"index": 0, "relevance_score": 0.40},
            {"index": 1, "relevance_score": -3.2}
        ]}),
        true,
    )
    .await;
    let r = Reranker::new(client, MODEL);
    assert!(r.present().await);
    let scores = r.scores("Who services the boiler?", &docs()).await.unwrap();
    assert_eq!(scores, vec![0.40, -3.2, 0.91]);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.reranks.len(), 1);
    let (auth, body) = &seen.reranks[0];
    assert_eq!(auth.as_deref(), Some("Bearer per-launch-key"));
    assert_eq!(
        body,
        &json!({"model": MODEL, "query": "Who services the boiler?", "documents": docs()})
    );
}

#[tokio::test]
async fn a_model_not_fully_on_disk_is_not_there() {
    let (client, seen) = start(json!({"results": []}), false).await;
    let r = Reranker::new(client, MODEL);
    assert!(!r.present().await);
    assert!(seen.lock().unwrap().reranks.is_empty());
    let (client, _) = start(json!({"results": []}), true).await;
    assert!(
        !Reranker::new(client, "jina-reranker-v1-tiny-en-GGUF")
            .present()
            .await
    );
}

#[tokio::test]
async fn a_reply_that_does_not_score_every_document_once_is_refused() {
    for results in [
        json!([{"index": 0, "relevance_score": 0.5}]),
        json!([
            {"index": 0, "relevance_score": 0.5},
            {"index": 0, "relevance_score": 0.4},
            {"index": 1, "relevance_score": 0.3}
        ]),
        json!([
            {"index": 0, "relevance_score": 0.5},
            {"index": 1, "relevance_score": 0.4},
            {"index": 7, "relevance_score": 0.3}
        ]),
    ] {
        let (client, _) = start(json!({ "results": results }), true).await;
        let r = Reranker::new(client, MODEL);
        assert!(r.scores("q", &docs()).await.is_err(), "{results}");
    }
}

#[tokio::test]
async fn nothing_to_score_asks_nothing_and_no_engine_is_an_error() {
    let (client, seen) = start(json!({"results": []}), true).await;
    let r = Reranker::new(client, MODEL);
    assert_eq!(r.scores("q", &[]).await.unwrap(), Vec::<f32>::new());
    assert!(seen.lock().unwrap().reranks.is_empty());
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let gone = Reranker::new(Client::new(closed, KEY), MODEL);
    assert!(!gone.present().await);
    assert!(gone.scores("q", &docs()).await.is_err());
}
