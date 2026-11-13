use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Receipt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Certainty {
    Sure,
    FairlySure,
    NotSure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MemoryStatus {
    Active,
    Fading,
    LetGo,
    Unconfirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum EntityKind {
    Person,
    Place,
    Thing,
}

/// Something Nocturne remembers, as one plain sentence with its receipts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MemoryView {
    pub id: String,
    pub sentence: String,
    pub subject: String,
    pub entity_id: Option<String>,
    pub predicate: String,
    pub object: String,
    pub certainty: Certainty,
    pub status: MemoryStatus,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub due: Option<String>,
    pub learned_at: String,
    pub last_used: Option<String>,
    pub let_go_at: Option<String>,
    pub keep_forever: bool,
    pub private: bool,
    /// Health, money or ID content: the interface may suggest marking it private.
    pub suggest_private: bool,
    /// The previous value when this memory replaced an older one ("was Wed").
    pub was: Option<String>,
    pub receipts: Vec<Receipt>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EntityView {
    pub id: String,
    pub kind: EntityKind,
    pub name: String,
    /// A short role, for example "Son" or "Leo's teacher".
    pub role: Option<String>,
    pub memory_count: u32,
    pub item_count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MemoryFilter {
    pub entity_id: Option<String>,
    pub status: Option<MemoryStatus>,
    pub query: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WhichOption {
    pub memory_id: String,
    pub text: String,
    pub receipt: Receipt,
}

/// One thing waiting on the person.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum NeedsYouEntry {
    Which {
        conflict_id: String,
        title: String,
        options: Vec<WhichOption>,
    },
    Unconfirmed {
        memory: MemoryView,
        agreeing_sources: u32,
        used: bool,
    },
    Due {
        memory: MemoryView,
        due: String,
        done: bool,
    },
    Promise {
        memory: MemoryView,
        done: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgetRow {
    pub memory_id: String,
    pub sentence: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgetOriginal {
    pub item_id: String,
    pub label: String,
    pub kind: crate::ItemKind,
    pub kept_at: String,
    pub duration_ms: Option<u32>,
    /// False when other memories still come from this original.
    pub removable: bool,
    pub other_memories: u32,
}

/// Exactly what a forget will remove. Executing needs the token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgetPreview {
    pub token: String,
    pub memories: Vec<ForgetRow>,
    pub originals: Vec<ForgetOriginal>,
    pub include_originals: bool,
    pub pages_affected: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgetReport {
    pub memories_removed: u32,
    pub originals_removed: u32,
    pub pages_redacted: u32,
}

/// What `resolve_which` settled, so the interface can say "Dentist set to Thursday 10:00."
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WhichResolved {
    /// The short subject, for example "Dentist".
    pub subject: String,
    /// The chosen value as written in the page line, for example "Thursday 19 Nov, 10:00".
    pub chosen: String,
    pub memory_id: String,
}
