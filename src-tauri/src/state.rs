//! The state the app manages: the engine, the mind slot, the status hub, the live asks.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use jiff::Timestamp;
use nocturne_contract::{
    BackupProgress, BackupSummary, EVENT_NEEDS_YOU, EVENT_SETUP, EVENT_STATUS, EngineStatus,
    ItemFilter, ItemStatus, SetupPhase, SetupView,
};
use nocturne_engine::{Clock, Engine, Mind};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::Notify;

use crate::ask::AskRegistry;
use crate::error::{CommandError, ErrorCode, blocking};
use crate::runtime::Services;
use crate::status::{Counts, MindState, StatusHub};

/// Overrides the data directory (tests and development).
pub const DATA_DIR_ENV: &str = "NOCTURNE_DATA_DIR";
/// Fills the mind slot with a scripted mind (development and tests, `fake-mind` feature).
pub const FAKE_MIND_ENV: &str = "NOCTURNE_FAKE_MIND";

/// Where the mind lives. The runtime fills it with the real mind once the private engine is
/// up and clears it when the engine gives up; while it is empty anything that needs the mind
/// says "unavailable".
///
/// The mind goes into the slot first and then reports `MindError::Busy` while a model loads
/// (the status shows "starting" and an ask shows "waking"); it is not held back until it is
/// ready. Use [`set_mind`] so the status and the setup view follow. The slot is the same one
/// the reader and the tidy hold (see [`MindSlot::shared`]).
pub struct MindSlot {
    inner: nocturne_intelligence::MindSlot,
    loading: AtomicBool,
}

impl Default for MindSlot {
    fn default() -> Self {
        MindSlot {
            inner: nocturne_intelligence::new_mind_slot(),
            loading: AtomicBool::new(false),
        }
    }
}

