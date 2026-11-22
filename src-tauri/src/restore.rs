//! What a backup leaves on disk: the staged store, the marker for the next start, the copies put
//! aside and the small state the backup screen reads.

use std::io::{self, Read};
use std::path::{Path, PathBuf};

use nocturne_contract::{BackupSummary, LastBackup, PutAsideView, RestoredView};
use nocturne_lemonade::selection::Selection;
use serde::{Deserialize, Serialize};

use crate::fsutil;

/// The only marker version this build understands.
const VERSION: u32 = 1;
/// The largest reading setup a backup may carry.
const MAX_SELECTION: usize = 64 * 1024;
const RESTORE: &str = "restore";
const STAGE: &str = "stage";
const CHECK: &str = "check";
const MARKER: &str = "pending.json";
const PUT_ASIDE: &str = "put-aside";
const BACKUP: &str = "backup";
const JOB: &str = "job.json";
const LAST: &str = "last.json";
const RESTORED: &str = "restored.json";
const INTERRUPTED_BACKUP: &str = "interrupted-backup";
const INTERRUPTED_RESTORE: &str = "interrupted-restore";

/// Where a scheduled restore takes its data from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// A backup that was unpacked into `restore/stage`.
    #[default]
    Stage,
    /// A copy put aside by an earlier restore, by its folder name.
    PutAside(String),
}

/// What a copy put aside holds, written beside it as `info.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Aside {
    pub at: String,
    pub memories: u32,
    pub pages: u32,
    pub originals: u32,
    pub bytes: f64,
}

/// What a scheduled restore asks the next start to do.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Marker {
    version: u32,
    replace: bool,
    stamp: String,
    #[serde(default)]
    source: Source,
    #[serde(default)]
    aside: Option<Aside>,
    #[serde(default)]
    summary: Option<BackupSummary>,
    /// A phone's memory that moved here: it takes this computer's own settings and paired
    /// phones again as it goes in place.
    #[serde(default)]
    arrival: bool,
}

/// The partial file of a backup being written.
#[derive(Debug, Serialize, Deserialize)]
struct Job {
    partial: PathBuf,
}

/// What starting the app did with a scheduled restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// Nothing was asked for.
    NothingPending,
    /// The staged store is in place. `kept` is the copy of what was here, put aside.
    Restored { kept: Option<PathBuf> },
    /// Nothing was changed; the next start tries again.
    Failed(String),
}

/// The folder a restore keeps its work in.
pub fn restore_dir(data: &Path) -> PathBuf {
    data.join(RESTORE)
}

/// Where a checked backup waits: `<data>/restore/stage`.
pub fn stage_dir(data: &Path) -> PathBuf {
    data.join(RESTORE).join(STAGE)
}

/// Where a backup is checked: `<data>/restore/check`.
pub fn check_dir(data: &Path) -> PathBuf {
    data.join(RESTORE).join(CHECK)
}

/// The marker a scheduled restore leaves for the next start.
pub fn marker_path(data: &Path) -> PathBuf {
    data.join(RESTORE).join(MARKER)
}

fn backup_dir(data: &Path) -> PathBuf {
    data.join(BACKUP)
}

fn selection_path(data: &Path) -> PathBuf {
    crate::models::config_dir(data).join("selection.json")
}

/// Remove a staged backup nothing has claimed, and the `restore` folder when it is then empty.
pub fn clear_stage(data: &Path) {
    let restore = data.join(RESTORE);
    fsutil::shred_tree(&restore.join(STAGE));
    prune(&restore);
}

/// Remove whatever a check wrote, and the `restore` folder when it is then empty.
pub fn clear_check(data: &Path) {
    let restore = data.join(RESTORE);
    fsutil::shred_tree(&restore.join(CHECK));
    prune(&restore);
}

/// The note a phone's move leaves in its stage; it goes beside the backup state once the memory
/// is in place, and never before.
pub const ARRIVAL_NOTE: &str = "moved.json";

/// The prefix of a stage a phone's move let go of, renamed away to be shredded.
pub const LET_GO: &str = "let-go-";

/// Shred what a move let go of and a stop left behind.
fn clear_let_go(data: &Path) {
    let restore = data.join(RESTORE);
    if let Ok(entries) = std::fs::read_dir(&restore) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(LET_GO) {
                fsutil::shred_tree(&entry.path());
            }
        }
    }
    prune(&restore);
}

/// Remove both the unpacked backup and any check scratch.
pub fn clear_staging(data: &Path) {
    let restore = data.join(RESTORE);
    fsutil::shred_tree(&restore.join(STAGE));
    fsutil::shred_tree(&restore.join(CHECK));
    prune(&restore);
}

/// Ask the next start to put the chosen store in place. `stamp` names the copy that will be put
/// aside; `aside` is what that copy holds, and `summary` what a backup held.
pub fn schedule(
    data: &Path,
    replace: bool,
    stamp: &str,
    source: Source,
    aside: Option<Aside>,
    summary: Option<BackupSummary>,
) -> io::Result<()> {
    write_marker(data, replace, stamp, source, aside, summary, false)
}

/// Ask the next start to put a phone's memory, settled in the stage, in place. `stamp` names
/// the copy of what is here that will be put aside, and `aside` what it holds.
pub fn schedule_arrival(
    data: &Path,
    replace: bool,
    stamp: &str,
    aside: Option<Aside>,
) -> io::Result<()> {
    write_marker(data, replace, stamp, Source::Stage, aside, None, true)
}

fn write_marker(
    data: &Path,
    replace: bool,
    stamp: &str,
    source: Source,
    aside: Option<Aside>,
    summary: Option<BackupSummary>,
    arrival: bool,
) -> io::Result<()> {
    match &source {
        Source::Stage => {
            if !stage_dir(data).join("store").join("nocturne.db").is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "there is no checked backup to restore",
                ));
            }
        }
        Source::PutAside(id) => match put_aside_path(data, id) {
            Some(path) if path.join("store").is_dir() => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "that copy is not there",
                ));
            }
        },
    }
    let marker = Marker {
        version: VERSION,
        replace,
        stamp: stamp.to_string(),
        source,
        aside,
        summary,
        arrival,
    };
    let bytes = serde_json::to_vec(&marker).map_err(io::Error::other)?;
    fsutil::create_private_dir(&data.join(RESTORE))?;
    fsutil::write_private(&marker_path(data), &bytes)
}

