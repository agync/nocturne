//! The look-ups of Ask (docs/design/ask-with-tools.md#the-look-ups): read-only finding by day
//! and by kind on the answering device, for the date step and for a mind that asks. Each
//! returns only lines the person kept, in the facts' own form, each with its receipt: the
//! memories kept with a day, and the person's own words of a check-in, a journal entry or a
//! good thing not read yet, and a health reading in the health wording. Nothing let go, being
//! forgotten or replaced is returned, nor a reading removed in Health; reflections and recaps
//! are never items, so never lines. Every look-up runs on a read-only connection.

use std::collections::HashSet;

use jiff::ToSpan;
use jiff::civil::{Date, Weekday};
use nocturne_contract::{
    Certainty, FeelingPart, JournalRole, MemoryStatus, MemoryView, ReadingKind, ReadingValue,
};
use rusqlite::Connection;
use serde_json::Value;

use crate::Engine;
use crate::datestep::DateStep;
use crate::error::Result;
use crate::journal::{Ctx, JItem, load_items};
use crate::rows::{mem_view, query_mems};
use crate::types::Hit;
use crate::util::enum_str;

/// What a line that is no memory, but a check-in, an entry or a reading kept as it is, is named
/// by: this, then its item's id.
pub const KEPT_LINE: &str = "kept:";

/// Whether `id` names a kept line (a check-in, an entry or a reading) rather than a memory.
pub fn is_kept_line(id: &str) -> bool {
    id.starts_with(KEPT_LINE)
}

/// The item a kept line is of.
pub fn kept_line_item(id: &str) -> Option<&str> {
    id.strip_prefix(KEPT_LINE)
}

/// The most days apart the ends of each look-up may be, both ends included: 62 apart is 63
/// days.
pub const CALENDAR_DAYS: i64 = 62;
pub const JOURNAL_DAYS: i64 = 14;
pub const HEALTH_DAYS: i64 = 31;
/// The most lines each look-up returns.
pub const CALENDAR_LINES: usize = 20;
pub const JOURNAL_LINES: usize = 20;
pub const SEARCH_LINES: usize = 12;
pub const HEALTH_LINES: usize = 31;
/// The most lines the date step adds to a question.
pub const DATE_STEP_LINES: usize = 12;
/// The most characters of an entry's own words in its line.
const ENTRY_CHARS: usize = 400;

/// The day, time and zone a question is asked in, on the answering device: the day by the
/// journal's day boundary, so at 1:30 am "today" is still the evening before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskToday {
    pub day: Date,
    pub hour: i8,
    pub minute: i8,
    /// The zone's own name, such as "Europe/London", or its offset when it has none.
    pub zone: String,
    pub week_start: Weekday,
}

/// What a look-up may return on this device. A phone alone and the computer's own windows see
/// everything the person kept; a companion phone's question gets no line of an original and
/// nothing private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookScope {
    pub originals: bool,
    pub private: bool,
}

impl LookScope {
    pub const ALL: LookScope = LookScope {
        originals: true,
        private: true,
    };
    pub const COMPANION: LookScope = LookScope {
        originals: false,
        private: false,
    };
}

/// The lines a look-up found: the first ones, in order, and how many more there were.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Looked {
    pub hits: Vec<Hit>,
    pub more: usize,
}

impl Looked {
    fn capped(mut hits: Vec<Hit>, limit: usize) -> Looked {
        let more = hits.len().saturating_sub(limit);
        hits.truncate(limit);
        Looked { hits, more }
    }
}

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// "Tuesday 24 November".
pub fn spoken_day(d: Date) -> String {
    crate::phones::calendar::day_words(d)
}

/// "Saturday 28 to Sunday 29 November", or one day as [`spoken_day`] says it.
pub fn spoken_days(from: Date, to: Date) -> String {
    if from == to {
        return spoken_day(from);
    }
    let first = spoken_day(from);
    if from.month() == to.month() && from.year() == to.year() {
        let short = first.rsplit_once(' ').map_or(first.as_str(), |(a, _)| a);
        format!("{short} to {}", spoken_day(to))
    } else {
        format!("{first} to {}", spoken_day(to))
    }
}

