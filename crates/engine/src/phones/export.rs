//! What phones brought, in the export: one calendar file per phone, one table per health kind.

use std::fmt::Write as _;
use std::path::Path;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use nocturne_contract::{CalendarEntry, ReadingValue};
use rusqlite::Connection;
use serde_json::Value;

use super::KIND_ORDER;
use super::read::{Stored, readings_of};
use super::readings::kind_title;
use crate::error::Result;
use crate::fsutil;

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    fsutil::create_private_file_new(path)?.write_all(bytes)?;
    Ok(())
}

/// A name safe as a file name on every system the export may be opened on.
fn file_safe(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim().to_string();
    if s.is_empty() { "Phone".to_string() } else { s }
}

fn ics_text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace("\r\n", "\\n")
        .replace('\n', "\\n")
}

/// Content lines folded at 75 octets, as calendar files want.
fn ics_line(out: &mut String, line: &str) {
    let mut width = 0;
    for c in line.chars() {
        let n = c.len_utf8();
        if width + n > 75 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(c);
        width += n;
    }
    out.push_str("\r\n");
}

fn ics_time(s: &str) -> Option<String> {
    let t: Timestamp = s.parse().ok()?;
    Some(t.strftime("%Y%m%dT%H%M%SZ").to_string())
}

fn ics_date(s: &str) -> Option<String> {
    Some(s.get(..10)?.replace('-', ""))
}

struct Event {
    item_id: String,
    entry: CalendarEntry,
    calendar: String,
    came_in: String,
    removed: bool,
}

fn calendar_file(events: &[Event]) -> String {
    let mut out = String::new();
    for l in [
        "BEGIN:VCALENDAR",
        "VERSION:2.0",
        "PRODID:-//Nocturne//Export//EN",
    ] {
        ics_line(&mut out, l);
    }
    for e in events {
        ics_line(&mut out, "BEGIN:VEVENT");
        ics_line(&mut out, &format!("UID:{}", ics_text(&e.entry.phone_id)));
        if let Some(t) = ics_time(&e.came_in) {
            ics_line(&mut out, &format!("DTSTAMP:{t}"));
        }
        if e.entry.all_day {
            if let Some(d) = ics_date(&e.entry.start) {
                ics_line(&mut out, &format!("DTSTART;VALUE=DATE:{d}"));
            }
            if let Some(d) = ics_date(&e.entry.end) {
                ics_line(&mut out, &format!("DTEND;VALUE=DATE:{d}"));
            }
        } else {
            if let Some(t) = ics_time(&e.entry.start) {
                ics_line(&mut out, &format!("DTSTART:{t}"));
            }
            if let Some(t) = ics_time(&e.entry.end) {
                ics_line(&mut out, &format!("DTEND:{t}"));
            }
        }
        ics_line(
            &mut out,
            &format!("SUMMARY:{}", ics_text(e.entry.title.trim())),
        );
        if let Some(p) = &e.entry.place {
            ics_line(&mut out, &format!("LOCATION:{}", ics_text(p)));
        }
        let mut about = Vec::new();
        if !e.entry.people.is_empty() {
            about.push(format!("With {}", e.entry.people.join(", ")));
        }
        if let Some(n) = &e.entry.notes {
            about.push(n.clone());
        }
        if !about.is_empty() {
            ics_line(
                &mut out,
                &format!("DESCRIPTION:{}", ics_text(&about.join("\n"))),
            );
        }
        if !e.calendar.is_empty() {
            ics_line(&mut out, &format!("CATEGORIES:{}", ics_text(&e.calendar)));
        }
        if e.removed {
            ics_line(&mut out, "STATUS:CANCELLED");
        }
        ics_line(&mut out, &format!("X-NOCTURNE-ITEM:{}", e.item_id));
        ics_line(&mut out, "END:VEVENT");
    }
    ics_line(&mut out, "END:VCALENDAR");
    out
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn num(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{n:.0}")
    } else {
        format!("{n:.1}")
    }
}

