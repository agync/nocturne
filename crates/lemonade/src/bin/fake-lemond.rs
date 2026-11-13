//! A small stand-in for `lemond` that speaks the parts of its HTTP API Nocturne uses, with
//! deterministic canned answers. It is the only engine the hermetic tests run.
//!
//! Behaviour is selected by a comma-separated mode string, read from the `FAKE_LEMOND_MODE`
//! environment variable or, because the supervisor scrubs the environment, from a
//! `fake_mode` key in `config.json`. Modes: `crash-after-ms=N`, `crash-first-after-ms=N`,
//! `slow-start-ms=N`, `fail-bind`, `fail-bind-first=N`, `no-progress`, `disk-full`,
//! `download-ms=N`, `free-bytes=N`, `gpu=none|integrated|discrete`, `downloaded=all`,
//! `spawn-sleeper` (a helper process that the engine cleans up), `leak-sleeper` (one it leaves),
//! `hang-shutdown`, `no-watchdog`, `ignore-key`, `bad-ws-port`, `half-present`, `pause-lag-ms=N`,
//! `conflict-first-remove`, `load-oom`, `load-error`. `POST /fake/crash` kills it on demand.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{Multipart, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Map, Value, json};

const CATALOG: &[(&str, f64, &str, &str)] = &[
    ("Qwen3.6-35B-A3B-MTP-GGUF", 23.8, "llamacpp", "chat,vision"),
    (
        "Whisper-Large-v3-Turbo",
        1.62,
        "whispercpp",
        "transcription",
    ),
    ("Qwen3-Embedding-0.6B-GGUF", 0.64, "llamacpp", "embeddings"),
    ("Qwen3.5-9B-GGUF", 6.88, "llamacpp", "chat,vision"),
    ("Whisper-Small", 0.488, "whispercpp", "transcription"),
    ("Qwen3.5-4B-MTP-GGUF", 3.66, "llamacpp", "chat,vision"),
    ("Whisper-Base", 0.148, "whispercpp", "transcription"),
    ("kokoro-v1", 0.354, "kokoro", "tts"),
    ("Whisper-Tiny", 0.075, "whispercpp", "transcription"),
];

const SCALE: f64 = 5000.0;
const BACKEND_BYTES: u64 = 100_000;

#[derive(Clone, Debug, PartialEq)]
enum Status {
    Downloading,
    Paused,
    Error(String),
}

#[derive(Clone, Debug)]
struct Job {
    id: String,
    kind: &'static str,
    name: String,
    total: u64,
    base: u64,
    started: Instant,
    status: Status,
    stopped_at: Option<Instant>,
}

#[derive(Default)]
struct Inner {
    partial: BTreeMap<String, u64>,
    downloaded: Vec<String>,
    backends: Vec<String>,
    jobs: Vec<Job>,
    log: Vec<Value>,
    script: VecDeque<Value>,
    config: Map<String, Value>,
    loaded: Option<String>,
    pulls: u64,
    fetched_on_use: Vec<String>,
    conflicts: u64,
}

struct App {
    key: String,
    port: u16,
    ws_port: u16,
    modes: RwLock<HashMap<String, String>>,
    cache: PathBuf,
    models_dir: String,
    sleeper: Mutex<Option<std::process::Child>>,
    leak: bool,
    crash_ms: Mutex<Option<u64>>,
    inner: Mutex<Inner>,
}

type Shared = Arc<App>;

impl App {
    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn mode(&self, key: &str) -> Option<String> {
        self.modes
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned()
    }

    fn mode_u64(&self, key: &str) -> Option<u64> {
        self.mode(key).and_then(|v| v.parse().ok())
    }

    fn flag(&self, key: &str) -> bool {
        self.mode(key).is_some()
    }

    fn download_ms(&self) -> f64 {
        self.mode_u64("download-ms").unwrap_or(600) as f64
    }

    fn model_total(&self, name: &str) -> Option<u64> {
        CATALOG
            .iter()
            .find(|m| m.0 == name)
            .map(|m| (m.1 * 1e9 / SCALE) as u64)
    }

