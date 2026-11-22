//! Calendar entries in: each one an item and one appointment memory made from its own fields.

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use nocturne_contract::{
    CalendarBatch, CalendarChange, CalendarEntry, EntityKind, PhoneCalendar, PhoneSource, Span,
    SyncReceipt,
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};

use super::{
    CALENDAR_PER_SEND, NewVersion, SendCount, check_device, content_hash, finish_send,
    insert_version, mark_removed, record, set_replaces, source_row, stopped,
};
use crate::Engine;
use crate::error::Result;
use crate::memory::apply_structured;
use crate::types::{Candidate, Predicate};
use crate::util::{fmt_ts, new_id};

/// The longest title, place, name or note kept from an entry, in characters.
const MAX_FIELD: usize = 2000;
const MAX_PEOPLE: usize = 100;

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// "Thursday 26 November".
pub(crate) fn day_words(d: Date) -> String {
    format!(
        "{} {} {}",
        WEEKDAYS[d.weekday().to_monday_zero_offset() as usize],
        d.day(),
        MONTHS[(d.month() - 1) as usize]
    )
}

/// "9:00", in the computer's time.
pub(crate) fn time_words(t: Timestamp, tz: &TimeZone) -> String {
    let z = t.to_zoned(tz.clone());
    format!("{}:{:02}", z.hour(), z.minute())
}

/// The day an entry is on: an all-day entry's own date, otherwise its start in the computer's
/// time.
pub(crate) fn entry_day(e: &CalendarEntry, tz: &TimeZone) -> Option<Date> {
    if e.all_day {
        return e.start.get(..10)?.parse().ok();
    }
    Some(
        e.start
            .parse::<Timestamp>()
            .ok()?
            .to_zoned(tz.clone())
            .date(),
    )
}

/// The last day an entry is on: an all-day entry ends the day before its end date, a timed one
/// on the day it ends, or the day before when it ends at midnight.
pub(crate) fn entry_end_day(e: &CalendarEntry, tz: &TimeZone) -> Option<Date> {
    let start = entry_day(e, tz)?;
    let end = if e.all_day {
        e.end.get(..10)?.parse::<Date>().ok()?.yesterday().ok()?
    } else {
        let z = e.end.parse::<Timestamp>().ok()?.to_zoned(tz.clone());
        let at_midnight = z.hour() == 0 && z.minute() == 0 && z.second() == 0;
        if at_midnight {
            z.date().yesterday().ok()?
        } else {
            z.date()
        }
    };
    Some(end.max(start))
}

fn valid(e: &CalendarEntry, tz: &TimeZone) -> bool {
    let short = |s: &str| s.chars().count() <= MAX_FIELD;
    let times_ok = if e.all_day {
        e.start.get(..10).is_some_and(|d| d.parse::<Date>().is_ok())
            && e.end.get(..10).is_some_and(|d| d.parse::<Date>().is_ok())
    } else {
        match (e.start.parse::<Timestamp>(), e.end.parse::<Timestamp>()) {
            (Ok(s), Ok(end)) => end >= s,
            _ => false,
        }
    };
    times_ok
        && entry_day(e, tz).is_some()
        && !e.phone_id.is_empty()
        && e.phone_id.len() <= 200
        && e.calendar_id.len() <= 200
        && !e.title.trim().is_empty()
        && short(&e.title)
        && e.place.as_deref().is_none_or(short)
        && e.notes.as_deref().is_none_or(short)
        && e.time_zone.as_deref().is_none_or(|z| z.len() <= 64)
        && e.people.len() <= MAX_PEOPLE
        && e.people.iter().all(|p| short(p))
}