impl MindSlot {
    pub fn get(&self) -> Option<Arc<dyn Mind>> {
        self.inner.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set(&self, mind: Arc<dyn Mind>) {
        *self.inner.write().unwrap_or_else(|e| e.into_inner()) = Some(mind);
        self.loading.store(false, Ordering::SeqCst);
    }

    pub fn clear(&self) {
        *self.inner.write().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// The slot as the reader and the tidy hold it.
    pub fn shared(&self) -> nocturne_intelligence::MindSlot {
        self.inner.clone()
    }

    /// The mind is starting (a model is being loaded); the slot may already hold it.
    pub fn set_loading(&self, loading: bool) {
        self.loading.store(loading, Ordering::SeqCst);
    }

    pub fn is_loading(&self) -> bool {
        self.loading.load(Ordering::SeqCst)
    }

    pub fn state(&self) -> MindState {
        if self.is_loading() {
            MindState::Loading
        } else if self.get().is_some() {
            MindState::Ready
        } else {
            MindState::Absent
        }
    }

    /// Fill the slot with a scripted mind when `NOCTURNE_FAKE_MIND=1`. Only a debug build with
    /// the `fake-mind` feature has one; a release build ignores the variable.
    pub fn install_from_env(&self) {
        let wanted = std::env::var(FAKE_MIND_ENV).is_ok_and(|v| v == "1");
        if !wanted {
            return;
        }
        #[cfg(all(feature = "fake-mind", debug_assertions))]
        {
            self.set(Arc::new(nocturne_engine::FakeMind::new()));
            tracing::info!("using the scripted mind");
        }
        #[cfg(not(all(feature = "fake-mind", debug_assertions)))]
        tracing::warn!("{FAKE_MIND_ENV} is set but this build has no scripted mind");
    }
}

pub struct AppState {
    engine: Option<Engine>,
    pub mind: MindSlot,
    pub status: StatusHub,
    pub asks: AskRegistry,
    /// The guide's vectors, for help with Nocturne.
    pub help: crate::help::HelpStore,
    /// The journal's looking back: the override of its check, and the writes in flight.
    pub reflecting: crate::journal::Reflecting,
    pub setup: RwLock<SetupView>,
    pub clock: Arc<dyn Clock>,
    pub data_dir: PathBuf,
    pub tray_available: AtomicBool,
    /// Signalled whenever there is new work for the reading pipeline (an item was kept or
    /// put back in the queue). Capture never waits on it.
    pub work: Notify,
    /// Files the person dropped or picked in this session: the only paths `keep_files` takes.
    chosen_paths: Mutex<HashSet<PathBuf>>,
    /// Files a save dialog chose as a backup destination, each good for one finished save.
    save_targets: Mutex<HashSet<PathBuf>>,
    /// Files an open dialog chose as a backup to restore: only the backup commands read them.
    restore_sources: Mutex<HashSet<PathBuf>>,
    /// Items the quick capture window kept in this session: the only ones it may read or take
    /// back.
    quick_items: Mutex<HashSet<String>>,
    /// Files dropped on the quick capture window, until it asks for them. That window may not
    /// listen to events, so it asks.
    quick_drop: Mutex<Vec<PathBuf>>,
    /// Folders made by an export in this session: the only ones `open_export` opens.
    exports: Mutex<HashSet<PathBuf>>,
    /// The one backup job: a backup is being written, or a backup file is being checked.
    backup: Mutex<BackupJob>,
    /// Passphrase work for new backups, as a test sets it. 0 is what Nocturne writes.
    backup_work: AtomicU8,
    /// Restarts the app after a restore has been scheduled (installed by `run`).
    restarter: Mutex<Option<RestartFn>>,
    /// Whether something has shown the main window yet.
    window_shown: AtomicBool,
    /// Start in the tray, without showing the window (set by the OS integration lane).
    start_hidden: AtomicBool,
    /// One status refresh at a time, so two events cannot publish out of order.
    refresh: tokio::sync::Mutex<()>,
    /// Whether a background refresh is waiting to start; a second request joins it.
    refresh_queued: AtomicBool,
    exit_hooks: Mutex<Vec<ExitHook>>,
    services: OnceLock<Arc<Services>>,
}

/// Restarts the app so a scheduled restore runs (installed by `run`).
pub type RestartFn = Arc<dyn Fn() + Send + Sync>;

/// The one backup or restore job, and the check it left for a restore.
#[derive(Default)]
struct BackupJob {
    running: bool,
    /// Set once a restore has been unpacked and only the switch at the next start is left.
    committed: bool,
    cancel: Arc<AtomicBool>,
    progress: Option<BackupProgress>,
    checked: Option<(PathBuf, BackupSummary)>,
}
/// Runs when the app is exiting (stop the private engine, flush, remove the tray).
pub type ExitHook = Box<dyn Fn() + Send + Sync>;

/// The most chosen paths remembered.
const MAX_CHOSEN: usize = 4096;

fn canonical(p: &Path) -> PathBuf {
    dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Setup before the engine answers, as far as its record on disk tells, so the window can tell a
/// first run or an unfinished first setup from a later launch straight away.
fn blank_setup(data_dir: &Path) -> SetupView {
    let blank = SetupView {
        phase: SetupPhase::NotStarted,
        detected: None,
        chosen: None,
        ram_gib: 0,
        free_disk_bytes: 0.0,
        options: Vec::new(),
        downloaded_bytes: 0.0,
        total_bytes: 0.0,
        eta_seconds: None,
        paused: None,
        error: None,
        needed_bytes: None,
    };
    nocturne_lemonade::record::seed(blank, &crate::models::config_dir(data_dir))
}

/// `NOCTURNE_DATA_DIR` when set and not empty, else the platform's app data directory.
pub fn resolve_data_dir(env_override: Option<String>, platform_default: PathBuf) -> PathBuf {
    let dir = match env_override {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => platform_default,
    };
    // Symlinks and `..` are resolved once, so every path derived from it (the asset scope, the
    // checks that a file is inside) compares like with like.
    let _ = std::fs::create_dir_all(&dir);
    dunce::canonicalize(&dir).unwrap_or(dir)
}

impl AppState {
    /// Open the store under `data_dir`. This never fails: when the store cannot be opened (the
    /// folder is not writable, the file is damaged) the state has no engine, says so in the
    /// status as a repair notice, and every engine command answers `unavailable`, so the window
    /// can still open and explain.
    pub fn open(data_dir: &Path, clock: Arc<dyn Clock>) -> AppState {
        let store = data_dir.join("store");
        let opened = std::fs::create_dir_all(data_dir)
            .map_err(nocturne_engine::Error::from)
            .and_then(|()| Engine::open(&store, clock.clone()));
        let engine = match opened {
            Ok(e) => Some(e),
            Err(e) => {
                tracing::error!("the store could not be opened: {e}");
                None
            }
        };
        let status = StatusHub::new(engine.as_ref().map(|_| data_dir));
        status.set_engine_up(engine.is_some());
        AppState {
            engine,
            mind: MindSlot::default(),
            status,
            asks: AskRegistry::default(),
            help: crate::help::HelpStore::default(),
            reflecting: crate::journal::Reflecting::default(),
            setup: RwLock::new(blank_setup(data_dir)),
            clock,
            data_dir: data_dir.to_path_buf(),
            tray_available: AtomicBool::new(false),
            work: Notify::new(),
            chosen_paths: Mutex::new(HashSet::new()),
            save_targets: Mutex::new(HashSet::new()),
            restore_sources: Mutex::new(HashSet::new()),
            backup_work: AtomicU8::new(0),
            quick_items: Mutex::new(HashSet::new()),
            quick_drop: Mutex::new(Vec::new()),
            exports: Mutex::new(HashSet::new()),
            backup: Mutex::new(BackupJob::default()),
            restarter: Mutex::new(None),
            window_shown: AtomicBool::new(false),
            start_hidden: AtomicBool::new(false),
            refresh: tokio::sync::Mutex::new(()),
            refresh_queued: AtomicBool::new(false),
            exit_hooks: Mutex::new(Vec::new()),
            services: OnceLock::new(),
        }
    }

    /// The running reader, tidy and scheduler, once [`crate::runtime::start`] has run.
    pub fn services(&self) -> Option<Arc<Services>> {
        self.services.get().cloned()
    }

    pub(crate) fn install_services(&self, services: Arc<Services>) -> bool {
        self.services.set(services).is_ok()
    }

    /// The quick capture window kept this item.
    pub fn note_quick_item(&self, id: &str) {
        let mut set = self.quick_items.lock().unwrap_or_else(|e| e.into_inner());
        if set.len() >= MAX_CHOSEN {
            set.clear();
        }
        set.insert(id.to_string());
    }

    pub fn quick_item(&self, id: &str) -> bool {
        self.quick_items
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(id)
    }

    /// Files were dropped on the quick capture window: hold them for it.
    pub fn note_quick_drop(&self, paths: &[PathBuf]) {
        let mut held = self.quick_drop.lock().unwrap_or_else(|e| e.into_inner());
        if held.len() >= MAX_CHOSEN {
            held.clear();
        }
        held.extend(paths.iter().cloned());
    }

    /// What was dropped on the quick capture window since it last asked.
    pub fn take_quick_drop(&self) -> Vec<PathBuf> {
        std::mem::take(&mut *self.quick_drop.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Remember files the person dropped or picked.
    pub fn allow_paths(&self, paths: impl IntoIterator<Item = PathBuf>) {
        let mut set = self.chosen_paths.lock().unwrap_or_else(|e| e.into_inner());
        if set.len() >= MAX_CHOSEN {
            set.clear();
        }
        set.extend(paths.into_iter().map(|p| canonical(&p)));
    }

    pub fn path_allowed(&self, path: &Path) -> bool {
        self.chosen_paths
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&canonical(path))
    }

    /// Remember a file a save dialog chose as a backup destination.
    pub fn allow_save_target(&self, path: &Path) {
        let mut set = self.save_targets.lock().unwrap_or_else(|e| e.into_inner());
        if set.len() >= MAX_CHOSEN {
            set.clear();
        }
        set.insert(canonical(path));
    }

    pub fn save_target_allowed(&self, path: &Path) -> bool {
        self.save_targets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&canonical(path))
    }

    /// Check and consume a save target: a finished save cannot be replayed without the dialog.
    pub fn take_save_target(&self, path: &Path) -> bool {
        self.save_targets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&canonical(path))
    }

    /// Ask less passphrase work of new backups, so a test's backups open quickly.
    #[doc(hidden)]
    pub fn set_backup_work_factor(&self, factor: u8) {
        self.backup_work.store(factor, Ordering::Relaxed);
    }

    /// The passphrase work a new backup asks for: `None` is the real one.
    pub fn backup_work_factor(&self) -> Option<u8> {
        Some(self.backup_work.load(Ordering::Relaxed)).filter(|f| *f != 0)
    }

    /// Remember a file an open dialog chose as a backup to restore.
    pub fn allow_restore_source(&self, path: &Path) {
        let mut set = self
            .restore_sources
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if set.len() >= MAX_CHOSEN {
            set.clear();
        }
        set.insert(canonical(path));
    }

    pub fn restore_source_allowed(&self, path: &Path) -> bool {
        self.restore_sources
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&canonical(path))
    }

    pub fn allow_export(&self, folder: &Path) {
        self.exports
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(canonical(folder));
    }

    pub fn export_allowed(&self, folder: &Path) -> bool {
        self.exports
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&canonical(folder))
    }

