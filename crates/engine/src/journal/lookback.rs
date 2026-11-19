//! Looking back: a day at a glance, what a reflection is written from, recaps composed from what
//! was kept, and the stored writing.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use nocturne_contract::{
    CheckIn, Compare, DayCount, DayReflection, Feeling, FeelingPart, Glance, GlanceKind, GoodThing,
    Highlight, ItemKind, JournalRole, KeptCount, MemoryView, Mention, MentionState, NameRef,
    Period, PeriodKind, Recap, Receipt, ReflectionLine, ReflectionSummary, ReflectionView,
    Together, Tried, WordCount, WrittenWhen,
};
use rusqlite::{Connection, params_from_iter};

use super::day::{
    checkin_view, good_view, is_background, mentions_of, names_in, period_days, period_of,
    previous_period,
};
use super::store::{self, NewRow};
use super::{Ctx, JItem, current, first_line, items_in_days, load_item};
use crate::Engine;
use crate::error::{Error, Result};
use crate::mind::{ReflectInput, ReflectItem, WrittenSentence};
use crate::rows::{MEM_COLS, mem_row, mem_view};
use crate::util::{fmt_ts, new_id, parse_ts};

const PARTS: [FeelingPart; 4] = [
    FeelingPart::WoundUp,
    FeelingPart::LitUp,
    FeelingPart::Low,
    FeelingPart::Settled,
];

/// A check-in this far from the middle of the field is a strong one.
const STRONG: f32 = 0.7;
/// A thing is "together" with a part of the field after this many days.
const TOGETHER_DAYS: u32 = 3;
/// Lines of "What changed" a recap carries.
const CHANGED_CAP: usize = 20;
/// Characters of one item a reflection is shown.
const REFLECT_TEXT_CAP: usize = 1200;

