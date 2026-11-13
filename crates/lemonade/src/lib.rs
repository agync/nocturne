//! Runs and supervises the private Lemonade server from the embedded bundle, sets it up for the
//! person's hardware and implements [`nocturne_engine::Mind`] over its HTTP API.

pub mod bundle;
pub mod client;
pub mod config;
mod error;
pub mod record;
pub mod supervisor;

pub use bundle::Dirs;
pub use client::Client;
pub use error::RuntimeError;
pub use supervisor::{RuntimeState, Supervisor, SupervisorOptions};