    pub fn forget_session(&self) {
        self.quick_items
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.quick_drop
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.exports
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.chosen_paths
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.save_targets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.restore_sources
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.set_checked_backup(None);
    }

    /// Take the one backup job, or say another is running. A new job starts uncancelled and
    /// uncommitted, and the last progress it reported is forgotten.
    pub fn begin_backup(&self) -> Result<Arc<AtomicBool>, CommandError> {
        let mut job = self.backup.lock().unwrap_or_else(|e| e.into_inner());
        if job.running {
            return Err(CommandError::new(
                ErrorCode::Busy,
                "a backup is already running",
            ));
        }
        job.running = true;
        job.committed = false;
        job.progress = None;
        job.cancel.store(false, Ordering::SeqCst);
        Ok(Arc::clone(&job.cancel))
    }

    /// Release the job, whether it finished, failed or was cancelled.
    pub fn end_backup(&self) {
        let mut job = self.backup.lock().unwrap_or_else(|e| e.into_inner());
        job.running = false;
        job.committed = false;
        job.progress = None;
    }

    /// Stop the running work. Harmless when nothing runs, and nothing once a restore is
    /// committed: the switch at the next start cannot be stopped any more.
    pub fn cancel_backup(&self) {
        let job = self.backup.lock().unwrap_or_else(|e| e.into_inner());
        if !job.committed {
            job.cancel.store(true, Ordering::SeqCst);
        }
    }

