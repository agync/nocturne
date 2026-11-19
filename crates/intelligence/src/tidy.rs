//! The overnight tidy: read what is still unread, let go of what faded, write the morning page.
//!
//! The tidy never reads an item itself. It asks the [`Reader`] to drain the items kept before
//! it began, watches progress, and when the drain is done (or the deadline comes) it composes
//! the page to one fixed end, writes the headline, saves it and tells the app. A page saved
//! with items still unread says so (`pending`), and is saved once more if the backlog drains
//! before the person first opens it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use jiff::civil::Date;
use jiff::{SignedDuration, Timestamp};
use nocturne_contract::{
    Edition, ItemFilter, ItemStatus, MemoryFilter, NoticeCode, PageSummary, PageView, Span,
    TidyProgress, TidyReport,
};
use nocturne_engine::day::page_instant;
use nocturne_engine::{BoxFuture, Clock, Engine, Error as EngineError};
use tokio::sync::{Mutex, watch};

use crate::events::{Events, notice};
use crate::host::{Host, SleepGuard, probe};
use crate::reader::{DrainHandle, Reader};
use crate::schedule::parse_hm;
use crate::{MindSlot, current_mind};
use nocturne_engine::coverage::uncovered;

/// Below this charge, off mains, the tidy waits for power.
pub const LOW_BATTERY: u8 = 40;
/// How long the mind may be unavailable before the page is written without what is unread.
pub const UNAVAILABLE_LIMIT: Duration = Duration::from_secs(20 * 60);
/// A tidy with no page time ahead (a catch-up) saves its page after this long at the latest.
pub const CATCH_UP_LIMIT: Duration = Duration::from_secs(2 * 60 * 60);
/// The headline gets this long.
const HEADLINE_LIMIT: Duration = Duration::from_secs(60);
const BACKFILL_CHUNK: usize = 200;
/// Filling in missing vectors gets this long in all, and each call this long.
const BACKFILL_LIMIT: Duration = Duration::from_secs(3 * 60);
const EMBED_LIMIT: Duration = Duration::from_secs(60);
/// A night's tidy asks for at most this many careful re-reads.
const CAREFUL_PER_NIGHT: usize = 8;
/// Items kept this long before the page's end are looked at for a careful re-read: each is
/// looked at on the night after it was kept.
const CAREFUL_WINDOW: SignedDuration = SignedDuration::from_hours(24);

/// Runs once the backlog is read and before the page is composed: the journal's looking back,
/// which the page may then carry a line about.
pub type BeforePage = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TidyKind {
    /// In the small hours, on mains, with nobody at the keyboard.
    Night,
    /// The page is late (the laptop was off or asleep): now, whatever the hour.
    CatchUp,
}

/// What one `tick` of the tidy did.
#[derive(Debug, Clone, PartialEq)]
pub enum TidyStep {
    /// No tidy running.
    Idle,
    /// Reading what is unread; `done` of `total`.
    Running { done: u32, total: u32 },
    /// Waiting for power.
    PausedBattery { done: u32, total: u32 },
    /// The page was saved.
    Finished(PageSummary),
    /// A page for this date already existed, so nothing was written.
    Skipped,
    /// The backlog behind a saved page drained and the page was saved again.
    Resaved(PageSummary),
}

struct Run {
    kind: TidyKind,
    date: Date,
    started: Timestamp,
    /// The one end the page is composed and saved to.
    end: Timestamp,
    /// The page is saved by then whatever is unread: the page time when it is still ahead,
    /// otherwise a fixed time after the start.
    deadline: Timestamp,
    drain: DrainHandle,
    guard: Option<SleepGuard>,
    paused: bool,
    overridden: bool,
    last_progress: Option<TidyProgress>,
}

/// A saved page that still has unread items behind it.
struct Watch {
    date: Date,
    end: Timestamp,
    tidied_from: Timestamp,
}

#[derive(Default)]
struct Slots {
    run: Option<Run>,
    watch: Option<Watch>,
}

