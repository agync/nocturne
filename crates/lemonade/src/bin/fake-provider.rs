//! A stand-in for the person's online provider: an OpenAI-compatible server on loopback, plain
//! HTTP, for tests, the network check and the S1 measurements. It never binds anything but
//! `127.0.0.1` and refuses to start when asked to.
//!
//! Usage: `fake-provider [--port N] [--host 127.0.0.1]`. Prints `ready <port>` once it listens.
//!
//! - `GET /v1/models` lists one model (`stand-in-model`, or the scripted `model`).
//! - `POST /v1/chat/completions` answers from the script, non-streaming or SSE.
//! - `GET /fake/requests` returns every request but its own controls (method, path, query,
//!   headers, body).
//! - `POST /fake/script` merges into the script: `reply` (text), `status` (an error status such
//!   as 401, 402, 403, 404, 429 or 500, `null` for none), `marker` (a string the error body
//!   carries beside the echoed `Authorization` header), `multiline` (an error body over several
//!   lines), `delay_ms`, `usage` (send `usage`), `cost` (a number for `usage.cost`, `null` for
//!   none), `empty` (an empty reply), `model`.
//! - `POST /fake/relay` with `{"url": "http://127.0.0.1:<port>/v1", "key": "..."}` forwards chat
//!   to a local OpenAI-compatible engine; `{"url": null}` stops relaying.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Map, Value, json};

const MAX_BODY: usize = 8 << 20;
const DEFAULT_REPLY: &str = "The bins go out on Tuesday [m1].";
const DEFAULT_MODEL: &str = "stand-in-model";

#[derive(Debug, Clone)]
struct Script {
    reply: String,
    status: Option<u16>,
    marker: String,
    multiline: bool,
    delay_ms: u64,
    usage: bool,
    cost: Option<f64>,
    empty: bool,
    model: String,
}

impl Default for Script {
    fn default() -> Self {
        Script {
            reply: DEFAULT_REPLY.into(),
            status: None,
            marker: "STAND-IN-MARKER".into(),
            multiline: false,
            delay_ms: 0,
            usage: true,
            cost: None,
            empty: false,
            model: DEFAULT_MODEL.into(),
        }
    }
}

#[derive(Default)]
struct Inner {
    script: Script,
    log: Vec<Value>,
    relay: Option<(String, String)>,
}

#[derive(Default)]
struct App {
    inner: Mutex<Inner>,
}

type Shared = Arc<App>;

impl App {
    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut port = 0u16;
    let mut host = Ipv4Addr::LOCALHOST.to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => port = args.get(i + 1).and_then(|p| p.parse().ok()).unwrap_or(0),
            "--host" => host = args.get(i + 1).cloned().unwrap_or_default(),
            other => {
                eprintln!("fake-provider: unknown argument {other}");
                std::process::exit(2);
            }
        }
        i += 2;
    }
    if host.parse::<IpAddr>().ok() != Some(IpAddr::V4(Ipv4Addr::LOCALHOST)) {
        eprintln!("fake-provider: listens on 127.0.0.1 only, not {host}");
        std::process::exit(2);
    }
    let listener =
        match tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("fake-provider: {e}");
                std::process::exit(1);
            }
        };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    println!("ready {port}");

    let app: Shared = Arc::default();
    let router = Router::new()
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat))
        .route("/fake/requests", get(requests))
        .route("/fake/script", post(script))
        .route("/fake/relay", post(relay))
        .layer(middleware::from_fn_with_state(app.clone(), record))
        .with_state(app);
    let _ = axum::serve(listener, router).await;
}

/// Records every request except the stand-in's own controls, then hands it on.
async fn record(State(app): State<Shared>, req: Request, next: Next) -> Response {
    let (parts, body) = req.into_parts();
    let bytes = to_bytes(body, MAX_BODY).await.unwrap_or_default();
    if !parts.uri.path().starts_with("/fake/") {
        let headers: Map<String, Value> = parts
            .headers
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_string(),
                    json!(String::from_utf8_lossy(v.as_bytes())),
                )
            })
            .collect();
        let body = serde_json::from_slice::<Value>(&bytes)
            .unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes)));
        app.inner().log.push(json!({
            "method": parts.method.as_str(),
            "path": parts.uri.path(),
            "query": parts.uri.query(),
            "headers": headers,
            "body": body,
        }));
    }
    next.run(Request::from_parts(parts, Body::from(bytes)))
        .await
}

async fn requests(State(app): State<Shared>) -> Json<Value> {
    Json(Value::Array(app.inner().log.clone()))
}

