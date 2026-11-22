//! A phone that keeps its own memory: the calls the phone's screens make of its own store, and
//! the counts the move to a computer speaks of.
//!
//! These never cross the sealed channel. The screens that a phone shares with a computer keep
//! speaking `RemoteCall`; the verbs here exist only where the phone is home.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    BackupReport, ExportReport, ForgetPreview, ForgetReport, LastBackup, PhoneItem, Receipt,
    RestoreCheck,
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
    /// A backup file that is not one, or is from a newer Nocturne.
    NotABackup,
    /// The phone has no room for it.
    Full,
    /// The phrase to delete everything was not typed exactly.
    WrongPhrase,
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
    /// Bring a backup back. `replace` says the person agreed to put what is here aside.
    RestoreBackup {
        file: String,
        passphrase: String,
        replace: bool,
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
    },
    Restored,
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
    Error {
        code: HomeError,
    },
}
