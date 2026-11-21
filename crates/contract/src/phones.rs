//! What paired phones bring: the calendar and health readings.
//!
//! The first group crosses the sealed channel from the phone app (`SendCalendar`,
//! `SendReadings`). The second is what the desktop windows see in From your phones, on the page,
//! in the journal and on each kind's own page. Times are RFC 3339 with offset and days are
//! `YYYY-MM-DD`, as everywhere in the contract; `*_id` fields on a desktop view are item ids,
//! receipts to the original.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What a phone can bring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PhoneSource {
    Calendar,
    Health,
}

/// What a computer takes, told to the phone in its hello. A phone hides an offer the computer
/// does not list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PhoneFeature {
    Calendar,
    Health,
}

/// Holiday and birthday calendars start off on the phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CalendarKind {
    Ordinary,
    Holidays,
    Birthdays,
}

/// A calendar as the phone names it. `colour` is a CSS colour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhoneCalendar {
    pub id: String,
    pub name: String,
    pub colour: Option<String>,
    pub kind: CalendarKind,
}

/// One appointment on one day. A repeating entry goes as the separate days it falls on inside
/// the window, each with its own `phone_id`. `notes` is sent only when the person turned notes
/// on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarEntry {
    pub phone_id: String,
    pub calendar_id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    /// The entry's IANA time zone, when the phone knows it.
    pub time_zone: Option<String>,
    pub all_day: bool,
    pub place: Option<String>,
    pub people: Vec<String>,
    pub repeats: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum CalendarChange {
    Upsert {
        entry: CalendarEntry,
    },
    /// Deleted or cancelled on the phone while still inside the window.
    Removed {
        phone_id: String,
    },
}

/// One frame of a calendar send. A send is every batch under one `sync_id`, `seq` from 0, the
/// last with `last` set; `window_from` and `window_to` are the days read, both included.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarBatch {
    pub sync_id: String,
    pub seq: u32,
    pub last: bool,
    pub window_from: String,
    pub window_to: String,
    pub calendars: Vec<PhoneCalendar>,
    pub changes: Vec<CalendarChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ReadingKind {
    Sleep,
    HeartRate,
    RestingHeartRate,
    BloodPressure,
    Steps,
    Workout,
    Weight,
}

/// A stretch of time, both ends RFC 3339. (`Span` is a receipt's place in an item.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TimeSpan {
    pub from: String,
    pub to: String,
}

/// One hour of heart rate: its lowest, highest and middle, in beats a minute. `start` is the
/// hour's first instant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HourRange {
    pub start: String,
    pub low: f64,
    pub high: f64,
    pub middle: f64,
}

/// What a reading holds, by kind. Units are as Health gives them: beats a minute, mmHg,
/// seconds and metres; weight carries its own unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ReadingValue {
    /// `in_bed_only` when the phone had no asleep periods and `asleep` holds time in bed.
    Sleep {
        asleep: Vec<TimeSpan>,
        in_bed_only: bool,
    },
    HeartRate {
        hours: Vec<HourRange>,
        count: u32,
    },
    RestingHeartRate {
        bpm: f64,
    },
    BloodPressure {
        top: f64,
        bottom: f64,
    },
    Steps {
        total: u32,
    },
    /// Never the route.
    Workout {
        kind: String,
        length_s: u32,
        distance_m: Option<f64>,
    },
    Weight {
        value: f64,
        unit: String,
    },
}

/// One reading: a night, a day of heart rate, resting heart rate or steps, one blood pressure
/// or weight reading, or one workout. `day` is the journal day it belongs to (a night belongs
/// to the day it ends on).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Reading {
    pub phone_id: String,
    pub day: String,
    pub start: String,
    pub end: Option<String>,
    /// The app or device Health names ("Watch", "This phone").
    pub source_device: Option<String>,
    pub value: ReadingValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ReadingChange {
    Upsert {
        reading: Reading,
    },
    /// Deleted in Health.
    Removed {
        phone_id: String,
        kind: ReadingKind,
    },
}

/// One frame of a health send, under one `sync_id` like [`CalendarBatch`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReadingBatch {
    pub sync_id: String,
    pub seq: u32,
    pub last: bool,
    pub changes: Vec<ReadingChange>,
}

/// The computer's answer to one batch. `stopped`: the person stopped this source on the
/// computer, and nothing was taken. `too_many`: the send went over its cap, and only the first
/// changes up to it were taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncReceipt {
    pub taken: u32,
    pub unchanged: u32,
    pub refused: u32,
    pub stopped: bool,
    pub too_many: bool,
}