/// The value and its unit, as Health gave them.
fn value_and_unit(v: &ReadingValue) -> (String, String) {
    match v {
        ReadingValue::Sleep {
            asleep,
            in_bed_only,
        } => {
            let mins: i64 = asleep
                .iter()
                .filter_map(|s| {
                    let a: Timestamp = s.from.parse().ok()?;
                    let b: Timestamp = s.to.parse().ok()?;
                    Some(b.duration_since(a).as_secs() / 60)
                })
                .sum();
            let unit = if *in_bed_only {
                "minutes in bed"
            } else {
                "minutes asleep"
            };
            (mins.to_string(), unit.into())
        }
        ReadingValue::HeartRate { hours, .. } => {
            let low = hours.iter().map(|h| h.low).fold(f64::MAX, f64::min);
            let high = hours.iter().map(|h| h.high).fold(f64::MIN, f64::max);
            if hours.is_empty() {
                (String::new(), "beats a minute".into())
            } else {
                (
                    format!("{} to {}", num(low), num(high)),
                    "beats a minute".into(),
                )
            }
        }
        ReadingValue::RestingHeartRate { bpm } => (num(*bpm), "beats a minute".into()),
        ReadingValue::BloodPressure { top, bottom } => (
            format!("{} over {}", num(*top), num(*bottom)),
            "mmHg".into(),
        ),
        ReadingValue::Steps { total } => (total.to_string(), "steps".into()),
        ReadingValue::Workout {
            kind,
            length_s,
            distance_m,
        } => {
            let mut v = format!("{kind}, {} minutes", length_s / 60);
            if let Some(d) = distance_m {
                let _ = write!(v, ", {} metres", num(*d));
            }
            (v, String::new())
        }
        ReadingValue::Weight { value, unit } => (num(*value), unit.clone()),
    }
}

fn readings_file(rows: &[Stored], tz: &TimeZone) -> String {
    let mut out = String::from("day,time,value,unit,source,removed\r\n");
    for r in rows {
        let time = r
            .start
            .parse::<Timestamp>()
            .map(|t| t.to_zoned(tz.clone()).strftime("%H:%M").to_string())
            .unwrap_or_default();
        let (value, unit) = value_and_unit(&r.value);
        let source = match &r.source_device {
            Some(d) => format!("{d}, through Health on {}", r.phone),
            None => format!("Health on {}", r.phone),
        };
        let removed = if r.removed { "removed in Health" } else { "" };
        let _ = write!(
            out,
            "{},{},{},{},{},{}\r\n",
            csv_field(&r.day),
            time,
            csv_field(&value),
            csv_field(&unit),
            csv_field(&source),
            removed
        );
    }
    out
}

/// Write `calendar/{phone}.ics` for each phone that brought its calendar and
/// `health/{kind}.csv` for each kind that came in. Nothing is written when nothing came in.
pub(crate) fn export_section(conn: &Connection, dir: &Path, tz: &TimeZone) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT r.device_id, COALESCE(d.name, ''), i.id, i.metadata, r.came_in_at, \
         r.removed_at IS NOT NULL \
         FROM phone_records r JOIN items i ON i.id = r.item_id \
         LEFT JOIN remote_devices d ON d.id = r.device_id \
         WHERE r.source = 'calendar' ORDER BY d.name, r.device_id, json_extract(i.metadata, '$.day'), i.id",
    )?;
    type Raw = (String, String, String, String, String, bool);
    let rows: Vec<Raw> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut phones: Vec<(String, String, Vec<Event>)> = Vec::new();
    for (device, name, item_id, meta, came_in, removed) in rows {
        let meta: Value = serde_json::from_str(&meta)?;
        let Some(entry) = meta
            .get("entry")
            .and_then(|e| serde_json::from_value::<CalendarEntry>(e.clone()).ok())
        else {
            continue;
        };
        let event = Event {
            item_id,
            calendar: meta
                .get("calendarName")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            entry,
            came_in,
            removed,
        };
        match phones.iter_mut().find(|p| p.0 == device) {
            Some(p) => p.2.push(event),
            None => phones.push((device, name, vec![event])),
        }
    }
    if !phones.is_empty() {
        let cal = dir.join("calendar");
        fsutil::create_chosen_dir_new(&cal)?;
        let mut used: Vec<String> = Vec::new();
        for (_, name, events) in &phones {
            let mut file = file_safe(name);
            let mut n = 1;
            while used.contains(&file) {
                n += 1;
                file = format!("{} ({n})", file_safe(name));
            }
            used.push(file.clone());
            write_new(
                &cal.join(format!("{file}.ics")),
                calendar_file(events).as_bytes(),
            )?;
        }
    }
    let mut health = None;
    for kind in KIND_ORDER {
        let rows = readings_of(conn, kind, "0000-01-01", "9999-12-31", true)?;
        if rows.is_empty() {
            continue;
        }
        let dir = match &health {
            Some(d) => d,
            None => {
                let d = dir.join("health");
                fsutil::create_chosen_dir_new(&d)?;
                health.insert(d)
            }
        };
        write_new(
            &dir.join(format!("{}.csv", kind_title(kind))),
            readings_file(&rows, tz).as_bytes(),
        )?;
    }
    Ok(())
}
