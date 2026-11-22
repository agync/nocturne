//! Passphrase backup: the store and its originals in one encrypted file.
//!
//! A backup is an age file, encrypted to a passphrase, whose plaintext is a plain tar archive:
//! `manifest.json`, then `store/nocturne.db`, then one `store/originals/<sha256>` per kept file
//! and optionally `engine/selection.json`. Making one takes a snapshot of the store, scrubs what is
//! not the person's own work out of it, streams the originals in without holding one in memory,
//! and then reads the whole file back before it takes its real name. Opening one checks every
//! part, and a restore is brought up to date in a staging folder, so nothing half-checked is
//! ever mistaken for a store.

use std::cell::Cell;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use age::secrecy::SecretString;
use nocturne_contract::{BackupPhase, BackupReport, BackupSummary, Holdings, KeptCounts};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::Engine;
use crate::error::{Error, Result};
use crate::fsutil;
use crate::store::{self, lock_down};
use crate::util::{fmt_ts, hex};

/// The file extension of a passphrase backup.
pub const BACKUP_EXT: &str = "nocturne";
/// The fewest characters a new backup passphrase may have.
pub const MIN_PASSPHRASE_CHARS: usize = 8;

/// The only file a caller may add to a backup, beside the store.
const SELECTION: &str = "engine/selection.json";
/// The scratch folder under the store, holding the snapshot while a backup is written.
const SCRATCH: &str = "backup-tmp";
const MANIFEST: &str = "manifest.json";
const DB_ENTRY: &str = "store/nocturne.db";
/// The backup format this build writes and reads; a higher number is a newer Nocturne's.
const FORMAT: u32 = 1;
/// The costliest passphrase setting a file may ask for, so a hostile file cannot stall opening.
const MAX_WORK_FACTOR: u8 = 19;
/// The passphrase setting a backup asks for, well below what the reader allows.
const WORK_FACTOR: u8 = 18;
const CHUNK: usize = 64 * 1024;
const BLOCK: usize = 512;
const MAX_MANIFEST: usize = 1024 * 1024;

/// What a backup run was told to do besides the passphrase.
pub struct BackupRun<'a> {
    /// (archive name, file path) pairs; only `engine/selection.json` is allowed, and a path that
    /// does not exist is skipped.
    pub extras: &'a [(&'a str, &'a Path)],
    /// Called with the phase and the bytes done out of the bytes in all.
    pub progress: &'a dyn Fn(BackupPhase, u64, u64),
    /// When set, the run stops and leaves nothing behind.
    pub cancel: &'a AtomicBool,
    /// Called once with the partial file just after it is created, so a crash can be cleaned up.
    pub started: &'a dyn Fn(&Path),
    /// The passphrase work to ask of whoever opens the file. `None` is what Nocturne writes
    /// (`WORK_FACTOR`); a test may pass less so its backups open quickly.
    pub work_factor: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: u32,
    store_version: u32,
    app_version: String,
    made_at: String,
    memories: u32,
    items: u32,
    pages: u32,
    originals: u32,
    #[serde(default)]
    online_used: bool,
    #[serde(default)]
    away_used: bool,
}

/// The entries a backup wrote, in order, and the manifest bytes, for reading it back.
pub(crate) struct Written {
    pub entries: Vec<(String, String)>,
    pub manifest: Vec<u8>,
}

/// One original to stream into the archive.
struct Original {
    sha: String,
    path: PathBuf,
    size: u64,
}

/// Bytes copied so far, out of the whole run, and the person's way to stop.
struct Counting<'a> {
    phase: BackupPhase,
    done: u64,
    total: u64,
    progress: &'a dyn Fn(BackupPhase, u64, u64),
    cancel: &'a AtomicBool,
}

impl Counting<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    fn step(&mut self, n: u64) {
        self.done += n;
        (self.progress)(self.phase, self.done, self.total);
    }

    fn finish(&mut self) {
        if self.done < self.total {
            self.done = self.total;
            (self.progress)(self.phase, self.done, self.total);
        }
    }
}

/// The passphrase as it is used: one Unicode form, so the same characters typed on another
/// keyboard open the same backup.
fn normalise(passphrase: &str) -> String {
    passphrase.nfc().collect()
}

fn partial_path(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_owned();
    name.push(".partial");
    PathBuf::from(name)
}

/// Remove the partial file, and overwrite and remove the scratch folder, when dropped, unless the
/// backup was committed. The partial file is encrypted, so it is only removed.
struct BackupGuard {
    partial: PathBuf,
    scratch: PathBuf,
    committed: bool,
}

impl Drop for BackupGuard {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.partial);
        }
        fsutil::shred_tree(&self.scratch);
    }
}

/// Remove a scratch folder a stopped run left behind, with its copy of the store in it.
pub(crate) fn remove_leftover_scratch(dir: &Path) {
    let scratch = dir.join(SCRATCH);
    if scratch.exists() {
        fsutil::shred_tree(&scratch);
    }
}

fn write_header(w: &mut impl Write, name: &str, size: u64) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header
        .set_path(name)
        .map_err(|_| Error::invalid("a backup entry name is too long"))?;
    header.set_size(size);
    header.set_mode(0o600);
    header.set_mtime(0);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_cksum();
    w.write_all(header.as_bytes())?;
    Ok(())
}

fn write_pad(w: &mut impl Write, size: u64) -> Result<()> {
    let rem = (BLOCK as u64 - (size % BLOCK as u64)) % BLOCK as u64;
    if rem > 0 {
        w.write_all(&[0u8; BLOCK][..rem as usize])?;
    }
    Ok(())
}

