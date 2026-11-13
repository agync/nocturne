//! The private engine (Lemonade) inside the app: where its bundle is, starting and watching it,
//! setup, and the mind it gives the reader once every part is on disk.
//!
//! Nothing here blocks launch: the engine starts on a background task, `check` looks at what is
//! on disk, and when everything is there the mind goes into the slot. The slot is cleared when
//! the engine gives up (and refilled when a later start works); in between the status says
//! "starting" and the reader waits.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use nocturne_contract::{LedgerKind, Notice, NoticeCode, SetupPhase, Tier};
use nocturne_engine::Mind;
use nocturne_lemonade::{
    Client, Dirs, LemonadeMind, RuntimeError, RuntimeState, Setup, Supervisor, bundle, tier,
};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::watch;

use super::{Bundle, Services};
use crate::error::CommandError;
use crate::state::{AppState, publish_setup, refresh_status, set_mind};

/// The environment variable that names a bundle (a folder with `lemond` in it).
pub const BUNDLE_ENV: &str = "NOCTURNE_LEMOND";
/// The id of the notice shown when the engine keeps failing.
pub const RESTARTS_NOTICE: &str = "restarts";

/// One running engine, with the setup that drives it and the tasks that watch both.
pub(super) struct Lemonade {
    supervisor: Supervisor,
    setup: Arc<Setup>,
    client: Client,
    watchers: Vec<JoinHandle<()>>,
}

impl Lemonade {
    /// Stop watching and stop the engine, asking politely for up to `budget`.
    pub(super) async fn stop(self, budget: Duration) {
        for task in &self.watchers {
            task.abort();
        }
        match tokio::time::timeout(budget, self.supervisor.shutdown()).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!("the private engine did not stop cleanly: {e}"),
            Err(_) => tracing::warn!("the private engine was slow to stop; it ends with the app"),
        }
    }
}

fn platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "aarch64") => "linux-aarch64",
        ("linux", _) => "linux-x86_64",
        ("macos", _) => "macos-aarch64",
        _ => "windows-x86_64",
    }
}

/// Where the bundle is: the folder `NOCTURNE_LEMOND` names (or the folder of the file it
/// names), then `lemonade` among the app's resources, then, in a debug build, the copy
/// `scripts/fetch-lemonade.sh` leaves in the checkout.
pub fn find_bundle<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let from_env = std::env::var_os(BUNDLE_ENV).map(PathBuf::from).map(|p| {
        if p.is_file() {
            p.parent().map(Path::to_path_buf).unwrap_or(p)
        } else {
            p
        }
    });
    let resources = app.path().resource_dir().ok().map(|d| d.join("lemonade"));
    let checkout = cfg!(debug_assertions).then(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vendor/lemonade")
            .join(platform())
    });
    [from_env, resources, checkout]
        .into_iter()
        .flatten()
        .find(|dir| bundle::locate(dir).is_ok())
}

/// Start the private engine and everything that follows from it. Quietly does nothing when
/// there is no bundle (a development checkout that has not fetched one).
pub(super) async fn launch<R: Runtime>(app: &AppHandle<R>, services: &Arc<Services>) {
    let state = app.state::<AppState>();
    let bundle = match &services.opts.bundle {
        Bundle::Off => return,
        Bundle::Find if state.mind.get().is_some() => return,
        Bundle::Find => find_bundle(app),
        Bundle::At(dir) => Some(dir.clone()),
    };
    let Some(bundle) = bundle else {
        tracing::info!("no private engine bundle was found; the mind stays away");
        return;
    };
    let dirs = Dirs::new(bundle, state.data_dir.join("engine"));
    let started = tokio::select! {
        _ = services.token().cancelled() => return,
        started = Supervisor::start(dirs, services.opts.supervisor.clone()) => started,
    };
    match started {
        Ok(supervisor) => adopt(app, services, supervisor).await,
        Err(e) => {
            tracing::warn!("the private engine did not start: {e}");
            gave_up(app, services).await;
        }
    }
}

