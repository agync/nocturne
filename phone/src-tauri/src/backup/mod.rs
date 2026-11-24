//! The nightly backup on a phone alone (ADR 0019): each night while the phone charges, what
//! changed goes to the person's own account, locked with their phrase. The engine does the work
//! ([`nocturne_engine::nightly`]); this holds what the phone keeps between nights and decides when
//! a night runs.
//!
//! The account is reached through a [`BackupPlace`] the phone's own code sets
//! ([`PhoneBackup::set_place`]): iCloud on an iPhone, Google Drive on Android. Until one is set,
//! nothing runs and the screens hear `unavailable`. The backup's key is held in the phone's safe
//! place ([`BackupKeys`]), so a night needs no phrase; the phrase and a recovery key lock it in the
//! account. Whether the phone is on Wi-Fi is told from outside ([`PhoneBackup::set_unmetered`]);
//! until it is, "Wi-Fi only" waits.

pub mod commands;
pub mod place_account;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use nocturne_contract::{BackupPhase, BackupSummary, HomeError, KeptCounts, PhoneMode};
use nocturne_engine::Clock;
use nocturne_engine::nightly::{
    self, BackupPlace, ExposeSecret, NightError, NightKey, NightReport, NightRun, NightState,
    PlaceError,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_nocturne_system::SystemExt;

use crate::home::Home;
use crate::sync::ledger::write_private;

/// What the phone keeps of the backup, beside its key.
const FILE: &str = "nightly-backup.json";
/// The key's name in the safe place.
const SECRET: &str = "backup";
/// A night is due once the last one finished this many hours ago.
const NIGHT_HOURS: i64 = 20;
/// After a night that did not finish, the next look waits this many minutes.
const RETRY_MINUTES: i64 = 60;
/// After a night that waited for a forget to finish, the next look waits this many minutes.
const BUSY_MINUTES: i64 = 2;

const UNKNOWN: u8 = 0;
const UNMETERED: u8 = 1;
const METERED: u8 = 2;

/// Where the backup's key is held.
pub trait BackupKeys: Send + Sync {
    /// False when it could not be kept.
    fn keep(&self, key: &str) -> bool;
    fn key(&self) -> Option<String>;
    fn forget(&self);
}

/// The key in this process's memory only, for tests.
#[derive(Default)]
pub struct MemoryBackupKeys(Mutex<Option<String>>);

impl BackupKeys for MemoryBackupKeys {
    fn keep(&self, key: &str) -> bool {
        *lock(&self.0) = Some(key.to_string());
        true
    }

    fn key(&self) -> Option<String> {
        lock(&self.0).clone()
    }

    fn forget(&self) {
        lock(&self.0).take();
    }
}

/// The key in the phone's safe place, as the mind's key is held.
pub struct SafeBackupKeys<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> BackupKeys for SafeBackupKeys<R> {
    fn keep(&self, key: &str) -> bool {
        match self.0.system().keep_secret(SECRET, key) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("the backup's key could not be kept in the safe place: {e:?}");
                false
            }
        }
    }

    fn key(&self) -> Option<String> {
        match self.0.system().secret(SECRET) {
            Ok(key) => key,
            Err(e) => {
                tracing::warn!("the phone's safe place could not be read: {e:?}");
                None
            }
        }
    }

    fn forget(&self) {
        if let Err(e) = self.0.system().forget_secret(SECRET) {
            tracing::warn!("the backup's key could not be removed from the safe place: {e:?}");
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// When the last night finished and what the account holds for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastNight {
    pub at: String,
    pub bytes: u64,
}

/// What stopped the nights until the person or the account changes something.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "trouble", rename_all = "camelCase")]
pub enum Trouble {
    NoRoom,
    /// Signed out of the account, or it left the phone: "Choose account" or signing in again.
    SignedOut,
    /// Nocturne's permission was taken back, or iCloud is off for Nocturne.
    NoPermission,
    #[serde(rename_all = "camelCase")]
    AnotherPhone {
        made_at: String,
    },
    Newer,
    Damaged,
    Failed,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    on: bool,
    wifi_only: bool,
    state: Option<NightState>,
    last: Option<LastNight>,
    trouble: Option<Trouble>,
    /// The person asked for the backup in the account to go; it goes once the account answers.
    delete_pending: bool,
    /// The memory moved to a computer: no more nights, and the key stays only so the backup
    /// can be deleted later (`docs/design/phone-backup.md`, "When the memory moves to a
    /// computer").
    #[serde(default)]
    moved: bool,
}

/// How the backup stands, as This phone shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum BackupState {
    Off,
    /// This phone cannot reach an account yet.
    Unavailable,
    Running,
    UpToDate,
    Waiting,
    WaitingForWifi,
    /// Restored here; waits for "Make this phone the one in use".
    NotInUse,
    NoRoom,
    SignedOut,
    NoPermission,
    #[serde(rename_all = "camelCase")]
    AnotherPhone {
        made_at: String,
    },
    Newer,
    Damaged,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    pub on: bool,
    pub wifi_only: bool,
    pub last: Option<LastNight>,
    pub state: BackupState,
    /// The memory moved to a computer; a backup kept in the account is "Your backup from
    /// {day}", there only to be deleted.
    pub moved: bool,
}

