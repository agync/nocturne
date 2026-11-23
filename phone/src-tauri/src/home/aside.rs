//! Memories put aside on this phone: listed with when they were put aside, how much room they
//! take and what they hold; one brought back in place of what is here, which is put aside in
//! turn so nothing is lost; or one deleted, which only the person asks for.

use std::path::Path;

use nocturne_contract::{HomeAnswer, HomeError, PhoneMode, PutAsideMemory};

use super::calls::clear;
use super::{Home, PUT_ASIDE, io_full};

/// The bytes a folder takes, every file below it counted.
fn folder_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => folder_bytes(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

impl Home {
    /// When a memory was put aside, from its folder's name, written in the phone's time zone.
    pub(super) fn put_aside_at(&self, name: &str) -> Option<String> {
        let stamp = name.strip_prefix(PUT_ASIDE)?;
        let stamp = stamp.get(..17)?;
        let civil = jiff::civil::DateTime::strptime("%Y-%m-%d-%H%M%S", stamp).ok()?;
        let at = civil.to_zoned(self.clock.tz()).ok()?.timestamp();
        Some(at.to_string())
    }

    /// When the newest memory put aside was put aside, for 0013's "Already put aside" line.
    pub(crate) fn newest_put_aside(&self) -> Option<String> {
        self.put_aside_names()
            .iter()
            .rev()
            .find_map(|n| self.put_aside_at(n))
    }

    /// Every memory put aside, newest first.
    pub(crate) fn put_aside_memories(&self) -> Vec<PutAsideMemory> {
        let mut out: Vec<PutAsideMemory> = self
            .put_aside_names()
            .into_iter()
            .filter_map(|id| {
                let at = self.put_aside_at(&id)?;
                let dir = self.path(&id);
                Some(PutAsideMemory {
                    at,
                    bytes: folder_bytes(&dir) as f64,
                    counts: nocturne_engine::store_kept_counts(&dir).ok(),
                    id,
                })
            })
            .collect();
        out.reverse();
        out
    }

    /// A name the screens sent, if it is one of the memories put aside.
    fn put_aside_named(&self, id: &str) -> Result<String, HomeError> {
        self.put_aside_names()
            .into_iter()
            .find(|n| n == id)
            .ok_or(HomeError::NotFound)
    }

    /// Bring a memory put aside back in place of what is here. What is here is put aside in
    /// turn, never deleted. One that cannot be read stays where it is: `DamagedBackup`.
    pub(crate) fn bring_back(&self, id: &str) -> Result<HomeAnswer, HomeError> {
        if self.mode_file().map(|f| f.mode) != Some(PhoneMode::Alone) {
            return Err(HomeError::NotReady);
        }
        let name = self.put_aside_named(id)?;
        let from = self.path(&name);
        if nocturne_engine::store_kept_counts(&from).is_err() {
            return Err(HomeError::DamagedBackup);
        }
        let _job = self.begin_job()?;
        let store = self.store_dir();
        let _only = self.exclusive();
        self.close();
        let moved = (|| {
            let holds = store.exists() && nocturne_engine::store_has_data(&store).unwrap_or(true);
            let aside = if holds {
                self.put_aside_store()?
            } else {
                if store.exists() {
                    std::fs::remove_dir_all(&store)?;
                }
                None
            };
            std::fs::rename(&from, &store).inspect_err(|_| {
                if let Some(aside) = &aside
                    && std::fs::rename(self.path(aside), &store).is_err()
                {
                    self.mark_damaged(Some(aside.clone()));
                }
            })
        })();
        if let Err(e) = moved {
            tracing::warn!("what was put aside could not be brought back: {e}");
            return Err(if io_full(&e) {
                HomeError::Full
            } else {
                HomeError::Failed
            });
        }
        self.clear_damaged();
        self.forget_backup_note();
        self.forget_fading_note();
        // The memory has changed places: say so even if it does not open, and the next open
        // shows what is wrong.
        if let Err(code) = self.open_store() {
            tracing::warn!("the memory brought back did not open: {code:?}");
        }
        Ok(HomeAnswer::Restored)
    }

    pub(crate) fn delete_put_aside(&self, id: &str) -> Result<HomeAnswer, HomeError> {
        let name = self.put_aside_named(id)?;
        let _job = self.begin_job()?;
        let dir = self.path(&name);
        clear(&dir);
        if dir.exists() {
            return Err(HomeError::Failed);
        }
        Ok(HomeAnswer::PutAsideDeleted)
    }
}
