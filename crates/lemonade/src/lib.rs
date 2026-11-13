//! Runs and supervises the private Lemonade server from the embedded bundle, sets it up for the
//! person's hardware and implements [`nocturne_engine::Mind`] over its HTTP API.

pub mod bundle;
pub mod client;
pub mod config;
mod error;
pub mod mind;
pub mod record;
pub mod residency;
pub mod setup;
pub mod supervisor;
pub mod tier;

pub use bundle::Dirs;
pub use client::Client;
pub use error::RuntimeError;
pub use mind::LemonadeMind;
pub use setup::{Setup, SetupOptions};
pub use supervisor::{Control, RuntimeState, Supervisor, SupervisorOptions};
pub use tier::{Component, components, detect, tier_options};
