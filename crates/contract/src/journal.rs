//! The journal: the day's entry, its timeline, check-ins, good things, the people and places
//! found in them, and the reflections and recaps that look back on a day or a period.
//!
//! A day runs from the day boundary to the next (`docs/design/journaling.md`, "What a day is").
//! Every date here is a journal day as `YYYY-MM-DD`, never a calendar date of an instant.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{EntityKind, ItemView, MemoryView, PageLine, Receipt};

/// The four parts of the feelings field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FeelingPart {
    /// More energy, unpleasant.
    WoundUp,
    /// More energy, pleasant.
    LitUp,
    /// Less energy, unpleasant.
    Low,
    /// Less energy, pleasant.
    Settled,
}

/// Where the person put the point: `pleasant` from -1 (unpleasant) to 1 (pleasant) and `energy`
/// from -1 (less) to 1 (more). Its distance from the middle is how strong the feeling is.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FeelingPoint {
    pub pleasant: f32,
    pub energy: f32,
}

/// A feeling: its part, the point and up to three words from Nocturne's own list, as shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Feeling {
    pub part: FeelingPart,
    pub point: FeelingPoint,
    pub words: Vec<String>,
}

/// What a check-in screen saves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckInInput {
    pub feeling: Feeling,
    /// "What's going on?"
    pub line: Option<String>,
    /// "Did you feel like doing anything?"
    pub urge: Option<String>,
    /// The check-in this one changes ("Change" on the timeline). It keeps that one's day.
    pub replaces: Option<String>,
}

/// A saved check-in. Journal content: never read aloud, notified or sent online.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckIn {
    /// The check-in's item id.
    pub id: String,
    pub kept_at: String,
    pub day: String,
    pub feeling: Feeling,
    pub line: Option<String>,
    pub urge: Option<String>,
    /// The quick note it was tapped with, when it came from quick capture or a phone keep.
    pub about: Option<String>,
    /// Ids of the ideas from "Something that might help" the person noted as tried, in order.
    pub tried: Vec<String>,
    pub from_device: Option<String>,
    pub mentions: Vec<Mention>,
    pub left_out: bool,
}

/// What an item is in the journal. Ordinary things kept have none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum JournalRole {
    /// A block of the day's entry: a paragraph, a voice note or a photo.
    Entry,
    Good,
    Checkin,
    /// An answer to the page's question from the journal, kept on the day it was answered.
    Answer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MentionState {
    Found,
    Confirmed,
    Removed,
}

/// A person, place or thing an item mentions, where it says it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Mention {
    pub id: String,
    pub item_id: String,
    pub entity_id: String,
    pub name: String,
    pub kind: EntityKind,
    pub role: Option<String>,
    /// Where the name is in the item's text, in UTF-16 code units as JavaScript counts them.
    /// `None` when it was not found word for word.
    pub start: Option<u32>,
    pub end: Option<u32>,
    pub state: MentionState,
    /// This item is where Nocturne first met the name.
    pub new: bool,
    /// Memories this item added about them.
    pub memory_count: u32,
}

/// An earlier version of a changed entry block, kept as history until deleted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EarlierVersion {
    pub item_id: String,
    pub text: Option<String>,
    pub kept_at: String,
}

