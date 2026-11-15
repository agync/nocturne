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
/// Payload: `ExportProgress`.
pub const EVENT_EXPORT: &str = "export-progress";
/// Payload: `BackupProgress`, while a backup is written or a backup file is checked.
pub const EVENT_BACKUP: &str = "backup-progress";
/// Payload: `RemoteView`.
pub const EVENT_REMOTE: &str = "remote-changed";
/// Payload: `PairingRequest`.
pub const EVENT_PAIRING: &str = "pairing-request";
/// Payload: `DeviceView`.
pub const EVENT_DEVICE: &str = "device-active";
/// Payload: `OnlineView`.
pub const EVENT_ONLINE: &str = "online-changed";
/// Payload: `ModelsView`.
pub const EVENT_MODELS: &str = "models-changed";

pub const ALL_EVENTS: [&str; 13] = [
    EVENT_STATUS,
    EVENT_ITEM,
    EVENT_SETUP,
    EVENT_ASK,
    EVENT_PAGE,
    EVENT_NEEDS_YOU,
    EVENT_EXPORT,
    EVENT_BACKUP,
    EVENT_REMOTE,
    EVENT_PAIRING,
    EVENT_DEVICE,
    EVENT_ONLINE,
    EVENT_MODELS,
];
