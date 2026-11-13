//! When to tidy. Every 30 seconds the scheduler looks at the clock, the power and the idle
//! time and decides: start the overnight tidy, start a catch-up, or leave it. It also notices
//! the machine having slept (a wall clock that ran ahead of the monotonic one, or a resume
//! signal from the host) and checks for a missed page at launch, on resume and every five
//! minutes.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use nocturne_contract::{ItemFilter, NoticeCode};
use nocturne_engine::{Clock, Engine};
use tokio_util::sync::CancellationToken;

use crate::events::{Events, notice};
use crate::host::{Host, probe};
use crate::reader::Reader;
use crate::schedule::{coming, parse_hm, target_date};
use crate::tidy::{LOW_BATTERY, Tidy, TidyKind, TidyStep};

/// How often the scheduler looks.
pub const TICK: Duration = Duration::from_secs(30);
/// How long the person must have been away before the night tidy starts.
const IDLE_NEEDED: Duration = Duration::from_secs(10 * 60);
/// How often a missed page is looked for, apart from launch and resume.
const CATCH_UP_EVERY: SignedDuration = SignedDuration::from_mins(5);
/// A wall clock this far ahead of the monotonic one means the machine slept.
const SLEEP_JUMP: SignedDuration = SignedDuration::from_secs(60);

/// What one `tick` did.
#[derive(Debug, Clone, PartialEq)]
pub enum SchedStep {
    /// Paused by the person.
    Paused,
    /// A tidy is running, or just moved on.
    Tidy(TidyStep),
    /// A tidy started.
    Started(TidyKind),
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
}

struct Inner {
    engine: Engine,
    reader: Reader,
    tidy: Tidy,
    host: Arc<dyn Host>,
    clock: Arc<dyn Clock>,
    events: Arc<dyn Events>,
    state: Mutex<State>,
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
                    ..State::default()
                }),
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
            if saved {
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
        self.eng(move |e| {
            let pages = e.pages()?;
            if pages.first().is_some_and(|p| p.date >= target.to_string()) {
                return Ok(false);
            }
            if !pages.is_empty() {
                return Ok(true);
            }
            // The very first page waits for something kept before that page time.
            let by = nocturne_engine::day::page_instant(target, hm, &tz)?;
            Ok(e.items(ItemFilter {
                since: None,
                status: None,
                limit: None,
            })?
            .iter()
            .any(|i| i.kept_at.parse::<Timestamp>().is_ok_and(|k| k < by)))
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

    /// Tick every 30 seconds until told to stop.
    pub async fn run(&self, shutdown: CancellationToken) {
        {
            let mut st = self.state();
            if std::mem::replace(&mut st.started, true) {
                return;
            }
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
