use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Receipt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PageVariant {
    Normal,
    First,
    Quiet,
    Heavy,
    BeingWritten,
    CatchUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LineKind {
    Due,
    Which,
    Unconfirmed,
    Promise,
    Changed,
    LetGo,
    Merged,
    Upcoming,
    Learned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Edition {
    Screen,
    Audio,
    Print,
    /// For a paired phone: private lines dropped exactly as for print.
    Phone,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Choice {
    pub memory_id: String,
    pub text: String,
    pub receipt: Receipt,
}

/// One line of the morning page. `text` marks emphasis with `**double asterisks**`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PageLine {
    pub id: String,
    pub kind: LineKind,
    pub text: String,
    pub memory_ids: Vec<String>,
    pub receipts: Vec<Receipt>,
    pub choices: Vec<Choice>,
    pub conflict_id: Option<String>,
    /// The short subject the line is about, for example "Dentist appointment"; lets the interface
    /// say "Dentist appointment set to Thursday 10:00." after a which-is-right choice.
    pub subject: Option<String>,
    pub done: bool,
    pub forgotten: bool,
    /// For upcoming lines: the day it happens (`YYYY-MM-DD`).
    pub on: Option<String>,
    /// For let-go lines: when it was last used (`YYYY-MM-DD`).
    pub last_used: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum QuestionKind {
    YesNo,
    PageTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum QuestionChoice {
    Yes,
    No,
    NotSure,
    Right,
    Earlier,
    Later,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Question {
    pub kind: QuestionKind,
    pub text: String,
    pub memory_id: Option<String>,
    pub receipt: Option<Receipt>,
    pub last_asked: Option<String>,
    pub answered: Option<QuestionChoice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateOffer {
    pub version: String,
    pub bytes: f64,
    pub mind: bool,
}

/// The morning page for one day, for one edition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PageView {
    pub date: String,
    pub page_number: u32,
    pub edition: Edition,
    pub variant: PageVariant,
    pub headline: String,
    pub dek: String,
    pub needs_you: Vec<PageLine>,
    pub changed: Vec<PageLine>,
    pub let_go: Vec<PageLine>,
    pub upcoming: Vec<PageLine>,
    pub learned: Vec<PageLine>,
    pub question: Option<Question>,
    pub more_needs_you: u32,
    pub more_changed: u32,
    pub kept_count: u32,
    pub changes_count: u32,
    pub let_go_count: u32,
    pub catch_up_days: u32,
    pub private_skipped: u32,
    /// Items in this page's window still waiting to be read.
    pub pending: u32,
    pub tidied_from: Option<String>,
    pub tidied_to: Option<String>,
    pub update: Option<UpdateOffer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PageSummary {
    pub date: String,
    pub page_number: u32,
    pub headline: String,
    pub needs_you: u32,
}

/// A part of the spoken page, in the order it is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SpokenKind {
    Opening,
    NeedsYou,
    Changed,
    ComingUp,
    LetGo,
    Question,
}

/// Where one part of the spoken page starts in the audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SpokenSection {
    pub kind: SpokenKind,
    pub start_ms: u32,
}

/// The spoken page: the audio file, its parts and how long it runs. `private_skipped` counts
/// the lines left out because they are private.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SpokenPage {
    pub path: String,
    pub sections: Vec<SpokenSection>,
    pub duration_ms: u32,
    pub private_skipped: u32,
}