/// The engine is up: set up its watchers, look at what is on disk and, if all of it is there,
/// fill the slot.
async fn adopt<R: Runtime>(app: &AppHandle<R>, services: &Arc<Services>, supervisor: Supervisor) {
    let state = app.state::<AppState>();
    let ledger = state.engine().ok();
    let setup = Arc::new(Setup::with_options(
        &supervisor,
        supervisor.dirs().models(),
        move |kind: LedgerKind, label: String, bytes: u64| {
            if let Some(engine) = ledger.clone() {
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(e) = engine.record_download(kind, &label, None, bytes as f64) {
                        tracing::warn!("a download could not be written to the ledger: {e}");
                    }
                });
            }
        },
        services.opts.setup.clone(),
    ));
    let client = supervisor.client();
    let watchers = vec![
        tauri::async_runtime::spawn(watch_engine(
            app.clone(),
            services.clone(),
            supervisor.state(),
        )),
        tauri::async_runtime::spawn(watch_setup(
            app.clone(),
            services.clone(),
            setup.view(),
            setup.clone(),
            client.clone(),
        )),
    ];
    let mut slot = services.lemonade.lock().await;
    if services.token().is_cancelled() {
        drop(slot);
        Lemonade {
            supervisor,
            setup,
            client,
            watchers,
        }
        .stop(services.opts.quit_budget)
        .await;
        return;
    }
    *slot = Some(Lemonade {
        supervisor,
        setup: setup.clone(),
        client,
        watchers,
    });
    drop(slot);
    match setup.check().await {
        Ok(view) => publish_setup(app, view),
        Err(e) => tracing::warn!("could not look at what is installed: {e}"),
    }
}

/// Follow the engine's state: starting and restarting show as "starting", a recovered engine
/// is read from again at once, an engine that gave up empties the slot.
async fn watch_engine<R: Runtime>(
    app: AppHandle<R>,
    services: Arc<Services>,
    mut rx: watch::Receiver<RuntimeState>,
) {
    loop {
        let now = rx.borrow_and_update().clone();
        let state = app.state::<AppState>();
        match now {
            RuntimeState::Starting | RuntimeState::Restarting { .. } => {
                state.mind.set_loading(state.mind.get().is_some());
                refresh_status(&app).await;
            }
            RuntimeState::Ready { port, version } => {
                tracing::info!("the private engine {version} is ready on 127.0.0.1:{port}");
                state.mind.set_loading(false);
                state.status.clear_notices_of(NoticeCode::Restarts);
                services.reader.retry_now();
                refresh_status(&app).await;
            }
            RuntimeState::Failed { reason } => {
                tracing::warn!("the private engine gave up: {reason}");
                gave_up(&app, &services).await;
            }
            RuntimeState::Stopped => {}
        }
        tokio::select! {
            _ = services.token().cancelled() => return,
            changed = rx.changed() => if changed.is_err() { return },
        }
    }
}

/// Forward setup's view to the interface, and give the reader its mind when setup is done.
async fn watch_setup<R: Runtime>(
    app: AppHandle<R>,
    services: Arc<Services>,
    mut rx: watch::Receiver<nocturne_contract::SetupView>,
    setup: Arc<Setup>,
    client: Client,
) {
    loop {
        let view = rx.borrow_and_update().clone();
        let done = view.phase == SetupPhase::Done;
        publish_setup(&app, view);
        if done && let Some(tier) = setup.installed() {
            fill(&app, &services, client.clone(), tier).await;
        }
        tokio::select! {
            _ = services.token().cancelled() => return,
            changed = rx.changed() => if changed.is_err() { return },
        }
    }
}

/// Put the mind for `tier` in the slot. A second call for the same tier changes nothing.
async fn fill<R: Runtime>(
    app: &AppHandle<R>,
    services: &Arc<Services>,
    client: Client,
    tier: Tier,
) {
    let state = app.state::<AppState>();
    let previous = services.set_tier(tier);
    if previous == Some(tier) && state.mind.get().is_some() {
        return;
    }
    tracing::info!("the mind is ready ({tier:?})");
    let mind: Arc<dyn Mind> = Arc::new(LemonadeMind::new(client, tier));
    state
        .status
        .set_mind_bytes(tier::components(tier).iter().map(|c| c.bytes as f64).sum());
    set_mind(app, Some(mind)).await;
    services.reader.retry_now();
    if let Some(from) = previous {
        super::tier_changed(app, from, tier).await;
    }
}

