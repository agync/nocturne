//! What phones brought, as the page, the journal and the health pages read it. Removed entries
//! and readings are left out of every line, chart and sentence; only a kind's own list shows
//! them.

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use nocturne_contract::{
    CalendarEntry, CalendarLine, DayHealth, DayRange, HealthSeries, NightLine, PressureReading,
    ReadingKind, ReadingRow, ReadingValue, SeriesPoint, TimeSpan, Versus, WorkoutRow,
};
use rusqlite::{Connection, params};
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

/// The day's current entries, all-day ones first, then by start.
pub(crate) fn calendar_entries(conn: &Connection, day: Date) -> Result<Vec<CalEntry>> {
    let mut stmt = conn.prepare_cached(
        "SELECT i.id, i.metadata, COALESCE(d.name, '') \
         FROM phone_records r JOIN items i ON i.id = r.item_id \
         LEFT JOIN remote_devices d ON d.id = r.device_id \
         WHERE r.source = 'calendar' AND r.removed_at IS NULL \
         AND json_extract(i.metadata, '$.day') = ?1",
    )?;
    let rows: Vec<(String, String, String)> = stmt
        .query_map([day.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut out = Vec::new();
    for (item_id, meta, phone) in rows {
        let meta: Value = serde_json::from_str(&meta)?;
        let Some(entry) = meta
            .get("entry")
            .and_then(|e| serde_json::from_value::<CalendarEntry>(e.clone()).ok())
        else {
            continue;
        };
        out.push(CalEntry {
            item_id,
            calendar: meta
                .get("calendarName")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            entry,
            phone,
        });
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
        "SELECT i.id, i.metadata, r.removed_at IS NOT NULL, COALESCE(d.name, '') \
         FROM phone_records r JOIN items i ON i.id = r.item_id \
         LEFT JOIN remote_devices d ON d.id = r.device_id \
         WHERE r.source = 'health' AND i.kind = 'reading' \
         AND json_extract(i.metadata, '$.reading.type') = ?1 \
         AND json_extract(i.metadata, '$.day') BETWEEN ?2 AND ?3 \
         AND (?4 OR r.removed_at IS NULL)",
    )?;
    let rows: Vec<(String, String, bool, String)> = stmt
        .query_map(params![enum_str(&kind), from, to, with_removed], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut out = Vec::new();
    for (item_id, meta, removed, phone) in rows {
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
/// the minutes of every workout together, otherwise the average. The receipt is the day's
/// latest reading.
fn per_day(kind: ReadingKind, readings: &[Stored]) -> Vec<SeriesPoint> {
    let mut days: BTreeMap<&str, Vec<&Stored>> = BTreeMap::new();
    for r in readings.iter().filter(|r| !r.removed) {
        days.entry(r.day.as_str()).or_default().push(r);
    }
    let mut out = Vec::new();
    for (day, rs) in days {
        let values: Vec<f64> = rs
            .iter()
            .map(|r| chart_value(&r.value))
            .filter(|v| v.is_finite())
            .collect();
        if values.is_empty() {
            continue;
        }
        let value = match kind {
            ReadingKind::Sleep | ReadingKind::Steps => {
                values.iter().copied().fold(f64::MIN, f64::max)
            }
            ReadingKind::Workout => values.iter().sum(),
            _ => values.iter().sum::<f64>() / values.len() as f64,
        };
        let latest = rs
            .iter()
            .max_by(|a, b| a.start.cmp(&b.start))
            .expect("not empty");
        out.push(SeriesPoint {
            day: day.to_string(),
            value,
            item_id: latest.item_id.clone(),
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
    let history = per_day(
        kind,
        &readings_of(conn, kind, &first.to_string(), &to.to_string(), false)?,
    );
    let values: Vec<(Date, f64)> = history
        .iter()
        .filter_map(|p| Some((p.day.parse().ok()?, p.value)))
        .collect();
    Ok(HealthSeries {
        kind,
        from: from.to_string(),
        to: to.to_string(),
        points,
        usual: usual::band(&values, to),
        journal_days: journal_days(conn, &from.to_string(), &to.to_string())?,
    })
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
    // The usual bedtime and waking time, from the same nights as the usual.
    let mut beds = Vec::new();
    let mut wakes = Vec::new();
    for p in &per {
        if let Some(n) = nights.iter().find(|n| n.item_id == p.item_id)
            && let Some((_, f, t)) = night_of(n)
        {
            beds.push(after_noon(f, tz));
            wakes.push(after_noon(t, tz));
        }
    }
    let enough = beds.len() >= USUAL_MIN_DAYS;
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
        usual_bed: enough.then(|| clock_of(median(beds))),
        usual_wake: enough.then(|| clock_of(median(wakes))),
        phone: best.phone.clone(),
    }))
}

/// "From Health" beside a journal day; none when no phone brings anything for it.
pub(crate) fn day_health(conn: &Connection, tz: &TimeZone, day: Date) -> Result<Option<DayHealth>> {
    let d = day.to_string();
    let night = night_line(conn, tz, day)?;
    let series = |kind: ReadingKind, n: i64| -> Result<Option<HealthSeries>> {
        let s = health_series(conn, kind, days_back(day, n)?, day)?;
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
        self.read(|conn| health_series(conn, kind, from, to))
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