struct Inner {
    engine: Engine,
    reader: Reader,
    slot: MindSlot,
    host: Arc<dyn Host>,
    clock: Arc<dyn Clock>,
    events: Arc<dyn Events>,
    slots: Mutex<Slots>,
    /// A page is being written (the lock is not held meanwhile, so pause and status still work).
    finishing: AtomicBool,
    anyway: AtomicBool,
    progress: watch::Sender<Option<TidyProgress>>,
    before_page: std::sync::Mutex<Option<BeforePage>>,
}

/// The tidy. Cheap to clone; clones share one state.
#[derive(Clone)]
pub struct Tidy {
    inner: Arc<Inner>,
}

impl Tidy {
    pub fn new(
        engine: Engine,
        reader: Reader,
        slot: MindSlot,
        host: Arc<dyn Host>,
        clock: Arc<dyn Clock>,
        events: Arc<dyn Events>,
    ) -> Tidy {
        Tidy {
            inner: Arc::new(Inner {
                engine,
                reader,
                slot,
                host,
                clock,
                events,
                slots: Mutex::new(Slots::default()),
                finishing: AtomicBool::new(false),
                anyway: AtomicBool::new(false),
                progress: watch::channel(None).0,
                before_page: std::sync::Mutex::new(None),
            }),
        }
    }

    /// What runs after the backlog is read and before the page is composed.
    pub fn set_before_page(&self, hook: BeforePage) {
        *self
            .inner
            .before_page
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(hook);
    }

    /// The person pressed "Tidy anyway": the battery rule is set aside until this tidy is done.
    pub fn tidy_anyway(&self) {
        self.inner.anyway.store(true, Ordering::SeqCst);
    }

    /// Consume a "Tidy anyway" request (the scheduler does, once per tick).
    pub(crate) fn take_anyway(&self) -> bool {
        self.inner.anyway.swap(false, Ordering::SeqCst)
    }

    /// The tidy's progress, for the status hub and for a page being written. `None` when no
    /// tidy is running.
    pub fn progress(&self) -> watch::Receiver<Option<TidyProgress>> {
        self.inner.progress.subscribe()
    }

    /// The person paused: a running tidy lets the machine sleep while it waits.
    pub async fn hold_off(&self) {
        if let Some(run) = self.inner.slots.lock().await.run.as_mut() {
            run.guard = None;
        }
    }

    /// Whether a tidy is running.
    pub async fn running(&self) -> bool {
        self.inner.finishing.load(Ordering::SeqCst) || self.inner.slots.lock().await.run.is_some()
    }

    fn now(&self) -> Timestamp {
        self.inner.clock.now()
    }

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

    fn emit_progress(&self, run: &mut Run) {
        let p = run.drain.progress();
        let progress = TidyProgress {
            done: p.done,
            total: p.total,
            eta_seconds: p.eta.map(|d| d.as_secs().min(u64::from(u32::MAX)) as u32),
        };
        if run.last_progress.as_ref() != Some(&progress) {
            run.last_progress = Some(progress.clone());
            self.inner.progress.send_replace(Some(progress.clone()));
            self.inner.events.tidy_progress(Some(progress));
        }
    }

    /// Begin the tidy for the page dated `date`. `overridden` sets the battery rule aside.
    pub async fn start(
        &self,
        kind: TidyKind,
        date: Date,
        overridden: bool,
    ) -> nocturne_engine::Result<()> {
        let mut slots = self.inner.slots.lock().await;
        if slots.run.is_some() {
            return Ok(());
        }
        let started = self.now();
        let tz = self.inner.clock.tz();
        let hm = parse_hm(&self.eng(|e| e.settings()).await?.page_time);
        let instant = page_instant(date, hm, &tz)?;
        // Items kept after this belong to the next page, so only what came before is read
        // and counted.
        let end = instant.min(started);
        // The night reads what the person was still writing in the journal.
        if let Err(e) = self.eng(|e| e.release_entry_holds()).await {
            tracing::warn!("could not release the held journal entries: {e}");
        }
        let drain = self.inner.reader.start_drain(end).await?;
        let mut run = Run {
            kind,
            date,
            started,
            end,
            deadline: if instant > started {
                instant
            } else {
                started + SignedDuration::try_from(CATCH_UP_LIMIT).unwrap_or_default()
            },
            drain,
            guard: None,
            paused: false,
            overridden,
            last_progress: None,
        };
        if kind == TidyKind::CatchUp {
            self.inner
                .events
                .notice(notice(NoticeCode::CatchingUp, None));
        }
        self.emit_progress(&mut run);
        slots.run = Some(run);
        Ok(())
    }