    /// Say the unpacked restore is fixed: `cancel_backup` no longer stops it.
    pub fn commit_backup(&self) {
        self.backup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .committed = true;
    }

    /// Whether no backup task is running.
    pub fn backup_idle(&self) -> bool {
        !self
            .backup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .running
    }

    /// What the running task last reported, for the backup screen.
    pub fn set_backup_progress(&self, progress: Option<BackupProgress>) {
        self.backup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .progress = progress;
    }

    pub fn backup_progress(&self) -> Option<BackupProgress> {
        let job = self.backup.lock().unwrap_or_else(|e| e.into_inner());
        if job.running { job.progress } else { None }
    }

    /// Remember what was checked, with the file it came from, or forget it with `None`.
    pub fn set_checked_backup(&self, checked: Option<(PathBuf, BackupSummary)>) {
        self.backup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .checked = checked;
    }

    pub fn checked_backup(&self) -> Option<(PathBuf, BackupSummary)> {
        self.backup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .checked
            .clone()
    }

    pub fn set_restarter(&self, restart: RestartFn) {
        *self.restarter.lock().unwrap_or_else(|e| e.into_inner()) = Some(restart);
    }

    pub fn restarter(&self) -> Option<RestartFn> {
        self.restarter
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn window_shown(&self) -> bool {
        self.window_shown.load(Ordering::SeqCst)
    }

    pub fn mark_window_shown(&self) {
        self.window_shown.store(true, Ordering::SeqCst);
    }

    pub fn start_hidden(&self) -> bool {
        self.start_hidden.load(Ordering::SeqCst)
    }

    pub fn set_start_hidden(&self, on: bool) {
        self.start_hidden.store(on, Ordering::SeqCst);
    }

    /// Register something to run when the app exits. The runtime lane stops its private
    /// engine here; the OS integration lane removes its tray.
    pub fn on_exit(&self, hook: ExitHook) {
        self.exit_hooks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(hook);
    }

    /// Run the exit hooks once. They block on the async runtime to stop the engine and Away from
    /// home, which panics on a thread that drives async tasks, so from inside the runtime (the
    /// restart after a restore) they run on a thread of their own.
    pub fn run_exit_hooks(&self) {
        let hooks = std::mem::take(&mut *self.exit_hooks.lock().unwrap_or_else(|e| e.into_inner()));
        if hooks.is_empty() {
            return;
        }
        let run = move || hooks.iter().for_each(|hook| hook());
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return run();
        };
        let apart = move || {
            std::thread::scope(|s| {
                if s.spawn(run).join().is_err() {
                    tracing::warn!("an exit hook panicked");
                }
            });
        };
        if runtime.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread {
            // Hand this worker's queued tasks to the others while the hooks wait on them.
            tokio::task::block_in_place(apart);
        } else {
            apart();
        }
    }

