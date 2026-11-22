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
    /// The previous value when this memory replaced an older one ("was Wed").
    pub was: Option<String>,
    /// What the memory was first filed under, when the person moved it to another person,
    /// place or thing.
    pub moved_from: Option<String>,
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
    /// Names it had before the person renamed it, newest first.
    #[serde(default)]
    pub former_names: Vec<String>,
    /// The latest rename or merge that can still be taken back.
    #[serde(default)]
    pub last_change: Option<EntityChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ChangeKind {
    Rename,
    Merge,
    Refile,
}

/// A change the person made to how things are filed, which can be taken back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EntityChange {
    pub id: String,
    pub kind: ChangeKind,
    /// The name it had before a rename, or the name of what was merged in.
    pub from: String,
    pub at: String,
}

/// What a rename did. A name that another thing already has changes nothing and is offered as
/// a merge instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum RenameOutcome {
    Renamed {
        entity: EntityView,
        /// Absent when the name did not change.
        change_id: Option<String>,
    },
    Exists {
        entity: EntityView,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MergeOutcome {
    /// The one that remains.
    pub entity: EntityView,
    pub change_id: String,
    /// Memories that moved across.
    pub moved: u32,
}

/// Where one memory is to be filed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum RefileTarget {
    Existing { entity_id: String },
    New { name: String, kind: EntityKind },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Refiled {
    pub memory: MemoryView,
    pub entity: EntityView,
    pub change_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MemoryFilter {
    pub entity_id: Option<String>,
    /// Only memories that came from this item.
    pub item_id: Option<String>,
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
    /// "Which {name} is this?": a name in an entry that fits two people. Badge "Same name".
    SameName {
        question: crate::SameNameQuestion,
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
    /// Lines in reflections and recaps that cite what is deleted and will be taken out.
    #[serde(default)]
    #[ts(optional)]
    pub reflection_lines: Option<u32>,
    /// Of those, the lines in week, month and year recaps ("Your reflection on this day, and
    /// {count} lines in your recaps that cite it.").
    #[serde(default)]
    #[ts(optional)]
    pub recap_lines: Option<u32>,
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
