//! What only a phone that is home is asked: finding by words, forgetting, backups, export,
//! deleting everything and what the memory holds. Every function here blocks: call it off the
//! async workers.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use nocturne_channel::crypto::check_id;
use nocturne_channel::protocol::MAX_QUESTION;
use nocturne_contract::{
    BackupPhase, Holdings, HomeAnswer, HomeCall, HomeError, HomeStats, MemoryCounts, PhoneMode,
    RestoreCheck,
};
use nocturne_engine::{BackupRun, DELETE_PHRASE, Engine, FIND_CAP};

use super::mode::ModeFile;
use super::{Home, io_full, is_full};

pub type Progress<'a> = &'a (dyn Fn(BackupPhase, u64, u64) + Sync);

/// Remove a folder of scratch or of copies, if it is there.
fn clear(path: &Path) {
    match std::fs::remove_dir_all(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!("a folder could not be removed: {e}"),
    }
}

fn error(code: HomeError) -> HomeAnswer {
    HomeAnswer::Error { code }
}

pub(crate) fn code(e: &nocturne_engine::Error) -> HomeError {
    use nocturne_engine::Error;
    match e {
        Error::NotFound(_) => HomeError::NotFound,
        Error::Invalid(_) => HomeError::Invalid,
        Error::StaleToken | Error::TooLate => HomeError::TooLate,
        Error::WrongPassphrase => HomeError::WrongPassphrase,
        Error::NotABackup => HomeError::NotABackup,
        Error::BackupTooNew => HomeError::NewerBackup,
        Error::BackupDamaged => HomeError::DamagedBackup,
        e if is_full(e) => HomeError::Full,
        _ => HomeError::Failed,
    }
}

impl Home {
    /// Answer one home call. `paired`: the phone holds a pairing with a computer, which keeps
    /// a backup from being brought back here.
    pub fn local_call(&self, call: HomeCall, paired: bool, progress: Progress<'_>) -> HomeAnswer {
        let result = match call {
            HomeCall::Find { query } => self.find(&query),
            HomeCall::PreviewForget { scope } => self.using().and_then(|(_use, o)| {
                o.engine
                    .preview_delete(
                        &scope.ids,
                        scope.include_originals,
                        &scope.also.unwrap_or_default(),
                        scope.pick.unwrap_or(false),
                    )
                    .map(|preview| HomeAnswer::ForgetPreview { preview })
                    .map_err(|e| code(&e))
            }),
            HomeCall::Forget { token } => self.using().and_then(|(_use, o)| {
                o.engine
                    .forget(&token)
                    .map(|outcome| HomeAnswer::Forgotten {
                        report: outcome.report,
                    })
                    .map_err(|e| code(&e))
            }),
            HomeCall::SaveBackup { dest, passphrase } => {
                self.save_backup(Path::new(&dest), &passphrase, progress)
            }
            HomeCall::CheckBackup { file, passphrase } => {
                self.check_backup(Path::new(&file), &passphrase, paired, progress)
            }
            HomeCall::RestoreBackup {
                file,
                passphrase,
                replace,
            } => self.restore_backup(Path::new(&file), &passphrase, replace, paired, progress),
            HomeCall::DeleteEverything { phrase } => self.delete_everything(&phrase),
            HomeCall::Export { dest } => self.using().and_then(|(_use, o)| {
                let dest = Path::new(&dest);
                if !dest.is_dir() || self.among_own_files(&dest.join("export")) {
                    return Err(HomeError::Invalid);
                }
                o.engine
                    .export_to(dest)
                    .map(|report| HomeAnswer::Exported { report })
                    .map_err(|e| code(&e))
            }),
            HomeCall::Stats => self.stats().map(|stats| HomeAnswer::Stats { stats }),
            HomeCall::SetPrivate { id, private } => self.set_private(&id, private),
        };
        result.unwrap_or_else(error)
    }