/// Why a backup call did not do what was asked, as the screens name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupProblem {
    /// No account can be reached from this phone yet.
    Unavailable,
    /// The memory is not open here, or another backup or restore is running.
    NotReady,
    /// Backup is not on.
    Off,
    /// "Wi-Fi only" is on and the phone is not on Wi-Fi.
    WaitingForWifi,
    NoPhrase,
    WrongPhrase,
    NoBackup,
    AlreadyThere,
    AnotherPhone,
    NotInUse,
    Newer,
    Damaged,
    NoRoom,
    SignedOut,
    NoPermission,
    Offline,
    Stopped,
    Busy,
    /// The phone's safe place did not take the key.
    NoSafePlace,
    /// A restore would replace a memory on this phone without being told to.
    HoldsMemory,
    Failed,
}

impl From<NightError> for BackupProblem {
    fn from(e: NightError) -> Self {
        match e {
            NightError::WrongPhrase => BackupProblem::WrongPhrase,
            NightError::NoBackup => BackupProblem::NoBackup,
            NightError::AlreadyThere => BackupProblem::AlreadyThere,
            NightError::AnotherPhone { .. } => BackupProblem::AnotherPhone,
            NightError::NotInUse => BackupProblem::NotInUse,
            NightError::TooNew => BackupProblem::Newer,
            NightError::Damaged => BackupProblem::Damaged,
            NightError::NoPhrase => BackupProblem::NoPhrase,
            NightError::Place(PlaceError::NoRoom) => BackupProblem::NoRoom,
            NightError::Place(PlaceError::SignedOut) => BackupProblem::SignedOut,
            NightError::Place(PlaceError::NoPermission) => BackupProblem::NoPermission,
            NightError::Place(PlaceError::Offline) => BackupProblem::Offline,
            NightError::Place(PlaceError::Failed(_)) => BackupProblem::Failed,
            NightError::Cancelled => BackupProblem::Stopped,
            NightError::Busy => BackupProblem::Busy,
            NightError::Failed(_) => BackupProblem::Failed,
        }
    }
}

impl From<HomeError> for BackupProblem {
    fn from(e: HomeError) -> Self {
        match e {
            HomeError::NotReady => BackupProblem::NotReady,
            HomeError::Full => BackupProblem::NoRoom,
            HomeError::Stopped => BackupProblem::Stopped,
            _ => BackupProblem::Failed,
        }
    }
}

/// What a restore did.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Restored {
    /// An empty phone: the backup is the memory here now.
    Replaced {
        summary: BackupSummary,
        kept: KeptCounts,
    },
    /// A phone that held a memory: the backup's things joined it.
    #[serde(rename_all = "camelCase")]
    BroughtIn {
        summary: BackupSummary,
        things: u32,
        entries: u32,
        checkins: u32,
        /// On both, so kept once.
        same: u32,
        already: u32,
    },
}

