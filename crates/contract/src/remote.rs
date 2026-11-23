//! Away from home: reaching Nocturne from the person's own paired phones.
//!
//! The first group is what the desktop windows see. The phone-facing group crosses only the
//! sealed channel between the computer and a paired phone; it is exported so the phone app
//! shares the same names.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    AppPlatform, AskDelta, AskPlace, BroughtCounts, CalendarBatch, CheckIn, CheckInInput, Feeling,
    ItemKind, JournalDay, MemoryCounts, PageSummary, PageView, Period, PhoneFeature,
    QuestionChoice, ReadingBatch, ReflectionView, SameNameAnswer, SameNameAnswered, SyncReceipt,
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
    /// A phone with a newer Nocturne than this computer tried to connect.
    NeedsNewerHere,
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
    /// The oldest phone build the computer speaks.
    pub min_phone_build: u32,
    /// The newest phone build the computer speaks. A computer from before the version window
    /// leaves it out: it speaks `min_phone_build` only (see [`PhoneHello::newest_phone_build`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_phone_build: Option<u32>,
    /// What this computer takes from a phone app. A computer from before phone apps sends none.
    #[serde(default)]
    #[ts(as = "Option<Vec<PhoneFeature>>", optional)]
    pub features: Vec<PhoneFeature>,
    /// This computer already holds a memory: a phone that offers its own waits while the person
    /// here chooses how it joins.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub memory_here: bool,
}

impl PhoneHello {
    /// The newest phone build the computer speaks.
    pub fn newest_phone_build(&self) -> u32 {
        self.max_phone_build
            .unwrap_or(self.min_phone_build)
            .max(self.min_phone_build)
    }
}

/// What a phone app says of itself first (`introduce`): which app, its version, and the oldest
/// and newest phone builds it speaks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhoneAppIntro {
    pub platform: AppPlatform,
    /// "Nocturne {version}" as the person sees it.
    pub version: String,
    pub min_build: u32,
    pub max_build: u32,
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
    /// Marked private. A computer never sends a private item to a phone, so only a phone that
    /// keeps its own memory sees it set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(optional, as = "Option<bool>")]
    pub private: bool,
    /// What it is in the journal; none for an ordinary thing kept. Journal items cannot be
    /// marked private.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub journal: Option<crate::JournalRole>,
    /// Who made the words of a voice note, when it was the phone itself: the screens say
    /// "Heard by this phone". Never sent by a computer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub heard_by: Option<crate::HeardBy>,
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
    /// The phone is older than the oldest build this computer speaks: update the phone.
    TooOld,
    /// The phone is newer than the newest build this computer speaks, or a call needs a newer
    /// build than the two agreed: update the computer.
    ComputerOutdated,
    /// A phone that keeps its own memory has no room for what it was asked to keep. The thing
    /// may be kept without its feeling or mark: the person frees some room and the keep goes
    /// again, and finishes. Never sent by a computer.
    Full,
    /// A reply this phone could not read, from a newer computer: the call failed, the session
    /// goes on. Never sent by a computer.
    Unknown,
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
        /// Shared from a web page or a link: kept as pasted web content, untrusted.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        #[ts(optional, as = "Option<bool>")]
        web: bool,
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
    /// A phone app's first call on a session: answered with the build both speak
    /// (`Introduced`), or `TooOld` or `ComputerOutdated` when there is none.
    Introduce {
        app: PhoneAppIntro,
    },
    /// A phone that keeps its own memory offers it to this computer, with what it holds.
    /// Answered with `MemoryAccepted`, `UseComputerMemory` (the computer holds one and the person
    /// there chose it), or an error (`TooLarge` over `MAX_MEMORY_BYTES`).
    OfferMemory {
        counts: MemoryCounts,
        bytes: f64,
        /// The store's schema version, so a computer that is older says so before any bytes move.
        schema: u32,
        /// The first day something was kept, for "{count} things kept on {name} since {date}".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        since: Option<String>,
        /// The person chose on the phone to bring its things in from what a move put aside: the
        /// computer adds them to its memory without asking again.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        #[ts(as = "Option<bool>", optional)]
        bring: bool,
    },
    /// The backup file of the offered memory goes up as the chunks of this call, `size` bytes
    /// in all (at most `MAX_MEMORY_BYTES`), after `OfferMemory` was accepted. `sha256` is the
    /// file's digest (lower-case hex) and `passphrase` the one-time passphrase it is locked
    /// with. Answered with `MemoryArrived` once checked and in place.
    MemoryDone {
        size: f64,
        sha256: String,
        passphrase: String,
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
    Hello {
        hello: PhoneHello,
    },
    Status {
        status: PhoneStatus,
    },
    Kept {
        kept: PhoneKept,
    },
    AskStarted {
        ask_id: String,
    },
    AskDelta {
        delta: AskDelta,
    },
    Page {
        page: Box<PageView>,
    },
    Pages {
        pages: Vec<PageSummary>,
    },
    Item {
        item: PhoneItem,
    },
    OriginalStart {
        mime: String,
        size: f64,
    },
    JournalDay {
        day: Box<JournalDay>,
    },
    LookBack {
        view: Box<ReflectionView>,
    },
    CheckIn {
        checkin: Box<CheckIn>,
    },
    SameName {
        answered: SameNameAnswered,
    },
    Synced {
        receipt: SyncReceipt,
    },
    Introduced {
        agreed: u32,
    },
    /// The offer is welcome. `here` is what this computer already holds, so the phone can say
    /// so before the upload; none when it holds nothing. `bring`: the phone's things are added
    /// to the memory here, with no restart, rather than the memory moving whole.
    MemoryAccepted {
        here: Option<MemoryCounts>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        #[ts(as = "Option<bool>", optional)]
        bring: bool,
    },
    /// The memory is checked and in place on this computer.
    MemoryArrived {
        counts: MemoryCounts,
    },
    /// The person on this computer chose its memory: nothing comes in, and the phone works with
    /// it like any other phone.
    UseComputerMemory,
    /// The phone's things are checked (`counts`, as the backup held them) and were added to the
    /// memory here (`added`).
    MemoryBrought {
        counts: MemoryCounts,
        added: BroughtCounts,
    },
    Done,
    Error {
        code: PhoneError,
    },
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
