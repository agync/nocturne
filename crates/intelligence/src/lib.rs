//! Nocturne's intelligence layer.

pub mod host;

#[cfg(any(test, feature = "fake"))]
pub use host::FakeHost;
pub use host::{Host, SleepGuard, SystemHost, probe};