fn left_out_ids(conn: &Connection) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT id FROM items WHERE left_out = 1")?;
    Ok(stmt
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

fn strip_spans(text: &str) -> String {
    text.replace("**", "")
}

pub(crate) fn clock_label(tz: &TimeZone, at: Timestamp) -> String {
    let z = at.to_zoned(tz.clone());
    let (h, suffix) = match z.hour() {
        0 => (12, "am"),
        h @ 1..=11 => (h, "am"),
        12 => (12, "pm"),
        h => (h - 12, "pm"),
    };
    format!("{h}:{:02} {suffix}", z.minute())
}

/// The first sentence of a stored day reflection, as the journal day shows it.
pub(crate) fn summary(
    conn: &Connection,
    ctx: &Ctx,
    originals: &Path,
    day: Date,
) -> Result<Option<ReflectionSummary>> {
    let period = period_of(PeriodKind::Day, day)?;
    let Some(row) = store::load(conn, &period)? else {
        return Ok(None);
    };
    let view = day_reflection(conn, ctx, originals, &period, &row)?;
    Ok(Some(ReflectionSummary {
        first_sentence: view
            .the_day
            .first()
            .map(|l| strip_spans(&l.text))
            .unwrap_or_default(),
        written_at: row.written_at,
        from_count: row.from_count,
    }))
}

/// Open promises, plans and due dates among the memories read from these items.
fn open_memories(conn: &Connection, items: &[String]) -> Result<Vec<MemoryView>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let marks = vec!["?"; items.len()].join(", ");
    let sql = format!(
        "SELECT {MEM_COLS} FROM memories m WHERE m.superseded_at IS NULL AND m.forgetting = 0 \
         AND m.done = 0 AND m.predicate IN ('promised', 'scheduled', 'due') \
         AND m.status IN ('active', 'fading', 'unconfirmed') \
         AND m.id IN (SELECT memory_id FROM memory_sources WHERE item_id IN ({marks})) \
         ORDER BY COALESCE(m.due, m.valid_from, '9999'), m.id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(items.iter()), mem_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.iter().map(|m| mem_view(conn, m)).collect()
}

/// A day reflection as stored, with lines citing left-out items hidden and what is not written
/// by the mind filled in from the day as it is now.
fn day_reflection(
    conn: &Connection,
    ctx: &Ctx,
    originals: &Path,
    period: &Period,
    row: &store::Row,
) -> Result<DayReflection> {
    let mut r: DayReflection = serde_json::from_str(&row.body)?;
    let left = left_out_ids(conn)?;
    store::retain_day(&mut r, &|id| !left.contains(id));
    let (start, end) = period_days(period)?;
    let all = items_in_days(conn, ctx, originals, start, end)?;
    let items: Vec<&JItem> = current(&all).into_iter().filter(|i| !i.left_out).collect();
    let mentions = mentions_of(conn, &super::day::ids_of(items.iter().copied()))?;
    r.id = row.id.clone();
    r.written_at = row.written_at.clone();
    r.written = row.written;
    r.so_far = row.so_far;
    r.from_count = row.from_count;
    r.lines_removed = row.lines_removed;
    r.felt = items
        .iter()
        .filter(|i| i.role() == Some(JournalRole::Checkin))
        .map(|i| checkin_view(i, &mentions))
        .collect::<Result<_>>()?;
    r.good = items
        .iter()
        .filter(|i| i.role() == Some(JournalRole::Good))
        .map(|i| good_view(conn, originals, i, &mentions))
        .collect::<Result<_>>()?;
    r.people = names_in(
        items.iter().copied().filter(|i| !is_background(i)),
        &mentions,
    );
    let ids: Vec<String> = items.iter().map(|i| i.id().to_string()).collect();
    r.still_open = open_memories(conn, &ids)?;
    Ok(r)
}

fn recap_view(conn: &Connection, row: &store::Row) -> Result<Recap> {
    let mut r: Recap = serde_json::from_str(&row.body)?;
    let left = left_out_ids(conn)?;
    store::retain_recap(&mut r, &|id| !left.contains(id));
    r.id = row.id.clone();
    r.lines_removed = row.lines_removed;
    Ok(r)
}

fn glance_kind(item: &JItem) -> GlanceKind {
    match item.view.kind {
        ItemKind::Audio => GlanceKind::Voice,
        ItemKind::Image => GlanceKind::Photo,
        ItemKind::Document => GlanceKind::Document,
        ItemKind::Web => GlanceKind::Web,
        ItemKind::Text | ItemKind::Checkin => GlanceKind::Note,
    }
}

/// Everything kept in a period, with the earlier versions of changed blocks left out.
struct Gathered {
    items: Vec<JItem>,
    mentions: HashMap<String, Vec<Mention>>,
}

impl Gathered {
    fn load(conn: &Connection, ctx: &Ctx, originals: &Path, period: &Period) -> Result<Gathered> {
        let (start, end) = period_days(period)?;
        let all = items_in_days(conn, ctx, originals, start, end)?;
        let mut items: Vec<JItem> = Vec::new();
        for i in current(&all) {
            if !i.view.private && !crate::capture::in_forget_job(conn, i.id())? {
                items.push(i.clone());
            }
        }
        let mentions = mentions_of(conn, &super::day::ids_of(&items))?;
        Ok(Gathered { items, mentions })
    }

    /// What reflections and recaps may use: nothing left out. Private items and items being
    /// forgotten are never gathered.
    fn usable(&self) -> impl Iterator<Item = &JItem> {
        self.items.iter().filter(|i| !i.left_out)
    }

    fn checkins(&self) -> Vec<&JItem> {
        self.usable()
            .filter(|i| i.role() == Some(JournalRole::Checkin))
            .collect()
    }
}

fn entry_text_lines(g: &Gathered, ctx: &Ctx) -> Result<(Vec<String>, Option<String>)> {
    let mut days: BTreeMap<Date, Vec<&JItem>> = BTreeMap::new();
    for i in g
        .usable()
        .filter(|i| i.role() == Some(JournalRole::Entry) && i.view.kind == ItemKind::Text)
    {
        days.entry(i.day(ctx)?).or_default().push(i);
    }
    let Some((_, mut blocks)) = days.into_iter().next_back() else {
        return Ok((Vec::new(), None));
    };
    blocks.sort_by(|a, b| {
        a.block()
            .cmp(&b.block())
            .then(a.view.kept_at.cmp(&b.view.kept_at))
    });
    let mut lines = Vec::new();
    let mut from = None;
    for b in blocks {
        for l in b.view.text.as_deref().unwrap_or("").lines() {
            let l = l.trim();
            if l.is_empty() {
                continue;
            }
            if from.is_none() {
                from = Some(b.id().to_string());
            }
            lines.push(l.to_string());
            if lines.len() == 2 {
                return Ok((lines, from));
            }
        }
    }
    Ok((lines, from))
}

fn glance_of(conn: &Connection, ctx: &Ctx, originals: &Path, period: &Period) -> Result<Glance> {
    let g = Gathered::load(conn, ctx, originals, period)?;
    let mut kept: Vec<KeptCount> = [
        GlanceKind::Note,
        GlanceKind::Voice,
        GlanceKind::Photo,
        GlanceKind::Document,
        GlanceKind::Web,
    ]
    .into_iter()
    .map(|kind| KeptCount {
        kind,
        item_ids: g
            .items
            .iter()
            .filter(|i| i.role() != Some(JournalRole::Checkin) && glance_kind(i) == kind)
            .map(|i| i.id().to_string())
            .collect(),
    })
    .filter(|k| !k.item_ids.is_empty())
    .collect();
    kept.sort_by_key(|k| std::cmp::Reverse(k.item_ids.len()));
    let felt = g
        .checkins()
        .into_iter()
        .map(|i| checkin_view(i, &g.mentions))
        .collect::<Result<_>>()?;
    let good = g
        .usable()
        .filter(|i| i.role() == Some(JournalRole::Good))
        .map(|i| good_view(conn, originals, i, &g.mentions))
        .collect::<Result<_>>()?;
    let with = names_in(g.usable().filter(|i| !is_background(i)), &g.mentions);
    let (entry_lines, entry_item_id) = entry_text_lines(&g, ctx)?;
    Ok(Glance {
        period: period.clone(),
        kept,
        felt,
        good,
        with,
        entry_lines,
        entry_item_id,
    })
}

fn reflect_label(item: &JItem) -> &'static str {
    match (item.role(), item.view.kind) {
        (Some(JournalRole::Entry), ItemKind::Text) => "today's entry",
        (Some(JournalRole::Good), _) => "good thing",
        (_, ItemKind::Audio) => "voice note",
        (_, ItemKind::Image) => "photo",
        (_, ItemKind::Document) => "document",
        (_, ItemKind::Web) => "web page",
        _ => "note",
    }
}

