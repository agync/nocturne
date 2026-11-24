//! The Nocturne memory engine.
//!
//! One SQLite file, no AI dependency. The engine keeps what arrives, holds memories with their
//! history and receipts, applies the forgetting policy, forgets completely on request, composes the
//! morning page and finds memories again. Everything the AI does reaches it through the [`Mind`]
//! trait, which the engine never calls itself: the caller reads items, asks the mind, and hands
//! the results back.
//!
//! Every public method is synchronous. Callers on an async runtime use `spawn_blocking`.
//! Policy (decay, ranking, windows) takes its time from the [`Clock`], so tests control it.

mod admin;
mod audio;
mod backup;
mod capture;
mod carried;
mod clock;
mod entity;
mod error;
mod export;
mod find;
mod forget;
mod fsutil;
pub mod guide;
mod journal;
mod lifecycle;
mod memory;
mod merge;
mod mind;
mod online;
mod page;
mod phones;
mod reindex;
mod remote;
mod retrieve;
mod rows;
mod settings;
mod types;
mod util;

pub mod coverage;
pub mod day;
pub mod decide;
pub mod private;
pub mod process;
pub mod store;

pub use backup::{
    BACKUP_EXT, BackupRun, LeftBehind, MIN_PASSPHRASE_CHARS, check_backup, check_backup_counted,
    probe_backup, store_has_data, store_kept_counts, store_left_behind, unpack_backup,
};
pub use capture::{MAX_AUDIO_MS, MAX_FAILURES, MAX_PAGES, kind_for_ext, text_title, trust_for};
pub use clock::{Clock, ManualClock, SystemClock};
pub use error::{Error, Result};
pub use export::DELETE_PHRASE;
pub use find::{ExcerptPart, FIND_CAP, FoundItem};
pub use guide::{Guide, GuideMatch, GuideSource, is_guide_section};
pub use journal::keep::KeepPlace;
pub use merge::BroughtIn;
pub use mind::{
    AnswerMeta, BoxFuture, ExtractContext, Fact, Mind, MindError, OnlineFailure, OnlineReceipt,
    ReflectInput, ReflectItem, Reflected, Transcript, Understanding, WrittenSentence,
};
#[cfg(any(test, feature = "fake"))]
pub use mind::{FAKE_DIM, FakeMind, Gate, hash_embedding};
pub use online::{NewSend, OnlineRow};
pub use page::{HEAVY_ITEMS, PAGE_LINE_CAP, needs_you_count, phone_view};
pub use phones::read::CALENDAR_TODAY_ROWS;
pub use phones::{CALENDAR_PER_SEND, PhoneBuilds, READINGS_PER_SEND, usual};
pub use reindex::ReindexRow;
pub use remote::{DeviceRow, RemoteRow};
pub use retrieve::{SOURCE_LINE, is_source_line, reranked};
pub use types::*;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use rusqlite::{Connection, Transaction, TransactionBehavior};

const READERS: usize = 4;
const DB_FILE: &str = "nocturne.db";
const TRUNCATE_PENDING: &str = "_truncatePending";
/// The finding model every tier uses until the person chooses another; vectors stored before
/// migration 10 were all made by it.
pub const DEFAULT_RECALL_MODEL: &str = "Qwen3-Embedding-0.6B-GGUF";

/// The engine. Cheap to clone; all clones share one store.
#[derive(Clone)]
pub struct Engine {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub(crate) dir: PathBuf,
    pub(crate) clock: Arc<dyn Clock>,
    writer: Mutex<Connection>,
    readers: Vec<Mutex<Connection>>,
    next_reader: AtomicUsize,
    pub(crate) vectors: RwLock<Option<Arc<retrieve::VecIndex>>>,
    /// Bumped whenever the vector index may be stale; a rebuilt index is installed only if it
    /// did not move while it was being built.
    pub(crate) vec_gen: AtomicU64,
    /// Bumped by anything that can change what a spoken edition should say.
    pub(crate) audio_gen: AtomicU64,
    /// Bumped when a forget or an undone keep begins: an export that ran across one is void.
    pub(crate) forget_gen: AtomicU64,
    /// The finding model new vectors are made with, written beside each one.
    pub(crate) recall_model: RwLock<String>,
}

impl Engine {
    /// Open (creating if needed) the store in `dir`.
    pub fn open(dir: &Path, clock: Arc<dyn Clock>) -> Result<Engine> {
        fsutil::create_private_dir(dir)?;
        fsutil::create_private_dir(&dir.join("originals"))?;
        let db = dir.join(DB_FILE);
        let writer = store::open(&db)?;
        let mut readers = Vec::with_capacity(READERS);
        for _ in 0..READERS {
            readers.push(Mutex::new(store::open_reader(&db)?));
        }
        let engine = Engine {
            inner: Arc::new(Inner {
                dir: dir.to_path_buf(),
                clock,
                writer: Mutex::new(writer),
                readers,
                next_reader: AtomicUsize::new(0),
                vectors: RwLock::new(None),
                vec_gen: AtomicU64::new(0),
                audio_gen: AtomicU64::new(0),
                forget_gen: AtomicU64::new(0),
                recall_model: RwLock::new(DEFAULT_RECALL_MODEL.to_string()),
            }),
        };
        // Items a worker was reading when the app last stopped go back where they were.
        engine.write(|tx| capture::reset_all_leases(tx).map(|_| ()))?;
        // A forget that was under way when the app stopped is finished before anything else.
        engine.resume_forgets()?;
        // Files with no row (a crash between a commit and the removal, or a half-copied
        // original) are shredded, and a log that could not be truncated last time is retried.
        engine.sweep_orphan_files()?;
        backup::remove_leftover_scratch(&engine.inner.dir);
        engine.retry_scrub()?;
        Ok(engine)
    }

