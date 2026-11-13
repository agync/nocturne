//! The state the app manages: the engine, the mind slot, the status hub, the live asks.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use jiff::Timestamp;
use nocturne_contract::{
    EVENT_NEEDS_YOU, EVENT_SETUP, EVENT_STATUS, EngineStatus, ItemFilter, ItemStatus, SetupPhase,
    SetupView,
};
use nocturne_engine::{Clock, Engine, Mind};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::Notify;

use crate::ask::AskRegistry;
use crate::error::{CommandError, blocking};
use crate::status::{Counts, MindState, StatusHub};

/// Overrides the data directory (tests and development).
pub const DATA_DIR_ENV: &str = "NOCTURNE_DATA_DIR";
/// Fills the mind slot with a scripted mind (development and tests, `fake-mind` feature).
pub const FAKE_MIND_ENV: &str = "NOCTURNE_FAKE_MIND";

/// Where the mind lives. The intelligence lane fills it with the real mind once the private
/// engine is up; until then it is empty and anything that needs the mind says "unavailable".
#[derive(Default)]
pub struct MindSlot {
    inner: RwLock<Option<Arc<dyn Mind>>>,
    loading: AtomicBool,
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

    /// Fill the slot with a scripted mind when `NOCTURNE_FAKE_MIND=1` and the build has one.
    pub fn install_from_env(&self) {
        let wanted = std::env::var(FAKE_MIND_ENV).is_ok_and(|v| v == "1");
        if !wanted {
            return;
        }
        #[cfg(feature = "fake-mind")]
        {
            self.set(Arc::new(nocturne_engine::FakeMind::new()));
            tracing::info!("using the scripted mind");
        }
        #[cfg(not(feature = "fake-mind"))]
        tracing::warn!("{FAKE_MIND_ENV} is set but this build has no scripted mind");
    }
}

pub struct AppState {
    engine: Option<Engine>,
    pub mind: MindSlot,
    pub status: StatusHub,
    pub asks: AskRegistry,
    pub setup: RwLock<SetupView>,
    pub clock: Arc<dyn Clock>,
    pub data_dir: PathBuf,
    pub tray_available: AtomicBool,
    /// Signalled whenever there is new work for the reading pipeline (an item was kept or
    /// put back in the queue). Capture never waits on it.
    pub work: Notify,
}

fn blank_setup() -> SetupView {
    SetupView {
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
    }
}

/// `NOCTURNE_DATA_DIR` when set and not empty, else the platform's app data directory.
pub fn resolve_data_dir(env_override: Option<String>, platform_default: PathBuf) -> PathBuf {
    match env_override {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => platform_default,
    }
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
            setup: RwLock::new(blank_setup()),
            clock,
            data_dir: data_dir.to_path_buf(),
            tray_available: AtomicBool::new(false),
            work: Notify::new(),
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

    pub fn tmp_dir(&self) -> PathBuf {
        self.data_dir.join("tmp")
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

fn read_counts(engine: &Engine) -> Counts {
    let needs_you = engine.needs_you().map(|v| v.len() as u32).unwrap_or(0);
    let count = |status| {
        engine
            .items(ItemFilter {
                status: Some(status),
                limit: Some(1000),
                ..Default::default()
            })
            .unwrap_or_default()
    };
    let waiting = count(ItemStatus::Kept).len() + count(ItemStatus::Reading).len();
    let couldnt_read = count(ItemStatus::CouldntRead)
        .into_iter()
        .take(10)
        .map(|i| i.id)
        .collect();
    Counts {
        needs_you,
        waiting_to_read: waiting as u32,
        couldnt_read,
    }
}

/// Recompute the status and emit `status-changed` (and `needs-you-changed`) if it differs from
/// the last one. Call after anything that can change what is waiting.
pub async fn refresh_status<R: Runtime>(app: &AppHandle<R>) -> EngineStatus {
    let state = app.state::<AppState>();
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

/// Replace the setup view (the runtime lane calls this as setup moves) and tell the interface.
pub fn publish_setup<R: Runtime>(app: &AppHandle<R>, view: SetupView) {
    let state = app.state::<AppState>();
    *state.setup.write().unwrap_or_else(|e| e.into_inner()) = view;
    let view = state.get_setup();
    if let Err(e) = app.emit(EVENT_SETUP, &view) {
        tracing::warn!("could not emit setup progress: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocturne_engine::SystemClock;

    #[test]
    fn the_data_dir_override_wins_unless_empty() {
        let default = PathBuf::from("/data/default");
        assert_eq!(
            resolve_data_dir(Some("/data/other".into()), default.clone()),
            PathBuf::from("/data/other")
        );
        assert_eq!(
            resolve_data_dir(Some("  ".into()), default.clone()),
            default
        );
        assert_eq!(resolve_data_dir(None, default.clone()), default);
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