/// Write one entry from memory and return the SHA-256 of the bytes written.
fn write_bytes(w: &mut impl Write, name: &str, bytes: &[u8]) -> Result<String> {
    write_header(w, name, bytes.len() as u64)?;
    w.write_all(bytes)?;
    write_pad(w, bytes.len() as u64)?;
    Ok(hex(&Sha256::digest(bytes)))
}

/// Stream exactly `size` bytes from `src`, hash them and pad the tar block.
fn write_stream(
    w: &mut impl Write,
    name: &str,
    size: u64,
    src: &mut impl Read,
    state: &mut Counting,
    short: &'static str,
) -> Result<String> {
    write_header(w, name, size)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut left = size;
    while left > 0 {
        if state.cancelled() {
            return Err(Error::Cancelled);
        }
        let want = left.min(CHUNK as u64) as usize;
        let n = src.read(&mut buf[..want])?;
        if n == 0 {
            return Err(Error::invalid(short));
        }
        hasher.update(&buf[..n]);
        w.write_all(&buf[..n])?;
        left -= n as u64;
        state.step(n as u64);
    }
    write_pad(w, size)?;
    Ok(hex(&hasher.finalize()))
}

fn is_sha256(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The `<sha256>` part of an original's entry name.
fn original_part(name: &str) -> Option<&str> {
    let rest = name.strip_prefix("store/originals/")?;
    is_sha256(rest).then_some(rest)
}

fn connection_has_data(conn: &Connection) -> Result<bool> {
    for table in [
        "items",
        "memories",
        "entities",
        "pages",
        "questions",
        "settings",
        "ledger",
        "online_sends",
    ] {
        let any: bool =
            conn.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| {
                r.get(0)
            })?;
        if any {
            return Ok(true);
        }
    }
    // A phone's own row is there from its first start and holds nothing of anyone's.
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM remote_devices WHERE connection != 'local')",
        [],
        |r| r.get(0),
    )?)
}

/// Open an unpacked database for checking, already locked down.
fn open_for_check(db: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db)?;
    lock_down(&conn)?;
    Ok(conn)
}

fn originals_exist(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|mut entries| {
        entries.any(|entry| entry.is_ok_and(|e| e.file_type().is_ok_and(|t| t.is_file())))
    })
}

/// True when a store folder on disk holds anything of the person's. A store whose database
/// cannot be opened counts as holding something, so nothing is discarded on a guess.
pub fn store_has_data(dir: &Path) -> Result<bool> {
    let db = dir.join(crate::DB_FILE);
    if db.is_file() {
        let conn = Connection::open_with_flags(
            &db,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        if connection_has_data(&conn)? {
            return Ok(true);
        }
    }
    Ok(originals_exist(&dir.join("originals")))
}

impl Engine {
    /// The name to suggest in a save dialog for today's backup.
    pub fn suggested_backup_name(&self) -> String {
        let day = self
            .clock()
            .now()
            .to_zoned(self.clock().tz())
            .date()
            .to_string();
        format!("Nocturne backup {day}.{BACKUP_EXT}")
    }

    /// The bytes a backup would roughly take: the store and the kept originals as they are.
    pub fn backup_estimate(&self) -> Result<u64> {
        let mut total = std::fs::metadata(self.db_path())
            .map(|m| m.len())
            .unwrap_or(0);
        if let Ok(entries) = std::fs::read_dir(self.originals_dir()) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata()
                    && meta.is_file()
                {
                    total += meta.len();
                }
            }
        }
        Ok(total)
    }

    /// What this store holds now, or `None` when it holds nothing of the person's. `bytes` is
    /// what a backup of it would roughly take.
    pub fn holdings(&self, made_after: Option<&str>) -> Result<Option<Holdings>> {
        if !self.has_data()? {
            return Ok(None);
        }
        let (memories, pages, originals, last_kept, kept_after_backup) = self.read(|conn| {
            let memories = count(
                conn,
                "SELECT COUNT(*) FROM memories WHERE superseded_at IS NULL",
            )?;
            let pages = count(conn, "SELECT COUNT(*) FROM pages")?;
            let originals = count(
                conn,
                "SELECT COUNT(*) FROM items WHERE file_name IS NOT NULL",
            )?;
            let last_kept: Option<String> =
                conn.query_row("SELECT MAX(kept_at) FROM items", [], |r| r.get(0))?;
            let kept_after_backup = match made_after {
                Some(after) => conn.query_row(
                    "SELECT COUNT(*) FROM items WHERE kept_at > ?1",
                    [after],
                    |r| r.get::<_, i64>(0),
                )? as u32,
                None => 0,
            };
            Ok((memories, pages, originals, last_kept, kept_after_backup))
        })?;
        Ok(Some(Holdings {
            memories,
            pages,
            originals,
            last_kept,
            kept_after_backup,
            bytes: self.backup_estimate()? as f64,
        }))
    }

    /// What this store holds, in the columns a phone shows before a restore.
    pub fn kept_counts(&self) -> Result<KeptCounts> {
        self.read(kept_counts_in)
    }
}

