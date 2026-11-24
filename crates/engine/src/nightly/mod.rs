//! The nightly backup of a phone that is its own home (ADR 0019): each night, what changed goes
//! to the person's own account as locked pieces, then one small locked night that lists them.
//!
//! In the account, a flat folder ([`BackupPlace`]) holds:
//!
//! - `key-phrase`, and `key-recovery` when the person made one: the backup's key, locked with the
//!   phrase or the recovery key ([`lock`]).
//! - `piece-<sha256 of the locked bytes>`: one locked piece of the store or of an original, cut
//!   where its content says ([`pieces`]). A piece no kept night lists is removed.
//! - `night-<count>-<phone>-<made>-<bytes>`: one locked night, listing the pieces of the store
//!   snapshot and of every original by the hash of their plain bytes. Its name says, unlocked,
//!   which phone wrote it and its count, so a phone sees a newer night from another phone without
//!   the key and stops, and when it was made (seconds since 1970) and what its pieces take, so
//!   "Which night?" lists the nights before the phrase. Nights named before the last two parts
//!   were added read as they were.
//!
//! A night is written only after every piece it lists is in the account, so a stopped run leaves
//! the nights before it whole. Seven nights are kept. A forget for good marks the store
//! ([`FORGOT`]); the next night is written at once and every older night goes, with the pieces only
//! they listed. A restore brings back the newest night into a staging folder and checks it the way
//! a backup file is checked.

mod lock;
mod pieces;
mod place;

use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use nocturne_contract::{BackupPhase, BackupSummary, KeptCounts};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Engine;
use crate::error::Error;
use crate::fsutil;
use crate::util::{fmt_ts, hex, random_hex};

pub use age::secrecy::{ExposeSecret, SecretString};
pub use lock::{
    KEY_PHRASE, KEY_RECOVERY, NightKey, PHRASE_WORDS, make_phrase, make_recovery_key,
    normalise_phrase, normalise_recovery_key,
};
pub use place::{BackupPlace, FolderPlace, MemoryPlace, PlaceEntry, PlaceError};

use pieces::{Cutter, MAX_PIECE};

/// The store's mark that something was forgotten for good since the last night: a count, so a
/// night clears only the forgets it carried.
pub(crate) const FORGOT: &str = "_forgotSinceNight";
/// The scratch folder under the store, holding the snapshot while a night is written.
pub(crate) const SCRATCH: &str = "night-tmp";
/// How many nights the account keeps.
pub const NIGHTS_KEPT: usize = 7;
/// The Nocturne that writes and reads nights here, as a night names the one that made it.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

const NIGHT: &str = "night-";
const PIECE: &str = "piece-";
/// The night format this build writes and reads; a higher number is a newer Nocturne's.
const FORMAT: u32 = 1;
/// The largest night read.
const MAX_NIGHT: usize = 64 * 1024 * 1024;
/// The largest locked piece read: the largest piece and age's framing.
const MAX_LOCKED_PIECE: usize = MAX_PIECE + 64 * 1024;

/// Why the backup did not do what was asked.
#[derive(Debug, thiserror::Error)]
pub enum NightError {
    /// The phrase or recovery key does not open this backup.
    #[error("that phrase does not open this backup")]
    WrongPhrase,
    /// The account holds no backup.
    #[error("there is no backup in the account")]
    NoBackup,
    /// The account already holds a backup, so a new one is not started over it.
    #[error("the account already holds a backup")]
    AlreadyThere,
    /// Another phone wrote a newer night: this phone stops, so nothing is overwritten.
    #[error("another phone made a newer backup")]
    AnotherPhone { made_at: String },
    /// This phone was restored and is not yet the phone in use.
    #[error("this phone is not the one that makes the backups")]
    NotInUse,
    /// A newer Nocturne wrote it.
    #[error("the backup is from a newer Nocturne")]
    TooNew,
    /// A piece, a night or a locked key does not match what it should be.
    #[error("the backup is damaged")]
    Damaged,
    /// The phrase is empty.
    #[error("a phrase is needed")]
    NoPhrase,
    #[error("{0}")]
    Place(#[from] PlaceError),
    #[error("cancelled")]
    Cancelled,
    /// Something was forgotten while the night was made, so none of it was kept.
    #[error("something changed while this ran")]
    Busy,
    #[error("{0}")]
    Failed(String),
}

impl From<Error> for NightError {
    fn from(e: Error) -> Self {
        match e {
            Error::Cancelled => NightError::Cancelled,
            Error::Busy => NightError::Busy,
            Error::BackupTooNew => NightError::TooNew,
            Error::BackupDamaged => NightError::Damaged,
            e => NightError::Failed(e.to_string()),
        }
    }
}

impl From<std::io::Error> for NightError {
    fn from(e: std::io::Error) -> Self {
        NightError::Failed(e.to_string())
    }
}

pub type NightResult<T> = std::result::Result<T, NightError>;

/// What this phone keeps of the backup between runs, beside the key in its safe place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NightState {
    /// The random number this phone names its nights with.
    pub device: String,
    /// The count of the newest night this phone wrote or took over.
    pub count: u64,
    /// Whether this phone makes the backups. A restored phone does not until the person says.
    pub in_use: bool,
}

impl NightState {
    /// A phone that has just turned backup on.
    pub fn new() -> Self {
        NightState {
            device: random_hex(8),
            count: 0,
            in_use: true,
        }
    }
}

impl Default for NightState {
    fn default() -> Self {
        Self::new()
    }
}

