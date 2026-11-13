//! The single reader of kept items.
//!
//! One `Reader` holds the mind and reads one item at a time, in this order of priority: an item
//! the person asked to read now; (an ask in flight pre-empts everything); new items, on mains
//! power or for voice notes; the tidy's drain; re-reads. A read is leased from the engine for ten
//! minutes and renewed every five while the mind works. If the mind goes away mid-item, an ask
//! pre-empts, or the app quits, the lease is given back without counting against the item and
//! nothing half-done is applied: memories are written in one transaction, after the mind has
//! finished with the item.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jiff::Timestamp;
use nocturne_contract::{CouldntReadReason, ItemKind, ItemStatus, ItemView, NoticeCode, Source};
use nocturne_engine::{
    ApplyReport, Candidate, Clock, Engine, Error as EngineError, ExtractContext, FailureOutcome,
    Mind, MindError, PendingItem, RecordOutcome, Transcript, Understanding,
};
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;

use crate::chunk::{CHUNK_CHARS, split_chunks};
use crate::events::{Events, notice};
use crate::host::{Host, probe};
use crate::{MindSlot, current_mind};

/// How long a lease lasts, and how often it is renewed while the mind works.
const LEASE: Duration = Duration::from_secs(10 * 60);
const RENEW_EVERY: Duration = Duration::from_secs(5 * 60);
/// Back-off after the mind is unavailable, busy or out of memory.
const BACKOFF: [Duration; 4] = [
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
    Duration::from_secs(300),
];
/// An item the mind failed on waits this long before another try.
const FAILURE_COOLDOWN: Duration = Duration::from_secs(30);
/// A re-read that failed is left alone this long.
const REREAD_COOLDOWN: jiff::SignedDuration = jiff::SignedDuration::from_mins(10);
/// A re-read that has failed this many times is given up on: its mark is cleared.
const REREAD_FAILURES: u32 = 3;
/// How long the run loop sleeps when there is nothing to do.
const IDLE_POLL: Duration = Duration::from_secs(30);
/// Known entities offered to the extractor.
const KNOWN_ENTITIES: usize = 200;
/// Items considered per tick.
const PEEK: usize = 200;

/// What one `tick` did.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Nothing to read, or nothing allowed to be read now.
    Idle,
    /// Paused by the person.
    Paused,
    /// An ask holds the mind.
    Asking,
    /// No mind yet, or it has gone away.
    NoMind,
    /// Waiting out a back-off after the mind failed.
    Backoff { until: Timestamp },
    /// An item was read (or given back).
    Read { item: String, outcome: Outcome },
}

/// How reading one item ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Understood,
    /// A scanned document with a title: kept under it, text not read.
    PartlyUnderstood,
    /// The item cannot be read, for this reason.
    CouldntRead(CouldntReadReason),
    /// The mind failed; the item will be tried again later.
    Retry,
    /// The mind failed three times; the item is given up on.
    GaveUp,
    /// The lease was given back: the mind went away, an ask pre-empted, or the app is quitting.
    Returned,
    /// Someone else took the item (undone, forgotten, lease expired); nothing was written.
    Lost,
}

/// Counters, for the app's debug view and for tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReaderStats {
    pub items_read: u64,
    pub renewals: u64,
}

/// An ask is being answered: the reader gives way until the guard is dropped.
pub struct AskGuard {
    reader: Reader,
}

impl Drop for AskGuard {
    fn drop(&mut self) {
        let mut st = self.reader.state();
        st.asks = st.asks.saturating_sub(1);
        drop(st);
        self.reader.inner.wake.notify_one();
    }
}

/// How far the tidy's drain has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DrainProgress {
    pub done: u32,
    pub total: u32,
    /// Reinforcements made while the drain ran.
    pub merged: u32,
    /// Estimated time left, from the mean of the last ten items.
    pub eta: Option<Duration>,
}

#[derive(Debug, Default)]
struct DrainState {
    remaining: HashSet<String>,
    total: u32,
    merged: u32,
    paused: bool,
    cancelled: bool,
    durations: VecDeque<Duration>,
}

/// The items the tidy asked to have read before it writes the page.
#[derive(Debug, Clone)]
pub struct DrainHandle {
    st: Arc<Mutex<DrainState>>,
}

