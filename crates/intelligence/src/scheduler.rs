//! When to tidy. Every 30 seconds the scheduler looks at the clock, the power and the idle
//! time and decides: start the overnight tidy, start a catch-up, or leave it. It also notices
//! the machine having slept (a wall clock that ran ahead of the monotonic one, or a resume
//! signal from the host) and checks for a missed page at launch, on resume and every five
//! minutes.
//!
//! It also runs the journal's looking back, once the app gives it a gate and the mind slot
//! ([`Scheduler::set_reflecting`]): the reflection for the day that ended, then any recap due,
//! under the night tidy's rules for idle time and power. The gate and the mind cover the day's
//! reflection only: a recap is put together from what was kept, so it is due on every computer,
//! with the gate closed and with no mind. A tidy that is about to write its page writes what is
//! due first, so the page can carry a line about it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use nocturne_contract::{ItemFilter, NoticeCode, Period, PeriodKind};
use nocturne_engine::{Clock, Engine, Mind, MindError};
use tokio_util::sync::CancellationToken;

use crate::events::{Events, notice};
use crate::host::{Host, probe};
use crate::reader::Reader;
use crate::reflect::{self, ReflectError, Wrote};
use crate::schedule::{coming, parse_hm, target_date};
use crate::tidy::{CATCH_UP_LIMIT, LOW_BATTERY, Tidy, TidyKind, TidyStep};
use crate::{MindSlot, current_mind};

/// How often the scheduler looks.
pub const TICK: Duration = Duration::from_secs(30);
/// How long the person must have been away before the night tidy starts.
const IDLE_NEEDED: Duration = Duration::from_secs(10 * 60);
/// How often a missed page is looked for, apart from launch and resume.
const CATCH_UP_EVERY: SignedDuration = SignedDuration::from_mins(5);
/// A wall clock this far ahead of the monotonic one means the machine slept.
const SLEEP_JUMP: SignedDuration = SignedDuration::from_secs(60);
/// A missed reflection is still written this many days back.
const REFLECT_BACK_DAYS: u32 = 7;
/// One reflection may take this long.
const WRITE_LIMIT: Duration = Duration::from_secs(10 * 60);
/// What is due before a page is written takes this long in all, at the most.
const BEFORE_PAGE_LIMIT: Duration = Duration::from_secs(30 * 60);
/// How often what is due is looked for, apart from launch and resume.
const REFLECT_CHECK_EVERY: SignedDuration = SignedDuration::from_mins(5);
/// A write that failed waits this long before it is tried again.
const RETRY_FAILED: SignedDuration = SignedDuration::from_mins(10);
/// A day that was listed, not written (nothing of it to write about), is not asked for again
/// for this long.
const RETRY_LISTED: SignedDuration = SignedDuration::from_hours(12);

/// What must still hold between one period and the next of a run of writing: the rules the run
/// started under.
#[derive(Clone, Copy)]
struct Rules {
    /// In the small hours before the page: any battery blocks, not just a low one.
    in_window: bool,
    /// The person asked for it now.
    anyway: bool,
    /// Nobody is about. A run inside the tidy has no such rule.
    need_away: bool,
}

/// Whether this size of computer writes reflections now (the journal check, or the person's own
/// model, or a test's override).
pub type Gate = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Clone)]
struct Reflecting {
    slot: MindSlot,
    gate: Gate,
}

/// What one `tick` did.
#[derive(Debug, Clone, PartialEq)]
pub enum SchedStep {
    /// Paused by the person.
    Paused,
    /// A tidy is running, or just moved on.
    Tidy(TidyStep),
    /// A tidy started.
    Started(TidyKind),
    /// The journal's looking back was written for these periods, newest first.
    Reflected(Vec<Period>),
    /// A tidy is due but the battery says wait.
    WaitingForPower,
    /// Nothing is due.
    Idle,
}

#[derive(Default)]
struct State {
    last_wall: Option<(Timestamp, tokio::time::Instant)>,
    resumes_seen: u64,
    /// Launch and resume ask for a catch-up check at the next tick.
    catch_up_due: bool,
    last_catch_up_check: Option<Timestamp>,
    paused_until: Option<Timestamp>,
    battery_notice: bool,
    started: bool,
    /// Launch and resume ask for the journal's due writing to be looked for at the next tick.
    reflect_due: bool,
    /// Whether the mind that writes a day's reflection was there at the last look: its coming
    /// back is looked at once, not at the next five-minute check.
    reflect_mind_there: bool,
    last_reflect_check: Option<Timestamp>,
    /// Periods not to try again until the time.
    reflect_wait: HashMap<Period, Timestamp>,
    writing: Option<Period>,
    shutdown: Option<CancellationToken>,
}

