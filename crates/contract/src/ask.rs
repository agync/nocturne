use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Receipt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AskPhase {
    /// The mind is starting; the first question after a while takes longer.
    Waking,
    Searching,
    Reading,
    Streaming,
    Done,
    NotSure,
    Nothing,
    /// The mind is not set up yet or not running.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NearMatch {
    pub text: String,
    pub receipt: Receipt,
}

/// One step of an answer. Deltas for one `askId` arrive in order and end with a terminal phase
/// (`done`, `notSure`, `nothing` or `unavailable`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AskDelta {
    pub ask_id: String,
    pub phase: AskPhase,
    pub text_delta: Option<String>,
    pub receipts: Vec<Receipt>,
    pub memory_ids: Vec<String>,
    pub near: Vec<NearMatch>,
    pub found: Option<u32>,
    pub eta_seconds: Option<u32>,
    /// Set on `done` when the bold spans in the streamed text cannot be matched one for one to
    /// `memory_ids`: the answer is to be shown without bold.
    #[serde(default)]
    pub plain: bool,
}
