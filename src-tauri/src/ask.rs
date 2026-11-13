//! Streamed answers. `ask` hands back an id at once; a task per question searches (engine),
//! wakes the mind if it is asleep, reads, and streams `ask-delta` events that all carry the
//! question's `askId`, ending in exactly one terminal phase. Cancelling stops that one task and
//! guarantees no delta follows.
//!
//! Phases: `searching`, then `waking` when the mind is starting, then `reading`, then
//! `streaming` deltas, then a terminal one: `done`, `notSure` (with the two nearest memories),
//! `nothing` (retrieval found nothing) or `unavailable` (no mind, or it failed).

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
/// Asks in flight at once; starting another cancels the oldest.
const MAX_ASKS: usize = 2;
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

/// The asks in flight, oldest first.
#[derive(Default)]
pub struct AskRegistry {
    live: Mutex<Vec<(String, Live)>>,
}

impl AskRegistry {
    fn live(&self) -> std::sync::MutexGuard<'_, Vec<(String, Live)>> {
        self.live.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Track a new ask, cancelling the oldest if there are already `MAX_ASKS`.
    fn register(&self, id: &str) -> Arc<Gate> {
        let gate = Arc::new(Gate::default());
        let evicted = {
            let mut live = self.live();
            let mut out = Vec::new();
            while live.len() >= MAX_ASKS {
                out.push(live.remove(0).1);
            }
            live.push((
                id.to_string(),
                Live {
                    gate: gate.clone(),
                    task: None,
                },
            ));
            out
        };
        evicted.into_iter().for_each(Live::stop);
        gate
    }

    fn set_task(&self, id: &str, task: JoinHandle<()>) {
        match self.live().iter_mut().find(|(i, _)| i == id) {
            Some((_, l)) => l.task = Some(task),
            // Already finished: nothing to track.
            None => drop(task),
        }
    }

    fn finish(&self, id: &str) {
        self.live().retain(|(i, _)| i != id);
    }

    /// Stop one ask. No delta for it is emitted after this returns. Unknown or finished
    /// asks are fine: cancelling twice is not an error.
    pub fn cancel(&self, id: &str) {
        let found = {
            let mut live = self.live();
            live.iter()
                .position(|(i, _)| i == id)
                .map(|at| live.remove(at).1)
        };
        if let Some(l) = found {
            l.stop();
        }
    }

    /// Stop every ask (after a forget, an undo or deleting everything). No delta follows.
    pub fn cancel_all(&self) {
        let all: Vec<Live> = self.live().drain(..).map(|(_, l)| l).collect();
        all.into_iter().for_each(Live::stop);
    }

    pub fn in_flight(&self) -> usize {
        self.live().len()
    }
}

impl Live {
    fn stop(self) {
        *self
            .gate
            .cancelled
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = true;
        if let Some(task) = self.task {
            task.abort();
        }
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
    /// Set once a terminal phase has gone out (or the ask was cancelled).
    ended: std::sync::atomic::AtomicBool,
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
            plain: false,
        };
        edit(&mut delta);
        let cancelled = self
            .gate
            .cancelled
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if matches!(
            phase,
            AskPhase::Done | AskPhase::NotSure | AskPhase::Nothing | AskPhase::Unavailable
        ) {
            self.ended.store(true, std::sync::atomic::Ordering::SeqCst);
        }
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
    let sink = Arc::new(Sink {
        app: app.clone(),
        id: id.clone(),
        gate,
        ended: std::sync::atomic::AtomicBool::new(false),
    });
    let finished = app.clone();
    let finished_id = id.clone();
    let task = tokio::spawn(async move {
        tokio::time::sleep(START_DELAY).await;
        // The work runs on its own task so that a panic in it (a mind that misbehaves) still
        // ends the ask with a terminal phase instead of leaving the page waiting.
        let worker = sink.clone();
        let outcome = tokio::spawn(async move { answer(&worker, question).await }).await;
        if outcome.is_err() {
            tracing::error!("an answer failed unexpectedly");
        }
        if !sink.ended.load(std::sync::atomic::Ordering::SeqCst) {
            sink.phase(AskPhase::Unavailable);
        }
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
    // The reader reads nothing while a question is being answered: the model serves one
    // request at a time, and the person is waiting.
    let _reader_waits = state.services().map(|s| s.reader.ask_begin());
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
                tracing::error!("retrieval failed: {:?}", err.code);
                sink.phase(AskPhase::Unavailable);
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

    let mut said = String::new();
    let meta = loop {
        let (tx, mut rx) = mpsc::channel::<String>(32);
        let m = mind.clone();
        let q = question.clone();
        let f = facts.clone();
        let mut task = AbortOnDrop(tokio::spawn(async move { m.answer(&q, &f, tx).await }));
        let mut streamed = false;
        said.clear();
        while let Some(text) = rx.recv().await {
            streamed = true;
            said.push_str(&text);
            sink.send(AskPhase::Streaming, |d| d.text_delta = Some(text));
        }
        match (&mut task.0).await {
            Ok(Ok(meta)) => break meta,
            // Only a mind that has said nothing yet is waited for; half an answer is not
            // started over.
            Ok(Err(MindError::Busy)) if !streamed && tokio::time::Instant::now() < deadline => {
                if !woke {
                    woke = true;
                    sink.phase(AskPhase::Waking);
                }
                tokio::time::sleep(WAKE_POLL).await;
            }
            Ok(Err(err)) => {
                tracing::warn!("the mind could not answer");
                let _ = err;
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

    // In the order the answer cites them: the nth bold span stands for the nth memory.
    let cited: Vec<&Hit> = meta
        .cited
        .iter()
        .filter_map(|id| hits.iter().find(|h| &h.memory.id == id))
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
    let plain = !bold_matches(&said, &meta.spans, &ids);
    sink.send(AskPhase::Done, |d| {
        d.receipts = receipts;
        d.memory_ids = ids;
        d.plain = plain;
    });
}

/// Whether the bold spans in `text` can stand one for one, in order, for the `cited` memories
/// (the interface reads the nth span as the nth memory): there is no bold, or the mind paired
/// every span with the memory the next citation after it names, and that pairing is `cited`.
fn bold_matches(text: &str, spans: &[String], cited: &[String]) -> bool {
    let markers = text.matches("**").count();
    markers == 0 || (markers.is_multiple_of(2) && markers / 2 == spans.len() && spans == cited)
}

#[cfg(test)]
mod tests {
    use super::bold_matches;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn bold_spans_match_one_for_one_in_order_or_not_at_all() {
        let ab = ids(&["a", "b"]);
        assert!(bold_matches("On Thursday.", &[], &ab));
        assert!(bold_matches("On **Thursday** with **Marek**.", &ab, &ab));
        assert!(!bold_matches(
            "On **Thursday** with **Marek**.",
            &ids(&["b", "a"]),
            &ab
        ));
        assert!(!bold_matches(
            "On **Thursday** with **Marek**.",
            &ids(&["a", "a"]),
            &ab
        ));
        assert!(!bold_matches(
            "On **Thursday** with Marek.",
            &ids(&["a"]),
            &ab
        ));
        assert!(!bold_matches("On **Thursday** with **Marek**.", &[], &ab));
        assert!(!bold_matches(
            "On **Thursday with Marek.",
            &ids(&["a"]),
            &ids(&["a"])
        ));
    }
}