impl DrainHandle {
    fn lock(&self) -> std::sync::MutexGuard<'_, DrainState> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn progress(&self) -> DrainProgress {
        let st = self.lock();
        let left = st.remaining.len() as u32;
        let eta = (!st.durations.is_empty() && left > 0).then(|| {
            let mean = st.durations.iter().sum::<Duration>() / st.durations.len() as u32;
            mean * left
        });
        DrainProgress {
            done: st.total.saturating_sub(left),
            total: st.total,
            merged: st.merged,
            eta,
        }
    }

    fn ids(&self) -> Vec<String> {
        self.lock().remaining.iter().cloned().collect()
    }

    /// Nothing left to read.
    pub fn is_done(&self) -> bool {
        self.lock().remaining.is_empty()
    }

    /// Hold the drain (the battery is low) or let it go on.
    pub fn set_paused(&self, paused: bool) {
        self.lock().paused = paused;
    }

    /// Stop draining; what is left is read the ordinary way.
    pub fn cancel(&self) {
        let mut st = self.lock();
        st.cancelled = true;
        st.remaining.clear();
    }

    fn contains(&self, id: &str) -> bool {
        let st = self.lock();
        !st.paused && !st.cancelled && st.remaining.contains(id)
    }

    fn settle(&self, id: &str, took: Option<Duration>, merged: u32) {
        let mut st = self.lock();
        if st.remaining.remove(id) {
            st.merged += merged;
            if let Some(d) = took {
                st.durations.push_back(d);
                if st.durations.len() > 10 {
                    st.durations.pop_front();
                }
            }
        }
    }
}

#[derive(Default)]
struct State {
    paused: bool,
    asks: u32,
    read_now: HashSet<String>,
    read_all_now: bool,
    drain: Option<DrainHandle>,
    backoff_step: usize,
    backoff_until: Option<Timestamp>,
    unavailable_since: Option<Timestamp>,
    cooldown: HashMap<String, Timestamp>,
    reread_failures: HashMap<String, u32>,
    rereading: bool,
}

struct Inner {
    engine: Engine,
    slot: MindSlot,
    host: Arc<dyn Host>,
    clock: Arc<dyn Clock>,
    events: Arc<dyn Events>,
    state: Mutex<State>,
    wake: Notify,
    preempt: watch::Sender<u64>,
    busy: AtomicBool,
    items_read: AtomicU64,
    renewals: AtomicU64,
}

/// The reader. Cheap to clone; clones share one queue.
#[derive(Clone)]
pub struct Reader {
    inner: Arc<Inner>,
}

struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Why the item was picked; lower reads first.
const CLASS_NOW: u8 = 0;
const CLASS_NEW: u8 = 1;
const CLASS_DRAIN: u8 = 2;
const CLASS_REREAD: u8 = 3;

enum Abort {
    Mind(MindError),
    Engine(EngineError),
}

impl From<MindError> for Abort {
    fn from(e: MindError) -> Self {
        Abort::Mind(e)
    }
}

impl From<EngineError> for Abort {
    fn from(e: EngineError) -> Self {
        Abort::Engine(e)
    }
}

/// A voice note: read at once, on mains or not, because the person is waiting for the words.
fn is_voice_note(p: &PendingItem) -> bool {
    p.kind == ItemKind::Audio && matches!(p.source, Source::QuickCapture | Source::Recording)
}