    fn find(&self, query: &str) -> Result<HomeAnswer, HomeError> {
        let (_use, open) = self.using()?;
        let query = query.trim();
        if query.len() > MAX_QUESTION {
            return Err(HomeError::Invalid);
        }
        if query.is_empty() {
            return Ok(HomeAnswer::Found { items: Vec::new() });
        }
        let found = open
            .engine
            .find_items(query, FIND_CAP)
            .map_err(|e| code(&e))?;
        Ok(HomeAnswer::Found {
            items: found.into_iter().map(Into::into).collect(),
        })
    }

    fn save_backup(
        &self,
        dest: &Path,
        passphrase: &str,
        progress: Progress<'_>,
    ) -> Result<HomeAnswer, HomeError> {
        let (_use, open) = self.using()?;
        if self.among_own_files(dest) {
            return Err(HomeError::Invalid);
        }
        let _job = self.begin_job()?;
        let cancel = AtomicBool::new(false);
        let run = BackupRun {
            extras: &[],
            progress,
            cancel: &cancel,
            started: &|_| {},
            work_factor: self.work_factor,
        };
        let report = open
            .engine
            .backup_to(dest, passphrase, &run)
            .map_err(|e| code(&e))?;
        self.note_backup(report.bytes);
        Ok(HomeAnswer::BackupSaved { report })
    }

    /// Whether the phone holds anything of the person's now. A store that cannot be opened
    /// counts as holding something, so nothing is replaced on a guess.
    fn here(&self, made_after: Option<&str>) -> Option<Holdings> {
        if let Ok(open) = self.open_store() {
            return open
                .engine
                .holdings(made_after)
                .unwrap_or_else(|_| Some(Holdings::default()));
        }
        let store = self.store_dir();
        if !store.exists() {
            return None;
        }
        match nocturne_engine::store_has_data(&store) {
            Ok(false) => None,
            _ => Some(Holdings::default()),
        }
    }

    /// A backup may come back on a phone that is alone or has not chosen yet; a companion's
    /// memory is on its computer.
    fn may_restore(&self, paired: bool) -> Result<(), HomeError> {
        match self.mode(paired) {
            Some(PhoneMode::Companion) => Err(HomeError::NotReady),
            _ => Ok(()),
        }
    }

    fn check_backup(
        &self,
        file: &Path,
        passphrase: &str,
        paired: bool,
        progress: Progress<'_>,
    ) -> Result<HomeAnswer, HomeError> {
        self.may_restore(paired)?;
        if self.among_own_files(file) {
            return Err(HomeError::Invalid);
        }
        let _job = self.begin_job()?;
        let cancel = AtomicBool::new(false);
        let scratch = self.check_dir();
        clear(&scratch);
        let checked = nocturne_engine::check_backup(file, passphrase, &scratch, &progress, &cancel);
        clear(&scratch);
        let summary = checked.map_err(|e| code(&e))?;
        let here = {
            let _use = self.gate.read().unwrap_or_else(|e| e.into_inner());
            self.here(Some(&summary.made_at))
        };
        Ok(HomeAnswer::BackupChecked {
            check: Box::new(RestoreCheck {
                summary,
                here,
                put_aside: None,
            }),
        })
    }

