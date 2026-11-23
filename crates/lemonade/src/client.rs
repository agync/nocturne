//! A typed client for the private engine's HTTP API. Cheap to clone; it follows the engine
//! across restarts because the supervisor updates the address it points at.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
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
    /// For online answers: never follows a redirect.
    online_http: reqwest::Client,
    timeouts: Timeouts,
    endpoint: RwLock<Option<Endpoint>>,
    generation: AtomicU64,
    activity: std::sync::Mutex<HashMap<String, Activity>>,
    alone: RwLock<Vec<String>>,
    saved: RwLock<HashMap<String, serde_json::Map<String, Value>>>,
    /// Whether `system_info` adds the graphics the engine does not report (see `graphics`).
    host_graphics: AtomicBool,
    /// What each launch writes as the engine's `max_loaded_models`.
    models_per_kind: AtomicU32,
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
        let online_http = reqwest::Client::builder()
            .connect_timeout(timeouts.connect)
            .timeout(timeouts.request)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .expect("http client");
        Client {
            shared: Arc::new(Shared {
                http,
                stream_http,
                online_http,
                timeouts,
                endpoint: RwLock::new(None),
                generation: AtomicU64::new(0),
                activity: std::sync::Mutex::new(HashMap::new()),
                alone: RwLock::new(Vec::new()),
                saved: RwLock::new(HashMap::new()),
                host_graphics: AtomicBool::new(false),
                models_per_kind: AtomicU32::new(1),
            }),
        }
    }

    /// Makes `system_info` look at this computer for the graphics the engine does not report.
    /// The app turns it on; tests leave it off so a result never depends on the machine.
    pub fn use_host_graphics(&self) {
        self.shared.host_graphics.store(true, Ordering::SeqCst);
        // The first look may start a process; it is not made on a request's own thread.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn_blocking(crate::graphics::intel_gpus);
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

    /// How many models of each kind the engine may hold, from the next launch on: two while the
    /// NPU reads items beside the mind.
    pub fn set_models_per_kind(&self, n: u32) {
        self.shared
            .models_per_kind
            .store(n.max(1), Ordering::Relaxed);
    }

    pub fn models_per_kind(&self) -> u32 {
        self.shared.models_per_kind.load(Ordering::Relaxed).max(1)
    }

    /// Models that must run with nothing else loaded: before one of them is used, every other
    /// model that is not busy is unloaded. For computers with little memory.
    pub fn set_alone(&self, models: Vec<String>) {
        *self.shared.alone.write().unwrap_or_else(|e| e.into_inner()) = models;
    }

    /// Unloads every other model that is not busy, when `model` must run alone. True when
    /// anything was unloaded. The online model is kept: it holds no memory here, and once
    /// unloaded the engine cannot load it again until it restarts (see [`crate::online`]).
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
            if other.model_name != model && other.model_name != crate::online::MODEL_ID && !busy {
                unloaded |= self.unload(&other.model_name).await.is_ok();
            }
        }
        unloaded
    }

    /// Requests to any model in flight now.
    pub fn busy_total(&self) -> u32 {
        self.shared
            .activity
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|a| a.busy)
            .sum()
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

    pub(crate) async fn send(rb: RequestBuilder) -> Result<Response, RuntimeError> {
        let response = rb.send().await.map_err(map_transport)?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        Err(map_status(status, &text))
    }

    pub(crate) async fn json<T: DeserializeOwned>(rb: RequestBuilder) -> Result<T, RuntimeError> {
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
    /// Why the engine finds no usable NPU, in its own words.
    #[serde(default)]
    pub error: String,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Devices {
    #[serde(default)]
    pub amd_gpu: Vec<Gpu>,
    /// NVIDIA's graphics. With none, the engine lists one entry that is not available.
    #[serde(default)]
    pub nvidia_gpu: Vec<Gpu>,
    /// Intel's graphics. The pinned engine has no such entry; `graphics` fills it in from the
    /// computer, and an engine that adds one is read the same way.
    #[serde(default)]
    pub intel_gpu: Vec<Gpu>,
    #[serde(default)]
    pub amd_npu: Option<Npu>,
    /// Apple's graphics, on a Mac.
    #[serde(default)]
    pub metal: Option<Gpu>,
}

impl Devices {
    /// Fills in Intel's graphics from `found` unless the engine listed some itself.
    pub(crate) fn add_intel(&mut self, found: impl FnOnce() -> Vec<Gpu>) {
        if self.intel_gpu.is_empty() {
            self.intel_gpu = found();
        }
    }

    /// Graphics of any maker are here and usable, apart from Apple's.
    pub fn has_graphics(&self) -> bool {
        [&self.amd_gpu, &self.nvidia_gpu, &self.intel_gpu]
            .into_iter()
            .flatten()
            .any(|g| g.available)
    }

    /// The largest memory a graphics card of its own has, in bytes: integrated graphics share
    /// the computer's memory and count for none.
    pub fn largest_pool(&self) -> u64 {
        let discrete = |g: &&Gpu| g.available && !g.integrated;
        self.amd_gpu
            .iter()
            .filter(discrete)
            .chain(self.nvidia_gpu.iter().filter(discrete))
            .chain(self.intel_gpu.iter().filter(discrete))
            .map(|g| (g.vram_gb * 1e9) as u64)
            .max()
            .unwrap_or(0)
    }
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
    /// Why a backend cannot be used, such as "Requires AMD XDNA 2 NPU".
    #[serde(default)]
    pub message: String,
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

    /// The engine's message about `backend` of `recipe`, empty when it gave none.
    pub fn backend_message(&self, recipe: &str, backend: &str) -> &str {
        self.recipes
            .get(recipe)
            .and_then(|r| r.backends.get(backend))
            .map_or("", |b| b.message.as_str())
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
    /// Recommended by the engine's catalogue.
    #[serde(default)]
    pub suggested: bool,
    /// Set on a model served by a provider rather than this computer.
    #[serde(default)]
    pub cloud_provider: Option<String>,
    /// The largest context the model takes, when the engine knows it.
    #[serde(default)]
    pub max_context_window: Option<u32>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f32>,
    /// A GBNF grammar the reply must follow, for llama.cpp. Sent instead of a JSON schema
    /// when the order of an object's keys matters: the engine parses the body into a map that
    /// sorts keys, so a schema reaches llama.cpp with its properties in alphabetical order,
    /// while a grammar is a string and arrives as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grammar: Option<String>,
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
            top_k: None,
            top_p: None,
            seed: None,
            presence_penalty: None,
            repeat_penalty: None,
            grammar: None,
            stream: false,
            timeout: None,
        }
    }

    /// The request with `sampling` applied: each value it sets replaces the request's own.
    pub fn sampled(mut self, sampling: &Sampling) -> ChatRequest {
        sampling.apply(&mut self);
        self
    }
}

