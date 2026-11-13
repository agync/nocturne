//! A typed client for the private engine's HTTP API. Cheap to clone; it follows the engine
//! across restarts because the supervisor updates the address it points at.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use futures_util::stream::BoxStream;
use reqwest::multipart::{Form, Part};
use reqwest::{Method, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
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
    /// Longest silence between the words of a streamed answer.
    pub idle: Duration,
    /// Longest wait for the first word, while the engine reads the prompt.
    pub first_token: Duration,
    /// Overall limit for a call that returns structured JSON in one piece.
    pub json: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts {
            connect: Duration::from_secs(2),
            request: Duration::from_secs(120),
            idle: Duration::from_secs(30),
            first_token: Duration::from_secs(120),
            json: Duration::from_secs(300),
        }
    }
}

struct Shared {
    http: reqwest::Client,
    stream_http: reqwest::Client,
    timeouts: Timeouts,
    endpoint: RwLock<Option<Endpoint>>,
    generation: AtomicU64,
    activity: std::sync::Mutex<HashMap<String, Activity>>,
    alone: RwLock<Vec<String>>,
    saved: RwLock<HashMap<String, serde_json::Map<String, Value>>>,
}

/// When a model was last asked for something, and how many requests it is serving now.
#[derive(Debug, Clone, Copy)]
pub struct Activity {
    pub last: Instant,
    pub busy: u32,
}

/// Marks a model busy for as long as it lives.
struct Busy {
    shared: Arc<Shared>,
    model: String,
}

impl Drop for Busy {
    fn drop(&mut self) {
        let mut map = self
            .shared
            .activity
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(a) = map.get_mut(&self.model) {
            a.busy = a.busy.saturating_sub(1);
            a.last = Instant::now();
        }
    }
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
            .no_proxy()
            .build()
            .expect("http client");
        Client {
            shared: Arc::new(Shared {
                http,
                stream_http,
                timeouts,
                endpoint: RwLock::new(None),
                generation: AtomicU64::new(0),
                activity: std::sync::Mutex::new(HashMap::new()),
                alone: RwLock::new(Vec::new()),
                saved: RwLock::new(HashMap::new()),
            }),
        }
    }

    /// Changes whenever the engine's address does, that is, at every launch and every stop.
    /// Anything remembered about the engine (models loaded, files present) is stale after it.
    pub fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::SeqCst)
    }

    pub(crate) fn set_endpoint(&self, endpoint: Option<Endpoint>) {
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        *self
            .shared
            .endpoint
            .write()
            .unwrap_or_else(|e| e.into_inner()) = endpoint;
    }

    fn busy(&self, model: &str) -> Busy {
        let mut map = self
            .shared
            .activity
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let a = map.entry(model.to_string()).or_insert(Activity {
            last: Instant::now(),
            busy: 0,
        });
        a.busy += 1;
        a.last = Instant::now();
        Busy {
            shared: self.shared.clone(),
            model: model.to_string(),
        }
    }

    /// The load options saved for each model in the engine's `recipe_options.json`. A load that
    /// saves its own options sends these too, so that saving does not drop them.
    pub fn set_saved_options(&self, options: Vec<(&str, serde_json::Map<String, Value>)>) {
        *self.shared.saved.write().unwrap_or_else(|e| e.into_inner()) = options
            .into_iter()
            .map(|(m, o)| (m.to_string(), o))
            .collect();
    }

    /// Models that must run with nothing else loaded: before one of them is used, every other
    /// model that is not busy is unloaded. For computers with little memory.
    pub fn set_alone(&self, models: Vec<String>) {
        *self.shared.alone.write().unwrap_or_else(|e| e.into_inner()) = models;
    }

    /// Unloads every other model that is not busy, when `model` must run alone. True when
    /// anything was unloaded.
    pub async fn make_room(&self, model: &str) -> bool {
        let alone = self
            .shared
            .alone
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .any(|m| m == model);
        if !alone {
            return false;
        }
        let Ok(health) = self.health().await else {
            return false;
        };
        let mut unloaded = false;
        for other in health.all_models_loaded {
            let busy = self.activity(&other.model_name).is_some_and(|a| a.busy > 0);
            if other.model_name != model && !busy {
                unloaded |= self.unload(&other.model_name).await.is_ok();
            }
        }
        unloaded
    }

    /// What this client last did with `model`, if anything since it was made.
    pub fn activity(&self, model: &str) -> Option<Activity> {
        self.shared
            .activity
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(model)
            .copied()
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
        let rb = self
            .request(Method::POST, "/internal/shutdown")?
            .timeout(Duration::from_secs(2))
            .json(&json!({}));
        let _: Value = Self::json(rb).await?;
        Ok(())
    }
}