/// What the account shows before the phrase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundBackup {
    pub at: Option<String>,
    pub bytes: u64,
}

fn open_with(place: &dyn BackupPlace, opening: &Opening) -> Result<NightKey, BackupProblem> {
    Ok(match opening {
        Opening::Phrase(p) => nightly::open_with_phrase(place, p)?,
        Opening::RecoveryKey(k) => nightly::open_with_recovery_key(place, k)?,
    })
}

/// How the backup is opened on a new phone.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "with", content = "text", rename_all = "camelCase")]
pub enum Opening {
    Phrase(String),
    RecoveryKey(String),
}

pub struct PhoneBackup {
    dir: PathBuf,
    keys: Box<dyn BackupKeys>,
    clock: Arc<dyn Clock>,
    place: RwLock<Option<Arc<dyn BackupPlace>>>,
    unmetered: AtomicU8,
    /// A night is under way.
    running: AtomicBool,
    /// When the last night that did not finish stopped, so a full or unreachable account is not
    /// asked again at every look.
    failed_at: Mutex<Option<(jiff::Timestamp, i64)>>,
    /// Held while the saved file is read and written.
    saving: Mutex<()>,
    /// The passphrase work the locked key asks for; `None` is what Nocturne writes.
    work_factor: Option<u8>,
}

