//! Shared harness: a mock-runtime app over a temp data dir, a fixed clock and an IPC helper.
#![allow(dead_code)]

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nocturne_app_lib::{AppState, configure};
use nocturne_contract::{AskDelta, AskPhase, MemoryView, PageView};
use nocturne_engine::{
    AnswerMeta, BoxFuture, Candidate, Engine, ExtractContext, FakeMind, ManualClock, Mind,
    MindError, Predicate, Transcript, Understanding,
};
use serde_json::Value;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{App, Listener, Manager, WebviewWindow, WebviewWindowBuilder};
use tokio::sync::mpsc;

pub struct Harness {
    pub app: App<MockRuntime>,
    pub webview: WebviewWindow<MockRuntime>,
    pub dir: tempfile::TempDir,
    pub clock: Arc<ManualClock>,
}

pub fn harness() -> Harness {
    build(None, |b| b)
}

pub fn harness_with_mind(mind: Arc<dyn Mind>) -> Harness {
    build(Some(mind), |b| b)
}

/// Build with a hook on the builder (to wrap the invoke handler, for example).
pub fn build(
    mind: Option<Arc<dyn Mind>>,
    hook: impl FnOnce(tauri::Builder<MockRuntime>) -> tauri::Builder<MockRuntime>,
) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::at("2026-11-13T07:00:00Z", "Europe/London"));
    let state = AppState::open(dir.path(), clock.clone());
    if let Some(m) = mind {
        state.mind.set(m);
    }
    finish(dir, clock, state, hook)
}

/// An app whose data folder cannot be opened.
pub fn broken_harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("a-file");
    std::fs::write(&blocked, b"x").unwrap();
    let clock = Arc::new(ManualClock::at("2026-11-13T07:00:00Z", "Europe/London"));
    let state = AppState::open(&blocked.join("inside"), clock.clone());
    assert!(!state.has_engine());
    finish(dir, clock, state, |b| b)
}

fn finish(
    dir: tempfile::TempDir,
    clock: Arc<ManualClock>,
    state: AppState,
    hook: impl FnOnce(tauri::Builder<MockRuntime>) -> tauri::Builder<MockRuntime>,
) -> Harness {
    let builder = hook(configure(mock_builder().manage(state)));
    let app = builder.build(mock_context(noop_assets())).unwrap();
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    Harness {
        app,
        webview,
        dir,
        clock,
    }
}

fn request(cmd: &str, body: InvokeBody, headers: tauri::http::HeaderMap) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().unwrap(),
        body,
        headers,
        invoke_key: INVOKE_KEY.to_string(),
    }
}

impl Harness {
    pub fn state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }

    pub fn engine(&self) -> Engine {
        self.state().engine().expect("an engine")
    }

    /// Call a command with JSON arguments; `Err` carries what the interface would see rejected.
    pub fn call(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        let req = request(cmd, InvokeBody::Json(args), Default::default());
        tauri::test::get_ipc_response(&self.webview, req).map(|b| b.deserialize::<Value>().unwrap())
    }

    pub fn ok(&self, cmd: &str, args: Value) -> Value {
        self.call(cmd, args)
            .unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
    }

    /// The error code of a command that must fail.
    pub fn code(&self, cmd: &str, args: Value) -> String {
        match self.call(cmd, args) {
            Ok(v) => panic!("{cmd} should have failed, got {v}"),
            Err(e) => e["code"]
                .as_str()
                .unwrap_or_else(|| panic!("{e}"))
                .to_string(),
        }
    }

    /// Call a command with a raw body and headers (a recording).
    pub fn call_raw(
        &self,
        cmd: &str,
        bytes: Vec<u8>,
        headers: &[(&str, &str)],
    ) -> Result<Value, Value> {
        let mut map = tauri::http::HeaderMap::new();
        for (k, v) in headers {
            map.insert(
                tauri::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                v.parse().unwrap(),
            );
        }
        let req = request(cmd, InvokeBody::Raw(bytes), map);
        tauri::test::get_ipc_response(&self.webview, req).map(|b| b.deserialize::<Value>().unwrap())
    }

    /// Collect the payloads of an event.
    pub fn events(&self, name: &str) -> Arc<Mutex<Vec<Value>>> {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        self.app.listen(name, move |ev| {
            sink.lock()
                .unwrap()
                .push(serde_json::from_str(ev.payload()).unwrap());
        });
        seen
    }

    pub fn ask_deltas(&self) -> Arc<Mutex<Vec<AskDelta>>> {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        self.app.listen(nocturne_contract::EVENT_ASK, move |ev| {
            sink.lock()
                .unwrap()
                .push(serde_json::from_str(ev.payload()).unwrap());
        });
        seen
    }

    /// Learn one memory from a new note; returns (item id, memory id).
    pub fn learn(
        &self,
        note: &str,
        sentence: &str,
        subject: &str,
        object: &str,
    ) -> (String, String) {
        let c = Candidate::new(subject, Predicate::Prefers, object, sentence, 0.9);
        let (item, mut created) = self.learn_candidate(note, c);
        (item, created.remove(0))
    }

    /// Keep a note and apply a candidate from it; returns the item id and the new memory ids.
    pub fn learn_candidate(&self, note: &str, c: Candidate) -> (String, Vec<String>) {
        let e = self.engine();
        let kept = e
            .keep_text(note, nocturne_contract::Source::QuickCapture)
            .unwrap();
        let item = match kept {
            nocturne_contract::Kept::New { item } | nocturne_contract::Kept::Duplicate { item } => {
                item.id
            }
        };
        let p = e.lease_item(&item, Duration::from_secs(60)).unwrap();
        let report = e.apply_candidates(&item, &p.lease, vec![c]).unwrap();
        (item, report.created)
    }

    pub fn memory(&self, id: &str) -> MemoryView {
        self.engine().get_memory(id).unwrap()
    }

    /// Compose and save today's page, as the scheduler would.
    pub fn save_todays_page(&self) -> PageView {
        let e = self.engine();
        let day = e.next_page_date().unwrap();
        let page = e
            .compose_page(day, nocturne_contract::Edition::Screen)
            .unwrap();
        e.save_page(&page).unwrap();
        e.page(Some(day)).unwrap().unwrap()
    }
}