    /// The engine, or `unavailable` when the store could not be opened.
    pub fn engine(&self) -> Result<Engine, CommandError> {
        self.engine
            .clone()
            .ok_or_else(|| CommandError::unavailable("the store could not be opened"))
    }

    pub fn has_engine(&self) -> bool {
        self.engine.is_some()
    }

    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }

    pub fn tray_available(&self) -> bool {
        self.tray_available.load(Ordering::SeqCst)
    }

    pub fn set_tray_available(&self, on: bool) {
        self.tray_available.store(on, Ordering::SeqCst);
    }

    /// Tell the reading pipeline there is work.
    pub fn wake_pipeline(&self) {
        self.work.notify_one();
        if let Some(services) = self.services() {
            services.reader.wake();
        }
    }

    /// The gate that serialises status refreshes, for tests that hold a refresh in progress.
    #[doc(hidden)]
    pub fn status_refresh_gate(&self) -> &tokio::sync::Mutex<()> {
        &self.refresh
    }

    fn setup_view(&self) -> SetupView {
        let mut view = self.setup.read().unwrap_or_else(|e| e.into_inner()).clone();
        if self.mind.state() == MindState::Ready && view.phase == SetupPhase::NotStarted {
            view.phase = SetupPhase::Done;
        }
        view
    }

    pub fn get_setup(&self) -> SetupView {
        self.setup_view()
    }
}

/// How many "couldn't read" notices can show at once; the ids are only looked up when some
/// item could not be read.
const COULDNT_READ_SHOWN: u32 = 10;

fn read_counts(engine: &Engine) -> Counts {
    let counts = engine.status_counts().unwrap_or_default();
    let couldnt_read = if counts.couldnt_read == 0 {
        Vec::new()
    } else {
        engine
            .items(ItemFilter {
                status: Some(ItemStatus::CouldntRead),
                limit: Some(COULDNT_READ_SHOWN),
                ..Default::default()
            })
            .unwrap_or_default()
            .into_iter()
            .map(|i| i.id)
            .collect()
    };
    Counts {
        needs_you: counts.needs_you,
        waiting_to_read: counts.waiting_to_read,
        couldnt_read,
    }
}

/// Recompute the status and emit `status-changed` (and `needs-you-changed`) if it differs from
/// the last one. Call after anything that can change what is waiting.
pub async fn refresh_status<R: Runtime>(app: &AppHandle<R>) -> EngineStatus {
    let state = app.state::<AppState>();
    let _one_at_a_time = state.refresh.lock().await;
    crate::runtime::follow_pause(&state);
    state.status.set_mind(state.mind.state());
    let counts = match state.engine() {
        Ok(engine) => blocking(move || Ok(read_counts(&engine)))
            .await
            .unwrap_or_default(),
        Err(_) => Counts::default(),
    };
    let ev = state.status.evaluate(&counts, state.now());
    if ev.status_changed
        && let Err(e) = app.emit(EVENT_STATUS, &ev.status)
    {
        tracing::warn!("could not emit the status: {e}");
    }
    if ev.needs_you_changed
        && let Err(e) = app.emit(EVENT_NEEDS_YOU, ev.status.needs_you)
    {
        tracing::warn!("could not emit the needs-you count: {e}");
    }
    ev.status
}