    fn job_bytes(&self, job: &Job) -> u64 {
        if job.status != Status::Downloading {
            return job.base;
        }
        let rate = job.total as f64 / self.download_ms() / 1000.0;
        let grown = job.base as f64 + rate * job.started.elapsed().as_micros() as f64;
        let mut cap = job.total as f64;
        if self.flag("no-progress") {
            cap = cap.min(job.total as f64 * 0.3);
        }
        if self.flag("disk-full") {
            cap = cap.min(job.total as f64 * 0.5);
        }
        grown.min(cap) as u64
    }

    /// Moves finished and failed jobs along. A finished job leaves the list.
    fn tick(&self, inner: &mut Inner) {
        let jobs = std::mem::take(&mut inner.jobs);
        for mut job in jobs {
            if job.status == Status::Downloading {
                let bytes = self.job_bytes(&job);
                if bytes >= job.total {
                    inner.partial.remove(&job.name);
                    match job.kind {
                        "model" => {
                            if !inner.downloaded.contains(&job.name) {
                                inner.downloaded.push(job.name.clone());
                            }
                        }
                        _ => {
                            if !inner.backends.contains(&job.name) {
                                inner.backends.push(job.name.clone());
                            }
                        }
                    }
                    continue;
                }
                if self.flag("disk-full") && bytes as f64 >= job.total as f64 * 0.5 {
                    job.base = bytes;
                    job.status = Status::Error(format!(
                        "Insufficient disk space: download requires 2.4 GB but only 0.3 GB is available on {}",
                        self.models_dir
                    ));
                }
            }
            inner.jobs.push(job);
        }
        self.persist_locked(inner);
    }

    fn persist_locked(&self, inner: &mut Inner) {
        let mut partial = inner.partial.clone();
        for job in &inner.jobs {
            partial.insert(job.name.clone(), self.job_bytes(job));
        }
        let state = json!({
            "partial": partial,
            "downloaded": inner.downloaded,
            "backends": inner.backends,
        });
        let staged = self.cache.join("fake-state.json.tmp");
        if std::fs::write(&staged, state.to_string()).is_ok() {
            let _ = std::fs::rename(&staged, self.cache.join("fake-state.json"));
        }
    }

    /// Like the real engine, any use of a registered model that is not on disk downloads it first.
    fn touch_model(&self, name: &str) {
        if !CATALOG.iter().any(|m| m.0 == name) {
            return;
        }
        let mut inner = self.inner();
        if !inner.downloaded.iter().any(|d| d == name) {
            inner.downloaded.push(name.to_string());
            inner.fetched_on_use.push(name.to_string());
        }
    }

    fn persist(&self) {
        let mut inner = self.inner();
        self.persist_locked(&mut inner);
    }

    /// Ends a helper process the way the real engine ends its backends, unless told to leak it.
    fn cleanup(&self) {
        self.persist();
        if self.leak {
            return;
        }
        if let Some(mut child) = self
            .sleeper
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// A stopped job's worker keeps reporting `running` for a moment, as the real one does.
    fn job_running(&self, job: &Job) -> bool {
        if job.status == Status::Downloading {
            return true;
        }
        let lag = Duration::from_millis(self.mode_u64("pause-lag-ms").unwrap_or(0));
        job.stopped_at.is_some_and(|t| t.elapsed() < lag)
    }

    fn snapshot(&self, job: &Job) -> Value {
        let bytes = self.job_bytes(job);
        let status = match &job.status {
            Status::Downloading => "downloading",
            Status::Paused => "paused",
            Status::Error(_) => "error",
        };
        let mut v = json!({
            "id": job.id,
            "type": job.kind,
            "model_name": job.name,
            "status": status,
            "running": self.job_running(job),
            "file": "fake.gguf",
            "file_index": 1,
            "total_files": 1,
            "bytes_downloaded": bytes,
            "bytes_total": job.total,
            "percent": if job.total == 0 { 0.0 } else { bytes as f64 * 100.0 / job.total as f64 },
            "total_download_size": job.total,
            "bytes_previously_downloaded": 0,
            "completed_files_bytes": 0,
            "cumulative_bytes_downloaded": bytes,
            "overall_bytes_downloaded": bytes,
            "complete": false,
        });
        if let Status::Error(msg) = &job.status {
            v["error"] = json!(msg);
        }
        v
    }
}

fn err(status: u16, code: &str, message: &str) -> Response {
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(json!({"error": {"message": message, "code": code, "type": "error"}})),
    )
        .into_response()
}

