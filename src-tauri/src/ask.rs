//! Streamed answers. `ask` hands back an id at once; a task per question searches (engine),
//! wakes the mind if it is asleep, reads, and streams `ask-delta` events that all carry the
//! question's `askId`, ending in exactly one terminal phase. Cancelling stops that one task and
//! guarantees no delta follows.
//!
//! Phases: `searching`, then `waking` when the mind is starting, then `reading`, then
//! `streaming` deltas, then a terminal one: `done`, `notSure` (with the two nearest memories),
//! `nothing` (retrieval found nothing) or `unavailable` (no mind, or it failed).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nocturne_contract::{AskDelta, AskPhase, EVENT_ASK, MemoryView, NearMatch};
use nocturne_engine::{Hit, MindError, UseKind};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::blocking;
use crate::state::AppState;

/// Memories handed to the mind.
const RETRIEVE_K: usize = 12;
/// How many near matches an "I'm not sure" shows.
const NEAR: usize = 2;
/// How long a waking mind is waited for, and how often it is asked.
const WAKE_LIMIT: Duration = Duration::from_secs(120);
const WAKE_POLL: Duration = Duration::from_millis(500);
/// Lets the invoke response carrying the id reach the page before the first delta does.
const START_DELAY: Duration = Duration::from_millis(25);

/// The cancel switch of one ask. Emitting and cancelling take the same lock, so once
/// `cancel` returns no delta can follow.
#[derive(Default)]
struct Gate {
    cancelled: Mutex<bool>,
}

struct Live {
    gate: Arc<Gate>,
    task: Option<JoinHandle<()>>,
}

/// The asks in flight.
#[derive(Default)]
pub struct AskRegistry {
    live: Mutex<HashMap<String, Live>>,
}

impl AskRegistry {
    fn live(&self) -> std::sync::MutexGuard<'_, HashMap<String, Live>> {
        self.live.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn register(&self, id: &str) -> Arc<Gate> {
        let gate = Arc::new(Gate::default());
        self.live().insert(
            id.to_string(),
            Live {
                gate: gate.clone(),
                task: None,
            },
        );
        gate
    }

    fn set_task(&self, id: &str, task: JoinHandle<()>) {
        match self.live().get_mut(id) {
            Some(l) => l.task = Some(task),
            // Already finished: nothing to track.
            None => drop(task),
        }
    }

    fn finish(&self, id: &str) {
        self.live().remove(id);
    }

    /// Stop one ask. No delta for it is emitted after this returns. Unknown or finished
    /// asks are fine: cancelling twice is not an error.
    pub fn cancel(&self, id: &str) {
        let Some(live) = self.live().remove(id) else {
            return;
        };
        *live
            .gate
            .cancelled
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = true;
        if let Some(task) = live.task {
            task.abort();
        }
    }

    pub fn in_flight(&self) -> usize {
        self.live().len()
    }
}

