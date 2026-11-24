//! The journal day: its entry, timeline, check-ins, good things and the names found in them.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use jiff::civil::Date;
use nocturne_contract::{
    CheckIn, DayPart, EarlierVersion, EntryBlock, GoodThing, ItemKind, ItemView, JournalAnswer,
    JournalDay, JournalRole, Mention, MentionState, NameRef, Period, PeriodKind, PeriodSummary,
    RowKind, Source, TimelineRow,
};
use rusqlite::Connection;

use super::mentions::mentions_for_items;
use super::{Ctx, INTRO_KEY, JItem, current, items_in_days, load_item, load_items};
use crate::Engine;
use crate::error::{Error, Result};
use crate::util::{parse_date, parse_ts};

pub(crate) fn mentions_of(
    conn: &Connection,
    ids: &[String],
) -> Result<HashMap<String, Vec<Mention>>> {
    mentions_for_items(conn, ids)
}

pub(crate) fn checkin_view(
    item: &JItem,
    mentions: &HashMap<String, Vec<Mention>>,
) -> Result<CheckIn> {
    let feeling = item
        .feeling()
        .ok_or_else(|| Error::invalid(format!("check-in {} has no feeling", item.id())))?;
    Ok(CheckIn {
        id: item.id().to_string(),
        kept_at: item.view.kept_at.clone(),
        day: item.day_text().unwrap_or_default().to_string(),
        feeling,
        line: item.meta_str("line").map(str::to_string),
        urge: item.meta_str("urge").map(str::to_string),
        about: item.about().map(str::to_string),
        tried: item
            .meta
            .get("tried")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        from_device: item.view.from_device.clone(),
        mentions: mentions.get(item.id()).cloned().unwrap_or_default(),
        left_out: item.left_out,
        after_sleep: None,
    })
}

pub(crate) fn good_view(
    conn: &Connection,
    originals: &Path,
    item: &JItem,
    mentions: &HashMap<String, Vec<Mention>>,
) -> Result<GoodThing> {
    let photo = match item.meta_str("photo") {
        Some(id) => crate::rows::get_item_view(conn, originals, id)?,
        None => None,
    };
    Ok(GoodThing {
        id: item.id().to_string(),
        text: item.view.text.clone().unwrap_or_default(),
        day: item.day_text().unwrap_or_default().to_string(),
        kept_at: item.view.kept_at.clone(),
        photo,
        from_device: item.view.from_device.clone(),
        mentions: mentions.get(item.id()).cloned().unwrap_or_default(),
        left_out: item.left_out,
    })
}

/// An entry block with its earlier versions, newest first.
pub(crate) fn entry_block(conn: &Connection, originals: &Path, item: &JItem) -> Result<EntryBlock> {
    let mentions = mentions_of(conn, &[item.id().to_string()])?;
    let mut earlier = Vec::new();
    let mut cur = item.replaces().map(str::to_string);
    while let Some(id) = cur {
        let Ok(old) = load_item(conn, originals, &id) else {
            break;
        };
        earlier.push(EarlierVersion {
            item_id: old.id().to_string(),
            text: old.view.text.clone(),
            kept_at: old.view.kept_at.clone(),
        });
        cur = old.replaces().map(str::to_string);
    }
    Ok(EntryBlock {
        item: item.view.clone(),
        block: item.block(),
        mentions: mentions.get(item.id()).cloned().unwrap_or_default(),
        changed_at: item.replaces().map(|_| item.view.kept_at.clone()),
        earlier,
        alt: item.meta_str("alt").map(str::to_string),
    })
}

/// When a block was read: the stamp the reading left, else when its memories were made, else
/// when it was last saved.
fn read_at(conn: &Connection, item: &JItem) -> Result<String> {
    if let Some(t) = item.meta_str("read_at") {
        return Ok(t.to_string());
    }
    let learned: Option<String> = conn.query_row(
        "SELECT MAX(m.recorded_at) FROM memories m \
         JOIN memory_sources s ON s.memory_id = m.id WHERE s.item_id = ?1",
        [item.id()],
        |r| r.get(0),
    )?;
    Ok(learned.unwrap_or_else(|| item.saved_at().to_string()))
}