/// Refresh the status without making the caller wait for it. Requests made while one is still
/// waiting to start share it, since it reads the store afresh when it runs.
pub fn refresh_status_soon<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    if state.refresh_queued.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        app.state::<AppState>()
            .refresh_queued
            .store(false, Ordering::Release);
        refresh_status(&app).await;
    });
}

/// Pause reading until an instant (RFC 3339), or (`None`) until the person resumes. The
/// settings command and the tray both come here, so they cannot drift apart.
pub async fn pause_until<R: Runtime>(
    app: &AppHandle<R>,
    until: Option<String>,
) -> Result<(), CommandError> {
    let state = app.state::<AppState>();
    if let Some(u) = &until {
        let at: Timestamp = u.parse().map_err(|_| {
            CommandError::invalid("pause until a time such as 2026-11-13T09:00:00Z")
        })?;
        if at <= state.now() {
            return Err(CommandError::invalid("that time has passed"));
        }
    }
    let handle = app.clone();
    crate::error::blocking_cmd(move || {
        handle.state::<AppState>().status.pause(until);
        Ok(())
    })
    .await?;
    refresh_status(app).await;
    Ok(())
}

/// Resume reading now, and wake the pipeline.
pub async fn resume_now<R: Runtime>(app: &AppHandle<R>) -> Result<(), CommandError> {
    let state = app.state::<AppState>();
    let handle = app.clone();
    crate::error::blocking_cmd(move || {
        handle.state::<AppState>().status.resume();
        Ok(())
    })
    .await?;
    state.wake_pipeline();
    refresh_status(app).await;
    Ok(())
}

/// Replace the setup view (the runtime lane calls this as setup moves) and tell the interface.
pub fn publish_setup<R: Runtime>(app: &AppHandle<R>, view: SetupView) {
    let state = app.state::<AppState>();
    *state.setup.write().unwrap_or_else(|e| e.into_inner()) = view;
    let view = state.get_setup();
    if let Err(e) = app.emit(EVENT_SETUP, &view) {
        tracing::warn!("could not emit setup progress: {e}");
    }
}