/// What a run was told besides the place and the key.
pub struct NightRun<'a> {
    /// Called with the phase and the bytes done out of the bytes in all.
    pub progress: &'a dyn Fn(BackupPhase, u64, u64),
    /// When set, the run stops; nights already in the account stay whole.
    pub cancel: &'a AtomicBool,
    /// Read every piece back before the night is written, as the first backup does. Other nights
    /// read back only the pieces they sent.
    pub check: bool,
}

/// What a night holds and what it took.
#[derive(Debug, Clone, PartialEq)]
pub struct NightReport {
    pub made_at: String,
    pub count: u64,
    /// The locked bytes of every piece the night lists, and of the night itself.
    pub bytes: u64,
    /// The locked bytes sent this run.
    pub sent: u64,
    pub memories: u32,
    pub items: u32,
    pub pages: u32,
    pub originals: u32,
    /// Older nights went because something was forgotten.
    pub carried_forget: bool,
}

/// The newest backup in an account, as seen before any phrase: when the account says it was
/// written, and how much the account holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub modified: Option<jiff::Timestamp>,
    pub bytes: u64,
}

/// A night as the key opened it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NightSummary {
    pub made_at: String,
    pub count: u64,
    pub device: String,
    pub bytes: u64,
    /// The things it holds.
    pub items: u32,
    /// When the first of them was kept; `None` for a night written before this was noted.
    pub first: Option<String>,
}

/// A night as the account lists it, before any phrase: what its name and the account's record
/// say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedNight {
    pub count: u64,
    pub made_at: Option<jiff::Timestamp>,
    /// What the night and its pieces take; `None` for a night named before its size was.
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PieceRef {
    /// The piece's name without its prefix: the SHA-256 of its locked bytes.
    piece: String,
    /// The SHA-256 of its plain bytes.
    hash: String,
    size: u64,
    locked: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileRef {
    sha: String,
    size: u64,
    pieces: Vec<PieceRef>,
}

#[derive(Deserialize)]
struct Head {
    format: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Body {
    format: u32,
    device: String,
    count: u64,
    made_at: String,
    app_version: String,
    store_version: u32,
    memories: u32,
    items: u32,
    pages: u32,
    /// When the first thing was kept. Nights written before it was noted have none.
    #[serde(default)]
    first: Option<String>,
    store: Vec<PieceRef>,
    files: Vec<FileRef>,
}

impl Body {
    fn pieces(&self) -> impl Iterator<Item = &PieceRef> {
        self.store
            .iter()
            .chain(self.files.iter().flat_map(|f| f.pieces.iter()))
    }
}

/// One night in the account, opened.
struct Night {
    name: String,
    locked: u64,
    body: Body,
}

impl Night {
    fn bytes(&self) -> u64 {
        self.locked + self.body.pieces_bytes()
    }

    fn summary(&self) -> NightSummary {
        NightSummary {
            made_at: self.body.made_at.clone(),
            count: self.body.count,
            device: self.body.device.clone(),
            bytes: self.bytes(),
            items: self.body.items,
            first: self.body.first.clone(),
        }
    }
}

impl Body {
    /// What the pieces it lists take in the account, each counted once.
    fn pieces_bytes(&self) -> u64 {
        let mut seen = BTreeSet::new();
        self.pieces()
            .filter(|p| seen.insert(p.piece.as_str()))
            .map(|p| p.locked)
            .sum()
    }
}

fn night_name(count: u64, device: &str, made: i64, bytes: u64) -> String {
    format!("{NIGHT}{count:012}-{device}-{made}-{bytes}")
}

/// What a night's name says, unlocked.
struct NightName<'a> {
    count: u64,
    device: &'a str,
    /// Seconds since 1970; not in a name written before it was added.
    made: Option<i64>,
    /// What its pieces take; not in a name written before it was added.
    bytes: Option<u64>,
}

fn read_night_name(name: &str) -> Option<NightName<'_>> {
    let digits = |s: &str| !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit());
    let rest = name.strip_prefix(NIGHT)?;
    let mut parts = rest.split('-');
    let count = parts.next()?;
    let device = parts.next()?;
    if count.len() != 12 || !count.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if device.len() != 16 || !device.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    // A later Nocturne may name more after these: the night is still counted, so this one
    // stops at it instead of passing it over.
    let mut number = || parts.next().filter(|p| digits(p));
    let made = number().and_then(|p| p.parse().ok());
    let bytes = number().and_then(|p| p.parse().ok());
    Some(NightName {
        count: count.parse().ok()?,
        device,
        made,
        bytes,
    })
}

/// The count and phone a night's name gives, unlocked.
fn parse_night(name: &str) -> Option<(u64, &str)> {
    read_night_name(name).map(|n| (n.count, n.device))
}

