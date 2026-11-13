//! The watched folder: files that appear in it after it is chosen are kept, once each.
//!
//! The folder is only ever read. Nothing in it is written, moved or deleted: a kept file is
//! copied into the store like any other. Files already there when the folder is chosen go into
//! the ledger without being kept. A file is kept only once it has stopped changing (size and
//! modified time the same on two looks a few seconds apart), so a half-synced file is not.
//!
//! [`Scanner`] is pure: it is given what is in the folder and the time and says what to keep.
//! The rest reads the folder, wakes on changes (with a slower rescan as the backstop) and keeps
//! files in small batches so capture stays quick.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use nocturne_contract::Source;
use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::Notify;

use super::state::OsState;
use crate::error::{CommandError, ErrorCode};

/// The most files remembered.
pub const LEDGER_CAP: usize = 100_000;
/// How deep below the chosen folder files are looked for.
pub const MAX_DEPTH: usize = 4;
/// How many files are kept at once.
pub const BATCH: usize = 20;

/// How often the folder is looked at.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// How long a file must be unchanged before it is kept.
    pub stable_after: Duration,
    /// The slow look that backs up change notifications.
    pub rescan: Duration,
    /// How long to wait after a change notification, so a burst is one look.
    pub debounce: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            stable_after: Duration::from_secs(3),
            rescan: Duration::from_secs(60),
            debounce: Duration::from_millis(500),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InboxError {
    #[error("choose a folder by its full path")]
    NotAbsolute,
    #[error("that folder is not there")]
    Missing,
    #[error("choose a folder, not the whole computer or your home folder")]
    TooBroad,
    #[error("that folder overlaps where Nocturne keeps its own files")]
    Overlap,
    #[error("that folder has too many files to watch")]
    TooMany,
}

/// A folder the person may choose: it exists, is a full path, is not the root or the home
/// folder itself, and does not contain (or sit inside) Nocturne's own data. Returns the real
/// location, with links resolved, so the overlap checks compare like with like.
pub fn validate(folder: &str, data_dir: &Path, home: Option<&Path>) -> Result<PathBuf, InboxError> {
    let path = Path::new(folder.trim());
    if !path.is_absolute() {
        return Err(InboxError::NotAbsolute);
    }
    let real = std::fs::canonicalize(path).map_err(|_| InboxError::Missing)?;
    if !real.is_dir() {
        return Err(InboxError::Missing);
    }
    let home = home.and_then(|h| std::fs::canonicalize(h).ok());
    if real.parent().is_none() || home.is_some_and(|h| h == real) {
        return Err(InboxError::TooBroad);
    }
    let data = std::fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
    if real.starts_with(&data) || data.starts_with(&real) {
        return Err(InboxError::Overlap);
    }
    Ok(real)
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

const PARTIAL_SUFFIXES: &[&str] = &[".part", ".crdownload", ".download", ".tmp"];

/// Whether a file name is one that may be kept: a kind Nocturne reads, not hidden, not a
/// partial download or sync temporary, not an editor's lock or backup.
pub fn eligible(path: &Path) -> bool {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_lowercase()) else {
        return false;
    };
    if name.starts_with('.') || name.starts_with('~') || name.ends_with('~') {
        return false;
    }
    if name.contains(".syncthing.") || PARTIAL_SUFFIXES.iter().any(|s| name.ends_with(s)) {
        return false;
    }
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| nocturne_engine::kind_for_ext(&e).is_some())
}

/// One file in the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The path below the chosen folder.
    pub rel: String,
    pub size: u64,
    /// Modified time in milliseconds since the epoch.
    pub mtime: i64,
}

/// What a look at the folder found.
#[derive(Debug, Default)]
pub struct Listing {
    pub entries: Vec<Entry>,
    /// False when part of the folder could not be read or the file count hit the limit.
    pub complete: bool,
}

/// Look at the folder: regular files that could be kept, to [`MAX_DEPTH`], skipping hidden
/// folders and anything that is a link (a link could lead anywhere).
pub fn list(root: &Path, limit: usize) -> std::io::Result<Listing> {
    std::fs::read_dir(root)?;
    let mut out = Listing {
        entries: Vec::new(),
        complete: true,
    };
    walk(root, root, 0, limit, &mut out);
    Ok(out)
}

fn walk(root: &Path, dir: &Path, depth: usize, limit: usize, out: &mut Listing) {
    let Ok(read) = std::fs::read_dir(dir) else {
        out.complete = false;
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            out.complete = false;
            continue;
        };
        let kind = meta.file_type();
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            if !hidden && depth < MAX_DEPTH {
                walk(root, &path, depth + 1, limit, out);
            }
        } else if kind.is_file() && eligible(&path) {
            if meta.len() == 0 || meta.len() > crate::commands::MAX_FILE_BYTES {
                continue;
            }
            if out.entries.len() >= limit {
                out.complete = false;
                return;
            }
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis() as i64);
            out.entries.push(Entry {
                rel: rel.to_string_lossy().into_owned(),
                size: meta.len(),
                mtime,
            });
        }
    }
}