fn modes_has(app: &App, key: &str) -> bool {
    app.flag(key)
}

fn parse_modes(s: &str) -> HashMap<String, String> {
    s.split(',')
        .filter(|p| !p.trim().is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (k.trim().to_string(), v.trim().to_string()),
            None => (p.trim().to_string(), String::new()),
        })
        .collect()
}

fn read_config(dir: &Path) -> Map<String, Value> {
    std::fs::read(dir.join("config.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut positional = Vec::new();
    let mut port = 0u16;
    let mut watchdog = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                port = args.get(i + 1).and_then(|p| p.parse().ok()).unwrap_or(0);
                i += 1;
            }
            "--host" | "--log-file" => i += 1,
            "--watchdog-fd" => {
                watchdog = true;
                i += 1;
            }
            "--no-broadcast" => {}
            other => positional.push(other.to_string()),
        }
        i += 1;
    }
    let (cache, config_dir) = match positional.as_slice() {
        [c, k, ..] => (PathBuf::from(c), PathBuf::from(k)),
        _ => {
            eprintln!("usage: fake-lemond <cache> <config> --port P");
            std::process::exit(2);
        }
    };
    let config = read_config(&config_dir);
    let mode_str = std::env::var("FAKE_LEMOND_MODE")
        .ok()
        .or_else(|| {
            config
                .get("fake_mode")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .unwrap_or_default();
    let modes = parse_modes(&mode_str);
    let key = std::env::var("LEMONADE_API_KEY").unwrap_or_default();

    let counter_path = cache.join("fake-launches");
    let launches = std::fs::read_to_string(&counter_path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0)
        + 1;
    // Written to a temporary name and renamed, so a reader never sees a half-written count.
    let staged = cache.join("fake-launches.tmp");
    if std::fs::write(&staged, launches.to_string()).is_ok() {
        let _ = std::fs::rename(&staged, &counter_path);
    }

    if let Some(ms) = modes
        .get("slow-start-ms")
        .and_then(|v| v.parse::<u64>().ok())
    {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
    let fail_first = modes
        .get("fail-bind-first")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    if modes.contains_key("fail-bind") || launches <= fail_first {
        eprintln!("address already in use");
        std::process::exit(98);
    }
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("bind failed: {e}");
            std::process::exit(98);
        }
    };
    let ws_port = config
        .get("websocket_port")
        .and_then(|v| v.as_u64())
        .unwrap_or(9000) as u16;
    let _ws_listener = tokio::net::TcpListener::bind(("127.0.0.1", ws_port))
        .await
        .ok();

    let mut inner = Inner {
        config: config.clone(),
        ..Inner::default()
    };
    if let Ok(bytes) = std::fs::read(cache.join("fake-state.json"))
        && let Ok(v) = serde_json::from_slice::<Value>(&bytes)
    {
        if let Some(map) = v["partial"].as_object() {
            for (k, n) in map {
                inner.partial.insert(k.clone(), n.as_u64().unwrap_or(0));
            }
        }
        inner.downloaded = serde_json::from_value(v["downloaded"].clone()).unwrap_or_default();
        inner.backends = serde_json::from_value(v["backends"].clone()).unwrap_or_default();
    }
    if modes.get("downloaded").map(String::as_str) == Some("all") {
        inner.downloaded = CATALOG.iter().map(|m| m.0.to_string()).collect();
    }
    let models_dir = config
        .get("models_dir")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let sleeper = if modes.contains_key("spawn-sleeper") || modes.contains_key("leak-sleeper") {
        std::process::Command::new("sleep").arg("300").spawn().ok()
    } else {
        None
    };
    let leak = modes.contains_key("leak-sleeper");
    let app: Shared = Arc::new(App {
        key,
        port,
        ws_port,
        modes: RwLock::new(modes),
        sleeper: Mutex::new(sleeper),
        leak,
        crash_ms: Mutex::new(None),
        cache,
        models_dir,
        inner: Mutex::new(inner),
    });

    if watchdog && !modes_has(&app, "no-watchdog") {
        let app = app.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = [0u8; 256];
            let mut stdin = std::io::stdin();
            while matches!(stdin.read(&mut buf), Ok(n) if n > 0) {}
            app.cleanup();
            std::process::exit(0);
        });
    }
    // The crash clock starts when the engine is first asked for its health, that is, when the
    // supervisor has taken it as ready, so a slow start cannot be mistaken for a crash at launch.
    *app.crash_ms.lock().unwrap_or_else(|e| e.into_inner()) =
        app.mode_u64("crash-after-ms").or_else(|| {
            (launches == 1)
                .then(|| app.mode_u64("crash-first-after-ms"))
                .flatten()
        });

    let versioned = Router::new()
        .route("/health", get(health))
        .route("/system-info", get(system_info))
        .route("/models", get(models))
        .route("/pull", post(pull))
        .route("/install", post(install))
        .route("/downloads", get(downloads))
        .route("/downloads/control", post(control))
        .route("/load", post(load))
        .route("/unload", post(unload))
        .route("/chat/completions", post(chat))
        .route("/embeddings", post(embeddings))
        .route("/audio/transcriptions", post(transcriptions))
        .route("/audio/speech", post(speech));
    let router = Router::new()
        .route("/live", get(|| async { Json(json!({"status": "ok"})) }))
        .nest("/v1", versioned.clone())
        .nest("/api/v1", versioned)
        .route("/internal/shutdown", post(shutdown))
        .route("/internal/set", post(set_config))
        .route("/internal/config", get(get_config))
        .route("/fake/env", get(fake_env))
        .route("/fake/requests", get(fake_requests))
        .route("/fake/script", post(fake_script))
        .route("/fake/mode", post(fake_mode))
        .route("/fake/crash", post(fake_crash))
        .route("/fake/stats", get(fake_stats))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app);
    let _ = axum::serve(listener, router).await;
}