/// A phone's memory waits for the next start to take its place.
pub fn arrival_waits(data: &Path) -> bool {
    read_marker(&marker_path(data)).is_ok_and(|m| m.arrival)
}

/// Clean up a backup or a restore that stopped unexpectedly. Runs before the store is opened.
pub fn recover(data: &Path) {
    let job = backup_dir(data).join(JOB);
    if job.exists() {
        if let Ok(bytes) = std::fs::read(&job)
            && let Ok(parsed) = serde_json::from_slice::<Job>(&bytes)
            && parsed
                .partial
                .extension()
                .is_some_and(|ext| ext == "partial")
        {
            let _ = std::fs::remove_file(&parsed.partial);
        }
        let _ = std::fs::remove_file(&job);
        write_flag(data, INTERRUPTED_BACKUP);
    }
}

/// Do what a scheduled restore asked for, before the store is opened. Safe to run again after a
/// crash at any step: the marker is removed only when the swap is complete, and what is here is
/// put aside rather than deleted.
pub fn apply_pending(data: &Path) -> ApplyOutcome {
    recover(data);
    // A check that stopped part way left a plain copy of a backup's database; nothing checks now.
    clear_check(data);
    clear_let_go(data);
    let marker_path = marker_path(data);
    if !marker_path.exists() {
        // Nobody asked for a restore: a stage left behind is one that stopped part way.
        let stage = stage_dir(data);
        if stage.exists() {
            write_flag(data, INTERRUPTED_RESTORE);
            fsutil::shred_tree(&stage);
        }
        prune(&data.join(RESTORE));
        return ApplyOutcome::NothingPending;
    }
    let marker = match read_marker(&marker_path) {
        Ok(marker) => marker,
        Err(reason) => return ApplyOutcome::Failed(reason),
    };

    let source = match &marker.source {
        Source::Stage => Some(stage_dir(data)),
        Source::PutAside(id) => put_aside_path(data, id),
    };
    let Some(source) = source else {
        return ApplyOutcome::Failed("the copy to bring back is not there".into());
    };
    let source_store = source.join("store");
    let store = data.join("store");
    if !source_store.is_dir() {
        // A source without its store was moved into place by a swap that stopped before the
        // marker went: finish the steps after the move. A copy to bring back that lost its store
        // while nothing is in place is damaged.
        if !source.exists() || store.is_dir() || matches!(marker.source, Source::Stage) {
            return complete(data, &source, &marker, &marker_path, None);
        }
        return ApplyOutcome::Failed("the copy to bring back is not there".into());
    }

    // What changed on this computer since the memory was settled (a phone removed or added,
    // a new address) holds: its own rows are taken again from the store still in use.
    // A moved memory that does not open here changes nothing: it is let go, and its note says
    // so, for the window and for the phone, which keeps its memory.
    if marker.arrival
        && store.is_dir()
        && let Err(e) = retake_computer(&source_store, &store)
    {
        abandon_arrival(data, &source, &marker_path);
        return ApplyOutcome::Failed(format!("the memory that moved here did not open ({e})"));
    }

    let mut kept = None;
    if store.exists() {
        if !marker.replace && store_is_empty_and_fresh(&store) {
            if let Err(e) = std::fs::remove_dir_all(&store) {
                return ApplyOutcome::Failed(format!(
                    "the empty store could not be removed ({:?})",
                    e.kind()
                ));
            }
        } else {
            match keep_aside(data, &marker.stamp, marker.aside.as_ref()) {
                Ok(folder) => kept = Some(folder),
                Err(e) => {
                    return ApplyOutcome::Failed(format!(
                        "what is here could not be put aside ({:?})",
                        e.kind()
                    ));
                }
            }
        }
    }

    if let Err(e) = std::fs::rename(&source_store, &store) {
        if let Some(folder) = &kept {
            undo_aside(data, folder);
        }
        return ApplyOutcome::Failed(format!(
            "the backup could not be put in place ({:?})",
            e.kind()
        ));
    }
    complete(data, &source, &marker, &marker_path, kept)
}

/// Let go of a moved memory that did not open: its note goes beside the backup state marked as
/// failed, and the stage and the marker go.
fn abandon_arrival(data: &Path, source: &Path, marker_path: &Path) {
    let note = std::fs::read(source.join(ARRIVAL_NOTE))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
    if let Some(mut note) = note {
        note["failed"] = serde_json::Value::Bool(true);
        let written = fsutil::create_private_dir(&backup_dir(data)).and_then(|()| {
            let bytes = serde_json::to_vec(&note).map_err(io::Error::other)?;
            fsutil::write_private(&backup_dir(data).join(ARRIVAL_NOTE), &bytes)
        });
        if let Err(e) = written {
            tracing::warn!("could not note that a moved memory did not open: {e}");
        }
    }
    let _ = std::fs::remove_file(marker_path);
    fsutil::shred_tree(source);
    prune(&data.join(RESTORE));
}

fn retake_computer(arriving: &Path, here: &Path) -> nocturne_engine::Result<()> {
    let clock = std::sync::Arc::new(nocturne_engine::SystemClock);
    nocturne_engine::Engine::open(arriving, clock)?.retake_computer(here)
}

/// The steps after the store is in place: the reading setup, the spent source folder and the
/// marker. Safe to run again when a crash stopped them part way.
fn complete(
    data: &Path,
    source: &Path,
    marker: &Marker,
    marker_path: &Path,
    kept: Option<PathBuf>,
) -> ApplyOutcome {
    let source_selection = source.join("engine").join("selection.json");
    if let Some(bytes) = sanitised_selection_file(&source_selection)
        && let Err(e) = write_selection(data, &bytes)
    {
        return ApplyOutcome::Failed(format!(
            "the reading setup could not be kept ({:?})",
            e.kind()
        ));
    }

    // A phone's memory in place: its note says so from now on.
    let note = source.join(ARRIVAL_NOTE);
    if marker.arrival && note.is_file() {
        let placed = fsutil::create_private_dir(&backup_dir(data))
            .and_then(|()| std::fs::copy(&note, backup_dir(data).join(ARRIVAL_NOTE)).map(drop));
        if let Err(e) = placed {
            return ApplyOutcome::Failed(format!(
                "the note of the memory that moved could not be kept ({:?})",
                e.kind()
            ));
        }
    }

    // The stage is spent, and so is a copy brought back: its store is the live one now, and what
    // was here is put aside in its place.
    if source.exists()
        && let Err(e) = std::fs::remove_dir_all(source)
    {
        return ApplyOutcome::Failed(format!(
            "the files that were moved into place could not be tidied ({:?})",
            e.kind()
        ));
    }
    finish(data, marker, marker_path, kept.as_deref());
    ApplyOutcome::Restored { kept }
}