    /// Unpack the backup beside the store, then put it in the store's place: what was here is
    /// put aside whole, never deleted. The phone is alone afterwards.
    fn restore_backup(
        &self,
        file: &Path,
        passphrase: &str,
        replace: bool,
        paired: bool,
        progress: Progress<'_>,
    ) -> Result<HomeAnswer, HomeError> {
        self.may_restore(paired)?;
        if self.among_own_files(file) {
            return Err(HomeError::Invalid);
        }
        let _job = self.begin_job()?;
        let holds = {
            let _use = self.gate.read().unwrap_or_else(|e| e.into_inner());
            self.here(None).is_some()
        };
        if holds && !replace {
            return Err(HomeError::Invalid);
        }
        let cancel = AtomicBool::new(false);
        let stage = self.stage_dir();
        clear(&stage);
        if let Err(e) = nocturne_engine::unpack_backup(file, passphrase, &stage, &progress, &cancel)
        {
            clear(&stage);
            return Err(code(&e));
        }
        let store = self.store_dir();
        // Every use ends first, and nothing opens the store while it changes places.
        let _only = self.exclusive();
        self.close();
        let moved = (|| {
            let holds = store.exists() && nocturne_engine::store_has_data(&store).unwrap_or(true);
            let aside = if holds {
                self.put_aside_store()?
            } else {
                if store.exists() {
                    // A fresh store with nothing of the person's in it: only this phone's row.
                    std::fs::remove_dir_all(&store)?;
                }
                None
            };
            std::fs::rename(stage.join("store"), &store).inspect_err(|_| {
                if let Some(aside) = &aside
                    && std::fs::rename(self.path(aside), &store).is_err()
                {
                    // The memory is safe but not in place: say so, never open an empty one.
                    self.mark_damaged(Some(aside.clone()));
                }
            })
        })();
        clear(&stage);
        if let Err(e) = moved {
            tracing::warn!("the backup could not be put in place: {e}");
            return Err(if io_full(&e) {
                HomeError::Full
            } else {
                HomeError::Failed
            });
        }
        self.clear_damaged();
        self.forget_backup_note();
        self.forget_fading_note();
        let in_store = Engine::open(&store, self.clock.clone())
            .ok()
            .and_then(|e| e.local_device().ok().flatten())
            .map(|d| d.name);
        let name = self.name_for_store(in_store);
        self.save_mode(&ModeFile {
            mode: PhoneMode::Alone,
            name,
        })
        .map_err(|_| HomeError::Failed)?;
        self.open_store()?;
        Ok(HomeAnswer::Restored)
    }

    /// Everything kept goes, and so does every store put aside and what a restore left. The
    /// phone stays alone, with an empty memory and its own row made again.
    fn delete_everything(&self, phrase: &str) -> Result<HomeAnswer, HomeError> {
        if phrase != DELETE_PHRASE {
            return Err(HomeError::WrongPhrase);
        }
        let _job = self.begin_job()?;
        let _only = self.exclusive();
        let open = self.open_store()?;
        open.engine
            .delete_everything(phrase)
            .map_err(|e| code(&e))?;
        for name in self.put_aside_names() {
            clear(&self.path(&name));
        }
        clear(&self.stage_dir());
        clear(&self.check_dir());
        clear(&self.keeping_dir());
        self.forget_backup_note();
        self.forget_fading_note();
        // The phone's own row went with the rest: made again, the store left open.
        let device = open
            .engine
            .ensure_local_device(&self.name_for_store(None))
            .map_err(|e| code(&e))?;
        self.set_device(device);
        Ok(HomeAnswer::Deleted)
    }

    /// The same rule as the computer's private switch: a journal entry, a good thing and a health
    /// reading cannot be marked, and what was read from the item follows it.
    fn set_private(&self, id: &str, private: bool) -> Result<HomeAnswer, HomeError> {
        if check_id(id).is_err() {
            return Err(HomeError::Invalid);
        }
        let (_use, open) = self.using()?;
        open.engine
            .set_item_private(id, private)
            .map_err(|e| code(&e))?;
        let item = open.engine.get_item(id).map_err(|e| code(&e))?;
        Ok(HomeAnswer::PrivateSet {
            item: super::dispatch::phone_item(&item),
        })
    }

    fn stats(&self) -> Result<HomeStats, HomeError> {
        let (_use, open) = self.using()?;
        let e = &open.engine;
        let holdings = e.holdings(None).map_err(|x| code(&x))?.unwrap_or_default();
        let items = e.stats().map_err(|x| code(&x))?.item_count;
        Ok(HomeStats {
            counts: MemoryCounts {
                items,
                memories: holdings.memories,
                pages: holdings.pages,
                originals: holdings.originals,
            },
            bytes: e.backup_estimate().map_err(|x| code(&x))? as f64,
            last_kept: holdings.last_kept,
            last_backup: self.last_backup(),
        })
    }
}