async fn guard(State(app): State<Shared>, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    if path == "/live" {
        return next.run(req).await;
    }
    let supplied = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let authorised = app.flag("ignore-key")
        || supplied.as_deref() == Some(format!("Bearer {}", app.key).as_str());
    let method = req.method().to_string();
    let query = req.uri().query().unwrap_or("").to_string();
    let is_fake = path.starts_with("/fake/");
    let is_json = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    let (parts, body) = req.into_parts();
    let bytes = axum::body::to_bytes(body, 64 * 1024 * 1024)
        .await
        .unwrap_or_default();
    if !is_fake {
        let body_json = if is_json {
            serde_json::from_slice::<Value>(&bytes).ok()
        } else {
            None
        };
        app.inner().log.push(json!({
            "method": method,
            "path": path,
            "query": query,
            "authorised": authorised,
            "content_type": parts.headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()),
            "body": body_json,
        }));
    }
    if !authorised {
        return err(401, "unauthorized", "invalid or missing API key");
    }
    next.run(Request::from_parts(parts, Body::from(bytes)))
        .await
}

async fn health(State(app): State<Shared>) -> Json<Value> {
    let crash = app
        .crash_ms
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some(ms) = crash {
        let app = app.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            app.persist();
            std::process::exit(1);
        });
    }
    let loaded = app.inner().loaded.clone();
    Json(json!({
        "status": "ok",
        "version": "fake-1.0",
        "model_loaded": loaded,
        "all_models_loaded": loaded.iter().map(|m| json!({"model_name": m, "type": "llm"})).collect::<Vec<_>>(),
        "websocket_port": if app.flag("bad-ws-port") { app.ws_port.wrapping_add(1) } else { app.ws_port },
        "max_models": {"llm": 1, "embedding": 1},
    }))
}

