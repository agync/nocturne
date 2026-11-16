//! What the desktop side remembers between launches, and what it holds while running.
//!
//! The file is `os-state.json` in the data folder, owner-only like everything else there. It
//! holds dates and the watched folder's ledger (salted hashes of paths and sizes, no names),
//! never anything the person kept.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use super::autostart::Target;
use super::inbox::{Handle, InboxLedger, Timing};
use super::notify::Notifier;
use super::shortcut::{Registrar, ShortcutSlot};

/// Runs between finding stable files and keeping them.
pub type BeforeKeep = Arc<dyn Fn(&[String]) + Send + Sync>;

pub const FILE: &str = "os-state.json";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OsFile {
    /// The page date a notification was sent for.
    pub notified: Option<String>,
    /// The last day the spoken page was used.
    pub audio_used: Option<String>,
    /// The page date the person opened.
    pub seen: Option<String>,
    /// The watched folder's ledger.
    pub inbox: Option<InboxLedger>,
    /// Whether the Background portal has been told about start with this computer (Flatpak).
    pub autostart_by_portal: bool,
}

/// Read the file; a missing or damaged one is an empty one.
pub fn load(dir: &Path) -> OsFile {
    std::fs::read(dir.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, file: &OsFile) -> std::io::Result<()> {
    crate::fsutil::create_private_dir(dir)?;
    crate::fsutil::write_private(&dir.join(FILE), &serde_json::to_vec(file)?)
}

/// Managed by the app once the desktop integrations start. Everything that reads it copes with
/// it being absent (tests, a store that could not be opened).
pub struct OsState {
    dir: PathBuf,
    file: Mutex<OsFile>,
    shortcut: Mutex<ShortcutSlot>,
    /// Held by whoever is moving the shortcut, for as long as that takes; the slot itself is only
    /// held for a moment, so reading it never waits for the desktop.
    shortcut_writer: Mutex<()>,
    notifier: Mutex<Option<Arc<dyn Notifier>>>,
    autostart: Mutex<Option<Target>>,
    #[cfg(target_os = "linux")]
    autostart_portal: Mutex<Option<super::portal::Bus>>,
    webview_dirs: Mutex<Vec<PathBuf>>,
    inbox: Mutex<Option<Handle>>,
    before_keep: Mutex<Option<BeforeKeep>>,
    inbox_timing: Mutex<Timing>,
    audio_attempts: Mutex<(Option<jiff::civil::Date>, u32)>,
}

impl OsState {
    pub fn new(dir: &Path) -> Self {
        OsState {
            dir: dir.to_path_buf(),
            file: Mutex::new(load(dir)),
            shortcut: Mutex::new(ShortcutSlot::default()),
            shortcut_writer: Mutex::new(()),
            notifier: Mutex::new(None),
            autostart: Mutex::new(None),
            #[cfg(target_os = "linux")]
            autostart_portal: Mutex::new(None),
            webview_dirs: Mutex::new(Vec::new()),
            inbox: Mutex::new(None),
            before_keep: Mutex::new(None),
            inbox_timing: Mutex::new(Timing::default()),
            audio_attempts: Mutex::new((None, 0)),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.dir
    }

    pub fn file(&self) -> OsFile {
        self.file.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Change the file and write it. A failed write is logged; the change still holds until
    /// the app exits.
    pub fn update<T>(&self, f: impl FnOnce(&mut OsFile) -> T) -> T {
        let mut g = self.file.lock().unwrap_or_else(|e| e.into_inner());
        let out = f(&mut g);
        if let Err(e) = save(&self.dir, &g) {
            tracing::warn!("could not save the desktop state: {e}");
        }
        out
    }

    pub fn forget_all(&self) {
        self.update(|f| *f = OsFile::default());
    }

    pub(super) fn shortcut_writer(&self) -> MutexGuard<'_, ()> {
        self.shortcut_writer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    pub(super) fn shortcut(&self) -> MutexGuard<'_, ShortcutSlot> {
        self.shortcut.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Hand the shortcut code the thing that grabs keys.
    pub fn set_registrar(&self, registrar: Arc<dyn Registrar>) {
        self.shortcut().registrar = Some(registrar);
    }

    /// Where the autostart entry goes (Linux).
    pub fn set_autostart(&self, target: Target) {
        *self.autostart.lock().unwrap_or_else(|e| e.into_inner()) = Some(target);
    }

    pub fn autostart(&self) -> Option<Target> {
        self.autostart
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Start with this computer through the Background portal (inside a Flatpak) instead of an
    /// entry file.
    #[cfg(target_os = "linux")]
    pub fn set_autostart_portal(&self, bus: super::portal::Bus) {
        *self
            .autostart_portal
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(bus);
    }

    #[cfg(target_os = "linux")]
    pub fn autostart_portal(&self) -> Option<super::portal::Bus> {
        self.autostart_portal
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The web view's own folders, removed by "Delete everything". Empty until [`super::install`]
    /// sets them from the real home folder, so tests never reach one.
    pub fn set_webview_dirs(&self, dirs: Vec<PathBuf>) {
        *self.webview_dirs.lock().unwrap_or_else(|e| e.into_inner()) = dirs;
    }

    pub fn webview_dirs(&self) -> Vec<PathBuf> {
        self.webview_dirs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Hand over the running folder watcher (to stop it), leaving none.
    pub fn take_inbox(&self) -> Option<Handle> {
        self.inbox.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    pub fn set_inbox(&self, handle: Option<Handle>) {
        *self.inbox.lock().unwrap_or_else(|e| e.into_inner()) = handle;
    }

    /// A hook that runs after the watcher has found stable files and before it keeps them, for
    /// tests that change the folder in that gap.
    pub fn set_before_keep(&self, hook: BeforeKeep) {
        *self.before_keep.lock().unwrap_or_else(|e| e.into_inner()) = Some(hook);
    }

    pub fn before_keep(&self) -> Option<BeforeKeep> {
        self.before_keep
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn inbox_running(&self) -> bool {
        self.inbox
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    pub fn set_inbox_timing(&self, timing: Timing) {
        *self.inbox_timing.lock().unwrap_or_else(|e| e.into_inner()) = timing;
    }

    pub fn inbox_timing(&self) -> Timing {
        *self.inbox_timing.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Take one of today's background tries at the spoken page, if any are left. Counting and
    /// taking are one step, so two checks at once cannot both take the last one.
    pub fn claim_audio_attempt(&self, day: jiff::civil::Date, most: u32) -> bool {
        let mut g = self
            .audio_attempts
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if g.0 != Some(day) {
            *g = (Some(day), 0);
        }
        if g.1 >= most {
            return false;
        }
        g.1 += 1;
        true
    }

    pub fn set_notifier(&self, notifier: Arc<dyn Notifier>) {
        *self.notifier.lock().unwrap_or_else(|e| e.into_inner()) = Some(notifier);
    }

    pub fn notifier(&self) -> Option<Arc<dyn Notifier>> {
        self.notifier
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// How the registered quick keep shortcut is spelled, when it is registered: the desktop's
    /// own name for the keys where the desktop holds it.
    pub fn shortcut_spelling(&self) -> Option<String> {
        super::shortcut::held_keys(self)
    }

    /// Whether the quick keep shortcut is registered right now.
    pub fn shortcut_registered(&self) -> bool {
        self.shortcut().current.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_round_trips_and_a_damaged_one_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()), OsFile::default());
        let state = OsState::new(dir.path());
        state.update(|f| f.notified = Some("2026-11-13".into()));
        assert_eq!(
            load(dir.path()).notified.as_deref(),
            Some("2026-11-13"),
            "saved on every update"
        );
        std::fs::write(dir.path().join(FILE), b"{ not json").unwrap();
        assert_eq!(load(dir.path()), OsFile::default());
    }

    #[test]
    fn background_tries_are_counted_per_day_and_taken_one_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let state = OsState::new(dir.path());
        let today: jiff::civil::Date = "2026-11-13".parse().unwrap();
        let taken = (0..10)
            .filter(|_| state.claim_audio_attempt(today, 3))
            .count();
        assert_eq!(taken, 3);
        let tomorrow: jiff::civil::Date = "2026-11-14".parse().unwrap();
        assert!(
            state.claim_audio_attempt(tomorrow, 3),
            "a new day starts again"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        OsState::new(dir.path()).update(|f| f.seen = Some("2026-11-13".into()));
        let mode = std::fs::metadata(dir.path().join(FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