/// How the next word is picked. A value left `None` is not sent, so the engine's own default
/// holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sampling {
    pub temperature: Option<f32>,
    pub top_k: Option<u32>,
    pub top_p: Option<f32>,
    pub seed: Option<i64>,
    pub presence_penalty: Option<f32>,
    pub repeat_penalty: Option<f32>,
}

impl Sampling {
    /// Only a temperature, as every request was sent before sampling could be set.
    pub const fn temperature(t: f32) -> Sampling {
        Sampling {
            temperature: Some(t),
            top_k: None,
            top_p: None,
            seed: None,
            presence_penalty: None,
            repeat_penalty: None,
        }
    }

    /// `self`, with every value `other` sets taking the place of its own.
    pub fn overlaid(self, other: &Sampling) -> Sampling {
        Sampling {
            temperature: other.temperature.or(self.temperature),
            top_k: other.top_k.or(self.top_k),
            top_p: other.top_p.or(self.top_p),
            seed: other.seed.or(self.seed),
            presence_penalty: other.presence_penalty.or(self.presence_penalty),
            repeat_penalty: other.repeat_penalty.or(self.repeat_penalty),
        }
    }

    pub fn apply(&self, req: &mut ChatRequest) {
        let s = req_sampling(req).overlaid(self);
        req.temperature = s.temperature;
        req.top_k = s.top_k;
        req.top_p = s.top_p;
        req.seed = s.seed;
        req.presence_penalty = s.presence_penalty;
        req.repeat_penalty = s.repeat_penalty;
    }
}