pub(crate) fn is_background(item: &JItem) -> bool {
    item.role().is_none()
        && (matches!(item.view.kind, ItemKind::Document | ItemKind::Web)
            || matches!(item.view.source, Source::Screenshot | Source::WatchedFolder))
}

/// What an answer to the page's question from the journal says: the choice, the line once added
/// and the block and sentence it was about.
fn answer_view(item: &JItem) -> Option<JournalAnswer> {
    let choice = serde_json::from_value(item.meta.get("choice")?.clone()).ok()?;
    Some(JournalAnswer {
        choice,
        line: item
            .view
            .text
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string),
        about: item.about()?.to_string(),
        sentence: item.meta_str("sentence")?.to_string(),
    })
}

fn row_kind(item: &JItem) -> RowKind {
    match item.role() {
        Some(JournalRole::Checkin) => RowKind::Checkin,
        Some(JournalRole::Good) => RowKind::Good,
        Some(JournalRole::Entry) => RowKind::Entry,
        Some(JournalRole::Answer) => RowKind::Answer,
        None if is_background(item) => RowKind::Elsewhere,
        None => match item.view.kind {
            ItemKind::Audio => RowKind::Voice,
            ItemKind::Image => RowKind::Photo,
            _ => RowKind::Note,
        },
    }
}

fn day_part(ctx: &Ctx, day: Date, at: &str) -> Result<DayPart> {
    let local = parse_ts(at)?.to_zoned_datetime(ctx.tz.clone());
    if local.date() > day {
        return Ok(DayPart::AfterMidnight);
    }
    Ok(match local.hour() {
        0..=11 => DayPart::Morning,
        12..=16 => DayPart::Afternoon,
        _ => DayPart::Evening,
    })
}

/// The names the items mention, once per person, place or thing: people, then places and
/// things, each in the order they first appear. Removed mentions are not names.
pub(crate) fn names_in<'a>(
    items: impl IntoIterator<Item = &'a JItem>,
    mentions: &HashMap<String, Vec<Mention>>,
) -> Vec<NameRef> {
    let mut out: Vec<NameRef> = Vec::new();
    for item in items {
        for m in mentions.get(item.id()).into_iter().flatten() {
            if m.state == MentionState::Removed {
                continue;
            }
            match out.iter_mut().find(|n| n.entity_id == m.entity_id) {
                Some(n) => {
                    n.new |= m.new;
                    if !n.item_ids.contains(&m.item_id) {
                        n.item_ids.push(m.item_id.clone());
                    }
                }
                None => out.push(NameRef {
                    entity_id: m.entity_id.clone(),
                    name: m.name.clone(),
                    kind: m.kind,
                    role: m.role.clone(),
                    new: m.new,
                    item_ids: vec![m.item_id.clone()],
                }),
            }
        }
    }
    let (people, rest): (Vec<_>, Vec<_>) = out
        .into_iter()
        .partition(|n| n.kind == nocturne_contract::EntityKind::Person);
    people.into_iter().chain(rest).collect()
}

/// Every id the mentions of `items` need.
pub(crate) fn ids_of<'a>(items: impl IntoIterator<Item = &'a JItem>) -> Vec<String> {
    items.into_iter().map(|i| i.id().to_string()).collect()
}

pub(crate) fn period_of(kind: PeriodKind, day: Date) -> Result<Period> {
    let (start, end) = match kind {
        PeriodKind::Day => (day, day),
        PeriodKind::Week => {
            let back = i64::from(day.weekday().to_monday_zero_offset());
            let start = day.checked_sub(jiff::Span::new().days(back))?;
            (start, start.checked_add(jiff::Span::new().days(6))?)
        }
        PeriodKind::Month => (day.first_of_month(), day.last_of_month()),
        PeriodKind::Year => (day.first_of_year(), day.last_of_year()),
    };
    Ok(Period {
        kind,
        start: start.to_string(),
        end: end.to_string(),
    })
}