    /// The data directory this engine was opened on.
    pub fn dir(&self) -> &Path {
        &self.inner.dir
    }

    /// The engine's current instant, from its clock.
    pub fn clock_now(&self) -> jiff::Timestamp {
        self.inner.clock.now()
    }

    /// The clock the engine takes its time from.
    pub fn clock(&self) -> &dyn Clock {
        self.inner.clock.as_ref()
    }

    /// Today's local date, as stored text.
    pub(crate) fn today(&self) -> String {
        self.inner
            .clock
            .now()
            .to_zoned_datetime(self.inner.clock.tz())
            .date()
            .to_string()
    }

    pub(crate) fn originals_dir(&self) -> PathBuf {
        self.inner.dir.join("originals")
    }

    pub(crate) fn db_path(&self) -> PathBuf {
        self.inner.dir.join(DB_FILE)
    }

    /// Drop the in-memory vector index; the next search rebuilds it.
    pub(crate) fn invalidate_vectors(&self) {
        self.inner.vec_gen.fetch_add(1, Ordering::SeqCst);
        *self
            .inner
            .vectors
            .write()
            .unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Run `f` in one write transaction on the single writer connection.
    pub(crate) fn write<T>(&self, f: impl FnOnce(&Transaction<'_>) -> Result<T>) -> Result<T> {
        let mut guard = lock(&self.inner.writer);
        let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    /// Run `f` on the writer connection without a transaction (pragmas, checkpoints).
    pub(crate) fn write_raw<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let guard = lock(&self.inner.writer);
        f(&guard)
    }

    /// How many forgets and undone keeps have begun since the store was opened: a caller that
    /// holds words from the store can tell whether any may have gone meanwhile.
    pub fn forget_generation(&self) -> u64 {
        self.inner.forget_gen.load(Ordering::SeqCst)
    }

    /// Note that a forget or an undone keep has begun (see `forget_gen`).
    pub(crate) fn bump_forget(&self) {
        self.inner.forget_gen.fetch_add(1, Ordering::SeqCst);
    }

    /// Mark spoken editions as possibly stale (see `audio_stamp`).
    pub(crate) fn bump_audio(&self) {
        self.inner.audio_gen.fetch_add(1, Ordering::SeqCst);
    }

    /// Fold the write-ahead log into the store and truncate it, so nothing deleted lingers in the
    /// `-wal` file. A busy reader blocks the truncate (no waiting inside SQLite: the retries
    /// here are short and the writer is free in between). Returns whether the log is now empty;
    /// if not, a `truncate pending` mark is stored and the next open, forget or tidy retries.
    pub(crate) fn checkpoint_truncate(&self) -> Result<bool> {
        // Clear the mark first (only if there is one), so the write it takes lands before the
        // truncate and not after it.
        self.write(|tx| {
            tx.execute("DELETE FROM settings WHERE key = ?1", [TRUNCATE_PENDING])?;
            Ok(())
        })?;
        let mut done = false;
        for attempt in 0..10 {
            let busy: i64 = self.write_raw(|conn| {
                conn.busy_timeout(std::time::Duration::ZERO)?;
                let r = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0));
                conn.busy_timeout(std::time::Duration::from_millis(5000))?;
                Ok(r?)
            })?;
            if busy == 0 {
                done = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20 * (attempt + 1)));
        }
        if done {
            store::restrict_store_files(&self.db_path());
        } else {
            tracing::warn!("the write-ahead log could not be truncated because a reader is busy");
            self.write(|tx| {
                tx.execute(
                    "INSERT OR REPLACE INTO settings(key, value) VALUES (?1, 'true')",
                    [TRUNCATE_PENDING],
                )?;
                Ok(())
            })?;
        }
        Ok(done)
    }

    /// Retry a truncate that could not finish earlier (for example after the next overnight
    /// tidy). Returns true when nothing is pending any more.
    pub fn retry_scrub(&self) -> Result<bool> {
        let pending: bool = self.read(|conn| {
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM settings WHERE key = ?1)",
                [TRUNCATE_PENDING],
                |r| r.get(0),
            )?)
        })?;
        if pending {
            self.checkpoint_truncate()
        } else {
            Ok(true)
        }
    }

    /// Run `f` on a read-only connection from the pool.
    pub(crate) fn read<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let n = self.inner.readers.len();
        let start = self.inner.next_reader.fetch_add(1, Ordering::Relaxed);
        for i in 0..n {
            if let Ok(guard) = self.inner.readers[(start + i) % n].try_lock() {
                return f(&guard);
            }
        }
        let guard = lock(&self.inner.readers[start % n]);
        f(&guard)
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