/// A weekday's name.
pub fn weekday_name(w: Weekday) -> &'static str {
    WEEKDAYS[w.to_monday_zero_offset() as usize]
}

/// Items that may be given: none being forgotten, none with something of it forgotten, and no
/// earlier version a later one replaced.
const KEPT_ITEM: &str = "i.forget_epoch = 0 \
     AND NOT EXISTS (SELECT 1 FROM forgotten_claims c WHERE c.item_id = i.id) \
     AND NOT EXISTS (SELECT 1 FROM forget_jobs j WHERE instr(j.items, '\"' || i.id || '\"') > 0) \
     AND NOT EXISTS (SELECT 1 FROM items n WHERE json_extract(n.metadata, '$.replaces') = i.id) \
     AND NOT EXISTS (SELECT 1 FROM phone_records r WHERE r.item_id = i.id AND r.removed_at IS NOT NULL)";

/// Memories that may be given: current, not let go and not being forgotten.
const KEPT_MEMORY: &str = "m.superseded_at IS NULL AND m.forgetting = 0 \
     AND m.status != 'letGo' AND m.let_go_at IS NULL";

/// The minutes past midnight a sentence names after "at", for ordering a day's lines.
fn minutes_in(sentence: &str) -> u32 {
    sentence
        .split(" at ")
        .skip(1)
        .find_map(|rest| {
            let t: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == ':')
                .collect();
            let (h, m) = t.split_once(':')?;
            Some(h.parse::<u32>().ok()? * 60 + m.parse::<u32>().ok()?)
        })
        .unwrap_or(0)
}

fn hit(memory: MemoryView) -> Hit {
    Hit {
        memory,
        score: 0.0,
        let_go: false,
    }
}

/// A kept line of `item`, saying `line`.
fn kept_hit(item: &JItem, line: String) -> Hit {
    hit(MemoryView {
        id: format!("{KEPT_LINE}{}", item.view.id),
        sentence: line.clone(),
        subject: item.view.title.clone(),
        entity_id: None,
        predicate: "kept".into(),
        object: line,
        certainty: Certainty::Sure,
        status: MemoryStatus::Active,
        valid_from: None,
        valid_to: None,
        due: None,
        learned_at: item.view.kept_at.clone(),
        last_used: None,
        let_go_at: None,
        keep_forever: false,
        private: item.view.private,
        was: None,
        moved_from: None,
        receipts: vec![item.receipt()],
    })
}

fn part_name(part: FeelingPart) -> &'static str {
    match part {
        FeelingPart::WoundUp => "Wound up",
        FeelingPart::LitUp => "Lit up",
        FeelingPart::Low => "Low",
        FeelingPart::Settled => "Settled",
    }
}

fn clip(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= ENTRY_CHARS {
        return text;
    }
    let cut: String = text.chars().take(ENTRY_CHARS).collect();
    format!("{}…", cut.trim_end())
}

fn time_of(ctx: &Ctx, at: &str) -> Option<String> {
    let t: jiff::Timestamp = at.parse().ok()?;
    let z = t.to_zoned_datetime(ctx.tz.clone());
    Some(format!("{}:{:02}", z.hour(), z.minute()))
}

/// "Check-in, Monday 23 November at 21:10: drained (Low). "Leo up twice again.""
fn checkin_line(ctx: &Ctx, item: &JItem, day: Date) -> String {
    let feeling = &item.meta["feeling"];
    let part = serde_json::from_value::<FeelingPart>(feeling["part"].clone())
        .map(part_name)
        .unwrap_or("");
    let words: Vec<&str> = feeling["words"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let when = match time_of(ctx, &item.view.kept_at) {
        Some(t) => format!("{} at {t}", spoken_day(day)),
        None => spoken_day(day),
    };
    let felt = if words.is_empty() {
        part.to_string()
    } else {
        format!("{} ({part})", words.join(", "))
    };
    match item
        .meta_str("line")
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        Some(said) => format!("Check-in, {when}: {felt}. \"{said}\""),
        None => format!("Check-in, {when}: {felt}."),
    }
}