/// Put a mind in the slot (or take it out with `None`), then bring the status and the setup
/// view in line with it. The runtime lane calls this when its private engine is up.
pub async fn set_mind<R: Runtime>(app: &AppHandle<R>, mind: Option<Arc<dyn Mind>>) {
    let state = app.state::<AppState>();
    match mind {
        Some(m) => state.mind.set(m),
        None => state.mind.clear(),
    }
    refresh_status(app).await;
    let view = state.get_setup();
    if let Err(e) = app.emit(EVENT_SETUP, &view) {
        tracing::warn!("could not emit setup progress: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocturne_contract::Tier;
    use nocturne_engine::SystemClock;

    fn counting_hooks(state: &AppState, count: usize) -> Arc<std::sync::atomic::AtomicUsize> {
        let ran = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for _ in 0..count {
            let ran = ran.clone();
            state.on_exit(Box::new(move || {
                tauri::async_runtime::block_on(tokio::task::yield_now());
                ran.fetch_add(1, Ordering::SeqCst);
            }));
        }
        ran
    }

    /// The restart after a restore calls the restarter from an async task, and the
    /// restarter runs the exit hooks, which block on the runtime.
    #[test]
    fn exit_hooks_run_from_an_async_task() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::open(dir.path(), Arc::new(SystemClock)));
        let ran = counting_hooks(&state, 2);
        let restarting = state.clone();
        tauri::async_runtime::block_on(tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            restarting.run_exit_hooks();
        }))
        .expect("the exit hooks ran without a panic");
        assert_eq!(ran.load(Ordering::SeqCst), 2);
        state.run_exit_hooks();
        assert_eq!(ran.load(Ordering::SeqCst), 2, "the hooks run once");
    }

    #[tokio::test]
    async fn exit_hooks_run_inside_a_single_threaded_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path(), Arc::new(SystemClock));
        let ran = counting_hooks(&state, 1);
        state.run_exit_hooks();
        assert_eq!(ran.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn exit_hooks_run_outside_any_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path(), Arc::new(SystemClock));
        let ran = counting_hooks(&state, 1);
        state.run_exit_hooks();
        assert_eq!(ran.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_data_dir_override_wins_unless_empty() {
        // The folder is created and resolved, so it lives in a scratch folder.
        let tmp = tempfile::tempdir().unwrap();
        let default = tmp.path().join("default");
        let other = tmp.path().join("other");
        let chosen = resolve_data_dir(Some(other.to_string_lossy().into_owned()), default.clone());
        assert_eq!(chosen, dunce::canonicalize(&other).unwrap());
        let default = dunce::canonicalize(resolve_data_dir(None, default)).unwrap();
        assert_eq!(
            resolve_data_dir(Some("  ".into()), default.clone()),
            default
        );
        assert_eq!(resolve_data_dir(None, default.clone()), default);
    }

    /// The data folder is joined, compared, shown and handed to the engine, so on Windows it must
    /// not carry the verbatim prefix (`\\?\C:\...`) that plain paths never have.
    #[test]
    fn the_data_dir_is_a_plain_path() {
        let tmp = tempfile::tempdir().unwrap();
        let chosen = resolve_data_dir(None, tmp.path().join("data"));
        assert!(chosen.is_absolute());
        assert!(!chosen.to_string_lossy().starts_with(r"\\?\"), "{chosen:?}");
    }

    #[cfg(unix)]
    #[test]
    fn the_data_dir_is_resolved_through_symlinks_and_dots() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let tricky = link.join("..").join("link");
        let got = resolve_data_dir(Some(tricky.to_string_lossy().into_owned()), PathBuf::new());
        assert_eq!(got, dunce::canonicalize(&real).unwrap());
    }

    /// The window opens setup on a first run, and only then, before the engine has answered.
    #[test]
    fn setup_says_from_disk_whether_a_size_was_ever_chosen() {
        let dir = tempfile::tempdir().unwrap();
        let first = AppState::open(dir.path(), Arc::new(SystemClock)).get_setup();
        assert_eq!((first.phase, first.chosen), (SetupPhase::NotStarted, None));
        let config = crate::models::config_dir(dir.path());
        let started = nocturne_lemonade::record::SetupRecord {
            version: 1,
            tier: Some(Tier::Small),
            ..Default::default()
        };
        nocturne_lemonade::record::write(&config, &started).unwrap();
        let unfinished = AppState::open(dir.path(), Arc::new(SystemClock)).get_setup();
        assert_eq!(
            (unfinished.phase, unfinished.chosen),
            (SetupPhase::Paused, Some(Tier::Small))
        );
        let finished = nocturne_lemonade::record::SetupRecord {
            done: true,
            completed: Some(Tier::Small),
            ..started
        };
        nocturne_lemonade::record::write(&config, &finished).unwrap();
        let later = AppState::open(dir.path(), Arc::new(SystemClock)).get_setup();
        assert_eq!(
            (later.phase, later.chosen),
            (SetupPhase::NotStarted, Some(Tier::Small))
        );
    }

    #[test]
    fn chosen_paths_and_exports_are_remembered_by_their_real_location() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path(), Arc::new(SystemClock));
        let f = dir.path().join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        assert!(!state.path_allowed(&f));
        state.allow_paths([f.clone()]);
        assert!(state.path_allowed(&f));
        assert!(state.path_allowed(&dir.path().join(".").join("a.txt")));
        assert!(!state.path_allowed(&dir.path().join("b.txt")));
        state.allow_export(dir.path());
        assert!(state.export_allowed(dir.path()));

        // A save target is good for one finished save; a restore source is only read.
        let dest = dir.path().join("b.nocturne");
        assert!(!state.save_target_allowed(&dest));
        state.allow_save_target(&dest);
        assert!(state.save_target_allowed(&dest));
        assert!(state.take_save_target(&dest));
        assert!(!state.save_target_allowed(&dest));
        assert!(!state.take_save_target(&dest));
        let source = dir.path().join("c.nocturne");
        state.allow_restore_source(&source);
        assert!(state.restore_source_allowed(&source));
        assert!(!state.restore_source_allowed(&f));

        state.forget_session();
        assert!(!state.export_allowed(dir.path()) && !state.path_allowed(&f));
        assert!(!state.restore_source_allowed(&source));
        assert!(!state.save_target_allowed(&dest));
    }

    #[test]
    fn a_new_state_asks_for_the_real_passphrase_work() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path(), Arc::new(SystemClock));
        assert_eq!(state.backup_work_factor(), None);
        state.set_backup_work_factor(10);
        assert_eq!(state.backup_work_factor(), Some(10));
    }

    #[test]
    fn open_lays_out_the_store_and_a_blocked_folder_leaves_no_engine() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path(), Arc::new(SystemClock));
        assert!(state.has_engine());
        assert!(dir.path().join("store").is_dir());

        // A file where the folder should be: nothing can be created under it.
        let blocked = dir.path().join("a-file");
        std::fs::write(&blocked, b"x").unwrap();
        let state = AppState::open(&blocked.join("inside"), Arc::new(SystemClock));
        assert!(!state.has_engine());
        assert_eq!(
            state.engine().err().map(|e| e.code),
            Some(crate::error::ErrorCode::Unavailable)
        );
    }

    #[test]
    fn a_damaged_store_leaves_no_engine_instead_of_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("store");
        std::fs::create_dir_all(&store).unwrap();
        std::fs::write(store.join("nocturne.db"), b"this is not a database at all").unwrap();
        let state = AppState::open(dir.path(), Arc::new(SystemClock));
        assert!(!state.has_engine());
    }

    #[test]
    fn one_backup_job_at_a_time_and_forgetting_drops_the_checked_one() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::open(dir.path(), Arc::new(SystemClock));
        let cancel = state.begin_backup().unwrap();
        let busy = state.begin_backup().unwrap_err();
        assert_eq!(busy.code, ErrorCode::Busy);
        assert!(
            !cancel.load(Ordering::SeqCst),
            "the second take changed nothing"
        );

        let progress = nocturne_contract::BackupProgress {
            phase: nocturne_contract::BackupPhase::Saving,
            done: 5.0,
            total: 10.0,
        };
        state.set_backup_progress(Some(progress));
        assert_eq!(state.backup_progress(), Some(progress));

        state.cancel_backup();
        assert!(cancel.load(Ordering::SeqCst));
        state.end_backup();
        assert!(state.backup_idle());
        assert!(state.backup_progress().is_none());

        let next = state.begin_backup().unwrap();
        assert!(!next.load(Ordering::SeqCst), "a new job starts uncancelled");
        state.commit_backup();
        state.cancel_backup();
        assert!(
            !next.load(Ordering::SeqCst),
            "cancel does nothing once a restore is committed"
        );
        state.end_backup();

        let summary = nocturne_contract::BackupSummary {
            made_at: "2026-11-13T07:00:00.000Z".into(),
            app_version: "0.2.2".into(),
            store_version: 10,
            memories: 1,
            items: 1,
            pages: 0,
            originals: 0,
            online_used: false,
            away_used: false,
            bytes: 10.0,
        };
        let file = dir.path().join("chosen.nocturne");
        state.set_checked_backup(Some((file.clone(), summary.clone())));
        assert_eq!(state.checked_backup(), Some((file, summary)));
        state.forget_session();
        assert!(state.checked_backup().is_none());
    }

    #[test]
    fn the_mind_slot_reports_its_state() {
        let slot = MindSlot::default();
        assert_eq!(slot.state(), MindState::Absent);
        slot.set_loading(true);
        assert_eq!(slot.state(), MindState::Loading);
        slot.set(Arc::new(nocturne_engine::FakeMind::new()));
        assert_eq!(
            slot.state(),
            MindState::Ready,
            "setting the mind ends loading"
        );
        slot.clear();
        assert_eq!(slot.state(), MindState::Absent);
    }
}
