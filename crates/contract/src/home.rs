//! A phone that keeps its own memory: the calls the phone's screens make of its own store, and
//! the counts the move to a computer speaks of.
//!
//! These never cross the sealed channel. The screens that a phone shares with a computer keep
//! speaking `RemoteCall`; the verbs here exist only where the phone is home.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    BackupReport, ExportReport, ForgetPreview, ForgetReport, LastBackup, PhoneDeletePreview,
    PhoneItem, PhoneSource, Receipt, RestoreCheck,
};

/// Where a phone's memory lives. `Alone`: on the phone itself. `Companion`: on a computer the
/// phone is paired with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PhoneMode {
    Alone,
    Companion,
}

/// One stretch of an excerpt, and whether a searched word is in it. The parts run together to
/// make the excerpt, so the interface can mark the match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExcerptPart {
    pub text: String,
    pub matched: bool,
}

/// A kept thing found by its words. The receipt names the original; a private one is flagged
/// and the screen decides what to show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FoundItem {
    pub receipt: Receipt,
    pub excerpt: Vec<ExcerptPart>,
    pub private: bool,
}

/// What to forget: the same scope a computer's forget takes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForgetScope {
    pub ids: Vec<String>,
    pub include_originals: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub also: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pick: Option<bool>,
}

/// What a memory holds, in the words the move and the backup screens count in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MemoryCounts {
    /// Things kept.
    pub items: u32,
    pub memories: u32,
    pub pages: u32,
    pub originals: u32,
}

/// What bringing a phone's things into a computer's memory added there, as both sides say it:
/// things kept, apart from the journal and check-ins; journal entries and good things; and
/// check-ins, each as they stand now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BroughtCounts {
    pub things: u32,
    pub entries: u32,
    pub checkins: u32,
}

/// What a memory holds, in the columns a phone shows before a restore replaces it: things kept
/// (not the journal, check-ins, the calendar or readings), journal entries and good things as
/// they stand now, photos and recordings, and check-ins.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KeptCounts {
    pub things: u32,
    pub journal_entries: u32,
    pub photos_and_recordings: u32,
    pub checkins: u32,
}

/// A memory put aside on this phone: by a restore that replaced it, by bringing back another,
/// or because it could not be opened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PutAsideMemory {
    /// Names this one in the calls that bring it back or delete it.
    pub id: String,
    /// When it was put aside.
    pub at: String,
    pub bytes: f64,
    /// None when it cannot be read: it was put aside because it could not be opened.
    pub counts: Option<KeptCounts>,
}

/// How a thing came to be kept, for the Kept row on its page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum KeptHow {
    Typed,
    Taken,
    Chosen,
    Recorded,
    Shared,
    Calendar,
    Health,
}

/// What a thing's page on a phone alone says of how it was kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ItemKept {
    /// How it was kept on this phone; none when it was kept elsewhere or not noted.
    pub how: Option<KeptHow>,
    /// The phone it was kept on, when that was another one.
    pub device: Option<String>,
    /// Nocturne worked something out from it, so there is something to forget.
    pub worked_out: bool,
}

/// What a phone that keeps its own memory shows of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HomeStats {
    pub counts: MemoryCounts,
    /// What the store takes on this phone, in bytes.
    pub bytes: f64,
    /// When the last thing was kept.
    pub last_kept: Option<String>,
    pub last_backup: Option<LastBackup>,
}

/// Why a home call did not do what was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HomeError {
    Invalid,
    NotFound,
    /// The memory is not open: the phone is not alone, or the store is being put right.
    NotReady,
    /// A forget token that was used or has gone stale.
    TooLate,
    /// The backup's passphrase did not open it.
    WrongPassphrase,
    /// A file that is not a Nocturne backup.
    NotABackup,
    /// The backup was made by a newer Nocturne than this one: update the app first.
    NewerBackup,
    /// The backup is damaged or incomplete. Nothing here was changed.
    DamagedBackup,
    /// The phone has no room for it.
    Full,
    /// The phrase to delete everything was not typed exactly.
    WrongPhrase,
    /// The person stopped a backup or a restore before it finished. Nothing changed.
    Stopped,
    Failed,
}

/// What the phone's screens ask of the phone's own memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "cmd", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum HomeCall {
    /// Find kept things by their words; at most `FIND_CAP` come back.
    Find {
        query: String,
    },
    PreviewForget {
        scope: ForgetScope,
    },
    Forget {
        token: String,
    },
    /// Write a locked backup file to `dest`.
    SaveBackup {
        dest: String,
        passphrase: String,
    },
    /// Open a backup file and say what restoring it would do here, before anything changes.
    CheckBackup {
        file: String,
        passphrase: String,
    },
    /// Bring a backup back. On a phone that holds a memory, `bring_in` says the person agreed to
    /// bring the backup's things into it.
    RestoreBackup {
        file: String,
        passphrase: String,
        bring_in: bool,
    },
    DeleteEverything {
        phrase: String,
    },
    /// Write everything kept, readable, into a fresh folder inside `dest`.
    Export {
        dest: String,
    },
    Stats,
    /// Mark a kept thing private, or not. A journal entry, a good thing, a check-in or a health
    /// reading cannot be: `Invalid`.
    SetPrivate {
        id: String,
        private: bool,
    },
    /// The memories put aside on this phone, newest first.
    PutAside,
    /// Bring one back in place of what is here, which is put aside in turn.
    BringBack {
        id: String,
    },
    DeletePutAside {
        id: String,
    },
    /// What deleting everything this phone brought from one source would remove.
    PreviewSourceDelete {
        source: PhoneSource,
    },
    /// Apply that preview. `also_stop`: the phone stops bringing that source.
    DeleteSource {
        token: String,
        source: PhoneSource,
        also_stop: bool,
    },
    /// Note how a thing was just kept on this phone. Calendar and Health are not noted: they
    /// say so themselves.
    SetKeptHow {
        id: String,
        how: KeptHow,
    },
    /// How a thing was kept, for its page.
    ItemKept {
        id: String,
    },
    /// Forget what was learned from one thing kept; `with_original` deletes the thing too.
    PreviewForgetItem {
        id: String,
        with_original: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum HomeAnswer {
    Found {
        items: Vec<FoundItem>,
    },
    ForgetPreview {
        preview: ForgetPreview,
    },
    Forgotten {
        report: ForgetReport,
    },
    BackupSaved {
        report: BackupReport,
    },
    BackupChecked {
        check: Box<RestoreCheck>,
        /// The columns of the backup, and of this phone when it holds anything.
        backup: KeptCounts,
        here: Option<KeptCounts>,
    },
    Restored,
    /// A backup's things joined the memory here.
    BroughtIn {
        things: u32,
        entries: u32,
        checkins: u32,
        /// On both, so kept once.
        same: u32,
    },
    Deleted,
    Exported {
        report: ExportReport,
    },
    Stats {
        stats: HomeStats,
    },
    /// The thing as it is now, after its private mark changed.
    PrivateSet {
        item: PhoneItem,
    },
    PutAside {
        memories: Vec<PutAsideMemory>,
    },
    PutAsideDeleted,
    SourceDeletePreview {
        preview: PhoneDeletePreview,
    },
    SourceDeleted,
    KeptHowSet,
    ItemKept {
        kept: ItemKept,
    },
    Error {
        code: HomeError,
    },
}