/// A duration in hours and minutes as the health wording writes it: "6 h 40 min".
fn duration(seconds: i64) -> String {
    let minutes = (seconds + 30) / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

fn number(n: f64) -> String {
    if (n - n.round()).abs() < 0.05 {
        format!("{}", n.round() as i64)
    } else {
        format!("{n:.1}")
    }
}

/// A reading as one line in the health wording, with its source: no judgement, no norm.
fn reading_line(ctx: &Ctx, item: &JItem) -> Option<String> {
    let value: ReadingValue = serde_json::from_value(item.meta["reading"].clone()).ok()?;
    let day: Date = item.meta_str("day")?.get(..10)?.parse().ok()?;
    let source = match &item.view.from_device {
        Some(phone) => format!("Health on {phone}"),
        None => "Health".to_string(),
    };
    let at = item
        .meta_str("start")
        .and_then(|s| time_of(ctx, s))
        .map(|t| format!(" at {t}"))
        .unwrap_or_default();
    let on = spoken_day(day);
    let said = match value {
        ReadingValue::Sleep {
            asleep,
            in_bed_only,
        } => {
            let seconds: i64 = asleep
                .iter()
                .filter_map(|s| {
                    let a: jiff::Timestamp = s.from.parse().ok()?;
                    let b: jiff::Timestamp = s.to.parse().ok()?;
                    Some(b.as_second() - a.as_second())
                })
                .sum();
            let what = if in_bed_only { "in bed" } else { "asleep" };
            format!("Night to {on}: {} {what}", duration(seconds))
        }
        ReadingValue::HeartRate { hours, .. } => {
            let low = hours.iter().map(|h| h.low).fold(f64::INFINITY, f64::min);
            let high = hours.iter().map(|h| h.high).fold(0.0, f64::max);
            if hours.is_empty() {
                return None;
            }
            format!(
                "{on}: heart rate {} to {} beats a minute",
                number(low),
                number(high)
            )
        }
        ReadingValue::RestingHeartRate { bpm } => {
            format!("{on}: resting heart rate {} beats a minute", number(bpm))
        }
        ReadingValue::BloodPressure { top, bottom } => format!(
            "{on}{at}: blood pressure {} over {}",
            number(top),
            number(bottom)
        ),
        ReadingValue::Steps { total } => format!("{on}: {total} steps"),
        ReadingValue::Workout {
            kind,
            length_s,
            distance_m,
        } => {
            let far = distance_m
                .map(|m| format!(", {} km", number(m / 1000.0)))
                .unwrap_or_default();
            format!(
                "{on}{at}: {}, {}{far}",
                kind.trim(),
                duration(i64::from(length_s))
            )
        }
        ReadingValue::Weight { value, unit } => {
            format!("{on}{at}: weight {} {}", number(value), unit.trim())
        }
    };
    Some(format!("{said} ({source})."))
}

/// "Journal, Tuesday 17 November: ..." or "Good thing, ...: ...", in the person's own words.
fn writing_line(item: &JItem, day: Date) -> Option<String> {
    let text = item.view.text.as_deref().filter(|t| !t.trim().is_empty())?;
    let what = match item.role() {
        Some(JournalRole::Good) => "Good thing",
        Some(JournalRole::Entry) => "Journal",
        _ => return None,
    };
    Some(format!("{what}, {}: {}", spoken_day(day), clip(text)))
}

/// A kept line as it reads now, for showing what was sent: `None` once its item is gone, being
/// forgotten or replaced.
pub(crate) fn kept_line_now(
    conn: &Connection,
    clock: &dyn crate::Clock,
    originals: &std::path::Path,
    id: &str,
) -> Result<Option<String>> {
    let Some(item_id) = kept_line_item(id) else {
        return Ok(None);
    };
    let ctx = Ctx::load(conn, clock)?;
    let Some(item) = load_items(
        conn,
        originals,
        &format!("i.id = ?1 AND {KEPT_ITEM}"),
        &[item_id.to_string()],
    )?
    .pop() else {
        return Ok(None);
    };
    if item.view.kind == nocturne_contract::ItemKind::Reading {
        return Ok(reading_line(&ctx, &item));
    }
    let day = item.day(&ctx)?;
    Ok(match item.role() {
        Some(JournalRole::Checkin) => Some(checkin_line(&ctx, &item, day)),
        _ => writing_line(&item, day),
    })
}

fn day_text(d: Date) -> String {
    d.to_string()
}

fn memories_on(
    conn: &Connection,
    from: Date,
    to: Date,
    scope: LookScope,
) -> Result<Vec<MemoryView>> {
    let rows = query_mems(
        conn,
        &format!(
            "{KEPT_MEMORY} AND (substr(COALESCE(m.due, ''), 1, 10) BETWEEN ?1 AND ?2 \
             OR substr(COALESCE(m.valid_from, ''), 1, 10) BETWEEN ?1 AND ?2) \
             AND NOT EXISTS (SELECT 1 FROM memory_sources s JOIN phone_records r \
             ON r.item_id = s.item_id WHERE s.memory_id = m.id AND r.removed_at IS NOT NULL) \
             ORDER BY m.recorded_at, m.id"
        ),
        rusqlite::params![day_text(from), day_text(to)],
    )?;
    let mut out = Vec::new();
    for row in &rows {
        let view = mem_view(conn, row)?;
        if (!scope.private && view.private) || view.receipts.is_empty() {
            continue;
        }
        out.push(view);
    }
    Ok(out)
}

fn day_of_memory(m: &MemoryView) -> String {
    m.due
        .as_deref()
        .or(m.valid_from.as_deref())
        .map(|d| d.get(..10).unwrap_or(d).to_string())
        .unwrap_or_default()
}

impl Engine {
    /// The day, time and zone a question is asked in, here.
    pub fn ask_today(&self) -> Result<AskToday> {
        let ctx = self.read(|conn| Ctx::load(conn, self.clock()))?;
        let now = ctx.now.to_zoned_datetime(ctx.tz.clone());
        let zone = match ctx.tz.iana_name() {
            Some(name) => name.to_string(),
            None => format!("UTC{}", now.offset()),
        };
        Ok(AskToday {
            day: ctx.today()?,
            hour: now.hour(),
            minute: now.minute(),
            zone,
            week_start: Weekday::Monday,
        })
    }

    /// The appointments and everything kept with a day from `from` to `to`, in order of day
    /// and time.
    pub fn look_up_calendar(
        &self,
        from: Date,
        to: Date,
        scope: LookScope,
        limit: usize,
    ) -> Result<Looked> {
        let mut found = self.read(|conn| memories_on(conn, from, to, scope))?;
        found.sort_by_key(|m| (day_of_memory(m), minutes_in(&m.sentence)));
        Ok(Looked::capped(found.into_iter().map(hit).collect(), limit))
    }

    /// What the person kept and wrote on the journal days `from` to `to`: each check-in as one
    /// line in their words, each entry and good thing as what was read from it or, while it is
    /// unread, its own words, and what was read from everything else kept those days.
    pub fn look_up_journal(
        &self,
        from: Date,
        to: Date,
        scope: LookScope,
        limit: usize,
    ) -> Result<Looked> {
        let originals = self.originals_dir();
        let hits = self.read(|conn| {
            let ctx = Ctx::load(conn, self.clock())?;
            let items = crate::journal::items_in_days(conn, &ctx, &originals, from, to)?;
            let mut out = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            for item in items {
                if !kept_chain(conn, &item.view.id)? || (!scope.private && item.view.private) {
                    continue;
                }
                let day = item.day(&ctx)?;
                let role = item.role();
                if role == Some(JournalRole::Checkin) {
                    out.push(kept_hit(&item, checkin_line(&ctx, &item, day)));
                    continue;
                }
                let memories = query_mems(
                    conn,
                    &format!(
                        "{KEPT_MEMORY} AND m.id IN (SELECT memory_id FROM memory_sources \
                         WHERE item_id = ?1) ORDER BY m.recorded_at, m.id"
                    ),
                    [&item.view.id],
                )?;
                let read_any = item.view.memory_count > 0;
                for row in &memories {
                    if seen.insert(row.id.clone()) {
                        let view = mem_view(conn, row)?;
                        if scope.private || !view.private {
                            out.push(hit(view));
                        }
                    }
                }
                // An entry or a good thing not read yet is given in its own words.
                if !read_any
                    && scope.originals
                    && matches!(role, Some(JournalRole::Entry | JournalRole::Good))
                    && let Some(line) = writing_line(&item, day)
                {
                    out.push(kept_hit(&item, line));
                }
            }
            Ok(out)
        })?;
        Ok(Looked::capped(hits, limit))
    }

    /// The readings of `kind` on the days `from` to `to`, one line each in the health wording.
    pub fn look_up_health(
        &self,
        kind: ReadingKind,
        from: Date,
        to: Date,
        scope: LookScope,
        limit: usize,
    ) -> Result<Looked> {
        let originals = self.originals_dir();
        let hits = self.read(|conn| {
            let ctx = Ctx::load(conn, self.clock())?;
            let found = load_items(
                conn,
                &originals,
                &format!(
                    "i.kind = 'reading' AND json_extract(i.metadata, '$.reading.type') = ?1 \
                     AND substr(json_extract(i.metadata, '$.day'), 1, 10) BETWEEN ?2 AND ?3 \
                     AND {KEPT_ITEM}"
                ),
                &[enum_str(&kind), day_text(from), day_text(to)],
            )?;
            let mut items: Vec<JItem> = Vec::new();
            for i in found {
                if (scope.private || !i.view.private) && kept_chain(conn, &i.view.id)? {
                    items.push(i);
                }
            }
            items.sort_by(|a, b| {
                (a.meta_str("day"), a.meta_str("start"))
                    .cmp(&(b.meta_str("day"), b.meta_str("start")))
            });
            Ok(items
                .iter()
                .filter_map(|i| reading_line(&ctx, i).map(|line| kept_hit(i, line)))
                .collect())
        })?;
        Ok(Looked::capped(hits, limit))
    }

    /// The kinds of reading kept here, so only those are offered.
    pub fn reading_kinds(&self) -> Result<Vec<ReadingKind>> {
        self.read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT DISTINCT json_extract(i.metadata, '$.reading.type') FROM items i \
                 WHERE i.kind = 'reading' AND {KEPT_ITEM}"
            ))?;
            let names: Vec<Option<String>> = stmt
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let mut kinds: Vec<ReadingKind> = names
                .into_iter()
                .flatten()
                .filter_map(|n| serde_json::from_value(Value::String(n)).ok())
                .collect();
            kinds.sort_by_key(enum_str);
            Ok(kinds)
        })
    }

    /// The lines the date step adds for `step` on `today`: for each range, the readings of the
    /// kinds named, the calendar, and the journal for its days up to today, each within its own
    /// limit; at most `limit` in all, one copy of each.
    pub fn date_step_lines(
        &self,
        step: &DateStep,
        today: Date,
        scope: LookScope,
        limit: usize,
    ) -> Result<Vec<Hit>> {
        let mut out: Vec<Hit> = Vec::new();
        let add = |found: Looked, out: &mut Vec<Hit>| {
            for h in found.hits {
                if !out.iter().any(|o| o.memory.id == h.memory.id) {
                    out.push(h);
                }
            }
        };
        for &(from, to) in &step.ranges {
            let to_health = clamp(from, to, HEALTH_DAYS).min(today);
            for kind in &step.kinds {
                if from <= today {
                    add(
                        self.look_up_health(*kind, from, to_health, scope, HEALTH_LINES)?,
                        &mut out,
                    );
                }
            }
            add(
                self.look_up_calendar(from, clamp(from, to, CALENDAR_DAYS), scope, CALENDAR_LINES)?,
                &mut out,
            );
            if from <= today {
                let end = to.min(today);
                let start = from.max(end.checked_sub(JOURNAL_DAYS.days()).unwrap_or(from));
                add(
                    self.look_up_journal(start, end, scope, JOURNAL_LINES)?,
                    &mut out,
                );
            }
        }
        out.truncate(limit);
        Ok(out)
    }

    /// What finding by `words` gives a look-up: Ask's own finding by words, with nothing let go
    /// or being forgotten.
    pub fn look_up_words(&self, words: &str, scope: LookScope, limit: usize) -> Result<Looked> {
        let found = self.retrieve(words, None, limit + 1)?;
        let ids: Vec<String> = found.iter().map(|h| h.memory.id.clone()).collect();
        let may = self.mind_may_read(&ids)?;
        let hits = found
            .into_iter()
            .zip(may)
            .filter(|(h, may)| *may && !h.let_go && (scope.private || !h.memory.private))
            .map(|(h, _)| h)
            .collect();
        Ok(Looked::capped(hits, limit))
    }
}