async fn system_info(State(app): State<Shared>) -> Json<Value> {
    let mut inner = app.inner();
    app.tick(&mut inner);
    let gpu_mode = app.mode("gpu");
    let gpu = gpu_mode.as_deref().unwrap_or("integrated");
    let amd_gpu = match gpu {
        "none" => json!([]),
        "discrete" => {
            json!([{"available": true, "family": "gfx1100", "integrated": false, "vram_gb": 16.0}])
        }
        _ => json!([{"available": true, "family": "gfx1150", "integrated": true, "vram_gb": 0.5}]),
    };
    let state = |recipe: &str, backend: &str| -> &'static str {
        if inner.backends.contains(&format!("{recipe}:{backend}")) {
            "installed"
        } else if backend != "cpu" && gpu == "none" {
            "unsupported"
        } else {
            "installable"
        }
    };
    let recipe = |name: &str, backends: &[&str]| -> Value {
        let map: Map<String, Value> = backends
            .iter()
            .map(|b| {
                (
                    b.to_string(),
                    json!({"state": state(name, b), "devices": ["cpu"]}),
                )
            })
            .collect();
        json!({"backends": map})
    };
    Json(json!({
        "OS Version": "Fake",
        "devices": {"amd_gpu": amd_gpu, "amd_npu": {"available": false}, "cpu": {"available": true}},
        "model_storage": {
            "free_bytes": app.mode_u64("free-bytes").unwrap_or(5_000_000_000_000),
            "path": app.models_dir,
        },
        "recipes": {
            "llamacpp": recipe("llamacpp", &["rocm", "vulkan", "cpu"]),
            "whispercpp": recipe("whispercpp", &["vulkan", "cpu"]),
            "kokoro": recipe("kokoro", &["cpu"]),
        }
    }))
}

async fn models(State(app): State<Shared>) -> Json<Value> {
    let mut inner = app.inner();
    app.tick(&mut inner);
    let half = app.flag("half-present");
    let data: Vec<Value> = CATALOG
        .iter()
        .map(|(name, size, recipe, labels)| {
            let present = inner.downloaded.iter().any(|d| d == name);
            json!({
                "id": name,
                "object": "model",
                "recipe": recipe,
                "size": size,
                "downloaded": present,
                "labels": labels.split(',').collect::<Vec<_>>(),
                "checkpoint_status": [
                    {"type": "main", "present": present && !half, "required": true},
                    {"type": "npu_cache", "present": false, "required": false},
                ],
            })
        })
        .collect();
    Json(json!({"object": "list", "data": data}))
}

fn start_job(app: &App, inner: &mut Inner, kind: &'static str, name: String, total: u64) -> Value {
    let id = if kind == "model" {
        format!("model:{name}")
    } else {
        format!("backend:{name}")
    };
    inner.pulls += 1;
    if let Some(existing) = inner.jobs.iter_mut().find(|j| j.id == id) {
        if existing.status != Status::Downloading {
            existing.status = Status::Downloading;
            existing.started = Instant::now();
        }
        return app.snapshot(&existing.clone());
    }
    let base = inner.partial.get(&name).copied().unwrap_or(0).min(total);
    let job = Job {
        id,
        kind,
        name,
        total,
        base,
        started: Instant::now(),
        status: Status::Downloading,
        stopped_at: None,
    };
    let snap = app.snapshot(&job);
    inner.jobs.push(job);
    snap
}

async fn pull(State(app): State<Shared>, Json(body): Json<Value>) -> Response {
    let name = body["model_name"]
        .as_str()
        .or_else(|| body["model"].as_str())
        .unwrap_or("")
        .to_string();
    let mut inner = app.inner();
    app.tick(&mut inner);
    if inner.config.get("offline") == Some(&json!(true)) {
        return err(400, "lemond_offline", "offline mode is on");
    }
    let Some(total) = app.model_total(&name) else {
        return err(400, "unknown_model", "model is not registered");
    };
    if inner.downloaded.contains(&name) {
        let mut snap = json!({"id": format!("model:{name}"), "type": "model", "model_name": name,
            "status": "completed", "running": false, "complete": true,
            "total_download_size": total, "cumulative_bytes_downloaded": 0});
        snap["bytes_downloaded"] = json!(0);
        return Json(snap).into_response();
    }
    Json(start_job(&app, &mut inner, "model", name, total)).into_response()
}

