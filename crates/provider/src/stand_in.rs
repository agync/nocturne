//! A stand-in for the person's provider, in a test's own process: an OpenAI-compatible server
//! on loopback, plain HTTP, as the computer's `fake-provider` is. It answers from a script and
//! keeps every request it receives. Only with the `stand-in` feature, which tests alone turn on.
//!
//! - `GET /v1/models` lists the scripted models.
//! - `POST /v1/chat/completions` answers with the next scripted reply, or the standing one.
//! - Anything else is kept and answered 404.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};

use axum::Router;
use axum::body::{Bytes, to_bytes};
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

pub const MODEL: &str = "stand-in-model";
pub const KEY: &str = "stand-in-key";

/// One answer to a chat request.
#[derive(Debug, Clone)]
pub enum Answer {
    /// A reply with these words, ended as `finish_reason`.
    Text(String, &'static str),
    /// A reply that asks for these look-ups, each a name and its arguments as JSON text.
    Tools(Vec<(String, String)>),
    /// This status, with this body.
    Status(u16, String),
    /// A redirect to this address.
    Redirect(String),
    /// No answer for a minute.
    Hang,
}

impl Answer {
    pub fn text(words: &str) -> Answer {
        Answer::Text(words.to_string(), "stop")
    }

    /// A reply that asks for one look-up.
    pub fn look(name: &str, arguments: &str) -> Answer {
        Answer::Tools(vec![(name.to_string(), arguments.to_string())])
    }
}

/// A request as the stand-in received it.
#[derive(Debug, Clone)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub authorization: Option<String>,
    pub body: Value,
    pub bytes: usize,
}

struct Script {
    next: VecDeque<Answer>,
    standing: Answer,
    models: Vec<String>,
    models_status: Option<u16>,
    seen: Vec<Seen>,
}

#[derive(Clone)]
pub struct StandIn {
    pub addr: SocketAddr,
    script: Arc<Mutex<Script>>,
}

impl StandIn {
    pub async fn start() -> StandIn {
        let script = Arc::new(Mutex::new(Script {
            next: VecDeque::new(),
            standing: Answer::text("ready"),
            models: vec![MODEL.to_string()],
            models_status: None,
            seen: Vec::new(),
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().fallback(answer).with_state(script.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        StandIn { addr, script }
    }

    /// The address a person would enter for it.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.addr.port())
    }

    fn script(&self) -> MutexGuard<'_, Script> {
        self.script.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The next chat request gets `answer`; queue several for several requests.
    pub fn then(&self, answer: Answer) {
        self.script().next.push_back(answer);
    }

    /// Every chat request with nothing queued gets `answer`.
    pub fn always(&self, answer: Answer) {
        self.script().standing = answer;
    }

    pub fn models(&self, models: &[&str]) {
        self.script().models = models.iter().map(|m| m.to_string()).collect();
    }

    pub fn models_status(&self, status: u16) {
        self.script().models_status = Some(status);
    }

    /// The list of models is answered again, after [`StandIn::models_status`].
    pub fn models_listed(&self) {
        self.script().models_status = None;
    }

    /// Every request received so far, in order.
    pub fn seen(&self) -> Vec<Seen> {
        self.script().seen.clone()
    }

    /// The chat requests received so far.
    pub fn chats(&self) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|s| s.path == "/v1/chat/completions")
            .collect()
    }
}

async fn answer(State(script): State<Arc<Mutex<Script>>>, req: Request) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let authorization = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes: Bytes = to_bytes(req.into_body(), 64 << 20)
        .await
        .unwrap_or_default();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let hangs = {
        let mut s = script.lock().unwrap_or_else(|e| e.into_inner());
        s.seen.push(Seen {
            method: method.clone(),
            path: path.clone(),
            authorization,
            body,
            bytes: bytes.len(),
        });
        path == "/v1/chat/completions"
            && matches!(s.next.front().unwrap_or(&s.standing), Answer::Hang)
    };
    if hangs {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        return StatusCode::GATEWAY_TIMEOUT.into_response();
    }
    let mut s = script.lock().unwrap_or_else(|e| e.into_inner());
    match (method.as_str(), path.as_str()) {
        ("GET", "/v1/models") => {
            if let Some(status) = s.models_status {
                return status_of(status, "{}".into());
            }
            let data: Vec<Value> = s
                .models
                .iter()
                .map(|m| json!({"id": m, "object": "model"}))
                .collect();
            axum::Json(json!({"object": "list", "data": data})).into_response()
        }
        ("POST", "/v1/chat/completions") => {
            let a = s.next.pop_front().unwrap_or_else(|| s.standing.clone());
            match a {
                Answer::Text(words, finish) => axum::Json(json!({
                    "id": "chatcmpl-stand-in",
                    "object": "chat.completion",
                    "model": MODEL,
                    "choices": [{"index": 0, "message": {"role": "assistant", "content": words},
                                 "finish_reason": finish}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": 2, "cost": 0.0001},
                }))
                .into_response(),
                Answer::Tools(calls) => {
                    let calls: Vec<Value> = calls
                        .into_iter()
                        .enumerate()
                        .map(|(i, (name, arguments))| {
                            json!({"id": format!("call_{i}"), "type": "function",
                                   "function": {"name": name, "arguments": arguments}})
                        })
                        .collect();
                    axum::Json(json!({
                        "id": "chatcmpl-stand-in",
                        "object": "chat.completion",
                        "model": MODEL,
                        "choices": [{"index": 0, "message": {"role": "assistant", "content": null,
                                     "tool_calls": calls}, "finish_reason": "tool_calls"}],
                        "usage": {"prompt_tokens": 10, "completion_tokens": 2, "cost": 0.0001},
                    }))
                    .into_response()
                }
                Answer::Status(status, body) => status_of(status, body),
                Answer::Hang => StatusCode::GATEWAY_TIMEOUT.into_response(),
                Answer::Redirect(to) => {
                    (StatusCode::TEMPORARY_REDIRECT, [(header::LOCATION, to)], "").into_response()
                }
            }
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

fn status_of(status: u16, body: String) -> Response {
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        body,
    )
        .into_response()
}
