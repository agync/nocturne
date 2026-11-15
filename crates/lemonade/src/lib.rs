//! Runs and supervises the private Lemonade server from the embedded bundle, sets it up for the
//! person's hardware and implements [`nocturne_engine::Mind`] over its HTTP API.

pub mod admin;
pub mod bundle;
pub mod client;
pub mod config;
mod error;
pub mod fit;
pub mod fit_table;
pub mod graphics;
pub mod jobs;
pub mod logpipe;
pub mod mind;
pub mod npu;
pub mod online;
pub mod process;
pub mod record;
pub mod rerank;
pub mod residency;
pub mod selection;
pub mod setup;
pub mod supervisor;
pub mod tier;

pub use admin::Admin;
pub use bundle::Dirs;
pub use client::Client;
pub use error::RuntimeError;
pub use mind::LemonadeMind;
pub use setup::{Setup, SetupOptions};
pub use supervisor::{Control, OnlineHold, RuntimeState, Supervisor, SupervisorOptions};
pub use tier::{Component, components, detect, tier_options};