async fn install(State(app): State<Shared>, Json(body): Json<Value>) -> Response {
    let recipe = body["recipe"].as_str().unwrap_or("").to_string();
    let backend = body["backend"].as_str().unwrap_or("").to_string();
    let mut inner = app.inner();
    app.tick(&mut inner);
    if inner.config.get("offline") == Some(&json!(true)) {
        return err(400, "lemond_offline", "offline mode is on");
    }
    let name = format!("{recipe}:{backend}");
    if inner.backends.contains(&name) {
        return Json(json!({"status": "success", "recipe": recipe, "backend": backend}))
            .into_response();
    }
    Json(start_job(&app, &mut inner, "backend", name, BACKEND_BYTES)).into_response()
}

async fn downloads(State(app): State<Shared>) -> Json<Value> {
    let mut inner = app.inner();
    app.tick(&mut inner);
    Json(Value::Array(
        inner.jobs.iter().map(|j| app.snapshot(j)).collect(),
    ))
}

async fn control(State(app): State<Shared>, Json(body): Json<Value>) -> Response {
    let id = body["id"].as_str().unwrap_or("").to_string();
    let action = body["action"].as_str().unwrap_or("").to_string();
    let mut inner = app.inner();
    app.tick(&mut inner);
    let Some(pos) = inner.jobs.iter().position(|j| j.id == id) else {
        return Json(json!({"status": "ok", "missing": true})).into_response();
    };
    let bytes = app.job_bytes(&inner.jobs[pos]);
    match action.as_str() {
        "pause" | "cancel" => {
            let job = &mut inner.jobs[pos];
            job.base = bytes;
            job.status = Status::Paused;
            job.stopped_at = Some(Instant::now());
            let snap = app.snapshot(job);
            app.persist_locked(&mut inner);
            Json(snap).into_response()
        }
        "remove" => {
            let busy = app.job_running(&inner.jobs[pos]);
            let first = app.flag("conflict-first-remove") && inner.conflicts == 0;
            if busy || first {
                inner.conflicts += 1;
                return err(409, "conflict", "the download worker is still running");
            }
            let job = inner.jobs.remove(pos);
            inner.partial.insert(job.name, bytes);
            app.persist_locked(&mut inner);
            Json(json!({"status": "ok"})).into_response()
        }
        _ => err(400, "invalid_argument", "unknown action"),
    }
}

async fn load(State(app): State<Shared>, Json(body): Json<Value>) -> Response {
    let name = body["model_name"].as_str().unwrap_or("").to_string();
    if app.flag("load-oom") {
        return err(
            500,
            "model_load_error",
            "llama-server exited: failed to allocate buffer (out of memory)",
        );
    }
    if app.flag("load-error") {
        return err(500, "model_load_error", "backend did not start");
    }
    let mut inner = app.inner();
    app.tick(&mut inner);
    if !CATALOG.iter().any(|m| m.0 == name) {
        return err(404, "model_not_found", "model is not registered");
    }
    if !inner.downloaded.contains(&name) {
        inner.downloaded.push(name.clone());
        inner.fetched_on_use.push(name.clone());
    }
    inner.loaded = Some(name.clone());
    Json(json!({"status": "success", "model_name": name})).into_response()
}

async fn unload(State(app): State<Shared>, body: Option<Json<Value>>) -> Json<Value> {
    let mut inner = app.inner();
    let _ = body;
    inner.loaded = None;
    Json(json!({"status": "success"}))
}

