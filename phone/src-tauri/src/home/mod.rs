//! A phone that holds its memory itself: one store in the app's own space, the same engine a
//! computer runs, opened when the phone is alone and the screens first need it. Nothing here
//! touches the network, and there is no mind: what is kept waits to be read.
//!
//! The way in for the sync (calendar entries and health readings into this store) is
//! [`Home::local`]: the open engine and this phone's own device id, the source what the phone
//! brings belongs to. It opens the store when it is not open yet, so call it off the async
//! workers. It answers `None` while the phone is not alone or its memory cannot be opened.
//!
//! A store that SQLite says is damaged is put aside whole, as `store-put-aside-{time}`, never
//! deleted, and nothing opens again until a backup is brought back or the phone starts over.

pub mod calls;
pub mod commands;
pub mod dispatch;
pub mod mode;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use nocturne_contract::{HomeError, PhoneMode};
use nocturne_engine::{Clock, Engine};
use serde::{Deserialize, Serialize};

use crate::sync::ledger::write_private;

const STORE: &str = "store";
const PUT_ASIDE: &str = "store-put-aside-";
const DAMAGED: &str = "store-damaged.json";
const LAST_BACKUP: &str = "last-backup.json";
const STAGE: &str = "restore-stage";
const CHECK: &str = "restore-check";
const KEEPING: &str = "keeping";
/// The device name a store brought back on a fresh phone takes when nothing else names it. Only
/// the store sees it: a phone alone shows no device names.
const FALLBACK_NAME: &str = "Phone";

/// The store, opened, and this phone's own row in it.
#[derive(Clone)]
pub struct Open {
    pub engine: Engine,
    pub device: String,
}

/// Marks a store that could not be opened, until a backup comes back or the phone starts over.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Damaged {
    /// The folder the damaged store was moved to; none when it could not be moved.
    put_aside: Option<String>,
}

/// Marks the system's own backup off for a folder (iCloud on an iPhone).
pub type ExcludeFromBackup = Box<dyn Fn(&Path) + Send + Sync>;

pub struct Home {
    dir: PathBuf,
    clock: Arc<dyn Clock>,
    open: Mutex<Option<Open>>,
    /// A backup, check or restore under way: one at a time.
    job: AtomicBool,
    exclude: ExcludeFromBackup,
    /// The passphrase work a backup asks of whoever opens it; `None` is what Nocturne writes.
    work_factor: Option<u8>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// SQLite's own word that the file is not a database it can read.
fn is_damage(e: &nocturne_engine::Error) -> bool {
    use rusqlite::ErrorCode::{DatabaseCorrupt, NotADatabase};
    matches!(
        e,
        nocturne_engine::Error::Store(rusqlite::Error::SqliteFailure(f, _))
            if matches!(f.code, DatabaseCorrupt | NotADatabase)
    )
}

/// Releases the one job however the call ends.
pub(crate) struct Job<'a>(&'a AtomicBool);

impl Drop for Job<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Home {
    pub fn new(dir: PathBuf, clock: Arc<dyn Clock>, exclude: ExcludeFromBackup) -> Home {
        Home {
            dir,
            clock,
            open: Mutex::new(None),
            job: AtomicBool::new(false),
            exclude,
            work_factor: None,
        }
    }

    pub(crate) fn store_dir(&self) -> PathBuf {
        self.dir.join(STORE)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub(crate) fn keeping_dir(&self) -> PathBuf {
        self.dir.join(KEEPING)
    }

    pub(crate) fn begin_job(&self) -> Result<Job<'_>, HomeError> {
        if self.job.swap(true, Ordering::SeqCst) {
            return Err(HomeError::NotReady);
        }
        Ok(Job(&self.job))
    }

    /// The open store and this phone's row in it, for the sync. See the module notes.
    pub fn local(&self) -> Option<Open> {
        self.open_store().ok()
    }

    /// The store, opened now if it is not open yet. Only while the phone is alone.
    pub(crate) fn open_store(&self) -> Result<Open, HomeError> {
        let mut slot = lock(&self.open);
        if let Some(open) = slot.as_ref() {
            return Ok(open.clone());
        }
        let Some(file) = self.mode_file().filter(|f| f.mode == PhoneMode::Alone) else {
            return Err(HomeError::NotReady);
        };
        if self.damaged().is_some() {
            return Err(HomeError::NotReady);
        }
        let store = self.store_dir();
        let engine = match Engine::open(&store, self.clock.clone()) {
            Ok(e) => e,
            Err(e) if is_damage(&e) => {
                tracing::warn!("the memory on this phone could not be opened: {e}");
                self.put_aside_damaged();
                return Err(HomeError::NotReady);
            }
            Err(e) => {
                tracing::warn!("the memory on this phone did not open: {e}");
                return Err(HomeError::NotReady);
            }
        };
        (self.exclude)(&store);
        let device = engine.ensure_local_device(&file.name).map_err(|e| {
            tracing::warn!("this phone's own row could not be made: {e}");
            HomeError::Failed
        })?;
        let open = Open { engine, device };
        *slot = Some(open.clone());
        Ok(open)
    }

