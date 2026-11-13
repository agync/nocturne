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
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use nocturne_contract::Source;
use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

use super::state::OsState;

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

/// Whether the folder holds more files than the ledger can remember.
pub fn too_many(root: &Path, cap: usize) -> bool {
    list(root, cap).is_ok_and(|l| !l.complete && l.entries.len() >= cap)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub path: String,
    pub size: u64,
    pub mtime: i64,
}

/// The files already dealt with in a folder, kept in `os-state.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxLedger {
    pub folder: String,
    pub entries: Vec<LedgerEntry>,
}

#[derive(Debug, Clone)]
struct Pending {
    size: u64,
    mtime: i64,
    looked: Timestamp,
}

/// Decides which files are new and stable. Knows nothing about disks or clocks.
#[derive(Debug)]
pub struct Scanner {
    folder: String,
    ledger: BTreeMap<String, (u64, i64)>,
    pending: BTreeMap<String, Pending>,
    baselined: bool,
}

impl Scanner {
    /// A scanner for `folder`, resuming from `saved` when it is the same folder.
    pub fn new(folder: &str, saved: Option<&InboxLedger>) -> Scanner {
        let saved = saved.filter(|l| l.folder == folder);
        Scanner {
            folder: folder.to_string(),
            ledger: saved
                .map(|l| {
                    l.entries
                        .iter()
                        .map(|e| (e.path.clone(), (e.size, e.mtime)))
                        .collect()
                })
                .unwrap_or_default(),
            pending: BTreeMap::new(),
            baselined: saved.is_some(),
        }
    }

    /// True until the files that were there at the start have been put in the ledger.
    pub fn needs_baseline(&self) -> bool {
        !self.baselined
    }

    /// Remember what is already in the folder, without keeping any of it.
    pub fn baseline(&mut self, entries: &[Entry]) {
        for e in entries {
            self.ledger.insert(e.rel.clone(), (e.size, e.mtime));
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
            if self.ledger.contains_key(&e.rel) {
                continue;
            }
            match self.pending.get_mut(&e.rel) {
                Some(p) if p.size == e.size && p.mtime == e.mtime => {
                    if now.duration_since(p.looked) >= stable_after {
                        ready.push(e.rel.clone());
                    }
                }
                Some(p) => {
                    *p = Pending {
                        size: e.size,
                        mtime: e.mtime,
                        looked: now,
                    };
                }
                None => {
                    self.pending.insert(
                        e.rel.clone(),
                        Pending {
                            size: e.size,
                            mtime: e.mtime,
                            looked: now,
                        },
                    );
                }
            }
        }
        ready.sort();
        ready
    }

    /// A file has been dealt with (kept, or given up on): never look at it as new again.
    pub fn settle(&mut self, rel: &str) {
        if let Some(p) = self.pending.remove(rel) {
            self.ledger.insert(rel.to_string(), (p.size, p.mtime));
        }
    }

    /// Forget files that are no longer in the folder, so the ledger stays the size of the folder.
    pub fn prune(&mut self, entries: &[Entry]) {
        let present: BTreeSet<&str> = entries.iter().map(|e| e.rel.as_str()).collect();
        self.ledger.retain(|rel, _| present.contains(rel.as_str()));
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn ledger(&self) -> InboxLedger {
        InboxLedger {
            folder: self.folder.clone(),
            entries: self
                .ledger
                .iter()
                .take(LEDGER_CAP)
                .map(|(path, (size, mtime))| LedgerEntry {
                    path: path.clone(),
                    size: *size,
                    mtime: *mtime,
                })
                .collect(),
        }
    }
}

/// Watch `root` and keep what appears there. Replaces any watcher that is running.
pub fn start<R: Runtime>(app: &AppHandle<R>, root: PathBuf) {
    let Some(os) = app.try_state::<OsState>() else {
        return;
    };
    let timing = os.inbox_timing();
    let handle = app.clone();
    let task = tauri::async_runtime::spawn(async move { run(handle, root, timing).await });
    os.replace_inbox(Some(task));
}

/// Stop watching.
pub fn stop<R: Runtime>(app: &AppHandle<R>) {
    if let Some(os) = app.try_state::<OsState>() {
        os.replace_inbox(None);
    }
}

fn wake_on_changes(
    root: &Path,
) -> Option<(
    notify::RecommendedWatcher,
    tokio::sync::mpsc::UnboundedReceiver<()>,
)> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok() {
            let _ = tx.send(());
        }
    })
    .ok()?;
    watcher.watch(root, RecursiveMode::Recursive).ok()?;
    Some((watcher, rx))
}

async fn run<R: Runtime>(app: AppHandle<R>, root: PathBuf, timing: Timing) {
    let Some(os) = app.try_state::<OsState>() else {
        return;
    };
    let folder = root.to_string_lossy().into_owned();
    let mut scanner = Scanner::new(&folder, os.file().inbox.as_ref());
    let mut changes = wake_on_changes(&root);
    if changes.is_none() {
        tracing::info!("the watched folder will be looked at on a timer only");
    }
    let stable =
        SignedDuration::try_from(timing.stable_after).unwrap_or(SignedDuration::from_secs(3));
    let mut reported_missing = false;
    loop {
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
                if changes.is_none()
                    && let Some(w) = wake_on_changes(&root)
                {
                    changes = Some(w);
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
                    for batch in ready.chunks(BATCH) {
                        let paths: Vec<PathBuf> = batch.iter().map(|rel| root.join(rel)).collect();
                        if let Err(e) = crate::keep_paths(&app, paths, Source::WatchedFolder).await
                        {
                            tracing::warn!(
                                "files from the watched folder were not kept: {:?}",
                                e.code
                            );
                        }
                        for rel in batch {
                            scanner.settle(rel);
                        }
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
                    _ = tokio::time::sleep(wait) => {}
                    woke = rx.recv() => {
                        if woke.is_some() {
                            tokio::time::sleep(timing.debounce).await;
                            while rx.try_recv().is_ok() {}
                        } else {
                            changes = None;
                        }
                    }
                }
            }
            None => tokio::time::sleep(wait).await,
        }
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