fn sha(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn equal(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn seal_night(key: &NightKey, name: &str, body: &Body) -> NightResult<Vec<u8>> {
    let json = serde_json::to_vec(body).map_err(|e| NightError::Failed(e.to_string()))?;
    let mac = key.mac(&[name.as_bytes(), &[0], &json]);
    let mut plain = hex(&mac).into_bytes();
    plain.push(b'\n');
    plain.extend_from_slice(&json);
    key.lock(&plain)
}

fn open_night(key: &NightKey, name: &str, locked: &[u8]) -> NightResult<Body> {
    if locked.len() > MAX_NIGHT {
        return Err(NightError::Damaged);
    }
    let (count, device) = parse_night(name).ok_or(NightError::Damaged)?;
    let plain = key.unlock(locked)?;
    let split = plain
        .iter()
        .position(|&b| b == b'\n')
        .ok_or(NightError::Damaged)?;
    let (mac, json) = (&plain[..split], &plain[split + 1..]);
    let expected = hex(&key.mac(&[name.as_bytes(), &[0], json]));
    if !equal(mac, expected.as_bytes()) {
        return Err(NightError::Damaged);
    }
    let head: Head = serde_json::from_slice(json).map_err(|_| NightError::Damaged)?;
    if head.format > FORMAT {
        return Err(NightError::TooNew);
    }
    let body: Body = serde_json::from_slice(json).map_err(|_| NightError::Damaged)?;
    if body.format == 0 || body.count != count || body.device != device {
        return Err(NightError::Damaged);
    }
    if body.store_version > crate::store::latest_version() {
        return Err(NightError::TooNew);
    }
    Ok(body)
}

/// Every night in the account the key opens, oldest first. A night from a newer Nocturne stops
/// everything; one that does not open is passed over, and nothing of it is relied on.
fn load_nights(place: &dyn BackupPlace, key: &NightKey) -> NightResult<Vec<Night>> {
    let mut nights = Vec::new();
    for entry in place.list(NIGHT)? {
        if parse_night(&entry.name).is_none() {
            continue;
        }
        let Some(locked) = place.get(&entry.name)? else {
            continue;
        };
        match open_night(key, &entry.name, &locked) {
            Ok(body) => nights.push(Night {
                name: entry.name,
                locked: locked.len() as u64,
                body,
            }),
            Err(NightError::TooNew) => return Err(NightError::TooNew),
            Err(e) => tracing::warn!("a night in the backup did not open: {e}"),
        }
    }
    nights.sort_by_key(|n| n.body.count);
    Ok(nights)
}

/// Whether the account holds a night `opened` does not: one of another backup, or one damaged.
fn foreign_nights(place: &dyn BackupPlace, opened: &[Night]) -> NightResult<bool> {
    let named = place
        .list(NIGHT)?
        .iter()
        .filter(|e| parse_night(&e.name).is_some())
        .count();
    Ok(named != opened.len())
}

/// A night another phone wrote after this phone's newest.
fn newer_from_another<'a>(nights: &'a [Night], state: &NightState) -> Option<&'a Night> {
    nights
        .iter()
        .rev()
        .find(|n| n.body.device != state.device && n.body.count > state.count)
}

/// Fetch one piece and prove it: its name is the hash of what came, the key opens it, and what
/// it holds is what the night says.
fn fetch_piece(place: &dyn BackupPlace, key: &NightKey, r: &PieceRef) -> NightResult<Vec<u8>> {
    let locked = place
        .get(&format!("{PIECE}{}", r.piece))?
        .ok_or(NightError::Damaged)?;
    if locked.len() > MAX_LOCKED_PIECE || sha(&locked) != r.piece {
        return Err(NightError::Damaged);
    }
    let plain = key.unlock(&locked)?;
    if plain.len() as u64 != r.size || sha(&plain) != r.hash {
        return Err(NightError::Damaged);
    }
    Ok(plain)
}

/// Whether anything was forgotten since the last night, and the mark's count.
fn forgot_mark(conn: &Connection) -> crate::Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [FORGOT], |r| {
            r.get(0)
        })
        .optional()?)
}

/// Mark the store: something was forgotten for good, so the next night replaces every older
/// one. Called in the forget's own transaction.
pub(crate) fn note_forgot(tx: &rusqlite::Transaction<'_>) -> crate::Result<()> {
    tx.execute(
        "INSERT INTO settings(key, value) VALUES (?1, '1') \
         ON CONFLICT(key) DO UPDATE SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)",
        [FORGOT],
    )?;
    Ok(())
}

/// Remove the scratch folder a stopped night left behind.
pub(crate) fn remove_leftover_scratch(dir: &Path) {
    let scratch = dir.join(SCRATCH);
    if scratch.exists() {
        fsutil::shred_tree(&scratch);
    }
}

/// A folder removed when dropped, unless kept.
struct Scratch {
    dir: std::path::PathBuf,
    keep: bool,
}

impl Scratch {
    fn new(dir: &Path) -> Self {
        Scratch {
            dir: dir.to_path_buf(),
            keep: false,
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.keep {
            fsutil::shred_tree(&self.dir);
        }
    }
}

/// Bytes done out of the bytes in all, and the way to stop.
struct Progress<'a> {
    phase: BackupPhase,
    done: u64,
    total: u64,
    run: &'a NightRun<'a>,
}

impl Progress<'_> {
    fn step(&mut self, n: u64) -> NightResult<()> {
        self.done = (self.done + n).min(self.total);
        (self.run.progress)(self.phase, self.done, self.total);
        if self.run.cancel.load(Ordering::SeqCst) {
            return Err(NightError::Cancelled);
        }
        Ok(())
    }
}

/// Sends what the account lacks and remembers what it holds.
struct Sender<'a> {
    place: &'a dyn BackupPlace,
    key: &'a NightKey,
    /// Pieces the account holds, by the hash of their plain bytes.
    known: HashMap<String, PieceRef>,
    sent: u64,
    /// The pieces sent this run.
    new: Vec<PieceRef>,
}

impl Sender<'_> {
    fn piece(&mut self, plain: &[u8]) -> NightResult<PieceRef> {
        let hash = sha(plain);
        if let Some(r) = self.known.get(&hash) {
            return Ok(r.clone());
        }
        let locked = self.key.lock(plain)?;
        let r = PieceRef {
            piece: sha(&locked),
            hash: hash.clone(),
            size: plain.len() as u64,
            locked: locked.len() as u64,
        };
        self.place.put(&format!("{PIECE}{}", r.piece), &locked)?;
        self.sent += locked.len() as u64;
        self.known.insert(hash, r.clone());
        self.new.push(r.clone());
        Ok(r)
    }
}

