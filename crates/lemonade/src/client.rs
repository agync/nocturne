//! A typed client for the private engine's HTTP API. Cheap to clone; it follows the engine
//! across restarts because the supervisor updates the address it points at.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use reqwest::{Method, RequestBuilder, Response};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::error::RuntimeError;

/// Where the engine is listening and the key it wants.
#[derive(Clone)]
pub struct Endpoint {
    pub port: u16,
    pub key: String,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("port", &self.port)
            .field("key", &"<redacted>")
            .finish()
    }
}

/// Time limits for requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    pub connect: Duration,
    pub request: Duration,
    /// Longest silence tolerated inside a streamed answer.
    pub idle: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts {
            connect: Duration::from_secs(2),
            request: Duration::from_secs(120),
            idle: Duration::from_secs(30),
        }
    }
}

struct Shared {
    http: reqwest::Client,
    #[allow(dead_code)]
    stream_http: reqwest::Client,
    timeouts: Timeouts,
    endpoint: RwLock<Option<Endpoint>>,
}

#[derive(Clone)]
pub struct Client {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").finish_non_exhaustive()
    }
}

/// What `GET /v1/health` tells us.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Health {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub websocket_port: Option<u16>,
    #[serde(default)]
    pub model_loaded: Option<String>,
    #[serde(default)]
    pub all_models_loaded: Vec<LoadedModel>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct LoadedModel {
    #[serde(default)]
    pub model_name: String,
    #[serde(default, rename = "type")]
    pub kind: String,
}

impl Client {
    /// A client for an engine already known to be at `port` with `key`.
    pub fn new(port: u16, key: impl Into<String>) -> Client {
        let client = Client::detached(Timeouts::default());
        client.set_endpoint(Some(Endpoint {
            port,
            key: key.into(),
        }));
        client
    }

    pub fn with_timeouts(port: u16, key: impl Into<String>, timeouts: Timeouts) -> Client {
        let client = Client::detached(timeouts);
        client.set_endpoint(Some(Endpoint {
            port,
            key: key.into(),
        }));
        client
    }

    /// A client with no engine behind it yet; every call fails with `Unavailable`.
    pub(crate) fn detached(timeouts: Timeouts) -> Client {
        let http = reqwest::Client::builder()
            .connect_timeout(timeouts.connect)
            .timeout(timeouts.request)
            .no_proxy()
            .build()
            .expect("http client");
        let stream_http = reqwest::Client::builder()
            .connect_timeout(timeouts.connect)
            .read_timeout(timeouts.idle)
            .no_proxy()
            .build()
            .expect("http client");
        Client {
            shared: Arc::new(Shared {
                http,
                stream_http,
                timeouts,
                endpoint: RwLock::new(None),
            }),
        }
    }

    pub(crate) fn set_endpoint(&self, endpoint: Option<Endpoint>) {
        *self
            .shared
            .endpoint
            .write()
            .unwrap_or_else(|e| e.into_inner()) = endpoint;
    }

    pub fn endpoint(&self) -> Option<Endpoint> {
        self.shared
            .endpoint
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn timeouts(&self) -> Timeouts {
        self.shared.timeouts
    }

    fn request(&self, method: Method, path: &str) -> Result<RequestBuilder, RuntimeError> {
        self.build(&self.shared.http, method, path, true)
    }

    fn build(
        &self,
        http: &reqwest::Client,
        method: Method,
        path: &str,
        keyed: bool,
    ) -> Result<RequestBuilder, RuntimeError> {
        let endpoint = self.endpoint().ok_or(RuntimeError::Unavailable)?;
        let url = format!("http://127.0.0.1:{}{path}", endpoint.port);
        let rb = http.request(method, url);
        Ok(if keyed {
            rb.bearer_auth(endpoint.key)
        } else {
            rb
        })
    }

    async fn send(rb: RequestBuilder) -> Result<Response, RuntimeError> {
        let response = rb.send().await.map_err(map_transport)?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        Err(map_status(status, &text))
    }

    async fn json<T: DeserializeOwned>(rb: RequestBuilder) -> Result<T, RuntimeError> {
        let response = Self::send(rb).await?;
        let bytes = response.bytes().await.map_err(map_transport)?;
        serde_json::from_slice(&bytes).map_err(|e| RuntimeError::BadResponse(e.to_string()))
    }

    async fn post_json<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<T, RuntimeError> {
        Self::json(self.request(Method::POST, path)?.json(body)).await
    }

    /// `GET /live`: answers without a key while the engine is up.
    pub async fn live(&self) -> Result<(), RuntimeError> {
        let rb = self.build(&self.shared.http, Method::GET, "/live", false)?;
        Self::send(rb).await.map(|_| ())
    }

    pub async fn health(&self) -> Result<Health, RuntimeError> {
        Self::json(self.request(Method::GET, "/v1/health")?).await
    }

    /// `POST /internal/set`: changes `config.json` settings without a restart.
    pub async fn set(&self, settings: &Value) -> Result<(), RuntimeError> {
        let _: Value = self.post_json("/internal/set", settings).await?;
        Ok(())
    }

    /// `GET /internal/config`: the settings in force.
    pub async fn config(&self) -> Result<Value, RuntimeError> {
        Self::json(self.request(Method::GET, "/internal/config")?).await
    }

    /// `POST /internal/shutdown`: unloads every model and stops the engine.
    pub async fn shutdown(&self) -> Result<(), RuntimeError> {
        let _: Value = self.post_json("/internal/shutdown", &json!({})).await?;
        Ok(())
    }
}

fn map_transport(e: reqwest::Error) -> RuntimeError {
    if e.is_timeout() {
        RuntimeError::Timeout
    } else if e.is_connect() {
        RuntimeError::Unavailable
    } else if e.is_decode() || e.is_body() {
        RuntimeError::StreamCut
    } else {
        RuntimeError::Unavailable
    }
}

/// Reads `{"error": "text"}` and `{"error": {"message", "code"}}` alike.
fn parse_error(text: &str) -> (String, String) {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return (String::new(), text.trim().to_string());
    };
    match &v["error"] {
        Value::String(s) => (v["code"].as_str().unwrap_or("").to_string(), s.clone()),
        Value::Object(o) => (
            o.get("code")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string(),
            o.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string(),
        ),
        _ => (String::new(), text.trim().to_string()),
    }
}

pub(crate) fn map_status(status: u16, body: &str) -> RuntimeError {
    let (code, message) = parse_error(body);
    match status {
        401 | 403 => RuntimeError::Unauthorized,
        404 => RuntimeError::NotFound(message),
        400 if code == "lemond_offline" => RuntimeError::Offline,
        400..=499 => RuntimeError::Rejected {
            status,
            code,
            message,
        },
        _ => RuntimeError::Server { status, message },
    }
}
