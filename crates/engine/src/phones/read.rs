//! What phones brought, as the page, the journal and the health pages read it. Removed entries
//! and readings are left out of every line, chart and sentence; only a kind's own list shows
//! them.

use std::collections::{BTreeMap, HashMap};

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use nocturne_contract::{
    BroughtItem, CalendarDetail, CalendarEntry, CalendarLine, CalendarWas, DayFromPhones,
    DayHealth, DayRange, HealthSeries, NightLine, PhoneCalendar, PressureReading, PressureUsual,
    ReadingKind, ReadingRow, ReadingValue, SeriesPoint, TimeSpan, Versus, WorkoutRow,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use super::usual::{self, USUAL_DAYS, USUAL_MIN_DAYS};
use crate::Engine;
use crate::error::{Error, Result};
use crate::util::enum_str;

/// "In your calendar today" shows this many rows, then "{count} more today".
pub const CALENDAR_TODAY_ROWS: usize = 5;
/// Resting heart rate's line in "From Health".
const RESTING_DAYS: i64 = 30;
/// Steps' day bars in "From Health".
const STEPS_DAYS: i64 = 14;

/// One current calendar entry.
pub(crate) struct CalEntry {
    pub item_id: String,
    pub entry: CalendarEntry,
    pub calendar: String,
    pub phone: String,
}

/// The calendars a phone named, as it last sent them.
pub(crate) fn phone_calendars(conn: &Connection, device_id: &str) -> Result<Vec<PhoneCalendar>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT calendars FROM phone_sources WHERE device_id = ?1 AND source = 'calendar'",
            [device_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(json
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default())
}

/// One current entry, read from its item.
pub(crate) fn cal_entry(
    conn: &Connection,
    item_id: &str,
    meta: &Value,
    device_id: &str,
    phone: String,
    names: &mut HashMap<String, Vec<PhoneCalendar>>,
) -> Result<Option<CalEntry>> {
    let Some(entry) = meta
        .get("entry")
        .and_then(|e| serde_json::from_value::<CalendarEntry>(e.clone()).ok())
    else {
        return Ok(None);
    };
    if !names.contains_key(device_id) {
        names.insert(device_id.to_string(), phone_calendars(conn, device_id)?);
    }
    let calendar = names[device_id]
        .iter()
        .find(|c| c.id == entry.calendar_id)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    Ok(Some(CalEntry {
        item_id: item_id.to_string(),
        entry,
        calendar,
        phone,
    }))
}

/// The entries on the day, an entry over several days on each of them: all-day ones first, then
/// by start.
pub(crate) fn calendar_entries(conn: &Connection, day: Date) -> Result<Vec<CalEntry>> {
    let mut stmt = conn.prepare_cached(
        "SELECT i.id, i.metadata, r.device_id, COALESCE(d.name, '') \
         FROM phone_records r JOIN items i ON i.id = r.item_id \
         LEFT JOIN remote_devices d ON d.id = r.device_id AND d.connection != 'local' \
         WHERE r.source = 'calendar' AND r.removed_at IS NULL \
         AND json_extract(i.metadata, '$.day') <= ?1 \
         AND COALESCE(json_extract(i.metadata, '$.endDay'), json_extract(i.metadata, '$.day')) >= ?1",
    )?;
    let rows: Vec<(String, String, String, String)> = stmt
        .query_map([day.to_string()], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut names = HashMap::new();
    let mut out = Vec::new();
    for (item_id, meta, device, phone) in rows {
        let meta: Value = serde_json::from_str(&meta)?;
        if let Some(c) = cal_entry(conn, &item_id, &meta, &device, phone, &mut names)? {
            out.push(c);
        }
    }
    out.sort_by(|a, b| {
        b.entry
            .all_day
            .cmp(&a.entry.all_day)
            .then_with(|| start_of(&a.entry).cmp(&start_of(&b.entry)))
            .then_with(|| a.item_id.cmp(&b.item_id))
    });
    Ok(out)
}

fn start_of(e: &CalendarEntry) -> Option<Timestamp> {
    e.start.parse().ok()
}

/// The place an IANA zone is named for: "Europe/Lisbon" is "Lisbon".
fn zone_place(name: &str) -> String {
    name.rsplit('/').next().unwrap_or(name).replace('_', " ")
}

/// The entry's own zone, when its clock differs from the computer's at the entry's start.
fn other_zone(e: &CalendarEntry, tz: &TimeZone) -> Option<String> {
    if e.all_day {
        return None;
    }
    let name = e.time_zone.as_deref()?;
    let theirs = TimeZone::get(name).ok()?;
    let at = start_of(e)?;
    (theirs.to_offset(at) != tz.to_offset(at)).then(|| zone_place(name))
}

pub(crate) fn calendar_line(c: &CalEntry, tz: &TimeZone) -> CalendarLine {
    CalendarLine {
        item_id: c.item_id.clone(),
        start: c.entry.start.clone(),
        end: c.entry.end.clone(),
        all_day: c.entry.all_day,
        title: c.entry.title.trim().to_string(),
        place: c.entry.place.clone().filter(|p| !p.trim().is_empty()),
        calendar: c.calendar.clone(),
        phone: c.phone.clone(),
        zone: other_zone(&c.entry, tz),
    }
}

/// One stored reading.
pub(crate) struct Stored {
    pub item_id: String,
    pub day: String,
    pub start: String,
    pub end: Option<String>,
    pub value: ReadingValue,
    pub source_device: Option<String>,
    pub phone: String,
    pub device_id: String,
    pub removed: bool,
}

/// The current readings of one kind on the days `from` to `to`, both included, oldest first.
pub(crate) fn readings_of(
    conn: &Connection,
    kind: ReadingKind,
    from: &str,
    to: &str,
    with_removed: bool,
) -> Result<Vec<Stored>> {
    let mut stmt = conn.prepare_cached(
        "SELECT i.id, i.metadata, r.removed_at IS NOT NULL, COALESCE(d.name, ''), r.device_id \
         FROM phone_records r JOIN items i ON i.id = r.item_id \
         LEFT JOIN remote_devices d ON d.id = r.device_id AND d.connection != 'local' \
         WHERE r.source = 'health' AND i.kind = 'reading' \
         AND json_extract(i.metadata, '$.reading.type') = ?1 \
         AND json_extract(i.metadata, '$.day') BETWEEN ?2 AND ?3 \
         AND (?4 OR r.removed_at IS NULL)",
    )?;
    let rows: Vec<(String, String, bool, String, String)> = stmt
        .query_map(params![enum_str(&kind), from, to, with_removed], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut out = Vec::new();
    for (item_id, meta, removed, phone, device_id) in rows {
        let meta: Value = serde_json::from_str(&meta)?;
        let Some(value) = meta
            .get("reading")
            .and_then(|v| serde_json::from_value::<ReadingValue>(v.clone()).ok())
        else {
            continue;
        };
        let text = |k: &str| meta.get(k).and_then(Value::as_str).map(str::to_string);
        out.push(Stored {
            item_id,
            day: text("day").unwrap_or_default(),
            start: text("start").unwrap_or_default(),
            end: text("end"),
            source_device: text("sourceDevice"),
            value,
            phone,
            device_id,
            removed,
        });
    }
    out.sort_by(|a, b| (&a.day, &a.start, &a.item_id).cmp(&(&b.day, &b.start, &b.item_id)));
    Ok(out)
}

fn days_back(day: Date, n: i64) -> Result<Date> {
    day.checked_sub(jiff::Span::new().days(n - 1))
        .map_err(Error::from)
}

fn asleep_minutes(spans: &[TimeSpan]) -> u32 {
    spans
        .iter()
        .filter_map(|s| {
            let (a, b) = (
                s.from.parse::<Timestamp>().ok()?,
                s.to.parse::<Timestamp>().ok()?,
            );
            Some(b.duration_since(a).as_secs().max(0))
        })
        .sum::<i64>()
        .div_euclid(60) as u32
}

/// The value one reading stands for on a chart.
fn chart_value(v: &ReadingValue) -> f64 {
    match v {
        ReadingValue::Sleep { asleep, .. } => f64::from(asleep_minutes(asleep)),
        ReadingValue::HeartRate { hours, .. } if !hours.is_empty() => {
            hours.iter().map(|h| h.middle).sum::<f64>() / hours.len() as f64
        }
        ReadingValue::HeartRate { .. } => f64::NAN,
        ReadingValue::RestingHeartRate { bpm } => *bpm,
        ReadingValue::BloodPressure { top, .. } => *top,
        ReadingValue::Steps { total } => f64::from(*total),
        ReadingValue::Workout { length_s, .. } => f64::from(*length_s) / 60.0,
        ReadingValue::Weight { value, .. } => *value,
    }
}

/// One value per day: the longest night, the most steps (two phones may count the same walk),
/// the minutes of every workout together, otherwise the average. The receipt is the reading the
/// value came from (the longest night, the most steps), else the day's latest.
pub(crate) fn per_day(kind: ReadingKind, readings: &[Stored]) -> Vec<SeriesPoint> {
    let mut days: BTreeMap<&str, Vec<&Stored>> = BTreeMap::new();
    for r in readings.iter().filter(|r| !r.removed) {
        days.entry(r.day.as_str()).or_default().push(r);
    }
    let mut out = Vec::new();
    for (day, rs) in days {
        let valued: Vec<(&Stored, f64)> = rs
            .iter()
            .map(|r| (*r, chart_value(&r.value)))
            .filter(|(_, v)| v.is_finite())
            .collect();
        if valued.is_empty() {
            continue;
        }
        let values: Vec<f64> = valued.iter().map(|(_, v)| *v).collect();
        let value = match kind {
            ReadingKind::Sleep | ReadingKind::Steps => {
                values.iter().copied().fold(f64::MIN, f64::max)
            }
            ReadingKind::Workout => values.iter().sum(),
            _ => values.iter().sum::<f64>() / values.len() as f64,
        };
        let chosen = match kind {
            ReadingKind::Sleep | ReadingKind::Steps => valued
                .iter()
                .max_by(|a, b| a.1.total_cmp(&b.1).then(a.0.start.cmp(&b.0.start)))
                .map(|(r, _)| *r),
            _ => valued
                .iter()
                .map(|(r, _)| *r)
                .max_by(|a, b| a.start.cmp(&b.start)),
        }
        .expect("not empty");
        out.push(SeriesPoint {
            day: day.to_string(),
            value,
            item_id: chosen.item_id.clone(),
        });
    }
    out
}

fn journal_days(conn: &Connection, from: &str, to: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare_cached(
        "SELECT DISTINCT json_extract(metadata, '$.journal_day') AS d FROM items \
         WHERE json_extract(metadata, '$.journal') IS NOT NULL AND d BETWEEN ?1 AND ?2 ORDER BY d",
    )?;
    Ok(stmt
        .query_map([from, to], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

pub(crate) fn health_series(
    conn: &Connection,
    tz: &TimeZone,
    kind: ReadingKind,
    from: Date,
    to: Date,
) -> Result<HealthSeries> {
    if from > to {
        return Err(Error::invalid("a range starts before it ends"));
    }
    let points = per_day(
        kind,
        &readings_of(conn, kind, &from.to_string(), &to.to_string(), false)?,
    );
    let first = days_back(to, USUAL_DAYS)?;
    let recent = readings_of(conn, kind, &first.to_string(), &to.to_string(), false)?;
    let history = per_day(kind, &recent);
    let values = dated(&history);
    let usual = usual::band(&values, to);
    let pressure_usual = if kind == ReadingKind::BloodPressure {
        let bottoms: Vec<(Date, f64)> = per_day_by(&recent, |v| match v {
            ReadingValue::BloodPressure { bottom, .. } => *bottom,
            _ => f64::NAN,
        });
        match (usual, usual::band(&bottoms, to)) {
            (Some(top), Some(bottom)) => Some(PressureUsual { top, bottom }),
            _ => None,
        }
    } else {
        None
    };
    let (usual_bed, usual_wake) = if kind == ReadingKind::Sleep {
        usual_times(&recent, &history, tz)
    } else {
        (None, None)
    };
    Ok(HealthSeries {
        kind,
        from: from.to_string(),
        to: to.to_string(),
        points,
        usual,
        journal_days: journal_days(conn, &from.to_string(), &to.to_string())?,
        pressure_usual,
        usual_bed,
        usual_wake,
    })
}

fn dated(points: &[SeriesPoint]) -> Vec<(Date, f64)> {
    points
        .iter()
        .filter_map(|p| Some((p.day.parse().ok()?, p.value)))
        .collect()
}

/// The average of one number per day, for a band of its own.
fn per_day_by(readings: &[Stored], value: impl Fn(&ReadingValue) -> f64) -> Vec<(Date, f64)> {
    let mut days: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for r in readings.iter().filter(|r| !r.removed) {
        let v = value(&r.value);
        if v.is_finite() {
            days.entry(r.day.as_str()).or_default().push(v);
        }
    }
    days.into_iter()
        .filter_map(|(d, vs)| Some((d.parse().ok()?, vs.iter().sum::<f64>() / vs.len() as f64)))
        .collect()
}

/// The usual bedtime and waking time, from the nights the usual counts, once there are enough.
fn usual_times(
    nights: &[Stored],
    per: &[SeriesPoint],
    tz: &TimeZone,
) -> (Option<String>, Option<String>) {
    let mut beds = Vec::new();
    let mut wakes = Vec::new();
    for p in per {
        if let Some(n) = nights.iter().find(|n| n.item_id == p.item_id)
            && let Some((_, f, t)) = night_of(n)
        {
            beds.push(after_noon(f, tz));
            wakes.push(after_noon(t, tz));
        }
    }
    if beds.len() < USUAL_MIN_DAYS {
        return (None, None);
    }
    (Some(clock_of(median(beds))), Some(clock_of(median(wakes))))
}

/// Minutes after noon, so a bedtime either side of midnight sorts as the evening it is.
fn after_noon(t: Timestamp, tz: &TimeZone) -> i64 {
    let z = t.to_zoned(tz.clone());
    (i64::from(z.hour()) * 60 + i64::from(z.minute()) + 12 * 60) % (24 * 60)
}

fn clock_of(after_noon: i64) -> String {
    let m = (after_noon + 12 * 60) % (24 * 60);
    format!("{:02}:{:02}", m / 60, m % 60)
}

fn median(mut v: Vec<i64>) -> i64 {
    v.sort_unstable();
    v[v.len() / 2]
}

fn night_of(s: &Stored) -> Option<(Vec<TimeSpan>, Timestamp, Timestamp)> {
    let ReadingValue::Sleep { asleep, .. } = &s.value else {
        return None;
    };
    let mut spans = asleep.clone();
    spans.sort_by(|a, b| a.from.cmp(&b.from));
    let from = spans.first()?.from.parse().ok()?;
    let to = spans
        .iter()
        .filter_map(|s| s.to.parse::<Timestamp>().ok())
        .max()?;
    Some((spans, from, to))
}

/// The night that ended on `day`: the longest one if two phones sent it.
pub(crate) fn night_line(conn: &Connection, tz: &TimeZone, day: Date) -> Result<Option<NightLine>> {
    let first = days_back(day, USUAL_DAYS)?;
    let nights = readings_of(
        conn,
        ReadingKind::Sleep,
        &first.to_string(),
        &day.to_string(),
        false,
    )?;
    let today = day.to_string();
    let Some(best) = nights
        .iter()
        .filter(|n| n.day == today)
        .max_by_key(|n| (chart_value(&n.value) as i64, n.start.clone()))
    else {
        return Ok(None);
    };
    let Some((periods, _, _)) = night_of(best) else {
        return Ok(None);
    };
    let asleep_min = asleep_minutes(&periods);
    let per = per_day(ReadingKind::Sleep, &nights);
    let values: Vec<(Date, f64)> = per
        .iter()
        .filter_map(|p| Some((p.day.parse().ok()?, p.value)))
        .collect();
    let versus = usual::band(&values, day).and_then(|b| {
        let v = f64::from(asleep_min);
        if v < b.low {
            Some(Versus::Less)
        } else if v > b.high {
            Some(Versus::More)
        } else {
            None
        }
    });
    let (usual_bed, usual_wake) = usual_times(&nights, &per, tz);
    Ok(Some(NightLine {
        item_id: best.item_id.clone(),
        asleep_min,
        from: periods.first().map(|s| s.from.clone()).unwrap_or_default(),
        to: periods
            .iter()
            .max_by_key(|s| s.to.parse::<Timestamp>().ok())
            .map(|s| s.to.clone())
            .unwrap_or_default(),
        awake_count: periods.len().saturating_sub(1) as u32,
        periods,
        versus,
        usual_bed,
        usual_wake,
        phone: best.phone.clone(),
    }))
}

/// "From Health" beside a journal day; none when no phone brings anything for it.
pub(crate) fn day_health(conn: &Connection, tz: &TimeZone, day: Date) -> Result<Option<DayHealth>> {
    let d = day.to_string();
    let night = night_line(conn, tz, day)?;
    let series = |kind: ReadingKind, n: i64| -> Result<Option<HealthSeries>> {
        let s = health_series(conn, tz, kind, days_back(day, n)?, day)?;
        Ok((!s.points.is_empty()).then_some(s))
    };
    let resting = series(ReadingKind::RestingHeartRate, RESTING_DAYS)?;
    let steps = series(ReadingKind::Steps, STEPS_DAYS)?;
    let heart = readings_of(conn, ReadingKind::HeartRate, &d, &d, false)?
        .into_iter()
        .filter_map(|r| match r.value {
            ReadingValue::HeartRate { hours, .. } if !hours.is_empty() => Some(DayRange {
                item_id: r.item_id,
                low: hours.iter().map(|h| h.low).fold(f64::MAX, f64::min),
                high: hours.iter().map(|h| h.high).fold(f64::MIN, f64::max),
                middle: hours.iter().map(|h| h.middle).sum::<f64>() / hours.len() as f64,
            }),
            _ => None,
        })
        .max_by(|a, b| a.item_id.cmp(&b.item_id));
    let pressure: Vec<PressureReading> =
        readings_of(conn, ReadingKind::BloodPressure, &d, &d, false)?
            .into_iter()
            .filter_map(|r| match r.value {
                ReadingValue::BloodPressure { top, bottom } => Some(PressureReading {
                    item_id: r.item_id,
                    at: r.start,
                    top,
                    bottom,
                }),
                _ => None,
            })
            .collect();
    let workouts = workouts_on(conn, day)?;
    if night.is_none()
        && resting.is_none()
        && steps.is_none()
        && heart.is_none()
        && pressure.is_empty()
        && workouts.is_empty()
    {
        return Ok(None);
    }
    Ok(Some(DayHealth {
        night,
        resting,
        steps,
        heart,
        pressure,
        workouts,
    }))
}

pub(crate) fn workouts_on(conn: &Connection, day: Date) -> Result<Vec<WorkoutRow>> {
    let d = day.to_string();
    Ok(readings_of(conn, ReadingKind::Workout, &d, &d, false)?
        .into_iter()
        .filter_map(|r| match r.value {
            ReadingValue::Workout {
                kind,
                length_s,
                distance_m,
            } => Some(WorkoutRow {
                item_id: r.item_id,
                start: r.start,
                kind,
                length_s,
                distance_m,
            }),
            _ => None,
        })
        .collect())
}

impl Engine {
    /// "In your calendar today" for `day`, every entry, with "{count} more" past the first five.
    pub fn calendar_today(&self, day: Date) -> Result<(Vec<CalendarLine>, u32)> {
        let tz = self.clock().tz();
        self.read(|conn| {
            let all: Vec<CalendarLine> = calendar_entries(conn, day)?
                .iter()
                .map(|c| calendar_line(c, &tz))
                .collect();
            Ok(cap_lines(all))
        })
    }

    /// The night that ended on `day`.
    pub fn last_night(&self, day: Date) -> Result<Option<NightLine>> {
        let tz = self.clock().tz();
        self.read(|conn| night_line(conn, &tz, day))
    }

    /// "From Health" beside the journal's day.
    pub fn day_health(&self, day: Date) -> Result<Option<DayHealth>> {
        let tz = self.clock().tz();
        self.read(|conn| day_health(conn, &tz, day))
    }

    /// One kind over the days `from` to `to`, both included, with the person's usual.
    pub fn health_series(&self, kind: ReadingKind, from: Date, to: Date) -> Result<HealthSeries> {
        let tz = self.clock().tz();
        self.read(|conn| health_series(conn, &tz, kind, from, to))
    }

    /// Every reading of one kind on the days `from` to `to`, newest first, removed ones too.
    pub fn readings(&self, kind: ReadingKind, from: Date, to: Date) -> Result<Vec<ReadingRow>> {
        self.read(|conn| {
            let mut rows: Vec<ReadingRow> =
                readings_of(conn, kind, &from.to_string(), &to.to_string(), true)?
                    .into_iter()
                    .map(|r| ReadingRow {
                        item_id: r.item_id,
                        day: r.day,
                        start: r.start,
                        end: r.end,
                        value: r.value,
                        source_device: r.source_device,
                        phone: r.phone,
                        removed: r.removed,
                        device_id: r.device_id,
                    })
                    .collect();
            rows.reverse();
            Ok(rows)
        })
    }
}

/// The first rows and how many more there are.
pub(crate) fn cap_lines(mut all: Vec<CalendarLine>) -> (Vec<CalendarLine>, u32) {
    let more = all.len().saturating_sub(CALENDAR_TODAY_ROWS) as u32;
    all.truncate(CALENDAR_TODAY_ROWS);
    (all, more)
}

/// What phones brought for a day, for "Delete this day": the entries that start on it and the
/// readings that belong to it, removed ones too.
pub(crate) fn day_brought(conn: &Connection, day: Date) -> Result<Option<DayFromPhones>> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.source, i.id, i.title, COALESCE(d.name, '') \
         FROM phone_records r JOIN items i ON i.id = r.item_id \
         LEFT JOIN remote_devices d ON d.id = r.device_id AND d.connection != 'local' \
         WHERE json_extract(i.metadata, '$.day') = ?1 \
         ORDER BY json_extract(i.metadata, '$.entry.start'), json_extract(i.metadata, '$.start'), i.id",
    )?;
    let rows: Vec<(String, String, String, String)> = stmt
        .query_map([day.to_string()], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut out = DayFromPhones {
        calendar: Vec::new(),
        health: Vec::new(),
    };
    for (source, item_id, title, phone) in rows {
        let row = BroughtItem {
            item_id,
            title,
            phone,
        };
        if source == "calendar" {
            out.calendar.push(row);
        } else {
            out.health.push(row);
        }
    }
    Ok((!out.calendar.is_empty() || !out.health.is_empty()).then_some(out))
}

/// A repeat rule in words, when it is a simple one: "Every Friday", "Every other week",
/// "Every day", "Every month", "Every year". Anything else is left unsaid.
pub(crate) fn repeat_words(rule: &str) -> Option<String> {
    let mut freq = None;
    let mut interval = 1u32;
    let mut days: Vec<&str> = Vec::new();
    for part in rule.trim().trim_start_matches("RRULE:").split(';') {
        let (k, v) = part.split_once('=')?;
        match k {
            "FREQ" => freq = Some(v),
            "INTERVAL" => interval = v.parse().ok()?,
            "BYDAY" => days = v.split(',').collect(),
            "WKST" | "UNTIL" | "COUNT" => {}
            _ => return None,
        }
    }
    let name = |d: &str| -> Option<&'static str> {
        Some(match d {
            "MO" => "Monday",
            "TU" => "Tuesday",
            "WE" => "Wednesday",
            "TH" => "Thursday",
            "FR" => "Friday",
            "SA" => "Saturday",
            "SU" => "Sunday",
            _ => return None,
        })
    };
    let every = match interval {
        1 => "Every",
        2 => "Every other",
        _ => return None,
    };
    Some(match (freq?, days.as_slice()) {
        ("DAILY", []) => match interval {
            1 => "Every day".to_string(),
            _ => "Every other day".to_string(),
        },
        ("WEEKLY", []) => format!("{every} week"),
        ("WEEKLY", ds) => {
            let names: Vec<&str> = ds.iter().map(|d| name(d)).collect::<Option<_>>()?;
            let list = match names.as_slice() {
                [one] => (*one).to_string(),
                [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
                [] => return None,
            };
            format!("{every} {list}")
        }
        ("MONTHLY", []) => format!("{every} month"),
        ("YEARLY", []) => format!("{every} year"),
        _ => return None,
    })
}

/// The name of the calendar an appointment came from, as its phone names it.
pub(crate) fn calendar_of(conn: &Connection, item_id: &str) -> Result<Option<String>> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT metadata, json_extract(metadata, '$.phone.device') FROM items \
             WHERE id = ?1 AND kind = 'calendar'",
            [item_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((meta, device)) = row else {
        return Ok(None);
    };
    let meta: Value = serde_json::from_str(&meta)?;
    let mut names = HashMap::new();
    Ok(
        cal_entry(conn, item_id, &meta, &device, String::new(), &mut names)?
            .map(|c| c.calendar)
            .filter(|name| !name.is_empty()),
    )
}

/// An appointment's own page: its calendar and phone, what it was before, and whether it was
/// removed on the phone.
pub(crate) fn calendar_detail(
    conn: &Connection,
    tz: &TimeZone,
    item_id: &str,
) -> Result<Option<CalendarDetail>> {
    let row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT metadata, kept_at, json_extract(metadata, '$.phone.device') FROM items \
             WHERE id = ?1 AND kind = 'calendar'",
            [item_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((meta, updated_at, device)) = row else {
        return Ok(None);
    };
    let meta: Value = serde_json::from_str(&meta)?;
    let phone: String = conn
        .query_row(
            "SELECT name FROM remote_devices WHERE id = ?1 AND connection != 'local'",
            [&device],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_default();
    let mut names = HashMap::new();
    let Some(c) = cal_entry(conn, item_id, &meta, &device, phone.clone(), &mut names)? else {
        return Ok(None);
    };
    let removed: bool = conn
        .query_row(
            "SELECT removed_at IS NOT NULL FROM phone_records WHERE item_id = ?1",
            [item_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(false);
    let was = match meta.get("replaces").and_then(Value::as_str) {
        Some(prev) => conn
            .query_row(
                "SELECT json_extract(metadata, '$.entry') FROM items WHERE id = ?1",
                [prev],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten()
            .and_then(|e| serde_json::from_str::<CalendarEntry>(&e).ok())
            .map(|e| CalendarWas {
                start: e.start,
                end: e.end,
                title: e.title.trim().to_string(),
                place: e.place,
            }),
        None => None,
    };
    let line = calendar_line(&c, tz);
    Ok(Some(CalendarDetail {
        calendar: c.calendar,
        phone,
        device_id: device,
        start: c.entry.start.clone(),
        end: c.entry.end.clone(),
        all_day: c.entry.all_day,
        zone: line.zone,
        place: line.place,
        people: c.entry.people.clone(),
        repeats: c.entry.repeats,
        repeat_rule: c.entry.repeat_rule.as_deref().and_then(repeat_words),
        updated_at,
        was,
        removed,
    }))
}