pub(crate) fn period_days(p: &Period) -> Result<(Date, Date)> {
    Ok((parse_date(&p.start)?, parse_date(&p.end)?))
}

/// The period of the same kind just before this one.
pub(crate) fn previous_period(p: &Period) -> Result<Period> {
    let (start, _) = period_days(p)?;
    period_of(p.kind, start.yesterday()?)
}

/// One journal day. `with_phones` adds what phones brought (rows, "From Health" and the groups
/// "Delete this day" offers); the export leaves it out, having its own files for it.
pub(crate) fn build_day(
    conn: &Connection,
    ctx: &Ctx,
    originals: &Path,
    day: Date,
    with_phones: bool,
) -> Result<JournalDay> {
    let all = items_in_days(conn, ctx, originals, day, day)?;
    let items = current(&all);
    let mentions = mentions_of(conn, &ids_of(items.iter().copied()))?;

    let mut blocks: Vec<&JItem> = items
        .iter()
        .copied()
        .filter(|i| i.role() == Some(JournalRole::Entry))
        .collect();
    blocks.sort_by(|a, b| {
        a.block()
            .cmp(&b.block())
            .then_with(|| a.view.kept_at.cmp(&b.view.kept_at))
    });
    let entry: Vec<EntryBlock> = blocks
        .iter()
        .map(|b| entry_block(conn, originals, b))
        .collect::<Result<_>>()?;
    let entry_saved_at = blocks.iter().map(|b| b.saved_at().to_string()).max();
    let entry_read_at = if !blocks.is_empty() && blocks.iter().all(|b| !b.pending()) {
        blocks
            .iter()
            .map(|b| read_at(conn, b))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .max()
    } else {
        None
    };

    let checkin_items: Vec<&JItem> = items
        .iter()
        .copied()
        .filter(|i| i.role() == Some(JournalRole::Checkin))
        .collect();
    let checkins: Vec<CheckIn> = checkin_items
        .iter()
        .map(|i| checkin_view(i, &mentions))
        .collect::<Result<_>>()?;
    let good: Vec<GoodThing> = items
        .iter()
        .copied()
        .filter(|i| i.role() == Some(JournalRole::Good))
        .map(|i| good_view(conn, originals, i, &mentions))
        .collect::<Result<_>>()?;

    let row_ids: std::collections::HashSet<&str> = items
        .iter()
        .filter(|i| i.role() != Some(JournalRole::Entry) && i.role() != Some(JournalRole::Checkin))
        .map(|i| i.id())
        .collect();
    let mut rows: Vec<(String, String, TimelineRow)> = Vec::new();
    for item in items.iter().copied() {
        let kind = row_kind(item);
        let (at, view): (String, ItemView) = match kind {
            RowKind::Entry => continue,
            RowKind::Checkin if item.about().is_some_and(|a| row_ids.contains(a)) => continue,
            _ => (item.view.kept_at.clone(), item.view.clone()),
        };
        let checkin = (kind == RowKind::Checkin)
            .then(|| checkin_view(item, &mentions))
            .transpose()?;
        let good_thing = (kind == RowKind::Good)
            .then(|| good_view(conn, originals, item, &mentions))
            .transpose()?;
        let feeling = checkin_items
            .iter()
            .find(|c| c.about() == Some(item.id()))
            .map(|c| checkin_view(c, &mentions))
            .transpose()?;
        let answer = (kind == RowKind::Answer)
            .then(|| answer_view(item))
            .flatten();
        let move_to = (kind == RowKind::Note && item.view.kind == ItemKind::Text)
            .then(|| -> Result<String> {
                Ok(match item.day_text() {
                    Some(_) => ctx.day_of(parse_ts(&item.view.kept_at)?)?.to_string(),
                    None => day.yesterday()?.to_string(),
                })
            })
            .transpose()?;
        rows.push((
            at.clone(),
            item.id().to_string(),
            TimelineRow {
                kind,
                part: day_part(ctx, day, &at)?,
                item: view,
                checkin,
                good: good_thing,
                feeling,
                mentions: mentions.get(item.id()).cloned().unwrap_or_default(),
                left_out: item.left_out,
                move_to,
                answer,
            },
        ));
    }
    if let Some(latest) = blocks.iter().max_by(|a, b| a.saved_at().cmp(b.saved_at())) {
        let at = latest.saved_at().to_string();
        rows.push((
            at.clone(),
            latest.id().to_string(),
            TimelineRow {
                kind: RowKind::Entry,
                part: day_part(ctx, day, &at)?,
                item: latest.view.clone(),
                checkin: None,
                good: None,
                feeling: None,
                mentions: Vec::new(),
                left_out: latest.left_out,
                move_to: None,
                answer: None,
            },
        ));
    }
    let health = if with_phones {
        crate::phones::read::day_health(conn, &ctx.tz, day)?
    } else {
        None
    };
    if with_phones {
        rows.extend(phone_rows(conn, ctx, originals, day, health.as_ref())?);
    }
    rows.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    // Last night beside the day's first check-in, with its receipt.
    let after_sleep =
        health
            .as_ref()
            .and_then(|h| h.night.as_ref())
            .map(|n| nocturne_contract::AfterSleep {
                minutes: n.asleep_min,
                item_id: n.item_id.clone(),
            });
    let mut checkins = checkins;
    if let (Some(a), Some(first)) = (&after_sleep, checkins.first_mut()) {
        first.after_sleep = Some(a.clone());
        for (_, _, row) in rows.iter_mut() {
            if let Some(c) = row.checkin.as_mut().filter(|c| c.id == first.id) {
                c.after_sleep = Some(a.clone());
            }
        }
    }

    let names = names_in(
        items.iter().copied().filter(|i| !is_background(i)),
        &mentions,
    );
    let reading = items.iter().filter(|i| i.pending()).count() as u32;
    let day_ids: Vec<String> = items.iter().map(|i| i.id().to_string()).collect();
    let same_name = super::mentions::open_questions(conn, Some(&day_ids))?;
    let first_use: bool = conn.query_row(
        "SELECT NOT EXISTS(SELECT 1 FROM settings WHERE key = ?1)",
        [INTRO_KEY],
        |r| r.get(0),
    )?;
    Ok(JournalDay {
        day: day.to_string(),
        today: day == ctx.today()?,
        first_use,
        entry,
        entry_saved_at,
        entry_read_at,
        rows: rows.into_iter().map(|r| r.2).collect(),
        checkins,
        good,
        people: names,
        reflection: super::lookback::summary(conn, ctx, originals, day)?,
        reading,
        same_name,
        health,
        from_phones: if with_phones {
            crate::phones::read::day_brought(conn, day)?
        } else {
            None
        },
    })
}

