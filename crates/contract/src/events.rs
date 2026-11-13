//! Event names the app shell emits. The payload type for each is named beside it.
//!
//! File drops have one owner per source: the interface keeps drops on its window through the
//! platform adapter; the app shell keeps drops on the tray and paths forwarded by a second
//! launch itself, so no drop event crosses the boundary.

/// Payload: `EngineStatus`.
pub const EVENT_STATUS: &str = "status-changed";
/// Payload: `ItemView`.
pub const EVENT_ITEM: &str = "item-changed";
/// Payload: `SetupView`.
pub const EVENT_SETUP: &str = "setup-progress";
/// Payload: `AskDelta`.
pub const EVENT_ASK: &str = "ask-delta";
/// Payload: `PageSummary`.
pub const EVENT_PAGE: &str = "page-ready";
/// Payload: `u32`, the number of things waiting on the person.
pub const EVENT_NEEDS_YOU: &str = "needs-you-changed";

pub const ALL_EVENTS: [&str; 6] = [
    EVENT_STATUS,
    EVENT_ITEM,
    EVENT_SETUP,
    EVENT_ASK,
    EVENT_PAGE,
    EVENT_NEEDS_YOU,
];