impl Engine {
    /// Write an encrypted backup to `dest`, which the save dialog chose, and return what it
    /// holds. The parent folder must exist. The file is written as `<dest>.partial`, read back
    /// to prove it, and only then renamed over `dest`.
    pub fn backup_to(
        &self,
        dest: &Path,
        passphrase: &str,
        run: &BackupRun,
    ) -> Result<BackupReport> {
        if passphrase.chars().count() < MIN_PASSPHRASE_CHARS {
            return Err(Error::invalid(format!(
                "a backup passphrase needs at least {MIN_PASSPHRASE_CHARS} characters"
            )));
        }
        for &(name, _) in run.extras {
            if name != SELECTION {
                return Err(Error::invalid(format!("{name} cannot go into a backup")));
            }
        }
        let parent = dest
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if !parent.is_dir() {
            return Err(Error::invalid("the folder to save into is not there"));
        }
        let passphrase = normalise(passphrase);
        let generation = self.inner.forget_gen.load(Ordering::SeqCst);
        let partial = partial_path(dest);
        // A partial a stopped run left behind is never reused.
        let _ = std::fs::remove_file(&partial);
        let scratch = self.inner.dir.join(SCRATCH);
        fsutil::create_private_dir(&scratch)?;
        let guard = BackupGuard {
            partial: partial.clone(),
            scratch,
            committed: false,
        };

        // One consistent moment, in a file of its own.
        let snapshot = guard.scratch.join("nocturne.db");
        let snapshot_text = snapshot
            .to_str()
            .ok_or_else(|| Error::invalid("the store path is not valid text"))?;
        self.read(|conn| {
            conn.execute("VACUUM INTO ?1", [snapshot_text])?;
            Ok(())
        })?;
        let snap = self.scrub_snapshot(&snapshot)?;

        let mut extra_files: Vec<(&str, PathBuf, u64)> = Vec::new();
        for &(name, path) in run.extras {
            if !path.exists() {
                continue;
            }
            let size = std::fs::metadata(path)?.len();
            extra_files.push((name, path.to_path_buf(), size));
        }

        let total = snap.size
            + snap.originals.iter().map(|o| o.size).sum::<u64>()
            + extra_files.iter().map(|e| e.2).sum::<u64>();

        let file = fsutil::create_private_file_new(&partial)?;
        (run.started)(&partial);
        let mut recipient = age::scrypt::Recipient::new(SecretString::from(passphrase.clone()));
        recipient.set_work_factor(run.work_factor.unwrap_or(WORK_FACTOR).min(WORK_FACTOR));
        let encryptor =
            age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
                .map_err(|e| Error::invalid(format!("the passphrase could not be used: {e}")))?;
        let stream = encryptor.wrap_output(file)?;
        let mut tar = tar::Builder::new(stream);
        let mut state = Counting {
            phase: BackupPhase::Saving,
            done: 0,
            total,
            progress: run.progress,
            cancel: run.cancel,
        };
        (run.progress)(BackupPhase::Saving, 0, total);
        if state.cancelled() {
            return Err(Error::Cancelled);
        }

        let manifest = serde_json::to_vec(&Manifest {
            format: FORMAT,
            store_version: snap.version,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            made_at: fmt_ts(self.clock().now()),
            memories: snap.memories,
            items: snap.items,
            pages: snap.pages,
            originals: snap.originals.len() as u32,
            online_used: snap.online_used,
            away_used: snap.away_used,
        })?;
        let mut written = Written {
            entries: Vec::new(),
            manifest: manifest.clone(),
        };
        written.entries.push((
            MANIFEST.to_string(),
            write_bytes(tar.get_mut(), MANIFEST, &manifest)?,
        ));

        let mut snapshot_file = std::fs::File::open(&snapshot)?;
        let hash = write_stream(
            tar.get_mut(),
            DB_ENTRY,
            snap.size,
            &mut snapshot_file,
            &mut state,
            "the snapshot ended early",
        )?;
        written.entries.push((DB_ENTRY.to_string(), hash));

        for original in &snap.originals {
            if state.cancelled() {
                return Err(Error::Cancelled);
            }
            let mut source = match std::fs::File::open(&original.path) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            let name = format!("store/originals/{}", original.sha);
            let hash = write_stream(
                tar.get_mut(),
                &name,
                original.size,
                &mut source,
                &mut state,
                "an original does not match what was kept",
            )?;
            // The entry's size came from metadata, so a file that grew since would be cut
            // short without a word: one more byte catches that.
            let mut extra = [0u8; 1];
            if source.read(&mut extra)? > 0 {
                return Err(Error::invalid("an original does not match what was kept"));
            }
            if hash != original.sha {
                return Err(Error::invalid("an original does not match what was kept"));
            }
            written.entries.push((name, hash));
        }

        for &(name, ref path, size) in &extra_files {
            if state.cancelled() {
                return Err(Error::Cancelled);
            }
            let mut source = std::fs::File::open(path)?;
            let hash = write_stream(
                tar.get_mut(),
                name,
                size,
                &mut source,
                &mut state,
                "a file ended early",
            )?;
            written.entries.push((name.to_string(), hash));
        }

        tar.finish()?;
        let stream = tar.into_inner()?;
        let out = stream.finish()?;
        out.sync_all()?;
        drop(out);
        state.finish();

        // Read the whole file back before it takes its real name.
        verify_written(&partial, &passphrase, &written, run.progress, run.cancel)?;

        // A forget or an undone keep began while the files were copied: what was written may
        // hold what the person just removed, so none of it is kept.
        if self.inner.forget_gen.load(Ordering::SeqCst) != generation {
            return Err(Error::Busy);
        }
        std::fs::rename(&partial, dest)?;
        let bytes = std::fs::metadata(dest)?.len() as f64;
        let mut guard = guard;
        guard.committed = true;
        Ok(BackupReport {
            path: dest.to_string_lossy().into_owned(),
            memories: snap.memories,
            pages: snap.pages,
            originals: snap.originals.len() as u32,
            bytes,
        })
    }

    /// True when this store holds anything of the person's: a kept item, a memory, a thing, a
    /// page, a question, a setting, a phone, a ledger row, an online question or an original.
    pub fn has_data(&self) -> Result<bool> {
        let in_tables = self.read(connection_has_data)?;
        Ok(in_tables || originals_exist(&self.originals_dir()))
    }

