//! The sealed channel's pure steps, shared by the computer and the phone app.
//!
//! [`protocol`] holds the constants, [`crypto`] the keys, pairing, session and frame steps, and
//! [`frame`] one session's channel with its strict counters. Nothing here does I/O, so the crate
//! builds for every target the phone app runs on.

pub mod crypto;
pub mod frame;
pub mod protocol;
