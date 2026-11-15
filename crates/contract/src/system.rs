use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{OnlineAdvanced, RemoteAdvanced, RemoteConnection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum EngineState {
    Starting,
    GettingReady,
    Resting,
    Reading,
    Tidying,
    NeedsYou,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TidyProgress {
    pub done: u32,
    pub total: u32,
    pub eta_seconds: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EngineStatus {
    pub state: EngineState,
    pub needs_you: u32,
    pub waiting_to_read: u32,
    /// Bytes sent to the internet since setup, other than downloads in the ledger.
    pub sent_to_internet_bytes: f64,
    /// False when outbound traffic cannot be measured on this system.
    pub sent_measured: bool,
    pub paused_until: Option<String>,
    pub tidy: Option<TidyProgress>,
    /// When the next overnight tidy is expected (`HH:MM` local).
    pub next_tidy: Option<String>,
    pub notices: Vec<Notice>,
}

/// A problem or event the interface explains in one sentence and one button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum NoticeCode {
    DownloadStalled,
    DiskFull,
    TierDropped,
    TierUnfinished,
    Restarts,
    CouldntRead,
    MicDenied,
    PrintFailed,
    CatchingUp,
    RepairNeeded,
    NotReady,
    TidyPausedBattery,
    UpdateRolledBack,
    ReReading,
    /// The online provider refused the key; Online answers is paused.
    OnlineKeyRefused,
    /// The online provider says the account has no credit; Online answers is paused.
    OnlineNoCredit,
    /// The month's online limit is reached.
    OnlineLimit,
    /// A model download for a job stopped before it finished.
    ModelDownloadStopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Notice {
    pub id: String,
    pub code: NoticeCode,
    pub item_id: Option<String>,
    pub eta_seconds: Option<u32>,
    pub bytes: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Tier {
    Large,
    Standard,
    Small,
    Tiny,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TierOption {
    pub tier: Tier,
    pub download_bytes: f64,
    pub min_ram_gib: u32,
    pub fits: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SetupPhase {
    NotStarted,
    Checking,
    Choosing,
    Downloading,
    Paused,
    Installing,
    Done,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PauseReason {
    Person,
    Offline,
    Battery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SetupError {
    Offline,
    DiskFull,
    NotEnoughMemory,
    EngineFailed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SetupView {
    pub phase: SetupPhase,
    pub detected: Option<Tier>,
    pub chosen: Option<Tier>,
    pub ram_gib: u32,
    pub free_disk_bytes: f64,
    pub options: Vec<TierOption>,
    pub downloaded_bytes: f64,
    pub total_bytes: f64,
    pub eta_seconds: Option<u32>,
    pub paused: Option<PauseReason>,
    pub error: Option<SetupError>,
    /// Extra free space needed when `error` is `diskFull`.
    pub needed_bytes: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Delivery {
    Notify,
    Quiet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LettingGo {
    Gentle,
    Normal,
    Brisk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SettingsView {
    /// `HH:MM`, local time.
    pub page_time: String,
    pub delivery: Delivery,
    pub auto_print: bool,
    pub voice: String,
    pub voice_speed: f32,
    pub shortcut: String,
    pub shortcut_available: bool,
    pub inbox_folder: Option<String>,
    pub start_at_login: bool,
    pub letting_go: LettingGo,
    pub theme: Theme,
    pub language: String,
    pub check_updates: bool,
    pub reminders: bool,
    pub lock: bool,
    pub agents: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SettingsPatch {
    pub page_time: Option<String>,
    pub delivery: Option<Delivery>,
    pub auto_print: Option<bool>,
    pub voice: Option<String>,
    pub voice_speed: Option<f32>,
    pub shortcut: Option<String>,
    pub inbox_folder: Option<String>,
    pub start_at_login: Option<bool>,
    pub letting_go: Option<LettingGo>,
    pub theme: Option<Theme>,
    pub language: Option<String>,
    pub check_updates: Option<bool>,
    pub reminders: Option<bool>,
    pub lock: Option<bool>,
    pub agents: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LedgerKind {
    Setup,
    AppUpdate,
    MindUpdate,
    UpdateCheck,
    /// A connection helper downloaded for Away from home.
    RemoteHelper,
    /// Away from home turned on or off.
    RemoteOnOff,
    /// Online answers turned on or off.
    OnlineOnOff,
    /// A connection check reached the person's provider.
    OnlineConnected,
    /// A model downloaded for a job the person changed.
    ModelDownload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LedgerEntry {
    pub kind: LedgerKind,
    pub label: String,
    pub bytes: f64,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrivacyView {
    pub sent_to_internet_bytes: f64,
    pub sent_measured: bool,
    pub checked_at: Option<String>,
    pub ledger: Vec<LedgerEntry>,
    pub kept_bytes: f64,
    pub mind_bytes: f64,
    pub data_dir: String,
    pub lock: bool,
    pub check_updates: bool,
    pub sent_to_phones_bytes: f64,
    pub remote_on: bool,
    pub remote_connection: Option<RemoteConnection>,
    pub remote_last_on_at: Option<String>,
    pub sent_online_bytes: f64,
    pub online_questions: u32,
    pub online_on: bool,
    pub online_host: Option<String>,
    pub online_last_on_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ComponentRole {
    Mind,
    Speech,
    Voice,
    Recall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ComponentView {
    pub role: ComponentRole,
    pub name: String,
    pub bytes: f64,
    pub backend: Option<String>,
    pub loaded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TidyReport {
    pub from: String,
    pub to: String,
    pub items: u32,
    pub merged: u32,
    pub let_go: u32,
}

/// Why the NPU does not read recordings, grounded in what the engine reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum NpuCause {
    /// The NPU is there but its kernel driver is not.
    DriverMissing,
    /// The NPU driver is too old.
    DriverOld,
    /// The kernel is too old for the NPU.
    KernelOld,
    /// The NPU firmware is too old.
    FirmwareOld,
    /// The memory lock limit is too low.
    MemoryLock,
    /// The engine does not support this NPU.
    NotSupported,
    /// The NPU model does not fit the memory budget beside the other jobs.
    TooLittleMemory,
    /// The NPU runtime or model is not downloaded yet.
    NotFetched,
    /// The person chose a model for reading recordings.
    Chosen,
    /// The NPU failed for another reason; `detail` holds the engine's words.
    Failed,
}

impl NpuCause {
    /// A cause the person can put right on this computer.
    pub fn fixable(self) -> bool {
        matches!(
            self,
            NpuCause::DriverMissing
                | NpuCause::DriverOld
                | NpuCause::KernelOld
                | NpuCause::FirmwareOld
                | NpuCause::MemoryLock
        )
    }
}

/// The NPU as Nocturne uses it. Shown only in the advanced view, except that a fixable cause may
/// raise a quiet note.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NpuView {
    /// This computer has an NPU the engine can name, or one whose driver is missing.
    pub present: bool,
    /// Recordings are read on it.
    pub used: bool,
    /// The model that reads recordings there, when used.
    pub model: Option<String>,
    /// Why it is not used, empty when it is or when there is none.
    pub causes: Vec<NpuCause>,
    /// The engine's own words for the last failure, for the advanced view.
    pub detail: Option<String>,
}

/// The only view that may show technical words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdvancedView {
    pub tier: Option<Tier>,
    pub ram_gib: u32,
    pub gpu: Option<String>,
    pub npu: bool,
    pub components: Vec<ComponentView>,
    pub engine_version: Option<String>,
    pub engine_port: Option<u16>,
    pub engine_uptime_seconds: Option<u32>,
    pub restarts: u32,
    pub store_version: u32,
    pub memory_count: u32,
    pub item_count: u32,
    pub integrity_ok: bool,
    pub last_tidy: Option<TidyReport>,
    pub agents: bool,
    pub log_dir: String,
    pub remote: Option<RemoteAdvanced>,
    pub online: Option<OnlineAdvanced>,
}

/// What an export wrote. `path` is the fresh folder created inside the chosen directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportReport {
    pub path: String,
    pub memories: u32,
    pub pages: u32,
    pub originals: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportProgress {
    pub done: u32,
    pub total: u32,
}
