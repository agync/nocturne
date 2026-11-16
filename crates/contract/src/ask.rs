use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{AnsweredBy, HelpView, LeftOut, Receipt};

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
    /// The answer is being written online.
    AskingOnline,
    /// The online answer is taking a while; the person may switch to this computer.
    WaitingOnline,
    /// The online answer did not come; this computer is answering instead.
    SwitchingToComputer,
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
    /// Set on the terminal delta: where the answer was written, and why on this computer.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub answered_by: Option<AnsweredBy>,
    /// Set when remembered things were kept back from an online question.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub left_out: Option<LeftOut>,
    /// The sent log entry for an online question, for "See what was sent".
    #[serde(default)]
    #[ts(optional = nullable)]
    pub send_id: Option<f64>,
    /// Set on the terminal delta when the Nocturne guide answers, or may answer, the question.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub help: Option<Box<HelpView>>,
}
