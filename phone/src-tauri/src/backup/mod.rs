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
pub mod screens;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use nocturne_contract::{BackupPhase, BackupSummary, HomeError, KeptCounts, PhoneMode};
use nocturne_engine::Clock;
use nocturne_engine::nightly::{
    self, BackupPlace, ExposeSecret, KeyLocks, NightError, NightKey, NightReport, NightRun,
    NightState, PlaceError,
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
/// Charging is noted at most this often, so a pass does not write at every look.
const CHARGED_NOTE_MINUTES: i64 = 60;

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
    /// The account chosen already holds another backup: nothing is written over it, and nothing
    /// is tried again until the person chooses an account.
    OtherBackup,
    /// The backup can't start in the account chosen until the phrase is set again: the phone
    /// does not know the locks to carry there.
    NeedsPhrase,
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
    /// The key as the account holds it, locked, so a new account can take the same phrase and
    /// recovery key.
    #[serde(default)]
    locks: Option<KeyLocks>,
    /// The person chose another account: the next night starts the backup there afresh.
    #[serde(default)]
    fresh: bool,
    /// The phrase locks carries to the account chosen last may have written before its first
    /// night there.
    #[serde(default)]
    carried: Vec<Vec<u8>>,
    /// Counts the accounts chosen, so a night that ran across a change notes nothing.
    #[serde(default)]
    account: u64,
    /// When a look last found the phone charging.
    #[serde(default)]
    charged_at: Option<String>,
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
    /// No night is due yet because the phone has not charged since the last one.
    NotCharged,
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
    OtherBackup,
    NeedsPhrase,
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
    /// The person asked for the backup in the account to go, and it goes once the account
    /// answers.
    pub delete_pending: bool,
    /// Something forgotten for good is still in the backup until the account can be reached.
    pub forget_waiting: bool,
}

/// Why a backup call did not do what was asked, as the screens name it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    /// A newer Nocturne made the backup: the one it names, when it does, and this one.
    #[serde(rename_all = "camelCase")]
    Newer {
        theirs: Option<String>,
        ours: String,
    },
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
            NightError::TooNew => BackupProblem::Newer {
                theirs: None,
                ours: nightly::APP_VERSION.to_string(),
            },
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
    /// An empty phone: the backup is the memory here now. `own`: the backup is this phone's
    /// own, so nothing asks whether this phone makes the backups.
    Replaced {
        summary: BackupSummary,
        kept: KeptCounts,
        own: bool,
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
        own: bool,
    },
}

/// What the account shows before the phrase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundBackup {
    pub at: Option<String>,
    pub bytes: u64,
}

/// A backup call's problem, naming the Nocturne that made the night when it was a newer one.
fn naming_newer(
    e: NightError,
    place: &dyn BackupPlace,
    key: &NightKey,
    count: Option<u64>,
) -> BackupProblem {
    match BackupProblem::from(e) {
        BackupProblem::Newer { ours, .. } => BackupProblem::Newer {
            theirs: nightly::made_with(place, key, count),
            ours,
        },
        other => other,
    }
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
    /// Held whenever `Saved::fresh`, `Saved::locks` or `Saved::carried` is decided on or changed:
    /// a fresh start on another account, its carry and the note that ends it, and every lock
    /// change. So a lock change lands either before the carry is noted, and the next night
    /// carries it, or after, and goes to the account itself.
    locking: Mutex<()>,
    /// The passphrase work the locked key asks for; `None` is what Nocturne writes.
    work_factor: Option<u8>,
    /// A recovery key's lock, made and shown but not yet saved by the person: nothing opens
    /// with it until it is kept, so "Cancel" leaves the earlier key working.
    new_key: Mutex<Option<Vec<u8>>>,
    /// Keeps what the phone notes of the backup out of the phone's own device backup, as the
    /// memory is: it holds the key's locks. Run after each write, since a file written again in
    /// its place loses the mark on an iPhone.
    exclude: Option<crate::home::ExcludeFromBackup>,
}

