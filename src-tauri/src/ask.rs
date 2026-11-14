//! Streamed answers. `ask` hands back an id at once; a task per question searches (engine),
//! wakes the mind if it is asleep, reads, and streams `ask-delta` events that all carry the
//! question's `askId`, ending in exactly one terminal phase. Cancelling stops that one task and
//! guarantees no delta follows.
//!
//! Phases: `searching`, then `waking` when the mind is starting, then `reading`, then
//! `streaming` deltas, then a terminal one: `done`, `notSure` (with the two nearest memories),
//! `nothing` (retrieval found nothing) or `unavailable` (no mind, or it failed).
//!
//! The windows ask through `start`. A paired phone asks through `start_with`: in its own lane,
//! about public memories only, with the deltas going to its session instead of an event.
//!
//! With Online answers on, the last step may be written by the person's provider: the hub
//! chooses per question, the memories that may not leave are filtered out here (and again just
//! before the body is built), and the phases are `askingOnline`, `waitingOnline` after six
//! seconds, and `switchingToComputer` when the provider fails, takes twenty seconds or Online
//! answers is turned off. Falling back to this computer is automatic and marked on the
//! terminal delta; nothing here ever goes online after something failed on this computer. A
//! provider that is not sure, when something private or sensitive or a line of an original was
//! kept back from it, also hands the question to this computer, which has what was kept back
//! (`notSureOnline`, still linked to what was sent).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nocturne_contract::{
    AnsweredBy, AskDelta, AskPhase, AskPlace, EVENT_ASK, LeftOut, LocalReason, NearMatch,
    SendResult,
};
use nocturne_engine::{AnswerMeta, Engine, Fact, Hit, Mind, MindError, UseKind, is_source_line};
use nocturne_lemonade::mind::online::OnlineMind;
use nocturne_lemonade::online::{MAX_FACTS, SWITCH_SECONDS, WAIT_SECONDS};
use nocturne_lemonade::rerank::Reranker;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::blocking;
use crate::online::controller::{AskOutcome, OnlineHub, reason_for};
use crate::online::route::RouteChoice;
use crate::state::AppState;

/// Memories handed to the mind.
const RETRIEVE_K: usize = 12;
/// What is kept of those once a reranking model has ordered them (Standard and Large).
const RERANKED_K: usize = 8;
/// A reranking model that does not answer in this time leaves the search's own order.
const RERANK_LIMIT: Duration = Duration::from_secs(4);
/// At most this many memories on the other side of an open question are brought in.
const MAX_SIDES: usize = 4;
/// How many near matches an "I'm not sure" shows.
const NEAR: usize = 2;
/// Asks in flight at once from the windows; starting another cancels the oldest of them.
const MAX_ASKS: usize = 2;
/// Asks in flight at once from one phone.
const MAX_PHONE_ASKS: usize = 1;
/// How long a waking mind is waited for, and how often it is asked.
const WAKE_LIMIT: Duration = Duration::from_secs(120);
const WAKE_POLL: Duration = Duration::from_millis(500);
/// Lets the invoke response carrying the id reach the page before the first delta does.
const START_DELAY: Duration = Duration::from_millis(25);

/// Who asked. Each lane has its own cap, and a new ask evicts only within its own lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskLane {
    Desktop,
    /// A paired phone, by device id.
    Phone(String),
}

impl AskLane {
    fn cap(&self) -> usize {
        match self {
            AskLane::Desktop => MAX_ASKS,
            AskLane::Phone(_) => MAX_PHONE_ASKS,
        }
    }
}

/// Where an ask's deltas go.
#[derive(Debug, Clone)]
pub enum AskSink {
    /// `ask-delta` events, for the windows.
    Emit,
    /// One receiver only, such as a phone's session; never an event.
    Channel(mpsc::UnboundedSender<AskDelta>),
}

#[derive(Debug, Clone)]
pub struct AskOptions {
    /// Private memories are dropped after retrieval, before the mind sees anything, and from
    /// near matches.
    pub public_only: bool,
    pub lane: AskLane,
    pub sink: AskSink,
    /// Where this one question may be answered.
    pub place: AskPlace,
}

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
    live: Mutex<Vec<(String, AskLane, Live)>>,
}

