//! The contract between the Nocturne engine, the app shell and the interface.
//!
//! Every type here crosses the Tauri boundary as camelCase JSON and is exported to TypeScript by
//! ts-rs (`cargo test -p nocturne-contract` writes `ui/src/engine/bindings/`). The engine returns
//! data, never interface copy: the interface owns every user-facing word except content (memory
//! sentences, page lines, answers), which comes from what the person kept.
//!
//! Timestamps are RFC 3339 strings with offset; dates are `YYYY-MM-DD`; sizes are bytes as `f64`
//! so they stay exact in JavaScript up to 2^53.

mod ask;
mod events;
mod item;
mod memory;
mod models;
mod online;
mod page;
mod remote;
mod system;

pub use ask::*;
pub use events::*;
pub use item::*;
pub use memory::*;
pub use models::*;
pub use online::*;
pub use page::*;
pub use remote::*;
pub use system::*;