    /// Scrub the snapshot and list the originals it still keeps. Returns its size, version,
    /// counts and whether online answers or a phone were in use, all before the scrub.
    fn scrub_snapshot(&self, snapshot: &Path) -> Result<Snapshot> {
        let now = fmt_ts(self.clock().now());
        let conn = Connection::open(snapshot)?;
        let online_used: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM online_state WHERE set_up = 1)",
            [],
            |r| r.get(0),
        )?;
        let away_used: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM remote_devices WHERE connection != 'local') \
             OR EXISTS(SELECT 1 FROM remote_state WHERE first_on_at IS NOT NULL)",
            [],
            |r| r.get(0),
        )?;
        scrub_database(&conn, &now)?;

        let memories = count(
            &conn,
            "SELECT COUNT(*) FROM memories WHERE superseded_at IS NULL",
        )?;
        let items = count(&conn, "SELECT COUNT(*) FROM items")?;
        let pages = count(&conn, "SELECT COUNT(*) FROM pages")?;
        let mut originals = Vec::new();
        {
            let mut stmt = conn.prepare(&format!(
                "SELECT i.sha256, i.ext FROM items i \
                 WHERE i.file_name IS NOT NULL AND {} \
                 ORDER BY i.sha256",
                crate::forget::ORIGINALS_NOT_IN_FORGET
            ))?;
            let rows: Vec<(String, Option<String>)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let originals_dir = self.originals_dir();
            for (sha, ext) in rows {
                let path = crate::capture::original_path(&originals_dir, &sha, ext.as_deref());
                match std::fs::metadata(&path) {
                    Ok(meta) => originals.push(Original {
                        sha,
                        path,
                        size: meta.len(),
                    }),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }

        let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        drop(conn);
        let size = std::fs::metadata(snapshot)?.len();
        Ok(Snapshot {
            size,
            version,
            memories,
            items,
            pages,
            online_used,
            away_used,
            originals,
        })
    }
}

/// Remove everything that belongs to the machine the store is leaving, and every half-finished
/// model change, in one transaction, then compact the file so nothing removed stays in it.
fn scrub_database(conn: &Connection, now: &str) -> Result<()> {
    conn.pragma_update(None, "secure_delete", "ON")?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    conn.execute_batch(
        "DELETE FROM audio_cache;
         DELETE FROM forget_tokens;
         UPDATE forget_jobs SET files = '[]';
         DELETE FROM online_state;
         INSERT INTO online_state(id) VALUES (1);
         DELETE FROM remote_state;
         INSERT INTO remote_state(id) VALUES (1);
         DELETE FROM reindex_state;
         INSERT INTO reindex_state(id) VALUES (1);
         DELETE FROM embeddings_next;",
    )?;
    conn.execute(
        "DELETE FROM settings WHERE key = ?1",
        [crate::TRUNCATE_PENDING],
    )?;
    // The phones stay paired by name, but what would let them in again does not travel. A
    // phone's own row was never a pairing and travels as it is.
    conn.execute(
        "UPDATE remote_devices SET public_key = x'', \
         revoked_at = COALESCE(revoked_at, ?1), address = NULL WHERE connection != 'local'",
        [now],
    )?;
    conn.execute_batch("COMMIT")?;
    // One plain file, with what was deleted overwritten and compacted away.
    conn.pragma_update(None, "journal_mode", "DELETE")?;
    conn.execute_batch("VACUUM")?;
    Ok(())
}

/// Every object in a database's schema, by type, name and statement.
fn schema_rows(conn: &Connection) -> Result<HashSet<(String, String, String)>> {
    let mut stmt = conn.prepare("SELECT type, name, COALESCE(sql, '') FROM sqlite_schema")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The schema a store at `version` has, read from a store migrated to that version from nothing.
fn allowed_schema(version: u32) -> Result<HashSet<(String, String, String)>> {
    let conn = Connection::open_in_memory()?;
    for (v, sql) in store::migrations() {
        if v <= version {
            conn.execute_batch(sql)?;
        }
    }
    schema_rows(&conn)
}

/// Spacing is not part of a definition.
fn squeeze(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Refuse a database whose schema is not exactly the one this build would have made at the
/// version it says it is. A table with another definition (an extra column, a check, a generated
/// column), an index with an expression, or a trigger or view from elsewhere could undo the
/// scrub or send what it reads somewhere.
fn validate_schema(conn: &Connection, version: u32) -> Result<()> {
    let allowed: HashSet<(String, String, String)> = allowed_schema(version)?
        .into_iter()
        .map(|(kind, name, sql)| (kind, name, squeeze(&sql)))
        .collect();
    let found: HashSet<(String, String, String)> = schema_rows(conn)?
        .into_iter()
        .map(|(kind, name, sql)| (kind, name, squeeze(&sql)))
        .collect();
    if found == allowed {
        Ok(())
    } else {
        Err(Error::BackupDamaged)
    }
}

/// Refuse item rows whose original cannot be placed safely under the originals folder.
fn validate_items(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("SELECT sha256, ext FROM items")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
    })?;
    for row in rows {
        let (sha, ext) = row?;
        if !is_sha256(&sha)
            || ext
                .as_deref()
                .is_some_and(|e| !crate::capture::extension_ok(e))
        {
            return Err(Error::BackupDamaged);
        }
    }
    Ok(())
}

/// Give every unpacked original the name the store expects, with the extension from its row.
fn place_originals(conn: &Connection, dir: &Path) -> Result<()> {
    let mut stmt = conn.prepare("SELECT sha256, ext FROM items WHERE ext IS NOT NULL")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (sha, ext) = row?;
        let from = dir.join(&sha);
        let to = crate::capture::original_path(dir, &sha, Some(&ext));
        if from.is_file() && from != to {
            std::fs::rename(&from, &to)?;
        }
    }
    Ok(())
}

