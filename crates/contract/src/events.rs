//! Event names the app shell emits. The payload type for each is named beside it.

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
/// Payload: `Vec<String>`, paths dropped on the window or the tray.
pub const EVENT_DROPPED: &str = "files-dropped";

pub const ALL_EVENTS: [&str; 7] = [
    EVENT_STATUS,
    EVENT_ITEM,
    EVENT_SETUP,
    EVENT_ASK,
    EVENT_PAGE,
    EVENT_NEEDS_YOU,
    EVENT_DROPPED,
];
