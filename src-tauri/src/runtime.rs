//! The running mind of the app: the reader that understands what was kept, the tidy that writes
//! the morning page, and the scheduler that decides when. They hold the same mind slot as
//! `AppState.mind`, so whatever fills the slot (the private engine, or a scripted mind in
//! development) is what they read with.
//!
//! [`start`] builds the three and runs them on one cancellation token; [`Services::shutdown`]
//! stops them and returns the item in flight to the queue. The reader and the tidy tell the app
//! through [`AppEvents`]: item changes, tidy progress, notices and the page being ready.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use jiff::Timestamp;
use nocturne_contract::TidyProgress;
use nocturne_contract::{EVENT_ITEM, EVENT_PAGE, ItemView, Notice, NoticeCode, PageSummary};
use nocturne_intelligence::{Events, Host, Reader, Scheduler, SystemHost, Tidy};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio_util::sync::CancellationToken;

use crate::state::{AppState, refresh_status};

/// How long quitting waits for the reader to give back the item it is reading.
const STOP_TASKS: Duration = Duration::from_secs(2);

/// What a run of the runtime is made of. The defaults are the product's; tests replace the host
/// (so nothing touches the machine's power or idle state).
#[derive(Clone, Default)]
pub struct StartOptions {
    /// The machine's power and idle state. `None` reads the real machine.
    pub host: Option<Arc<dyn Host>>,
}

impl StartOptions {
    /// The defaults, with the machine's power and idle state answered by `host`.
    pub fn with_host(host: Arc<dyn Host>) -> Self {
        StartOptions { host: Some(host) }
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
}

impl Services {
    pub fn token(&self) -> &CancellationToken {
        &self.token
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
    let host = opts.host.unwrap_or_else(|| {
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
    });
    if !state.install_services(services.clone()) {
        services.token.cancel();
        return None;
    }
    follow_pause(&state);
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
