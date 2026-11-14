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
    let (hits, found, facts) = match blocking(move || with_disputes(&e, hits, public_only)).await {
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
    conclude(sink, &engine, &hits, found, &meta, &said).await;
}

/// What one question found, for the online answer.
struct Asked<'a> {
    engine: &'a Engine,
    question: &'a str,
    hits: &'a [Hit],
    found: usize,
    facts: &'a [Fact],
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
            None => sendable.push((*fact).clone()),
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

/// The facts for the mind: each hit with the memories it disagrees with in an open
/// which-is-right question. A memory on the other side that the search did not find is brought
/// in, so the answer can give both and cite both: at most `MAX_SIDES` of them, with the facts
/// kept to `RETRIEVE_K` by leaving out the lowest hits. A side that can no longer be read (it is
/// being forgotten), or a private one when `public_only`, is left out. Returns the facts' hits, how many of them the search found
/// (they come first), and the facts.
fn with_disputes(
    engine: &Engine,
    mut hits: Vec<Hit>,
    public_only: bool,
) -> nocturne_engine::Result<(Vec<Hit>, usize, Vec<Fact>)> {
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
    Ok((hits, found, facts))
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