/// One block of the day's entry: the current version of a paragraph, voice note or photo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EntryBlock {
    pub item: ItemView,
    /// Order in the entry, from 0.
    pub block: u32,
    pub mentions: Vec<Mention>,
    /// Newest first.
    pub earlier: Vec<EarlierVersion>,
    /// When this version replaced an earlier one that had been read.
    pub changed_at: Option<String>,
    /// A photo's description as the person changed it ("Describe this photo").
    pub alt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GoodThing {
    /// The good thing's item id.
    pub id: String,
    pub text: String,
    pub day: String,
    pub kept_at: String,
    pub photo: Option<ItemView>,
    pub from_device: Option<String>,
    pub mentions: Vec<Mention>,
    pub left_out: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DayPart {
    Morning,
    Afternoon,
    Evening,
    AfterMidnight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RowKind {
    /// A note the person kept.
    Note,
    Voice,
    /// A photo the person took.
    Photo,
    Checkin,
    Good,
    /// A document, web page, screenshot or file from the inbox folder: background.
    Elsewhere,
    /// "{time} · Today's entry": when the entry was written. Not repeated in full.
    Entry,
    /// An answer to the page's question from the journal.
    Answer,
}

/// An answer to the page's question from the journal, as the timeline shows it: the choice, the
/// line if one was added, and "About: “{sentence}”", which opens the block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct JournalAnswer {
    pub choice: crate::QuestionChoice,
    pub line: Option<String>,
    /// The entry block the question was about.
    pub about: String,
    /// The person's sentence the question quoted.
    pub sentence: String,
}

/// One row of "Through the day", oldest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TimelineRow {
    pub kind: RowKind,
    pub part: DayPart,
    pub item: ItemView,
    pub checkin: Option<CheckIn>,
    pub good: Option<GoodThing>,
    /// The feeling tapped with a quick note.
    pub feeling: Option<CheckIn>,
    pub mentions: Vec<Mention>,
    pub left_out: bool,
    /// For a quick note: the day "Move to {weekday}" moves it to.
    pub move_to: Option<String>,
    /// For an answer to the page's question from the journal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub answer: Option<JournalAnswer>,
}

/// A name found in the day's entries, once per person, place or thing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NameRef {
    pub entity_id: String,
    pub name: String,
    pub kind: EntityKind,
    pub role: Option<String>,
    pub new: bool,
    /// The items that mention it, the receipts of the name.
    pub item_ids: Vec<String>,
}

/// "Which {name} is this?": a name in an entry that fits two people Nocturne already knows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SameNameQuestion {
    pub id: String,
    pub name: String,
    /// The line it was said in, as written.
    pub quote: String,
    pub receipt: Receipt,
    pub options: Vec<SameNameOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SameNameOption {
    pub entity_id: String,
    pub name: String,
    pub role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum SameNameAnswer {
    Entity {
        entity_id: String,
    },
    /// "Someone new".
    New,
    /// "Leave it": nothing is filed.
    Leave,
}

/// What an answer filed, for "Filed under {name}, {role}."
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SameNameAnswered {
    pub name: String,
    pub role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReflectionSummary {
    pub first_sentence: String,
    pub written_at: String,
    pub from_count: u32,
}

/// One journal day, as the journal opens it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct JournalDay {
    pub day: String,
    /// The day that is still going.
    pub today: bool,
    /// The person has not dismissed "Your journal" yet.
    pub first_use: bool,
    pub entry: Vec<EntryBlock>,
    /// The latest save of any entry block.
    pub entry_saved_at: Option<String>,
    /// When the entry was last read, once every block is read.
    pub entry_read_at: Option<String>,
    pub rows: Vec<TimelineRow>,
    pub checkins: Vec<CheckIn>,
    /// At most three.
    pub good: Vec<GoodThing>,
    pub people: Vec<NameRef>,
    pub reflection: Option<ReflectionSummary>,
    /// Things kept that day still waiting to be read.
    pub reading: u32,
    /// Open "Which {name} is this?" questions from this day's items.
    pub same_name: Vec<SameNameQuestion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PeriodKind {
    Day,
    Week,
    Month,
    Year,
}

/// A day, a week, a month or a year, as journal days, both ends included.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Period {
    pub kind: PeriodKind,
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum WrittenWhen {
    OnDemand,
    Overnight,
}

/// A sentence that looks back, with what it came from. `text` marks spans with `**double
/// asterisks**`, each span standing for the receipt at the same index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReflectionLine {
    pub text: String,
    pub receipts: Vec<Receipt>,
}

/// The day's reflection. Only "The day" is written by the mind; the other sections are what
/// was kept, as it was kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DayReflection {
    pub id: String,
    pub period: Period,
    pub written_at: String,
    pub written: WrittenWhen,
    /// Written while the day was still going.
    pub so_far: bool,
    pub from_count: u32,
    pub the_day: Vec<ReflectionLine>,
    /// "How it felt": the day's check-ins.
    pub felt: Vec<CheckIn>,
    pub good: Vec<GoodThing>,
    pub people: Vec<NameRef>,
    /// Promises, plans and due dates said that day that are not done.
    pub still_open: Vec<MemoryView>,
    /// Things from the period not yet read when it was written.
    pub still_reading: u32,
    /// Lines taken out because what they came from was deleted.
    pub lines_removed: u32,
}