impl AskRegistry {
    fn live(&self) -> std::sync::MutexGuard<'_, Vec<(String, AskLane, Live)>> {
        self.live.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Track a new ask, cancelling the oldest in its lane if the lane is full.
    fn register(&self, id: &str, lane: AskLane) -> Arc<Gate> {
        let gate = Arc::new(Gate::default());
        let evicted = {
            let mut live = self.live();
            let mut out = Vec::new();
            while live.iter().filter(|(_, l, _)| *l == lane).count() >= lane.cap() {
                let oldest = live.iter().position(|(_, l, _)| *l == lane).unwrap_or(0);
                out.push(live.remove(oldest).2);
            }
            live.push((
                id.to_string(),
                lane,
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
        match self.live().iter_mut().find(|(i, _, _)| i == id) {
            Some((_, _, l)) => l.task = Some(task),
            // Already finished: nothing to track.
            None => drop(task),
        }
    }

    fn finish(&self, id: &str) {
        self.live().retain(|(i, _, _)| i != id);
    }

    /// Stop one ask. No delta for it is emitted after this returns. Unknown or finished
    /// asks are fine: cancelling twice is not an error.
    pub fn cancel(&self, id: &str) {
        let found = {
            let mut live = self.live();
            live.iter()
                .position(|(i, _, _)| i == id)
                .map(|at| live.remove(at).2)
        };
        if let Some(l) = found {
            l.stop();
        }
    }

    /// Stop one ask only if it belongs to `lane`: a phone stops its own asks and no other.
    /// Returns whether it was there.
    pub fn cancel_in(&self, lane: &AskLane, id: &str) -> bool {
        let found = {
            let mut live = self.live();
            live.iter()
                .position(|(i, l, _)| i == id && l == lane)
                .map(|at| live.remove(at).2)
        };
        let was = found.is_some();
        if let Some(l) = found {
            l.stop();
        }
        was
    }

    /// Stop every ask in one lane (a phone that was removed). No delta of them follows.
    pub fn cancel_lane(&self, lane: &AskLane) {
        let gone: Vec<Live> = {
            let mut live = self.live();
            let (out, keep): (Vec<_>, Vec<_>) = live.drain(..).partition(|(_, l, _)| l == lane);
            *live = keep;
            out.into_iter().map(|(_, _, l)| l).collect()
        };
        gone.into_iter().for_each(Live::stop);
    }

    /// Stop every ask (after a forget, an undo or deleting everything). No delta follows.
    pub fn cancel_all(&self) {
        let all: Vec<Live> = self.live().drain(..).map(|(_, _, l)| l).collect();
        all.into_iter().for_each(Live::stop);
    }

    pub fn in_flight(&self) -> usize {
        self.live().len()
    }

    /// Asks in flight in one lane.
    pub fn in_flight_in(&self, lane: &AskLane) -> usize {
        self.live().iter().filter(|(_, l, _)| l == lane).count()
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

/// What every terminal delta of one ask says about where it was answered.
#[derive(Default, Clone)]
struct Mark {
    answered_by: Option<AnsweredBy>,
    left_out: Option<LeftOut>,
    send_id: Option<f64>,
}

/// One ask's way of talking to the interface.
struct Sink<R: Runtime> {
    app: AppHandle<R>,
    id: String,
    gate: Arc<Gate>,
    out: AskSink,
    /// Set once a terminal phase has gone out (or the ask was cancelled).
    ended: std::sync::atomic::AtomicBool,
    mark: Mutex<Mark>,
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
            answered_by: None,
            left_out: None,
            send_id: None,
        };
        edit(&mut delta);
        let terminal = matches!(
            phase,
            AskPhase::Done | AskPhase::NotSure | AskPhase::Nothing | AskPhase::Unavailable
        );
        if terminal {
            let mark = self.mark.lock().unwrap_or_else(|e| e.into_inner()).clone();
            delta.answered_by = delta.answered_by.take().or(mark.answered_by);
            delta.left_out = delta.left_out.or(mark.left_out);
            delta.send_id = delta.send_id.or(mark.send_id);
        }
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
        if *cancelled {
            return;
        }
        match &self.out {
            AskSink::Emit => {
                if let Err(e) = self.app.emit(EVENT_ASK, &delta) {
                    tracing::warn!("could not emit an answer step: {e}");
                }
            }
            // A receiver that has gone away (a closed session) just stops listening.
            AskSink::Channel(tx) => {
                let _ = tx.send(delta);
            }
        }
    }

    fn phase(&self, phase: AskPhase) {
        self.send(phase, |_| {});
    }

    fn mark(&self, edit: impl FnOnce(&mut Mark)) {
        edit(&mut self.mark.lock().unwrap_or_else(|e| e.into_inner()));
    }
}

/// Start answering `question` for the windows; returns the ask id at once.
pub fn start<R: Runtime>(app: &AppHandle<R>, question: String) -> String {
    start_placed(app, question, AskPlace::Auto)
}

/// Like [`start`], with the place the person chose for this question.
pub fn start_placed<R: Runtime>(app: &AppHandle<R>, question: String, place: AskPlace) -> String {
    start_with(
        app,
        question,
        AskOptions {
            public_only: false,
            lane: AskLane::Desktop,
            sink: AskSink::Emit,
            place,
        },
    )
}

/// Start answering `question` with the given options; returns the ask id at once.
pub fn start_with<R: Runtime>(app: &AppHandle<R>, question: String, opts: AskOptions) -> String {
    let state = app.state::<AppState>();
    let id = format!("ask_{}", random_hex(8));
    let gate = state.asks.register(&id, opts.lane.clone());
    let public_only = opts.public_only;
    let (lane, place) = (opts.lane.clone(), opts.place);
    let sink = Arc::new(Sink {
        app: app.clone(),
        id: id.clone(),
        gate,
        out: opts.sink,
        ended: std::sync::atomic::AtomicBool::new(false),
        mark: Mutex::new(Mark::default()),
    });
    let finished = app.clone();
    let finished_id = id.clone();
    let task = tokio::spawn(async move {
        tokio::time::sleep(START_DELAY).await;
        // The work runs on its own task so that a panic in it (a mind that misbehaves) still
        // ends the ask with a terminal phase instead of leaving the page waiting.
        let worker = sink.clone();
        let outcome =
            tokio::spawn(async move { answer(&worker, question, public_only, &lane, place).await })
                .await;
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

/// The question as the recall model reads it, for meaning-based search: through the mind's
/// query embedding, which gives a recall model trained on search instructions its own and
/// embeds the question as it is for any other.
pub async fn embed_question(mind: &dyn Mind, question: &str) -> Result<Vec<f32>, MindError> {
    mind.embed_query(question).await
}

/// On a tier with a reranking model that is on disk, the search's hits ordered by it and cut to
/// [`RERANKED_K`]. Anything that goes wrong leaves the search's own order.
async fn rerank(state: &AppState, question: &str, hits: Vec<Hit>) -> Vec<Hit> {
    let Some(services) = state.services() else {
        return hits;
    };
    let Some(model) = services.tier().and_then(nocturne_lemonade::tier::reranker) else {
        return hits;
    };
    let Some(client) = services.client().await else {
        return hits;
    };
    rerank_with(&Reranker::new(client, model), question, hits).await
}

/// [`rerank`] with a given reranker: the scores of the hits' sentences, if the model is there
/// and answers in time.
pub async fn rerank_with(reranker: &Reranker, question: &str, hits: Vec<Hit>) -> Vec<Hit> {
    if hits.len() < 2 {
        return hits;
    }
    let sentences: Vec<String> = hits.iter().map(|h| h.memory.sentence.clone()).collect();
    let scored = tokio::time::timeout(RERANK_LIMIT, async {
        if !reranker.present().await {
            return None;
        }
        reranker.scores(question, &sentences).await.ok()
    })
    .await;
    match scored {
        Ok(Some(scores)) => nocturne_engine::reranked(question, hits, &scores, RERANKED_K),
        Ok(None) => hits,
        Err(_) => {
            tracing::warn!("the reranking model took too long; keeping the search's order");
            hits
        }
    }
}

async fn answer<R: Runtime>(
    sink: &Sink<R>,
    question: String,
    public_only: bool,
    lane: &AskLane,
    place: AskPlace,
) {
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
        match embed_question(mind.as_ref(), &question).await {
            Ok(v) => break Some(v),
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
    let hits: Vec<Hit> = hits
        .into_iter()
        .filter(|h| !h.let_go && !(public_only && h.memory.private))
        .collect();
    let hits = rerank(&state, &question, hits).await;
    // Lines of the originals that reading missed; a phone never gets them (`phone_safe`).
    let (q, e) = (question.clone(), engine.clone());
    let hits = match blocking(move || {
        let fallback = hits.clone();
        Ok(e.with_source_lines(&q, hits, RETRIEVE_K)
            .unwrap_or_else(|_| {
                tracing::warn!("could not look at the originals for lines");
                fallback
            }))
    })
    .await
    {
        Ok(h) => h,
        Err(err) => {
            tracing::error!("retrieval failed: {:?}", err.code);
            sink.phase(AskPhase::Unavailable);
            return;
        }
    };
    let hits = if public_only {
        let e = engine.clone();
        let safe = blocking(move || {
            let mut out = Vec::with_capacity(hits.len());
            // A phone gets memories only: a line of an original has not been checked for what
            // may leave this computer.
            for h in hits.into_iter().filter(|h| !is_source_line(&h.memory.id)) {
                if phone_safe(&e, &h.memory)? {
                    out.push(h);
                }
            }
            Ok(out)
        })
        .await;
        match safe {
            Ok(h) => h,
            Err(err) => {
                tracing::error!("could not check what may go out: {:?}", err.code);
                sink.phase(AskPhase::Unavailable);
                return;
            }
        }
    } else {
        hits
    };
    if hits.is_empty() {
        sink.phase(AskPhase::Nothing);
        return;
    }
    let e = engine.clone();
    let (hits, found, facts, pairs) =
        match blocking(move || with_disputes(&e, hits, public_only)).await {
            Ok(v) => v,
            Err(err) => {
                tracing::error!("could not read the open questions: {:?}", err.code);
                sink.phase(AskPhase::Unavailable);
                return;
            }
        };
    sink.send(AskPhase::Reading, |d| d.found = Some(facts.len() as u32));

    // Where this question may be answered is decided once, before anything is asked: a
    // failure here never sends it online.
    let hub = crate::online::hub(&sink.app);
    // Watched from before the choice: a turn-off from here on stops the question going out.
    let turned_off = hub.as_ref().map(|h| h.gate());
    let choice = match &hub {
        Some(h) => h.route_for(lane, place).await,
        None => RouteChoice::Off,
    };
    let here_because = match (choice, &hub) {
        (RouteChoice::Off, _) => None,
        (RouteChoice::Computer(reason), _) => Some(reason),
        (
            RouteChoice::Online {
                mind: online,
                host,
                send_sensitive,
                reservation,
            },
            Some(hub),
        ) => {
            // Held until the question is logged or has fallen back.
            let _reservation = reservation;
            let Some(turned_off) = turned_off else {
                return;
            };
            let asked = Asked {
                engine: &engine,
                question: &question,
                hits: &hits,
                found,
                facts: &facts,
                pairs: &pairs,
            };
            match ask_online(
                sink,
                hub,
                &online,
                &host,
                send_sensitive,
                &asked,
                turned_off,
            )
            .await
            {
                Online::Answered => return,
                Online::Here(reason) => Some(reason),
            }
        }
        (RouteChoice::Online { .. }, None) => None,
    };
    if let Some(reason) = here_because {
        sink.mark(|m| {
            m.answered_by = Some(AnsweredBy::Computer {
                reason: Some(reason),
            })
        });
    }

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
    let mut meta = meta;
    let known: Vec<String> = hits.iter().map(|h| h.memory.id.clone()).collect();
    if let Some((extra, ids)) = settle(&pairs, &meta, &said, &known) {
        tracing::info!("an answer gave one side of two; the other is added");
        said.push_str(&extra);
        meta.cited.extend(ids);
        sink.send(AskPhase::Streaming, |d| d.text_delta = Some(extra));
    }
    conclude(sink, &engine, &hits, found, &meta, &said).await;
}

/// What one question found, for the online answer.
struct Asked<'a> {
    engine: &'a Engine,
    question: &'a str,
    hits: &'a [Hit],
    found: usize,
    facts: &'a [Fact],
    pairs: &'a [Sides],
}

/// How an online question ended.
enum Online {
    /// The terminal delta has gone out.
    Answered,
    /// This computer answers instead, for this reason.
    Here(LocalReason),
}

/// The online part of one question: filter, send once, wait (six seconds, then twenty), and
/// either finish with the provider's answer or hand back to this computer.
async fn ask_online<R: Runtime>(
    sink: &Sink<R>,
    hub: &OnlineHub<R>,
    mind: &OnlineMind,
    host: &str,
    send_sensitive: bool,
    asked: &Asked<'_>,
    mut turned_off: tokio::sync::watch::Receiver<u64>,
) -> Online {
    // Lines of originals stay here: they have not been through what decides what may leave.
    let lines_held = asked.facts.iter().any(|f| is_source_line(&f.memory.id));
    let facts: Vec<&Fact> = asked
        .facts
        .iter()
        .filter(|f| !is_source_line(&f.memory.id))
        .collect();
    let ids: Vec<String> = facts.iter().map(|f| f.memory.id.clone()).collect();
    let e = asked.engine.clone();
    let verdicts = match blocking(move || e.online_verdicts(&ids, send_sensitive)).await {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!("could not check what may go online: {:?}", err.code);
            hub.record_not_sent(asked.question, host, LocalReason::Problem)
                .await;
            return Online::Here(LocalReason::Problem);
        }
    };
    let (mut private, mut sensitive) = (false, false);
    let mut sendable: Vec<Fact> = Vec::new();
    for (fact, verdict) in facts.iter().zip(&verdicts) {
        match verdict {
            None => {
                // A move goes out as the memory is kept: the old value it carries here is
                // another memory's, judged for this computer only, and the Sent log shows
                // exactly the memory's own words.
                let mut fact = (*fact).clone();
                if let Some(p) = asked
                    .pairs
                    .iter()
                    .find(|p| p.moved && p.newer.id == fact.memory.id)
                {
                    fact.memory.was = p.newer_was.clone();
                }
                sendable.push(fact);
            }
            Some(LeftOut::Private) => private = true,
            Some(LeftOut::Sensitive) => sensitive = true,
            Some(LeftOut::Both) => (private, sensitive) = (true, true),
        }
    }
    sendable.truncate(MAX_FACTS);
    let prepared = mind.prepare(asked.question, &sendable);
    if prepared.as_ref().is_some_and(|p| p.dropped > 0) {
        private = true;
    }
    let left_out = match (private, sensitive) {
        (true, true) => Some(LeftOut::Both),
        (true, false) => Some(LeftOut::Private),
        (false, true) => Some(LeftOut::Sensitive),
        (false, false) => None,
    };
    let Some(prepared) = prepared else {
        hub.record_not_sent(asked.question, host, LocalReason::KeptHere)
            .await;
        return Online::Here(LocalReason::KeptHere);
    };

    hub.before_send().await;
    // Turned off since the route was chosen: nothing goes out.
    if turned_off.has_changed().unwrap_or(true) || !hub.still_on() {
        hub.record_not_sent(asked.question, host, LocalReason::TurnedOff)
            .await;
        return Online::Here(LocalReason::TurnedOff);
    }
    sink.phase(AskPhase::AskingOnline);
    let (tx, mut rx) = mpsc::channel::<String>(8);
    let send = mind.send(&prepared, tx);
    tokio::pin!(send);
    let wait = tokio::time::sleep(Duration::from_secs(WAIT_SECONDS));
    let switch = tokio::time::sleep(Duration::from_secs(SWITCH_SECONDS));
    tokio::pin!(wait, switch);
    let mut waited = false;
    let ended = loop {
        // A turn-off is looked at first, so a question it races never starts.
        tokio::select! {
            biased;
            _ = turned_off.changed() => break Err(LocalReason::TurnedOff),
            r = &mut send => break Ok(r),
            () = &mut wait, if !waited => {
                waited = true;
                sink.phase(AskPhase::WaitingOnline);
            }
            () = &mut switch => break Err(LocalReason::Slow),
        }
    };
    let outcome = |receipt, result, failure, counted| AskOutcome {
        question: asked.question.to_string(),
        host: host.to_string(),
        model: hub.model(),
        receipt,
        result,
        failure,
        counted,
    };
    let (result, receipt) = match ended {
        // The request was dropped: the engine drops its own, and nothing comes back.
        Err(reason) => {
            let failure =
                (reason == LocalReason::Slow).then_some(nocturne_engine::OnlineFailure::Slow);
            hub.record(outcome(
                prepared.receipt.clone(),
                SendResult::Failed { reason },
                failure,
                true,
            ))
            .await;
            sink.phase(AskPhase::SwitchingToComputer);
            return Online::Here(reason);
        }
        Ok(r) => r,
    };
    let meta = match result {
        Ok(meta) => meta,
        Err(err) => {
            let mut failure = match err {
                MindError::Online(f) => Some(f),
                _ => None,
            };
            let mut reason = failure.map(reason_for).unwrap_or(LocalReason::Problem);
            // A download window that opened after the route was chosen takes the route away
            // for a while: the 404 that follows is not a missing model.
            if failure == Some(nocturne_engine::OnlineFailure::ModelMissing)
                && hub.downloading().await
            {
                failure = None;
                reason = LocalReason::Downloading;
            }
            let counted = receipt.status.is_some();
            hub.record(outcome(
                receipt,
                SendResult::Failed { reason },
                failure,
                counted,
            ))
            .await;
            sink.phase(AskPhase::SwitchingToComputer);
            return Online::Here(reason);
        }
    };
    let mut said = String::new();
    while let Ok(t) = rx.try_recv() {
        said.push_str(&t);
    }
    let cited = meta
        .cited
        .iter()
        .filter(|id| asked.hits.iter().any(|h| &h.memory.id == *id))
        .count();
    let stands = !meta.not_sure && cited > 0;
    let result = if stands {
        SendResult::Answered {
            receipts: cited as u32,
        }
    } else {
        SendResult::NotSure
    };
    let send_id = hub.record(outcome(receipt, result, None, true)).await;
    // The provider was not sure without what was kept back (something private or sensitive, or
    // lines of originals): this computer, which has it, answers instead, and the mark still
    // leads to what was sent.
    if !stands && (left_out.is_some() || lines_held) {
        sink.mark(|m| m.send_id = send_id.map(|id| id as f64));
        sink.phase(AskPhase::SwitchingToComputer);
        return Online::Here(LocalReason::NotSureOnline);
    }
    // What was left out is said only of an answer written online.
    // A side kept back from the provider (a line of an original, something private) that the
    // answer needed is added here, from this computer; it never goes out, the Sent log keeps
    // only what came back, and the mark says something stayed here.
    let mut meta = meta;
    let mut left_out = left_out;
    let known: Vec<String> = asked.hits.iter().map(|h| h.memory.id.clone()).collect();
    if stands && let Some((extra, ids)) = settle(asked.pairs, &meta, &said, &known) {
        said.push_str(&extra);
        meta.cited.extend(ids);
        left_out = Some(match left_out {
            Some(LeftOut::Sensitive | LeftOut::Both) => LeftOut::Both,
            _ => LeftOut::Private,
        });
    }
    sink.mark(|m| {
        m.answered_by = Some(AnsweredBy::Online {
            host: host.to_string(),
        });
        m.send_id = send_id.map(|id| id as f64);
        m.left_out = left_out;
    });
    // Only an answer that stands is shown; a "not sure" offers this computer, never resends.
    if stands && !said.is_empty() {
        let text = said.clone();
        sink.send(AskPhase::Streaming, |d| d.text_delta = Some(text));
    }
    conclude(sink, asked.engine, asked.hits, asked.found, &meta, &said).await;
    Online::Answered
}

/// The terminal delta for an answer: `notSure` with the nearest matches, or `done` with the
/// receipts of the memories it cites.
async fn conclude<R: Runtime>(
    sink: &Sink<R>,
    engine: &Engine,
    hits: &[Hit],
    found: usize,
    meta: &AnswerMeta,
    said: &str,
) {
    // In the order the answer cites them: the nth bold span stands for the nth memory.
    let cited: Vec<&Hit> = meta
        .cited
        .iter()
        .filter_map(|id| hits.iter().find(|h| &h.memory.id == id))
        .collect();
    // An answer with nothing to cite is not an answer: every claim carries a receipt.
    if meta.not_sure || cited.is_empty() {
        // Near matches are what the search found, never a side brought in from a question.
        let near = hits[..found]
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
    let cited_ids: Vec<String> = cited.iter().map(|h| h.memory.id.clone()).collect();
    let receipts = cited
        .iter()
        .flat_map(|h| h.memory.receipts.iter().cloned())
        .collect();
    // A line of an original is no memory: its receipt is the original, and the answer is shown
    // without bold, which would stand for memories one for one.
    let ids: Vec<String> = cited_ids
        .iter()
        .filter(|id| !is_source_line(id))
        .cloned()
        .collect();
    // Being cited in an answer counts as use; done before the answer is final so the next
    // look at the memory already shows it.
    let used = ids.clone();
    let engine = engine.clone();
    if let Err(e) = blocking(move || engine.mark_used(&used, UseKind::Answer)).await {
        tracing::warn!("could not note the memories used: {e}");
    }
    let plain = ids.len() != cited_ids.len() || !bold_matches(said, &meta.spans, &ids);
    sink.send(AskPhase::Done, |d| {
        d.receipts = receipts;
        d.memory_ids = ids;
        d.plain = plain;
    });
}

/// One side of two, as the answer would give it.
#[derive(Debug, Clone, PartialEq)]
struct Side {
    id: String,
    sentence: String,
}

/// Two facts an answer must not give one of alone: the sides of an open which-is-right
/// question, or an appointment and where a later note says it moved (`moved`, with the memory
/// that says it moved as `notice`). A move reaches the mind as the newer side alone, carrying
/// the old value as `was`; `older` is then not among the facts.
#[derive(Debug, Clone, PartialEq)]
struct Sides {
    older: Side,
    newer: Side,
    /// Whether `newer` was clearly kept later.
    ordered: bool,
    moved: bool,
    notice: Option<String>,
    /// For a move, the newer side's own `was`, before the old value was put there.
    newer_was: Option<String>,
}

/// The facts' hits, how many of them the search found, the facts, and the pairs of sides.
type Gathered = (Vec<Hit>, usize, Vec<Fact>, Vec<Sides>);

/// The facts for the mind.
///
/// A dated appointment a later note says moved (`Engine::later_moves`) is given as a move: in
/// its place, what that note says it moved to, with the old value as `was`. That is the line of
/// the note, or on a phone, which never gets a line of an original, the memory that says it
/// moved, when it may go out; otherwise the appointment stays as it is. A line the search
/// already offered is used as it is, not offered twice.
///
/// Each hit then comes with the memories it disagrees with in an open which-is-right question.
/// A side the search did not find is brought in, so the answer can give both and cite both: at
/// most `MAX_SIDES` of them, with the facts kept to `RETRIEVE_K` by leaving out the lowest hits.
/// A side that can no longer be read (it is being forgotten), or a private one when
/// `public_only`, is left out. Returns the facts' hits, how many of them the search found (they
/// come first), the facts, and the pairs of sides.
fn with_disputes(
    engine: &Engine,
    mut hits: Vec<Hit>,
    public_only: bool,
) -> nocturne_engine::Result<Gathered> {
    let mut pairs: Vec<Sides> = Vec::new();
    for (old, notice, mut side) in engine.later_moves(&hits)? {
        let Some(at) = hits.iter().position(|h| h.memory.id == old) else {
            continue;
        };
        if public_only && is_source_line(&side.memory.id) {
            side = match engine.get_memory(&notice) {
                Ok(memory) => Hit {
                    memory,
                    score: 0.0,
                    let_go: false,
                },
                Err(nocturne_engine::Error::NotFound(_)) => continue,
                Err(e) => return Err(e),
            };
        }
        if public_only && !phone_safe(engine, &side.memory)? {
            continue;
        }
        let item = |h: &Hit| h.memory.receipts.first().map(|r| r.item_id.clone());
        let offered = hits.iter().position(|h| {
            h.memory.id == side.memory.id
                || (is_source_line(&h.memory.id)
                    && is_source_line(&side.memory.id)
                    && h.memory.sentence == side.memory.sentence
                    && item(h) == item(&side))
        });
        let older = Side {
            id: old,
            sentence: hits[at].memory.sentence.clone(),
        };
        // The old value travels with the move as `was`: on a phone only when it may go there.
        let was = (!public_only || phone_safe(engine, &hits[at].memory)?)
            .then(|| hits[at].memory.object.clone());
        let newer_was = match offered {
            Some(i) => hits[i].memory.was.clone(),
            None => side.memory.was.clone(),
        };
        let newer = match offered {
            Some(i) => {
                hits[i].memory.was = was;
                let id = hits[i].memory.id.clone();
                hits.remove(at);
                id
            }
            None => {
                side.memory.was = was;
                let id = side.memory.id.clone();
                hits[at] = side;
                id
            }
        };
        let sentence = hits
            .iter()
            .find(|h| h.memory.id == newer)
            .map(|h| h.memory.sentence.clone())
            .unwrap_or_default();
        pairs.push(Sides {
            older,
            newer: Side {
                id: newer,
                sentence,
            },
            ordered: true,
            moved: true,
            notice: Some(notice),
            newer_was,
        });
    }

    let ids: Vec<String> = hits.iter().map(|h| h.memory.id.clone()).collect();
    let disputes = engine.open_disputes(&ids)?;
    let mut sides: Vec<Hit> = Vec::new();
    'hits: for (at, hit) in hits.iter().enumerate() {
        let id = &hit.memory.id;
        for (a, b) in &disputes {
            let other = match (a == id, b == id) {
                (true, _) => b,
                (_, true) => a,
                _ => continue,
            };
            if hits.iter().chain(&sides).any(|h| &h.memory.id == other) {
                continue;
            }
            // The hit that asks for this side must stay among the facts once it is added.
            let drops_hits = hits.len() + sides.len() + 1 > RETRIEVE_K;
            if sides.len() == MAX_SIDES || (drops_hits && at + sides.len() + 1 >= RETRIEVE_K) {
                break 'hits;
            }
            match engine.get_memory(other) {
                Ok(memory) if public_only && !phone_safe(engine, &memory)? => {}
                Ok(memory) => sides.push(Hit {
                    memory,
                    score: 0.0,
                    let_go: false,
                }),
                Err(nocturne_engine::Error::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
    }
    hits.truncate(RETRIEVE_K - sides.len());
    let found = hits.len();
    hits.extend(sides);
    let facts = hits
        .iter()
        .map(|h| {
            let id = &h.memory.id;
            Fact {
                memory: h.memory.clone(),
                disputed_by: disputes
                    .iter()
                    .filter_map(|(a, b)| match (a == id, b == id) {
                        (true, _) => Some(b.clone()),
                        (_, true) => Some(a.clone()),
                        _ => None,
                    })
                    .filter(|other| hits.iter().any(|h| &h.memory.id == other))
                    .collect(),
            }
        })
        .collect();
    let side = |id: &str| {
        hits.iter().find(|h| h.memory.id == id).map(|h| {
            let kept = h.memory.receipts.iter().map(|r| r.kept_at.clone()).max();
            (
                Side {
                    id: id.to_string(),
                    sentence: h.memory.sentence.clone(),
                },
                kept,
            )
        })
    };
    for (a, b) in &disputes {
        let (Some((a, ka)), Some((b, kb))) = (side(a), side(b)) else {
            continue;
        };
        let ordered = ka.is_some() && kb.is_some() && ka != kb;
        let (older, newer) = if ka > kb { (b, a) } else { (a, b) };
        pairs.push(Sides {
            older,
            newer,
            ordered,
            moved: false,
            notice: None,
            newer_was: None,
        });
    }
    Ok((hits, found, facts, pairs))
}

const WEEKDAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

const ORDINALS: [(&str, &str); 5] = [
    ("first", "1st"),
    ("second", "2nd"),
    ("third", "3rd"),
    ("fourth", "4th"),
    ("fifth", "5th"),
];

/// Words that tell nothing apart.
const PLAIN: [&str; 16] = [
    "the", "a", "an", "is", "are", "was", "on", "at", "of", "in", "to", "and", "it", "my", "with",
    "for",
];

/// The words of `text`, lower case, with a time kept whole ("14:30", "2.30" as "2:30").
fn tokens(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.to_lowercase().chars().collect();
    let mut out = Vec::new();
    let mut word = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let joins = matches!(c, ':' | '.')
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && chars.get(i + 1).is_some_and(char::is_ascii_digit);
        if c.is_alphanumeric() || joins {
            word.push(if c == '.' { ':' } else { c });
        } else if !word.is_empty() {
            out.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// A time of day in its 24-hour forms: "14:30", "2:30 pm" and "2:30pm" are 14:30; "2:30" with no
/// am or pm may be 02:30 or 14:30; "2pm" is 14:00. A bare number is no time.
fn clock(word: &str, next: Option<&String>) -> Option<Vec<String>> {
    let (body, half) = match (word.strip_suffix("am"), word.strip_suffix("pm")) {
        (Some(b), _) => (b, Some(false)),
        (_, Some(b)) => (b, Some(true)),
        _ => (
            word,
            match next.map(String::as_str) {
                Some("am") => Some(false),
                Some("pm") => Some(true),
                _ => None,
            },
        ),
    };
    let (h, m) = match body.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None if half.is_some() => (body.parse::<u32>().ok()?, 0),
        None => return None,
    };
    if h > 23 || m > 59 {
        return None;
    }
    let hours = match half {
        Some(pm) if (1..=12).contains(&h) => vec![h % 12 + if pm { 12 } else { 0 }],
        Some(_) => return None,
        None if (1..=12).contains(&h) => vec![h, (h + 12) % 24],
        None => vec![h],
    };
    Some(
        hours
            .into_iter()
            .map(|h| format!("t:{h:02}:{m:02}"))
            .collect(),
    )
}

/// The days and the times of day `text` names, each in the forms it may take: a weekday, a day
/// of a month ("19 November", "November 19th"), an ordinal of a weekday ("first Tuesday"), a
/// month on its own. Other numbers (a phone number, a room) are none of these.
fn days_and_times(text: &str) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
    // "Tuesdays" is Tuesday.
    let t: Vec<String> = tokens(text)
        .into_iter()
        .map(|w| match w.strip_suffix('s') {
            Some(day) if WEEKDAYS.contains(&day) => day.to_string(),
            _ => w,
        })
        .collect();
    let month = |i: usize| t.get(i).filter(|w| MONTHS.contains(&w.as_str()));
    let (mut days, mut times) = (Vec::new(), Vec::new());
    let mut dated_months = Vec::new();
    for (i, w) in t.iter().enumerate() {
        if WEEKDAYS.contains(&w.as_str()) {
            days.push(vec![format!("wd:{w}")]);
        } else if let Some((word, _)) = ORDINALS.iter().find(|(a, b)| w == a || w == b)
            && t.get(i + 1).is_some_and(|n| WEEKDAYS.contains(&n.as_str()))
        {
            days.push(vec![format!("ord:{word}")]);
        } else if let Some(forms) = clock(w, t.get(i + 1)) {
            times.push(forms);
        } else {
            let n = w.trim_end_matches(|c: char| c.is_ascii_alphabetic());
            let day = (w.len() - n.len() <= 2 && !n.is_empty())
                .then(|| n.parse::<u32>().ok())
                .flatten()
                .filter(|d| (1..=31).contains(d));
            let near = if t.get(i + 1).is_some_and(|x| x == "of") {
                month(i + 2).map(|m| (i + 2, m))
            } else {
                month(i + 1)
                    .map(|m| (i + 1, m))
                    .or_else(|| i.checked_sub(1).and_then(|j| month(j).map(|m| (j, m))))
            };
            if let (Some(d), Some((at, m))) = (day, near) {
                days.push(vec![format!("dm:{d}:{m}")]);
                dated_months.push(at);
            }
        }
    }
    for (i, w) in t.iter().enumerate() {
        if MONTHS.contains(&w.as_str()) && !dated_months.contains(&i) {
            days.push(vec![format!("m:{w}")]);
        }
    }
    (days, times)
}

/// The sentences of an answer. A full stop between digits ("14.30") ends none.
fn sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut now = String::new();
    for (i, &c) in chars.iter().enumerate() {
        now.push(c);
        let ends = matches!(c, '!' | '?' | ';' | '\n')
            || (c == '.' && chars.get(i + 1).is_none_or(|n| n.is_whitespace()));
        if ends {
            out.push(std::mem::take(&mut now));
        }
    }
    out.push(now);
    out.retain(|s| s.chars().any(char::is_alphanumeric));
    out
}

/// Words that point back at what was just named.
const POINTERS: [&str; 4] = ["it", "they", "that", "this"];

/// Whether an answer sentence speaks of what two sides are about: it shares a word with either
/// (not a day, a number or a small word), or points back at it ("It is now ...").
fn about(sentence: &str, side: &str, other: &str) -> bool {
    let side = format!("{side} {other}");
    let side = side.as_str();
    let (days, times) = days_and_times(side);
    let values: std::collections::HashSet<String> =
        days.into_iter().chain(times).flatten().collect();
    let theirs: Vec<String> = tokens(side)
        .into_iter()
        .filter(|w| {
            w.chars().count() > 2
                && !PLAIN.contains(&w.as_str())
                && !w.chars().any(|c| c.is_ascii_digit())
                && !WEEKDAYS.contains(&w.as_str())
                && !MONTHS.contains(&w.as_str())
                && !values.iter().any(|v| v.ends_with(w.as_str()))
        })
        .collect();
    tokens(sentence)
        .iter()
        .any(|w| theirs.contains(w) || POINTERS.contains(&w.as_str()))
}

/// Whether `answer` says what `side` says and `other` does not: in one sentence that speaks of
/// it, its day or date (any one of them) and every time of day of its own, in any usual form.
/// A side with no day or time of its own is said when any word of its own is.
fn carries(answer: &str, side: &str, other: &str) -> bool {
    let all = |text: &str| {
        let (d, t) = days_and_times(text);
        d.into_iter()
            .chain(t)
            .flatten()
            .collect::<std::collections::HashSet<String>>()
    };
    let theirs = all(other);
    let (days, times) = days_and_times(side);
    let own = |v: Vec<Vec<String>>| -> Vec<Vec<String>> {
        v.into_iter()
            .filter(|forms| !forms.iter().any(|f| theirs.contains(f)))
            .collect()
    };
    let (days, times) = (own(days), own(times));
    if days.is_empty() && times.is_empty() {
        let (words, theirs) = (tokens(side), tokens(other));
        let said = tokens(answer);
        let named: Vec<&String> = words
            .iter()
            .filter(|w| !theirs.contains(w) && !PLAIN.contains(&w.as_str()))
            .collect();
        return named.is_empty() || named.iter().any(|w| said.contains(w));
    }
    sentences(answer).iter().any(|s| {
        let said = all(s);
        let has = |forms: &Vec<String>| forms.iter().any(|f| said.contains(f));
        about(s, side, other) && (days.is_empty() || days.iter().any(has)) && times.iter().all(has)
    })
}

fn full_stop(sentence: &str) -> String {
    let s = sentence.trim();
    if s.ends_with(['.', '!', '?']) {
        s.to_string()
    } else {
        format!("{s}.")
    }
}

/// Whether an answer already says two notes disagree.
fn says_unsettled(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("not settled") || lower.contains("disagree")
}

/// What to add to an answer that cites one of two sides without saying what the other says: an
/// open question answered as settled, or a move answered without where it moved to.
///
/// The missing side follows in its own words and is cited: for an open question marked as the
/// earlier or the later note, then that it is not settled (also when the answer gives both but
/// picks one); for a move as the later note, said to be where it moved when the answer gave the
/// old day. A move needs only its newer side said. A side with no receipt among `known` is never
/// added. The answer's own words stay as they are. Returns the words to add and the memories
/// they cite, or `None`.
fn settle(
    pairs: &[Sides],
    meta: &AnswerMeta,
    said: &str,
    known: &[String],
) -> Option<(String, Vec<String>)> {
    if meta.not_sure {
        return None;
    }
    let mut extra = String::new();
    let mut ids: Vec<String> = Vec::new();
    for p in pairs {
        let touched = meta
            .cited
            .iter()
            .any(|id| id == &p.older.id || id == &p.newer.id || p.notice.as_ref() == Some(id));
        if !touched {
            continue;
        }
        let so_far = format!("{said}{extra}");
        let mut add: Vec<(&Side, &str)> = Vec::new();
        if !carries(&so_far, &p.newer.sentence, &p.older.sentence) {
            let label = match (p.moved, p.ordered) {
                (true, _) if carries(&so_far, &p.older.sentence, &p.newer.sentence) => {
                    "It has since moved. The later note says:"
                }
                (true, _) => "The later note says:",
                (false, true) => "A later note says:",
                (false, false) => "Another note says:",
            };
            add.push((&p.newer, label));
        }
        if !p.moved && !carries(&so_far, &p.older.sentence, &p.newer.sentence) {
            let label = if p.ordered {
                "An earlier note says:"
            } else {
                "Another note says:"
            };
            add.push((&p.older, label));
        }
        if add.iter().any(|(side, _)| !known.contains(&side.id)) {
            continue;
        }
        for (side, label) in &add {
            extra.push_str(&format!(" {label} {}", full_stop(&side.sentence)));
            if !meta.cited.contains(&side.id) && !ids.contains(&side.id) {
                ids.push(side.id.clone());
            }
        }
        if !p.moved && !says_unsettled(&format!("{said}{extra}")) {
            extra.push_str(" It is not settled which is right.");
        }
    }
    (!extra.is_empty()).then_some((extra, ids))
}

/// For a phone: the memory is public in every version and every item it cites may go out, so
/// neither its receipts nor near matches open an item that also holds something private.
fn phone_safe(
    engine: &Engine,
    memory: &nocturne_contract::MemoryView,
) -> nocturne_engine::Result<bool> {
    let mut ids = vec![memory.id.clone()];
    ids.extend(memory.receipts.iter().map(|r| r.item_id.clone()));
    Ok(engine.private_ids(&ids)?.is_empty())
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
    use super::{AnswerMeta, Side, Sides, bold_matches, carries, settle};

    const CLUB1: &str = "The book club meets on the first Tuesday of the month at the library.";
    const CLUB2: &str =
        "The book club meeting is on the second Wednesday of the month at the community hall.";
    const WED: &str =
        "The dentist appointment with Dr Okafor is on Wednesday 18 November at 10:00.";
    const NOW_THU: &str = "Dr Okafor's receptionist called: it is now Thursday 19 November at 14:30, not Wednesday 18 November.";

    fn side(id: &str, sentence: &str) -> Side {
        Side {
            id: id.into(),
            sentence: sentence.into(),
        }
    }

    fn cites(ids: &[&str]) -> AnswerMeta {
        AnswerMeta {
            cited: ids.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn known(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn club() -> Vec<Sides> {
        vec![Sides {
            older: side("club1", CLUB1),
            newer: side("club2", CLUB2),
            ordered: true,
            moved: false,
            notice: None,
            newer_was: None,
        }]
    }

    fn moved() -> Vec<Sides> {
        vec![Sides {
            older: side("wed", WED),
            newer: side("line:itm:moved", NOW_THU),
            ordered: true,
            moved: true,
            notice: Some("notice".into()),
            newer_was: None,
        }]
    }

    const ALL: [&str; 5] = ["club1", "club2", "wed", "line:itm:moved", "notice"];

    /// Tiny's second tries on the book club question, citing both notes but saying one: the
    /// other is added after the answer's own words.
    #[test]
    fn an_open_question_answered_with_one_side_gets_the_other() {
        let pairs = club();
        let all = known(&ALL);
        let said = "The book club meeting is on the second Wednesday of the month, but it is not settled which fact is correct.";
        let (extra, ids) = settle(&pairs, &cites(&["club2", "club1"]), said, &all).unwrap();
        assert_eq!(extra, format!(" An earlier note says: {CLUB1}"));
        assert!(ids.is_empty(), "both were cited already");
        let said = "The book club meets on the first Tuesday of the month at the library.";
        let (extra, ids) = settle(&pairs, &cites(&["club1"]), said, &all).unwrap();
        assert_eq!(
            extra,
            format!(" A later note says: {CLUB2} It is not settled which is right.")
        );
        assert_eq!(ids, ["club2"]);
        let both =
            "Book club meets on the first Tuesday or the second Wednesday; it is not settled.";
        assert!(settle(&pairs, &cites(&["club1", "club2"]), both, &all).is_none());
        // Not about the question, or not sure: left as it is.
        assert!(settle(&pairs, &cites(&["other"]), "Bins go out on Tuesday.", &all).is_none());
        let unsure = AnswerMeta {
            not_sure: true,
            ..cites(&["club1"])
        };
        assert!(settle(&pairs, &unsure, "", &all).is_none());
    }

    /// Both sides named, one picked: that it is not settled is added.
    #[test]
    fn an_answer_that_picks_one_of_two_named_sides_is_told_not_settled() {
        let said = "The book club meets on the first Tuesday, not the second Wednesday.";
        let (extra, ids) =
            settle(&club(), &cites(&["club1", "club2"]), said, &known(&ALL)).unwrap();
        assert_eq!(extra, " It is not settled which is right.");
        assert!(ids.is_empty());
    }

    /// A side whose receipt is not among the facts is never added.
    #[test]
    fn a_side_with_no_receipt_is_never_added() {
        let said = "The book club meets on the first Tuesday of the month at the library.";
        assert!(settle(&club(), &cites(&["club1"]), said, &known(&["club1"])).is_none());
    }

    /// An answer about two things keeps all it said; only the missing side is added.
    #[test]
    fn a_multi_fact_answer_keeps_its_other_content() {
        let said =
            "The boiler service is on Monday 30 November. Book club is on the first Tuesday.";
        let (extra, ids) =
            settle(&club(), &cites(&["boiler", "club1"]), said, &known(&ALL)).unwrap();
        let whole = format!("{said}{extra}");
        assert!(whole.starts_with(said));
        assert!(whole.contains(CLUB2) && whole.ends_with("It is not settled which is right."));
        assert_eq!(ids, ["club2"]);
    }

    /// A move answered without where it went gets the note's line; one that gave the old day is
    /// told it has since moved. Saying the new day and time in any usual form is enough.
    #[test]
    fn a_move_without_where_it_went_gets_the_note() {
        let pairs = moved();
        let all = known(&ALL);
        let (extra, ids) = settle(
            &pairs,
            &cites(&["notice"]),
            "The dentist appointment has been moved.",
            &all,
        )
        .unwrap();
        assert_eq!(extra, format!(" The later note says: {NOW_THU}"));
        assert_eq!(ids, ["line:itm:moved"]);
        let (extra, _) = settle(&pairs, &cites(&["line:itm:moved"]), WED, &all).unwrap();
        assert_eq!(
            extra,
            format!(" It has since moved. The later note says: {NOW_THU}")
        );
        for said in [
            "Your dentist appointment is now on Thursday 19 November at 14:30.",
            "The dentist appointment moved to Thursday at 2:30 pm.",
            "It is on 19th of November, 2.30pm.",
        ] {
            assert!(
                settle(&pairs, &cites(&["line:itm:moved"]), said, &all).is_none(),
                "{said}"
            );
        }
        let no_time = "The dentist appointment is on Thursday 19 November.";
        assert!(settle(&pairs, &cites(&["line:itm:moved"]), no_time, &all).is_some());
    }

    #[test]
    fn a_side_is_said_with_its_day_and_its_time_where_it_is_spoken_of() {
        assert!(carries(
            "Book club: first Tuesday, at the library.",
            CLUB1,
            CLUB2
        ));
        assert!(
            carries("Book club is on Tuesdays.", CLUB1, CLUB2),
            "the day is enough"
        );
        assert!(!carries("Book club is at the library.", CLUB1, CLUB2));
        assert!(carries(
            "The dentist appointment is Thursday 19 November at 2:30 pm.",
            NOW_THU,
            WED
        ));
        assert!(!carries(
            "It is at 2:30 am on Thursday 19 November.",
            NOW_THU,
            WED
        ));
        assert!(carries("It is at 2:30 on 19 November.", NOW_THU, WED));
        assert!(
            !carries(
                "The dentist appointment is soon. Bins go out Thursday 19 November at 14:30.",
                NOW_THU,
                WED
            ),
            "a day said of something else"
        );
        let room = "The meeting is in room 214 on Friday; call 0161 496 0000.";
        let other = "The meeting is on Monday.";
        assert!(
            carries("The meeting is on Friday.", room, other),
            "numbers are no day"
        );
        assert!(carries(
            "At the hall.",
            "Meets at the hall.",
            "Meets at the library."
        ));
        assert!(!carries(
            "At the hall.",
            "Meets at the library.",
            "Meets at the hall."
        ));
    }

    /// On a phone, the old value goes with a move only when the old memory may go there: an
    /// appointment from a note that also holds something private does not, though the note
    /// that says it moved does. The search's own filter is left out to reach this check.
    #[test]
    fn on_a_phone_an_old_value_that_may_not_go_there_stays_out_of_the_move() {
        use nocturne_contract::{Kept, Source, Span};
        use nocturne_engine::{Candidate, Engine, ManualClock, Predicate};
        let dir = tempfile::tempdir().unwrap();
        let clock = std::sync::Arc::new(ManualClock::at("2026-11-12T08:05:00Z", "Europe/London"));
        let engine = Engine::open(dir.path(), clock.clone()).unwrap();
        let apply = |text: &str, read: Vec<Candidate>| {
            let item = match engine.keep_text(text, Source::QuickCapture).unwrap() {
                Kept::New { item } | Kept::Duplicate { item } => item.id,
            };
            let lease = engine
                .lease_item(&item, std::time::Duration::from_secs(60))
                .unwrap();
            engine.apply_candidates(&item, &lease.lease, read).unwrap()
        };
        let mut wed = Candidate::new(
            "Dentist appointment",
            Predicate::Scheduled,
            "Wednesday 18 November at 10:00",
            WED,
            0.9,
        );
        wed.due = Some("2026-11-18".parse().unwrap());
        wed.span = Some(Span::Quote { text: WED.into() });
        let pills = Candidate::new("Gran", Predicate::Takes, "pills", "Gran takes pills.", 0.9);
        let first = apply(&format!("{WED} Gran takes pills."), vec![wed, pills]);
        engine.set_private(&first.created[1], true).unwrap();
        clock.set_str("2026-11-12T12:00:00Z");
        apply(
            "The dentist appointment has been moved.",
            vec![Candidate::new(
                "Dentist appointment",
                Predicate::Other("moved".into()),
                "moved",
                "The dentist appointment has been moved.",
                0.9,
            )],
        );
        let hits = engine
            .retrieve("When is my dentist appointment?", None, 12)
            .unwrap();
        assert_eq!(engine.later_moves(&hits).unwrap().len(), 1);
        let (_, _, facts, pairs) = super::with_disputes(&engine, hits.clone(), true).unwrap();
        assert_eq!(pairs.len(), 1, "the move stands");
        assert!(facts.iter().all(|f| f.memory.was.is_none()), "{facts:?}");
        let (_, _, facts, _) = super::with_disputes(&engine, hits, false).unwrap();
        assert!(
            facts.iter().any(|f| f.memory.was.is_some()),
            "here it goes with it"
        );
    }

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