/// What a reflection is shown of an item: the person's words or what was read from it, a
/// summary instead for what was kept from elsewhere, cut at a word when long.
fn reflect_text(item: &JItem) -> String {
    let fields = if is_background(item) {
        [&item.view.summary, &item.view.text]
    } else {
        [&item.view.text, &item.view.summary]
    };
    let text = fields
        .into_iter()
        .flatten()
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .unwrap_or_default();
    match text.char_indices().nth(REFLECT_TEXT_CAP) {
        None => text.to_string(),
        Some((at, _)) => text[..at]
            .rsplit_once(char::is_whitespace)
            .map_or(&text[..at], |(head, _)| head)
            .trim_end()
            .to_string(),
    }
}

fn reflect_items(conn: &Connection, ctx: &Ctx, g: &Gathered) -> Result<Vec<ReflectItem>> {
    let mut items: Vec<&JItem> = Vec::new();
    for i in g
        .usable()
        .filter(|i| i.role() != Some(JournalRole::Checkin) && !i.pending())
    {
        if !crate::remote::withheld_from_phones(conn, i.id())? {
            items.push(i);
        }
    }
    items.sort_by(|a, b| {
        a.saved_at()
            .cmp(b.saved_at())
            .then(a.block().cmp(&b.block()))
            .then(a.id().cmp(b.id()))
    });
    let mut out = Vec::new();
    for i in items {
        let text = reflect_text(i);
        if text.is_empty() {
            continue;
        }
        out.push(ReflectItem {
            id: i.id().to_string(),
            at: clock_label(&ctx.tz, parse_ts(i.saved_at())?),
            label: reflect_label(i).to_string(),
            text,
            background: is_background(i),
        });
    }
    Ok(out)
}

/// The mean of two strengths: how far a point sits from the middle of the field.
fn distance(f: &Feeling) -> f32 {
    (f.point.pleasant.powi(2) + f.point.energy.powi(2)).sqrt()
}

fn part_index(p: FeelingPart) -> usize {
    PARTS.iter().position(|x| *x == p).unwrap_or(0)
}

struct Candidate {
    score: u8,
    highlight: Highlight,
}

/// The line of `text` that holds UTF-16 offset `at`.
fn line_at(text: &str, at: u32) -> Option<String> {
    let mut offset = 0u32;
    for line in text.split('\n') {
        let len = line.encode_utf16().count() as u32;
        if at <= offset + len {
            let line = line.trim();
            return (!line.is_empty()).then(|| line.to_string());
        }
        offset += len + 1;
    }
    None
}