/// The appointment an entry says, from its own fields.
pub(crate) fn candidate(e: &CalendarEntry, day: Date, tz: &TimeZone) -> Candidate {
    let title = e.title.trim();
    let mut object = day_words(day);
    if !e.all_day
        && let Ok(start) = e.start.parse::<Timestamp>()
    {
        object = format!("{object} at {}", time_words(start, tz));
    }
    let place = e.place.as_deref().map(str::trim).filter(|p| !p.is_empty());
    let sentence = match place {
        Some(p) => format!("{title} is on {object}, at {p}."),
        None => format!("{title} is on {object}."),
    };
    let mut c = Candidate::new(title, Predicate::Scheduled, &object, &sentence, 1.0);
    c.subject_kind = Some(EntityKind::Thing);
    c.due = Some(day);
    c.valid_from = Some(day);
    c.span = Some(Span::Quote {
        text: title.to_string(),
    });
    c.tags = vec!["appointment".to_string()];
    c
}

/// The calendars a send names. The first batch of a send lists them all, so it replaces what was
/// kept; later batches add to it. A name or colour too long to be one is left out.
fn calendars_merged(stored: &str, sent: &[PhoneCalendar], first: bool) -> Vec<PhoneCalendar> {
    let mut all: Vec<PhoneCalendar> = if first {
        Vec::new()
    } else {
        serde_json::from_str(stored).unwrap_or_default()
    };
    let fits = |c: &&PhoneCalendar| {
        !c.id.is_empty()
            && c.id.len() <= 200
            && c.name.chars().count() <= 200
            && c.colour.as_deref().is_none_or(|x| x.len() <= 40)
    };
    for c in sent.iter().filter(fits) {
        match all.iter_mut().find(|a| a.id == c.id) {
            Some(a) => *a = c.clone(),
            None => all.push(c.clone()),
        }
    }
    all
}