    /// Move the tidy on: battery, sleep guard, progress, and the page when it is time.
    pub async fn tick(&self) -> TidyStep {
        let mut slots = self.inner.slots.lock().await;
        if slots.run.is_none() {
            let watch = slots.watch.take();
            drop(slots);
            return self.tick_watch(watch).await;
        }
        // Items undone or forgotten meanwhile must not be waited for.
        let drain = slots.run.as_ref().map(|r| r.drain.clone());
        if let Some(drain) = drain {
            self.inner.reader.prune_drain(&drain).await;
        }
        let now = self.now();
        let ac = probe(&self.inner.host, |h| h.on_ac_power()).await;
        let percent = probe(&self.inner.host, |h| h.battery_percent()).await;
        let anyway = self.take_anyway();
        let run = slots.run.as_mut().expect("checked above");
        run.overridden |= anyway;

        // Off mains and low: wait for power, unless the person said otherwise.
        let low = ac == Some(false) && percent.is_some_and(|p| p < LOW_BATTERY);
        let must_pause = low && !run.overridden;
        if must_pause && !run.paused {
            run.paused = true;
            run.drain.set_paused(true);
            self.inner
                .events
                .notice(notice(NoticeCode::TidyPausedBattery, None));
        } else if !must_pause && run.paused {
            run.paused = false;
            run.drain.set_paused(false);
            self.inner
                .events
                .notice_cleared(NoticeCode::TidyPausedBattery);
            self.inner.reader.wake();
        }
        // The machine is kept awake only while a tidy runs on mains.
        let want_guard = !run.paused && ac != Some(false);
        if !want_guard {
            run.guard = None;
        } else if run.guard.is_none() {
            run.guard = probe(&self.inner.host, |h| h.inhibit_sleep()).await;
        }
        self.emit_progress(run);

        let p = run.drain.progress();
        let unavailable_for = self
            .inner
            .reader
            .unavailable_since()
            .map(|since| since.max(run.started))
            .map(|since| now.duration_since(since));
        let late = now >= run.deadline;
        let mind_gone = unavailable_for
            .is_some_and(|d| d >= SignedDuration::try_from(UNAVAILABLE_LIMIT).unwrap_or_default());
        if !(run.drain.is_done() || late || mind_gone) {
            return if run.paused {
                TidyStep::PausedBattery {
                    done: p.done,
                    total: p.total,
                }
            } else {
                TidyStep::Running {
                    done: p.done,
                    total: p.total,
                }
            };
        }
        let run = slots.run.take().expect("checked above");
        self.inner.finishing.store(true, Ordering::SeqCst);
        drop(slots);
        self.inner.reader.end_drain(&run.drain);
        // The mind is asked for things now; the lock is free so pausing and status still work.
        let (step, watch) = self.finish(run).await;
        if watch.is_some() {
            self.inner.slots.lock().await.watch = watch;
        }
        self.inner.finishing.store(false, Ordering::SeqCst);
        step
    }

