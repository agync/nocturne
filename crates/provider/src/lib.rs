//! The Nocturne mind on an online provider the person chose, called by the device's own core:
//! the address rules a provider must pass ([`address`]), one client that reaches only its one
//! host ([`client`]), and the mind that reads, writes and answers through it ([`mind`]). The
//! words and the judging are `nocturne-mind`'s, so the computer's mind and this one say the same.
//! Nothing here is reached until the person turns it on and a [`client::Provider`] is made.

pub mod address;
pub mod client;
pub mod image;
pub mod mind;
#[cfg(feature = "stand-in")]
pub mod stand_in;

pub use client::{Exchange, Provider};
pub use mind::{Decode, Job, ProviderMind, Record, Unsent};