impl Reader {
    pub fn new(
        engine: Engine,
        slot: MindSlot,
        host: Arc<dyn Host>,
        clock: Arc<dyn Clock>,
        events: Arc<dyn Events>,
    ) -> Reader {
        Reader {
            inner: Arc::new(Inner {
                engine,
                slot,
                host,
                clock,
                events,
                state: Mutex::new(State::default()),
                wake: Notify::new(),
                preempt: watch::channel(0).0,
                busy: AtomicBool::new(false),
                items_read: AtomicU64::new(0),
                renewals: AtomicU64::new(0),
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn stats(&self) -> ReaderStats {
        ReaderStats {
            items_read: self.inner.items_read.load(Ordering::SeqCst),
            renewals: self.inner.renewals.load(Ordering::SeqCst),
        }
    }

    /// Wake the run loop (new item kept, mind arrived, power changed).
    pub fn wake(&self) {
        self.inner.wake.notify_one();
    }

    /// Ask the item in flight to stop and give its lease back.
    fn preempt(&self) {
        self.inner.preempt.send_modify(|v| *v += 1);
    }

    /// The person pressed Read now on one item, or on everything waiting (`None`). Bypasses the
    /// power policy.
    pub async fn read_now(&self, item: Option<&str>) {
        match item {
            Some(id) => {
                // Moves it to the front of the queue as well.
                let i = id.to_string();
                let _ = self.eng(move |e| e.prioritize(&i)).await;
                self.state().read_now.insert(id.to_string());
            }
            None => self.state().read_all_now = true,
        }
        self.wake();
    }

    /// An ask is being answered: pre-empt the item in flight and read nothing until the guard
    /// is dropped.
    pub fn ask_begin(&self) -> AskGuard {
        self.state().asks += 1;
        self.preempt();
        AskGuard {
            reader: self.clone(),
        }
    }

    /// Pause (the person's pause) or resume. Pausing gives back the item in flight; the app
    /// unloads the models.
    pub fn set_paused(&self, paused: bool) {
        self.state().paused = paused;
        if paused {
            self.preempt();
        } else {
            self.wake();
        }
    }

    /// Whether an ask holds the mind.
    pub fn is_asking(&self) -> bool {
        self.state().asks > 0
    }

    /// Forget the drain's items that are gone (undone, forgotten, deleted) or no longer
    /// waiting, so the tidy never waits for what cannot come back.
    pub async fn prune_drain(&self, handle: &DrainHandle) {
        let ids = handle.ids();
        if ids.is_empty() {
            return;
        }
        let gone = self
            .eng(move |e| {
                let mut gone = Vec::new();
                for id in ids {
                    match e.get_item(&id) {
                        Ok(v) if matches!(v.status, ItemStatus::Kept | ItemStatus::Reading) => {}
                        Ok(_) | Err(EngineError::NotFound(_)) => gone.push(id),
                        Err(_) => {}
                    }
                }
                Ok(gone)
            })
            .await
            .unwrap_or_default();
        let mut st = handle.lock();
        for id in gone {
            st.remaining.remove(&id);
        }
    }

    /// Since when the mind has been unavailable (absent, or failing), if it is.
    pub fn unavailable_since(&self) -> Option<Timestamp> {
        self.state().unavailable_since
    }

    /// The items kept before `cutoff` that are still waiting or being read now.
    pub async fn start_drain(&self, cutoff: Timestamp) -> Result<DrainHandle, EngineError> {
        let ids = self
            .eng(move |e| {
                let mut ids: Vec<String> = Vec::new();
                for item in e.pending_items(usize::MAX)? {
                    if item
                        .kept_at
                        .parse::<Timestamp>()
                        .is_ok_and(|kept| kept < cutoff)
                    {
                        ids.push(item.id);
                    }
                }
                for item in e.items(nocturne_contract::ItemFilter {
                    since: None,
                    status: Some(ItemStatus::Reading),
                    limit: None,
                })? {
                    if item
                        .kept_at
                        .parse::<Timestamp>()
                        .is_ok_and(|kept| kept < cutoff)
                    {
                        ids.push(item.id);
                    }
                }
                Ok(ids)
            })
            .await?;
        let handle = DrainHandle {
            st: Arc::new(Mutex::new(DrainState {
                total: ids.len() as u32,
                remaining: ids.into_iter().collect(),
                ..DrainState::default()
            })),
        };
        let old = self.state().drain.replace(handle.clone());
        if let Some(old) = old {
            old.cancel();
        }
        self.wake();
        Ok(handle)
    }

    /// The tidy is done with its drain.
    pub fn end_drain(&self, handle: &DrainHandle) {
        let mut st = self.state();
        if st
            .drain
            .as_ref()
            .is_some_and(|d| Arc::ptr_eq(&d.st, &handle.st))
        {
            st.drain = None;
        }
    }

    /// Run the engine on a blocking thread.
    async fn eng<T, F>(&self, f: F) -> nocturne_engine::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Engine) -> nocturne_engine::Result<T> + Send + 'static,
    {
        let engine = self.inner.engine.clone();
        tokio::task::spawn_blocking(move || f(&engine))
            .await
            .unwrap_or_else(|_| Err(EngineError::Invalid("a worker thread failed".into())))
    }

    fn now(&self) -> Timestamp {
        self.inner.clock.now()
    }

    fn mark_unavailable(&self, now: Timestamp) {
        self.state().unavailable_since.get_or_insert(now);
    }

    fn mark_available(&self) {
        let mut st = self.state();
        st.unavailable_since = None;
        st.backoff_step = 0;
        st.backoff_until = None;
    }

    fn back_off(&self, now: Timestamp) {
        let mut st = self.state();
        let wait = BACKOFF[st.backoff_step.min(BACKOFF.len() - 1)];
        st.backoff_step += 1;
        st.backoff_until = now.checked_add(wait).ok();
        st.unavailable_since.get_or_insert(now);
    }

    /// Reset the back-off (the machine woke up, the mind restarted): try again now.
    pub fn retry_now(&self) {
        let mut st = self.state();
        st.backoff_until = None;
        st.cooldown.clear();
        drop(st);
        self.wake();
    }

    /// Read at most one item, to the end, and say what happened. Production calls this in a
    /// loop (`run`); tests call it directly.
    pub async fn tick(&self) -> Step {
        if self.inner.busy.swap(true, Ordering::SeqCst) {
            return Step::Idle;
        }
        let _busy = BusyGuard(&self.inner.busy);
        // Subscribe before choosing, so a pre-emption after this point is never missed.
        let mut preempt = self.inner.preempt.subscribe();
        let now = self.now();
        {
            let st = self.state();
            if st.paused {
                return Step::Paused;
            }
            if st.asks > 0 {
                return Step::Asking;
            }
        }
        let Some(mind) = current_mind(&self.inner.slot) else {
            self.mark_unavailable(now);
            return Step::NoMind;
        };
        {
            let mut st = self.state();
            if st.backoff_step == 0 {
                st.unavailable_since = None;
            }
            if let Some(until) = st.backoff_until
                && now < until
            {
                return Step::Backoff { until };
            }
        }
        let on_ac = probe(&self.inner.host, |h| h.on_ac_power())
            .await
            .unwrap_or(true);
        let picked = match self.choose(on_ac, now).await {
            Ok(Some(p)) => p,
            Ok(None) => return self.idle_step(),
            Err(e) => {
                tracing::warn!("could not pick an item to read: {e}");
                return Step::Idle;
            }
        };
        let (class, pending, reread) = picked;
        let id = pending.id.clone();
        self.announce(&id).await;
        let started = tokio::time::Instant::now();
        let outcome = self.run_item(&mind, pending, reread, &mut preempt).await;
        let took = started.elapsed();
        self.settle(&id, outcome, took, class).await;
        Step::Read { item: id, outcome }
    }

    /// Nothing to read: clear the re-reading notice if it was up.
    fn idle_step(&self) -> Step {
        let was = std::mem::take(&mut self.state().rereading);
        if was {
            self.inner.events.notice_cleared(NoticeCode::ReReading);
        }
        Step::Idle
    }

    /// Pick the next item and lease it.
    async fn choose(
        &self,
        on_ac: bool,
        now: Timestamp,
    ) -> Result<Option<(u8, PendingItem, bool)>, EngineError> {
        let (read_now, read_all, drain, cooldown) = {
            let mut st = self.state();
            st.cooldown.retain(|_, until| *until > now);
            (
                st.read_now.clone(),
                st.read_all_now,
                st.drain.clone(),
                st.cooldown.clone(),
            )
        };
        self.eng(move |e| {
            let mut best: Option<(u8, PendingItem)> = None;
            let mut requested = false;
            let waiting = e.pending_items(PEEK)?;
            let waiting_ids: HashSet<String> = waiting.iter().map(|p| p.id.clone()).collect();
            for p in waiting {
                if cooldown.contains_key(&p.id) {
                    continue;
                }
                let class = if read_all || read_now.contains(&p.id) {
                    CLASS_NOW
                } else if on_ac || is_voice_note(&p) {
                    CLASS_NEW
                } else if drain.as_ref().is_some_and(|d| d.contains(&p.id)) {
                    CLASS_DRAIN
                } else {
                    continue;
                };
                if best.as_ref().is_none_or(|(c, _)| class < *c) {
                    best = Some((class, p));
                }
            }
            // "Read it again" on an item that was read before: the person's own request, so it
            // is allowed even after something was forgotten from it.
            if best.as_ref().is_none_or(|(c, _)| *c > CLASS_NOW) {
                for id in &read_now {
                    if waiting_ids.contains(id) || cooldown.contains_key(id) {
                        continue;
                    }
                    let readable = e.get_item(id).is_ok_and(|v| {
                        matches!(
                            v.status,
                            ItemStatus::Understood
                                | ItemStatus::PartlyUnderstood
                                | ItemStatus::NeedsYou
                                | ItemStatus::Unconfirmed
                        )
                    });
                    if readable {
                        best = Some((
                            CLASS_NOW,
                            PendingItem {
                                id: id.clone(),
                                ..empty_pending()
                            },
                        ));
                        requested = true;
                        break;
                    }
                }
            }
            if best.is_none() && on_ac {
                for id in e.rereads(10)? {
                    if !cooldown.contains_key(&id) {
                        best = Some((
                            CLASS_REREAD,
                            PendingItem {
                                id,
                                lease: String::new(),
                                ..empty_pending()
                            },
                        ));
                        break;
                    }
                }
            }
            let Some((class, p)) = best else {
                return Ok(None);
            };
            let leased = if requested {
                e.lease_item_by_request(&p.id, LEASE)
            } else {
                e.lease_item(&p.id, LEASE)
            };
            match leased {
                Ok(leased) => Ok(Some((class, leased, requested || class == CLASS_REREAD))),
                // Taken by someone else or gone: try again on the next tick.
                Err(EngineError::LeaseLost | EngineError::NotFound(_)) => Ok(None),
                Err(e) => Err(e),
            }
        })
        .await
    }

    /// Tell the app the item is being read now.
    async fn announce(&self, id: &str) {
        let id = id.to_string();
        if let Ok(view) = self.eng(move |e| e.get_item(&id)).await {
            self.inner.events.item_changed(&view);
        }
    }

    /// Read one leased item, giving the lease back if pre-empted.
    async fn run_item(
        &self,
        mind: &Arc<dyn Mind>,
        p: PendingItem,
        reread: bool,
        preempt: &mut watch::Receiver<u64>,
    ) -> Outcome {
        let id = p.id.clone();
        let lease = p.lease.clone();
        let work = self.process(mind, p, reread);
        tokio::pin!(work);
        let mut renew =
            tokio::time::interval_at(tokio::time::Instant::now() + RENEW_EVERY, RENEW_EVERY);
        let result = loop {
            tokio::select! {
                biased;
                _ = preempt.changed() => {
                    if self.give_back(&id, &lease).await {
                        return Outcome::Returned;
                    }
                    // Too late: the write had already committed. Report what it made.
                    return self.committed_outcome(&id).await;
                }
                r = &mut work => break r,
                _ = renew.tick() => {
                    let (i, l) = (id.clone(), lease.clone());
                    match self.eng(move |e| e.renew_lease(&i, &l, LEASE)).await {
                        Ok(()) => { self.inner.renewals.fetch_add(1, Ordering::SeqCst); }
                        // Either the item is already applied (only the vectors are left) or
                        // the lease is truly gone; the work finds out which when it writes.
                        Err(EngineError::LeaseLost) => {}
                        Err(e) => tracing::warn!("could not renew a lease: {e}"),
                    }
                }
            }
        };
        match result {
            Ok(outcome) => outcome,
            Err(abort) => self.handle_abort(&id, &lease, abort, reread).await,
        }
    }

    /// Give the lease back uncounted. False when there was nothing to give back (the read had
    /// already finished, or the item is gone).
    async fn give_back(&self, id: &str, lease: &str) -> bool {
        let (i, l) = (id.to_string(), lease.to_string());
        match self.eng(move |e| e.release_lease(&i, &l, false)).await {
            Ok(()) => true,
            Err(EngineError::LeaseLost) | Err(EngineError::NotFound(_)) => false,
            Err(e) => {
                tracing::warn!("could not give a lease back: {e}");
                false
            }
        }
    }

    /// What an item became when the read finished before it could be stopped.
    async fn committed_outcome(&self, id: &str) -> Outcome {
        let i = id.to_string();
        match self.eng(move |e| e.get_item(&i)).await {
            Ok(v) => match v.status {
                ItemStatus::Understood | ItemStatus::NeedsYou | ItemStatus::Unconfirmed => {
                    Outcome::Understood
                }
                ItemStatus::PartlyUnderstood => Outcome::PartlyUnderstood,
                ItemStatus::CouldntRead => {
                    Outcome::CouldntRead(v.reason.unwrap_or(CouldntReadReason::Unsupported))
                }
                ItemStatus::Kept | ItemStatus::Reading => Outcome::Lost,
            },
            Err(_) => Outcome::Lost,
        }
    }

    /// A re-read went wrong: the item stays as it was. After a few failures the mark is
    /// cleared and the item is left alone.
    async fn reread_failed(&self, id: &str, now: Timestamp) -> Outcome {
        let failures = {
            let mut st = self.state();
            st.cooldown.insert(id.to_string(), now + REREAD_COOLDOWN);
            let n = st.reread_failures.entry(id.to_string()).or_insert(0);
            *n += 1;
            *n
        };
        if failures >= REREAD_FAILURES {
            let i = vec![id.to_string()];
            let _ = self.eng(move |e| e.clear_reread(&i)).await;
            let mut st = self.state();
            st.reread_failures.remove(id);
            st.cooldown.remove(id);
        }
        Outcome::Returned
    }

    /// The mind (or the engine) stopped the read; decide what that means for the item.
    async fn handle_abort(&self, id: &str, lease: &str, abort: Abort, reread: bool) -> Outcome {
        let now = self.now();
        let (i, l) = (id.to_string(), lease.to_string());
        let fail = |reason: CouldntReadReason| {
            let (i, l) = (i.clone(), l.clone());
            async move {
                match self.eng(move |e| e.fail_item(&i, &l, reason)).await {
                    Ok(()) => Outcome::CouldntRead(reason),
                    Err(_) => Outcome::Lost,
                }
            }
        };
        match abort {
            Abort::Engine(EngineError::NotFound(_)) => Outcome::Lost,
            // A forget or an undo since the read began: nothing was written. Hand the lease
            // back so the item is not stuck as being read.
            Abort::Engine(EngineError::LeaseLost) => {
                self.give_back(id, lease).await;
                Outcome::Lost
            }
            Abort::Engine(e) => {
                tracing::warn!("reading {id} stopped: {e}");
                self.give_back(id, lease).await;
                self.state()
                    .cooldown
                    .insert(id.to_string(), now + FAILURE_COOLDOWN);
                Outcome::Returned
            }
            // Reading never goes online; an online failure here is treated as the mind away.
            Abort::Mind(MindError::Unavailable | MindError::Busy | MindError::Online(_)) => {
                self.give_back(id, lease).await;
                self.back_off(now);
                Outcome::Returned
            }
            Abort::Mind(MindError::OutOfMemory) => {
                self.give_back(id, lease).await;
                self.back_off(now);
                self.inner.events.out_of_memory();
                Outcome::Returned
            }
            // A re-read never takes away what an earlier reading understood: whatever goes
            // wrong, the item stays as it was and is tried again much later.
            Abort::Mind(
                MindError::TooLong
                | MindError::Unsupported(_)
                | MindError::Language(_)
                | MindError::Failed(_),
            ) if reread => {
                self.give_back(id, lease).await;
                self.reread_failed(id, now).await
            }
            Abort::Mind(MindError::TooLong) => fail(CouldntReadReason::TooLong).await,
            Abort::Mind(MindError::Unsupported(_)) => fail(CouldntReadReason::Unsupported).await,
            Abort::Mind(MindError::Language(_)) => fail(CouldntReadReason::Language).await,
            Abort::Mind(MindError::Failed(why)) => {
                tracing::warn!("the mind failed on an item: {why}");
                match self.eng(move |e| e.record_failure(&i, &l)).await {
                    Ok(FailureOutcome::GaveUp) => Outcome::GaveUp,
                    Ok(FailureOutcome::Retry) => {
                        let wait = FAILURE_COOLDOWN.as_secs() as i64;
                        self.state()
                            .cooldown
                            .insert(id.to_string(), now + jiff::SignedDuration::from_secs(wait));
                        Outcome::Retry
                    }
                    Err(_) => Outcome::Lost,
                }
            }
        }
    }

    /// Book-keeping after an item: queues, drain, notices, events.
    async fn settle(&self, id: &str, outcome: Outcome, took: Duration, class: u8) {
        let ok = matches!(outcome, Outcome::Understood | Outcome::PartlyUnderstood);
        if ok {
            self.state().reread_failures.remove(id);
            self.mark_available();
            self.inner.items_read.fetch_add(1, Ordering::SeqCst);
            let i = id.to_string();
            if let Err(e) = self.eng(move |e| e.record_reading_time(&i, took)).await {
                tracing::warn!("could not note how long reading took: {e}");
            }
        }
        let i = id.to_string();
        let view: Option<ItemView> = self.eng(move |e| e.get_item(&i)).await.ok();
        let left_queue = view
            .as_ref()
            .is_none_or(|v| !matches!(v.status, ItemStatus::Kept | ItemStatus::Reading));
        let (drain, announce_rereading) = {
            let mut st = self.state();
            if left_queue {
                st.read_now.remove(id);
            }
            let announce = class == CLASS_REREAD && !st.rereading;
            if announce {
                st.rereading = true;
            }
            (st.drain.clone(), announce)
        };
        if announce_rereading {
            self.inner
                .events
                .notice(notice(NoticeCode::ReReading, None));
        }
        if left_queue && let Some(d) = drain {
            d.settle(id, ok.then_some(took), 0);
        }
        if let Some(view) = &view {
            self.inner.events.item_changed(view);
        }
        if outcome == Outcome::GaveUp {
            self.inner
                .events
                .notice(notice(NoticeCode::CouldntRead, Some(id)));
        }
        if self.state().read_all_now {
            let more = self
                .eng(|e| e.pending_items(1).map(|p| !p.is_empty()))
                .await
                .unwrap_or(false);
            if !more {
                self.state().read_all_now = false;
            }
        }
    }

    /// Read the item through to its memories.
    async fn process(
        &self,
        mind: &Arc<dyn Mind>,
        p: PendingItem,
        reread: bool,
    ) -> Result<Outcome, Abort> {
        let started = tokio::time::Instant::now();
        let id = p.id.clone();
        let lease = p.lease.clone();
        let mut text: Option<String> = p.text.clone().filter(|t| !t.trim().is_empty());

        if text.is_none() {
            match p.kind {
                ItemKind::Text | ItemKind::Web => {
                    return self
                        .fail(&id, &lease, CouldntReadReason::Unsupported, reread)
                        .await;
                }
                ItemKind::Audio => {
                    let Some(path) = p.original.clone() else {
                        return self
                            .fail(&id, &lease, CouldntReadReason::Unsupported, reread)
                            .await;
                    };
                    let t: Transcript = mind.transcribe(&path).await?;
                    let body = t.text.clone();
                    let (i, l) = (id.clone(), lease.clone());
                    let recorded = self.eng(move |e| e.record_transcript(&i, &l, t)).await?;
                    if recorded == RecordOutcome::TooLong {
                        return self.too_long(&id, &lease, reread).await;
                    }
                    if body.trim().is_empty() {
                        return self
                            .fail(&id, &lease, CouldntReadReason::Unsupported, reread)
                            .await;
                    }
                    text = Some(body);
                }
                ItemKind::Image | ItemKind::Document => {
                    let Some(path) = p.original.clone() else {
                        return self
                            .fail(&id, &lease, CouldntReadReason::Unsupported, reread)
                            .await;
                    };
                    let mut u: Understanding = if p.kind == ItemKind::Image {
                        mind.read_image(&path).await?
                    } else {
                        mind.read_document(&path).await?
                    };
                    if u.text.trim().is_empty()
                        && let Some(summary) = u.summary.clone().filter(|s| !s.trim().is_empty())
                    {
                        u.text = summary;
                    }
                    if u.text.trim().is_empty() {
                        // A scan with nothing readable: kept under its title if it has one.
                        let title = u.title.trim().to_string();
                        if title.is_empty() {
                            return self
                                .fail(&id, &lease, CouldntReadReason::Unsupported, reread)
                                .await;
                        }
                        let (i, l) = (id.clone(), lease.clone());
                        self.eng(move |e| e.mark_partly(&i, &l, &title)).await?;
                        return Ok(Outcome::PartlyUnderstood);
                    }
                    let body = u.text.clone();
                    let (i, l) = (id.clone(), lease.clone());
                    let recorded = self.eng(move |e| e.record_understanding(&i, &l, u)).await?;
                    if recorded == RecordOutcome::TooLong {
                        return self.too_long(&id, &lease, reread).await;
                    }
                    text = Some(body);
                }
            }
        }
        let text = text.unwrap_or_default();

        let now = self.now();
        let known = self
            .eng(|e| {
                Ok(e.entities(None)?
                    .into_iter()
                    .take(KNOWN_ENTITIES)
                    .map(|en| (en.name, en.kind))
                    .collect::<Vec<_>>())
            })
            .await?;
        let cx = ExtractContext {
            item_kind: p.kind,
            kept_at: p.kept_at.parse().unwrap_or(now),
            today: now.to_zoned(self.inner.clock.tz()).date(),
            known_entities: known,
        };
        let mut candidates: Vec<Candidate> = Vec::new();
        for chunk in split_chunks(&text, CHUNK_CHARS) {
            let found = mind.extract(&chunk, &cx).await?;
            merge_candidates(&mut candidates, found);
        }
        let (i, l) = (id.clone(), lease.clone());
        let took = started.elapsed();
        let report: ApplyReport = self
            .eng(move |e| e.apply_candidates_took(&i, &l, candidates, took))
            .await?;

        // The item is understood. Vectors are best effort: the tidy fills in any that are
        // missing.
        self.embed_new(mind, &report).await;
        if !report.reinforced.is_empty() {
            let drain = self.state().drain.clone();
            if let Some(d) = drain {
                let n = report.reinforced.len() as u32;
                let mut st = d.lock();
                if st.remaining.contains(&id) {
                    st.merged += n;
                }
            }
        }
        Ok(Outcome::Understood)
    }

    /// The engine refused the reading as too long. For a first reading it has marked the item;
    /// for a re-read it has left the item alone and the lease goes back.
    async fn too_long(&self, id: &str, lease: &str, reread: bool) -> Result<Outcome, Abort> {
        if reread {
            self.give_back(id, lease).await;
            return Ok(self.reread_failed(id, self.now()).await);
        }
        Ok(Outcome::CouldntRead(CouldntReadReason::TooLong))
    }

    async fn fail(
        &self,
        id: &str,
        lease: &str,
        reason: CouldntReadReason,
        reread: bool,
    ) -> Result<Outcome, Abort> {
        if reread {
            self.give_back(id, lease).await;
            return Ok(self.reread_failed(id, self.now()).await);
        }
        let (i, l) = (id.to_string(), lease.to_string());
        self.eng(move |e| e.fail_item(&i, &l, reason)).await?;
        Ok(Outcome::CouldntRead(reason))
    }

    /// Vectors for the memories a reading created or reworded.
    async fn embed_new(&self, mind: &Arc<dyn Mind>, report: &ApplyReport) {
        let ids: Vec<String> = report
            .created
            .iter()
            .chain(report.reworded.iter())
            .cloned()
            .collect();
        if ids.is_empty() {
            return;
        }
        let rows = self
            .eng(move |e| {
                let mut rows = Vec::new();
                for id in ids {
                    if let Ok(m) = e.get_memory(&id) {
                        rows.push((id, m.sentence));
                    }
                }
                Ok(rows)
            })
            .await
            .unwrap_or_default();
        if rows.is_empty() {
            return;
        }
        let texts: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
        match mind.embed(&texts).await {
            Ok(vecs) if vecs.len() == rows.len() => {
                let pairs: Vec<(String, Vec<f32>)> =
                    rows.into_iter().map(|r| r.0).zip(vecs).collect();
                let _ = self.eng(move |e| e.store_embeddings(pairs)).await;
            }
            Ok(_) => tracing::warn!("the mind returned the wrong number of vectors"),
            Err(e) => tracing::debug!("vectors left for the tidy: {e}"),
        }
    }

    /// Read until told to stop. On shutdown the item in flight gives its lease back.
    pub async fn run(&self, shutdown: CancellationToken) {
        while !shutdown.is_cancelled() {
            let step = {
                let tick = self.tick();
                tokio::pin!(tick);
                tokio::select! {
                    step = &mut tick => step,
                    _ = shutdown.cancelled() => {
                        self.preempt();
                        tick.await
                    }
                }
            };
            let wait = match step {
                Step::Read { .. } => continue,
                Step::Backoff { until } => {
                    let secs = until.as_second() - self.now().as_second();
                    Duration::from_secs(secs.clamp(1, IDLE_POLL.as_secs() as i64) as u64)
                }
                _ => IDLE_POLL,
            };
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = self.inner.wake.notified() => {}
                _ = tokio::time::sleep(wait) => {}
            }
        }
    }
}

fn empty_pending() -> PendingItem {
    PendingItem {
        id: String::new(),
        kind: ItemKind::Text,
        source: Source::Paste,
        trust: nocturne_contract::Trust::Trusted,
        kept_at: String::new(),
        original: None,
        mime: None,
        text: None,
        attempts: 0,
        lease: String::new(),
    }
}

/// Add `found` to `into`, dropping a candidate that says what an earlier one already said.
fn merge_candidates(into: &mut Vec<Candidate>, found: Vec<Candidate>) {
    for c in found {
        if c.subject.trim().is_empty() || c.object.trim().is_empty() || c.sentence.trim().is_empty()
        {
            continue;
        }
        let key = |c: &Candidate| {
            (
                c.subject.trim().to_lowercase(),
                c.predicate.as_str().to_string(),
                c.object.trim().to_lowercase(),
            )
        };
        let k = key(&c);
        if let Some(existing) = into.iter_mut().find(|e| key(e) == k) {
            if c.confidence > existing.confidence {
                *existing = c;
            }
        } else {
            into.push(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocturne_engine::Predicate;

    fn c(subject: &str, object: &str, confidence: f32) -> Candidate {
        Candidate::new(subject, Predicate::Knows, object, "A sentence.", confidence)
    }

    #[test]
    fn candidates_from_chunks_merge_and_blank_ones_are_dropped() {
        let mut all = vec![c("Gran", "Leeds", 0.5)];
        merge_candidates(
            &mut all,
            vec![
                c(" gran ", "LEEDS", 0.9),
                c("Leo", "swimming", 0.7),
                c("", "x", 0.9),
            ],
        );
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].confidence, 0.9, "the surer one is kept");
    }
}