struct Snapshot {
    size: u64,
    version: u32,
    memories: u32,
    items: u32,
    pages: u32,
    online_used: bool,
    away_used: bool,
    originals: Vec<Original>,
}

fn count(conn: &Connection, sql: &str) -> Result<u32> {
    let n: i64 = conn.query_row(sql, [], |r| r.get(0))?;
    Ok(n as u32)
}

/// The columns a phone shows before a restore: things kept apart from the journal, check-ins,
/// the calendar and readings; journal entries and good things as they stand now; photos and
/// recordings with their files.
pub(crate) fn kept_counts_in(conn: &Connection) -> Result<KeptCounts> {
    let things = count(
        conn,
        "SELECT COUNT(*) FROM items WHERE kind NOT IN ('checkin', 'calendar', 'reading') \
         AND json_extract(metadata, '$.journal') IS NULL",
    )?;
    let journal_entries = count(
        conn,
        "SELECT COUNT(*) FROM items i WHERE json_extract(i.metadata, '$.journal') IN ('entry', 'good') \
         AND NOT EXISTS (SELECT 1 FROM items n WHERE json_extract(n.metadata, '$.replaces') = i.id)",
    )?;
    let photos_and_recordings = count(
        conn,
        "SELECT COUNT(*) FROM items WHERE kind IN ('image', 'audio') AND file_name IS NOT NULL",
    )?;
    Ok(KeptCounts {
        things,
        journal_entries,
        photos_and_recordings,
    })
}

/// [`Engine::kept_counts`] for a store folder that is not open, such as one put aside. Read
/// only; a store that cannot be read is an error.
pub fn store_kept_counts(dir: &Path) -> Result<KeptCounts> {
    let conn = Connection::open_with_flags(
        dir.join(crate::DB_FILE),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    kept_counts_in(&conn)
}

/// The backup file, counted as age reads it, so progress can report bytes of the file itself.
struct CountingFile {
    file: std::fs::File,
    read: Rc<Cell<u64>>,
}

impl Read for CountingFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.file.read(buf)?;
        self.read.set(self.read.get() + n as u64);
        Ok(n)
    }
}

type Payload = age::stream::StreamReader<CountingFile>;

/// Where an entry being read reports its bytes and asks whether to stop.
struct Reading<'a> {
    at: Rc<Cell<u64>>,
    size: u64,
    phase: BackupPhase,
    progress: &'a dyn Fn(BackupPhase, u64, u64),
    cancel: &'a AtomicBool,
}

impl Reading<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    fn report(&self) {
        (self.progress)(self.phase, self.at.get(), self.size);
    }

    fn finish(&self) {
        (self.progress)(self.phase, self.size, self.size);
    }
}

/// Ok when `file` starts with an age header a passphrase can open. No passphrase is needed and
/// nothing is unpacked.
pub fn probe_backup(file: &Path) -> Result<()> {
    let input = std::fs::File::open(file)?;
    let decryptor = age::Decryptor::new(input).map_err(|_| Error::NotABackup)?;
    if !decryptor.is_scrypt() {
        return Err(Error::NotABackup);
    }
    Ok(())
}

/// Read a whole backup back and check it against what was written. Any mismatch is
/// `BackupDamaged`; a cancel stops at once.
pub(crate) fn verify_written(
    file: &Path,
    passphrase: &str,
    written: &Written,
    progress: &dyn Fn(BackupPhase, u64, u64),
    cancel: &AtomicBool,
) -> Result<()> {
    let input = std::fs::File::open(file)?;
    let size = input.metadata()?.len();
    let counter = Rc::new(Cell::new(0u64));
    let counting = CountingFile {
        file: input,
        read: Rc::clone(&counter),
    };
    let decryptor = age::Decryptor::new(counting).map_err(|_| Error::BackupDamaged)?;
    let mut identity = age::scrypt::Identity::new(SecretString::from(normalise(passphrase)));
    identity.set_max_work_factor(MAX_WORK_FACTOR);
    let payload = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|_| Error::BackupDamaged)?;
    let reading = Reading {
        at: Rc::clone(&counter),
        size,
        phase: BackupPhase::Checking,
        progress,
        cancel,
    };
    (progress)(BackupPhase::Checking, 0, size);

    let mut archive = tar::Archive::new(payload);
    let mut seen = 0usize;
    {
        // Raw: a long-name or extended header is refused as it is, never read into memory.
        let entries = archive
            .entries()
            .map_err(|_| Error::BackupDamaged)?
            .raw(true);
        for entry in entries {
            let mut entry = entry.map_err(|_| Error::BackupDamaged)?;
            if !entry.header().entry_type().is_file() {
                return Err(Error::BackupDamaged);
            }
            if reading.cancelled() {
                return Err(Error::Cancelled);
            }
            let name = entry
                .path()
                .map_err(|_| Error::BackupDamaged)?
                .to_str()
                .ok_or(Error::BackupDamaged)?
                .to_string();
            let Some((expected_name, expected_hash)) = written.entries.get(seen) else {
                return Err(Error::BackupDamaged);
            };
            if &name != expected_name {
                return Err(Error::BackupDamaged);
            }
            let mut hasher = Sha256::new();
            let mut manifest = Vec::new();
            let mut buf = vec![0u8; CHUNK];
            loop {
                if reading.cancelled() {
                    return Err(Error::Cancelled);
                }
                let n = entry.read(&mut buf).map_err(|_| Error::BackupDamaged)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                if name == MANIFEST && manifest.len() < MAX_MANIFEST {
                    manifest.extend_from_slice(&buf[..n]);
                }
                reading.report();
            }
            if name == MANIFEST && manifest != written.manifest {
                return Err(Error::BackupDamaged);
            }
            if hex(&hasher.finalize()) != *expected_hash {
                return Err(Error::BackupDamaged);
            }
            seen += 1;
        }
    }
    if seen != written.entries.len() {
        return Err(Error::BackupDamaged);
    }

    // Read to the end of the encrypted stream, so a cut after the last entry is noticed too.
    let mut rest = archive.into_inner();
    let mut sink = vec![0u8; CHUNK];
    loop {
        if reading.cancelled() {
            return Err(Error::Cancelled);
        }
        match rest.read(&mut sink) {
            Ok(0) => break,
            Ok(_) => reading.report(),
            Err(_) => return Err(Error::BackupDamaged),
        }
    }
    reading.finish();
    Ok(())
}