/// Remove a marker, write what the restore screen shows and tidy the folders it left.
fn finish(data: &Path, marker: &Marker, marker_path: &Path, kept: Option<&Path>) {
    if matches!(marker.source, Source::Stage)
        && let Some(summary) = &marker.summary
    {
        let put_aside_at = match kept {
            Some(folder) => folder.file_name().map(|n| n.to_string_lossy().into_owned()),
            None => marker.aside.as_ref().map(|a| a.at.clone()),
        };
        let view = RestoredView {
            made_at: summary.made_at.clone(),
            memories: summary.memories,
            pages: summary.pages,
            originals: summary.originals,
            online_used: summary.online_used,
            away_used: summary.away_used,
            put_aside_at,
        };
        let _ = write_restored(data, &view);
    }
    let _ = std::fs::remove_file(marker_path);
    prune(&data.join(RESTORE));
}

/// Overwrite and remove every staged file, every copy put aside and the backup state: nothing of
/// any of them survives.
pub fn remove_all(data: &Path) {
    for sub in [RESTORE, PUT_ASIDE, BACKUP] {
        let path = data.join(sub);
        if path.exists() {
            fsutil::shred_tree(&path);
        }
    }
}

/// Remember the partial file of the backup being written, so a crash can be cleaned up.
pub fn write_job(data: &Path, partial: &Path) -> io::Result<()> {
    fsutil::create_private_dir(&backup_dir(data))?;
    let bytes = serde_json::to_vec(&Job {
        partial: partial.to_path_buf(),
    })
    .map_err(io::Error::other)?;
    fsutil::write_private(&backup_dir(data).join(JOB), &bytes)
}

pub fn clear_job(data: &Path) {
    let _ = std::fs::remove_file(backup_dir(data).join(JOB));
}

/// The last finished backup, if one was recorded.
pub fn last_backup(data: &Path) -> Option<LastBackup> {
    let bytes = std::fs::read(backup_dir(data).join(LAST)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn write_last(data: &Path, at: &str, bytes: f64) -> io::Result<()> {
    fsutil::create_private_dir(&backup_dir(data))?;
    let record = LastBackup {
        at: at.to_string(),
        bytes,
    };
    let bytes = serde_json::to_vec(&record).map_err(io::Error::other)?;
    fsutil::write_private(&backup_dir(data).join(LAST), &bytes)
}

pub fn restored(data: &Path) -> Option<RestoredView> {
    let bytes = std::fs::read(backup_dir(data).join(RESTORED)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_restored(data: &Path, view: &RestoredView) -> io::Result<()> {
    fsutil::create_private_dir(&backup_dir(data))?;
    let bytes = serde_json::to_vec(view).map_err(io::Error::other)?;
    fsutil::write_private(&backup_dir(data).join(RESTORED), &bytes)
}

pub fn finish_restore(data: &Path) {
    let _ = std::fs::remove_file(backup_dir(data).join(RESTORED));
}

pub fn interrupted_backup(data: &Path) -> bool {
    backup_dir(data).join(INTERRUPTED_BACKUP).exists()
}

pub fn interrupted_restore(data: &Path) -> bool {
    backup_dir(data).join(INTERRUPTED_RESTORE).exists()
}

pub fn dismiss_notices(data: &Path) {
    clear_interrupted_backup(data);
    clear_interrupted_restore(data);
}

pub fn clear_interrupted_backup(data: &Path) {
    let _ = std::fs::remove_file(backup_dir(data).join(INTERRUPTED_BACKUP));
}

pub fn clear_interrupted_restore(data: &Path) {
    let _ = std::fs::remove_file(backup_dir(data).join(INTERRUPTED_RESTORE));
}

/// Every copy put aside, newest first. A copy whose `info.json` cannot be read is listed with
/// zero counts, so the person can still delete it.
pub fn put_aside(data: &Path) -> Vec<PutAsideView> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(data.join(PUT_ASIDE)) {
        for entry in entries.flatten() {
            let id = entry.file_name().to_string_lossy().into_owned();
            if !valid_id(&id) || !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let info = read_aside(&entry.path()).unwrap_or_default();
            out.push(PutAsideView {
                id,
                at: info.at,
                memories: info.memories,
                pages: info.pages,
                originals: info.originals,
                bytes: info.bytes,
            });
        }
    }
    out.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
    out
}

/// When the newest copy put aside was made.
pub fn newest_put_aside(data: &Path) -> Option<String> {
    put_aside(data)
        .into_iter()
        .find(|p| !p.at.is_empty())
        .map(|p| p.at)
}

/// Delete one copy put aside, after validating its id.
pub fn delete_put_aside(data: &Path, id: &str) -> io::Result<()> {
    let path = put_aside_path(data, id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "that is not a copy's name"))?;
    if !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "that copy is not there",
        ));
    }
    fsutil::shred_tree(&path);
    Ok(())
}

/// A copy's folder, when its id is one this app wrote.
pub fn put_aside_path(data: &Path, id: &str) -> Option<PathBuf> {
    valid_id(id).then(|| data.join(PUT_ASIDE).join(id))
}

/// `YYYY-MM-DD HHMMSS`, or that with ` (n)` when an earlier one took the name.
pub fn valid_id(id: &str) -> bool {
    let Some((date, rest)) = id.split_once(' ') else {
        return false;
    };
    if date.len() != 10
        || !date.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        })
    {
        return false;
    }
    let (time, suffix) = match rest.split_once(' ') {
        Some((time, suffix)) => (time, Some(suffix)),
        None => (rest, None),
    };
    if time.len() != 6 || !time.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    match suffix {
        None => true,
        Some(suffix) => suffix
            .strip_prefix('(')
            .and_then(|r| r.strip_suffix(')'))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
    }
}

