//! What the desktop side remembers between launches, and what it holds while running.
//!
//! The file is `os-state.json` in the data folder, owner-only like everything else there. It
//! holds dates and the watched folder's ledger, never anything the person kept.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use super::autostart::Target;
use super::inbox::{InboxLedger, Timing};
use super::notify::Notifier;
use super::shortcut::{Registrar, ShortcutSlot};

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
    notifier: Mutex<Option<Arc<dyn Notifier>>>,
    autostart: Mutex<Option<Target>>,
    inbox: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    inbox_timing: Mutex<Timing>,
}

impl OsState {
    pub fn new(dir: &Path) -> Self {
        OsState {
            dir: dir.to_path_buf(),
            file: Mutex::new(load(dir)),
            shortcut: Mutex::new(ShortcutSlot::default()),
            notifier: Mutex::new(None),
            autostart: Mutex::new(None),
            inbox: Mutex::new(None),
            inbox_timing: Mutex::new(Timing::default()),
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

    /// Replace the running folder watcher (stopping the old one), or stop it with `None`.
    pub fn replace_inbox(&self, task: Option<tauri::async_runtime::JoinHandle<()>>) {
        let old = std::mem::replace(
            &mut *self.inbox.lock().unwrap_or_else(|e| e.into_inner()),
            task,
        );
        if let Some(old) = old {
            old.abort();
        }
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

    pub fn set_notifier(&self, notifier: Arc<dyn Notifier>) {
        *self.notifier.lock().unwrap_or_else(|e| e.into_inner()) = Some(notifier);
    }

    pub fn notifier(&self) -> Option<Arc<dyn Notifier>> {
        self.notifier
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// How the registered quick keep shortcut is spelled, when it is registered.
    pub fn shortcut_spelling(&self) -> Option<String> {
        self.shortcut().spelling.clone()
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
