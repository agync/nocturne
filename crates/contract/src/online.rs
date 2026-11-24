//! Online answers: the last step of Ask written by a provider the person entered, through the
//! embedded engine. The key crosses the boundary once, inbound, in `OnlineForm`, and is never
//! returned in any of these types.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Where one question may be answered. `ThisComputer` is the per-question override.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AskPlace {
    #[default]
    Auto,
    ThisComputer,
}

/// Why a question was answered on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LocalReason {
    /// The person chose this computer for this question.
    Chosen,
    NoInternet,
    CouldntReach,
    Slow,
    Limit,
    NoCredit,
    KeyRefused,
    /// Nothing that may leave the computer was left to send.
    KeptHere,
    Problem,
    /// A download the person agreed to is under way; the engine runs without the route meanwhile.
    Downloading,
    /// Online answers was turned off, or a check of a provider began, while the question was on
    /// its way.
    TurnedOff,
    /// The question went online and the provider was not sure, while something that stays on
    /// this computer was kept back from it; this computer, which has it, answered instead. The
    /// sent log keeps what went out.
    NotSureOnline,
    /// The person agreed to Online answers before only what they mark private was held back,
    /// and has not yet seen the notice that says so.
    Changed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum AnsweredBy {
    Computer { reason: Option<LocalReason> },
    Online { host: String },
}

/// What stayed on this computer when an online answer was written. A private mark keeps nothing
/// back (decision 0011, amended 2026-11-23).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LeftOut {
    /// Part of the answer came from lines of originals, which never go online; this computer
    /// added them.
    Lines,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum OnlinePause {
    /// The month's limit is reached until `until` (RFC 3339, the 1st of next month, local).
    Limit {
        until: String,
    },
    KeyRefused,
    NoCredit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OnlineProblem {
    Unreachable,
    NoInternet,
    ModelMissing,
    KeyUnavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum OnlineState {
    Off,
    Checking,
    On,
    Paused { pause: OnlinePause },
    Problem { code: OnlineProblem, since: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LimitKind {
    Questions,
    Usd,
    None,
}

/// A monthly limit. There is none unless the person sets one; `questions` is the number offered
/// when they choose to count questions. `usd` is offered only when the provider reports cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OnlineLimit {
    pub kind: LimitKind,
    pub questions: u32,
    pub usd: Option<f64>,
}

impl Default for OnlineLimit {
    fn default() -> Self {
        Self {
            kind: LimitKind::None,
            questions: 200,
            usd: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum KeyStorage {
    Keychain,
    /// The owner-only file used when the system keychain is unavailable.
    File,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OnlineView {
    pub state: OnlineState,
    pub set_up: bool,
    /// The provider's host, shown as "your provider (host)".
    pub host: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub phones: bool,
    pub phones_available: bool,
    /// Online answers is on and the person agreed before only what they mark private was held
    /// back: the next question that would go online waits for the one-time notice.
    pub change_notice: bool,
    pub limit: OnlineLimit,
    pub questions_this_month: u32,
    pub spent_this_month_usd: Option<f64>,
    /// True when the provider reported cost on the last three answers.
    pub cost_reported: bool,
    /// When the month's count starts again (`YYYY-MM-DD`, local).
    pub resets_on: String,
    pub last_sent_at: Option<String>,
    pub on_since: Option<String>,
    pub last_on_at: Option<String>,
    pub sent_bytes: f64,
    pub key_storage: Option<KeyStorage>,
}

/// The connection form. Inbound only: it is never serialised, compared or copied, and the shell
/// turns `key` into a redacting key type at the command boundary.
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OnlineForm {
    pub base_url: String,
    pub key: String,
    pub model: String,
}

impl std::fmt::Debug for OnlineForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnlineForm")
            .field("base_url", &self.base_url)
            .field("key", &"[key]")
            .field("model", &self.model)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum CheckOutcome {
    Works {
        host: String,
        model: String,
    },
    Unreachable,
    KeyRefused,
    NoCredit,
    ModelMissing,
    NoInternet,
    NotHttps,
    /// The address names this computer, the home network or another place never sent to.
    AddressNotAllowed,
    /// Too many requests (429): busy, try later.
    Busy,
    /// The provider failed (5xx).
    ServerError,
    /// The provider answered with nothing.
    EmptyReply,
    Cancelled,
}

/// One look at the service's list of models, at the person's request: the names to choose
/// from, none, or why the look failed, worded as the check words it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ModelList {
    /// The names exactly as listed, in the service's order.
    Listed {
        names: Vec<String>,
    },
    /// The look worked and listed no names, or the address has no list.
    None,
    Failed {
        problem: CheckOutcome,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OnlineSettings {
    pub phones: bool,
    pub limit: OnlineLimit,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OnlineSettingsPatch {
    pub phones: Option<bool>,
    pub limit: Option<OnlineLimit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SendKind {
    Question,
    /// The one question of a connection check.
    Check,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum SendResult {
    Answered { receipts: u32 },
    NotSure,
    Failed { reason: LocalReason },
    NotSent { reason: LocalReason },
}

/// A sent memory line as it stands now against what was sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SentMemory {
    /// The memory still reads exactly as the line sent: `text` is that line.
    Same,
    /// The memory was re-read or edited since: the words sent are not kept.
    Changed,
    Deleted,
}

/// One line of what was sent, rebuilt from the sent log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum SentLine {
    Text {
        text: String,
    },
    /// The memory sent as `[mN]`. `text` is the line as sent, only while `state` is `Same`.
    Memory {
        n: u32,
        text: Option<String>,
        state: SentMemory,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OnlineSend {
    pub id: f64,
    pub at: String,
    pub kind: SendKind,
    pub host: String,
    pub model: String,
    pub question: String,
    pub lines: Vec<SentLine>,
    pub instructions: String,
    pub bytes_up: f64,
    pub bytes_down: f64,
    pub cost_usd: Option<f64>,
    pub result: SendResult,
    /// The request reached the provider: it was handed over a connection the provider accepted
    /// and a status came back. False when there was no internet, the address could not be
    /// reached, or the question was cut off or never left.
    pub reached: bool,
    /// The memory lines that came from the journal, by their `[mN]` number, with the journal
    /// day they were kept on: "from your journal, {day}".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub journal: Option<Vec<JournalSent>>,
}

/// A sent line read from a journal entry or a good thing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct JournalSent {
    pub n: u32,
    pub day: String,
}

/// Online answers in the advanced details view. Never carries the key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OnlineAdvanced {
    pub state: String,
    pub route: String,
    pub base_url: Option<String>,
    pub host: Option<String>,
    pub model: Option<String>,
    pub max_tokens: u32,
    pub key_storage: Option<KeyStorage>,
    pub last_at: Option<String>,
    pub last_status: Option<u16>,
    pub last_seconds: Option<f32>,
    pub last_bytes_up: Option<f64>,
    pub last_bytes_down: Option<f64>,
    pub cost_reported: bool,
    pub sends_kept: u32,
    pub questions_this_month: u32,
}
