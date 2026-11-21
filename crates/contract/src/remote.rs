//! Away from home: reaching Nocturne from the person's own paired phones.
//!
//! The first group is what the desktop windows see. The phone-facing group crosses only the
//! sealed channel between the computer and a paired phone; it is exported so the phone app
//! shares the same names.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    AskDelta, AskPlace, CalendarBatch, CheckIn, CheckInInput, Feeling, ItemKind, JournalDay,
    PageSummary, PageView, Period, PhoneFeature, QuestionChoice, ReadingBatch, ReflectionView,
    SameNameAnswer, SameNameAnswered, SyncReceipt,
};

/// How phones reach the computer: Simple goes through Cloudflare, Most private through the
/// person's own Tailscale account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RemoteConnection {
    Simple,
    MostPrivate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AutoOff {
    AfterDay,
    AfterWeek,
    Unused30Days,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SignInStep {
    SignIn,
    /// The account must allow secure links (HTTPS) once.
    AllowLinks,
    /// The account's owner must approve this computer before it may join.
    Approve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RemoteProblem {
    NoInternet,
    DownloadFailed,
    CheckFailed,
    HelperFailed,
    AccountBlocked,
    Unsupported,
}

/// A QR code as a square module matrix: `modules` is row-major, `'1'` dark and `'0'` light.
/// The interface draws it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QrCode {
    pub size: u32,
    pub modules: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum RemoteState {
    Off,
    Downloading {
        received_bytes: f64,
        total_bytes: f64,
        paused: bool,
    },
    Starting,
    NeedsSignIn {
        url: String,
        step: SignInStep,
    },
    On {
        address: String,
        machine_name: Option<String>,
    },
    NeedsRelink {
        address: String,
        pairing_link: String,
        qr: QrCode,
        words: String,
    },
    Problem {
        code: RemoteProblem,
        attempts: u32,
        since: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DeviceView {
    pub id: String,
    pub name: String,
    pub paired_at: String,
    pub last_used_at: Option<String>,
    pub active: bool,
    pub needs_address: bool,
}

/// A phone asking to be added. `check_phrase` holds the three words both screens show, for
/// the scanned and the typed code alike.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PairingRequest {
    pub pairing_id: String,
    pub device_name: String,
    pub check_phrase: Vec<String>,
    pub decide_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum PairingPhase {
    WaitingForScan,
    WaitingForYou {
        request: PairingRequest,
    },
    Allowed {
        device_name: String,
    },
    Refused,
    TimedOut,
    Expired,
    /// A second commit, or a reveal too late, used the code up.
    Spent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PairingStatus {
    pub pairing_id: String,
    pub phase: PairingPhase,
    pub expires_at: String,
}

/// One open offer to add a phone. `typed_code` carries the same secret as `link`: 24
/// characters, shown as four groups of six.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PairingOffer {
    pub pairing_id: String,
    pub link: String,
    pub qr: QrCode,
    pub typed_address: String,
    pub typed_code: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoteView {
    pub state: RemoteState,
    pub connection: RemoteConnection,
    pub auto_off: AutoOff,
    pub keep_awake: bool,
    pub devices: Vec<DeviceView>,
    pub sent_to_phones_bytes: f64,
    pub last_on_at: Option<String>,
    pub on_since: Option<String>,
    pub set_up_before: bool,
    pub installed: Vec<RemoteConnection>,
    pub trial: bool,
    pub pairing: Option<PairingStatus>,
    pub computer_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RemoteAction {
    Asked,
    KeptText,
    KeptPhoto,
    KeptDocument,
    KeptVoiceNote,
    AnsweredName,
    AnsweredQuestion,
    /// Counts only, never what the entries said.
    SentCalendar,
    SentReadings,
    TookBack,
    ReadPage,
    OpenedOriginal,
    TookNewAddress,
    Allowed,
    Refused,
    Removed,
    Renamed,
    Disconnected,
    WrongCode,
    Limited,
    TurnedOn,
    TurnedOff,
    SwitchedConnection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RemoteResult {
    Ok,
    Refused,
    Limited,
    Failed,
}

/// One line of the activity list: what happened, never what it contained.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoteEvent {
    pub device_name: Option<String>,
    pub action: RemoteAction,
    pub at: String,
    pub result: RemoteResult,
    pub count: u32,
}

/// Changes to Away from home's settings. A new `connection` unpairs every phone.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoteSettingsPatch {
    pub connection: Option<RemoteConnection>,
    pub auto_off: Option<AutoOff>,
    pub keep_awake: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LastContact {
    pub device_name: String,
    pub at: String,
    pub session_renewed_at: Option<String>,
}

/// Away from home in the advanced details view, the only place its technical words appear.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoteAdvanced {
    pub connection: RemoteConnection,
    pub helper_name: Option<String>,
    pub helper_version: Option<String>,
    pub helper_checked: bool,
    pub helper_uptime_seconds: Option<u32>,
    pub helper_restarts: u32,
    pub address: Option<String>,
    pub address_since: Option<String>,
    /// For example "unix socket, owner only" or "127.0.0.1:<port>, helper key only".
    pub listener: Option<String>,
    pub route: Option<String>,
    pub machine_name: Option<String>,
    pub last_contact: Option<LastContact>,
    pub helper_estimate_bytes_per_day: Option<f64>,
    pub audit_events: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PhoneReadiness {
    Ready,
    GettingReady,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhoneHello {
    pub computer_name: String,
    pub device_name: String,
    pub connection: RemoteConnection,
    pub version: String,
    // The oldest phone build the computer accepts; an older phone must ask for a fresh link.
    pub min_phone_build: u32,
    /// What this computer takes from a phone app. A computer from before phone apps sends none.
    #[serde(default)]
    #[ts(as = "Option<Vec<PhoneFeature>>", optional)]
    pub features: Vec<PhoneFeature>,
}

/// What a phone may know of the computer's state. `needs_you` counts the public needs-you
/// lines of the page at `page_date`, the same count as that page's phone summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhoneStatus {
    pub readiness: PhoneReadiness,
    pub page_date: Option<String>,
    pub needs_you: u32,
}

/// An item as a phone sees it. Never carries a path. `summary` and `text` are cut to
/// `MAX_ITEM_TEXT` bytes each (at a character boundary) so the item fits in one frame;
/// `truncated` says whether either was cut.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhoneItem {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    pub kept_at: String,
    pub duration_ms: Option<u32>,
    pub pages: Option<u32>,
    pub has_original: bool,
    pub summary: Option<String>,
    pub text: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhoneKept {
    pub item: PhoneItem,
    pub duplicate: bool,
    pub take_back_until: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PhoneError {
    Invalid,
    TooLarge,
    TooMany,
    TooLate,
    NotYours,
    NotFound,
    NotReady,
    Paused,
    Busy,
    Revoked,
    SessionExpired,
    /// The question would go online, but the person has not yet seen that Online answers now
    /// holds back only what they mark private. Nothing was started; the phone shows the notice.
    OnlineChanged,
}

/// Everything a phone may ask for: the allowed calls, the journal calls, the two phone verbs
/// and the two sends of a phone app.
///
/// A keep may carry a feeling and a good-thing mark; both are optional and default to nothing,
/// so a phone built before the journal still parses and keeps as it always did. It may also say
/// when it was made (`captured_at`, RFC 3339): a keep that waited on the phone belongs to the
/// day it was made, not the day it arrived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "cmd", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum RemoteCall {
    GetStatus,
    KeepText {
        text: String,
        client_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        feeling: Option<Feeling>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        good: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        captured_at: Option<String>,
    },
    KeepBytes {
        mime: String,
        size: f64,
        client_id: String,
        caption: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        feeling: Option<Feeling>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        good: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        captured_at: Option<String>,
    },
    KeepRecording {
        mime: String,
        size: f64,
        client_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        feeling: Option<Feeling>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        good: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        captured_at: Option<String>,
    },
    Unkeep {
        id: String,
    },
    Ask {
        question: String,
        /// `thisComputer` when the person chose to have this question answered on the computer.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        place: Option<AskPlace>,
    },
    CancelAsk {
        ask_id: String,
    },
    GetPage {
        date: Option<String>,
    },
    ListPages,
    GetItem {
        id: String,
    },
    GetOriginal {
        id: String,
    },
    Disconnect,
    /// A journal day, today when `date` is none.
    GetJournalDay {
        date: Option<String>,
    },
    /// What is written for a day, week, month or year, or its glance.
    GetLookBack {
        period: Period,
    },
    /// Ask the computer to write it ("Reflect on today").
    LookBack {
        period: Period,
    },
    /// The whole check-in, by touch.
    KeepCheckIn {
        input: CheckInInput,
        client_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        captured_at: Option<String>,
    },
    /// "Which {name} is this?": the one change a phone may make beyond keeping.
    AnswerSameName {
        question_id: String,
        answer: SameNameAnswer,
    },
    /// The page's question from the journal, by touch: the choice, and the line when one was
    /// written before the answer went across. Kept once per `client_id`, like a keep; answered
    /// with the phone's page as it stands after.
    AnswerJournalQuestion {
        page_date: String,
        choice: QuestionChoice,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        line: Option<String>,
        client_id: String,
    },
    /// The person has seen that Online answers now holds back only what they mark private.
    /// Answered with `Done`.
    SeeOnlineChange,
    /// One batch of calendar changes from a phone app, answered with `Synced`.
    SendCalendar {
        batch: CalendarBatch,
    },
    /// One batch of health readings from a phone app, answered with `Synced`.
    SendReadings {
        batch: ReadingBatch,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoteRequest {
    pub id: u32,
    pub call: RemoteCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum RemoteAnswer {
    Hello { hello: PhoneHello },
    Status { status: PhoneStatus },
    Kept { kept: PhoneKept },
    AskStarted { ask_id: String },
    AskDelta { delta: AskDelta },
    Page { page: Box<PageView> },
    Pages { pages: Vec<PageSummary> },
    Item { item: PhoneItem },
    OriginalStart { mime: String, size: f64 },
    JournalDay { day: Box<JournalDay> },
    LookBack { view: Box<ReflectionView> },
    CheckIn { checkin: Box<CheckIn> },
    SameName { answered: SameNameAnswered },
    Synced { receipt: SyncReceipt },
    Done,
    Error { code: PhoneError },
    Revoked,
}

/// An answer to request `id`. Id 0 is unsolicited (`Revoked`, `Hello`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoteReply {
    pub id: u32,
    pub answer: RemoteAnswer,
}