async fn script(State(app): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
    let mut inner = app.inner();
    let s = &mut inner.script;
    if let Some(v) = body.get("reply").and_then(Value::as_str) {
        s.reply = v.to_string();
    }
    if let Some(v) = body.get("status") {
        s.status = v.as_u64().and_then(|n| u16::try_from(n).ok());
    }
    if let Some(v) = body.get("marker").and_then(Value::as_str) {
        s.marker = v.to_string();
    }
    if let Some(v) = body.get("multiline").and_then(Value::as_bool) {
        s.multiline = v;
    }
    if let Some(v) = body.get("delay_ms").and_then(Value::as_u64) {
        s.delay_ms = v;
    }
    if let Some(v) = body.get("usage").and_then(Value::as_bool) {
        s.usage = v;
    }
    if let Some(v) = body.get("cost") {
        s.cost = v.as_f64();
    }
    if let Some(v) = body.get("empty").and_then(Value::as_bool) {
        s.empty = v;
    }
    if let Some(v) = body.get("model").and_then(Value::as_str) {
        s.model = v.to_string();
    }
    Json(json!({"ok": true}))
}

async fn relay(State(app): State<Shared>, Json(body): Json<Value>) -> Response {
    let url = body.get("url").and_then(Value::as_str);
    let key = body.get("key").and_then(Value::as_str).unwrap_or_default();
    let mut inner = app.inner();
    match url {
        None => inner.relay = None,
        Some(u) => {
            let local = reqwest::Url::parse(u)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .is_some_and(|h| h == "127.0.0.1");
            if !local {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "relays to 127.0.0.1 only"})),
                )
                    .into_response();
            }
            inner.relay = Some((u.trim_end_matches('/').to_string(), key.to_string()));
        }
    }
    Json(json!({"ok": true})).into_response()
}

async fn models(State(app): State<Shared>) -> Json<Value> {
    let model = app.inner().script.model.clone();
    Json(json!({"object": "list", "data": [{"id": model, "object": "model"}]}))
}

async fn chat(State(app): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let (script, relay) = {
        let inner = app.inner();
        (inner.script.clone(), inner.relay.clone())
    };
    if script.delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(script.delay_ms)).await;
    }
    if let Some(status) = script.status {
        return error_reply(status, &script, &headers);
    }
    if let Some((url, key)) = relay {
        return relay_chat(&url, &key, body).await;
    }
    let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let model = request["model"]
        .as_str()
        .unwrap_or(DEFAULT_MODEL)
        .to_string();
    let content = if script.empty {
        String::new()
    } else {
        script.reply.clone()
    };
    let mut usage = json!({"prompt_tokens": 100, "completion_tokens": 12, "total_tokens": 112});
    if let Some(cost) = script.cost {
        usage["cost"] = json!(cost);
    }
    if request["stream"].as_bool() == Some(true) {
        let first = json!({"id": "chatcmpl-stand-in", "object": "chat.completion.chunk",
            "model": model,
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": content},
                         "finish_reason": null}]});
        let mut last = json!({"id": "chatcmpl-stand-in", "object": "chat.completion.chunk",
            "model": model, "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]});
        if script.usage {
            last["usage"] = usage;
        }
        let text = format!("data: {first}\n\ndata: {last}\n\ndata: [DONE]\n\n");
        let mut resp = Response::new(Body::from(text));
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        );
        return resp;
    }
    let mut reply = json!({
        "id": "chatcmpl-stand-in",
        "object": "chat.completion",
        "model": model,
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content},
                     "finish_reason": "stop"}],
    });
    if script.usage {
        reply["usage"] = usage;
    }
    Json(reply).into_response()
}

/// An error whose body echoes the `Authorization` header and the marker, as a careless provider
/// might, so tests can prove neither reaches a log.
fn error_reply(status: u16, script: &Script, headers: &HeaderMap) -> Response {
    let auth = headers
        .get(header::AUTHORIZATION)
        .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned())
        .unwrap_or_default();
    let message = if script.multiline {
        format!(
            "{m} first line\n{m} second line\nauthorization was {auth}\n{m} last line",
            m = script.marker
        )
    } else {
        format!("{} authorization was {auth}", script.marker)
    };
    let body = json!({"error": {"message": message, "type": "stand_in_error", "code": status}});
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (code, Json(body)).into_response()
}

async fn relay_chat(url: &str, key: &str, body: Bytes) -> Response {
    let sent = reqwest::Client::new()
        .post(format!("{url}/chat/completions"))
        .bearer_auth(key)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.to_vec())
        .send()
        .await;
    match sent {
        Ok(r) => {
            let status = StatusCode::from_u16(r.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let kind = r
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| HeaderValue::from_bytes(v.as_bytes()).ok())
                .unwrap_or(HeaderValue::from_static("application/json"));
            let bytes = r.bytes().await.unwrap_or_default();
            let mut resp = Response::new(Body::from(bytes));
            *resp.status_mut() = status;
            resp.headers_mut().insert(header::CONTENT_TYPE, kind);
            resp
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": {"message": format!("relay failed: {e}")}})),
        )
            .into_response(),
    }
}