/// An empty phrase is refused; any other is taken as typed, normalised.
fn usable_phrase(phrase: &str) -> NightResult<String> {
    let phrase = normalise_phrase(phrase);
    if phrase.is_empty() {
        return Err(NightError::NoPhrase);
    }
    Ok(phrase)
}

fn work(work_factor: Option<u8>) -> u8 {
    work_factor
        .unwrap_or(crate::backup::WORK_FACTOR)
        .min(crate::backup::WORK_FACTOR)
}

/// Start a backup in an empty account: a new key, locked with `phrase`. `work_factor` is for
/// tests; `None` is what Nocturne writes.
pub fn turn_on(
    place: &dyn BackupPlace,
    phrase: &str,
    work_factor: Option<u8>,
) -> NightResult<NightKey> {
    let phrase = usable_phrase(phrase)?;
    if !place.list("key-")?.is_empty() || !place.list(NIGHT)?.is_empty() {
        return Err(NightError::AlreadyThere);
    }
    let key = NightKey::generate();
    place.put(KEY_PHRASE, &lock::wrap(&key, &phrase, work(work_factor))?)?;
    Ok(key)
}

/// Make a recovery key that opens this backup too, in place of any made before, and return it
/// to be shown once.
pub fn add_recovery_key(
    place: &dyn BackupPlace,
    key: &NightKey,
    work_factor: Option<u8>,
) -> NightResult<String> {
    Ok(add_recovery_key_locked(place, key, work_factor)?.0)
}

/// [`add_recovery_key`], with the lock it wrote, so the phone's copy of the locks follows what
/// the account holds without reading it back.
pub fn add_recovery_key_locked(
    place: &dyn BackupPlace,
    key: &NightKey,
    work_factor: Option<u8>,
) -> NightResult<(String, Vec<u8>)> {
    let (text, locked) = recovery_lock(key, work_factor)?;
    place.put(KEY_RECOVERY, &locked)?;
    Ok((text, locked))
}

/// A new recovery key and the key locked with it, written nowhere: for a backup starting afresh
/// in an account that holds none of its locks yet.
pub fn recovery_lock(key: &NightKey, work_factor: Option<u8>) -> NightResult<(String, Vec<u8>)> {
    let text = make_recovery_key();
    let locked = lock::wrap(key, &text, work(work_factor))?;
    Ok((text, locked))
}

/// Write a recovery key's lock made with [`recovery_lock`] once the person has saved the key: it
/// opens the backup from now on, in place of any made before.
pub fn keep_recovery_lock(place: &dyn BackupPlace, locked: &[u8]) -> NightResult<()> {
    Ok(place.put(KEY_RECOVERY, locked)?)
}

/// The key locked with `phrase`, written nowhere, as [`recovery_lock`].
pub fn phrase_lock(key: &NightKey, phrase: &str, work_factor: Option<u8>) -> NightResult<Vec<u8>> {
    let phrase = usable_phrase(phrase)?;
    lock::wrap(key, &phrase, work(work_factor))
}

/// The recovery key no longer opens the backup.
pub fn remove_recovery_key(place: &dyn BackupPlace) -> NightResult<()> {
    Ok(place.delete(KEY_RECOVERY)?)
}

/// Whether the account holds a recovery key for the backup.
pub fn has_recovery_key(place: &dyn BackupPlace) -> NightResult<bool> {
    Ok(place
        .list(KEY_RECOVERY)?
        .iter()
        .any(|e| e.name == KEY_RECOVERY))
}

/// The key as the account holds it, locked with the phrase and with the recovery key if one was
/// made. The phone keeps a copy so the backup can start again in another account with the same
/// phrase and recovery key, which it never holds itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyLocks {
    pub phrase: Vec<u8>,
    pub recovery: Option<Vec<u8>>,
}

/// The locks on the key in the account.
pub fn key_locks(place: &dyn BackupPlace) -> NightResult<KeyLocks> {
    Ok(KeyLocks {
        phrase: place.get(KEY_PHRASE)?.ok_or(NightError::NoBackup)?,
        recovery: place.get(KEY_RECOVERY)?,
    })
}

/// Start the backup again in another account, with the same key, phrase and recovery key: the
/// next night there is sent in full, and the account's locks become `locks`, so a recovery key
/// they don't hold opens nothing there. An account whose nights all open with `key` (the person
/// chose it again) keeps them, unless another phone wrote a newer one, which stops this phone
/// before anything is written. An account holding anything else of a backup (a night the key
/// doesn't open, or locks or pieces with no night of this backup) is refused, so nothing is
/// written over it.
///
/// `tried` are the phrase locks earlier carries to this account may have left before a night was
/// written there: the account holding one of them is this backup's too.
pub fn carry_locks(
    place: &dyn BackupPlace,
    key: &NightKey,
    locks: &KeyLocks,
    tried: &[Vec<u8>],
    state: &NightState,
) -> NightResult<()> {
    let nights = load_nights(place, key)?;
    if foreign_nights(place, &nights)? {
        return Err(NightError::AlreadyThere);
    }
    if let Some(other) = newer_from_another(&nights, state) {
        return Err(NightError::AnotherPhone {
            made_at: other.body.made_at.clone(),
        });
    }
    if nights.is_empty() {
        let theirs = match place.get(KEY_PHRASE)? {
            Some(phrase) => phrase != locks.phrase && !tried.contains(&phrase),
            None => !place.list(KEY_RECOVERY)?.is_empty() || !place.list(PIECE)?.is_empty(),
        };
        if theirs {
            return Err(NightError::AlreadyThere);
        }
    }
    place.put(KEY_PHRASE, &locks.phrase)?;
    match &locks.recovery {
        Some(r) => place.put(KEY_RECOVERY, r)?,
        None => place.delete(KEY_RECOVERY)?,
    }
    Ok(())
}