fn req_sampling(req: &ChatRequest) -> Sampling {
    Sampling {
        temperature: req.temperature,
        top_k: req.top_k,
        top_p: req.top_p,
        seed: req.seed,
        presence_penalty: req.presence_penalty,
        repeat_penalty: req.repeat_penalty,
    }
}

/// How a decision question is answered by `/v1/systemone`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SystemOneKind {
    Choice,
    Score,
    Noul,
}

/// One question for a decision model.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SystemOneQuestion {
    #[serde(rename = "type")]
    pub kind: SystemOneKind,
    pub instructions: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criteria: Option<Criteria>,
}

/// What a question is judged against: for a choice, each option with its description (or none),
/// sent as an object in this order, since the engine passes the order on and the answers depend
/// on it; for a score, the levels, lowest first.
#[derive(Debug, Clone, PartialEq)]
pub enum Criteria {
    Options(Vec<(String, Option<String>)>),
    Levels(Vec<String>),
}

impl Serialize for Criteria {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            Criteria::Options(options) => {
                let mut map = s.serialize_map(Some(options.len()))?;
                for (option, description) in options {
                    map.serialize_entry(option, description)?;
                }
                map.end()
            }
            Criteria::Levels(levels) => {
                let mut seq = s.serialize_seq(Some(levels.len()))?;
                for level in levels {
                    seq.serialize_element(level)?;
                }
                seq.end()
            }
        }
    }
}

/// Questions about one state, as `POST /v1/systemone` takes them.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SystemOneRequest {
    pub model: String,
    pub state: String,
    pub questions: BTreeMap<String, SystemOneQuestion>,
}

/// A decision model's answer to one question. Each field is what the engine sent for that kind
/// of question and is left out of the others.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct SystemOneAnswer {
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub choice: Option<String>,
    /// The expected level of a score.
    #[serde(default)]
    pub score: Option<f64>,
    /// The probability that a `noul` question is true.
    #[serde(default)]
    pub noul: Option<f64>,
    /// Keyed by option for a choice and by level index for a score.
    #[serde(default)]
    pub probabilities: Option<BTreeMap<String, f64>>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub truncated: bool,
    /// Set for one question that FastFlowLM could not take, such as `max_length_exceeded`.
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct SystemOneReply {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub answers: BTreeMap<String, SystemOneAnswer>,
}

/// A whole reply, and whether the engine stopped it at the token ceiling.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatReply {
    pub text: String,
    pub cut: bool,
}

/// The body of an online question, and nothing more.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OnlineRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: f32,
    pub max_tokens: u32,
    /// Overall limit for this call.
    #[serde(skip)]
    pub timeout: Option<Duration>,
}

/// What came back from an online question that the engine answered with success.
#[derive(Debug, Clone, PartialEq)]
pub struct OnlineReply {
    pub status: u16,
    pub text: String,
    pub bytes_up: u64,
    pub bytes_down: u64,
    /// `usage.cost`, only when the provider reported it as a number.
    pub cost_usd: Option<f64>,
}

