//! Choosing the model for each job. Tiers stay the preset; a job may be given another catalogue
//! model that fits this computer.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Tier;

/// A job a model does. Maps to `ComponentRole`: Answering is Mind, Recordings is Speech, Aloud
/// is Voice, Finding is Recall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Job {
    Answering,
    Recordings,
    Aloud,
    Finding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Fit {
    Fits,
    Tight,
    WontFit,
    CantUse,
}

/// Why a choice does not fit, naming the one limiter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum FitReason {
    /// Memory: `limiter` is the other job whose model leaves too little room, if one does.
    Memory {
        limiter: Option<Job>,
    },
    Disk {
        short_bytes: f64,
    },
    NotAvailable,
    NeedsOtherJob {
        job: Job,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModelChoice {
    pub id: String,
    pub name: String,
    pub job: Job,
    pub size_bytes: f64,
    pub memory_bytes: f64,
    pub on_disk: bool,
    pub current: bool,
    pub recommended: bool,
    pub tested: bool,
    pub fit: Fit,
    pub reason: Option<FitReason>,
    pub smaller: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct JobView {
    pub job: Job,
    pub model: String,
    pub name: String,
    pub size_bytes: f64,
    pub memory_bytes: f64,
    pub recommended: bool,
    pub tested: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ChangePhase {
    Downloading {
        received: f64,
        total: f64,
        paused: bool,
    },
    Failed {
        received: f64,
        total: f64,
    },
    Ready,
}

/// A change of model for one job, under way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangeView {
    pub job: Job,
    pub model: String,
    pub from: String,
    pub phase: ChangePhase,
    /// True when the download carried on after a restart.
    pub resumed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ReindexPhase {
    Running,
    Done,
    Cancelled,
}

/// Memories being read again for a new finding model; the old index stays in use until done.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReindexView {
    pub model: String,
    pub from: String,
    pub done: u32,
    pub total: u32,
    pub eta_seconds: Option<u32>,
    pub phase: ReindexPhase,
}

/// The model a job used before a change, kept until the person removes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OldModelView {
    pub job: Job,
    pub model: String,
    pub bytes: f64,
    pub removed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModelsView {
    pub preset: Tier,
    pub custom: bool,
    pub ram_bytes: f64,
    pub budget_bytes: f64,
    pub busiest_bytes: f64,
    pub jobs: Vec<JobView>,
    pub change: Option<ChangeView>,
    pub reindex: Option<ReindexView>,
    pub old: Option<OldModelView>,
    /// True when a change needs the person (a failed download, for one).
    pub attention: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangePreview {
    pub job: Job,
    pub model: String,
    pub from: String,
    pub download_bytes: f64,
    pub free_now: f64,
    pub free_after: f64,
    pub memory_now: f64,
    pub memory_after: f64,
    pub fit: Fit,
    pub smaller: bool,
    pub tested: bool,
    pub reindex_items: Option<u32>,
    pub reindex_eta_seconds: Option<u32>,
    /// True when only memories kept from now on would use the new model.
    pub new_only: bool,
}