/// `to`, or the last day at most `days` apart from `from`.
fn clamp(from: Date, to: Date, days: i64) -> Date {
    to.min(from.checked_add(days.days()).unwrap_or(to))
}

/// Whether the item and every earlier version it replaced may be given ([`kept_line_may_go`]).
fn kept_chain(conn: &Connection, item_id: &str) -> Result<bool> {
    kept_line_may_go(conn, &format!("{KEPT_LINE}{item_id}"))
}

fn kept_item(conn: &Connection, id: &str) -> Result<bool> {
    let sql = format!("SELECT EXISTS (SELECT 1 FROM items i WHERE i.id = ?1 AND {KEPT_ITEM})");
    Ok(conn.query_row(&sql, [id], |r| r.get(0))?)
}

/// Whether the kept line `id` may still be given or sent: its item exists and passes the rules
/// every look-up applies (not being forgotten, in no forget job, nothing of it forgotten, not
/// replaced, not removed in Health), and so does every earlier version it replaced. Anything
/// else, an id that is no kept line included, may not.
pub(crate) fn kept_line_may_go(conn: &Connection, id: &str) -> Result<bool> {
    let Some(item) = kept_line_item(id) else {
        return Ok(false);
    };
    if !kept_item(conn, item)? {
        return Ok(false);
    }
    let sql = "WITH RECURSIVE chain(id) AS ( \
         SELECT json_extract(metadata, '$.replaces') FROM items WHERE id = ?1 \
         UNION SELECT json_extract(i.metadata, '$.replaces') FROM items i JOIN chain c ON i.id = c.id) \
         SELECT COUNT(*) FROM chain c JOIN items i ON i.id = c.id WHERE i.forget_epoch != 0 \
         OR EXISTS (SELECT 1 FROM forgotten_claims f WHERE f.item_id = i.id) \
         OR EXISTS (SELECT 1 FROM forget_jobs j WHERE instr(j.items, '\"' || i.id || '\"') > 0)";
    let bad: i64 = conn.query_row(sql, [item], |r| r.get(0))?;
    Ok(bad == 0)
}

#[cfg(test)]
#[path = "lookup_tests.rs"]
mod tests;