    /// Write and save the page. The watch is set when items are still unread behind it.
    async fn finish(&self, run: Run) -> (TidyStep, Option<Watch>) {
        let Run {
            kind,
            date,
            started,
            end,
            drain,
            guard,
            ..
        } = run;
        // The page is composed to `end` now; anything still unread stays counted as pending.
        let progress = drain.progress();
        drop(guard);
        self.inner.progress.send_replace(None);
        self.inner.events.tidy_progress(None);
        self.inner
            .events
            .notice_cleared(NoticeCode::TidyPausedBattery);
        if kind == TidyKind::CatchUp {
            self.inner.events.notice_cleared(NoticeCode::CatchingUp);
        }

        // A page for this date may have been saved while the tidy ran.
        let existing = self.eng(move |e| e.page(Some(date))).await;
        if matches!(existing, Ok(Some(_))) {
            return (TidyStep::Skipped, None);
        }

        let decayed = self
            .eng(|e| {
                let report = e.decay()?;
                let _ = e.retry_scrub();
                Ok(report)
            })
            .await
            .unwrap_or_default();
        self.backfill_embeddings().await;
        let hook = self
            .inner
            .before_page
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(hook) = hook {
            hook().await;
        }

        let Some(mut page) = self.compose(date, end).await else {
            return (TidyStep::Skipped, None);
        };
        let finished = self.now();
        self.write_headline(&mut page).await;
        page.tidied_from = Some(started.to_string());
        page.tidied_to = Some(finished.to_string());
        let pending = page.pending;
        let saved = {
            let page = page.clone();
            self.eng(move |e| e.save_page_until(&page, end)).await
        };
        if let Err(e) = saved {
            tracing::warn!("could not save the page: {e}");
            return (TidyStep::Skipped, None);
        }
        let summary = self.summary(date).await.unwrap_or(PageSummary {
            date: date.to_string(),
            page_number: page.page_number,
            headline: page.headline.clone(),
            needs_you: page.needs_you.len() as u32,
        });
        let report = TidyReport {
            from: started.to_string(),
            to: finished.to_string(),
            items: progress.done,
            merged: progress.merged,
            let_go: decayed.let_go,
        };
        let _ = self.eng(move |e| e.record_tidy(report)).await;
        self.queue_careful(end).await;
        let watch = (pending > 0).then_some(Watch {
            date,
            end,
            tidied_from: started,
        });
        self.inner.events.page_ready(summary.clone());
        (TidyStep::Finished(summary), watch)
    }