struct Inner {
    engine: Engine,
    reader: Reader,
    tidy: Tidy,
    host: Arc<dyn Host>,
    clock: Arc<dyn Clock>,
    events: Arc<dyn Events>,
    state: Mutex<State>,
    reflecting: Mutex<Option<Reflecting>>,
}

/// Decides when the tidy runs. Cheap to clone.
#[derive(Clone)]
pub struct Scheduler {
    inner: Arc<Inner>,
}

impl Scheduler {
    pub fn new(
        engine: Engine,
        reader: Reader,
        tidy: Tidy,
        host: Arc<dyn Host>,
        clock: Arc<dyn Clock>,
        events: Arc<dyn Events>,
    ) -> Scheduler {
        Scheduler {
            inner: Arc::new(Inner {
                engine,
                reader,
                tidy,
                host,
                clock,
                events,
                state: Mutex::new(State {
                    catch_up_due: true,
                    reflect_due: true,
                    ..State::default()
                }),
                reflecting: Mutex::new(None),
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn tidy(&self) -> &Tidy {
        &self.inner.tidy
    }

    pub fn reader(&self) -> &Reader {
        &self.inner.reader
    }

    /// Let the scheduler write the journal's reflections and recaps with the mind in `slot`, while
    /// `gate` says this computer may. Without it, nothing is written.
    pub fn set_reflecting(&self, slot: MindSlot, gate: Gate) {
        *self
            .inner
            .reflecting
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(Reflecting { slot, gate });
        let weak: Weak<Inner> = Arc::downgrade(&self.inner);
        self.inner.tidy.set_before_page(Arc::new(move || {
            let weak = weak.clone();
            Box::pin(async move {
                if let Some(inner) = weak.upgrade() {
                    Scheduler { inner }.reflect_before_page().await;
                }
            })
        }));
    }

    /// Whether the scheduler is writing the reflection or recap for `period` now.
    pub fn writing(&self, period: &Period) -> bool {
        self.state().writing.as_ref() == Some(period)
    }

    /// The person's pause: the reader and the tidy stop until `until` (`None` resumes). The
    /// app unloads the models when it pauses.
    pub fn set_paused_until(&self, until: Option<Timestamp>) {
        self.state().paused_until = until;
        self.inner.reader.set_paused(until.is_some());
    }

    async fn eng<T, F>(&self, f: F) -> nocturne_engine::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Engine) -> nocturne_engine::Result<T> + Send + 'static,
    {
        let engine = self.inner.engine.clone();
        tokio::task::spawn_blocking(move || f(&engine))
            .await
            .unwrap_or_else(|_| {
                Err(nocturne_engine::Error::Invalid(
                    "a worker thread failed".into(),
                ))
            })
    }

    /// Has the machine slept since the last tick?
    async fn woke_up(&self, now: Timestamp) -> bool {
        let count = probe(&self.inner.host, |h| Some(h.resume_count()))
            .await
            .unwrap_or(0);
        let mono = tokio::time::Instant::now();
        let mut st = self.state();
        let mut woke = count > st.resumes_seen;
        st.resumes_seen = count;
        if let Some((wall, mono_then)) = st.last_wall {
            let wall_elapsed = now.duration_since(wall);
            let mono_elapsed = SignedDuration::try_from(mono.duration_since(mono_then))
                .unwrap_or(SignedDuration::ZERO);
            woke |= wall_elapsed - mono_elapsed > SLEEP_JUMP;
        }
        st.last_wall = Some((now, mono));
        woke
    }

    /// Look once and act.
    pub async fn tick(&self) -> SchedStep {
        let now = self.inner.clock.now();
        if self.woke_up(now).await {
            let mut st = self.state();
            st.catch_up_due = true;
            st.reflect_due = true;
            drop(st);
            // The mind may have been reloaded under us: try again now.
            self.inner.reader.retry_now();
        }
        let (ended, paused) = {
            let mut st = self.state();
            match st.paused_until {
                Some(until) if now >= until => {
                    st.paused_until = None;
                    (true, false)
                }
                Some(_) => (false, true),
                None => (false, false),
            }
        };
        if ended {
            self.inner.reader.set_paused(false);
        }
        if paused {
            self.inner.tidy.hold_off().await;
            return SchedStep::Paused;
        }

        // A tidy that is running moves on; the request to tidy anyway is seen by it.
        if self.inner.tidy.running().await {
            let step = self.inner.tidy.tick().await;
            return SchedStep::Tidy(step);
        }
        let anyway = self.inner.tidy.take_anyway();
        let step = self.consider_start(now, anyway).await;
        if matches!(step, SchedStep::Idle) {
            // Possibly a saved page waiting for its backlog to drain.
            let t = self.inner.tidy.tick().await;
            if t != TidyStep::Idle {
                return SchedStep::Tidy(t);
            }
            if let Some(step) = self.consider_reflect(now, anyway).await {
                return step;
            }
        }
        step
    }

    async fn consider_start(&self, now: Timestamp, anyway: bool) -> SchedStep {
        let Ok(settings) = self.eng(|e| e.settings()).await else {
            return SchedStep::Idle;
        };
        let hm = parse_hm(&settings.page_time);
        let tz = self.inner.clock.tz();
        let (Some(coming), Some(target)) = (coming(now, hm, &tz), target_date(now, hm, &tz)) else {
            return SchedStep::Idle;
        };
        let ac = probe(&self.inner.host, |h| h.on_ac_power()).await;
        let percent = probe(&self.inner.host, |h| h.battery_percent()).await;
        let low = ac == Some(false) && percent.is_some_and(|p| p < LOW_BATTERY);

        // Catch-up: a page for this morning has not been written.
        let due = {
            let mut st = self.state();
            let due = st.catch_up_due
                || st
                    .last_catch_up_check
                    .is_none_or(|t| now.duration_since(t) >= CATCH_UP_EVERY);
            if due {
                st.last_catch_up_check = Some(now);
            }
            due
        };
        if due {
            let missed = self.page_missed(target, hm).await;
            if missed {
                // Still due until it starts: a low battery only postpones it.
                if low && !anyway {
                    return self.battery_wait();
                }
                self.state().catch_up_due = false;
                return self.begin(TidyKind::CatchUp, None, anyway).await;
            }
            self.state().catch_up_due = false;
        }

        // The night tidy: inside the window, on mains, with nobody about, no page yet.
        if now >= coming.opens && now < coming.instant {
            let saved = self
                .eng(move |e| Ok(e.page(Some(coming.date))?.is_some()))
                .await
                .unwrap_or(true);
            if saved || !self.anything_to_write(now).await {
                return SchedStep::Idle;
            }
            // Unknown idle time counts as away: this is the small hours.
            let away = probe(&self.inner.host, |h| h.idle_for())
                .await
                .is_none_or(|d| d >= IDLE_NEEDED);
            if !away && !anyway {
                return SchedStep::Idle;
            }
            if ac == Some(false) && !anyway {
                return self.battery_wait();
            }
            return self.begin(TidyKind::Night, Some(coming.date), anyway).await;
        }
        self.clear_battery_notice();
        SchedStep::Idle
    }

    fn battery_wait(&self) -> SchedStep {
        let first = !std::mem::replace(&mut self.state().battery_notice, true);
        if first {
            self.inner
                .events
                .notice(notice(NoticeCode::TidyPausedBattery, None));
        }
        SchedStep::WaitingForPower
    }

    fn clear_battery_notice(&self) {
        if std::mem::take(&mut self.state().battery_notice) {
            self.inner
                .events
                .notice_cleared(NoticeCode::TidyPausedBattery);
        }
    }

    /// No page dated on or after `target` is saved, and there is something to write one about.
    async fn page_missed(&self, target: jiff::civil::Date, hm: (i8, i8)) -> bool {
        let tz = self.inner.clock.tz();
        let Ok(by) = nocturne_engine::day::page_instant(target, hm, &tz) else {
            return false;
        };
        let saved = self
            .eng(move |e| Ok(e.pages()?.first().map(|p| p.date.clone())))
            .await
            .unwrap_or(None);
        if saved.is_some_and(|d| d >= target.to_string()) {
            return false;
        }
        self.anything_to_write(by).await
    }

    /// Once a page exists, any later page is worth writing. The very first waits for something
    /// kept before `before`: a fresh install writes no empty page.
    async fn anything_to_write(&self, before: Timestamp) -> bool {
        self.eng(move |e| {
            if !e.pages()?.is_empty() {
                return Ok(true);
            }
            Ok(e.items(ItemFilter {
                since: None,
                status: None,
                limit: None,
            })?
            .iter()
            .any(|i| i.kept_at.parse::<Timestamp>().is_ok_and(|k| k < before)))
        })
        .await
        .unwrap_or(false)
    }

    async fn begin(
        &self,
        kind: TidyKind,
        date: Option<jiff::civil::Date>,
        anyway: bool,
    ) -> SchedStep {
        let date = match date {
            Some(d) => Ok(d),
            None => self.eng(|e| e.next_page_date()).await,
        };
        let Ok(date) = date else {
            return SchedStep::Idle;
        };
        self.clear_battery_notice();
        match self.inner.tidy.start(kind, date, anyway).await {
            Ok(()) => SchedStep::Started(kind),
            Err(e) => {
                tracing::warn!("could not start the tidy: {e}");
                SchedStep::Idle
            }
        }
    }

    /// Whether the app has given the scheduler a gate and the mind slot.
    fn reflecting_set(&self) -> bool {
        self.inner
            .reflecting
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// The mind that writes a day's reflection, when this computer may and the mind is there.
    /// A recap needs neither.
    fn reflect_mind(&self) -> Option<Arc<dyn Mind>> {
        let cfg = self
            .inner
            .reflecting
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()?;
        let mind = current_mind(&cfg.slot)?;
        (cfg.gate)().then_some(mind)
    }

    fn paused(&self) -> bool {
        self.state().paused_until.is_some()
    }

    /// What is due and not waiting, newest first. Without a mind, only recaps: a day waits for
    /// one.
    async fn reflections_due(&self, now: Timestamp, with_mind: bool) -> Vec<Period> {
        let due = self
            .eng(|e| e.reflections_due(REFLECT_BACK_DAYS))
            .await
            .unwrap_or_default();
        let mut st = self.state();
        st.reflect_wait.retain(|_, until| *until > now);
        due.into_iter()
            .filter(|p| {
                (with_mind || p.kind != PeriodKind::Day) && !st.reflect_wait.contains_key(p)
            })
            .collect()
    }

    /// The journal's looking back, on its own: the tidy has nothing to start. Written under the
    /// night tidy's rules in its window (the small hours, on mains, with nobody about), and after
    /// it, as in the catch-up, with nobody about and a battery above the tidy's limit.
    async fn consider_reflect(&self, now: Timestamp, anyway: bool) -> Option<SchedStep> {
        if !self.reflecting_set() {
            return None;
        }
        let mind = self.reflect_mind();
        {
            let mut st = self.state();
            let came_back = mind.is_some() && !std::mem::replace(&mut st.reflect_mind_there, true);
            if mind.is_none() {
                st.reflect_mind_there = false;
            }
            let flagged = std::mem::take(&mut st.reflect_due) || came_back;
            let stale = st
                .last_reflect_check
                .is_none_or(|t| now.duration_since(t) >= REFLECT_CHECK_EVERY);
            if !(flagged || stale) {
                return None;
            }
        }
        let settings = self.eng(|e| e.settings()).await.ok()?;
        let tz = self.inner.clock.tz();
        let next = coming(now, parse_hm(&settings.page_time), &tz)?;
        let in_window = now >= next.opens && now < next.instant;
        let rules = Rules {
            in_window,
            anyway,
            need_away: true,
        };
        let ac = probe(&self.inner.host, |h| h.on_ac_power()).await;
        if !self.may_write(rules).await {
            // Looked at again at the next tick, before anything is asked of the store.
            self.state().reflect_due = true;
            return None;
        }
        let due = self.reflections_due(now, mind.is_some()).await;
        self.state().last_reflect_check = Some(now);
        if due.is_empty() {
            return None;
        }
        let _awake = if ac == Some(false) {
            None
        } else {
            probe(&self.inner.host, |h| h.inhibit_sleep()).await
        };
        let until = if in_window {
            next.instant
        } else {
            now + SignedDuration::try_from(CATCH_UP_LIMIT).unwrap_or_default()
        };
        let written = self.write_due(mind.as_ref(), due, Some(until), rules).await;
        (!written.is_empty()).then_some(SchedStep::Reflected(written))
    }

    /// The tidy has read its backlog and is about to write the page: write what is due first.
    async fn reflect_before_page(&self) {
        if !self.reflecting_set() {
            return;
        }
        let mind = self.reflect_mind();
        let rules = Rules {
            in_window: false,
            anyway: false,
            need_away: false,
        };
        if !self.may_write(rules).await {
            return;
        }
        let now = self.inner.clock.now();
        let due = self.reflections_due(now, mind.is_some()).await;
        if due.is_empty() {
            return;
        }
        let until = now + SignedDuration::try_from(BEFORE_PAGE_LIMIT).unwrap_or_default();
        self.write_due(mind.as_ref(), due, Some(until), rules).await;
    }

    /// Whether the battery and, when `rules` ask for it, the idle time allow writing.
    async fn may_write(&self, rules: Rules) -> bool {
        if rules.anyway {
            return true;
        }
        let ac = probe(&self.inner.host, |h| h.on_ac_power()).await;
        let percent = probe(&self.inner.host, |h| h.battery_percent()).await;
        let blocked = if rules.in_window {
            ac == Some(false)
        } else {
            ac == Some(false) && percent.is_some_and(|p| p < LOW_BATTERY)
        };
        if blocked {
            return false;
        }
        !rules.need_away
            || probe(&self.inner.host, |h| h.idle_for())
                .await
                .is_none_or(|d| d >= IDLE_NEEDED)
    }

    /// Write each period in turn, stopping when the person pauses, the app quits, `until` has
    /// passed, or `rules` no longer hold. Returns what was written. A day needs `mind`; when it
    /// is not there, or fails to answer, the days left wait and the recaps are still put
    /// together.
    async fn write_due(
        &self,
        mind: Option<&Arc<dyn Mind>>,
        due: Vec<Period>,
        until: Option<Timestamp>,
        rules: Rules,
    ) -> Vec<Period> {
        // The reader reads nothing while the mind writes: it serves one request at a time.
        let _reader_waits = (mind.is_some() && due.iter().any(|p| p.kind == PeriodKind::Day))
            .then(|| self.inner.reader.ask_begin());
        let shutdown = self.state().shutdown.clone();
        let mut mind = mind.cloned();
        let mut written = Vec::new();
        for (n, period) in due.into_iter().enumerate() {
            if self.paused()
                || shutdown.as_ref().is_some_and(|t| t.is_cancelled())
                || until.is_some_and(|u| self.inner.clock.now() >= u)
                || (n > 0 && !self.may_write(rules).await)
            {
                break;
            }
            let day = period.kind == PeriodKind::Day;
            if day && mind.is_none() {
                continue;
            }
            self.state().writing = Some(period.clone());
            let outcome = {
                let write = tokio::time::timeout(WRITE_LIMIT, async {
                    let when = nocturne_contract::WrittenWhen::Overnight;
                    match (&mind, day) {
                        (Some(m), true) => {
                            reflect::write(&self.inner.engine, m.as_ref(), &period, when).await
                        }
                        _ => {
                            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
                            reflect::write_recap(&self.inner.engine, &period, when, stop).await
                        }
                    }
                });
                match &shutdown {
                    Some(t) => tokio::select! {
                        r = write => Some(r),
                        () = t.cancelled() => None,
                    },
                    None => Some(write.await),
                }
            };
            self.state().writing = None;
            let now = self.inner.clock.now();
            let wait = |this: &Self, how_long: SignedDuration| {
                this.state()
                    .reflect_wait
                    .insert(period.clone(), now + how_long);
            };
            match outcome {
                None => break,
                Some(Ok(Ok(Wrote::Day(_) | Wrote::Recap(_)))) => written.push(period.clone()),
                Some(Ok(Ok(Wrote::Listed))) => wait(self, RETRY_LISTED),
                Some(Ok(Err(ReflectError::Mind(MindError::Unavailable | MindError::Busy)))) => {
                    mind = None;
                }
                Some(Ok(Err(ReflectError::Mind(MindError::Unsupported(_))))) => {
                    wait(self, RETRY_LISTED);
                    mind = None;
                }
                Some(Ok(Err(e))) => {
                    tracing::warn!("could not look back on a period: {e}");
                    wait(self, RETRY_FAILED);
                }
                Some(Err(_)) => {
                    tracing::warn!("looking back on a period took too long");
                    wait(self, RETRY_FAILED);
                }
            }
        }
        written
    }

    /// Tick every 30 seconds until told to stop.
    pub async fn run(&self, shutdown: CancellationToken) {
        {
            let mut st = self.state();
            if std::mem::replace(&mut st.started, true) {
                return;
            }
            st.shutdown = Some(shutdown.clone());
        }
        while !shutdown.is_cancelled() {
            self.tick().await;
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(TICK) => {}
            }
        }
    }
}