/// Remove a folder unless it is kept: a check never keeps one, a restore keeps the unpacked one.
struct DirGuard {
    root: PathBuf,
    keep: bool,
}

impl Drop for DirGuard {
    fn drop(&mut self) {
        if !self.keep {
            fsutil::shred_tree(&self.root);
        }
    }
}

/// Check a backup in full without keeping anything: the database alone is written to `scratch`
/// to be read and migrated, and the folder is removed whether this succeeds or not.
pub fn check_backup(
    file: &Path,
    passphrase: &str,
    scratch: &Path,
    progress: &dyn Fn(BackupPhase, u64, u64),
    cancel: &AtomicBool,
) -> Result<BackupSummary> {
    check_backup_counted(file, passphrase, scratch, progress, cancel).map(|(summary, _)| summary)
}

/// [`check_backup`], with what the backup holds in the columns a phone shows.
pub fn check_backup_counted(
    file: &Path,
    passphrase: &str,
    scratch: &Path,
    progress: &dyn Fn(BackupPhase, u64, u64),
    cancel: &AtomicBool,
) -> Result<(BackupSummary, KeptCounts)> {
    read_backup(
        file,
        passphrase,
        scratch,
        false,
        BackupPhase::Checking,
        progress,
        cancel,
    )
}

/// Unpack a backup into `stage` for a restore, checking every part as it goes.
pub fn unpack_backup(
    file: &Path,
    passphrase: &str,
    stage: &Path,
    progress: &dyn Fn(BackupPhase, u64, u64),
    cancel: &AtomicBool,
) -> Result<BackupSummary> {
    read_backup(
        file,
        passphrase,
        stage,
        true,
        BackupPhase::Restoring,
        progress,
        cancel,
    )
    .map(|(summary, _)| summary)
}