/// The engine could not be started or kept: nothing can be read, and a notice says so. Another
/// start is tried after a while.
async fn gave_up<R: Runtime>(app: &AppHandle<R>, services: &Arc<Services>) {
    let state = app.state::<AppState>();
    state.mind.set_loading(false);
    set_mind(app, None).await;
    state.status.add_notice(Notice {
        id: RESTARTS_NOTICE.to_string(),
        code: NoticeCode::Restarts,
        item_id: None,
        eta_seconds: None,
        bytes: None,
    });
    refresh_status(app).await;
    retry_later(app, services);
}

/// Start the engine again after `retry_after`, once at a time.
fn retry_later<R: Runtime>(app: &AppHandle<R>, services: &Arc<Services>) {
    if services
        .retry_pending
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return;
    }
    let (app, services) = (app.clone(), services.clone());
    tauri::async_runtime::spawn(async move {
        tokio::select! {
            _ = services.token().cancelled() => return,
            _ = tokio::time::sleep(services.opts.retry_after) => {}
        }
        services
            .retry_pending
            .store(false, std::sync::atomic::Ordering::SeqCst);
        relaunch(&app, &services).await;
    });
}

/// Replace the engine with a fresh one.
pub(super) async fn relaunch<R: Runtime>(app: &AppHandle<R>, services: &Arc<Services>) {
    let old = services.lemonade.lock().await.take();
    if let Some(old) = old {
        old.stop(services.opts.quit_budget).await;
    }
    launch(app, services).await;
}

impl Services {
    /// Take the engine out to stop it (quitting).
    pub(super) async fn take_engine(&self) -> Option<Lemonade> {
        self.lemonade.lock().await.take()
    }

    /// The process id of the private engine, when it is running.
    pub async fn engine_pid(&self) -> Option<u32> {
        self.lemonade.lock().await.as_ref()?.supervisor.pid()
    }

    pub(super) async fn setup(&self) -> Option<Arc<Setup>> {
        self.lemonade.lock().await.as_ref().map(|l| l.setup.clone())
    }

    /// A client for the private engine, once it is running.
    pub async fn client(&self) -> Option<Client> {
        self.lemonade
            .lock()
            .await
            .as_ref()
            .map(|l| l.client.clone())
    }

    /// Let go of every model the engine has loaded (the person paused).
    pub(super) async fn unload_models(&self) {
        let (Some(client), Some(tier)) = (self.client().await, self.tier()) else {
            return;
        };
        for component in tier::components(tier) {
            // A model that is not loaded is not an error worth hearing about.
            let _ = client.unload(component.model).await;
        }
    }
}

/// Begin setup for `tier` (the person chose it, or agreed to the smaller one).
pub async fn start_setup<R: Runtime>(app: &AppHandle<R>, tier: Tier) -> Result<(), CommandError> {
    let setup = setup_of(app).await?;
    setup.start(tier).await.map_err(runtime_error)
}

pub async fn pause_setup<R: Runtime>(app: &AppHandle<R>) -> Result<(), CommandError> {
    setup_of(app).await?.pause().await.map_err(runtime_error)
}

pub async fn resume_setup<R: Runtime>(app: &AppHandle<R>) -> Result<(), CommandError> {
    setup_of(app).await?.resume().await.map_err(runtime_error)
}

async fn setup_of<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<Setup>, CommandError> {
    let services = app
        .state::<AppState>()
        .services()
        .ok_or_else(|| CommandError::unavailable("setup is not running"))?;
    services
        .setup()
        .await
        .ok_or_else(|| CommandError::unavailable("the private engine is not running"))
}

fn runtime_error(e: RuntimeError) -> CommandError {
    match e {
        RuntimeError::Unavailable => CommandError::unavailable("the private engine is not running"),
        other => CommandError::internal(other),
    }
}