/// What phones brought for the day, as quiet rows: appointments from the calendar at their
/// time (they are plans, not things kept), workouts at their start, and "Woke" when last night
/// ended.
fn phone_rows(
    conn: &Connection,
    ctx: &Ctx,
    originals: &Path,
    day: Date,
    health: Option<&nocturne_contract::DayHealth>,
) -> Result<Vec<(String, String, TimelineRow)>> {
    let mut wanted: Vec<(String, String, RowKind)> = Vec::new();
    for c in crate::phones::read::calendar_entries(conn, day)? {
        let starts_today = crate::phones::calendar::entry_day(&c.entry, &ctx.tz) == Some(day);
        let at = if c.entry.all_day || !starts_today {
            crate::util::fmt_ts(day.to_zoned_datetime(ctx.tz.clone())?.timestamp())
        } else {
            crate::util::fmt_ts(parse_ts(&c.entry.start)?)
        };
        wanted.push((at, c.item_id, RowKind::Calendar));
    }
    if let Some(h) = health {
        for w in &h.workouts {
            wanted.push((
                crate::util::fmt_ts(parse_ts(&w.start)?),
                w.item_id.clone(),
                RowKind::Workout,
            ));
        }
        if let Some(n) = &h.night {
            wanted.push((
                crate::util::fmt_ts(parse_ts(&n.to)?),
                n.item_id.clone(),
                RowKind::Woke,
            ));
        }
    }
    let mut out = Vec::new();
    for (at, id, kind) in wanted {
        let Some(view) = crate::rows::get_item_view(conn, originals, &id)? else {
            continue;
        };
        out.push((
            at.clone(),
            id,
            TimelineRow {
                kind,
                part: day_part(ctx, day, &at)?,
                item: view,
                checkin: None,
                good: None,
                feeling: None,
                mentions: Vec::new(),
                left_out: false,
                move_to: None,
                answer: None,
            },
        ));
    }
    Ok(out)
}

