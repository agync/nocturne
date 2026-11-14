//! Nocturne's intelligence layer.
//!
//! The engine never calls a mind. This crate does: one [`Reader`] holds the mind and reads what
//! was kept, in a fixed order of priority; the [`Tidy`] writes the morning page and the
//! [`Scheduler`] decides when, with catch-up for laptops that were asleep; the [`Host`] answers
//! the questions the policy needs (on mains power, away from the keyboard, about to sleep).
//!
//! Everything takes its time from the engine's clock and exposes `tick()`, so tests drive it with
//! a manual clock and a mutable fake host.

mod chunk;
mod events;
pub mod host;
mod reader;
pub mod reindex;
mod schedule;
mod scheduler;
mod tidy;

use std::sync::{Arc, RwLock};

use nocturne_engine::Mind;

pub use chunk::split_chunks;
#[cfg(any(test, feature = "fake"))]
pub use events::{Event, RecordingEvents};
pub use events::{Events, NullEvents};
#[cfg(any(test, feature = "fake"))]
pub use host::FakeHost;
pub use host::{Host, SleepGuard, SystemHost, probe};
pub use reader::{AskGuard, DrainHandle, DrainProgress, Outcome, Reader, ReaderStats, Step};
pub use schedule::{Coming, coming, target_date};
pub use scheduler::{SchedStep, Scheduler};
pub use tidy::{Tidy, TidyKind, TidyStep};

/// Where the app puts the mind once it is ready (and takes it away when it goes). Never held
/// across an await.
pub type MindSlot = Arc<RwLock<Option<Arc<dyn Mind>>>>;

/// An empty slot: no mind yet.
pub fn new_mind_slot() -> MindSlot {
    Arc::new(RwLock::new(None))
}

/// The mind in the slot, if any.
pub(crate) fn current_mind(slot: &MindSlot) -> Option<Arc<dyn Mind>> {
    slot.read().unwrap_or_else(|e| e.into_inner()).clone()
}