/// A server-owned download job, for a model (`model:<name>`) or a backend (`backend:<recipe>:<backend>`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Download {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub model_name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub complete: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub total_download_size: u64,
    #[serde(default)]
    pub cumulative_bytes_downloaded: u64,
    #[serde(default)]
    pub overall_bytes_downloaded: u64,
    #[serde(default)]
    pub bytes_downloaded: u64,
}

impl Download {
    /// Bytes fetched so far across every file of the job.
    pub fn bytes(&self) -> u64 {
        self.cumulative_bytes_downloaded
            .max(self.overall_bytes_downloaded)
            .max(self.bytes_downloaded)
    }

    pub fn finished(&self) -> bool {
        self.complete || self.status == "completed"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Pause,
    Cancel,
    Remove,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Action::Pause => "pause",
            Action::Cancel => "cancel",
            Action::Remove => "remove",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadOptions {
    pub ctx_size: Option<u32>,
    pub llamacpp_backend: Option<String>,
    pub whispercpp_backend: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendState {
    Installable,
    Installed,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Gpu {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub integrated: bool,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub vram_gb: f64,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Npu {
    #[serde(default)]
    pub available: bool,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Devices {
    #[serde(default)]
    pub amd_gpu: Vec<Gpu>,
    #[serde(default)]
    pub amd_npu: Option<Npu>,
    /// Apple's graphics, on a Mac.
    #[serde(default)]
    pub metal: Option<Gpu>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct ModelStorage {
    #[serde(default)]
    pub free_bytes: u64,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct BackendInfo {
    #[serde(default)]
    pub state: String,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Recipe {
    #[serde(default)]
    pub backends: BTreeMap<String, BackendInfo>,
}

/// The part of `GET /v1/system-info` Nocturne reads.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct SystemInfo {
    #[serde(default)]
    pub devices: Devices,
    #[serde(default)]
    pub model_storage: Option<ModelStorage>,
    #[serde(default)]
    pub recipes: BTreeMap<String, Recipe>,
}

impl SystemInfo {
    pub fn backend_state(&self, recipe: &str, backend: &str) -> BackendState {
        match self
            .recipes
            .get(recipe)
            .and_then(|r| r.backends.get(backend))
            .map(|b| b.state.as_str())
        {
            // A backend that needs an update installs again like a new one.
            Some("installable" | "update_required") => BackendState::Installable,
            Some("installed") => BackendState::Installed,
            Some("unsupported") => BackendState::Unsupported,
            _ => BackendState::Unknown,
        }
    }

    pub fn free_bytes(&self) -> Option<u64> {
        self.model_storage.as_ref().map(|s| s.free_bytes)
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct CheckpointStatus {
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub present: bool,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct ModelInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub recipe: String,
    /// Download size in gigabytes.
    #[serde(default)]
    pub size: f64,
    #[serde(default)]
    pub downloaded: bool,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub checkpoint_status: Vec<CheckpointStatus>,
}

impl ModelInfo {
    /// Every required file is on disk.
    pub fn present(&self) -> bool {
        self.downloaded
            && self
                .checkpoint_status
                .iter()
                .filter(|c| c.required)
                .all(|c| c.present)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Message {
    pub role: String,
    pub content: Value,
}

impl Message {
    pub fn system(text: &str) -> Message {
        Message {
            role: "system".into(),
            content: json!(text),
        }
    }

    pub fn user(text: &str) -> Message {
        Message {
            role: "user".into(),
            content: json!(text),
        }
    }

    /// A user message with an attached image, given as a `data:` URI.
    pub fn user_with_image(text: &str, data_uri: &str) -> Message {
        Message {
            role: "user".into(),
            content: json!([
                {"type": "text", "text": text},
                {"type": "image_url", "image_url": {"url": data_uri}},
            ]),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<Value>,
    /// False turns off reasoning on models that think.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctx_size: Option<u32>,
    pub stream: bool,
    /// Overall limit for this call, instead of the client's default.
    #[serde(skip)]
    pub timeout: Option<Duration>,
}

impl ChatRequest {
    pub fn new(model: &str, messages: Vec<Message>) -> ChatRequest {
        ChatRequest {
            model: model.to_string(),
            messages,
            temperature: None,
            max_tokens: None,
            response_format: None,
            enable_thinking: Some(false),
            ctx_size: None,
            stream: false,
            timeout: None,
        }
    }
}

/// The text of an answer as it arrives. A stream that ends before the engine says it is
/// finished yields `RuntimeError::StreamCut` and then stops.
pub type ChatStream = BoxStream<'static, Result<String, RuntimeError>>;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Segment {
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: f64,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Transcription {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub segments: Vec<Segment>,
}

const LOAD_TIMEOUT: Duration = Duration::from_secs(900);
const AUDIO_TIMEOUT: Duration = Duration::from_secs(4 * 3600);

impl Client {
    pub async fn system_info(&self) -> Result<SystemInfo, RuntimeError> {
        Self::json(self.request(Method::GET, "/v1/system-info")?).await
    }

    /// Every model in the catalogue, downloaded or not.
    pub async fn models(&self) -> Result<Vec<ModelInfo>, RuntimeError> {
        #[derive(Deserialize)]
        struct List {
            #[serde(default)]
            data: Vec<ModelInfo>,
        }
        let list: List = Self::json(self.request(Method::GET, "/v1/models?show_all=true")?).await?;
        Ok(list.data)
    }

    /// Installs a backend as a server-owned download job.
    pub async fn install(&self, recipe: &str, backend: &str) -> Result<Download, RuntimeError> {
        self.post_json(
            "/v1/install",
            &json!({"recipe": recipe, "backend": backend, "stream": true, "subscribe": false}),
        )
        .await
    }

    /// Pulls a model as a server-owned download job.
    pub async fn pull(&self, model: &str) -> Result<Download, RuntimeError> {
        self.post_json(
            "/v1/pull",
            &json!({"model_name": model, "stream": true, "subscribe": false}),
        )
        .await
    }

    pub async fn downloads(&self) -> Result<Vec<Download>, RuntimeError> {
        Self::json(self.request(Method::GET, "/v1/downloads")?).await
    }

    /// The job's latest snapshot, or `None` when it was removed.
    pub async fn control(
        &self,
        job: &str,
        action: Action,
    ) -> Result<Option<Download>, RuntimeError> {
        let v: Value = self
            .post_json(
                "/v1/downloads/control",
                &json!({"id": job, "action": action.as_str()}),
            )
            .await?;
        if v.get("id").is_some() {
            serde_json::from_value(v)
                .map(Some)
                .map_err(|e| RuntimeError::BadResponse(e.to_string()))
        } else {
            Ok(None)
        }
    }

    pub async fn load(&self, model: &str, options: &LoadOptions) -> Result<(), RuntimeError> {
        let _busy = self.busy(model);
        let mut body = json!({"model_name": model});
        if let Some(n) = options.ctx_size {
            body["ctx_size"] = json!(n);
            // So that an automatic reload after eviction keeps the context size.
            body["save_options"] = json!(true);
            // Saving replaces the stored options: send the others Nocturne saves for this model.
            if let Some(saved) = self
                .shared
                .saved
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .get(model)
            {
                for (key, value) in saved {
                    if body.get(key).is_none() {
                        body[key.as_str()] = value.clone();
                    }
                }
            }
        }
        if let Some(b) = &options.llamacpp_backend {
            body["llamacpp_backend"] = json!(b);
        }
        if let Some(b) = &options.whispercpp_backend {
            body["whispercpp_backend"] = json!(b);
        }
        let rb = self
            .request(Method::POST, "/v1/load")?
            .timeout(LOAD_TIMEOUT)
            .json(&body);
        let response = rb.send().await.map_err(map_transport)?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(classify_load_failure(status, &text));
        }
        Ok(())
    }

    pub async fn unload(&self, model: &str) -> Result<(), RuntimeError> {
        let _: Value = self
            .post_json("/v1/unload", &json!({"model_name": model}))
            .await?;
        Ok(())
    }

    /// The whole answer at once.
    pub async fn chat_complete(&self, req: &ChatRequest) -> Result<String, RuntimeError> {
        let _busy = self.busy(&req.model);
        let mut req = req.clone();
        req.stream = false;
        let mut rb = self
            .request(Method::POST, "/v1/chat/completions")?
            .json(&req);
        if let Some(limit) = req.timeout {
            rb = rb.timeout(limit);
        }
        let v: Value = Self::json(rb).await?;
        v["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| RuntimeError::BadResponse("no message in the answer".into()))
    }

    /// An answer constrained to `schema` and parsed into `T`.
    pub async fn chat_json<T: DeserializeOwned>(
        &self,
        req: &ChatRequest,
        schema: &Value,
    ) -> Result<T, RuntimeError> {
        let mut req = req.clone();
        req.response_format = Some(json_schema_format(schema));
        req.timeout.get_or_insert(self.shared.timeouts.json);
        let text = self.chat_complete(&req).await?;
        parse_json_text(&text)
    }

    /// The answer as a stream of text deltas, reasoning left out.
    pub async fn chat(&self, req: &ChatRequest) -> Result<ChatStream, RuntimeError> {
        let busy = self.busy(&req.model);
        let mut req = req.clone();
        req.stream = true;
        let rb = self
            .build(
                &self.shared.stream_http,
                Method::POST,
                "/v1/chat/completions",
                true,
            )?
            .json(&req);
        let first = self.shared.timeouts.first_token;
        let response = tokio::time::timeout(first, Self::send(rb))
            .await
            .map_err(|_| RuntimeError::Timeout)??;
        let deltas = sse_deltas(response, first, self.shared.timeouts.idle);
        // The model stays busy until the answer is read or dropped.
        Ok(Box::pin(deltas.map(move |d| {
            let _ = &busy;
            d
        })))
    }

    pub async fn transcribe(
        &self,
        audio: &Path,
        model: &str,
        language: Option<&str>,
    ) -> Result<Transcription, RuntimeError> {
        let _busy = self.busy(model);
        let bytes = tokio::fs::read(audio).await?;
        let name = audio
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "audio.wav".into());
        let mime = match audio.extension().and_then(|e| e.to_str()) {
            Some("mp3") => "audio/mpeg",
            Some("ogg") | Some("opus") => "audio/ogg",
            Some("webm") => "audio/webm",
            Some("m4a") => "audio/mp4",
            Some("flac") => "audio/flac",
            _ => "audio/wav",
        };
        let part = Part::bytes(bytes)
            .file_name(name)
            .mime_str(mime)
            .map_err(|e| RuntimeError::BadResponse(e.to_string()))?;
        let mut form = Form::new()
            .text("model", model.to_string())
            .text("response_format", "verbose_json")
            .part("file", part);
        if let Some(lang) = language {
            form = form.text("language", lang.to_string());
        }
        let rb = self
            .request(Method::POST, "/v1/audio/transcriptions")?
            .timeout(AUDIO_TIMEOUT)
            .multipart(form);
        Self::json(rb).await
    }

    pub async fn embeddings(
        &self,
        model: &str,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, RuntimeError> {
        #[derive(Deserialize)]
        struct Item {
            #[serde(default)]
            index: usize,
            embedding: Vec<f32>,
        }
        #[derive(Deserialize)]
        struct List {
            data: Vec<Item>,
        }
        let _busy = self.busy(model);
        let list: List = self
            .post_json("/v1/embeddings", &json!({"model": model, "input": texts}))
            .await?;
        if list.data.len() != texts.len() {
            return Err(RuntimeError::BadResponse(format!(
                "asked for {} embeddings, got {}",
                texts.len(),
                list.data.len()
            )));
        }
        let mut items = list.data;
        items.sort_by_key(|i| i.index);
        Ok(items.into_iter().map(|i| i.embedding).collect())
    }

    /// Speech as WAV bytes.
    pub async fn speech(
        &self,
        model: &str,
        voice: &str,
        text: &str,
    ) -> Result<Vec<u8>, RuntimeError> {
        let _busy = self.busy(model);
        let rb = self
            .request(Method::POST, "/v1/audio/speech")?
            .timeout(AUDIO_TIMEOUT)
            .json(&json!({
                "model": model, "voice": voice, "input": text, "response_format": "wav",
            }));
        let response = Self::send(rb).await?;
        let bytes = response.bytes().await.map_err(map_transport)?;
        Ok(bytes.to_vec())
    }
}

pub(crate) fn json_schema_format(schema: &Value) -> Value {
    json!({
        "type": "json_schema",
        "json_schema": {"name": "result", "strict": true, "schema": schema},
    })
}

/// Models sometimes wrap JSON in a code fence; take what is inside.
pub(crate) fn parse_json_text<T: DeserializeOwned>(text: &str) -> Result<T, RuntimeError> {
    let trimmed = text.trim();
    let inner = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|t| t.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);
    serde_json::from_str(inner).map_err(|e| RuntimeError::BadResponse(e.to_string()))
}

struct SseState {
    body: BoxStream<'static, Result<bytes::Bytes, reqwest::Error>>,
    buf: Vec<u8>,
    ready: VecDeque<Result<String, RuntimeError>>,
    finished: bool,
    complete: bool,
    started: bool,
    first: Duration,
    idle: Duration,
}

fn sse_deltas(response: Response, first: Duration, idle: Duration) -> ChatStream {
    let state = SseState {
        body: response.bytes_stream().boxed(),
        buf: Vec::new(),
        ready: VecDeque::new(),
        finished: false,
        complete: false,
        started: false,
        first,
        idle,
    };
    futures_util::stream::unfold(state, |mut st| async move {
        loop {
            if let Some(item) = st.ready.pop_front() {
                return Some((item, st));
            }
            if st.finished {
                return None;
            }
            let patience = if st.started { st.idle } else { st.first };
            let next = match tokio::time::timeout(patience, st.body.next()).await {
                Ok(item) => item.map(|r| r.map_err(|_| ())),
                Err(_) => Some(Err(())),
            };
            match next {
                Some(Ok(chunk)) => {
                    st.buf.extend_from_slice(&chunk);
                    while let Some(pos) = st.buf.iter().position(|b| *b == b'\n') {
                        let line: Vec<u8> = st.buf.drain(..=pos).collect();
                        let line = String::from_utf8_lossy(&line);
                        let line = line.trim();
                        let Some(data) = line.strip_prefix("data:") else {
                            continue;
                        };
                        let data = data.trim();
                        if data == "[DONE]" {
                            st.complete = true;
                            continue;
                        }
                        let Ok(v) = serde_json::from_str::<Value>(data) else {
                            continue;
                        };
                        if let Some(e) = v.get("error") {
                            st.ready.push_back(Err(RuntimeError::Server {
                                status: e["status_code"].as_u64().unwrap_or(502) as u16,
                                message: e["message"].as_str().unwrap_or("").to_string(),
                            }));
                            st.finished = true;
                            break;
                        }
                        let choice = &v["choices"][0];
                        if let Some(text) = choice["delta"]["content"].as_str()
                            && !text.is_empty()
                        {
                            st.started = true;
                            st.ready.push_back(Ok(text.to_string()));
                        }
                        if !choice["finish_reason"].is_null() {
                            st.complete = true;
                        }
                    }
                }
                Some(Err(_)) => {
                    st.ready.push_back(Err(RuntimeError::StreamCut));
                    st.finished = true;
                }
                None => {
                    if !st.complete {
                        st.ready.push_back(Err(RuntimeError::StreamCut));
                    }
                    st.finished = true;
                }
            }
        }
    })
    .boxed()
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

/// What a failed `/v1/load` means: a model that would not load (and whether it ran out of
/// memory) is told apart from the ordinary request errors.
pub(crate) fn classify_load_failure(status: u16, body: &str) -> RuntimeError {
    let (code, message) = parse_error(body);
    let load_failed = status >= 500 || code == "model_load_error" || code == "model_load_refused";
    if !load_failed {
        return map_status(status, body);
    }
    RuntimeError::LoadFailed {
        out_of_memory: mentions_out_of_memory(&message),
        message,
    }
}

fn mentions_out_of_memory(message: &str) -> bool {
    let m = message.to_lowercase();
    [
        "out of memory",
        "outofmemory",
        "out_of_device_memory",
        "out_of_host_memory",
        "outofdevicememory",
        "outofhostmemory",
        "bad_alloc",
        "failed to allocate",
        "cannot allocate",
        "insufficient memory",
        "not enough memory",
        "oom-kill",
    ]
    .iter()
    .any(|k| m.contains(k))
        || m.split(|c: char| !c.is_alphanumeric()).any(|w| w == "oom")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn body(code: &str, message: &str) -> String {
        json!({"error": {"code": code, "message": message}}).to_string()
    }

    #[test]
    fn load_failures_are_told_apart() {
        let oom = |msg: &str| classify_load_failure(500, &body("model_load_error", msg));
        for msg in [
            "llama-server: failed to allocate buffer of size 9 GiB",
            "ggml_vulkan: vk::Result::eErrorOutOfDeviceMemory",
            "CUDA error: out of memory",
            "process killed by OOM killer",
            "std::bad_alloc",
        ] {
            assert!(
                matches!(
                    oom(msg),
                    RuntimeError::LoadFailed {
                        out_of_memory: true,
                        ..
                    }
                ),
                "{msg}"
            );
        }
        for msg in [
            "bad gguf magic",
            "the room is full",
            "backend did not start",
        ] {
            assert!(
                matches!(
                    oom(msg),
                    RuntimeError::LoadFailed {
                        out_of_memory: false,
                        ..
                    }
                ),
                "{msg}"
            );
        }
        assert!(matches!(
            classify_load_failure(400, &body("model_load_refused", "unsupported gguf type")),
            RuntimeError::LoadFailed {
                out_of_memory: false,
                ..
            }
        ));
        assert!(matches!(
            classify_load_failure(502, "out of memory"),
            RuntimeError::LoadFailed {
                out_of_memory: true,
                ..
            }
        ));
        assert_eq!(classify_load_failure(401, ""), RuntimeError::Unauthorized);
        assert!(matches!(
            classify_load_failure(404, &body("model_not_found", "no such model")),
            RuntimeError::NotFound(_)
        ));
        assert!(matches!(
            classify_load_failure(409, &body("slots_pinned_error", "pinned")),
            RuntimeError::Rejected { status: 409, .. }
        ));
    }
}
