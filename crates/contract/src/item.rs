use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ItemKind {
    Text,
    Image,
    Audio,
    Document,
    Web,
}

/// Where an item came from. Decides its default trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Source {
    QuickCapture,
    Drop,
    Paste,
    PastedWeb,
    WatchedFolder,
    Screenshot,
    Recording,
    Agent,
    Setup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Trust {
    Trusted,
    Untrusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ItemStatus {
    Kept,
    Reading,
    Understood,
    PartlyUnderstood,
    CouldntRead,
    NeedsYou,
    Unconfirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CouldntReadReason {
    Unsupported,
    Corrupt,
    Locked,
    TooLong,
    Language,
    MindUnavailable,
}

/// Something the person kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ItemView {
    pub id: String,
    pub kind: ItemKind,
    /// A short title: the first line of a note, a file name, or what the mind called it.
    pub title: String,
    pub source: Source,
    pub trust: Trust,
    pub kept_at: String,
    pub status: ItemStatus,
    pub reason: Option<CouldntReadReason>,
    pub duration_ms: Option<u32>,
    pub pages: Option<u32>,
    pub file_name: Option<String>,
    pub has_original: bool,
    /// Absolute path of the stored original, for the interface to show through the asset
    /// protocol. `None` for typed notes and for originals that were forgotten.
    pub original_path: Option<String>,
    pub private: bool,
    pub memory_count: u32,
    pub understood_in_ms: Option<u32>,
    /// What the mind understood: a description of an image or document.
    pub summary: Option<String>,
    /// Transcript of audio, or the text itself for notes.
    pub text: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum Kept {
    New { item: ItemView },
    Duplicate { item: ItemView },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ItemFilter {
    /// Only items kept at or after this instant.
    pub since: Option<String>,
    pub status: Option<ItemStatus>,
    pub limit: Option<u32>,
}

/// Where in an original a memory came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum Span {
    Time { start_ms: u32, end_ms: u32 },
    Page { page: u32 },
    Region { x: f32, y: f32, w: f32, h: f32 },
    Quote { text: String },
}

/// The proof attached to every claim: which item, and where in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Receipt {
    pub item_id: String,
    pub kind: ItemKind,
    pub source: Source,
    /// The item title, for example "Appointment card" or "Bakery brief.pdf".
    pub label: String,
    pub kept_at: String,
    pub duration_ms: Option<u32>,
    pub span: Option<Span>,
    pub unconfirmed: bool,
}