    /// Let go of the open store. A call already holding it finishes on the files it had.
    pub(crate) fn close(&self) {
        lock(&self.open).take();
    }

    /// The folder a damaged store was put aside in, while the phone waits for a backup or a
    /// fresh start. `Some("")` when it could not even be moved.
    pub fn damaged(&self) -> Option<String> {
        let bytes = std::fs::read(self.path(DAMAGED)).ok()?;
        let d: Damaged = serde_json::from_slice(&bytes).unwrap_or_default();
        Some(d.put_aside.unwrap_or_default())
    }

    fn put_aside_damaged(&self) {
        let put_aside = match self.put_aside_store() {
            Ok(name) => name,
            Err(e) => {
                tracing::warn!("the damaged memory could not be put aside: {e}");
                None
            }
        };
        let bytes = serde_json::to_vec(&Damaged { put_aside }).expect("the mark serialises");
        if let Err(e) = write_private(&self.path(DAMAGED), &bytes) {
            tracing::warn!("could not mark the memory as damaged: {e}");
        }
    }

    pub(crate) fn clear_damaged(&self) {
        let _ = std::fs::remove_file(self.path(DAMAGED));
    }

    /// Move the store to a folder of its own beside it, named for now. Answers that folder's
    /// name, or none when there was no store.
    pub(crate) fn put_aside_store(&self) -> std::io::Result<Option<String>> {
        let store = self.store_dir();
        if !store.exists() {
            return Ok(None);
        }
        let stamp = self
            .clock
            .now()
            .to_zoned(self.clock.tz())
            .strftime("%Y-%m-%d-%H%M%S")
            .to_string();
        let mut name = format!("{PUT_ASIDE}{stamp}");
        let mut n = 1;
        while self.path(&name).exists() {
            n += 1;
            name = format!("{PUT_ASIDE}{stamp}-{n}");
        }
        std::fs::rename(&store, self.path(&name))?;
        Ok(Some(name))
    }

    /// Every store put aside on this phone, by folder name.
    pub(crate) fn put_aside_names(&self) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(PUT_ASIDE))
            .collect();
        out.sort();
        out
    }

    /// Whether a file at `path` would land among the app's own files: beside them, or in the
    /// store, a copy of it or a folder of work. A file the screens name never goes there.
    pub(crate) fn among_own_files(&self, path: &Path) -> bool {
        let (Some(Ok(parent)), Ok(dir)) = (
            path.parent().map(dunce::canonicalize),
            dunce::canonicalize(&self.dir),
        ) else {
            return false;
        };
        let Ok(rest) = parent.strip_prefix(&dir) else {
            return false;
        };
        match rest.components().next() {
            None => true,
            Some(first) => {
                let first = first.as_os_str().to_string_lossy();
                first.starts_with(STORE) || [STAGE, CHECK, KEEPING].contains(&first.as_ref())
            }
        }
    }

    pub(crate) fn stage_dir(&self) -> PathBuf {
        self.path(STAGE)
    }

    pub(crate) fn check_dir(&self) -> PathBuf {
        self.path(CHECK)
    }

    pub(crate) fn last_backup(&self) -> Option<nocturne_contract::LastBackup> {
        let bytes = std::fs::read(self.path(LAST_BACKUP)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub(crate) fn note_backup(&self, bytes: f64) {
        let last = nocturne_contract::LastBackup {
            at: self.clock.now().to_string(),
            bytes,
        };
        let json = serde_json::to_vec(&last).expect("the note serialises");
        if let Err(e) = write_private(&self.path(LAST_BACKUP), &json) {
            tracing::warn!("could not note the backup: {e}");
        }
    }

    pub(crate) fn forget_backup_note(&self) {
        let _ = std::fs::remove_file(self.path(LAST_BACKUP));
    }

    #[cfg(test)]
    pub(crate) fn set_work_factor(&mut self, w: u8) {
        self.work_factor = Some(w);
    }
}