struct AbortOnDrop<T>(JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// One ask's way of talking to the interface.
struct Sink<R: Runtime> {
    app: AppHandle<R>,
    id: String,
    gate: Arc<Gate>,
}

impl<R: Runtime> Sink<R> {
    fn send(&self, phase: AskPhase, edit: impl FnOnce(&mut AskDelta)) {
        let mut delta = AskDelta {
            ask_id: self.id.clone(),
            phase,
            text_delta: None,
            receipts: Vec::new(),
            memory_ids: Vec::new(),
            near: Vec::new(),
            found: None,
            eta_seconds: None,
        };
        edit(&mut delta);
        let cancelled = self
            .gate
            .cancelled
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !*cancelled && let Err(e) = self.app.emit(EVENT_ASK, &delta) {
            tracing::warn!("could not emit an answer step: {e}");
        }
    }

    fn phase(&self, phase: AskPhase) {
        self.send(phase, |_| {});
    }
}

/// Start answering `question`; returns the ask id at once.
pub fn start<R: Runtime>(app: &AppHandle<R>, question: String) -> String {
    let state = app.state::<AppState>();
    let id = format!("ask_{}", random_hex(8));
    let gate = state.asks.register(&id);
    let sink = Sink {
        app: app.clone(),
        id: id.clone(),
        gate,
    };
    let finished = app.clone();
    let finished_id = id.clone();
    let task = tokio::spawn(async move {
        tokio::time::sleep(START_DELAY).await;
        answer(&sink, question).await;
        finished.state::<AppState>().asks.finish(&finished_id);
    });
    state.asks.set_task(&id, task);
    id
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("the operating system provides random bytes");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

async fn answer<R: Runtime>(sink: &Sink<R>, question: String) {
    let state = sink.app.state::<AppState>();
    let (Some(mind), Ok(engine)) = (state.mind.get(), state.engine()) else {
        sink.phase(AskPhase::Unavailable);
        return;
    };
    sink.phase(AskPhase::Searching);

    // Meaning-based search needs the mind to embed the question. A sleeping mind is woken
    // (and the person told); one that is not there ends the ask.
    let deadline = tokio::time::Instant::now() + WAKE_LIMIT;
    let mut woke = state.mind.is_loading();
    if woke {
        sink.phase(AskPhase::Waking);
    }
    let query_vec = loop {
        match mind.embed(std::slice::from_ref(&question)).await {
            Ok(v) => break v.into_iter().next(),
            Err(MindError::Busy) if tokio::time::Instant::now() < deadline => {
                if !woke {
                    woke = true;
                    sink.phase(AskPhase::Waking);
                }
                tokio::time::sleep(WAKE_POLL).await;
            }
            Err(MindError::Unavailable | MindError::Busy) => {
                sink.phase(AskPhase::Unavailable);
                return;
            }
            // A mind that cannot embed still lets words find memories.
            Err(_) => break None,
        }
    };

    let q = question.clone();
    let e = engine.clone();
    let hits: Vec<Hit> =
        match blocking(move || e.retrieve(&q, query_vec.as_deref(), RETRIEVE_K)).await {
            Ok(h) => h,
            Err(err) => {
                tracing::error!("retrieval failed: {err}");
                sink.phase(AskPhase::NotSure);
                return;
            }
        };
    // A let-go memory is one the person no longer sees; it does not answer questions.
    let hits: Vec<Hit> = hits.into_iter().filter(|h| !h.let_go).collect();
    if hits.is_empty() {
        sink.phase(AskPhase::Nothing);
        return;
    }
    let facts: Vec<MemoryView> = hits.iter().map(|h| h.memory.clone()).collect();
    sink.send(AskPhase::Reading, |d| d.found = Some(facts.len() as u32));

    let meta = loop {
        let (tx, mut rx) = mpsc::channel::<String>(32);
        let m = mind.clone();
        let q = question.clone();
        let f = facts.clone();
        let mut task = AbortOnDrop(tokio::spawn(async move { m.answer(&q, &f, tx).await }));
        while let Some(text) = rx.recv().await {
            sink.send(AskPhase::Streaming, |d| d.text_delta = Some(text));
        }
        match (&mut task.0).await {
            Ok(Ok(meta)) => break meta,
            Ok(Err(MindError::Busy)) if tokio::time::Instant::now() < deadline => {
                if !woke {
                    woke = true;
                    sink.phase(AskPhase::Waking);
                }
                tokio::time::sleep(WAKE_POLL).await;
            }
            Ok(Err(err)) => {
                tracing::warn!("the mind could not answer: {err}");
                sink.phase(AskPhase::Unavailable);
                return;
            }
            Err(join) => {
                tracing::error!("the answer task failed: {join}");
                sink.phase(AskPhase::Unavailable);
                return;
            }
        }
    };

    let cited: Vec<&Hit> = hits
        .iter()
        .filter(|h| meta.cited.contains(&h.memory.id))
        .collect();
    // An answer with nothing to cite is not an answer: every claim carries a receipt.
    if meta.not_sure || cited.is_empty() {
        let near = hits
            .iter()
            .filter_map(|h| {
                h.memory.receipts.first().map(|r| NearMatch {
                    text: h.memory.sentence.clone(),
                    receipt: r.clone(),
                })
            })
            .take(NEAR)
            .collect();
        sink.send(AskPhase::NotSure, |d| d.near = near);
        return;
    }
    let ids: Vec<String> = cited.iter().map(|h| h.memory.id.clone()).collect();
    let receipts = cited
        .iter()
        .flat_map(|h| h.memory.receipts.iter().cloned())
        .collect();
    // Being cited in an answer counts as use; done before the answer is final so the next
    // look at the memory already shows it.
    let used = ids.clone();
    if let Err(e) = blocking(move || engine.mark_used(&used, UseKind::Answer)).await {
        tracing::warn!("could not note the memories used: {e}");
    }
    sink.send(AskPhase::Done, |d| {
        d.receipts = receipts;
        d.memory_ids = ids;
    });
}