/// Lock the key with a new phrase. The old phrase stops opening the backup; a recovery key
/// still does. The key must be the one the account's nights are locked with.
pub fn change_phrase(
    place: &dyn BackupPlace,
    key: &NightKey,
    phrase: &str,
    work_factor: Option<u8>,
) -> NightResult<()> {
    change_phrase_locked(place, key, phrase, work_factor).map(drop)
}

/// [`change_phrase`], with the lock it wrote.
pub fn change_phrase_locked(
    place: &dyn BackupPlace,
    key: &NightKey,
    phrase: &str,
    work_factor: Option<u8>,
) -> NightResult<Vec<u8>> {
    usable_phrase(phrase)?;
    let nights = place.list(NIGHT)?;
    if !nights.is_empty() && load_nights(place, key)?.is_empty() {
        return Err(NightError::Damaged);
    }
    let locked = phrase_lock(key, phrase, work_factor)?;
    place.put(KEY_PHRASE, &locked)?;
    Ok(locked)
}

/// The key, from the phrase the person typed.
pub fn open_with_phrase(place: &dyn BackupPlace, phrase: &str) -> NightResult<NightKey> {
    let phrase = usable_phrase(phrase).map_err(|_| NightError::WrongPhrase)?;
    let locked = place.get(KEY_PHRASE)?.ok_or(NightError::NoBackup)?;
    lock::unwrap(&locked, &phrase)
}

/// The key, from the recovery key the person typed.
pub fn open_with_recovery_key(place: &dyn BackupPlace, text: &str) -> NightResult<NightKey> {
    let Some(text) = normalise_recovery_key(text) else {
        return Err(NightError::WrongPhrase);
    };
    match place.get(KEY_RECOVERY)? {
        Some(locked) => lock::unwrap(&locked, &text),
        None if place.list(KEY_PHRASE)?.is_empty() => Err(NightError::NoBackup),
        None => Err(NightError::WrongPhrase),
    }
}

/// The newest backup in the account, before any phrase, or `None` when there is none.
pub fn found(place: &dyn BackupPlace) -> NightResult<Option<Found>> {
    let all = place.list("")?;
    let newest = all
        .iter()
        .filter_map(|e| parse_night(&e.name).map(|(count, _)| (count, e)))
        .max_by_key(|(count, _)| *count);
    let Some((_, night)) = newest else {
        return Ok(None);
    };
    if !all.iter().any(|e| e.name == KEY_PHRASE) {
        return Ok(None);
    }
    Ok(Some(Found {
        modified: night.modified,
        bytes: all.iter().map(|e| e.size).sum(),
    }))
}

/// The Nocturne a night says made it, the night of `count` or the newest, for "This backup is
/// from a newer Nocturne". Read without the checks a night must pass to be restored: a newer
/// Nocturne's night does not pass them, and nothing here is relied on but the words shown.
pub fn made_with(place: &dyn BackupPlace, key: &NightKey, count: Option<u64>) -> Option<String> {
    let listed = place.list(NIGHT).ok()?;
    let named: Vec<(u64, &str)> = listed
        .iter()
        .filter_map(|e| parse_night(&e.name).map(|(c, _)| (c, e.name.as_str())))
        .collect();
    let (_, name) = named
        .iter()
        .find(|(c, _)| Some(*c) == count)
        .or_else(|| named.iter().max_by_key(|(c, _)| *c))?;
    let locked = place.get(name).ok()??;
    if locked.len() > MAX_NIGHT {
        return None;
    }
    let plain = key.unlock(&locked).ok()?;
    let split = plain.iter().position(|&b| b == b'\n')?;
    let json: serde_json::Value = serde_json::from_slice(&plain[split + 1..]).ok()?;
    let version = json.get("appVersion")?.as_str()?;
    let shown = version.len() <= 32
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    shown.then(|| version.to_string())
}

/// The nights in the account as its listing shows them, newest first, before any phrase: their
/// count, when each was made and what each takes. Empty when the account holds no backup.
pub fn listed_nights(place: &dyn BackupPlace) -> NightResult<Vec<ListedNight>> {
    if !place.list(KEY_PHRASE)?.iter().any(|e| e.name == KEY_PHRASE) {
        return Ok(Vec::new());
    }
    let mut nights: Vec<ListedNight> = place
        .list(NIGHT)?
        .iter()
        .filter_map(|e| {
            let n = read_night_name(&e.name)?;
            Some(ListedNight {
                count: n.count,
                made_at: n
                    .made
                    .and_then(|s| jiff::Timestamp::from_second(s).ok())
                    .or(e.modified),
                bytes: n.bytes.map(|b| b + e.size),
            })
        })
        .collect();
    nights.sort_by_key(|n| std::cmp::Reverse(n.count));
    Ok(nights)
}

/// Every night the key opens, newest first, for "Which night?".
pub fn nights(place: &dyn BackupPlace, key: &NightKey) -> NightResult<Vec<NightSummary>> {
    Ok(load_nights(place, key)?
        .iter()
        .rev()
        .map(Night::summary)
        .collect())
}

/// The newest night the key opens, or `None` when there is none.
pub fn newest(place: &dyn BackupPlace, key: &NightKey) -> NightResult<Option<NightSummary>> {
    Ok(load_nights(place, key)?.last().map(Night::summary))
}