/// One night at a time, released when dropped.
struct Running<'a>(&'a AtomicBool);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl PhoneBackup {
    pub fn new(dir: PathBuf, keys: Box<dyn BackupKeys>, clock: Arc<dyn Clock>) -> Self {
        PhoneBackup {
            dir,
            keys,
            clock,
            place: RwLock::new(None),
            unmetered: AtomicU8::new(UNKNOWN),
            running: AtomicBool::new(false),
            failed_at: Mutex::new(None),
            saving: Mutex::new(()),
            work_factor: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_work_factor(&mut self, w: u8) {
        self.work_factor = Some(w);
    }

    /// The account the backup goes to, once the phone can reach one; `None` takes it away.
    pub fn set_place(&self, place: Option<Arc<dyn BackupPlace>>) {
        *self.place.write().unwrap_or_else(|e| e.into_inner()) = place;
    }

    /// Whether the phone is on Wi-Fi or another network that is not metered; `None` when the
    /// phone cannot say.
    pub fn set_unmetered(&self, unmetered: Option<bool>) {
        let v = match unmetered {
            None => UNKNOWN,
            Some(true) => UNMETERED,
            Some(false) => METERED,
        };
        self.unmetered.store(v, Ordering::SeqCst);
    }

    fn place(&self) -> Result<Arc<dyn BackupPlace>, BackupProblem> {
        self.place
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or(BackupProblem::Unavailable)
    }

    fn path(&self) -> PathBuf {
        self.dir.join(FILE)
    }

    fn saved(&self) -> Saved {
        std::fs::read(self.path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn update<T>(&self, f: impl FnOnce(&mut Saved) -> T) -> T {
        let _held = lock(&self.saving);
        let mut saved = self.saved();
        let out = f(&mut saved);
        let json = serde_json::to_vec(&saved).expect("the backup's state serialises");
        if let Err(e) = write_private(&self.path(), &json) {
            tracing::warn!("the backup's state could not be saved: {e}");
        }
        out
    }

    fn key(&self) -> Result<NightKey, BackupProblem> {
        let text = self.keys.key().ok_or(BackupProblem::Off)?;
        NightKey::from_secret(&text).map_err(|_| BackupProblem::Damaged)
    }

    fn keep_key(&self, key: &NightKey) -> Result<(), BackupProblem> {
        if self.keys.keep(key.secret().expose_secret()) {
            Ok(())
        } else {
            Err(BackupProblem::NoSafePlace)
        }
    }

    /// Whether backup is on, so the phone asks for time in the background.
    pub fn is_on(&self) -> bool {
        let saved = self.saved();
        saved.on || saved.delete_pending
    }

    /// Whether the nights wait for Wi-Fi, so the phone opens their background windows only there.
    /// A deletion still owed to the account goes on any network.
    pub fn wifi_only(&self) -> bool {
        let saved = self.saved();
        saved.on && saved.wifi_only && !saved.delete_pending
    }

    fn waits_for_wifi(&self, saved: &Saved) -> bool {
        saved.wifi_only && self.unmetered.load(Ordering::SeqCst) != UNMETERED
    }

    fn recent(&self, last: &Option<LastNight>) -> bool {
        let Some(at) = last
            .as_ref()
            .and_then(|l| l.at.parse::<jiff::Timestamp>().ok())
        else {
            return false;
        };
        self.clock.now().duration_since(at) < jiff::SignedDuration::from_hours(NIGHT_HOURS)
    }

    /// How the backup stands now.
    pub fn status(&self) -> BackupStatus {
        let saved = self.saved();
        let state = if !saved.on {
            BackupState::Off
        } else if self.place().is_err() {
            BackupState::Unavailable
        } else if self.running.load(Ordering::SeqCst) {
            BackupState::Running
        } else if let Some(trouble) = &saved.trouble {
            match trouble.clone() {
                Trouble::NoRoom => BackupState::NoRoom,
                Trouble::SignedOut => BackupState::SignedOut,
                Trouble::NoPermission => BackupState::NoPermission,
                Trouble::AnotherPhone { made_at } => BackupState::AnotherPhone { made_at },
                Trouble::Newer => BackupState::Newer,
                Trouble::Damaged => BackupState::Damaged,
                Trouble::Failed => BackupState::Failed,
            }
        } else if saved.state.as_ref().is_some_and(|s| !s.in_use) {
            BackupState::NotInUse
        } else if self.waits_for_wifi(&saved) {
            BackupState::WaitingForWifi
        } else if self.recent(&saved.last) {
            BackupState::UpToDate
        } else {
            BackupState::Waiting
        };
        BackupStatus {
            on: saved.on,
            wifi_only: saved.wifi_only,
            last: saved.last,
            state,
            moved: saved.moved,
        }
    }

    /// A new phrase of Nocturne's for step 2.
    pub fn new_phrase(&self) -> String {
        nightly::make_phrase(&nocturne_channel::protocol::WORDS)
    }

    /// "Turn on": a new backup in the account, locked with `phrase`. The first night runs next
    /// ([`PhoneBackup::back_up_now`]).
    pub fn turn_on(&self, phrase: &str) -> Result<(), BackupProblem> {
        let place = self.place()?;
        let key = nightly::turn_on(place.as_ref(), phrase, self.work_factor)?;
        if let Err(problem) = self.keep_key(&key) {
            let _ = place.delete(nightly::KEY_PHRASE);
            return Err(problem);
        }
        self.update(|s| {
            s.on = true;
            s.state = Some(NightState::new());
            s.last = None;
            s.trouble = None;
            s.delete_pending = false;
            s.moved = false;
        });
        Ok(())
    }

    /// A recovery key that opens the backup too, in place of any made before, to be shown once.
    pub fn recovery_key(&self) -> Result<String, BackupProblem> {
        let place = self.place()?;
        let key = self.key()?;
        Ok(nightly::add_recovery_key(
            place.as_ref(),
            &key,
            self.work_factor,
        )?)
    }

    /// The recovery key no longer opens the backup.
    pub fn drop_recovery_key(&self) -> Result<(), BackupProblem> {
        let place = self.place()?;
        Ok(nightly::remove_recovery_key(place.as_ref())?)
    }

    /// "Change the phrase…": the old phrase stops opening the backup.
    pub fn change_phrase(&self, phrase: &str) -> Result<(), BackupProblem> {
        let place = self.place()?;
        let key = self.key()?;
        Ok(nightly::change_phrase(
            place.as_ref(),
            &key,
            phrase,
            self.work_factor,
        )?)
    }

    pub fn set_wifi_only(&self, on: bool) {
        self.update(|s| s.wifi_only = on);
    }

    /// "Turn off…": no more nights. With `also_delete` the backup in the account goes too, now
    /// or, if the account does not answer, when it next does, and the key leaves this phone.
    /// Kept, the key stays, so the switch turns it on again with no steps and a forget for good
    /// still reaches the backup.
    pub fn turn_off(&self, also_delete: bool) -> Result<(), BackupProblem> {
        if also_delete {
            self.keys.forget();
        }
        self.update(|s| {
            s.on = false;
            s.trouble = None;
            if also_delete {
                *s = Saved {
                    wifi_only: s.wifi_only,
                    delete_pending: true,
                    ..Saved::default()
                };
            }
        });
        if also_delete {
            self.delete_pending()?;
        }
        Ok(())
    }

    /// The switch, on again after "Turn off" kept the backup: the same key and phrase.
    pub fn turn_on_again(&self) -> Result<(), BackupProblem> {
        self.place()?;
        self.key()?;
        let saved = self.saved();
        if saved.state.is_none() || saved.moved {
            return Err(BackupProblem::Off);
        }
        self.update(|s| {
            s.on = true;
            s.trouble = None;
        });
        Ok(())
    }

    /// The memory moved to a computer, or is put aside for the computer's: backup stops for good
    /// (decision 19). A backup kept in the account stays as it was on the day of the move, and
    /// the key stays only so This phone can delete it later; nothing runs, not even after a
    /// forget, since the memory here is no longer the person's.
    pub fn memory_moved(&self) {
        self.update(|s| {
            if s.state.is_some() || s.on {
                s.on = false;
                s.trouble = None;
                s.moved = true;
            }
        });
    }

    /// Backup stops by itself once the phone works with a computer, whatever screen moved it.
    fn follow(&self, home: &Home) {
        let saved = self.saved();
        if !saved.moved
            && (saved.on || saved.state.is_some())
            && home.mode(false) == Some(PhoneMode::Companion)
        {
            self.memory_moved();
        }
    }

    /// Delete everything on this phone reaches the backup too: it is deleted from the account,
    /// now or the next time the account answers, and backup is off.
    pub fn erase(&self) {
        if let Err(e) = self.turn_off(true) {
            tracing::warn!("the backup in the account waits to be deleted: {e:?}");
        }
    }

    /// Whether a backup turned off was kept in the account with its key on this phone.
    fn kept_off(&self, saved: &Saved) -> bool {
        !saved.on && !saved.moved && saved.state.is_some() && self.keys.key().is_some()
    }

    fn delete_pending(&self) -> Result<(), BackupProblem> {
        if !self.saved().delete_pending {
            return Ok(());
        }
        let place = self.place()?;
        nightly::delete_all(place.as_ref())?;
        self.update(|s| s.delete_pending = false);
        Ok(())
    }

    /// The newest backup in the account, before the phrase: when it was written and its size.
    pub fn found(&self) -> Result<Option<FoundBackup>, BackupProblem> {
        let place = self.place()?;
        Ok(nightly::found(place.as_ref())?.map(|f| FoundBackup {
            at: f.modified.map(|t| t.to_string()),
            bytes: f.bytes,
        }))
    }

    /// "Back up now", and each night: one night, read back in full when it is the first. With
    /// "Wi-Fi only" on and no Wi-Fi, it runs only when the person chose mobile data this once.
    pub fn back_up_now(
        &self,
        home: &Home,
        over_mobile_data: bool,
        progress: &dyn Fn(BackupPhase, u64, u64),
    ) -> Result<LastNight, BackupProblem> {
        self.follow(home);
        let saved = self.saved();
        if !saved.on {
            return Err(BackupProblem::Off);
        }
        if self.waits_for_wifi(&saved) && !over_mobile_data {
            return Err(BackupProblem::WaitingForWifi);
        }
        self.run_night(home, progress)
    }

    fn run_night(
        &self,
        home: &Home,
        progress: &dyn Fn(BackupPhase, u64, u64),
    ) -> Result<LastNight, BackupProblem> {
        let saved = self.saved();
        let place = self.place()?;
        let key = self.key()?;
        let mut state = saved.state.clone().unwrap_or_default();
        let check = saved.last.is_none();
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(BackupProblem::NotReady);
        }
        let _running = Running(&self.running);
        let _job = home.begin_job()?;
        let (_use, open) = home.using()?;
        let run = NightRun {
            progress,
            cancel: &home.stop,
            check,
        };
        let result = open
            .engine
            .back_up_night(place.as_ref(), &key, &mut state, &run);
        let noted = self.note(state, result);
        // "Turn off" with delete may have come while this night was written: what it wrote goes
        // too.
        if let Err(e) = self.delete_pending() {
            tracing::warn!("the backup in the account waits to be deleted: {e:?}");
        }
        noted
    }

    fn note(
        &self,
        state: NightState,
        result: Result<NightReport, NightError>,
    ) -> Result<LastNight, BackupProblem> {
        let trouble = match &result {
            Ok(_) => None,
            Err(NightError::AnotherPhone { made_at }) => Some(Trouble::AnotherPhone {
                made_at: made_at.clone(),
            }),
            Err(NightError::TooNew) => Some(Trouble::Newer),
            Err(NightError::Damaged) => Some(Trouble::Damaged),
            Err(NightError::Place(PlaceError::NoRoom)) => Some(Trouble::NoRoom),
            Err(NightError::Place(PlaceError::SignedOut)) => Some(Trouble::SignedOut),
            Err(NightError::Place(PlaceError::NoPermission)) => Some(Trouble::NoPermission),
            Err(NightError::Place(PlaceError::Failed(_)) | NightError::Failed(_)) => {
                Some(Trouble::Failed)
            }
            Err(_) => None,
        };
        let last = result.as_ref().ok().map(|r| LastNight {
            at: r.made_at.clone(),
            bytes: r.bytes,
        });
        self.update(|s| {
            if s.state.is_none() {
                return;
            }
            s.state = Some(state);
            s.trouble = trouble;
            if let Some(last) = &last {
                s.last = Some(last.clone());
            }
        });
        match result {
            Ok(_) => Ok(last.expect("a night that was written is noted")),
            Err(e) => {
                tracing::warn!("the night's backup did not finish: {e}");
                Err(e.into())
            }
        }
    }

    /// The nights the backup holds, newest first, for "Which night?".
    pub fn nights(&self, opening: &Opening) -> Result<Vec<nightly::NightSummary>, BackupProblem> {
        let place = self.place()?;
        let key = open_with(place.as_ref(), opening)?;
        Ok(nightly::nights(place.as_ref(), &key)?)
    }

    /// "Bring it back": the night of `night`, or the newest, opened with the phrase or the
    /// recovery key. On an empty phone it becomes the memory here. On a phone that holds a
    /// memory its things are brought in: nothing here is put aside or deleted, and a thing on
    /// both is kept once. Backup is then on; a backup another phone made waits for "Make this
    /// phone the one in use".
    pub fn restore(
        &self,
        home: &Home,
        paired: bool,
        opening: &Opening,
        night: Option<u64>,
        progress: &dyn Fn(BackupPhase, u64, u64),
    ) -> Result<Restored, BackupProblem> {
        let place = self.place()?;
        home.may_restore(paired)?;
        let key = open_with(place.as_ref(), opening)?;
        let _job = home.begin_job()?;
        let holds = home.here(None).is_some();
        let stage = home.stage_dir();
        crate::home::calls::clear(&stage);
        let run = NightRun {
            progress,
            cancel: &home.stop,
            check: false,
        };
        let unpacked = match nightly::restore_night(place.as_ref(), &key, night, &stage, &run) {
            Ok(done) => done,
            Err(e) => {
                crate::home::calls::clear(&stage);
                return Err(e.into());
            }
        };
        let restored = if holds {
            let brought = home
                .using()
                .map_err(BackupProblem::from)
                .and_then(|(_use, open)| {
                    open.engine
                        .bring_in_backup(&stage.join("store"))
                        .map_err(|e| {
                            tracing::warn!("the backup's things could not be brought in: {e}");
                            BackupProblem::Failed
                        })
                });
            crate::home::calls::clear(&stage);
            let brought = brought?;
            Restored::BroughtIn {
                summary: unpacked.summary,
                things: brought.things,
                entries: brought.entries,
                checkins: brought.checkins,
                same: brought.same,
                already: brought.already,
            }
        } else {
            home.put_in_place(&stage)?;
            Restored::Replaced {
                summary: unpacked.summary,
                kept: unpacked.kept,
            }
        };
        let own = self.key().is_ok_and(|held| held.same_as(&key))
            && self
                .saved()
                .state
                .is_some_and(|s| s.device == unpacked.night.device);
        if !own {
            self.keep_key(&key)?;
            self.update(|s| {
                s.on = false;
                s.state = Some(unpacked.state);
                s.last = None;
                s.trouble = None;
                s.delete_pending = false;
                s.moved = false;
            });
        }
        Ok(restored)
    }

    /// "Make this phone the one in use": backup is on, and its first night here is written at
    /// once, so it is the newest and another phone stops when it next looks.
    pub fn make_in_use(&self, home: &Home) -> Result<(), BackupProblem> {
        self.follow(home);
        if self.saved().moved {
            return Err(BackupProblem::Off);
        }
        let place = self.place()?;
        let key = self.key()?;
        let mut state = self.saved().state.unwrap_or_default();
        nightly::take_over(place.as_ref(), &key, &mut state)?;
        self.update(|s| {
            s.on = true;
            s.state = Some(state);
            s.trouble = None;
        });
        self.run_night(home, &|_, _, _| {}).map(|_| ())
    }

    /// One look from the background loop: a night runs when one is due while the phone
    /// charges, or at once after something was forgotten for good, also while backup is off
    /// with the backup kept. Returns whether a night was written; after one that was not, the
    /// next look waits.
    pub fn pass(&self, home: &Home, charging: bool) -> bool {
        self.follow(home);
        let saved = self.saved();
        if saved.delete_pending && self.place().is_ok() {
            if let Err(e) = self.delete_pending() {
                tracing::warn!("the backup in the account could not be deleted yet: {e:?}");
            }
            return false;
        }
        let kept_off = self.kept_off(&saved);
        if !(saved.on || kept_off) || self.place().is_err() || self.running.load(Ordering::SeqCst) {
            return false;
        }
        let blocked = matches!(
            saved.trouble,
            Some(
                Trouble::AnotherPhone { .. }
                    | Trouble::Newer
                    | Trouble::SignedOut
                    | Trouble::NoPermission
            )
        );
        if blocked || saved.state.as_ref().is_some_and(|s| !s.in_use) {
            return false;
        }
        if self.waits_for_wifi(&saved) {
            return false;
        }
        let now = self.clock.now();
        if lock(&self.failed_at).is_some_and(|(at, mins)| {
            now.duration_since(at) < jiff::SignedDuration::from_mins(mins)
        }) {
            return false;
        }
        let forgot = home
            .with_local(|open| open.engine.forgot_since_night().unwrap_or(false))
            .unwrap_or(false);
        let due = saved.on && charging && !self.recent(&saved.last);
        if !forgot && !due {
            return false;
        }
        let ran = self.run_night(home, &|_, _, _| {});
        *lock(&self.failed_at) = match &ran {
            Ok(_) => None,
            Err(BackupProblem::Busy) => Some((now, BUSY_MINUTES)),
            Err(_) => Some((now, RETRY_MINUTES)),
        };
        ran.is_ok()
    }
}