    /// The items kept in the day before `end` whose memories leave out a number or name they
    /// give are marked to be read again, carefully, sentence by sentence. The reader does it
    /// after the page, only on mains and when nothing else is waiting.
    async fn queue_careful(&self, end: Timestamp) {
        let since = end - CAREFUL_WINDOW;
        let picked = self
            .eng(move |e| {
                let items = e.items(ItemFilter {
                    since: Some(since.to_string()),
                    status: None,
                    limit: None,
                })?;
                // Items already marked (a new mind reads everything again) are that re-read's.
                let marked: std::collections::HashSet<String> =
                    e.rereads(usize::MAX)?.into_iter().collect();
                let names: Vec<String> = e.entities(None)?.into_iter().map(|en| en.name).collect();
                let mut picked = Vec::new();
                for item in items {
                    if picked.len() == CAREFUL_PER_NIGHT {
                        break;
                    }
                    // An item with a question open is left for the person to settle.
                    let read = matches!(
                        item.status,
                        ItemStatus::Understood | ItemStatus::Unconfirmed
                    );
                    let before = item.kept_at.parse::<Timestamp>().is_ok_and(|k| k < end);
                    let Some(text) = item.text.as_deref().filter(|t| !t.trim().is_empty()) else {
                        continue;
                    };
                    if !read || !before || marked.contains(&item.id) {
                        continue;
                    }
                    let memories = e.memories(MemoryFilter {
                        item_id: Some(item.id.clone()),
                        ..MemoryFilter::default()
                    })?;
                    // A memory of it that a later item replaced is history the re-read must
                    // not reopen: such an item is not read again.
                    if memories.len() != item.memory_count as usize {
                        continue;
                    }
                    let said: Vec<String> = memories
                        .iter()
                        .map(|m| {
                            let quotes: Vec<&str> = m
                                .receipts
                                .iter()
                                .filter(|r| r.item_id == item.id)
                                .filter_map(|r| match &r.span {
                                    Some(Span::Quote { text }) => Some(text.as_str()),
                                    _ => None,
                                })
                                .collect();
                            format!(
                                "{} | {} | {} | {}",
                                quotes.join(" | "),
                                m.subject,
                                m.sentence,
                                m.object
                            )
                        })
                        .collect();
                    if !uncovered(text, &said, &names).is_empty()
                        && e.queue_reread(std::slice::from_ref(&item.id))? == 1
                    {
                        picked.push(item.id);
                    }
                }
                Ok(picked)
            })
            .await;
        match picked {
            Ok(ids) if !ids.is_empty() => {
                tracing::info!("{} items will be read again carefully", ids.len());
                self.inner.reader.read_carefully(&ids);
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("could not look for items to read again: {e}"),
        }
    }

    async fn compose(&self, date: Date, end: Timestamp) -> Option<PageView> {
        match self
            .eng(move |e| e.compose_page_until(date, end, Edition::Screen))
            .await
        {
            Ok(page) => Some(page),
            Err(e) => {
                tracing::warn!("could not compose the page: {e}");
                None
            }
        }
    }

    async fn summary(&self, date: Date) -> Option<PageSummary> {
        let pages = self.eng(|e| e.pages()).await.ok()?;
        pages.into_iter().find(|p| p.date == date.to_string())
    }

    /// The mind's headline and dek; the composed page's own (the template) when it cannot.
    async fn write_headline(&self, page: &mut PageView) {
        let Some(mind) = current_mind(&self.inner.slot) else {
            return;
        };
        match tokio::time::timeout(HEADLINE_LIMIT, mind.write_headline(page)).await {
            Ok(Ok((headline, dek))) if !headline.trim().is_empty() => {
                page.headline = headline;
                if !dek.trim().is_empty() {
                    page.dek = dek;
                }
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) => tracing::info!("the headline falls back to the template: {e}"),
            Err(_) => tracing::info!("the headline took too long; using the template"),
        }
    }

    /// Vectors for memories that have none, newest first. Bounded in time, and it gives way to
    /// an ask: a stuck mind must not hold up the page.
    async fn backfill_embeddings(&self) {
        let _ = tokio::time::timeout(BACKFILL_LIMIT, self.backfill_loop()).await;
    }

    async fn backfill_loop(&self) {
        let Some(mind) = current_mind(&self.inner.slot) else {
            return;
        };
        loop {
            if self.inner.reader.is_asking() {
                return;
            }
            let model = self.inner.engine.recall_model();
            let Ok(rows) = self
                .eng(|e| e.memories_without_embedding(BACKFILL_CHUNK))
                .await
            else {
                return;
            };
            if rows.is_empty() {
                return;
            }
            let texts: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
            // Made with the finding model the store names, and marked with that one.
            match tokio::time::timeout(
                EMBED_LIMIT,
                crate::reindex::embed_as(mind.as_ref(), &model, &texts),
            )
            .await
            {
                Ok(Ok(vecs)) if vecs.len() == rows.len() => {
                    let n = rows.len();
                    let pairs: Vec<(String, Vec<f32>)> =
                        rows.into_iter().map(|r| r.0).zip(vecs).collect();
                    let model = model.clone();
                    if self
                        .eng(move |e| e.store_embeddings_from(&model, pairs))
                        .await
                        .is_err()
                    {
                        return;
                    }
                    if n < BACKFILL_CHUNK {
                        return;
                    }
                }
                _ => return,
            }
        }
    }

    /// With no tidy running: a saved page with unread items behind it is written again, once,
    /// when they have all been read and the person has not yet opened it.
    async fn tick_watch(&self, watch: Option<Watch>) -> TidyStep {
        let Some(w) = watch else {
            return TidyStep::Idle;
        };
        let (date, end, from) = (w.date, w.end, w.tidied_from);
        let can = self
            .eng(move |e| e.can_resave_page(date))
            .await
            .unwrap_or(false);
        if !can {
            return TidyStep::Idle;
        }
        let waiting = |this: &Tidy, w: Watch| {
            let this = this.clone();
            async move {
                this.inner.slots.lock().await.watch = Some(w);
                TidyStep::Idle
            }
        };
        let Some(mut page) = self.compose(date, end).await else {
            return waiting(self, w).await;
        };
        if page.pending > 0 {
            return waiting(self, w).await;
        }
        self.write_headline(&mut page).await;
        page.tidied_from = Some(from.to_string());
        page.tidied_to = Some(self.now().to_string());
        let saved = {
            let page = page.clone();
            self.eng(move |e| e.resave_page(&page, end)).await
        };
        match saved {
            Ok(true) => {
                let summary = self.summary(date).await.unwrap_or(PageSummary {
                    date: date.to_string(),
                    page_number: page.page_number,
                    headline: page.headline.clone(),
                    needs_you: page.needs_you.len() as u32,
                });
                self.inner.events.page_ready(summary.clone());
                TidyStep::Resaved(summary)
            }
            _ => TidyStep::Idle,
        }
    }
}