// What the desktop windows see -------------------------------------------------------------

/// From your phones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhoneSourcesView {
    pub phones: Vec<PhoneSources>,
    pub page: PhonePageOptions,
    /// "Notice what goes together".
    pub noticing: bool,
}

/// One paired phone and what it brings. A source is none until the phone has offered it;
/// `in_browser` is a phone on the browser page, which brings nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhoneSources {
    pub device_id: String,
    pub name: String,
    pub in_browser: bool,
    pub calendar: Option<SourceState>,
    pub health: Option<SourceState>,
}

/// `calendars` and `upcoming` count for the calendar, `kinds` for health.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SourceState {
    pub taking: bool,
    pub last_in_at: Option<String>,
    pub calendars: u32,
    pub upcoming: u32,
    pub kinds: Vec<ReadingKind>,
}

/// What the morning page shows from phones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhonePageOptions {
    pub calendar_today: bool,
    pub sleep_last_night: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CameInTag {
    New,
    Changed,
    Removed,
}

/// One line of "What came in", newest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CameInRow {
    pub item_id: String,
    pub came_in_at: String,
    pub title: String,
    pub day: String,
    pub tag: CameInTag,
}

/// One appointment in "In your calendar today". `zone` is the entry's own place name when its
/// time zone differs from the computer's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarLine {
    pub item_id: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    pub title: String,
    pub place: Option<String>,
    pub calendar: String,
    pub phone: String,
    pub zone: Option<String>,
}

/// How a value sits against the person's usual, said only when outside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Versus {
    More,
    Less,
    Higher,
    Lower,
}

/// Last night: the page's line, the journal's night bar and the first check-in's context.
/// `usual_bed` and `usual_wake` are times of day (`HH:MM`) for the two faint ticks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NightLine {
    pub item_id: String,
    pub asleep_min: u32,
    pub from: String,
    pub to: String,
    pub awake_count: u32,
    pub periods: Vec<TimeSpan>,
    pub versus: Option<Versus>,
    pub usual_bed: Option<String>,
    pub usual_wake: Option<String>,
    pub phone: String,
}

/// The person's usual: the middle half of their own readings over the last 60 days.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Band {
    pub low: f64,
    pub high: f64,
}

/// One day's value on a line or day bars.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SeriesPoint {
    pub day: String,
    pub value: f64,
    pub item_id: String,
}

/// One kind over days, `from` to `to` both included. `usual` is none before 14 days of
/// readings; `journal_days` are the days with a journal entry, for the day markers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HealthSeries {
    pub kind: ReadingKind,
    pub from: String,
    pub to: String,
    pub points: Vec<SeriesPoint>,
    pub usual: Option<Band>,
    pub journal_days: Vec<String>,
}

/// One day's heart rate range, in beats a minute.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DayRange {
    pub item_id: String,
    pub low: f64,
    pub high: f64,
    pub middle: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PressureReading {
    pub item_id: String,
    pub at: String,
    pub top: f64,
    pub bottom: f64,
}

/// "Walk · 42 min · 3.1 km" on the timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorkoutRow {
    pub item_id: String,
    pub start: String,
    pub kind: String,
    pub length_s: u32,
    pub distance_m: Option<f64>,
}

/// "From Health" beside a journal day. Kinds not brought are none or empty.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DayHealth {
    pub night: Option<NightLine>,
    pub resting: Option<HealthSeries>,
    pub steps: Option<HealthSeries>,
    pub heart: Option<DayRange>,
    pub pressure: Vec<PressureReading>,
    pub workouts: Vec<WorkoutRow>,
}

/// One reading in "Show as a list" and on a kind's own page. `removed`: deleted in Health,
/// kept here until the person deletes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReadingRow {
    pub item_id: String,
    pub day: String,
    pub start: String,
    pub end: Option<String>,
    pub value: ReadingValue,
    pub source_device: Option<String>,
    pub phone: String,
    pub removed: bool,
}

/// What to delete from one phone: one source, optionally some kinds, optionally only before a
/// day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhoneDeleteScope {
    pub device_id: String,
    pub source: PhoneSource,
    pub kinds: Option<Vec<ReadingKind>>,
    pub before: Option<String>,
}

/// What a delete would remove, and the token that applies it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhoneDeletePreview {
    pub token: String,
    pub items: u32,
    pub versions: u32,
    pub memories: u32,
    pub page_lines: u32,
    pub recap_lines: u32,
    pub noticing: u32,
}