/// One kind of thing kept, for "9 things kept: 4 notes, ...".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GlanceKind {
    Note,
    Voice,
    Photo,
    Document,
    Web,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KeptCount {
    pub kind: GlanceKind,
    pub item_ids: Vec<String>,
}

/// The day or period at a glance: built from what is already known, with no reading at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Glance {
    pub period: Period,
    /// Biggest count first; kinds with none are left out.
    pub kept: Vec<KeptCount>,
    pub felt: Vec<CheckIn>,
    pub good: Vec<GoodThing>,
    /// People, then places and things.
    pub with: Vec<NameRef>,
    /// The first two lines of the day's entry, and the block they came from.
    pub entry_lines: Vec<String>,
    pub entry_item_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GlanceReason {
    /// First download, or the engine is starting.
    NotReady,
    /// This size of computer has not passed the journal check.
    NotOnThisComputer,
    /// The period holds only things kept from elsewhere, or too few lines carried a receipt.
    Listed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Highlight {
    pub day: String,
    /// In the person's own words, or a memory's sentence with `**spans**`.
    pub text: String,
    /// The text is a quote and is shown in quotation marks.
    pub quoted: bool,
    pub receipts: Vec<Receipt>,
    pub photo: Option<ItemView>,
    pub feeling: Option<Feeling>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WordCount {
    pub word: String,
    pub part: FeelingPart,
    pub checkin_ids: Vec<String>,
}

/// "On {count} of the {total} days you {did}, you checked in as {part or word}." Shown only
/// when it held at least three times.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Together {
    pub count: u32,
    pub total: u32,
    pub with: NameRef,
    pub part: Option<FeelingPart>,
    pub word: Option<String>,
    pub receipts: Vec<Receipt>,
}

/// "Gran · 4 days". Counts are days, not mentions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DayCount {
    pub with: NameRef,
    pub days: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Tried {
    pub idea: String,
    pub day: String,
    pub checkin_id: String,
}

/// "{count} check-ins, {difference} more than {period}". Counts only, never a judgement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Compare {
    pub count: u32,
    pub previous: u32,
    pub previous_period: Period,
}

/// A week, month or year looked back on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Recap {
    pub id: String,
    pub period: Period,
    pub written_at: String,
    pub written: WrittenWhen,
    pub so_far: bool,
    /// Days in the period with anything in the journal.
    pub days: u32,
    pub highlights: Vec<Highlight>,
    /// Every check-in in the period, for the dots.
    pub checkins: Vec<CheckIn>,
    /// Most used first.
    pub words: Vec<WordCount>,
    pub together: Vec<Together>,
    pub compare: Option<Compare>,
    pub good: Vec<GoodThing>,
    pub people: Vec<DayCount>,
    /// "New this week", "New this month", or the year's "Firsts".
    pub new_names: Vec<NameRef>,
    pub tried: Vec<Tried>,
    pub changed: Vec<PageLine>,
    /// Empty for a year.
    pub still_open: Vec<MemoryView>,
    pub lines_removed: u32,
    /// Things from the period not yet read when it was put together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub still_reading: Option<u32>,
}

/// What the journal shows for a day or a period that is looked back on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ReflectionView {
    /// A day's reflection.
    Written { reflection: DayReflection },
    /// A week, month or year.
    Recap { recap: Recap },
    /// The day or period listed, not written about.
    Glance {
        glance: Glance,
        reason: GlanceReason,
    },
    /// Nothing written yet; the person can ask for it.
    NotYet { glance: Glance },
    /// Being written on this computer now, or a year that takes two nights.
    Writing { glance: Glance },
    /// "Nothing to look back on yet." or "Nothing was kept this week."
    Nothing { period: Period },
}

/// A period that can be opened from "Choose a day".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PeriodSummary {
    pub period: Period,
    /// A reflection or recap is written for it.
    pub written: bool,
}

/// The evening reminder, off by default. Its words are fixed and carry nothing kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EveningReminder {
    pub on: bool,
    /// `HH:MM`, local time.
    pub time: String,
}