/// The memories an item supports that are current.
fn current_memories(tx: &Transaction<'_>, item_id: &str) -> Result<Vec<String>> {
    let mut stmt = tx.prepare(
        "SELECT m.id FROM memory_sources s JOIN memories m ON m.id = s.memory_id \
         WHERE s.item_id = ?1 AND m.superseded_at IS NULL AND m.forgetting = 0 ORDER BY m.id",
    )?;
    Ok(stmt
        .query_map([item_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

/// Whether every item a memory comes from is a calendar entry.
fn calendar_only(tx: &Transaction<'_>, memory: &str) -> Result<bool> {
    Ok(tx.query_row(
        "SELECT NOT EXISTS(SELECT 1 FROM memory_sources s JOIN items i ON i.id = s.item_id \
         WHERE s.memory_id = ?1 AND i.kind != 'calendar')",
        [memory],
        |r| r.get(0),
    )?)
}

/// Make or update the appointment for a new version. An earlier version's appointment becomes
/// its history, never a question: the phone's calendar says which is right.
fn appointment(
    tx: &Transaction<'_>,
    now: &str,
    today: &str,
    tz: &TimeZone,
    item_id: &str,
    cand: Candidate,
    earlier: Option<&str>,
) -> Result<()> {
    let old = match earlier {
        Some(e) => current_memories(tx, e)?,
        None => Vec::new(),
    };
    let (report, files) = apply_structured(tx, now, today, tz, item_id, vec![cand])?;
    crate::audio::shred_all(files);
    let new = current_memories(tx, item_id)?;
    for cid in &report.conflicts {
        // Two entries in the phone's calendar never disagree: they are two appointments.
        let pair: Option<(String, String)> = tx
            .query_row("SELECT a, b FROM conflicts WHERE id = ?1", [cid], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .ok();
        let Some((a, b)) = pair else { continue };
        let ours = |m: &str| new.iter().any(|n| n == m) || old.iter().any(|o| o == m);
        let other = if ours(&a) { &b } else { &a };
        if old.iter().any(|o| o == other) || calendar_only(tx, other)? {
            tx.execute("DELETE FROM conflicts WHERE id = ?1", [cid])?;
        }
    }
    for n in &new {
        // An entry that came back after it was cancelled is on again.
        tx.execute(
            "UPDATE memories SET valid_to = NULL WHERE id = ?1 AND valid_to IS NOT NULL",
            [n],
        )?;
        for o in old.iter().filter(|o| *o != n) {
            tx.execute(
                "UPDATE memories SET superseded_at = ?2, superseded_by = ?3 \
                 WHERE id = ?1 AND superseded_at IS NULL",
                params![o, now, n],
            )?;
            // What the person chose for the appointment holds for its new day.
            tx.execute(
                "UPDATE memories SET keep_forever = 1 WHERE id = ?1 \
                 AND EXISTS (SELECT 1 FROM memories WHERE id = ?2 AND keep_forever = 1)",
                params![n, o],
            )?;
        }
    }
    Ok(())
}

/// A title the mind already read for another entry is not read again: its mentions are copied,
/// and the entry is understood.
fn reuse_mentions(tx: &Transaction<'_>, item_id: &str, title: &str) -> Result<()> {
    let read: Option<String> = tx
        .query_row(
            "SELECT id FROM items WHERE kind = 'calendar' AND text = ?1 AND id != ?2 \
             AND status = 'understood' ORDER BY kept_at DESC, id LIMIT 1",
            params![title, item_id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(read) = read else {
        return Ok(());
    };
    let mut stmt = tx.prepare(
        "SELECT entity_id, name, start, end, ord, state FROM mentions WHERE item_id = ?1",
    )?;
    type Mention = (String, String, Option<i64>, Option<i64>, i64, String);
    let rows: Vec<Mention> = stmt
        .query_map([&read], |r| {
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
    let now: String = tx.query_row("SELECT kept_at FROM items WHERE id = ?1", [item_id], |r| {
        r.get(0)
    })?;
    for (entity, name, start, end, ord, state) in rows {
        tx.execute(
            "INSERT OR IGNORE INTO mentions(id, item_id, entity_id, name, start, end, ord, state, \
             new, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9)",
            params![
                new_id("men"),
                item_id,
                entity,
                name,
                start,
                end,
                ord,
                state,
                now
            ],
        )?;
    }
    tx.execute(
        "UPDATE items SET status = 'understood' WHERE id = ?1 AND status = 'kept'",
        [item_id],
    )?;
    Ok(())
}

/// A cancelled or deleted entry: its appointment's valid time ends, and nothing is deleted. An
/// appointment something else still says (a note, a letter, another phone's calendar) stays on.
fn close(tx: &Transaction<'_>, item_id: &str, today: &str) -> Result<()> {
    for m in current_memories(tx, item_id)? {
        let others: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM memory_sources s JOIN items i ON i.id = s.item_id \
             WHERE s.memory_id = ?1 AND s.item_id != ?2 AND (i.kind != 'calendar' OR EXISTS \
               (SELECT 1 FROM phone_records r WHERE r.item_id = i.id AND r.removed_at IS NULL)))",
            params![m, item_id],
            |r| r.get(0),
        )?;
        if others {
            continue;
        }
        tx.execute(
            "UPDATE memories SET valid_to = ?2 WHERE id = ?1 AND valid_to IS NULL",
            params![m, today],
        )?;
    }
    Ok(())
}

pub(crate) fn take(engine: &Engine, device_id: &str, batch: CalendarBatch) -> Result<SyncReceipt> {
    let now_ts = engine.clock_now();
    let now = fmt_ts(now_ts);
    let tz = engine.clock().tz();
    let today = now_ts.to_zoned(tz.clone()).date().to_string();
    let window = (
        batch.window_from.parse::<Date>().ok(),
        batch.window_to.parse::<Date>().ok(),
    );
    engine.write(|tx| {
        check_device(tx, device_id)?;
        let row = source_row(tx, device_id, PhoneSource::Calendar)?;
        if !row.taking {
            return Ok(stopped());
        }
        let first = batch.seq == 0 && !batch.calendars.is_empty();
        let calendars = calendars_merged(&row.calendars, &batch.calendars, first);
        tx.execute(
            "UPDATE phone_sources SET calendars = ?3 WHERE device_id = ?1 AND source = ?2",
            params![device_id, "calendar", serde_json::to_string(&calendars)?],
        )?;
        let mut count = SendCount::new(&row, &batch.sync_id, CALENDAR_PER_SEND);
        let mut receipt = SyncReceipt {
            taken: 0,
            unchanged: 0,
            refused: 0,
            stopped: false,
            too_many: false,
        };
        for change in batch.changes {
            if !count.fits() {
                receipt.too_many = true;
                receipt.refused += 1;
                continue;
            }
            match change {
                CalendarChange::Upsert { entry } => {
                    if !valid(&entry, &tz) {
                        receipt.refused += 1;
                        continue;
                    }
                    let Some(day) = entry_day(&entry, &tz) else {
                        receipt.refused += 1;
                        continue;
                    };
                    // The calendar's name and colour are read from the phone's list when shown,
                    // so renaming a calendar is not a change to every entry in it.
                    let content = json!({ "entry": entry });
                    let hash =
                        content_hash(device_id, PhoneSource::Calendar, &entry.phone_id, &content);
                    let before = record(tx, device_id, PhoneSource::Calendar, &entry.phone_id)?;
                    if let Some(b) = &before
                        && b.is_current(device_id, PhoneSource::Calendar, &entry.phone_id, &content)
                    {
                        receipt.unchanged += 1;
                        continue;
                    }
                    let mut meta = content;
                    meta["day"] = Value::String(day.to_string());
                    let last = entry_end_day(&entry, &tz).unwrap_or(day);
                    meta["endDay"] = Value::String(last.to_string());
                    let title = entry.title.trim().to_string();
                    let id = insert_version(
                        tx,
                        &now,
                        NewVersion {
                            device_id,
                            source: PhoneSource::Calendar,
                            phone_id: &entry.phone_id,
                            kind: "calendar",
                            title: &title,
                            // The mind reads the title for the people and places it names.
                            text: Some(&title),
                            status: "kept",
                            hash: &hash,
                            metadata: meta,
                        },
                        before.as_ref(),
                    )?;
                    let earlier = before.as_ref().map(|b| b.item_id.as_str());
                    appointment(
                        tx,
                        &now,
                        &today,
                        &tz,
                        &id,
                        candidate(&entry, day, &tz),
                        earlier,
                    )?;
                    set_replaces(tx, &id, before.as_ref())?;
                    reuse_mentions(tx, &id, &title)?;
                    receipt.taken += 1;
                }
                CalendarChange::Removed { phone_id } => {
                    let Some(b) = record(tx, device_id, PhoneSource::Calendar, &phone_id)? else {
                        receipt.unchanged += 1;
                        continue;
                    };
                    let day: Option<Date> = tx
                        .query_row(
                            "SELECT json_extract(metadata, '$.day') FROM items WHERE id = ?1",
                            [&b.item_id],
                            |r| r.get::<_, Option<String>>(0),
                        )?
                        .and_then(|d| d.parse().ok());
                    // Only an entry inside the window the phone read can have gone from it:
                    // one that aged out of the window is not removed.
                    // The phone reads its window in its own time zone: a day of margin at each
                    // edge keeps a time-zone difference from counting as inside.
                    let inside = match (window, day) {
                        ((Some(from), Some(to)), Some(d)) => from < d && d < to,
                        _ => false,
                    };
                    if b.removed || !inside {
                        receipt.unchanged += 1;
                        continue;
                    }
                    mark_removed(tx, device_id, PhoneSource::Calendar, &phone_id, &now)?;
                    close(tx, &b.item_id, &today)?;
                    receipt.taken += 1;
                }
            }
        }
        finish_send(
            tx,
            device_id,
            PhoneSource::Calendar,
            &batch.sync_id,
            &count,
            &now,
        )?;
        Ok(receipt)
    })
}