/// The one reader both checking and unpacking use: `keep_originals` says whether the files are
/// written out or only hashed and dropped.
fn read_backup(
    file: &Path,
    passphrase: &str,
    dir: &Path,
    keep_originals: bool,
    phase: BackupPhase,
    progress: &dyn Fn(BackupPhase, u64, u64),
    cancel: &AtomicBool,
) -> Result<(BackupSummary, KeptCounts)> {
    let input = std::fs::File::open(file)?;
    let size = input.metadata()?.len();
    let counter = Rc::new(Cell::new(0u64));
    let counting = CountingFile {
        file: input,
        read: Rc::clone(&counter),
    };
    let decryptor = age::Decryptor::new(counting).map_err(|e| match e {
        age::DecryptError::ExcessiveWork { .. } => Error::BackupDamaged,
        _ => Error::NotABackup,
    })?;
    let mut identity = age::scrypt::Identity::new(SecretString::from(normalise(passphrase)));
    identity.set_max_work_factor(MAX_WORK_FACTOR);
    let payload = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|e| match e {
            age::DecryptError::DecryptionFailed
            | age::DecryptError::NoMatchingKeys
            | age::DecryptError::KeyDecryptionFailed => Error::WrongPassphrase,
            _ => Error::BackupDamaged,
        })?;
    let reading = Reading {
        at: Rc::clone(&counter),
        size,
        phase,
        progress,
        cancel,
    };

    crate::private::create_dir_new(dir)?;
    let mut guard = DirGuard {
        root: dir.to_path_buf(),
        keep: false,
    };
    let store_dir = dir.join("store");
    crate::private::create_dir_new(&store_dir)?;
    if keep_originals {
        crate::private::create_dir_new(&store_dir.join("originals"))?;
    }
    (progress)(phase, 0, size);

    let mut manifest: Option<Manifest> = None;
    let mut have_db = false;
    let mut originals = 0u32;
    let mut names: HashSet<String> = HashSet::new();
    let mut step = 0u8;
    let mut archive = tar::Archive::new(payload);
    {
        // Raw: a long-name or extended header is refused as it is, never read into memory.
        let entries = archive
            .entries()
            .map_err(|_| Error::BackupDamaged)?
            .raw(true);
        for entry in entries {
            let mut entry = entry.map_err(|_| Error::BackupDamaged)?;
            if !entry.header().entry_type().is_file() {
                return Err(Error::BackupDamaged);
            }
            if reading.cancelled() {
                return Err(Error::Cancelled);
            }
            let name = entry
                .path()
                .map_err(|_| Error::BackupDamaged)?
                .to_str()
                .ok_or(Error::BackupDamaged)?
                .to_string();
            if !names.insert(name.clone()) {
                return Err(Error::BackupDamaged);
            }
            if step == 0 {
                if name != MANIFEST {
                    return Err(Error::BackupDamaged);
                }
                let bytes = read_small(&mut entry, &reading)?;
                let found: Manifest =
                    serde_json::from_slice(&bytes).map_err(|_| Error::BackupDamaged)?;
                if found.format > FORMAT {
                    return Err(Error::BackupTooNew);
                }
                if found.format == 0 {
                    return Err(Error::BackupDamaged);
                }
                if found.store_version > store::latest_version() {
                    return Err(Error::BackupTooNew);
                }
                manifest = Some(found);
                step = 1;
            } else if step == 1 {
                if name != DB_ENTRY {
                    return Err(Error::BackupDamaged);
                }
                extract(
                    &mut entry,
                    Some(&store_dir.join("nocturne.db")),
                    &reading,
                    None,
                )?;
                have_db = true;
                step = 2;
            } else if name == SELECTION {
                if step != 2 {
                    return Err(Error::BackupDamaged);
                }
                let dest = if keep_originals {
                    crate::private::create_dir_new(&dir.join("engine"))?;
                    Some(dir.join(SELECTION))
                } else {
                    None
                };
                extract(&mut entry, dest.as_deref(), &reading, None)?;
                step = 3;
            } else if let Some(sha) = original_part(&name) {
                if step != 2 {
                    return Err(Error::BackupDamaged);
                }
                let dest = if keep_originals {
                    Some(store_dir.join("originals").join(sha))
                } else {
                    None
                };
                let mut hasher = Some(Sha256::new());
                extract(&mut entry, dest.as_deref(), &reading, hasher.as_mut())?;
                let hash = hasher
                    .map(|h| hex(&h.finalize()))
                    .unwrap_or_else(String::new);
                if hash != sha {
                    return Err(Error::BackupDamaged);
                }
                originals += 1;
            } else {
                return Err(Error::BackupDamaged);
            }
        }
    }
    if manifest.is_none() || !have_db {
        return Err(Error::BackupDamaged);
    }

    // Read to the end of the encrypted stream, so a cut after the last entry is noticed too.
    let mut rest = archive.into_inner();
    let mut sink = vec![0u8; CHUNK];
    loop {
        if reading.cancelled() {
            return Err(Error::Cancelled);
        }
        match rest.read(&mut sink) {
            Ok(0) => break,
            Ok(_) => reading.report(),
            Err(_) => return Err(Error::BackupDamaged),
        }
    }

    let db = store_dir.join("nocturne.db");
    {
        let conn = open_for_check(&db).map_err(|_| Error::BackupDamaged)?;
        let version: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|_| Error::BackupDamaged)?;
        if version > store::latest_version() {
            return Err(Error::BackupTooNew);
        }
        validate_schema(&conn, version).map_err(|_| Error::BackupDamaged)?;
        let check: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|_| Error::BackupDamaged)?;
        if check != "ok" {
            return Err(Error::BackupDamaged);
        }
        validate_items(&conn).map_err(|_| Error::BackupDamaged)?;
    }

    // An older backup is brought up to date here, in the scratch or staging folder. A restore
    // is scrubbed too: a crafted or older file may carry what a backup would have removed.
    let conn = store::open_untrusted(&db).map_err(|_| Error::BackupDamaged)?;
    if keep_originals {
        place_originals(&conn, &store_dir.join("originals")).map_err(|_| Error::BackupDamaged)?;
        scrub_database(&conn, &fmt_ts(jiff::Timestamp::now())).map_err(|_| Error::BackupDamaged)?;
    }
    let memories = count(
        &conn,
        "SELECT COUNT(*) FROM memories WHERE superseded_at IS NULL",
    )
    .map_err(|_| Error::BackupDamaged)?;
    let items = count(&conn, "SELECT COUNT(*) FROM items").map_err(|_| Error::BackupDamaged)?;
    let pages = count(&conn, "SELECT COUNT(*) FROM pages").map_err(|_| Error::BackupDamaged)?;
    let kept = kept_counts_in(&conn).map_err(|_| Error::BackupDamaged)?;
    let _: i64 = conn
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
        .map_err(|_| Error::BackupDamaged)?;
    drop(conn);
    for suffix in ["-wal", "-shm"] {
        let mut name = db.as_os_str().to_owned();
        name.push(suffix);
        fsutil::shred(Path::new(&name));
    }

    let found = manifest.ok_or(Error::BackupDamaged)?;
    guard.keep = keep_originals;
    let summary = BackupSummary {
        made_at: found.made_at,
        app_version: found.app_version,
        store_version: found.store_version,
        memories,
        items,
        pages,
        originals,
        online_used: found.online_used,
        away_used: found.away_used,
        bytes: size as f64,
    };
    Ok((summary, kept))
}

fn read_small(entry: &mut tar::Entry<'_, Payload>, reading: &Reading) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        if reading.cancelled() {
            return Err(Error::Cancelled);
        }
        let n = entry.read(&mut buf).map_err(|_| Error::BackupDamaged)?;
        if n == 0 {
            return Ok(out);
        }
        out.extend_from_slice(&buf[..n]);
        if out.len() > MAX_MANIFEST {
            return Err(Error::BackupDamaged);
        }
        reading.report();
    }
}