/// The real path of a listed file, if it is still a plain file inside `root` right now.
///
/// A listing is a moment in the past: by the time a file is kept, it (or a folder on its way)
/// may have been swapped for a link to somewhere else. So just before keeping, every folder
/// between `root` and the file must still be a real folder, the file must still be a regular
/// file, and its real path must still be under the real `root`. What is left is a window of
/// time between this check and the open inside the store. A batch confirms each file right
/// before that file's own keep, so the gap is one file's keep, not the whole batch's. Only
/// another process of the same user could use it, and that process could read the file itself.
/// (Opening without following links needs `libc`, which this app does not depend on.)
pub fn confirm_inside(root: &Path, rel: &str) -> Option<PathBuf> {
    let root = std::fs::canonicalize(root).ok()?;
    let mut at = root.clone();
    let mut parts = Path::new(rel).components().peekable();
    while let Some(part) = parts.next() {
        let std::path::Component::Normal(name) = part else {
            return None;
        };
        at.push(name);
        let kind = std::fs::symlink_metadata(&at).ok()?.file_type();
        let last = parts.peek().is_none();
        if kind.is_symlink() || (last && !kind.is_file()) || (!last && !kind.is_dir()) {
            return None;
        }
    }
    let real = std::fs::canonicalize(&at).ok()?;
    real.starts_with(&root).then_some(real)
}

/// Whether the folder holds more files than the ledger can remember.
pub fn too_many(root: &Path, cap: usize) -> bool {
    list(root, cap).is_ok_and(|l| !l.complete && l.entries.len() >= cap)
}

/// One file dealt with: a salted hash of its path below the folder, and what it looked like.
/// No name is stored: the ledger outlives forgetting, so it must not say what was in the folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub h: String,
    pub size: u64,
    pub mtime: i64,
}

/// The files already dealt with in a folder, kept in `os-state.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxLedger {
    /// Random per ledger, so the hashes mean nothing outside it.
    pub salt: String,
    /// The hash of the folder's path, to tell whether the ledger belongs to it.
    pub folder: String,
    /// The newest files, at most [`LEDGER_CAP`].
    pub entries: Vec<LedgerEntry>,
}

