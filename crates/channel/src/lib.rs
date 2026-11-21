//! The sealed channel's pure steps, shared by the computer and the phone app.
//!
//! [`protocol`] holds the constants, [`crypto`] the keys, pairing, session and frame steps,
//! [`frame`] one session's channel with its strict counters, and [`phone`] the phone's side of
//! pairing and opening a session. None of those does I/O, so the crate builds for every target
//! the phone app runs on. The `client` feature adds the phone's networked client.

pub mod crypto;
pub mod frame;
pub mod phone;
pub mod protocol;

#[cfg(feature = "client")]
pub mod client;