/// Whether the phone was found charging since the last night finished.
fn charged_since(saved: &Saved) -> bool {
    let at = |t: &Option<String>| t.as_ref().and_then(|t| t.parse::<jiff::Timestamp>().ok());
    match (
        at(&saved.charged_at),
        at(&saved.last.as_ref().map(|l| l.at.clone())),
    ) {
        (Some(charged), Some(last)) => charged > last,
        (None, Some(_)) => false,
        // Before the first night there is nothing to wait for but tonight.
        (_, None) => true,
    }
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
            locking: Mutex::new(()),
            work_factor: None,
            new_key: Mutex::new(None),
            exclude: None,
        }
    }

    /// The way to keep its notes out of the phone's own device backup.
    pub fn with_exclude(mut self, exclude: crate::home::ExcludeFromBackup) -> Self {
        self.exclude = Some(exclude);
        self
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
        match write_private(&self.path(), &json) {
            Ok(()) => {
                if let Some(exclude) = &self.exclude {
                    exclude(&self.path());
                }
            }
            Err(e) => tracing::warn!("the backup's state could not be saved: {e}"),
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

    /// Reads the key's locks back from the account, with `locking` held. When that fails the
    /// copy stays unknown, and an unknown copy is never carried to another account. Never on an
    /// account the backup is starting afresh on: what it holds is not this backup's yet.
    fn remember_locks(&self, place: &dyn BackupPlace) {
        if self.saved().fresh {
            return;
        }
        match nightly::key_locks(place) {
            Ok(locks) => self.update(|s| s.locks = Some(locks)),
            Err(e) => tracing::warn!("the backup's locks could not be read back: {e}"),
        }
    }

    /// Changes a lock in the account, and the phone's copy with it, with `locking` held. The copy is unknown while the
    /// change runs, so a change that half happened never leaves an old phrase's or a dropped
    /// recovery key's lock to be carried; it follows the lock written when one was known before,
    /// and is read back otherwise.
    fn relock<T>(
        &self,
        place: &dyn BackupPlace,
        change: impl FnOnce() -> Result<T, BackupProblem>,
        apply: impl FnOnce(&T, &mut KeyLocks),
    ) -> Result<T, BackupProblem> {
        let before = self.update(|s| s.locks.take());
        let out = change();
        match (&out, before) {
            (Ok(done), Some(mut locks)) => {
                apply(done, &mut locks);
                self.update(|s| s.locks = Some(locks));
            }
            _ => self.remember_locks(place),
        }
        out
    }

    /// The person chose the account again after Google's screen, or picked another. Signed out
    /// or no permission is over; on another account the backup starts there afresh, with the
    /// same phrase and recovery key, and its first night goes in full. The earlier account
    /// can't be reached any more, so a deletion owed to it is dropped.
    pub fn reconnected(&self, changed: bool) {
        *lock(&self.failed_at) = None;
        let _held = lock(&self.locking);
        self.update(|s| {
            if matches!(
                s.trouble,
                Some(Trouble::SignedOut | Trouble::NoPermission | Trouble::OtherBackup)
            ) {
                s.trouble = None;
            }
            if !changed {
                return;
            }
            s.delete_pending = false;
            s.account += 1;
            s.carried.clear();
            if s.state.is_some() && !s.moved {
                s.fresh = true;
                s.last = None;
                s.trouble = None;
            }
        });
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
                Trouble::OtherBackup => BackupState::OtherBackup,
                Trouble::NeedsPhrase => BackupState::NeedsPhrase,
                Trouble::Damaged => BackupState::Damaged,
                Trouble::Failed => BackupState::Failed,
            }
        } else if saved.state.as_ref().is_some_and(|s| !s.in_use) {
            BackupState::NotInUse
        } else if self.waits_for_wifi(&saved) {
            BackupState::WaitingForWifi
        } else if self.recent(&saved.last) {
            BackupState::UpToDate
        } else if !charged_since(&saved) {
            BackupState::NotCharged
        } else {
            BackupState::Waiting
        };
        BackupStatus {
            on: saved.on,
            wifi_only: saved.wifi_only,
            last: saved.last,
            state,
            moved: saved.moved,
            delete_pending: saved.delete_pending,
            forget_waiting: false,
        }
    }

    /// How the backup stands, with whether a forget for good still waits for the account: it
    /// is due at once, yet the last look could not run it.
    pub fn status_with(&self, home: &Home) -> BackupStatus {
        let mut status = self.status();
        let saved = self.saved();
        let reaches = saved.on || self.kept_off(&saved);
        let held_up = lock(&self.failed_at).is_some()
            || self.place().is_err()
            || self.waits_for_wifi(&saved)
            || matches!(&saved.trouble, Some(t) if *t != Trouble::Failed);
        status.forget_waiting = reaches
            && held_up
            && home
                .with_local(|open| open.engine.forgot_since_night().unwrap_or(false))
                .unwrap_or(false);
        status
    }

    fn note_charging(&self) {
        let now = self.clock.now();
        let noted = self
            .saved()
            .charged_at
            .and_then(|at| at.parse::<jiff::Timestamp>().ok());
        if noted.is_some_and(|at| {
            now.duration_since(at) < jiff::SignedDuration::from_mins(CHARGED_NOTE_MINUTES)
        }) {
            return;
        }
        self.update(|s| s.charged_at = Some(now.to_string()));
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
        let _held = lock(&self.locking);
        self.update(|s| {
            s.on = true;
            s.state = Some(NightState::new());
            s.last = None;
            s.trouble = None;
            s.delete_pending = false;
            s.moved = false;
            s.fresh = false;
            s.locks = None;
        });
        self.remember_locks(place.as_ref());
        Ok(())
    }

    /// A new recovery key, to be shown once. Nothing opens with it until
    /// [`PhoneBackup::keep_recovery_key`]; until then any key made before still works.
    pub fn recovery_key(&self) -> Result<String, BackupProblem> {
        self.place()?;
        let key = self.key()?;
        let (text, locked) = nightly::recovery_lock(&key, self.work_factor)?;
        *lock(&self.new_key) = Some(locked);
        Ok(text)
    }

    /// The person saved the recovery key just made: it opens the backup, in place of any made
    /// before. When the account can't be reached the key stays made, so this can be asked again.
    pub fn keep_recovery_key(&self) -> Result<(), BackupProblem> {
        let place = self.place()?;
        let locked = lock(&self.new_key).take().ok_or(BackupProblem::NotReady)?;
        let _held = lock(&self.locking);
        let kept = if self.saved().fresh {
            // Nothing goes to an account the backup is starting afresh on before its first
            // night carries the copy there.
            let known = self.update(|s| match s.locks.as_mut() {
                Some(locks) => {
                    locks.recovery = Some(locked.clone());
                    true
                }
                None => false,
            });
            if known {
                Ok(())
            } else {
                Err(BackupProblem::Damaged)
            }
        } else {
            self.relock(
                place.as_ref(),
                || Ok(nightly::keep_recovery_lock(place.as_ref(), &locked)?),
                |(), locks| locks.recovery = Some(locked.clone()),
            )
        };
        if kept.is_err() {
            lock(&self.new_key).get_or_insert(locked);
        }
        kept
    }

    /// The recovery key just made was not saved: it never opens the backup, and any made before
    /// still does.
    pub fn drop_recovery_key(&self) {
        lock(&self.new_key).take();
    }

    /// "Change the phrase…": the old phrase stops opening the backup.
    pub fn change_phrase(&self, phrase: &str) -> Result<(), BackupProblem> {
        let place = self.place()?;
        let key = self.key()?;
        let _held = lock(&self.locking);
        if self.saved().fresh {
            let locked = nightly::phrase_lock(&key, phrase, self.work_factor)?;
            self.update(|s| {
                match s.locks.as_mut() {
                    Some(locks) => locks.phrase = locked,
                    None => {
                        s.locks = Some(KeyLocks {
                            phrase: locked,
                            recovery: None,
                        })
                    }
                }
                if s.trouble == Some(Trouble::NeedsPhrase) {
                    s.trouble = None;
                }
            });
            return Ok(());
        }
        self.relock(
            place.as_ref(),
            || {
                Ok(nightly::change_phrase_locked(
                    place.as_ref(),
                    &key,
                    phrase,
                    self.work_factor,
                )?)
            },
            |locked, locks| locks.phrase = locked.clone(),
        )?;
        Ok(())
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
        let held = lock(&self.locking);
        self.update(|s| {
            s.on = false;
            s.trouble = None;
            if also_delete {
                *s = Saved {
                    wifi_only: s.wifi_only,
                    delete_pending: true,
                    account: s.account,
                    ..Saved::default()
                };
            }
        });
        drop(held);
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
    /// Not on an account chosen since: the backup kept is in the earlier one.
    fn kept_off(&self, saved: &Saved) -> bool {
        !saved.on
            && !saved.moved
            && !saved.fresh
            && saved.state.is_some()
            && self.keys.key().is_some()
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
        let (result, carried) = match self.start_afresh(place.as_ref(), &key, &state) {
            Ok(carried) => (
                open.engine
                    .back_up_night(place.as_ref(), &key, &mut state, &run),
                carried,
            ),
            Err(e) => (Err(e), None),
        };
        let held = lock(&self.locking);
        let noted = self.note(state, result, saved.account, carried);
        if noted.is_ok() && self.saved().locks.is_none() {
            self.remember_locks(place.as_ref());
        }
        drop(held);
        // "Turn off" with delete may have come while this night was written: what it wrote goes
        // too.
        if let Err(e) = self.delete_pending() {
            tracing::warn!("the backup in the account waits to be deleted: {e:?}");
        }
        noted
    }

    /// On another account, the locks go there before its first night; answers the copy carried.
    fn start_afresh(
        &self,
        place: &dyn BackupPlace,
        key: &NightKey,
        state: &NightState,
    ) -> Result<Option<KeyLocks>, NightError> {
        let _held = lock(&self.locking);
        let saved = self.saved();
        if !saved.fresh {
            return Ok(None);
        }
        // Without a copy the phone can trust, nothing is carried: "Change the phrase" makes a
        // new one.
        let locks = saved.locks.ok_or(NightError::NoPhrase)?;
        self.update(|s| {
            if !s.carried.contains(&locks.phrase) {
                s.carried.push(locks.phrase.clone());
            }
        });
        nightly::carry_locks(place, key, &locks, &saved.carried, state)?;
        Ok(Some(locks))
    }

    /// Notes how a night ended, with `locking` held, unless another account was chosen while it
    /// ran. The fresh start
    /// is over only once the copy it carried is still the phone's: a lock changed meanwhile is
    /// carried at the next night.
    fn note(
        &self,
        state: NightState,
        result: Result<NightReport, NightError>,
        account: u64,
        carried: Option<KeyLocks>,
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
            // Only a start in another account finds a backup there already.
            Err(NightError::AlreadyThere) => Some(Trouble::OtherBackup),
            Err(NightError::NoPhrase) => Some(Trouble::NeedsPhrase),
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
            if s.state.is_none() || s.account != account {
                return;
            }
            s.state = Some(state);
            s.trouble = trouble;
            if let Some(last) = &last {
                s.last = Some(last.clone());
                if carried.is_none() || carried == s.locks {
                    s.fresh = false;
                    s.carried.clear();
                }
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

    /// The nights the account lists, newest first, before the phrase.
    pub fn listed(&self) -> Result<Vec<nightly::ListedNight>, BackupProblem> {
        let place = self.place()?;
        Ok(nightly::listed_nights(place.as_ref())?)
    }

    /// The nights the backup holds, newest first, for "Which night?".
    pub fn nights(&self, opening: &Opening) -> Result<Vec<nightly::NightSummary>, BackupProblem> {
        let place = self.place()?;
        let key = open_with(place.as_ref(), opening)?;
        nightly::nights(place.as_ref(), &key)
            .map_err(|e| naming_newer(e, place.as_ref(), &key, None))
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
                return Err(naming_newer(e, place.as_ref(), &key, night));
            }
        };
        let own = self.key().is_ok_and(|held| held.same_as(&key))
            && self
                .saved()
                .state
                .is_some_and(|s| s.device == unpacked.night.device);
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
                own,
            }
        } else {
            home.put_in_place(&stage)?;
            Restored::Replaced {
                summary: unpacked.summary,
                kept: unpacked.kept,
                own,
            }
        };
        if !own {
            self.keep_key(&key)?;
            let _held = lock(&self.locking);
            self.update(|s| {
                s.on = false;
                s.state = Some(unpacked.state);
                s.last = None;
                s.trouble = None;
                s.delete_pending = false;
                s.moved = false;
                s.fresh = false;
                s.locks = None;
            });
            self.remember_locks(place.as_ref());
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
        self.pass_with(home, charging, &|_, _, _| {})
    }

    /// [`PhoneBackup::pass`], telling how far a night is as it runs.
    pub fn pass_with(
        &self,
        home: &Home,
        charging: bool,
        progress: &dyn Fn(BackupPhase, u64, u64),
    ) -> bool {
        self.follow(home);
        if charging {
            self.note_charging();
        }
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
                    | Trouble::OtherBackup
                    | Trouble::NeedsPhrase
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
        let ran = self.run_night(home, progress);
        *lock(&self.failed_at) = match &ran {
            Ok(_) => None,
            Err(BackupProblem::Busy) => Some((now, BUSY_MINUTES)),
            Err(_) => Some((now, RETRY_MINUTES)),
        };
        ran.is_ok()
    }
}