/// This phone becomes the one in use: its next night is the newest, and any other phone stops
/// when it next looks.
pub fn take_over(
    place: &dyn BackupPlace,
    key: &NightKey,
    state: &mut NightState,
) -> NightResult<()> {
    let nights = load_nights(place, key)?;
    if let Some(n) = nights.last() {
        state.count = state.count.max(n.body.count);
    }
    state.in_use = true;
    Ok(())
}

/// Remove the whole backup from the account: the locked keys first, so nothing left can be
/// opened even if the rest is cut short, then every night and piece.
pub fn delete_all(place: &dyn BackupPlace) -> NightResult<()> {
    place.delete(KEY_PHRASE)?;
    place.delete(KEY_RECOVERY)?;
    for prefix in [NIGHT, PIECE] {
        for e in place.list(prefix)? {
            place.delete(&e.name)?;
        }
    }
    Ok(())
}

/// Read the newest night back in full: every piece is fetched, opened and matched. Nothing is
/// written.
pub fn check_newest(
    place: &dyn BackupPlace,
    key: &NightKey,
    run: &NightRun,
) -> NightResult<NightSummary> {
    let nights = load_nights(place, key)?;
    let night = nights.last().ok_or(NightError::NoBackup)?;
    check_night(place, key, night, run)?;
    Ok(night.summary())
}

fn check_night(
    place: &dyn BackupPlace,
    key: &NightKey,
    night: &Night,
    run: &NightRun,
) -> NightResult<()> {
    check_body(place, key, &night.body, run)
}

fn check_body(
    place: &dyn BackupPlace,
    key: &NightKey,
    body: &Body,
    run: &NightRun,
) -> NightResult<()> {
    let total = body.pieces().map(|p| p.size).sum();
    let mut progress = Progress {
        phase: BackupPhase::Checking,
        done: 0,
        total,
        run,
    };
    (run.progress)(BackupPhase::Checking, 0, total);
    for r in body.pieces() {
        fetch_piece(place, key, r)?;
        progress.step(r.size)?;
    }
    Ok(())
}

/// A night brought back into a staging folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Unpacked {
    /// What it holds, counted from the store itself.
    pub summary: BackupSummary,
    /// Its columns for the phone.
    pub kept: KeptCounts,
    /// The night it came from.
    pub night: NightSummary,
    /// The state a phone restored from it starts with: a phone of its own, not in use until the
    /// person says.
    pub state: NightState,
}

/// Bring back the night of `count`, or the newest when `None`, into `stage` (a folder that must
/// not exist yet), as `stage/store`, checked in full and brought up to date the way a backup file
/// is. The caller puts it in the store's place on an empty phone, or brings its things in
/// ([`Engine::bring_in_backup`]) on a phone that holds a memory.
pub fn restore_night(
    place: &dyn BackupPlace,
    key: &NightKey,
    count: Option<u64>,
    stage: &Path,
    run: &NightRun,
) -> NightResult<Unpacked> {
    let nights = load_nights(place, key)?;
    let night = match count {
        Some(c) => nights.iter().find(|n| n.body.count == c),
        None => nights.last(),
    }
    .ok_or(NightError::NoBackup)?;
    crate::private::create_dir_new(stage)?;
    let mut guard = Scratch::new(stage);
    let store_dir = stage.join("store");
    crate::private::create_dir_new(&store_dir)?;
    let originals = store_dir.join("originals");
    crate::private::create_dir_new(&originals)?;

    let body = &night.body;
    let total = body.pieces().map(|p| p.size).sum();
    let mut progress = Progress {
        phase: BackupPhase::Restoring,
        done: 0,
        total,
        run,
    };
    (run.progress)(BackupPhase::Restoring, 0, total);
    let mut write = |path: &Path, refs: &[PieceRef], want: Option<(&str, u64)>| {
        let mut out = fsutil::create_private_file_new(path)?;
        let mut hasher = Sha256::new();
        let mut size = 0u64;
        for r in refs {
            let plain = fetch_piece(place, key, r)?;
            std::io::Write::write_all(&mut out, &plain)?;
            hasher.update(&plain);
            size += plain.len() as u64;
            progress.step(r.size)?;
        }
        out.sync_all()?;
        if let Some((sha, expected)) = want
            && (hex(&hasher.finalize()) != sha || size != expected)
        {
            return Err(NightError::Damaged);
        }
        NightResult::Ok(())
    };
    write(&store_dir.join(crate::DB_FILE), &body.store, None)?;
    let mut seen = BTreeSet::new();
    for f in &body.files {
        if !crate::backup::is_sha256(&f.sha) || !seen.insert(f.sha.as_str()) {
            return Err(NightError::Damaged);
        }
        write(
            &originals.join(&f.sha),
            &f.pieces,
            Some((f.sha.as_str(), f.size)),
        )?;
    }
    // The whole night is checked as a backup file is before anything on the phone changes.
    (run.progress)(BackupPhase::Checking, 0, total);
    let (memories, items, pages, kept) = crate::backup::check_unpacked(&store_dir, true)?;
    if run.cancel.load(Ordering::SeqCst) {
        return Err(NightError::Cancelled);
    }
    (run.progress)(BackupPhase::Checking, total, total);
    guard.keep = true;
    let summary = BackupSummary {
        made_at: body.made_at.clone(),
        app_version: body.app_version.clone(),
        store_version: body.store_version,
        memories,
        items,
        pages,
        originals: body.files.len() as u32,
        online_used: false,
        away_used: false,
        bytes: night.bytes() as f64,
    };
    let state = NightState {
        device: random_hex(8),
        count: nights.last().map_or(body.count, |n| n.body.count),
        in_use: false,
    };
    Ok(Unpacked {
        summary,
        kept,
        night: night.summary(),
        state,
    })
}

