//! The running mind of the app: the reader that understands what was kept, the tidy that writes
//! the morning page, and the scheduler that decides when. They hold the same mind slot as
//! `AppState.mind`, so whatever fills the slot (the private engine, or a scripted mind in
//! development) is what they read with.
//!
//! [`start`] builds the three and runs them on one cancellation token; [`Services::shutdown`]
//! stops them and returns the item in flight to the queue. The reader and the tidy tell the app
//! through [`AppEvents`]: item changes, tidy progress, notices and the page being ready.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jiff::Timestamp;
use nocturne_contract::{
    EVENT_ITEM, EVENT_PAGE, ItemFilter, ItemStatus, ItemView, Notice, NoticeCode, PageSummary,
    TidyProgress, Tier,
};
use nocturne_intelligence::{Events, Host, Reader, Scheduler, SystemHost, Tidy};
use nocturne_lemonade::{SetupOptions, SupervisorOptions};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio_util::sync::CancellationToken;

use crate::state::{AppState, refresh_status};

pub mod lemonade;
pub use lemonade::{pause_setup, resume_setup, start_setup};

/// How long quitting waits for the reader to give back the item it is reading.
const STOP_TASKS: Duration = Duration::from_secs(2);
/// How long quitting waits for the private engine to stop. It also ends by itself within about
/// six seconds of the app going, so the wait is short.
const QUIT_BUDGET: Duration = Duration::from_secs(5);
/// How long after the engine gives up another start is tried.
const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);
/// Items kept this recently are read again after an upgrade.
const REREAD_DAYS: i64 = 30;

/// Where the private engine comes from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Bundle {
    /// Look in the usual places (see [`lemonade::find_bundle`]). A mind already in the slot (a
    /// scripted one in development) means there is no need.
    #[default]
    Find,
    /// This folder, which holds `lemond`.
    At(PathBuf),
    /// No private engine: the reader uses whatever mind is put in the slot.
    Off,
}

/// What a run of the runtime is made of. The defaults are the product's; tests replace the host
/// (so nothing touches the machine's power or idle state) and the engine.
#[derive(Clone)]
pub struct StartOptions {
    /// The machine's power and idle state. `None` reads the real machine.
    pub host: Option<Arc<dyn Host>>,
    pub bundle: Bundle,
    pub supervisor: SupervisorOptions,
    pub setup: SetupOptions,
    /// How long after the engine gives up another start is tried.
    pub retry_after: Duration,
    /// How long quitting waits for the engine to stop.
    pub quit_budget: Duration,
}

impl Default for StartOptions {
    fn default() -> Self {
        StartOptions {
            host: None,
            bundle: Bundle::Find,
            supervisor: SupervisorOptions::default(),
            setup: SetupOptions::default(),
            retry_after: RETRY_AFTER,
            quit_budget: QUIT_BUDGET,
        }
    }
}

impl StartOptions {
    /// No private engine, and the machine's power and idle state answered by `host`.
    pub fn with_host(host: Arc<dyn Host>) -> Self {
        StartOptions {
            host: Some(host),
            bundle: Bundle::Off,
            ..StartOptions::default()
        }
    }

    /// The engine in `bundle`, and the machine's power and idle state answered by `host`.
    pub fn with_engine(host: Arc<dyn Host>, bundle: impl Into<PathBuf>) -> Self {
        StartOptions {
            bundle: Bundle::At(bundle.into()),
            ..StartOptions::with_host(host)
        }
    }
}

/// The reader, the tidy and the scheduler, running. Held by `AppState`.
pub struct Services {
    pub reader: Reader,
    pub tidy: Tidy,
    pub scheduler: Scheduler,
    token: CancellationToken,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    /// The pause last handed to the scheduler, so it hears about changes only.
    paused: Mutex<Option<Timestamp>>,
    opts: StartOptions,
    lemonade: tokio::sync::Mutex<Option<lemonade::Lemonade>>,
    /// The tier of the mind in the slot, once there is one.
    tier: Mutex<Option<Tier>>,
    retry_pending: AtomicBool,
}