/// Every journal day with something kept on it, newest first.
pub(crate) fn days_with_items(conn: &Connection, ctx: &Ctx) -> Result<Vec<Date>> {
    let mut stmt = conn.prepare(
        "SELECT json_extract(metadata, '$.journal_day'), kept_at FROM items \
             WHERE kind NOT IN ('calendar', 'reading')",
    )?;
    let mut days: BTreeSet<Date> = BTreeSet::new();
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (day, kept) = row?;
        days.insert(match day {
            Some(d) => parse_date(&d)?,
            None => ctx.day_of(parse_ts(&kept)?)?,
        });
    }
    Ok(days.into_iter().rev().collect())
}

impl Engine {
    /// The journal day that is going now.
    pub fn journal_today(&self) -> Result<Date> {
        self.journal_day_of(self.clock_now())
    }

    /// The journal day an instant belongs to.
    pub fn journal_day_of(&self, t: jiff::Timestamp) -> Result<Date> {
        self.read(|conn| Ctx::load(conn, self.clock())?.day_of(t))
    }

    /// The instants a journal day runs between: `[start, end)`.
    pub fn journal_bounds(&self, day: Date) -> Result<(jiff::Timestamp, jiff::Timestamp)> {
        self.read(|conn| Ctx::load(conn, self.clock())?.bounds(day))
    }

    /// One day as the journal opens it.
    pub fn journal_day(&self, day: Date) -> Result<JournalDay> {
        let originals = self.originals_dir();
        self.read(|conn| {
            let ctx = Ctx::load(conn, self.clock())?;
            if day > ctx.today()? {
                return Err(Error::invalid("that day has not come yet"));
            }
            build_day(conn, &ctx, &originals, day, true)
        })
    }

    /// Every day with something kept on it, newest first.
    pub fn journal_days(&self) -> Result<Vec<Date>> {
        self.read(|conn| days_with_items(conn, &Ctx::load(conn, self.clock())?))
    }

    /// The day, week (Monday to Sunday), month or year that holds `day`.
    pub fn period_of(&self, kind: PeriodKind, day: Date) -> Result<Period> {
        period_of(kind, day)
    }

    /// Periods with anything kept in them, newest first, and whether each was looked back on.
    pub fn periods(&self, kind: PeriodKind) -> Result<Vec<PeriodSummary>> {
        self.read(|conn| {
            let ctx = Ctx::load(conn, self.clock())?;
            let mut seen: Vec<Period> = Vec::new();
            for day in days_with_items(conn, &ctx)? {
                let p = period_of(kind, day)?;
                if !seen.contains(&p) {
                    seen.push(p);
                }
            }
            seen.into_iter()
                .map(|period| {
                    let written = super::store::exists(conn, &period)?;
                    Ok(PeriodSummary { period, written })
                })
                .collect()
        })
    }

    /// The days an entity is mentioned on, newest first.
    pub fn entity_journal_days(&self, entity_id: &str) -> Result<Vec<Date>> {
        let originals = self.originals_dir();
        self.read(|conn| {
            let ctx = Ctx::load(conn, self.clock())?;
            let items = load_items(
                conn,
                &originals,
                "i.id IN (SELECT item_id FROM mentions WHERE entity_id = ?1 AND state != 'removed')",
                &[entity_id.to_string()],
            )?;
            let days: BTreeSet<Date> = items
                .iter()
                .map(|i| i.day(&ctx))
                .collect::<Result<_>>()?;
            Ok(days.into_iter().rev().collect())
        })
    }
}
