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

use nocturne_contract::{LedgerKind, Notice, NoticeCode, SetupError, SetupPhase, Tier};
use nocturne_engine::Mind;
use nocturne_lemonade::client::SystemInfo;
use nocturne_lemonade::{
    Admin, Client, Control, Dirs, LemonadeMind, RuntimeError, RuntimeState, Setup, Supervisor,
    bundle, fit, residency, selection, tier,
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

/// How long past the polite budget a stop may take: SIGTERM, half a second, then SIGKILL. With
/// the reader's half second and the one-second budget, quitting takes at most two and a half.
const STOP_AFTER_BUDGET: Duration = Duration::from_secs(1);

/// What is known of the engine for the advanced details. Empty while there is no engine.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EngineDetails {
    pub version: Option<String>,
    pub port: Option<u16>,
    pub uptime_seconds: Option<u32>,
    pub restarts: u32,
}

/// One running engine, with the setup that drives it and the tasks that watch both.
pub(super) struct Lemonade {
    supervisor: Supervisor,
    setup: Arc<Setup>,
    client: Client,
    admin: Admin,
    watchers: Vec<JoinHandle<()>>,
}

impl Lemonade {
    /// Stop watching and stop the engine: ask politely for up to `budget`, then end its whole
    /// process group (SIGTERM, then SIGKILL).
    pub(super) async fn stop(self, budget: Duration) {
        for task in &self.watchers {
            task.abort();
        }
        let limit = budget + STOP_AFTER_BUDGET;
        match tokio::time::timeout(limit, self.supervisor.shutdown_within(budget)).await {
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
    let route = crate::online::route_at_launch(app);
    let mut start = tokio::spawn(Supervisor::start_with(
        dirs,
        services.opts.supervisor.clone(),
        route,
    ));
    let started = tokio::select! {
        _ = services.token().cancelled() => {
            // Quitting while the engine is still starting: it is stopped as soon as it is up,
            // and if the app is gone first its watchdog ends it.
            tauri::async_runtime::spawn(async move {
                if let Ok(Ok(supervisor)) = start.await {
                    let _ = supervisor.shutdown().await;
                }
            });
            return;
        }
        started = &mut start => started.unwrap_or_else(|e| Err(RuntimeError::Start(e.to_string()))),
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
    let admin = supervisor.admin();
    crate::models::on_engine(app, supervisor.control());
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
            admin.clone(),
        )),
    ];
    let mut slot = services.lemonade.lock().await;
    if services.token().is_cancelled() {
        drop(slot);
        Lemonade {
            supervisor,
            setup,
            client,
            admin,
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
        admin,
        watchers,
    });
    drop(slot);
    match setup.check().await {
        Ok(view) => publish_setup(app, view),
        Err(e) => tracing::warn!("could not look at what is installed: {e}"),
    }
    if let Some(tier) = setup.take_dropped() {
        super::change_unfinished(app, services, tier);
        refresh_status(app).await;
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
    admin: Admin,
) {
    let mut last = None;
    loop {
        let view = rx.borrow_and_update().clone();
        let done = view.phase == SetupPhase::Done;
        if last.replace(view.phase) != Some(view.phase) {
            tracing::info!("setup is {:?}", view.phase);
        }
        publish_setup(&app, view);
        if done && let Some(tier) = setup.installed() {
            fill(&app, &services, client.clone(), &admin, tier).await;
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
    admin: &Admin,
    tier: Tier,
) {
    let state = app.state::<AppState>();
    let previous = services.set_tier(tier);
    if previous == Some(tier) && state.mind.get().is_some() {
        return;
    }
    tracing::info!("the mind is ready ({tier:?})");
    // On a first setup the engine was started before the tier was known.
    let args =
        serde_json::json!({"llamacpp": {"args": nocturne_lemonade::config::llamacpp_args(tier)}});
    if let Err(e) = admin.set(&args).await {
        tracing::warn!("could not set the engine's cache sizes: {e}");
    }
    let changed = previous.is_some_and(|from| from != tier);
    if changed {
        super::memory_settled(&state, services);
    }
    put_mind(app, client, tier).await;
    services.reader.retry_now();
    match previous {
        Some(from) if from != tier => super::tier_changed(app, from, tier).await,
        // The same mind is back after the engine was away: a memory problem has not gone away
        // with it, but what could not be read in the meantime is tried again.
        _ => super::mind_recovered(app).await,
    }
    crate::models::on_mind_ready(app);
}

/// A mind for the models chosen for each job on `tier`, in the slot, with the finding model
/// and the models that must run alone set to match.
async fn put_mind<R: Runtime>(app: &AppHandle<R>, client: Client, tier: Tier) {
    let state = app.state::<AppState>();
    let config_dir = crate::models::config_dir(&state.data_dir);
    crate::models::repair_after_swap(app, &config_dir);
    let chosen = selection::Selection {
        tier,
        ..selection::read(&config_dir)
    };
    let picks = selection::picks(&chosen);
    let ram = crate::models::ram_bytes(app);
    let budget = fit::for_host(&SystemInfo::default(), ram, u64::MAX);
    client.set_alone(residency::alone_for(&chosen, &budget));
    let bytes: f64 = [&picks.mind, &picks.speech, &picks.voice, &picks.recall]
        .into_iter()
        .map(|m| model_bytes(tier, m) as f64)
        .sum();
    let recall = picks.recall.clone();
    let mind: Arc<dyn Mind> = Arc::new(LemonadeMind::new(client, picks, tier));
    state.status.set_mind_bytes(bytes);
    set_mind(app, Some(mind)).await;
    // Named after the mind is in: a vector the old mind was still making is then marked with
    // the old name, and the backfill makes it again with the new one.
    if let Ok(engine) = state.engine()
        && !recall.is_empty()
    {
        engine.set_recall_model(&recall);
    }
}

/// The download size of `model`: the tier's own figure, else the table's memory less what the
/// engine adds, else nothing known.
fn model_bytes(tier: Tier, model: &str) -> u64 {
    tier::components(tier)
        .into_iter()
        .chain(
            [Tier::Large, Tier::Standard, Tier::Small, Tier::Tiny]
                .into_iter()
                .flat_map(tier::components),
        )
        .find(|c| c.model == model)
        .map_or(0, |c| c.bytes)
}

/// The person changed a model: the mind in the slot is rebuilt from the selection on disk. Does
/// nothing while there is no mind (setup is not done, or the engine is away).
pub async fn refill_mind<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let Some(services) = state.services() else {
        return;
    };
    let (Some(client), Some(tier)) = (services.client().await, services.tier()) else {
        return;
    };
    if state.mind.get().is_none() {
        return;
    }
    put_mind(app, client, tier).await;
    services.reader.retry_now();
}

/// The engine could not be started or kept: nothing can be read, and a notice says so. Another
/// start is tried after a while.
async fn gave_up<R: Runtime>(app: &AppHandle<R>, services: &Arc<Services>) {
    let state = app.state::<AppState>();
    state.mind.set_loading(false);
    set_mind(app, None).await;
    // Setup is not "done" while there is nothing to read with.
    let mut view = state.get_setup();
    view.phase = SetupPhase::Failed;
    view.error = Some(SetupError::EngineFailed);
    publish_setup(app, view);
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

/// Replace an engine that has given up (or is missing) with a fresh one. Does nothing while
/// another replacement is under way, so the timed retry and "Try again" cannot start two engines.
pub(super) async fn relaunch<R: Runtime>(app: &AppHandle<R>, services: &Arc<Services>) {
    if services
        .relaunching
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return;
    }
    // A healthy engine is left alone: both the timed retry and "Try again" can arrive after
    // another attempt has already put one back.
    if services.engine_needs_replacing().await {
        let old = services.lemonade.lock().await.take();
        if let Some(old) = old {
            old.stop(services.opts.quit_budget).await;
        }
        launch(app, services).await;
    }
    services
        .relaunching
        .store(false, std::sync::atomic::Ordering::SeqCst);
}

/// "Try again" on the notice about restarts: start the engine afresh now instead of waiting for
/// the next timed attempt.
pub async fn retry_engine<R: Runtime>(app: &AppHandle<R>) -> Result<(), CommandError> {
    let services = app
        .state::<AppState>()
        .services()
        .ok_or_else(|| CommandError::unavailable("nothing is running yet"))?;
    relaunch(app, &services).await;
    Ok(())
}

impl Services {
    /// True when there is no engine, or the one there has given up.
    async fn engine_needs_replacing(&self) -> bool {
        match &*self.lemonade.lock().await {
            None => true,
            Some(engine) => matches!(
                *engine.supervisor.state().borrow(),
                RuntimeState::Failed { .. }
            ),
        }
    }

    /// Take the engine out to stop it (quitting).
    pub(super) async fn take_engine(&self) -> Option<Lemonade> {
        self.lemonade.lock().await.take()
    }

    /// The process id of the private engine, when it is running.
    pub async fn engine_pid(&self) -> Option<u32> {
        self.lemonade.lock().await.as_ref()?.supervisor.pid()
    }

    /// What the advanced details say about the private engine.
    pub async fn engine_details(&self) -> EngineDetails {
        let slot = self.lemonade.lock().await;
        let Some(engine) = slot.as_ref() else {
            return EngineDetails::default();
        };
        let ready = match &*engine.supervisor.state().borrow() {
            RuntimeState::Ready { port, version } => Some((version.clone(), *port)),
            _ => None,
        };
        // Uptime belongs to a running engine: nothing is reported for one that is starting,
        // restarting or has given up.
        let uptime_seconds = ready.as_ref().and_then(|_| {
            engine
                .supervisor
                .uptime()
                .map(|d| u32::try_from(d.as_secs()).unwrap_or(u32::MAX))
        });
        EngineDetails {
            version: ready.as_ref().map(|(v, _)| v.clone()),
            port: ready.map(|(_, p)| p),
            uptime_seconds,
            restarts: engine.supervisor.restarts(),
        }
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

    /// The supervisor's control handle, once the engine is running, for the online and model
    /// hubs.
    pub async fn engine_control(&self) -> Option<Control> {
        self.lemonade
            .lock()
            .await
            .as_ref()
            .map(|l| l.supervisor.control())
    }

    /// The engine's admin endpoints, once it is running.
    pub async fn admin(&self) -> Option<Admin> {
        self.lemonade.lock().await.as_ref().map(|l| l.admin.clone())
    }

    /// Let go of every model the engine has loaded (the person paused).
    pub(super) async fn unload_models(&self) {
        let (Some(client), Some(tier)) = (self.client().await, self.tier()) else {
            return;
        };
        let mut models: Vec<String> = tier::components(tier)
            .into_iter()
            .map(|c| c.model.to_string())
            .collect();
        if let Some(lemonade) = self.lemonade.lock().await.as_ref() {
            let picks = selection::picks(&selection::read(&lemonade.supervisor.dirs().config()));
            models.extend([picks.mind, picks.speech, picks.voice, picks.recall]);
        }
        models.sort();
        models.dedup();
        for model in models.iter().filter(|m| !m.is_empty()) {
            // A model that is not loaded is not an error worth hearing about.
            let _ = client.unload(model).await;
        }
    }
}

/// Begin setup for `tier` (the person chose it, or agreed to the smaller one).
pub async fn start_setup<R: Runtime>(app: &AppHandle<R>, tier: Tier) -> Result<(), CommandError> {
    let setup = setup_of(app).await?;
    setup.start(tier).await.map_err(runtime_error)?;
    let state = app.state::<AppState>();
    if let Some(services) = state.services()
        && services.tier_unfinished() == Some(tier)
    {
        super::unfinished_settled(&state, &services);
        refresh_status(app).await;
    }
    Ok(())
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
        RuntimeError::Busy => CommandError::new(crate::error::ErrorCode::Busy, e),
        RuntimeError::Unavailable => CommandError::unavailable("the private engine is not running"),
        other => CommandError::internal(other),
    }
}