/// FNV-1a over the salt and the text: stable across versions of Rust, which a ledger kept on
/// disk needs, and enough to keep names out of a file that is not meant to be read.
fn digest(salt: &str, text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in salt.bytes().chain([0]).chain(text.bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn fresh_salt() -> String {
    let mut bytes = [0u8; 8];
    if getrandom::fill(&mut bytes).is_err() {
        bytes = Timestamp::now().as_nanosecond().to_le_bytes()[..8]
            .try_into()
            .unwrap_or_default();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// How many times a file that could not be kept, for a reason that may not pass, is tried.
/// A busy store, an unavailable engine or a full disk is not counted: those are waited out.
pub const MAX_TRIES: u8 = 20;

/// The longest wait between two tries of the same file.
const MAX_BACKOFF: SignedDuration = SignedDuration::from_secs(600);

/// The wait after the `failures`th failure in a row: the base, doubled each time, up to
/// [`MAX_BACKOFF`].
fn backoff(base: SignedDuration, failures: u32) -> SignedDuration {
    let doubled = base
        .as_millis()
        .saturating_mul(1i128 << failures.saturating_sub(1).min(30));
    SignedDuration::from_millis(doubled.min(MAX_BACKOFF.as_millis()) as i64)
}

#[derive(Debug, Clone)]
struct Pending {
    size: u64,
    mtime: i64,
    looked: Timestamp,
    /// Failures that count towards giving up.
    tries: u8,
    /// Failures in a row of any kind, for the wait between tries.
    failures: u32,
    /// Not tried again before this.
    retry_at: Option<Timestamp>,
}

impl Pending {
    fn new(e: &Entry, looked: Timestamp) -> Pending {
        Pending {
            size: e.size,
            mtime: e.mtime,
            looked,
            tries: 0,
            failures: 0,
            retry_at: None,
        }
    }
}

/// Decides which files are new and stable. Knows nothing about disks or clocks.
#[derive(Debug)]
pub struct Scanner {
    folder: String,
    salt: String,
    ledger: BTreeMap<String, (u64, i64)>,
    pending: BTreeMap<String, Pending>,
    baselined: bool,
}

impl Scanner {
    /// A scanner for `folder`, resuming from `saved` when it is the same folder.
    pub fn new(folder: &str, saved: Option<&InboxLedger>) -> Scanner {
        let saved = saved.filter(|l| !l.salt.is_empty() && l.folder == digest(&l.salt, folder));
        let salt = saved.map_or_else(fresh_salt, |l| l.salt.clone());
        Scanner {
            folder: digest(&salt, folder),
            ledger: saved
                .map(|l| {
                    l.entries
                        .iter()
                        .map(|e| (e.h.clone(), (e.size, e.mtime)))
                        .collect()
                })
                .unwrap_or_default(),
            salt,
            pending: BTreeMap::new(),
            baselined: saved.is_some(),
        }
    }

    fn key(&self, rel: &str) -> String {
        digest(&self.salt, rel)
    }

    /// True until the files that were there at the start have been put in the ledger.
    pub fn needs_baseline(&self) -> bool {
        !self.baselined
    }

    /// Remember what is already in the folder, without keeping any of it.
    pub fn baseline(&mut self, entries: &[Entry]) {
        for e in entries {
            self.ledger.insert(self.key(&e.rel), (e.size, e.mtime));
        }
        self.baselined = true;
    }

    /// Look at the folder at `now`. Returns the new files that have not changed for
    /// `stable_after`. They stay pending until [`Scanner::settle`].
    pub fn observe(
        &mut self,
        entries: &[Entry],
        now: Timestamp,
        stable_after: SignedDuration,
    ) -> Vec<String> {
        let present: BTreeSet<&str> = entries.iter().map(|e| e.rel.as_str()).collect();
        self.pending.retain(|rel, _| present.contains(rel.as_str()));
        let mut ready = Vec::new();
        for e in entries {
            if self.ledger.contains_key(&self.key(&e.rel)) {
                continue;
            }
            match self.pending.get_mut(&e.rel) {
                Some(p) if p.size == e.size && p.mtime == e.mtime => {
                    if now.duration_since(p.looked) >= stable_after
                        && p.retry_at.is_none_or(|at| now >= at)
                    {
                        ready.push(e.rel.clone());
                    }
                }
                Some(p) => *p = Pending::new(e, now),
                None => {
                    self.pending.insert(e.rel.clone(), Pending::new(e, now));
                }
            }
        }
        ready.sort();
        ready
    }

    /// A file has been dealt with (kept, or given up on): never look at it as new again.
    pub fn settle(&mut self, rel: &str) {
        if let Some(p) = self.pending.remove(rel) {
            self.ledger.insert(self.key(rel), (p.size, p.mtime));
        }
    }

    /// A file could not be kept for a reason that may pass. It stays pending and is tried again
    /// after a wait that doubles from `base` up to ten minutes. When `counts` it also moves
    /// towards [`MAX_TRIES`], after which it is given up on; a failure that is the engine's or
    /// the disk's, not the file's, does not count. Returns true when it was given up on.
    pub fn failed(
        &mut self,
        rel: &str,
        now: Timestamp,
        base: SignedDuration,
        counts: bool,
    ) -> bool {
        let gave_up = self.pending.get_mut(rel).is_some_and(|p| {
            p.failures = p.failures.saturating_add(1);
            p.retry_at = Some(now + backoff(base, p.failures));
            if counts {
                p.tries += 1;
            }
            p.tries >= MAX_TRIES
        });
        if gave_up {
            self.settle(rel);
        }
        gave_up
    }

    /// Forget files that are no longer in the folder, so the ledger stays the size of the folder.
    pub fn prune(&mut self, entries: &[Entry]) {
        let present: BTreeSet<String> = entries.iter().map(|e| self.key(&e.rel)).collect();
        self.ledger.retain(|h, _| present.contains(h));
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// What to save: the newest [`LEDGER_CAP`] files, so a folder past the cap still remembers
    /// what it saw last.
    pub fn ledger(&self) -> InboxLedger {
        let mut entries: Vec<LedgerEntry> = self
            .ledger
            .iter()
            .map(|(h, (size, mtime))| LedgerEntry {
                h: h.clone(),
                size: *size,
                mtime: *mtime,
            })
            .collect();
        entries.sort_by(|a, b| b.mtime.cmp(&a.mtime).then_with(|| a.h.cmp(&b.h)));
        entries.truncate(LEDGER_CAP);
        InboxLedger {
            salt: self.salt.clone(),
            folder: self.folder.clone(),
            entries,
        }
    }
}

/// A running watcher, stopped by asking it to and waiting for it.
pub struct Handle {
    cancel: Arc<Cancel>,
    task: tauri::async_runtime::JoinHandle<()>,
}

#[derive(Default)]
struct Cancel {
    flag: AtomicBool,
    wake: Notify,
}

impl Cancel {
    fn stop(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.wake.notify_waiters();
        self.wake.notify_one();
    }

    fn stopped(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// Sleep, or less if asked to stop.
    async fn sleep(&self, d: Duration) {
        if self.stopped() {
            return;
        }
        tokio::select! {
            () = tokio::time::sleep(d) => {}
            () = self.wake.notified() => {}
        }
    }
}

impl Handle {
    /// Ask the watcher to stop and wait until it has: nothing is being kept when this returns.
    pub async fn stop(self) {
        self.cancel.stop();
        let _ = self.task.await;
    }
}

/// Watch `root` and keep what appears there. Replaces any watcher that is running, after it
/// has stopped.
pub async fn start<R: Runtime>(app: &AppHandle<R>, root: PathBuf) {
    let Some(os) = app.try_state::<OsState>() else {
        return;
    };
    stop(app).await;
    let timing = os.inbox_timing();
    let cancel = Arc::new(Cancel::default());
    let handle = app.clone();
    let token = cancel.clone();
    let task = tauri::async_runtime::spawn(async move { run(handle, root, timing, token).await });
    os.set_inbox(Some(Handle { cancel, task }));
}

/// Stop watching, and wait until nothing is being kept any more.
pub async fn stop<R: Runtime>(app: &AppHandle<R>) {
    let Some(os) = app.try_state::<OsState>() else {
        return;
    };
    if let Some(handle) = os.take_inbox() {
        handle.stop().await;
    }
}

type Wake = (
    notify::RecommendedWatcher,
    tokio::sync::mpsc::UnboundedReceiver<()>,
);

/// A change watcher that does not follow links (a link could lead the watcher anywhere).
fn wake_on_changes(root: &Path) -> Option<Wake> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut watcher = notify::RecommendedWatcher::new(
        move |event: notify::Result<notify::Event>| {
            if event.is_ok() {
                let _ = tx.send(());
            }
        },
        notify::Config::default().with_follow_symlinks(false),
    )
    .ok()?;
    watcher.watch(root, RecursiveMode::Recursive).ok()?;
    Some((watcher, rx))
}

/// Setting a watch walks the folder, which can take a while: off the async threads.
async fn watch(root: &Path) -> Option<Wake> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || wake_on_changes(&root))
        .await
        .ok()
        .flatten()
}

async fn run<R: Runtime>(app: AppHandle<R>, root: PathBuf, timing: Timing, cancel: Arc<Cancel>) {
    let Some(os) = app.try_state::<OsState>() else {
        return;
    };
    let mut root = root;
    let mut scanner = Scanner::new(&root.to_string_lossy(), os.file().inbox.as_ref());
    let mut changes = watch(&root).await;
    if changes.is_none() {
        tracing::info!("the watched folder will be looked at on a timer only");
    }
    let stable =
        SignedDuration::try_from(timing.stable_after).unwrap_or(SignedDuration::from_secs(3));
    let mut reported_missing = false;
    let mut checked = false;
    while !cancel.stopped() {
        let looked = {
            let root = root.clone();
            tokio::task::spawn_blocking(move || list(&root, LEDGER_CAP))
                .await
                .map_err(std::io::Error::other)
                .and_then(|r| r)
        };
        match looked {
            Ok(listing) => {
                reported_missing = false;
                if !checked {
                    // A folder that was not there at start is checked when it first appears.
                    match validate(
                        &root.to_string_lossy(),
                        os.data_dir(),
                        home_dir().as_deref(),
                    ) {
                        Ok(real) => {
                            if real != root {
                                root = real;
                                scanner =
                                    Scanner::new(&root.to_string_lossy(), os.file().inbox.as_ref());
                            }
                            checked = true;
                        }
                        Err(e) => {
                            tracing::warn!("the watched folder is not used: {e}");
                            return;
                        }
                    }
                }
                if changes.is_none() {
                    changes = watch(&root).await;
                }
                if scanner.needs_baseline() {
                    scanner.baseline(&listing.entries);
                    os.update(|f| f.inbox = Some(scanner.ledger()));
                    tracing::info!(
                        "watching a folder: {} files already there",
                        listing.entries.len()
                    );
                } else {
                    let ready = scanner.observe(&listing.entries, Timestamp::now(), stable);
                    if !ready.is_empty()
                        && let Some(hook) = os.before_keep()
                    {
                        hook(&ready);
                    }
                    for batch in ready.chunks(BATCH) {
                        if cancel.stopped() {
                            return;
                        }
                        keep_batch(&app, &root, batch, &mut scanner, &cancel, stable).await;
                        os.update(|f| f.inbox = Some(scanner.ledger()));
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    if listing.complete {
                        scanner.prune(&listing.entries);
                    }
                }
            }
            Err(_) => {
                if !reported_missing {
                    tracing::info!("the watched folder is not there; looking again later");
                    reported_missing = true;
                }
                changes = None;
            }
        }
        let wait = if scanner.has_pending() {
            timing.stable_after
        } else {
            timing.rescan
        };
        match changes.as_mut() {
            Some((_, rx)) => {
                tokio::select! {
                    () = cancel.sleep(wait) => {}
                    woke = rx.recv() => {
                        if woke.is_some() {
                            cancel.sleep(timing.debounce).await;
                            while rx.try_recv().is_ok() {}
                        } else {
                            changes = None;
                        }
                    }
                }
            }
            None => cancel.sleep(wait).await,
        }
    }
}

/// What to do with a file that could not be kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// Never look at it as new again.
    Settle,
    /// Try again later; `counts` when the failure moves it towards being given up on.
    Retry { counts: bool },
}

/// Only a file that is gone, or that can never be kept, is settled. A busy store, an unavailable
/// engine or a full disk is waited out and does not count. A NotFound from the store while the
/// file is still there is not the file's absence: it is tried again, and counted.
fn outcome(code: ErrorCode, still_there: bool) -> Outcome {
    match code {
        ErrorCode::NotFound if !still_there => Outcome::Settle,
        ErrorCode::Invalid => Outcome::Settle,
        ErrorCode::Unavailable | ErrorCode::Internal | ErrorCode::Busy => {
            Outcome::Retry { counts: false }
        }
        _ => Outcome::Retry { counts: true },
    }
}

/// Keep one batch of stable files. Each is confirmed right before its own keep, inside the
/// blocking work: it may have become a link since it was listed, and a long batch must not widen
/// that window for the files at its end. What cannot be kept for good is given up on; what
/// failed for a reason that may pass is tried again later, after a wait that grows.
async fn keep_batch<R: Runtime>(
    app: &AppHandle<R>,
    root: &Path,
    batch: &[String],
    scanner: &mut Scanner,
    cancel: &Cancel,
    base: SignedDuration,
) {
    if cancel.stopped() {
        return;
    }
    let owned = root.to_path_buf();
    let results = crate::keep_each(app, batch.to_vec(), Source::WatchedFolder, move |rel| {
        confirm_inside(&owned, rel).ok_or_else(|| CommandError::not_found("not a plain file"))
    })
    .await;
    let Ok(results) = results else {
        for rel in batch {
            scanner.failed(rel, Timestamp::now(), base, false);
        }
        return;
    };
    for (rel, result) in batch.iter().zip(results) {
        let Err(e) = result else {
            scanner.settle(rel);
            continue;
        };
        // The store reports its own missing things as NotFound too: the file is gone only if it
        // is no longer there as a plain file.
        let still_there = e.code == ErrorCode::NotFound && confirm_inside(root, rel).is_some();
        let counts = match outcome(e.code, still_there) {
            Outcome::Settle => {
                tracing::debug!("a file in the watched folder cannot be kept: {:?}", e.code);
                scanner.settle(rel);
                continue;
            }
            Outcome::Retry { counts } => counts,
        };
        let gave_up = scanner.failed(rel, Timestamp::now(), base, counts);
        tracing::warn!(
            "a file in the watched folder was not kept ({:?}){}",
            e.code,
            if gave_up {
                ": given up"
            } else {
                ": trying again"
            }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rel: &str, size: u64, mtime: i64) -> Entry {
        Entry {
            rel: rel.into(),
            size,
            mtime,
        }
    }

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    const STABLE: SignedDuration = SignedDuration::from_secs(3);

    #[test]
    fn existing_files_are_not_kept() {
        let mut s = Scanner::new("/f", None);
        assert!(s.needs_baseline());
        let before = [entry("a.pdf", 10, 1), entry("b.txt", 5, 2)];
        s.baseline(&before);
        assert!(!s.needs_baseline());
        for sec in 0..10 {
            let now = at("2026-11-13T09:00:00Z") + SignedDuration::from_secs(sec);
            assert!(s.observe(&before, now, STABLE).is_empty());
        }
    }

    #[test]
    fn new_file_kept_once_after_stable() {
        let mut s = Scanner::new("/f", None);
        s.baseline(&[]);
        let t0 = at("2026-11-13T09:00:00Z");
        let secs = |n| t0 + SignedDuration::from_secs(n);
        assert!(
            s.observe(&[entry("a.pdf", 10, 1)], t0, STABLE).is_empty(),
            "first look"
        );
        assert!(
            s.observe(&[entry("a.pdf", 20, 2)], secs(4), STABLE)
                .is_empty(),
            "it grew"
        );
        assert!(
            s.observe(&[entry("a.pdf", 20, 2)], secs(6), STABLE)
                .is_empty(),
            "same, but not for long enough"
        );
        let ready = s.observe(&[entry("a.pdf", 20, 2)], secs(8), STABLE);
        assert_eq!(ready, ["a.pdf"]);
        s.settle("a.pdf");
        assert!(
            s.observe(&[entry("a.pdf", 20, 2)], secs(70), STABLE)
                .is_empty()
        );

        // A restart with the saved ledger does not keep it again.
        let saved = s.ledger();
        let mut again = Scanner::new("/f", Some(&saved));
        assert!(!again.needs_baseline());
        assert!(
            again
                .observe(&[entry("a.pdf", 20, 2)], secs(100), STABLE)
                .is_empty()
        );
        assert!(
            again
                .observe(&[entry("a.pdf", 20, 2)], secs(200), STABLE)
                .is_empty()
        );
        // Another folder starts from nothing.
        assert!(Scanner::new("/other", Some(&saved)).needs_baseline());
    }

    #[test]
    fn the_ledger_holds_hashes_not_names() {
        let mut s = Scanner::new("/mnt/someone/Inbox", None);
        s.baseline(&[entry("diagnosis letter.pdf", 10, 1)]);
        let saved = serde_json::to_string(&s.ledger()).unwrap();
        assert!(!saved.contains("diagnosis"), "{saved}");
        assert!(!saved.contains("someone"), "{saved}");
        let again = Scanner::new("/mnt/someone/Inbox", Some(&s.ledger()));
        assert!(!again.needs_baseline(), "the same folder resumes");
        assert!(Scanner::new("/other", Some(&s.ledger())).needs_baseline());
        assert_ne!(
            Scanner::new("/f", None).ledger().salt,
            Scanner::new("/f", None).ledger().salt,
            "every ledger has its own salt"
        );
    }

    #[test]
    fn a_ledger_past_its_cap_keeps_the_newest_files() {
        let mut s = Scanner::new("/f", None);
        let many: Vec<Entry> = (0..(LEDGER_CAP + 50))
            .map(|i| entry(&format!("f{i}.txt"), 1, i as i64))
            .collect();
        s.baseline(&many);
        let saved = s.ledger();
        assert_eq!(saved.entries.len(), LEDGER_CAP);
        let mut again = Scanner::new("/f", Some(&saved));
        let newest = [entry(
            &format!("f{}.txt", LEDGER_CAP + 49),
            1,
            (LEDGER_CAP + 49) as i64,
        )];
        let now = at("2026-11-13T09:00:00Z");
        assert!(again.observe(&newest, now, STABLE).is_empty());
        assert!(
            again
                .observe(&newest, now + SignedDuration::from_secs(10), STABLE)
                .is_empty(),
            "the newest file is remembered, not looked at as new"
        );
    }

    #[test]
    fn a_file_that_failed_is_tried_again_and_given_up_on_after_a_while() {
        let mut s = Scanner::new("/f", None);
        s.baseline(&[]);
        let e = [entry("a.pdf", 1, 1)];
        let t0 = at("2026-11-13T09:00:00Z");
        s.observe(&e, t0, STABLE);
        let mut now = t0;
        for round in 1..MAX_TRIES {
            now += SignedDuration::from_secs(3600);
            assert_eq!(s.observe(&e, now, STABLE), ["a.pdf"], "round {round}");
            assert!(!s.failed("a.pdf", now, STABLE, true));
        }
        now += SignedDuration::from_secs(3600);
        assert_eq!(s.observe(&e, now, STABLE), ["a.pdf"]);
        assert!(s.failed("a.pdf", now, STABLE, true), "given up on");
        now += SignedDuration::from_secs(3600);
        assert!(s.observe(&e, now, STABLE).is_empty());
    }

    #[test]
    fn a_failure_that_is_not_the_files_is_waited_out_for_as_long_as_it_lasts() {
        let mut s = Scanner::new("/f", None);
        s.baseline(&[]);
        let e = [entry("a.pdf", 1, 1)];
        let t0 = at("2026-11-13T09:00:00Z");
        s.observe(&e, t0, STABLE);
        let mut now = t0 + SignedDuration::from_secs(10);
        for _ in 0..(u32::from(MAX_TRIES) * 3) {
            assert_eq!(s.observe(&e, now, STABLE), ["a.pdf"]);
            assert!(!s.failed("a.pdf", now, STABLE, false), "never given up on");
            now += SignedDuration::from_secs(601);
        }
        assert_eq!(s.observe(&e, now, STABLE), ["a.pdf"], "still tried");
    }

    #[test]
    fn retries_back_off_by_doubling_up_to_ten_minutes() {
        let mut s = Scanner::new("/f", None);
        s.baseline(&[]);
        let e = [entry("a.pdf", 1, 1)];
        let t0 = at("2026-11-13T09:00:00Z");
        s.observe(&e, t0, STABLE);
        let mut now = t0 + SignedDuration::from_secs(10);
        for wait in [3, 6, 12, 24, 48, 96, 192, 384, 600, 600, 600] {
            assert_eq!(s.observe(&e, now, STABLE), ["a.pdf"]);
            s.failed("a.pdf", now, STABLE, false);
            let early = now + SignedDuration::from_secs(wait - 1);
            assert!(s.observe(&e, early, STABLE).is_empty(), "{wait}s: too soon");
            now += SignedDuration::from_secs(wait);
        }
    }

    #[test]
    fn a_changed_file_starts_again() {
        let mut s = Scanner::new("/f", None);
        s.baseline(&[]);
        let t0 = at("2026-11-13T09:00:00Z");
        s.observe(&[entry("a.pdf", 1, 1)], t0, STABLE);
        let now = t0 + SignedDuration::from_secs(10);
        s.observe(&[entry("a.pdf", 1, 1)], now, STABLE);
        s.failed("a.pdf", now, STABLE, false);
        s.observe(&[entry("a.pdf", 2, 2)], now, STABLE);
        let later = now + STABLE + SignedDuration::from_secs(1);
        assert_eq!(s.observe(&[entry("a.pdf", 2, 2)], later, STABLE), ["a.pdf"]);
    }

    #[test]
    fn only_a_file_that_is_gone_or_can_never_be_kept_is_settled() {
        use ErrorCode::*;
        assert_eq!(outcome(NotFound, false), Outcome::Settle);
        assert_eq!(outcome(Invalid, true), Outcome::Settle);
        assert_eq!(
            outcome(NotFound, true),
            Outcome::Retry { counts: true },
            "the store's own NotFound is not the file's absence"
        );
        for waited_out in [Unavailable, Internal, Busy] {
            assert_eq!(outcome(waited_out, true), Outcome::Retry { counts: false });
        }
    }

    #[test]
    fn old_mtime_new_file_is_kept() {
        let mut s = Scanner::new("/f", None);
        s.baseline(&[]);
        let t0 = at("2026-11-13T09:00:00Z");
        let photo = [entry("photo.jpg", 4_000_000, 1_262_304_000_000)];
        s.observe(&photo, t0, STABLE);
        let ready = s.observe(&photo, t0 + SignedDuration::from_secs(5), STABLE);
        assert_eq!(
            ready,
            ["photo.jpg"],
            "a sync tool keeps the photo's own date"
        );
    }

    #[test]
    fn a_file_that_vanishes_before_it_settles_is_forgotten() {
        let mut s = Scanner::new("/f", None);
        s.baseline(&[]);
        let t0 = at("2026-11-13T09:00:00Z");
        s.observe(&[entry("a.pdf", 1, 1)], t0, STABLE);
        assert!(s.has_pending());
        s.observe(&[], t0 + SignedDuration::from_secs(1), STABLE);
        assert!(!s.has_pending());
    }

    #[test]
    fn partial_and_hidden_names_skipped() {
        for name in [
            "a.pdf.part",
            "a.pdf.crdownload",
            "a.pdf.download",
            "a.pdf.tmp",
            "~a.pdf",
            "a.pdf~",
            ".a.pdf",
            ".~lock.a.pdf#",
            ".syncthing.a.pdf.tmp",
            "a.pdf.syncthing.x.pdf",
            ".goutputstream-ABC123",
            "~$a.docx",
            "readme",
            "a.exe",
            "A.PDF.PART",
        ] {
            assert!(!eligible(Path::new(&format!("/f/{name}"))), "{name}");
        }
        for name in ["a.pdf", "Photo.JPG", "note.md", "voice.m4a", "a.docx"] {
            assert!(eligible(Path::new(&format!("/f/{name}"))), "{name}");
        }
    }

    #[test]
    fn rejects_root_home_and_overlap() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let data = home.join("data");
        let inside = data.join("store");
        let outside = home.join("Documents");
        for d in [&data, &inside, &outside] {
            std::fs::create_dir_all(d).unwrap();
        }
        let check = |p: &Path| validate(&p.to_string_lossy(), &data, Some(&home));
        assert_eq!(
            check(&outside).unwrap(),
            std::fs::canonicalize(&outside).unwrap()
        );
        assert_eq!(check(Path::new("/")), Err(InboxError::TooBroad));
        assert_eq!(check(&home), Err(InboxError::TooBroad));
        assert_eq!(
            check(&data),
            Err(InboxError::Overlap),
            "the data folder itself"
        );
        assert_eq!(
            check(&inside),
            Err(InboxError::Overlap),
            "inside the data folder"
        );
        assert_eq!(
            validate(
                &home.to_string_lossy(),
                &home.join("data"),
                Some(&root.path().join("x"))
            ),
            Err(InboxError::Overlap),
            "a folder that contains the data folder"
        );
        assert_eq!(check(&home.join("nope")), Err(InboxError::Missing));
        assert_eq!(
            validate("relative/path", &data, Some(&home)),
            Err(InboxError::NotAbsolute)
        );
        let file = outside.join("f.txt");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(
            check(&file),
            Err(InboxError::Missing),
            "a file is not a folder"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_to_outside_not_kept() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), b"secret").unwrap();
        std::fs::create_dir(outside.path().join("dir")).unwrap();
        std::fs::write(outside.path().join("dir").join("inner.txt"), b"x").unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("real.txt"), b"real").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            folder.path().join("link.txt"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path().join("dir"), folder.path().join("linked-dir"))
            .unwrap();
        let listing = list(folder.path(), 100).unwrap();
        let names: Vec<_> = listing.entries.iter().map(|e| e.rel.as_str()).collect();
        assert_eq!(names, ["real.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_swapped_for_a_link_after_the_look_is_not_kept() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), b"secret").unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("a.txt"), b"fine").unwrap();
        let seen = list(folder.path(), 10).unwrap();
        assert_eq!(seen.entries.len(), 1);
        let real = std::fs::canonicalize(folder.path().join("a.txt")).unwrap();
        assert_eq!(confirm_inside(folder.path(), "a.txt"), Some(real));

        std::fs::remove_file(folder.path().join("a.txt")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            folder.path().join("a.txt"),
        )
        .unwrap();
        assert_eq!(confirm_inside(folder.path(), "a.txt"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_subfolder_swapped_for_a_link_after_the_look_is_not_followed() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("b.txt"), b"secret").unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("sub")).unwrap();
        std::fs::write(folder.path().join("sub").join("b.txt"), b"fine").unwrap();
        assert!(confirm_inside(folder.path(), "sub/b.txt").is_some());

        std::fs::remove_dir_all(folder.path().join("sub")).unwrap();
        std::os::unix::fs::symlink(outside.path(), folder.path().join("sub")).unwrap();
        assert_eq!(confirm_inside(folder.path(), "sub/b.txt"), None);
    }

    #[test]
    fn a_path_that_climbs_out_or_is_not_a_plain_file_is_refused() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("d")).unwrap();
        std::fs::write(folder.path().join("a.txt"), b"x").unwrap();
        assert_eq!(confirm_inside(folder.path(), "../a.txt"), None);
        assert_eq!(confirm_inside(folder.path(), "/etc/passwd"), None);
        assert_eq!(
            confirm_inside(folder.path(), "d"),
            None,
            "a folder is not a file"
        );
        assert_eq!(confirm_inside(folder.path(), "missing.txt"), None);
    }

    #[test]
    fn listing_skips_hidden_folders_empty_files_and_deep_files() {
        let folder = tempfile::tempdir().unwrap();
        let p = folder.path();
        std::fs::create_dir_all(p.join(".hidden")).unwrap();
        std::fs::write(p.join(".hidden").join("a.txt"), b"x").unwrap();
        std::fs::write(p.join("empty.txt"), b"").unwrap();
        let deep = p.join("1").join("2").join("3").join("4");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("ok.txt"), b"x").unwrap();
        std::fs::create_dir_all(deep.join("5")).unwrap();
        std::fs::write(deep.join("5").join("too-deep.txt"), b"x").unwrap();
        std::fs::write(p.join("top.txt"), b"x").unwrap();
        let mut names: Vec<_> = list(p, 100)
            .unwrap()
            .entries
            .into_iter()
            .map(|e| e.rel)
            .collect();
        names.sort();
        assert_eq!(names, ["1/2/3/4/ok.txt", "top.txt"]);
    }

    #[test]
    fn a_folder_over_the_cap_is_refused() {
        let folder = tempfile::tempdir().unwrap();
        for i in 0..6 {
            std::fs::write(folder.path().join(format!("{i}.txt")), b"x").unwrap();
        }
        assert!(too_many(folder.path(), 5));
        assert!(!too_many(folder.path(), 6));
        assert!(!too_many(folder.path(), 100));
    }
}