impl Engine {
    /// Write tonight's night: what changed goes as locked pieces, then the night that lists
    /// them all, then nights past the seventh, or every older one after a forget, go with the
    /// pieces no kept night lists. `state` moves on to the new night; the caller keeps it.
    pub fn back_up_night(
        &self,
        place: &dyn BackupPlace,
        key: &NightKey,
        state: &mut NightState,
        run: &NightRun,
    ) -> NightResult<NightReport> {
        if !state.in_use {
            return Err(NightError::NotInUse);
        }
        let nights = load_nights(place, key)?;
        if let Some(other) = newer_from_another(&nights, state) {
            return Err(NightError::AnotherPhone {
                made_at: other.body.made_at.clone(),
            });
        }
        // A night the key doesn't open is another backup's, or damaged: the pieces it lists
        // would go as ones no kept night lists.
        if foreign_nights(place, &nights)? {
            return Err(NightError::Damaged);
        }

        if !place.list(KEY_PHRASE)?.iter().any(|e| e.name == KEY_PHRASE) {
            // Nothing could open what is written: the person must hear of it.
            return Err(NightError::Damaged);
        }
        // A forget still finishing has hidden its memories but not yet removed them: they would
        // go into this night while its mark is cleared.
        let forgetting: bool = self.read(|conn| {
            Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM forget_jobs)", [], |r| r.get(0))?)
        })?;
        if forgetting {
            return Err(NightError::Busy);
        }
        // Read before the snapshot: a forget after it marks the store again and stays marked.
        let forgot = self.read(forgot_mark)?;
        let generation = self.inner.forget_gen.load(Ordering::SeqCst);
        let scratch = Scratch::new(&self.inner.dir.join(SCRATCH));
        fsutil::shred_tree(&scratch.dir);
        fsutil::create_private_dir(&scratch.dir)?;
        let (snapshot, snap) = self.take_snapshot(&scratch.dir)?;

        let held: BTreeSet<String> = place
            .list(PIECE)?
            .into_iter()
            .filter_map(|e| e.name.strip_prefix(PIECE).map(str::to_string))
            .collect();
        let mut known: HashMap<String, PieceRef> = HashMap::new();
        let mut known_files: HashMap<String, FileRef> = HashMap::new();
        for n in &nights {
            for r in n.body.pieces() {
                if held.contains(&r.piece) {
                    known.insert(r.hash.clone(), r.clone());
                }
            }
            for f in &n.body.files {
                if f.pieces.iter().all(|r| held.contains(&r.piece)) {
                    known_files.insert(f.sha.clone(), f.clone());
                }
            }
        }

        let new_files: Vec<_> = snap
            .originals
            .iter()
            .filter(|o| known_files.get(&o.sha).is_none_or(|f| f.size != o.size))
            .collect();
        let total = snap.size + new_files.iter().map(|o| o.size).sum::<u64>();
        let mut progress = Progress {
            phase: BackupPhase::Saving,
            done: 0,
            total,
            run,
        };
        (run.progress)(BackupPhase::Saving, 0, total);
        if run.cancel.load(Ordering::SeqCst) {
            return Err(NightError::Cancelled);
        }

        let cutter = Cutter::new(key.gear_seed());
        let mut sender = Sender {
            place,
            key,
            known,
            sent: 0,
            new: Vec::new(),
        };
        let mut store = Vec::new();
        cutter.pieces(&mut std::fs::File::open(&snapshot)?, |plain| {
            store.push(sender.piece(plain)?);
            progress.step(plain.len() as u64)
        })?;

        let mut files = Vec::new();
        for original in &snap.originals {
            if let Some(f) = known_files.get(&original.sha)
                && f.size == original.size
            {
                files.push(f.clone());
                continue;
            }
            let mut source = match std::fs::File::open(&original.path) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            let mut hasher = Sha256::new();
            let mut size = 0u64;
            let mut refs = Vec::new();
            cutter.pieces(&mut (&mut source).take(original.size + 1), |plain| {
                hasher.update(plain);
                size += plain.len() as u64;
                refs.push(sender.piece(plain)?);
                progress.step(plain.len() as u64)
            })?;
            if size != original.size || hex(&hasher.finalize()) != original.sha {
                return Err(NightError::Failed(
                    "an original does not match what was kept".into(),
                ));
            }
            files.push(FileRef {
                sha: original.sha.clone(),
                size,
                pieces: refs,
            });
        }

        let count = nights
            .iter()
            .map(|n| n.body.count)
            .chain([state.count])
            .max()
            .unwrap_or(0)
            + 1;
        let made = self.clock().now();
        let body = Body {
            format: FORMAT,
            device: state.device.clone(),
            count,
            made_at: fmt_ts(made),
            app_version: APP_VERSION.to_string(),
            store_version: snap.version,
            memories: snap.memories,
            items: snap.items,
            pages: snap.pages,
            first: snap.first.clone(),
            store,
            files,
        };
        // A night lists only pieces that read back as they were sent: on the first night every
        // piece, read back in full ([`NightRun::check`]); on the others, those sent tonight.
        if run.check {
            check_body(place, key, &body, run)?;
        } else {
            for r in &sender.new {
                fetch_piece(place, key, r)?;
                if run.cancel.load(Ordering::SeqCst) {
                    return Err(NightError::Cancelled);
                }
            }
        }
        let name = night_name(count, &state.device, made.as_second(), body.pieces_bytes());
        let locked = seal_night(key, &name, &body)?;
        // A forget began while the pieces were made: they may hold what the person just removed,
        // so no night lists them, and the next prune takes them.
        if self.inner.forget_gen.load(Ordering::SeqCst) != generation {
            return Err(NightError::Busy);
        }
        place.put(&name, &locked)?;
        sender.sent += locked.len() as u64;
        // A night is a backup once it reads back as written.
        let back = place.get(&name)?.ok_or(NightError::Damaged)?;
        if back != locked || open_night(key, &name, &back).is_err() {
            return Err(NightError::Damaged);
        }
        state.count = count;
        let night = Night {
            name,
            locked: locked.len() as u64,
            body,
        };
        drop(scratch);

