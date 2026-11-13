//! Engine-level types that are not part of the interface contract.

use std::path::PathBuf;

use jiff::civil::Date;
use nocturne_contract::{EntityKind, ItemKind, MemoryView, Span, Trust};
use serde::{Deserialize, Serialize};

pub type ItemId = String;
pub type MemoryId = String;
pub type ConflictId = String;
pub type Tag = String;

/// The small, fixed set of relations (decision 0008). Anything else is quarantined as
/// unconfirmed until the person accepts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Predicate {
    Prefers,
    WorksOn,
    Decided,
    LivesIn,
    Knows,
    Happened,
    Due,
    Promised,
    Scheduled,
    Takes,
    Other(String),
}

impl Predicate {
    pub fn as_str(&self) -> &str {
        match self {
            Predicate::Prefers => "prefers",
            Predicate::WorksOn => "works_on",
            Predicate::Decided => "decided",
            Predicate::LivesIn => "lives_in",
            Predicate::Knows => "knows",
            Predicate::Happened => "happened",
            Predicate::Due => "due",
            Predicate::Promised => "promised",
            Predicate::Scheduled => "scheduled",
            Predicate::Takes => "takes",
            Predicate::Other(s) => s,
        }
    }

    pub fn parse(s: &str) -> Predicate {
        match s {
            "prefers" => Predicate::Prefers,
            "works_on" => Predicate::WorksOn,
            "decided" => Predicate::Decided,
            "lives_in" => Predicate::LivesIn,
            "knows" => Predicate::Knows,
            "happened" => Predicate::Happened,
            "due" => Predicate::Due,
            "promised" => Predicate::Promised,
            "scheduled" => Predicate::Scheduled,
            "takes" => Predicate::Takes,
            other => Predicate::Other(other.trim().to_lowercase()),
        }
    }

    pub fn is_other(&self) -> bool {
        matches!(self, Predicate::Other(_))
    }
}

/// One claim the mind found in an item. The extractor writes the British English `sentence`;
/// the engine never templates memory sentences.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub subject: String,
    pub subject_kind: Option<EntityKind>,
    pub predicate: Predicate,
    pub object: String,
    pub sentence: String,
    pub valid_from: Option<Date>,
    pub valid_to: Option<Date>,
    pub due: Option<Date>,
    pub confidence: f32,
    pub span: Option<Span>,
    pub tags: Vec<Tag>,
}

impl Candidate {
    /// A candidate with the required parts and no dates, tags or span.
    pub fn new(
        subject: &str,
        predicate: Predicate,
        object: &str,
        sentence: &str,
        confidence: f32,
    ) -> Candidate {
        Candidate {
            subject: subject.to_string(),
            subject_kind: None,
            predicate,
            object: object.to_string(),
            sentence: sentence.to_string(),
            valid_from: None,
            valid_to: None,
            due: None,
            confidence,
            span: None,
            tags: Vec::new(),
        }
    }
}

/// An item waiting to be read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingItem {
    pub id: ItemId,
    pub kind: ItemKind,
    pub source: nocturne_contract::Source,
    pub trust: Trust,
    pub kept_at: String,
    /// The stored original, when the item came from a file.
    pub original: Option<PathBuf>,
    pub mime: Option<String>,
    /// Text already known: the note itself, or a transcript recorded before a restart.
    pub text: Option<String>,
    pub attempts: u32,
    /// The token to present on every later call about this item. Empty if only peeked at.
    pub lease: String,
}

/// What recording a reading did to the item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordOutcome {
    Recorded,
    /// The recording or document is over the limit: the item is kept but not read.
    TooLong,
}

/// What recording a mind failure did to the item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureOutcome {
    /// The item went back to where it was; try again later.
    Retry,
    /// Too many failures: the item is `couldntRead` (corrupt).
    GaveUp,
}

/// Counts for the status line, computed in the store without loading rows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusCounts {
    /// Entries waiting on the person, as `needs_you()` would list them.
    pub needs_you: u32,
    /// Items kept and not yet read (waiting or being read).
    pub waiting_to_read: u32,
    /// Items the mind could not read.
    pub couldnt_read: u32,
    /// Unconfirmed memories waiting on the person (part of `needs_you`).
    pub unconfirmed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseKind {
    Answer,
    Page,
    Confirmed,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ApplyReport {
    pub created: Vec<MemoryId>,
    pub reinforced: Vec<MemoryId>,
    pub promoted: Vec<MemoryId>,
    pub unconfirmed: Vec<MemoryId>,
    pub conflicts: Vec<ConflictId>,
    /// Memories from an earlier reading of this item that this reading no longer supports.
    pub removed: Vec<MemoryId>,
    /// Memories whose sentence changed on a re-read: their old vector was dropped and they need
    /// embedding again.
    pub reworded: Vec<MemoryId>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecayReport {
    pub faded: u32,
    pub let_go: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    pub memory: MemoryView,
    pub score: f32,
    pub let_go: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub store_version: u32,
    pub memory_count: u32,
    pub item_count: u32,
    pub integrity_ok: bool,
}

/// What a correction changes. `object` is required; the rest keep their value when `None`.
/// For the date fields, `Some(None)` clears the date and `Some(Some(d))` sets it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CorrectPatch {
    pub object: String,
    /// The corrected sentence. When absent, the new object replaces the old one inside the old
    /// sentence if it appears there, otherwise the object becomes the sentence.
    pub sentence: Option<String>,
    pub due: Option<Option<Date>>,
    pub valid_from: Option<Option<Date>>,
    pub valid_to: Option<Option<Date>>,
}

impl CorrectPatch {
    /// A correction of the object alone.
    pub fn object(object: &str) -> Self {
        CorrectPatch {
            object: object.to_string(),
            ..Default::default()
        }
    }
}

/// The result of a completed forget. `fully_scrubbed` is false when the write-ahead log could
/// not be truncated yet (a reader was busy); it is retried at the next open, forget or tidy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForgetOutcome {
    pub report: nocturne_contract::ForgetReport,
    pub fully_scrubbed: bool,
}