/// A failed online question. `message` is the engine's own `error.message`; the provider's body
/// is never kept. `status` is `None` when the engine itself did not answer.
#[derive(Debug, Clone, PartialEq)]
pub struct OnlineCallError {
    pub status: Option<u16>,
    pub message: String,
    pub bytes_up: u64,
    pub bytes_down: u64,
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
        let mut info: SystemInfo =
            Self::json(self.request(Method::GET, "/v1/system-info")?).await?;
        if self.shared.host_graphics.load(Ordering::SeqCst) {
            info.devices.add_intel(crate::graphics::intel_gpus);
        }
        Ok(info)
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

    /// Registers a model from a checkpoint under `model` (`user.<name>`) and pulls it, with the
    /// labels the engine needs to route it. A llama.cpp decision model is labelled
    /// `classification` and `systemone`.
    pub async fn pull_registered(
        &self,
        model: &str,
        checkpoint: &str,
        recipe: &str,
        labels: &[&str],
    ) -> Result<Download, RuntimeError> {
        self.post_json(
            "/v1/pull",
            &json!({
                "model_name": model,
                "checkpoint": checkpoint,
                "recipe": recipe,
                "labels": labels,
                "stream": true,
                "subscribe": false,
            }),
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

    /// Deletes a model's files (`POST /v1/delete`), unloading it first.
    pub async fn delete(&self, model: &str) -> Result<(), RuntimeError> {
        let _: Value = self
            .post_json("/v1/delete", &json!({"model_name": model}))
            .await?;
        Ok(())
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
        self.chat_reply(req).await.map(|r| r.text)
    }

    /// The whole answer at once, saying whether it was cut off at `max_tokens`.
    pub async fn chat_reply(&self, req: &ChatRequest) -> Result<ChatReply, RuntimeError> {
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
        let choice = &v["choices"][0];
        let text = choice["message"]["content"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| RuntimeError::BadResponse("no message in the answer".into()))?;
        Ok(ChatReply {
            text,
            cut: choice["finish_reason"] == "length",
        })
    }

    /// The whole answer, read as it arrives. After each piece `stop` is shown the text so far;
    /// once it says so, the request is dropped, which ends it in the engine as well, and the
    /// reply is what had come. `req.timeout` (or the JSON limit) bounds the whole call.
    pub async fn chat_watched(
        &self,
        req: &ChatRequest,
        mut stop: impl FnMut(&str) -> bool,
    ) -> Result<ChatReply, RuntimeError> {
        let _busy = self.busy(&req.model);
        let mut req = req.clone();
        req.stream = true;
        let limit = req.timeout.unwrap_or(self.shared.timeouts.json);
        let read = async {
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
            let mut pieces = sse_pieces(response, first, self.shared.timeouts.idle);
            let mut reply = ChatReply {
                text: String::new(),
                cut: false,
            };
            while let Some(piece) = pieces.next().await {
                match piece? {
                    Piece::Text(t) => {
                        reply.text.push_str(&t);
                        if stop(&reply.text) {
                            break;
                        }
                    }
                    Piece::End(why) => reply.cut = why == "length",
                }
            }
            Ok(reply)
        };
        tokio::time::timeout(limit, read)
            .await
            .map_err(|_| RuntimeError::Timeout)?
    }

    /// One online question through the engine route: a non-streaming chat with exactly
    /// `model`, `messages`, `temperature` and `max_tokens`, nothing else. The engine passes the
    /// provider's status through; on failure only the engine's own `error.message` is kept,
    /// never the provider's body (`error.details.response`), which can echo the key.
    pub async fn chat_online(&self, req: &OnlineRequest) -> Result<OnlineReply, OnlineCallError> {
        let body = serde_json::to_vec(req).unwrap_or_default();
        let bytes_up = body.len() as u64;
        let fail = |status: Option<u16>, message: String, bytes_down: u64| OnlineCallError {
            status,
            message,
            bytes_up,
            bytes_down,
        };
        let rb = self
            .build(
                &self.shared.online_http,
                Method::POST,
                "/v1/chat/completions",
                true,
            )
            .map_err(|_| fail(None, String::new(), 0))?
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body);
        let rb = match req.timeout {
            Some(t) => rb.timeout(t),
            None => rb,
        };
        let response = rb.send().await.map_err(|e| {
            let message = if e.is_timeout() { "timed out" } else { "" };
            fail(None, message.into(), 0)
        })?;
        let status = response.status().as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| fail(Some(status), String::new(), 0))?;
        let bytes_down = bytes.len() as u64;
        if !(200..300).contains(&status) {
            let (_, message) = parse_error(&String::from_utf8_lossy(&bytes));
            return Err(fail(Some(status), message, bytes_down));
        }
        let v: Value = serde_json::from_slice(&bytes)
            .map_err(|_| fail(Some(status), String::new(), bytes_down))?;
        Ok(OnlineReply {
            status,
            text: v["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            bytes_up,
            bytes_down,
            cost_usd: v["usage"]["cost"].as_f64(),
        })
    }

    /// Asks a decision model typed questions about a state. Experimental in the engine: only a
    /// model labelled `classification` and `systemone` serves it, and one that is not on disk is
    /// fetched first, so callers check it is present before asking.
    pub async fn systemone(&self, req: &SystemOneRequest) -> Result<SystemOneReply, RuntimeError> {
        let _busy = self.busy(&req.model);
        let rb = self
            .request(Method::POST, "/v1/systemone")?
            .json(req)
            .timeout(self.shared.timeouts.json);
        Self::json(rb).await
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
        let deltas = sse_pieces(response, first, self.shared.timeouts.idle).filter_map(|p| {
            std::future::ready(match p {
                Ok(Piece::Text(t)) => Some(Ok(t)),
                Ok(Piece::End(_)) => None,
                Err(e) => Some(Err(e)),
            })
        });
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

    /// The vector for a question, made the way `model` wants a search query made: see
    /// [`query_text`]. What it is searched against is embedded as it is.
    pub async fn embed_query(&self, model: &str, question: &str) -> Result<Vec<f32>, RuntimeError> {
        let vecs = self
            .embeddings(model, &[query_text(model, question)])
            .await?;
        vecs.into_iter()
            .next()
            .ok_or_else(|| RuntimeError::BadResponse("no embedding in the answer".into()))
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

/// What a search for memories asks the recall model to look for.
pub const QUERY_TASK: &str = "Given a question a person asks about their own life, retrieve the notes they kept that answer it";

/// A question as `model` wants a search query written. Qwen3-Embedding is trained with a task
/// line before each query, written as its model card does (`Instruct: {task}\nQuery:{query}`),
/// and with none before the documents searched; other models get the question as it is.
pub fn query_text(model: &str, question: &str) -> String {
    if model.to_ascii_lowercase().starts_with("qwen3-embedding") {
        format!("Instruct: {QUERY_TASK}\nQuery:{question}")
    } else {
        question.to_string()
    }
}

pub(crate) use nocturne_mind::prompts::response_format as json_schema_format;

/// Models sometimes wrap JSON in a code fence; take what is inside.
pub(crate) fn parse_json_text<T: DeserializeOwned>(text: &str) -> Result<T, RuntimeError> {
    nocturne_mind::reading::json_text(text).map_err(|e| RuntimeError::BadResponse(e.to_string()))
}

/// A piece of a streamed reply: some of its text, or the engine's reason for ending it.
enum Piece {
    Text(String),
    End(String),
}

struct SseState {
    body: BoxStream<'static, Result<bytes::Bytes, reqwest::Error>>,
    buf: Vec<u8>,
    ready: VecDeque<Result<Piece, RuntimeError>>,
    finished: bool,
    complete: bool,
    started: bool,
    first: Duration,
    idle: Duration,
}

fn sse_pieces(
    response: Response,
    first: Duration,
    idle: Duration,
) -> BoxStream<'static, Result<Piece, RuntimeError>> {
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
                            // As an error before the stream: a refused request is Rejected.
                            let status = e["status_code"].as_u64().or(e["code"].as_u64());
                            let status = status.unwrap_or(502) as u16;
                            st.ready.push_back(Err(map_status(status, data)));
                            st.finished = true;
                            break;
                        }
                        let choice = &v["choices"][0];
                        if let Some(text) = choice["delta"]["content"].as_str()
                            && !text.is_empty()
                        {
                            st.started = true;
                            st.ready.push_back(Ok(Piece::Text(text.to_string())));
                        }
                        if let Some(why) = choice["finish_reason"].as_str() {
                            st.ready.push_back(Ok(Piece::End(why.to_string())));
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
    // An engine that cannot be reached is unavailable, however the connection failed. Windows
    // takes about two seconds to refuse a connection to a port nobody listens on, so there the
    // connect limit can run out first.
    if e.is_connect() {
        RuntimeError::Unavailable
    } else if e.is_timeout() {
        RuntimeError::Timeout
    } else if e.is_decode() || e.is_body() {
        RuntimeError::StreamCut
    } else {
        RuntimeError::Unavailable
    }
}

/// How much of an error body that is not JSON is kept: enough to tell what kind of failure it
/// was, never a whole body.
const UNPARSED_KEPT: usize = 120;

/// Reads `{"error": "text"}` and `{"error": {"message", "code"}}` alike. Only `code` and
/// `message` are kept: `details` can carry an upstream body. A body that is not JSON keeps only
/// the start of its first line.
fn parse_error(text: &str) -> (String, String) {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return (String::new(), unparsed_start(text));
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
        _ => (String::new(), String::new()),
    }
}

fn unparsed_start(text: &str) -> String {
    let line = text.trim().lines().next().unwrap_or_default();
    let mut end = line.len().min(UNPARSED_KEPT);
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    line[..end].to_string()
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
    #[test]
    fn intel_graphics_from_the_computer_never_replace_the_engines_own() {
        let found = || {
            vec![Gpu {
                available: true,
                ..Gpu::default()
            }]
        };
        let mut none = Devices::default();
        none.add_intel(found);
        assert_eq!(none.intel_gpu.len(), 1);
        assert!(none.has_graphics());
        let mut reported = Devices {
            intel_gpu: vec![Gpu {
                name: "from the engine".into(),
                ..Gpu::default()
            }],
            ..Devices::default()
        };
        reported.add_intel(|| panic!("the engine already listed Intel graphics"));
        assert_eq!(reported.intel_gpu[0].name, "from the engine");
    }

    use super::*;

    fn body(code: &str, message: &str) -> String {
        json!({"error": {"code": code, "message": message}}).to_string()
    }

    #[test]
    fn an_error_keeps_its_message_and_never_the_details() {
        let echoed = "sk-or-v1-0123456789abcdef0123456789abcdef";
        let body = json!({"error": {
            "message": "cloud (nocturne_online) request failed",
            "type": "backend_error",
            "details": {"status_code": 401, "response": {"error": {"message": echoed}}},
        }})
        .to_string();
        let (_, message) = parse_error(&body);
        assert_eq!(message, "cloud (nocturne_online) request failed");
        let shown = map_status(500, &body).to_string();
        assert!(!shown.contains("0123456789abcdef"), "{shown}");
        let (_, other) = parse_error(&json!({"detail": echoed}).to_string());
        assert!(other.is_empty());
    }

    #[test]
    fn a_body_that_is_not_json_keeps_only_the_start_of_its_first_line() {
        let long = format!(
            "out of memory {}\nsecond line {}",
            "x".repeat(400),
            "sk-secret"
        );
        let (_, message) = parse_error(&long);
        assert!(message.starts_with("out of memory"));
        assert!(message.len() <= UNPARSED_KEPT);
        assert!(!message.contains("sk-secret"));
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