        let carried_forget = forgot.is_some();
        let mut kept: Vec<&Night> = if carried_forget {
            Vec::new()
        } else {
            nights.iter().rev().take(NIGHTS_KEPT - 1).collect()
        };
        kept.push(&night);
        let keep_names: BTreeSet<&str> = kept.iter().map(|n| n.name.as_str()).collect();
        let listed: BTreeSet<&str> = kept
            .iter()
            .flat_map(|n| n.body.pieces())
            .map(|p| p.piece.as_str())
            .collect();
        let pruned = (|| -> NightResult<()> {
            for n in &nights {
                if !keep_names.contains(n.name.as_str()) {
                    place.delete(&n.name)?;
                }
            }
            for piece in &held {
                if !listed.contains(piece.as_str()) {
                    place.delete(&format!("{PIECE}{piece}"))?;
                }
            }
            Ok(())
        })();
        if let Err(e) = pruned {
            // The night is whole; what was not removed goes with the next one, and a forget's
            // mark stays so the next night removes it.
            tracing::warn!("older nights could not all be removed yet: {e}");
            return Ok(Self::report(&night, sender.sent, false));
        }
        if let Some(mark) = &forgot {
            self.write(|tx| {
                tx.execute(
                    "DELETE FROM settings WHERE key = ?1 AND value = ?2",
                    [FORGOT, mark.as_str()],
                )?;
                Ok(())
            })?;
        }

        Ok(Self::report(&night, sender.sent, carried_forget))
    }

    fn report(night: &Night, sent: u64, carried_forget: bool) -> NightReport {
        NightReport {
            made_at: night.body.made_at.clone(),
            count: night.body.count,
            bytes: night.bytes(),
            sent,
            memories: night.body.memories,
            items: night.body.items,
            pages: night.body.pages,
            originals: night.body.files.len() as u32,
            carried_forget,
        }
    }

    /// Whether something was forgotten for good since the last night, so the next one should
    /// run at once.
    pub fn forgot_since_night(&self) -> crate::Result<bool> {
        Ok(self.read(forgot_mark)?.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_says_its_count_phone_day_and_size() {
        let name = night_name(12, "0123456789abcdef", 1_763_003_520, 4_096);
        let n = read_night_name(&name).unwrap();
        assert_eq!(n.count, 12);
        assert_eq!(n.device, "0123456789abcdef");
        assert_eq!(n.made, Some(1_763_003_520));
        assert_eq!(n.bytes, Some(4_096));
        let old = read_night_name("night-000000000003-0123456789abcdef").unwrap();
        assert_eq!((old.count, old.made, old.bytes), (3, None, None));
        let later = read_night_name("night-000000000004-0123456789abcdef-12-34-more").unwrap();
        assert_eq!(
            (later.count, later.made, later.bytes),
            (4, Some(12), Some(34)),
            "a later Nocturne's name still counts"
        );
        for bad in [
            "night-3-0123456789abcdef",
            "night-000000000003-0123456789abcdeg",
        ] {
            assert!(read_night_name(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_newer_nocturne_s_night_is_refused_and_names_its_maker() {
        let place = MemoryPlace::new();
        let key = NightKey::generate();
        let name = "night-000000000002-0123456789abcdef-1763003520-0";
        let json = serde_json::json!({ "format": 2, "appVersion": "9.1.0", "whatever": true });
        let json = serde_json::to_vec(&json).unwrap();
        let mac = key.mac(&[name.as_bytes(), &[0], &json]);
        let mut plain = hex(&mac).into_bytes();
        plain.push(b'\n');
        plain.extend_from_slice(&json);
        place.put(name, &key.lock(&plain).unwrap()).unwrap();
        assert!(matches!(nights(&place, &key), Err(NightError::TooNew)));
        assert_eq!(made_with(&place, &key, None).as_deref(), Some("9.1.0"));
        assert_eq!(made_with(&place, &key, Some(2)).as_deref(), Some("9.1.0"));
        assert_eq!(made_with(&place, &NightKey::generate(), None), None);
    }

    #[test]
    fn a_night_named_and_written_before_its_day_and_size_still_reads() {
        let place = MemoryPlace::new();
        let key = NightKey::generate();
        let name = "night-000000000001-0123456789abcdef";
        // A night as the first build wrote it: no first day in the body, none in the name.
        let body: Body = serde_json::from_value(serde_json::json!({
            "format": 1,
            "device": "0123456789abcdef",
            "count": 1,
            "madeAt": "2026-11-20T03:00:00Z",
            "appVersion": "0.8.0",
            "storeVersion": 1,
            "memories": 0,
            "items": 3,
            "pages": 0,
            "store": [],
            "files": []
        }))
        .unwrap();
        place
            .put(name, &seal_night(&key, name, &body).unwrap())
            .unwrap();
        place.put(KEY_PHRASE, b"locked").unwrap();
        let opened = nights(&place, &key).unwrap();
        assert_eq!(opened.len(), 1);
        assert_eq!((opened[0].items, opened[0].first.clone()), (3, None));
        let listed = listed_nights(&place).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!((listed[0].count, listed[0].bytes), (1, None));
    }
}