/// Copy one entry, writing it when a destination is given and hashing it always.
fn extract(
    entry: &mut tar::Entry<'_, Payload>,
    dest: Option<&Path>,
    reading: &Reading,
    mut hasher: Option<&mut Sha256>,
) -> Result<()> {
    let mut out = match dest {
        Some(path) => Some(fsutil::create_private_file_new(path)?),
        None => None,
    };
    let mut buf = vec![0u8; CHUNK];
    loop {
        if reading.cancelled() {
            return Err(Error::Cancelled);
        }
        let n = entry.read(&mut buf).map_err(|_| Error::BackupDamaged)?;
        if n == 0 {
            return Ok(());
        }
        if let Some(h) = hasher.as_mut() {
            h.update(&buf[..n]);
        }
        if let Some(out) = out.as_mut() {
            out.write_all(&buf[..n])?;
        }
        reading.report();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASS: &str = "a long enough passphrase";

    #[test]
    fn kept_counts_split_things_the_journal_and_files() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE items(id TEXT, kind TEXT, file_name TEXT, metadata TEXT);
             INSERT INTO items VALUES
               ('n', 'text', NULL, '{}'),
               ('p', 'image', 'p.jpg', '{}'),
               ('v', 'audio', 'v.m4a', '{}'),
               ('d', 'document', 'd.pdf', '{}'),
               ('e1', 'text', NULL, '{\"journal\":\"entry\"}'),
               ('e2', 'text', NULL, '{\"journal\":\"entry\",\"replaces\":\"e1\"}'),
               ('g', 'text', NULL, '{\"journal\":\"good\"}'),
               ('c', 'checkin', NULL, '{\"journal\":\"checkin\"}'),
               ('a', 'calendar', NULL, '{}'),
               ('r', 'reading', NULL, '{}');",
        )
        .unwrap();
        assert_eq!(
            kept_counts_in(&conn).unwrap(),
            KeptCounts {
                things: 4,
                journal_entries: 2,
                photos_and_recordings: 2,
            }
        );
    }

    fn manifest() -> Vec<u8> {
        serde_json::to_vec(&Manifest {
            format: FORMAT,
            store_version: store::latest_version(),
            app_version: "0.2.2".into(),
            made_at: "2026-11-15T09:00:00.000Z".into(),
            memories: 0,
            items: 1,
            pages: 0,
            originals: 1,
            online_used: false,
            away_used: false,
        })
        .unwrap()
    }

    /// A hand-written backup file and the entries it should read back as.
    fn hand_written(dir: &Path) -> (PathBuf, Written) {
        let file = dir.join("hand.nocturne");
        let manifest = manifest();
        let db = b"the database bytes".to_vec();
        let original = b"the kept original".to_vec();
        let sha = hex(&Sha256::digest(&original));
        let name = format!("store/originals/{sha}");
        let no_progress = |_: BackupPhase, _: u64, _: u64| {};
        let cancel = AtomicBool::new(false);
        let mut state = Counting {
            phase: BackupPhase::Saving,
            done: 0,
            total: 0,
            progress: &no_progress,
            cancel: &cancel,
        };
        let mut entries = Vec::new();
        {
            let out = std::fs::File::create(&file).unwrap();
            let encryptor =
                age::Encryptor::with_user_passphrase(SecretString::from(PASS.to_string()));
            let stream = encryptor.wrap_output(out).unwrap();
            let mut tar = tar::Builder::new(stream);
            entries.push((
                MANIFEST.to_string(),
                write_bytes(tar.get_mut(), MANIFEST, &manifest).unwrap(),
            ));
            entries.push((
                DB_ENTRY.to_string(),
                write_bytes(tar.get_mut(), DB_ENTRY, &db).unwrap(),
            ));
            entries.push((
                name.clone(),
                write_stream(
                    tar.get_mut(),
                    &name,
                    original.len() as u64,
                    &mut original.as_slice(),
                    &mut state,
                    "short",
                )
                .unwrap(),
            ));
            tar.finish().unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        (file, Written { entries, manifest })
    }

    #[test]
    fn a_database_being_checked_is_locked_down() {
        let conn = Connection::open_in_memory().unwrap();
        let trusted: i64 = conn
            .query_row("PRAGMA trusted_schema", [], |r| r.get(0))
            .unwrap();
        assert_eq!(trusted, 1, "the default this guards against");
        lock_down(&conn).unwrap();
        let trusted: i64 = conn
            .query_row("PRAGMA trusted_schema", [], |r| r.get(0))
            .unwrap();
        assert_eq!(trusted, 0);
        assert!(
            conn.db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE)
                .unwrap()
        );
        assert!(conn.execute_batch("PRAGMA writable_schema=ON").is_ok());
        conn.execute_batch("CREATE TABLE t(a)").unwrap();
        let _ = conn.execute_batch("UPDATE sqlite_master SET sql='x'");
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE sql='x'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "defensive mode refuses writes to the schema table");
    }

    fn is_locked_down(conn: &Connection) -> bool {
        let trusted: i64 = conn
            .query_row("PRAGMA trusted_schema", [], |r| r.get(0))
            .unwrap();
        trusted == 0
            && conn
                .db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE)
                .unwrap()
    }

    #[test]
    fn the_connection_that_checks_an_unpacked_database_is_locked_down() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nocturne.db");
        Connection::open(&path).unwrap();
        assert!(is_locked_down(&open_for_check(&path).unwrap()));
    }

    #[test]
    fn the_connection_that_migrates_and_scrubs_an_unpacked_database_is_locked_down() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nocturne.db");
        assert!(is_locked_down(&store::open_untrusted(&path).unwrap()));
        assert!(!is_locked_down(&store::open(&path).unwrap()));
    }

    fn silent(_: BackupPhase, _: u64, _: u64) {}

    #[test]
    fn a_file_that_reads_back_as_written_passes() {
        let dir = tempfile::tempdir().unwrap();
        let (file, written) = hand_written(dir.path());
        verify_written(&file, PASS, &written, &silent, &AtomicBool::new(false)).unwrap();
    }

    #[test]
    fn a_file_changed_after_it_was_written_fails_the_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let (file, written) = hand_written(dir.path());
        let mut bytes = std::fs::read(&file).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xff;
        let changed = dir.path().join("changed.nocturne");
        std::fs::write(&changed, bytes).unwrap();
        assert!(matches!(
            verify_written(&changed, PASS, &written, &silent, &AtomicBool::new(false)),
            Err(Error::BackupDamaged)
        ));
    }
}
