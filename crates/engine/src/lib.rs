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

mod clock;
mod error;
mod util;

pub mod store;

pub use clock::{Clock, ManualClock, SystemClock};
pub use error::{Error, Result};

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
        std::fs::create_dir_all(dir)?;
        std::fs::create_dir_all(dir.join("originals"))?;
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