/// Wait until `done` holds, or fail after a few seconds.
pub fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "timed out: {what}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub fn is_terminal(p: AskPhase) -> bool {
    matches!(
        p,
        AskPhase::Done | AskPhase::NotSure | AskPhase::Nothing | AskPhase::Unavailable
    )
}

pub fn deltas_for(all: &Mutex<Vec<AskDelta>>, id: &str) -> Vec<AskDelta> {
    all.lock()
        .unwrap()
        .iter()
        .filter(|d| d.ask_id == id)
        .cloned()
        .collect()
}

pub fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

pub fn id_of(kept: &Value) -> String {
    kept["item"]["id"].as_str().unwrap().to_string()
}

/// How a test mind answers.
#[derive(Clone, Copy)]
pub enum AnswerMode {
    /// Streams each fact's sentence and cites them all.
    Normal,
    /// Says it is not sure.
    NotSure,
    /// Waits before streaming, so a test can cancel in the middle.
    Slow(Duration),
    /// Never finishes.
    Never,
    /// Streams, then cites nothing.
    CitesNothing,
}

/// A scripted mind with knobs: how it answers and how long it takes to wake.
pub struct TestMind {
    inner: FakeMind,
    pub mode: AnswerMode,
    pub busy_embeds: AtomicU32,
}

impl TestMind {
    pub fn new(mode: AnswerMode) -> Arc<Self> {
        Arc::new(TestMind {
            inner: FakeMind::new(),
            mode,
            busy_embeds: AtomicU32::new(0),
        })
    }

    pub fn waking(busy_embeds: u32) -> Arc<Self> {
        let m = TestMind::new(AnswerMode::Normal);
        m.busy_embeds.store(busy_embeds, Ordering::SeqCst);
        m
    }
}

impl Mind for TestMind {
    fn transcribe<'a>(&'a self, audio: &'a Path) -> BoxFuture<'a, Result<Transcript, MindError>> {
        self.inner.transcribe(audio)
    }

    fn read_image<'a>(
        &'a self,
        image: &'a Path,
    ) -> BoxFuture<'a, Result<Understanding, MindError>> {
        self.inner.read_image(image)
    }

    fn read_document<'a>(
        &'a self,
        doc: &'a Path,
    ) -> BoxFuture<'a, Result<Understanding, MindError>> {
        self.inner.read_document(doc)
    }

    fn extract<'a>(
        &'a self,
        text: &'a str,
        cx: &'a ExtractContext,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, MindError>> {
        self.inner.extract(text, cx)
    }

    fn embed<'a>(&'a self, texts: &'a [String]) -> BoxFuture<'a, Result<Vec<Vec<f32>>, MindError>> {
        Box::pin(async move {
            let left = self.busy_embeds.load(Ordering::SeqCst);
            if left > 0 {
                self.busy_embeds.store(left - 1, Ordering::SeqCst);
                return Err(MindError::Busy);
            }
            self.inner.embed(texts).await
        })
    }

    fn answer<'a>(
        &'a self,
        question: &'a str,
        facts: &'a [MemoryView],
        out: mpsc::Sender<String>,
    ) -> BoxFuture<'a, Result<AnswerMeta, MindError>> {
        Box::pin(async move {
            match self.mode {
                AnswerMode::Normal => self.inner.answer(question, facts, out).await,
                AnswerMode::NotSure => Ok(AnswerMeta {
                    cited: Vec::new(),
                    not_sure: true,
                }),
                AnswerMode::Slow(d) => {
                    tokio::time::sleep(d).await;
                    self.inner.answer(question, facts, out).await
                }
                AnswerMode::Never => std::future::pending().await,
                AnswerMode::CitesNothing => {
                    let _ = out.send("Something.".to_string()).await;
                    Ok(AnswerMeta::default())
                }
            }
        })
    }

    fn write_headline<'a>(
        &'a self,
        page: &'a PageView,
    ) -> BoxFuture<'a, Result<(String, String), MindError>> {
        self.inner.write_headline(page)
    }

    fn speak<'a>(
        &'a self,
        text: &'a str,
        voice: &'a str,
    ) -> BoxFuture<'a, Result<Vec<u8>, MindError>> {
        self.inner.speak(text, voice)
    }
}