fn highlights(
    conn: &Connection,
    ctx: &Ctx,
    originals: &Path,
    period: &Period,
    g: &Gathered,
) -> Result<Vec<Highlight>> {
    let mut by_day: BTreeMap<Date, Vec<Candidate>> = BTreeMap::new();
    let mut push = |day: Date, score: u8, highlight: Highlight| {
        by_day
            .entry(day)
            .or_default()
            .push(Candidate { score, highlight });
    };
    let mut longest: Option<(usize, Date, &JItem)> = None;
    for item in g.usable() {
        let day = item.day(ctx)?;
        let ds = day.to_string();
        match item.role() {
            Some(JournalRole::Good) => {
                let thing = good_view(conn, originals, item, &g.mentions)?;
                if let Some(photo) = thing.photo.clone() {
                    let mut receipts = vec![item.receipt()];
                    let text = if thing.text.is_empty() {
                        // Words taken from the photo cite the photo too.
                        receipts.push(load_item(conn, originals, &photo.id)?.receipt());
                        photo.summary.clone().unwrap_or_else(|| photo.title.clone())
                    } else {
                        thing.text.clone()
                    };
                    push(
                        day,
                        5,
                        Highlight {
                            day: ds.clone(),
                            text,
                            quoted: !thing.text.is_empty(),
                            receipts,
                            photo: Some(photo),
                            feeling: None,
                        },
                    );
                }
            }
            Some(JournalRole::Checkin) => {
                let Some(feeling) = item.feeling() else {
                    continue;
                };
                if distance(&feeling) >= STRONG && !feeling.words.is_empty() {
                    let line = item.meta_str("line").map(str::to_string);
                    push(
                        day,
                        4,
                        Highlight {
                            day: ds.clone(),
                            quoted: line.is_some(),
                            text: line.unwrap_or_else(|| feeling.words.join(", ")),
                            receipts: vec![item.receipt()],
                            photo: None,
                            feeling: Some(feeling),
                        },
                    );
                }
            }
            Some(JournalRole::Entry) if item.view.kind == ItemKind::Text => {
                let len = item.view.text.as_deref().map_or(0, |t| t.chars().count());
                if longest.is_none_or(|(l, _, _)| len > l) {
                    longest = Some((len, day, item));
                }
            }
            _ => {}
        }
        if is_background(item) {
            continue;
        }
        for m in g.mentions.get(item.id()).into_iter().flatten() {
            if m.new
                && m.state != MentionState::Removed
                && m.kind != nocturne_contract::EntityKind::Thing
                && let (Some(start), Some(text)) = (m.start, item.view.text.as_deref())
                && let Some(line) = line_at(text, start)
            {
                push(
                    day,
                    3,
                    Highlight {
                        day: ds.clone(),
                        text: line,
                        quoted: true,
                        receipts: vec![item.receipt()],
                        photo: None,
                        feeling: None,
                    },
                );
            }
        }
    }
    // Decisions read from the period's own words.
    let item_days: HashMap<&str, Date> = g
        .usable()
        .filter_map(|i| i.day(ctx).ok().map(|d| (i.id(), d)))
        .collect();
    if !item_days.is_empty() {
        let marks = vec!["?"; item_days.len()].join(", ");
        let sql = format!(
            "SELECT {MEM_COLS}, (SELECT s.item_id FROM memory_sources s WHERE s.memory_id = m.id \
             AND s.item_id IN ({marks}) ORDER BY s.item_id LIMIT 1) \
             FROM memories m WHERE m.predicate = 'decided' AND m.superseded_at IS NULL \
             AND m.forgetting = 0 AND m.status IN ('active', 'fading') \
             AND EXISTS (SELECT 1 FROM memory_sources s WHERE s.memory_id = m.id \
                         AND s.item_id IN ({marks})) ORDER BY m.id"
        );
        let mut args: Vec<&str> = item_days.keys().copied().collect();
        args.sort_unstable();
        let both: Vec<&str> = args.iter().chain(args.iter()).copied().collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(both), |r| {
                Ok((mem_row(r)?, r.get::<_, String>(28)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (m, item_id) in rows {
            let Some(day) = item_days.get(item_id.as_str()).copied() else {
                continue;
            };
            let view = mem_view(conn, &m)?;
            push(
                day,
                3,
                Highlight {
                    day: day.to_string(),
                    text: view.sentence.clone(),
                    quoted: false,
                    receipts: view.receipts.clone(),
                    photo: None,
                    feeling: None,
                },
            );
        }
    }
    if let Some((_, day, item)) = longest
        && let Some(line) = item.view.text.as_deref().and_then(first_line)
    {
        push(
            day,
            2,
            Highlight {
                day: day.to_string(),
                text: line,
                quoted: true,
                receipts: vec![item.receipt()],
                photo: None,
                feeling: None,
            },
        );
    }
    let mut best: Vec<(u8, Date, Highlight)> = by_day
        .into_iter()
        .filter_map(|(day, mut c)| {
            c.sort_by_key(|c| std::cmp::Reverse(c.score));
            c.into_iter().next().map(|c| (c.score, day, c.highlight))
        })
        .collect();
    best.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let chosen: Vec<(u8, Date, Highlight)> = match period.kind {
        PeriodKind::Year => {
            let mut picked: Vec<(u8, Date, Highlight)> = Vec::new();
            let mut months: HashSet<i8> = HashSet::new();
            let mut rest = Vec::new();
            for c in best {
                if months.insert(c.1.month()) {
                    picked.push(c);
                } else {
                    rest.push(c);
                }
            }
            picked.extend(rest.into_iter().take(3));
            picked
        }
        PeriodKind::Week => best.into_iter().take(5).collect(),
        _ => best.into_iter().take(8).collect(),
    };
    let mut out: Vec<(Date, Highlight)> = chosen.into_iter().map(|c| (c.1, c.2)).collect();
    out.sort_by_key(|c| c.0);
    Ok(out.into_iter().map(|c| c.1).collect())
}

fn compose(
    conn: &Connection,
    ctx: &Ctx,
    originals: &Path,
    period: &Period,
    written: WrittenWhen,
) -> Result<Recap> {
    if period.kind == PeriodKind::Day {
        return Err(Error::invalid("a day is reflected on, not recapped"));
    }
    let (_, end) = period_days(period)?;
    let g = Gathered::load(conn, ctx, originals, period)?;
    let days: BTreeSet<Date> = g.items.iter().map(|i| i.day(ctx)).collect::<Result<_>>()?;
    let checkin_items = g.checkins();
    let checkins: Vec<CheckIn> = checkin_items
        .iter()
        .map(|i| checkin_view(i, &g.mentions))
        .collect::<Result<_>>()?;

    let mut words: HashMap<String, (FeelingPart, Vec<String>)> = HashMap::new();
    for c in &checkins {
        for w in &c.feeling.words {
            let e = words
                .entry(w.clone())
                .or_insert_with(|| (c.feeling.part, Vec::new()));
            e.1.push(c.id.clone());
        }
    }
    let mut words: Vec<WordCount> = words
        .into_iter()
        .map(|(word, (part, checkin_ids))| WordCount {
            word,
            part,
            checkin_ids,
        })
        .collect();
    words.sort_by(|a, b| {
        b.checkin_ids
            .len()
            .cmp(&a.checkin_ids.len())
            .then(a.word.cmp(&b.word))
    });

    // The days each thing was mentioned on, and what was felt on each day.
    let mut with: Vec<(NameRef, BTreeSet<Date>)> = Vec::new();
    for item in g.usable().filter(|i| !is_background(i)) {
        let day = item.day(ctx)?;
        for m in g.mentions.get(item.id()).into_iter().flatten() {
            if m.state == MentionState::Removed {
                continue;
            }
            match with.iter_mut().find(|(n, _)| n.entity_id == m.entity_id) {
                Some((n, d)) => {
                    n.new |= m.new;
                    if !n.item_ids.contains(&m.item_id) {
                        n.item_ids.push(m.item_id.clone());
                    }
                    d.insert(day);
                }
                None => with.push((
                    NameRef {
                        entity_id: m.entity_id.clone(),
                        name: m.name.clone(),
                        kind: m.kind,
                        role: m.role.clone(),
                        new: m.new,
                        item_ids: vec![m.item_id.clone()],
                    },
                    BTreeSet::from([day]),
                )),
            }
        }
    }
    let mut felt_on: HashMap<(Date, usize), &JItem> = HashMap::new();
    for item in &checkin_items {
        if let Some(f) = item.feeling() {
            felt_on
                .entry((item.day(ctx)?, part_index(f.part)))
                .or_insert(item);
        }
    }
    let mut together: Vec<Together> = Vec::new();
    for (name, entity_days) in &with {
        if (entity_days.len() as u32) < TOGETHER_DAYS {
            continue;
        }
        let mut best: Option<(usize, Vec<&JItem>)> = None;
        for (pi, _) in PARTS.iter().enumerate() {
            let hits: Vec<&JItem> = entity_days
                .iter()
                .filter_map(|d| felt_on.get(&(*d, pi)).copied())
                .collect();
            if hits.len() as u32 >= TOGETHER_DAYS
                && best.as_ref().is_none_or(|(_, b)| hits.len() > b.len())
            {
                best = Some((pi, hits));
            }
        }
        if let Some((pi, hits)) = best {
            together.push(Together {
                count: hits.len() as u32,
                total: entity_days.len() as u32,
                with: name.clone(),
                part: Some(PARTS[pi]),
                word: None,
                receipts: hits.iter().map(|i| i.receipt()).collect(),
            });
        }
    }
    together.sort_by(|a, b| b.count.cmp(&a.count).then(a.with.name.cmp(&b.with.name)));
    together.truncate(3);

    let compare = if matches!(period.kind, PeriodKind::Month | PeriodKind::Year) {
        let previous_period = previous_period(period)?;
        let prev = Gathered::load(conn, ctx, originals, &previous_period)?;
        let previous = prev.checkins().len() as u32;
        let count = checkins.len() as u32;
        (count > 0 || previous > 0).then_some(Compare {
            count,
            previous,
            previous_period,
        })
    } else {
        None
    };

    let mut good_all: Vec<GoodThing> = g
        .usable()
        .filter(|i| i.role() == Some(JournalRole::Good))
        .map(|i| good_view(conn, originals, i, &g.mentions))
        .collect::<Result<_>>()?;
    if period.kind == PeriodKind::Year {
        let mut per_month: BTreeMap<i8, GoodThing> = BTreeMap::new();
        for t in good_all {
            let month = t.day.parse::<Date>()?.month();
            match per_month.get(&month) {
                Some(have) if have.photo.is_some() || t.photo.is_none() => {}
                _ => {
                    per_month.insert(month, t);
                }
            }
        }
        good_all = per_month.into_values().collect();
    }

    let mut people: Vec<DayCount> = with
        .iter()
        .map(|(n, d)| DayCount {
            with: n.clone(),
            days: d.iter().map(Date::to_string).collect(),
        })
        .collect();
    people.sort_by(|a, b| {
        b.days
            .len()
            .cmp(&a.days.len())
            .then(a.with.name.cmp(&b.with.name))
    });
    if period.kind == PeriodKind::Year {
        people.truncate(10);
    }

    let new_only: HashMap<String, Vec<Mention>> = g
        .mentions
        .iter()
        .map(|(k, v)| (k.clone(), v.iter().filter(|m| m.new).cloned().collect()))
        .collect();
    let new_names = names_in(g.usable().filter(|i| !is_background(i)), &new_only);

    let mut tried = Vec::new();
    for c in &checkins {
        for idea in &c.tried {
            tried.push(Tried {
                idea: idea.clone(),
                day: c.day.clone(),
                checkin_id: c.id.clone(),
            });
        }
    }

    let mut changed: Vec<nocturne_contract::PageLine> = Vec::new();
    {
        let mut stmt =
            conn.prepare("SELECT date FROM pages WHERE date BETWEEN ?1 AND ?2 ORDER BY date")?;
        let dates: Vec<String> = stmt
            .query_map([&period.start, &period.end], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for d in dates {
            if let Some(page) = crate::page::load_page(conn, Some(d.parse()?))? {
                for line in page.changed.into_iter().filter(|l| !l.forgotten) {
                    if !changed.iter().any(|c| c.id == line.id) {
                        changed.push(line);
                    }
                }
            }
        }
        changed.truncate(CHANGED_CAP);
    }

    let still_open = if period.kind == PeriodKind::Year {
        Vec::new()
    } else {
        let ids: Vec<String> = g.usable().map(|i| i.id().to_string()).collect();
        open_memories(conn, &ids)?
    };

    let still_reading = g.usable().filter(|i| i.pending()).count() as u32;

    Ok(Recap {
        id: new_id("rcp"),
        period: period.clone(),
        written_at: fmt_ts(ctx.now),
        written,
        so_far: !ctx.ended(end)?,
        days: days.len() as u32,
        highlights: highlights(conn, ctx, originals, period, &g)?,
        checkins,
        words,
        together,
        compare,
        good: good_all,
        people,
        new_names,
        tried,
        changed,
        still_open,
        lines_removed: 0,
        still_reading: (still_reading > 0).then_some(still_reading),
    })
}

fn day_input(conn: &Connection, ctx: &Ctx, day: Date, g: &Gathered) -> Result<ReflectInput> {
    Ok(ReflectInput {
        day,
        so_far: !ctx.ended(day)?,
        items: reflect_items(conn, ctx, g)?,
    })
}

impl Engine {
    /// The day, week, month or year at a glance: counts, feelings, good things, names and the
    /// first lines of the entry, with no reading at all.
    pub fn glance(&self, period: &Period) -> Result<Glance> {
        let originals = self.originals_dir();
        self.read(|conn| glance_of(conn, &Ctx::load(conn, self.clock())?, &originals, period))
    }

    /// What a day's reflection is written from. Check-ins and items left out are not in it.
    pub fn reflection_input(&self, period: &Period) -> Result<ReflectInput> {
        if period.kind != PeriodKind::Day {
            return Err(Error::invalid(
                "only a day has a reflection written from its items",
            ));
        }
        let originals = self.originals_dir();
        self.read(|conn| {
            let ctx = Ctx::load(conn, self.clock())?;
            let g = Gathered::load(conn, &ctx, &originals, period)?;
            day_input(conn, &ctx, period_days(period)?.0, &g)
        })
    }

    /// Compose a week, month or year from what was kept, with no mind.
    pub fn compose_recap(&self, period: &Period, written: WrittenWhen) -> Result<Recap> {
        let originals = self.originals_dir();
        self.read(|conn| {
            compose(
                conn,
                &Ctx::load(conn, self.clock())?,
                &originals,
                period,
                written,
            )
        })
    }

    /// Store a day's reflection from the sentences a mind wrote. A sentence that cites nothing
    /// the input had is dropped. A finished reflection replaces a so-far one.
    pub fn save_day_reflection(
        &self,
        period: &Period,
        input: &ReflectInput,
        sentences: &[WrittenSentence],
        written: WrittenWhen,
    ) -> Result<DayReflection> {
        self.save_day_reflection_unless(period, input, sentences, written, &|| false)
    }

    /// [`Engine::save_day_reflection`], except that nothing is stored when `stopped` says so,
    /// checked inside the transaction that would store it. The reflection stored before stays.
    pub fn save_day_reflection_unless(
        &self,
        period: &Period,
        input: &ReflectInput,
        sentences: &[WrittenSentence],
        written: WrittenWhen,
        stopped: &dyn Fn() -> bool,
    ) -> Result<DayReflection> {
        if period.kind != PeriodKind::Day || period_days(period)?.0 != input.day {
            return Err(Error::invalid("that reflection is not for this day"));
        }
        let originals = self.originals_dir();
        let now = fmt_ts(self.clock_now());
        let id = new_id("rfl");
        self.write(|tx| {
            if stopped() {
                return Err(Error::Cancelled);
            }
            let ctx = Ctx::load(tx, self.clock())?;
            let mut lines = Vec::new();
            for s in sentences {
                let receipts: Option<Vec<Receipt>> = s
                    .cites
                    .iter()
                    .map(|i| {
                        input
                            .items
                            .get(*i)
                            .and_then(|it| load_item(tx, &originals, &it.id).ok())
                            .map(|it| it.receipt())
                    })
                    .collect();
                match receipts {
                    Some(receipts) if !receipts.is_empty() => lines.push(ReflectionLine {
                        text: s.text.clone(),
                        receipts,
                    }),
                    _ => {}
                }
            }
            let g = Gathered::load(tx, &ctx, &originals, period)?;
            let reflection = DayReflection {
                id: id.clone(),
                period: period.clone(),
                written_at: now.clone(),
                written,
                so_far: input.so_far,
                from_count: input.items.len() as u32,
                the_day: lines,
                felt: Vec::new(),
                good: Vec::new(),
                people: Vec::new(),
                still_open: Vec::new(),
                still_reading: g.items.iter().filter(|i| i.pending()).count() as u32,
                lines_removed: 0,
            };
            store::upsert(
                tx,
                &NewRow {
                    period,
                    id: &id,
                    written_at: &now,
                    written,
                    so_far: input.so_far,
                    from_count: reflection.from_count,
                    lines_removed: 0,
                    body: &serde_json::to_value(&reflection)?,
                },
            )?;
            Ok(())
        })?;
        self.read(|conn| {
            let ctx = Ctx::load(conn, self.clock())?;
            let row = store::load(conn, period)?.ok_or_else(|| Error::not_found("reflection"))?;
            day_reflection(conn, &ctx, &originals, period, &row)
        })
    }

    /// Store a recap, replacing the one for its period.
    pub fn save_recap(&self, recap: &Recap) -> Result<()> {
        self.save_recap_unless(recap, &|| false)
    }

    /// [`Engine::save_recap`], except that nothing is stored when `stopped` says so, checked
    /// inside the transaction that would store it.
    pub fn save_recap_unless(&self, recap: &Recap, stopped: &dyn Fn() -> bool) -> Result<()> {
        if recap.period.kind == PeriodKind::Day {
            return Err(Error::invalid("a day is reflected on, not recapped"));
        }
        self.write(|tx| {
            if stopped() {
                return Err(Error::Cancelled);
            }
            // What was deleted after the recap was composed is not brought back by saving it.
            let mut recap = recap.clone();
            let mut live = HashSet::new();
            for id in store::item_ids_in(&serde_json::to_value(&recap)?) {
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)",
                    [&id],
                    |r| r.get(0),
                )?;
                if exists && !crate::capture::in_forget_job(tx, &id)? {
                    live.insert(id);
                }
            }
            recap.lines_removed += store::retain_recap(&mut recap, &|id| live.contains(id));
            store::upsert(
                tx,
                &NewRow {
                    period: &recap.period,
                    id: &recap.id,
                    written_at: &recap.written_at,
                    written: recap.written,
                    so_far: recap.so_far,
                    from_count: recap.days,
                    lines_removed: recap.lines_removed,
                    body: &serde_json::to_value(&recap)?,
                },
            )
        })
    }

    /// The reflection or recap stored for a period, without the lines that cite items left out.
    pub fn saved_reflection(&self, period: &Period) -> Result<Option<ReflectionView>> {
        let originals = self.originals_dir();
        self.read(|conn| {
            let ctx = Ctx::load(conn, self.clock())?;
            let Some(row) = store::load(conn, period)? else {
                return Ok(None);
            };
            Ok(Some(if period.kind == PeriodKind::Day {
                ReflectionView::Written {
                    reflection: day_reflection(conn, &ctx, &originals, period, &row)?,
                }
            } else {
                ReflectionView::Recap {
                    recap: recap_view(conn, &row)?,
                }
            }))
        })
    }

    /// The person deletes the writing for a period. Nothing they kept is touched, and the
    /// period is not put together again overnight; only asking for it again clears that.
    pub fn delete_reflection(&self, period: &Period) -> Result<()> {
        let now = fmt_ts(self.clock_now());
        self.write(|tx| {
            store::delete(tx, period)?;
            store::decline(tx, period, &now)
        })
    }

    /// Drop the writing for a period because the engine itself found it empty or void. The
    /// period is not declined: it may be written again.
    pub fn discard_reflection(&self, period: &Period) -> Result<()> {
        self.write(|tx| store::delete(tx, period))
    }

    /// The person asks for a period they deleted to be put together again: it is no longer
    /// declined.
    pub fn clear_declined_reflection(&self, period: &Period) -> Result<()> {
        self.write(|tx| store::allow(tx, period))
    }

    /// Periods that ended up to `back_days` ago and have not been looked back on, newest
    /// first; a day comes before the week, month or year it ends.
    pub fn reflections_due(&self, back_days: u32) -> Result<Vec<Period>> {
        let originals = self.originals_dir();
        self.read(|conn| {
            let ctx = Ctx::load(conn, self.clock())?;
            let today = ctx.today()?;
            let mut due: Vec<(Date, u8, Period)> = Vec::new();
            let mut seen: HashSet<Period> = HashSet::new();
            for day in super::day::days_with_items(conn, &ctx)? {
                for (rank, kind) in [
                    PeriodKind::Day,
                    PeriodKind::Week,
                    PeriodKind::Month,
                    PeriodKind::Year,
                ]
                .into_iter()
                .enumerate()
                {
                    let period = period_of(kind, day)?;
                    let end = period_days(&period)?.1;
                    if !ctx.ended(end)?
                        || i64::from(today.since(end)?.get_days()) > i64::from(back_days)
                        || !seen.insert(period.clone())
                        || store::is_final(conn, &period)?
                        || store::is_declined(conn, &period)?
                    {
                        continue;
                    }
                    if kind == PeriodKind::Day {
                        let g = Gathered::load(conn, &ctx, &originals, &period)?;
                        if day_input(conn, &ctx, day, &g)?.items.is_empty() {
                            continue;
                        }
                    }
                    due.push((end, rank as u8, period));
                }
            }
            due.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            Ok(due.into_iter().map(|d| d.2).collect())
        })
    }
}