fn default_chat_content(body: &Value) -> String {
    let fmt = &body["response_format"];
    if fmt["type"] == "json_schema" {
        let schema = fmt["json_schema"]["schema"].to_string();
        if schema.contains("memories") {
            return json!({"memories": []}).to_string();
        }
        if schema.contains("headline") {
            return json!({"headline": "A quiet day", "dek": "Nothing much happened."}).to_string();
        }
        return json!({"title": "Fake", "text": "fake text", "summary": "s", "language": "en"})
            .to_string();
    }
    "The fake engine says so [m1].".to_string()
}

async fn chat(State(app): State<Shared>, Json(body): Json<Value>) -> Response {
    app.touch_model(body["model"].as_str().unwrap_or(""));
    let scripted = app.inner().script.pop_front();
    let spec = scripted.unwrap_or_else(|| json!({"content": default_chat_content(&body)}));
    if let Some(status) = spec["status"].as_u64() {
        return err(
            status as u16,
            "scripted",
            spec["error"].as_str().unwrap_or("scripted failure"),
        );
    }
    let streaming = body["stream"] == true;
    let deltas: Vec<String> = match spec["deltas"].as_array() {
        Some(d) => d
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect(),
        None => spec["content"]
            .as_str()
            .unwrap_or("")
            .split_inclusive(' ')
            .map(String::from)
            .collect(),
    };
    if !streaming {
        let content: String = deltas.concat();
        return Json(json!({
            "id": "chatcmpl-fake", "object": "chat.completion", "model": body["model"],
            "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}],
        }))
        .into_response();
    }
    let cut = spec["cut"] == true;
    let mut delay = Duration::from_millis(spec["delay_ms"].as_u64().unwrap_or(0));
    if cut {
        delay = delay.max(Duration::from_millis(30));
    }
    let reasoning = spec["reasoning"].as_str().map(String::from);
    let stream = paced(sse_items(deltas, cut, reasoning), delay);
    let mut resp = Response::new(Body::from_stream(stream));
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    resp
}

fn sse_items(
    deltas: Vec<String>,
    cut: bool,
    reasoning: Option<String>,
) -> Vec<Result<String, std::io::Error>> {
    let chunk = |delta: Value, finish: Value| {
        format!(
            "data: {}\n\n",
            json!({"id": "chatcmpl-fake", "object": "chat.completion.chunk",
                "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]})
        )
    };
    let mut items = Vec::new();
    if let Some(r) = reasoning {
        items.push(Ok(chunk(
            json!({"role": "assistant", "reasoning_content": r}),
            Value::Null,
        )));
    }
    items.extend(
        deltas
            .into_iter()
            .map(|d| Ok(chunk(json!({"content": d}), Value::Null))),
    );
    if cut {
        items.push(Err(std::io::Error::other("cut")));
    } else {
        items.push(Ok(chunk(json!({}), json!("stop"))));
        items.push(Ok("data: [DONE]\n\n".to_string()));
    }
    items
}

fn paced(
    items: Vec<Result<String, std::io::Error>>,
    delay: Duration,
) -> impl futures_util::Stream<Item = Result<String, std::io::Error>> {
    futures_util::stream::unfold(items.into_iter(), move |mut it| async move {
        let next = it.next()?;
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        Some((next, it))
    })
}

fn fake_vector(text: &str) -> Vec<f32> {
    let mut v = vec![0f32; 8];
    for (i, b) in text.bytes().enumerate() {
        v[i % 8] += f32::from(b) / 255.0;
    }
    v
}

async fn embeddings(State(app): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
    app.touch_model(body["model"].as_str().unwrap_or(""));
    let inputs: Vec<String> = match &body["input"] {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect(),
        _ => vec![],
    };
    let data: Vec<Value> = inputs
        .iter()
        .enumerate()
        .map(|(i, t)| json!({"object": "embedding", "index": i, "embedding": fake_vector(t)}))
        .collect();
    Json(json!({"object": "list", "data": data, "model": body["model"]}))
}

async fn transcriptions(State(app): State<Shared>, mut multipart: Multipart) -> Response {
    let mut fields = Map::new();
    let mut file_name = Value::Null;
    let mut file_len = 0usize;
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            file_name = json!(field.file_name());
            file_len = field.bytes().await.map(|b| b.len()).unwrap_or(0);
        } else {
            fields.insert(name, json!(field.text().await.unwrap_or_default()));
        }
    }
    app.touch_model(fields.get("model").and_then(|m| m.as_str()).unwrap_or(""));
    app.inner().log.push(json!({
        "multipart": true, "fields": fields, "file_name": file_name, "file_len": file_len,
    }));
    if fields.get("response_format").and_then(|v| v.as_str()) == Some("verbose_json") {
        return Json(json!({
            "text": " hello world",
            "language": "en",
            "duration": 3.2,
            "segments": [
                {"start": 0.0, "end": 1.5, "text": " hello"},
                {"start": 1.5, "end": 3.2, "text": " world"},
            ],
        }))
        .into_response();
    }
    Json(json!({"text": " hello world"})).into_response()
}