impl Services {
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    /// The tier of the mind in the slot.
    pub fn tier(&self) -> Option<Tier> {
        *self.tier.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Remember the tier of the mind now in the slot; returns the one before it.
    fn set_tier(&self, tier: Tier) -> Option<Tier> {
        self.tier
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replace(tier)
    }

    /// Stop reading and tidying. The item being read goes back to the queue, uncounted.
    pub async fn shutdown(&self) {
        self.token.cancel();
        let tasks = std::mem::take(&mut *self.tasks.lock().unwrap_or_else(|e| e.into_inner()));
        let joined = async {
            for task in tasks {
                let _ = task.await;
            }
        };
        if tokio::time::timeout(STOP_TASKS, joined).await.is_err() {
            tracing::warn!("the reader did not stop in time");
        }
        if let Some(engine) = self.take_engine().await {
            engine.stop(self.opts.quit_budget).await;
        }
    }
}

/// Build the reader, the tidy and the scheduler over the app's engine and mind slot, run them,
/// and stop them when the app exits. Does nothing when there is no engine (the store could not
/// be opened) or when it has already run.
pub fn start<R: Runtime>(app: &AppHandle<R>, opts: StartOptions) -> Option<Arc<Services>> {
    let state = app.state::<AppState>();
    let engine = state.engine().ok()?;
    if state.services().is_some() {
        return None;
    }
    let host = opts.host.clone().unwrap_or_else(|| {
        let host = SystemHost::new();
        host.watch_resume();
        Arc::new(host)
    });
    let events: Arc<dyn Events> = Arc::new(AppEvents { app: app.clone() });
    let slot = state.mind.shared();
    let clock = state.clock.clone();
    let reader = Reader::new(
        engine.clone(),
        slot.clone(),
        host.clone(),
        clock.clone(),
        events.clone(),
    );
    let tidy = Tidy::new(
        engine.clone(),
        reader.clone(),
        slot,
        host.clone(),
        clock.clone(),
        events.clone(),
    );
    let scheduler = Scheduler::new(engine, reader.clone(), tidy.clone(), host, clock, events);
    let token = CancellationToken::new();
    let tasks = vec![
        tauri::async_runtime::spawn({
            let (reader, token) = (reader.clone(), token.clone());
            async move { reader.run(token).await }
        }),
        tauri::async_runtime::spawn({
            let (scheduler, token) = (scheduler.clone(), token.clone());
            async move { scheduler.run(token).await }
        }),
    ];
    let services = Arc::new(Services {
        reader,
        tidy,
        scheduler,
        token,
        tasks: Mutex::new(tasks),
        paused: Mutex::new(None),
        opts,
        lemonade: tokio::sync::Mutex::new(None),
        tier: Mutex::new(None),
        retry_pending: AtomicBool::new(false),
    });
    if !state.install_services(services.clone()) {
        services.token.cancel();
        return None;
    }
    follow_pause(&state);
    tauri::async_runtime::spawn({
        let (app, services) = (app.clone(), services.clone());
        async move { lemonade::launch(&app, &services).await }
    });
    let quitting = services.clone();
    state.on_exit(Box::new(move || {
        tauri::async_runtime::block_on(quitting.shutdown());
    }));
    Some(services)
}

/// Hand the person's pause (kept by the status hub) to the scheduler when it changes.
pub(crate) fn follow_pause(state: &AppState) {
    let Some(services) = state.services() else {
        return;
    };
    let until = state.status.paused_until(state.now());
    {
        let mut seen = services.paused.lock().unwrap_or_else(|e| e.into_inner());
        if *seen == until {
            return;
        }
        *seen = until;
    }
    services.scheduler.set_paused_until(until);
    if until.is_some() {
        tauri::async_runtime::spawn(async move { services.unload_models().await });
    }
}

/// The mind in the slot is now for a different tier than it was. After an upgrade, what was
/// kept in the last thirty days is read again by the better mind, in the background; after a
/// drop to a smaller tier, what could not be read for want of a mind is tried again.
pub async fn tier_changed<R: Runtime>(app: &AppHandle<R>, from: Tier, to: Tier) {
    let state = app.state::<AppState>();
    let Ok(engine) = state.engine() else { return };
    let now = state.now();
    let upgrade = rank(to) > rank(from);
    let queued = crate::error::blocking(move || {
        if upgrade {
            let since = now
                .checked_sub(jiff::SignedDuration::from_hours(24 * REREAD_DAYS))
                .map_err(nocturne_engine::Error::from)?;
            let mut ids = Vec::new();
            for status in [
                ItemStatus::Understood,
                ItemStatus::PartlyUnderstood,
                ItemStatus::NeedsYou,
                ItemStatus::Unconfirmed,
            ] {
                ids.extend(
                    engine
                        .items(ItemFilter {
                            since: Some(since.to_string()),
                            status: Some(status),
                            limit: None,
                        })?
                        .into_iter()
                        .map(|i| i.id),
                );
            }
            engine.queue_reread(&ids)
        } else {
            let waiting = engine.items(ItemFilter {
                status: Some(ItemStatus::CouldntRead),
                ..ItemFilter::default()
            })?;
            let mut retried = 0;
            for item in waiting
                .into_iter()
                .filter(|i| i.reason == Some(nocturne_contract::CouldntReadReason::MindUnavailable))
            {
                engine.retry_item(&item.id)?;
                retried += 1;
            }
            Ok(retried)
        }
    })
    .await;
    match queued {
        Ok(n) => tracing::info!("{n} items will be read again after the change of mind"),
        Err(e) => tracing::warn!("could not queue the re-reading: {:?}", e.code),
    }
    if let Some(services) = state.services() {
        services.reader.wake();
    }
    refresh_status(app).await;
}

fn rank(tier: Tier) -> u8 {
    match tier {
        Tier::Large => 3,
        Tier::Standard => 2,
        Tier::Small => 1,
        Tier::Unsupported => 0,
    }
}

/// What the reader and the tidy tell the app.
struct AppEvents<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> AppEvents<R> {
    fn refresh(&self) {
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            refresh_status(&app).await;
        });
    }
}

impl<R: Runtime> Events for AppEvents<R> {
    fn item_changed(&self, item: &ItemView) {
        if let Err(e) = self.app.emit(EVENT_ITEM, item) {
            tracing::warn!("could not emit an item: {e}");
        }
        self.refresh();
    }

    fn notice(&self, notice: Notice) {
        // An item that could not be read is shown from the engine's own count, so that the
        // notice goes when the item is tried again.
        if notice.code != NoticeCode::CouldntRead {
            self.app.state::<AppState>().status.add_notice(notice);
        }
        self.refresh();
    }

    fn notice_cleared(&self, code: NoticeCode) {
        self.app.state::<AppState>().status.clear_notices_of(code);
        self.refresh();
    }

    fn tidy_progress(&self, progress: Option<TidyProgress>) {
        self.app.state::<AppState>().status.set_tidy(progress);
        self.refresh();
    }

    fn page_ready(&self, page: PageSummary) {
        if let Err(e) = self.app.emit(EVENT_PAGE, &page) {
            tracing::warn!("could not emit the page: {e}");
        }
        self.refresh();
    }
}
