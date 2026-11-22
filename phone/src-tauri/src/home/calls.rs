//! What only a phone that is home is asked: finding by words, forgetting, backups, export,
//! deleting everything and what the memory holds. Every function here blocks: call it off the
//! async workers.

use std::path::Path;

use nocturne_channel::crypto::check_id;
use nocturne_channel::protocol::MAX_QUESTION;
use nocturne_contract::{
    BackupPhase, BackupReport, Holdings, HomeAnswer, HomeCall, HomeError, HomeStats, ItemKept,
    ItemKind, KeptCounts, KeptHow, MemoryCounts, MemoryFilter, PhoneDeleteScope, PhoneMode,
    PhoneSource, RestoreCheck, Source,
};
use nocturne_engine::{BackupRun, DELETE_PHRASE, Engine, FIND_CAP};

use super::mode::ModeFile;
use super::{Home, io_full, is_full};

pub type Progress<'a> = &'a (dyn Fn(BackupPhase, u64, u64) + Sync);

/// Remove a folder of scratch or of copies, if it is there.
pub(crate) fn clear(path: &Path) {
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
        Error::Cancelled => HomeError::Stopped,
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
            HomeCall::SaveBackup { dest, passphrase } => self
                .make_backup(Path::new(&dest), &passphrase, progress)
                .map(|report| {
                    self.note_backup(report.bytes);
                    HomeAnswer::BackupSaved { report }
                }),
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
            HomeCall::PutAside => Ok(HomeAnswer::PutAside {
                memories: self.put_aside_memories(),
            }),
            HomeCall::BringBack { id } => self.bring_back(&id),
            HomeCall::DeletePutAside { id } => self.delete_put_aside(&id),
            HomeCall::PreviewSourceDelete { source } => self.preview_source_delete(source),
            HomeCall::DeleteSource { token, .. } => self.using().and_then(|(_use, o)| {
                // The phone stops bringing a source by its own switch, never by the store's:
                // the store would turn away what the phone brings once it is on again.
                o.engine
                    .delete_phone_data(&token, false)
                    .map(|_| HomeAnswer::SourceDeleted)
                    .map_err(|e| code(&e))
            }),
            HomeCall::SetKeptHow { id, how } => self.set_kept_how(&id, how),
            HomeCall::ItemKept { id } => self.item_kept(&id),
            HomeCall::PreviewForgetItem { id, with_original } => {
                self.preview_forget_item(&id, with_original)
            }
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

    /// Write a locked backup to `dest`. It is not noted as the last backup: the caller does that
    /// once the file is where the person wanted it.
    pub(crate) fn make_backup(
        &self,
        dest: &Path,
        passphrase: &str,
        progress: Progress<'_>,
    ) -> Result<BackupReport, HomeError> {
        let (_use, open) = self.using()?;
        if self.among_own_files(dest) {
            return Err(HomeError::Invalid);
        }
        let _job = self.begin_job()?;
        let run = BackupRun {
            extras: &[],
            progress,
            cancel: &self.stop,
            started: &|_| {},
            work_factor: self.work_factor,
        };
        open.engine
            .backup_to(dest, passphrase, &run)
            .map_err(|e| code(&e))
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

    /// What the phone holds now in the replace columns; zero where the store cannot be read.
    fn here_counts(&self) -> KeptCounts {
        match self.open_store() {
            Ok(open) => open.engine.kept_counts().unwrap_or_default(),
            Err(_) => nocturne_engine::store_kept_counts(&self.store_dir()).unwrap_or_default(),
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
        let scratch = self.check_dir();
        clear(&scratch);
        let checked = nocturne_engine::check_backup_counted(
            file, passphrase, &scratch, &progress, &self.stop,
        );
        clear(&scratch);
        let (summary, backup) = checked.map_err(|e| code(&e))?;
        let (here, here_counts) = {
            let _use = self.gate.read().unwrap_or_else(|e| e.into_inner());
            let here = self.here(Some(&summary.made_at));
            let counts = here.as_ref().map(|_| self.here_counts());
            (here, counts)
        };
        Ok(HomeAnswer::BackupChecked {
            check: Box::new(RestoreCheck {
                summary,
                here,
                put_aside: self.newest_put_aside(),
            }),
            backup,
            here: here_counts,
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
        let stage = self.stage_dir();
        clear(&stage);
        if let Err(e) =
            nocturne_engine::unpack_backup(file, passphrase, &stage, &progress, &self.stop)
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
        // The memory has changed places: say so even if it does not open, and the next open
        // shows what is wrong.
        if let Err(code) = self.open_store() {
            tracing::warn!("the memory brought back did not open: {code:?}");
        }
        Ok(HomeAnswer::Restored)
    }

    /// Everything kept goes, and so does every store put aside and what a restore left. The
    /// store itself is removed and the phone has not chosen where its memory lives, so it opens
    /// on Welcome as a new phone would. The caller resets what the phone brings.
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
        drop(open);
        self.close();
        clear(&self.store_dir());
        for name in self.put_aside_names() {
            clear(&self.path(&name));
        }
        clear(&self.stage_dir());
        clear(&self.check_dir());
        clear(&self.keeping_dir());
        self.forget_backup_note();
        self.forget_fading_note();
        if self.store_dir().exists() || !self.put_aside_names().is_empty() {
            return Err(HomeError::Failed);
        }
        self.clear_damaged();
        // Last, so a stop part way through still opens on what is left, never on Welcome.
        self.forget_mode().map_err(|e| {
            tracing::warn!("could not forget where the memory lives: {e}");
            HomeError::Failed
        })?;
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

    fn set_kept_how(&self, id: &str, how: KeptHow) -> Result<HomeAnswer, HomeError> {
        if check_id(id).is_err() || matches!(how, KeptHow::Calendar | KeptHow::Health) {
            return Err(HomeError::Invalid);
        }
        let word = match how {
            KeptHow::Typed => "typed",
            KeptHow::Taken => "taken",
            KeptHow::Chosen => "chosen",
            KeptHow::Recorded => "recorded",
            _ => "shared",
        };
        let (_use, open) = self.using()?;
        open.engine
            .set_kept_how(id, word)
            .map(|()| HomeAnswer::KeptHowSet)
            .map_err(|e| code(&e))
    }

    fn item_kept(&self, id: &str) -> Result<HomeAnswer, HomeError> {
        if check_id(id).is_err() {
            return Err(HomeError::Invalid);
        }
        let (_use, open) = self.using()?;
        let e = &open.engine;
        let item = e.get_item(id).map_err(|x| code(&x))?;
        let noted = e.kept_how(id).map_err(|x| code(&x))?;
        let here = e
            .local_device()
            .ok()
            .flatten()
            .map(|d| d.name)
            .unwrap_or_default();
        let device = item.from_device.clone().filter(|d| *d != here);
        let how = match (item.kind, noted.as_deref()) {
            (ItemKind::Calendar, _) => Some(KeptHow::Calendar),
            (ItemKind::Reading, _) => Some(KeptHow::Health),
            _ if device.is_some() => None,
            (_, Some("typed")) => Some(KeptHow::Typed),
            (_, Some("taken")) => Some(KeptHow::Taken),
            (_, Some("chosen")) => Some(KeptHow::Chosen),
            (_, Some("recorded")) => Some(KeptHow::Recorded),
            (_, Some("shared")) => Some(KeptHow::Shared),
            // A link shared on this phone before keeps noted how; one pasted on a computer and
            // restored here was not kept on this phone at all.
            _ if item.source == Source::PastedWeb && item.from_device.is_some() => {
                Some(KeptHow::Shared)
            }
            _ => None,
        };
        Ok(HomeAnswer::ItemKept {
            kept: ItemKept {
                how,
                device,
                worked_out: item.memory_count > 0,
            },
        })
    }

    fn preview_source_delete(&self, source: PhoneSource) -> Result<HomeAnswer, HomeError> {
        let (_use, open) = self.using()?;
        open.engine
            .preview_phone_delete(PhoneDeleteScope {
                device_id: open.device.clone(),
                source,
                kinds: None,
                before: None,
                item_id: None,
            })
            .map(|preview| HomeAnswer::SourceDeletePreview { preview })
            .map_err(|e| code(&e))
    }

    /// "Forget…" takes what was learned from a thing and keeps the thing; "Delete…" takes the
    /// thing and its original too. `NotFound` when there is nothing to take.
    fn preview_forget_item(&self, id: &str, with_original: bool) -> Result<HomeAnswer, HomeError> {
        if check_id(id).is_err() {
            return Err(HomeError::Invalid);
        }
        let (_use, open) = self.using()?;
        let e = &open.engine;
        e.get_item(id).map_err(|x| code(&x))?;
        let learned: Vec<String> = e
            .memories(MemoryFilter {
                entity_id: None,
                item_id: Some(id.to_string()),
                status: None,
                query: None,
                limit: None,
            })
            .map_err(|x| code(&x))?
            .into_iter()
            .map(|m| m.id)
            .collect();
        let also = if with_original {
            vec![id.to_string()]
        } else {
            Vec::new()
        };
        if learned.is_empty() && also.is_empty() {
            return Err(HomeError::NotFound);
        }
        e.preview_delete(&learned, with_original, &also, false)
            .map(|preview| HomeAnswer::ForgetPreview { preview })
            .map_err(|x| code(&x))
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