async fn speech(State(app): State<Shared>, Json(body): Json<Value>) -> Response {
    app.touch_model(body["model"].as_str().unwrap_or(""));
    let mut wav = b"RIFF\x24\x00\x00\x00WAVEfmt ".to_vec();
    wav.extend_from_slice(&[
        16, 0, 0, 0, 1, 0, 1, 0, 0x80, 0xbb, 0, 0, 0, 0x77, 1, 0, 2, 0, 16, 0,
    ]);
    wav.extend_from_slice(b"data\x00\x00\x00\x00");
    let mut resp = Response::new(Body::from(wav));
    let ct = if body["response_format"] == "wav" {
        "audio/wav"
    } else {
        "audio/mpeg"
    };
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(ct));
    resp
}

async fn shutdown(State(app): State<Shared>) -> Json<Value> {
    if app.flag("hang-shutdown") {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
    app.cleanup();
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        std::process::exit(0);
    });
    Json(json!({"status": "shutting down"}))
}

async fn set_config(State(app): State<Shared>, Json(body): Json<Value>) -> Response {
    let Some(map) = body.as_object().filter(|m| !m.is_empty()) else {
        return err(400, "invalid_argument", "empty body");
    };
    let mut inner = app.inner();
    for (k, v) in map {
        inner.config.insert(k.clone(), v.clone());
    }
    Json(json!({"status": "success", "updated": body})).into_response()
}

async fn get_config(State(app): State<Shared>) -> Json<Value> {
    Json(Value::Object(app.inner().config.clone()))
}

async fn fake_env() -> Json<Value> {
    Json(Value::Object(
        std::env::vars().map(|(k, v)| (k, json!(v))).collect(),
    ))
}

async fn fake_requests(State(app): State<Shared>) -> Json<Value> {
    Json(Value::Array(app.inner().log.clone()))
}

async fn fake_script(State(app): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
    let mut inner = app.inner();
    if let Some(items) = body["chat"].as_array() {
        inner.script = items.iter().cloned().collect();
    }
    Json(json!({"status": "ok"}))
}

/// Replaces the mode string while the fake runs, for tests that change conditions mid-flight.
async fn fake_mode(State(app): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
    *app.modes.write().unwrap_or_else(|e| e.into_inner()) =
        parse_modes(body["mode"].as_str().unwrap_or(""));
    Json(json!({"status": "ok"}))
}

/// Dies at once, as if it had crashed, for tests that choose the moment.
async fn fake_crash(State(app): State<Shared>) -> Json<Value> {
    app.persist();
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(20)).await;
        std::process::exit(1);
    });
    Json(json!({"status": "crashing"}))
}

async fn fake_stats(State(app): State<Shared>) -> Json<Value> {
    let inner = app.inner();
    Json(json!({
        "pulls": inner.pulls,
        "port": app.port,
        "downloaded": inner.downloaded,
        "backends": inner.backends,
        "fetched_on_use": inner.fetched_on_use,
        "conflicts": inner.conflicts,
        "jobs": inner.jobs.iter().map(|j| &j.id).collect::<Vec<_>>(),
    }))
}
