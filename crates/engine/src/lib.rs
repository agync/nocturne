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

mod capture;
mod clock;
mod error;
mod fsutil;
mod mind;
mod rows;
mod types;
mod util;

pub mod store;

pub use capture::{MAX_AUDIO_MS, MAX_PAGES, kind_for_ext, trust_for};
pub use clock::{Clock, ManualClock, SystemClock};
pub use error::{Error, Result};
pub use mind::{AnswerMeta, BoxFuture, ExtractContext, Mind, MindError, Transcript, Understanding};
#[cfg(any(test, feature = "fake"))]
pub use mind::{FAKE_DIM, FakeMind, hash_embedding};
pub use types::*;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::{Connection, Transaction, TransactionBehavior};

const READERS: usize = 4;
const DB_FILE: &str = "nocturne.db";

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
            }),
        };
        // Items a worker was reading when the app last stopped go back in the queue.
        let now = util::fmt_ts(engine.clock().now());
        engine.write(|tx| capture::reset_expired_leases(tx, &now).map(|_| ()))?;
        Ok(engine)
    }

    /// The data directory this engine was opened on.
    pub fn dir(&self) -> &Path {
        &self.inner.dir
    }

    pub(crate) fn clock(&self) -> &dyn Clock {
        self.inner.clock.as_ref()
    }

    pub(crate) fn originals_dir(&self) -> PathBuf {
        self.inner.dir.join("originals")
    }

    pub(crate) fn db_path(&self) -> PathBuf {
        self.inner.dir.join(DB_FILE)
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

    /// Fold the write-ahead log into the store and truncate it, so nothing deleted lingers in the
    /// `-wal` file. A busy reader blocks the truncate, so retry briefly and warn on failure.
    pub(crate) fn checkpoint_truncate(&self) -> Result<()> {
        for attempt in 0..10 {
            let busy: i64 = self.write_raw(|conn| {
                Ok(conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))?)
            })?;
            if busy == 0 {
                store::restrict_store_files(&self.db_path());
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(20 * (attempt + 1)));
        }
        tracing::warn!("the write-ahead log could not be truncated because a reader is busy");
        Ok(())
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