fn read_marker(path: &Path) -> Result<Marker, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("the restore marker could not be read ({:?})", e.kind()))?;
    let marker: Marker = serde_json::from_slice(&bytes)
        .map_err(|_| "the restore marker is not valid".to_string())?;
    if marker.version != VERSION {
        return Err(format!("the restore marker is version {}", marker.version));
    }
    Ok(marker)
}

fn read_aside(folder: &Path) -> Option<Aside> {
    let bytes = std::fs::read(folder.join("info.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// A backup's reading setup, made safe for this machine. `None` when the file does not parse,
/// names a model this app would not run, or is too large to be one. A good one loses the download
/// state and the models a change left behind: those belong to the machine that made the backup.
pub fn sanitise_selection(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() > MAX_SELECTION {
        return None;
    }
    let mut selection: Selection = serde_json::from_slice(bytes).ok()?;
    if !selection.custom.values().all(|name| model_name_ok(name)) {
        return None;
    }
    selection.pending = None;
    selection.old = None;
    serde_json::to_vec(&selection).ok()
}

/// Read a reading setup, capped, and make it safe. `None` for a missing or untrustworthy file.
pub fn sanitised_selection_file(path: &Path) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut file = std::fs::File::open(path).ok()?;
    file.by_ref()
        .take((MAX_SELECTION + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    sanitise_selection(&bytes)
}

fn model_name_ok(name: &str) -> bool {
    !name.is_empty()
        && !name.contains("..")
        && name.chars().count() <= 200
        && name.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '+' | '@' | '-')
        })
}

/// A store with nothing of the person's in it, made by this app and never used: only its database,
/// the database's log files and an empty originals folder.
fn store_is_empty_and_fresh(store: &Path) -> bool {
    if nocturne_engine::store_has_data(store).unwrap_or(true) {
        return false;
    }
    match std::fs::read_dir(store) {
        Ok(entries) => entries.flatten().all(|entry| {
            let name = entry.file_name();
            match name.to_string_lossy().as_ref() {
                "nocturne.db" | "nocturne.db-wal" | "nocturne.db-shm" | "originals" => true,
                // Made on every start, so it is part of a fresh install while nothing is in it.
                "audio" => std::fs::read_dir(entry.path()).is_ok_and(|mut e| e.next().is_none()),
                _ => false,
            }
        }),
        Err(_) => false,
    }
}

/// Move the store that is about to be replaced into its own folder, beside a copy of the reading
/// setup and what it held, without ever writing over an earlier copy.
fn keep_aside(data: &Path, stamp: &str, aside: Option<&Aside>) -> io::Result<PathBuf> {
    let root = data.join(PUT_ASIDE);
    fsutil::create_private_dir(&root)?;
    let folder = free_name(&root.join(safe_stamp(stamp)));
    fsutil::create_private_dir(&folder)?;
    // Everything that can fail comes before the store moves, so a failure here never leaves the
    // person's memories somewhere the app is not looking.
    let prepared = (|| {
        let selection = selection_path(data);
        if selection.is_file() {
            let engine = folder.join("engine");
            fsutil::create_private_dir(&engine)?;
            std::fs::copy(&selection, engine.join("selection.json"))?;
        }
        if let Some(aside) = aside {
            let bytes = serde_json::to_vec(aside).map_err(io::Error::other)?;
            fsutil::write_private(&folder.join("info.json"), &bytes)?;
        }
        std::fs::rename(data.join("store"), folder.join("store"))
    })();
    if let Err(e) = prepared {
        // Only what this call made and put nothing of the person's in.
        let _ = std::fs::remove_dir_all(&folder);
        return Err(e);
    }
    Ok(folder)
}

/// Move a store that was put aside back to where the app looks for it.
fn undo_aside(data: &Path, folder: &Path) {
    if std::fs::rename(folder.join("store"), data.join("store")).is_ok() {
        let _ = std::fs::remove_dir_all(folder);
    }
}

fn write_selection(data: &Path, bytes: &[u8]) -> io::Result<()> {
    fsutil::create_private_dir(&crate::models::config_dir(data))?;
    fsutil::write_private(&selection_path(data), bytes)
}

fn write_flag(data: &Path, name: &str) {
    let dir = backup_dir(data);
    let _ = fsutil::create_private_dir(&dir);
    let _ = fsutil::write_private(&dir.join(name), b"1");
}

fn free_name(want: &Path) -> PathBuf {
    if !want.exists() {
        return want.to_path_buf();
    }
    let parent = want.parent().unwrap_or(Path::new("."));
    let name = want
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut n = 2u32;
    loop {
        let candidate = parent.join(format!("{name} ({n})"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// A folder name that is legal everywhere, made only of the characters a stamp may carry.
fn safe_stamp(stamp: &str) -> String {
    let cleaned: String = stamp
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.trim().is_empty() {
        "restore".to_string()
    } else {
        cleaned
    }
}

fn prune(dir: &Path) {
    if std::fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_none()) {
        let _ = std::fs::remove_dir(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocturne_contract::{ComponentRole, Tier};
    use nocturne_lemonade::selection::{OldModel, Pending};

    const STAMP: &str = "2026-11-15 120000";

    fn selection() -> Selection {
        let mut selection = Selection::preset(Tier::Standard);
        selection
            .custom
            .insert(ComponentRole::Mind, "a-model.gguf".into());
        selection.pending = Some(Pending {
            role: ComponentRole::Mind,
            model: "pending.gguf".into(),
            from: "current.gguf".into(),
            paused: false,
            failed: false,
        });
        selection.old = Some(OldModel {
            role: ComponentRole::Mind,
            model: "old.gguf".into(),
        });
        selection
    }

    fn staged(data: &Path) {
        let store = stage_dir(data).join("store");
        std::fs::create_dir_all(store.join("originals")).unwrap();
        std::fs::write(store.join("nocturne.db"), b"NEWDB").unwrap();
        std::fs::write(store.join("originals/new"), b"NEWORIGINAL").unwrap();
        let engine = stage_dir(data).join("engine");
        std::fs::create_dir_all(&engine).unwrap();
        std::fs::write(
            engine.join("selection.json"),
            serde_json::to_vec(&selection()).unwrap(),
        )
        .unwrap();
    }

    fn live(data: &Path) {
        let store = data.join("store");
        std::fs::create_dir_all(store.join("originals")).unwrap();
        std::fs::write(store.join("nocturne.db"), b"OLDDB").unwrap();
        std::fs::write(store.join("originals/old"), b"OLDORIGINAL").unwrap();
        std::fs::create_dir_all(crate::models::config_dir(data)).unwrap();
        std::fs::write(selection_path(data), b"{\"old\":true}").unwrap();
    }

    fn summary() -> BackupSummary {
        BackupSummary {
            made_at: "2026-11-14T06:00:00.000Z".into(),
            app_version: "0.2.2".into(),
            store_version: 10,
            memories: 2,
            items: 3,
            pages: 1,
            originals: 1,
            online_used: true,
            away_used: false,
            bytes: 100.0,
        }
    }

    fn aside() -> Aside {
        Aside {
            at: STAMP.into(),
            memories: 1,
            pages: 1,
            originals: 1,
            bytes: 50.0,
        }
    }

    fn read(path: impl AsRef<Path>) -> Vec<u8> {
        std::fs::read(path).unwrap()
    }

    #[test]
    fn a_staged_backup_replaces_an_empty_install() {
        let dir = tempfile::tempdir().unwrap();
        staged(dir.path());
        schedule(
            dir.path(),
            false,
            STAMP,
            Source::Stage,
            None,
            Some(summary()),
        )
        .unwrap();
        assert_eq!(
            apply_pending(dir.path()),
            ApplyOutcome::Restored { kept: None }
        );
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"NEWDB");
        assert_eq!(read(dir.path().join("store/originals/new")), b"NEWORIGINAL");
        let installed: Selection =
            serde_json::from_slice(&read(selection_path(dir.path()))).unwrap();
        assert_eq!(installed.tier, Tier::Standard);
        assert!(installed.pending.is_none() && installed.old.is_none());
        assert_eq!(
            installed
                .custom
                .get(&ComponentRole::Mind)
                .map(String::as_str),
            Some("a-model.gguf")
        );
        assert!(!marker_path(dir.path()).exists());
        assert!(!stage_dir(dir.path()).exists());
        assert!(!dir.path().join(RESTORE).exists());
        assert!(!dir.path().join(PUT_ASIDE).exists());

        let restored = restored(dir.path()).unwrap();
        assert_eq!(restored.made_at, summary().made_at);
        assert!(restored.online_used && !restored.away_used);
        assert_eq!(restored.put_aside_at, None);
    }

    #[test]
    fn an_install_with_only_its_empty_folders_is_replaced_without_putting_it_aside() {
        let dir = tempfile::tempdir().unwrap();
        staged(dir.path());
        let store = dir.path().join("store");
        std::fs::create_dir_all(store.join("originals")).unwrap();
        std::fs::create_dir_all(store.join("audio")).unwrap();
        schedule(
            dir.path(),
            false,
            STAMP,
            Source::Stage,
            None,
            Some(summary()),
        )
        .unwrap();
        assert_eq!(
            apply_pending(dir.path()),
            ApplyOutcome::Restored { kept: None }
        );
        assert_eq!(read(store.join("nocturne.db")), b"NEWDB");
        assert!(!dir.path().join(PUT_ASIDE).exists());
    }

    #[test]
    fn an_audio_file_keeps_an_install_from_counting_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("store");
        std::fs::create_dir_all(store.join("audio")).unwrap();
        std::fs::write(store.join("audio/page.wav"), b"x").unwrap();
        assert!(!store_is_empty_and_fresh(&store));
    }

    #[cfg(unix)]
    #[test]
    fn a_failure_while_putting_aside_leaves_the_store_where_it_was() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        let selection = selection_path(dir.path());
        std::fs::set_permissions(&selection, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&selection).is_ok() {
            return;
        }
        let result = keep_aside(dir.path(), STAMP, None);
        std::fs::set_permissions(&selection, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(result.is_err());
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"OLDDB");
        assert_eq!(read(dir.path().join("store/originals/old")), b"OLDORIGINAL");
        let left = std::fs::read_dir(dir.path().join(PUT_ASIDE))
            .map(|d| d.count())
            .unwrap_or(0);
        assert_eq!(left, 0);
    }

    #[test]
    fn what_is_here_is_put_aside_with_its_info_and_reading_setup() {
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        staged(dir.path());
        schedule(
            dir.path(),
            true,
            STAMP,
            Source::Stage,
            Some(aside()),
            Some(summary()),
        )
        .unwrap();
        let kept = match apply_pending(dir.path()) {
            ApplyOutcome::Restored { kept: Some(p) } => p,
            other => panic!("{other:?}"),
        };
        assert_eq!(kept, dir.path().join(PUT_ASIDE).join(STAMP));
        assert_eq!(read(kept.join("store/nocturne.db")), b"OLDDB");
        assert_eq!(read(kept.join("store/originals/old")), b"OLDORIGINAL");
        assert_eq!(read(kept.join("engine/selection.json")), b"{\"old\":true}");
        assert!(!read(kept.join("info.json")).is_empty());
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"NEWDB");
        assert!(!marker_path(dir.path()).exists());

        let listed = put_aside(dir.path());
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, STAMP);
        assert_eq!((listed[0].memories, listed[0].pages), (1, 1));
        assert_eq!(
            restored(dir.path()).unwrap().put_aside_at,
            Some(STAMP.into())
        );
    }

    #[test]
    fn data_here_that_a_schedule_did_not_expect_is_put_aside_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        staged(dir.path());
        schedule(
            dir.path(),
            false,
            STAMP,
            Source::Stage,
            None,
            Some(summary()),
        )
        .unwrap();
        let kept = match apply_pending(dir.path()) {
            ApplyOutcome::Restored { kept: Some(p) } => p,
            other => panic!("{other:?}"),
        };
        assert_eq!(read(kept.join("store/nocturne.db")), b"OLDDB");
        assert_eq!(read(kept.join("store/originals/old")), b"OLDORIGINAL");
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"NEWDB");
        assert!(!marker_path(dir.path()).exists());
    }

    #[cfg(unix)]
    #[test]
    fn clearing_a_stage_or_a_check_overwrites_the_plaintext_first() {
        let dir = tempfile::tempdir().unwrap();
        for (folder, clear) in [
            (stage_dir(dir.path()), clear_stage as fn(&Path)),
            (check_dir(dir.path()), clear_check as fn(&Path)),
        ] {
            std::fs::create_dir_all(&folder).unwrap();
            let plain = folder.join("plain.txt");
            std::fs::write(&plain, b"a secret").unwrap();
            let spy = dir.path().join(format!(
                "spy-{}",
                folder.file_name().unwrap().to_string_lossy()
            ));
            std::fs::hard_link(&plain, &spy).unwrap();
            clear(dir.path());
            assert!(!folder.exists());
            let bytes = std::fs::read(&spy).unwrap();
            assert_eq!(bytes.len(), 8);
            assert!(bytes.iter().all(|&b| b == 0), "{folder:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_stale_stage_is_overwritten_before_it_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        staged(dir.path());
        let plain = stage_dir(dir.path()).join("plain.txt");
        std::fs::write(&plain, b"a secret").unwrap();
        let spy = dir.path().join("stale-spy");
        std::fs::hard_link(&plain, &spy).unwrap();
        assert_eq!(apply_pending(dir.path()), ApplyOutcome::NothingPending);
        assert!(!stage_dir(dir.path()).exists());
        let bytes = std::fs::read(&spy).unwrap();
        assert!(bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn a_stage_with_no_marker_is_dropped_and_a_flag_is_left() {
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        staged(dir.path());
        assert_eq!(apply_pending(dir.path()), ApplyOutcome::NothingPending);
        assert!(!stage_dir(dir.path()).exists());
        assert!(interrupted_restore(dir.path()));
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"OLDDB");
    }

    #[test]
    fn a_moves_note_counts_only_once_its_memory_is_in_place() {
        // Stopped before the marker: the stage and its note go, and nothing says it arrived.
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        std::fs::create_dir_all(stage_dir(dir.path()).join("store")).unwrap();
        std::fs::write(stage_dir(dir.path()).join("store/nocturne.db"), b"PHONE").unwrap();
        std::fs::write(stage_dir(dir.path()).join(ARRIVAL_NOTE), b"{}").unwrap();
        assert_eq!(apply_pending(dir.path()), ApplyOutcome::NothingPending);
        assert!(!backup_dir(dir.path()).join(ARRIVAL_NOTE).exists());

        // With its marker the store goes in place and the note with it.
        std::fs::create_dir_all(stage_dir(dir.path()).join("store")).unwrap();
        std::fs::write(stage_dir(dir.path()).join("store/nocturne.db"), b"PHONE").unwrap();
        std::fs::write(stage_dir(dir.path()).join(ARRIVAL_NOTE), b"{}").unwrap();
        let marker = Marker {
            version: VERSION,
            replace: true,
            stamp: STAMP.into(),
            source: Source::Stage,
            aside: None,
            summary: None,
            arrival: true,
        };
        std::fs::write(
            marker_path(dir.path()),
            serde_json::to_vec(&marker).unwrap(),
        )
        .unwrap();
        // The arriving store here is not a real one, so it does not open: nothing changes, the
        // stage goes and the note says it failed.
        assert!(matches!(apply_pending(dir.path()), ApplyOutcome::Failed(_)));
        let note: serde_json::Value = serde_json::from_slice(
            &std::fs::read(backup_dir(dir.path()).join(ARRIVAL_NOTE)).unwrap(),
        )
        .unwrap();
        assert_eq!(note["failed"], true);
        assert!(!marker_path(dir.path()).exists());
        assert!(!stage_dir(dir.path()).exists());
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"OLDDB");
    }

    #[test]
    fn what_a_move_let_go_of_is_gone_at_the_next_start() {
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        let gone = dir.path().join(RESTORE).join(format!("{LET_GO}abc"));
        std::fs::create_dir_all(gone.join("store")).unwrap();
        std::fs::write(gone.join("store/nocturne.db"), b"PHONE").unwrap();
        assert_eq!(apply_pending(dir.path()), ApplyOutcome::NothingPending);
        assert!(!gone.exists());
        assert!(!dir.path().join(RESTORE).exists());
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"OLDDB");
    }

    #[test]
    fn a_marker_with_no_stage_is_finished_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        std::fs::create_dir_all(dir.path().join(RESTORE)).unwrap();
        let marker = Marker {
            version: VERSION,
            replace: true,
            stamp: STAMP.into(),
            source: Source::Stage,
            aside: None,
            summary: None,
            arrival: false,
        };
        std::fs::write(
            marker_path(dir.path()),
            serde_json::to_vec(&marker).unwrap(),
        )
        .unwrap();
        assert_eq!(
            apply_pending(dir.path()),
            ApplyOutcome::Restored { kept: None }
        );
        assert!(!marker_path(dir.path()).exists());
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"OLDDB");
    }

    #[test]
    fn an_unknown_marker_version_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        staged(dir.path());
        std::fs::create_dir_all(dir.path().join(RESTORE)).unwrap();
        let marker = Marker {
            version: VERSION + 1,
            replace: true,
            stamp: STAMP.into(),
            source: Source::Stage,
            aside: None,
            summary: None,
            arrival: false,
        };
        std::fs::write(
            marker_path(dir.path()),
            serde_json::to_vec(&marker).unwrap(),
        )
        .unwrap();
        assert!(matches!(apply_pending(dir.path()), ApplyOutcome::Failed(_)));
        assert!(marker_path(dir.path()).exists());
        assert!(stage_dir(dir.path()).join("store/nocturne.db").exists());
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"OLDDB");
    }

    #[test]
    fn a_crash_after_what_was_here_moved_aside_is_finished() {
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        staged(dir.path());
        let kept = dir.path().join(PUT_ASIDE).join(STAMP);
        std::fs::create_dir_all(&kept).unwrap();
        std::fs::rename(dir.path().join("store"), kept.join("store")).unwrap();
        std::fs::create_dir_all(kept.join("engine")).unwrap();
        std::fs::copy(
            selection_path(dir.path()),
            kept.join("engine/selection.json"),
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join(RESTORE)).unwrap();
        let marker = Marker {
            version: VERSION,
            replace: true,
            stamp: STAMP.into(),
            source: Source::Stage,
            aside: Some(aside()),
            summary: Some(summary()),
            arrival: false,
        };
        std::fs::write(
            marker_path(dir.path()),
            serde_json::to_vec(&marker).unwrap(),
        )
        .unwrap();

        assert_eq!(
            apply_pending(dir.path()),
            ApplyOutcome::Restored { kept: None }
        );
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"NEWDB");
        assert_eq!(read(kept.join("store/nocturne.db")), b"OLDDB");
        assert!(!marker_path(dir.path()).exists());
        assert_eq!(
            restored(dir.path()).unwrap().put_aside_at,
            Some(STAMP.into())
        );
    }

    #[test]
    fn a_second_copy_with_the_same_stamp_does_not_overwrite_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join(PUT_ASIDE).join(STAMP).join("store");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::write(first.join("nocturne.db"), b"FIRST").unwrap();
        live(dir.path());
        staged(dir.path());
        schedule(dir.path(), true, STAMP, Source::Stage, None, None).unwrap();
        assert!(matches!(
            apply_pending(dir.path()),
            ApplyOutcome::Restored { kept: Some(_) }
        ));
        assert_eq!(read(first.join("nocturne.db")), b"FIRST");
        let second = dir
            .path()
            .join(PUT_ASIDE)
            .join(format!("{STAMP} (2)"))
            .join("store");
        assert_eq!(read(second.join("nocturne.db")), b"OLDDB");
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"NEWDB");
    }

    #[test]
    fn bringing_back_a_copy_put_aside_puts_what_is_here_aside_too() {
        let dir = tempfile::tempdir().unwrap();
        let older = dir.path().join(PUT_ASIDE).join("2026-11-14 090000");
        std::fs::create_dir_all(older.join("store")).unwrap();
        std::fs::write(older.join("store/nocturne.db"), b"OLDER").unwrap();
        std::fs::write(
            older.join("info.json"),
            serde_json::to_vec(&Aside {
                at: "2026-11-14 090000".into(),
                ..Aside::default()
            })
            .unwrap(),
        )
        .unwrap();
        live(dir.path());
        schedule(
            dir.path(),
            true,
            STAMP,
            Source::PutAside("2026-11-14 090000".into()),
            Some(aside()),
            None,
        )
        .unwrap();
        match apply_pending(dir.path()) {
            ApplyOutcome::Restored { kept: Some(p) } => {
                assert_eq!(read(p.join("store/nocturne.db")), b"OLDDB");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"OLDER");
        assert!(
            !older.exists(),
            "the copy is the live store now, not a second one"
        );
        assert!(restored(dir.path()).is_none(), "not a backup restore");
    }

    #[test]
    fn a_missing_copy_to_bring_back_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let copy = dir.path().join(PUT_ASIDE).join("2026-11-14 090000");
        std::fs::create_dir_all(copy.join("store")).unwrap();
        std::fs::write(copy.join("store/nocturne.db"), b"OLDER").unwrap();
        schedule(
            dir.path(),
            true,
            STAMP,
            Source::PutAside("2026-11-14 090000".into()),
            Some(aside()),
            None,
        )
        .unwrap();
        std::fs::remove_dir_all(copy.join("store")).unwrap();
        assert!(matches!(apply_pending(dir.path()), ApplyOutcome::Failed(_)));
        assert!(marker_path(dir.path()).exists());
        assert!(copy.exists());
        assert!(!dir.path().join("store").exists());
    }

    #[test]
    fn a_crash_after_a_copy_moved_into_place_is_finished() {
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        let copy = dir.path().join(PUT_ASIDE).join("2026-11-14 090000");
        std::fs::create_dir_all(copy.join("store")).unwrap();
        std::fs::write(copy.join("store/nocturne.db"), b"OLDER").unwrap();
        std::fs::create_dir_all(copy.join("engine")).unwrap();
        std::fs::write(
            copy.join("engine/selection.json"),
            serde_json::to_vec(&selection()).unwrap(),
        )
        .unwrap();
        schedule(
            dir.path(),
            true,
            STAMP,
            Source::PutAside("2026-11-14 090000".into()),
            Some(aside()),
            None,
        )
        .unwrap();
        // The crash: what was here is put aside and the copy is in place, nothing after that.
        let kept = dir.path().join(PUT_ASIDE).join(STAMP);
        std::fs::create_dir_all(&kept).unwrap();
        std::fs::rename(dir.path().join("store"), kept.join("store")).unwrap();
        std::fs::rename(copy.join("store"), dir.path().join("store")).unwrap();

        assert_eq!(
            apply_pending(dir.path()),
            ApplyOutcome::Restored { kept: None }
        );
        assert!(!marker_path(dir.path()).exists());
        assert!(!copy.exists(), "the spent copy is tidied");
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"OLDER");
        assert_eq!(read(kept.join("store/nocturne.db")), b"OLDDB");
        let installed: Selection =
            serde_json::from_slice(&read(selection_path(dir.path()))).unwrap();
        assert!(installed.custom.contains_key(&ComponentRole::Mind));
    }

    #[test]
    fn a_crash_after_a_backup_moved_into_place_keeps_its_reading_setup() {
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        staged(dir.path());
        schedule(
            dir.path(),
            true,
            STAMP,
            Source::Stage,
            Some(aside()),
            Some(summary()),
        )
        .unwrap();
        let kept = dir.path().join(PUT_ASIDE).join(STAMP);
        std::fs::create_dir_all(&kept).unwrap();
        std::fs::rename(dir.path().join("store"), kept.join("store")).unwrap();
        std::fs::rename(
            stage_dir(dir.path()).join("store"),
            dir.path().join("store"),
        )
        .unwrap();

        assert_eq!(
            apply_pending(dir.path()),
            ApplyOutcome::Restored { kept: None }
        );
        let installed: Selection =
            serde_json::from_slice(&read(selection_path(dir.path()))).unwrap();
        assert_eq!(installed.tier, Tier::Standard);
        assert!(!stage_dir(dir.path()).exists());
        assert_eq!(apply_pending(dir.path()), ApplyOutcome::NothingPending);
        assert!(
            !interrupted_restore(dir.path()),
            "a finished restore is not reported as stopped"
        );
    }

    #[test]
    fn a_check_that_stopped_part_way_is_overwritten_at_the_next_start() {
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        let check = check_dir(dir.path()).join("store");
        std::fs::create_dir_all(&check).unwrap();
        std::fs::write(check.join("nocturne.db"), b"PLAIN").unwrap();
        assert_eq!(apply_pending(dir.path()), ApplyOutcome::NothingPending);
        assert!(!check_dir(dir.path()).exists());
        assert!(!dir.path().join(RESTORE).exists());
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"OLDDB");
    }

    #[test]
    fn a_marker_that_outlived_a_finished_bring_back_is_cleared() {
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        let copy = dir.path().join(PUT_ASIDE).join("2026-11-14 090000");
        std::fs::create_dir_all(copy.join("store")).unwrap();
        std::fs::write(copy.join("store/nocturne.db"), b"OLDER").unwrap();
        schedule(
            dir.path(),
            true,
            STAMP,
            Source::PutAside("2026-11-14 090000".into()),
            Some(aside()),
            None,
        )
        .unwrap();
        assert!(matches!(
            apply_pending(dir.path()),
            ApplyOutcome::Restored { kept: Some(_) }
        ));
        // A crash before the marker went: the same marker comes back on the next start.
        let marker = Marker {
            version: VERSION,
            replace: true,
            stamp: STAMP.into(),
            source: Source::PutAside("2026-11-14 090000".into()),
            aside: None,
            summary: None,
            arrival: false,
        };
        std::fs::create_dir_all(restore_dir(dir.path())).unwrap();
        std::fs::write(
            marker_path(dir.path()),
            serde_json::to_vec(&marker).unwrap(),
        )
        .unwrap();
        assert_eq!(
            apply_pending(dir.path()),
            ApplyOutcome::Restored { kept: None }
        );
        assert!(!marker_path(dir.path()).exists());
        assert_eq!(read(dir.path().join("store/nocturne.db")), b"OLDER");
    }

    #[test]
    fn a_good_reading_setup_keeps_its_choices_and_loses_the_download_state() {
        let bytes = serde_json::to_vec(&selection()).unwrap();
        let clean = sanitise_selection(&bytes).unwrap();
        let parsed: Selection = serde_json::from_slice(&clean).unwrap();
        assert!(parsed.pending.is_none(), "a restore starts no download");
        assert!(parsed.old.is_none(), "a restore removes nothing");
        assert_eq!(
            parsed.custom.get(&ComponentRole::Mind).map(String::as_str),
            Some("a-model.gguf")
        );
        assert_eq!(parsed.tier, Tier::Standard);
    }

    #[test]
    fn a_reading_setup_this_app_would_not_run_is_refused() {
        assert!(sanitise_selection(b"not json").is_none());
        assert!(sanitise_selection(br#"{"tier":"no such tier"}"#).is_none());
        for name in ["", "../escape", "a name with spaces", &"m".repeat(201)] {
            let mut bad = selection();
            bad.custom.insert(ComponentRole::Mind, name.to_string());
            let bytes = serde_json::to_vec(&bad).unwrap();
            assert!(sanitise_selection(&bytes).is_none(), "{name:?}");
        }
        assert!(sanitise_selection(&vec![b'x'; MAX_SELECTION + 1]).is_none());
    }

    #[test]
    fn a_stamp_must_be_one_this_app_wrote() {
        assert!(valid_id("2026-11-15 120000"));
        assert!(valid_id("2026-11-15 120000 (12)"));
        assert!(!valid_id("../x"));
        assert!(!valid_id("2026-11-15 120000/../x"));
        assert!(!valid_id("2026-11-15"));
        assert!(!valid_id("2026-11-15 12000"));
        assert!(!valid_id("2026-11-15 120000 ()"));
        assert!(!valid_id("2026_11_15 120000"));
        assert!(put_aside_path(Path::new("/data"), "../x").is_none());
    }

    #[test]
    fn an_interrupted_backup_is_cleaned_up_and_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let partial = dir.path().join("chosen.nocturne.partial");
        std::fs::write(&partial, b"half a file").unwrap();
        write_job(dir.path(), &partial).unwrap();
        apply_pending(dir.path());
        assert!(!partial.exists());
        assert!(!dir.path().join(BACKUP).join(JOB).exists());
        assert!(interrupted_backup(dir.path()));
        dismiss_notices(dir.path());
        assert!(!interrupted_backup(dir.path()));
    }

    #[test]
    fn remove_all_takes_the_stage_the_copies_and_the_backup_state() {
        let dir = tempfile::tempdir().unwrap();
        live(dir.path());
        staged(dir.path());
        schedule(dir.path(), true, STAMP, Source::Stage, Some(aside()), None).unwrap();
        apply_pending(dir.path());
        write_last(dir.path(), "2026-11-15 100000", 10.0).unwrap();
        assert!(dir.path().join(PUT_ASIDE).exists());
        remove_all(dir.path());
        assert!(!dir.path().join(RESTORE).exists());
        assert!(!dir.path().join(PUT_ASIDE).exists());
        assert!(!dir.path().join(BACKUP).exists());
    }

    #[test]
    fn deleting_one_copy_leaves_the_others() {
        let dir = tempfile::tempdir().unwrap();
        for at in ["2026-11-14 090000", "2026-11-15 100000"] {
            let folder = dir.path().join(PUT_ASIDE).join(at);
            std::fs::create_dir_all(folder.join("store")).unwrap();
            std::fs::write(folder.join("store/nocturne.db"), b"x").unwrap();
        }
        assert_eq!(put_aside(dir.path()).len(), 2);
        delete_put_aside(dir.path(), "2026-11-14 090000").unwrap();
        let left = put_aside(dir.path());
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, "2026-11-15 100000");
        assert!(delete_put_aside(dir.path(), "../x").is_err());
    }
}
