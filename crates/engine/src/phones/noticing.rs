//! Noticing: a sentence in a recap when sleep, steps or workouts went with something in the
//! journal (`docs/design/phone-health.md`, "Noticing").
//!
//! Only sleep, steps and workouts are read here, so heart rate, resting heart rate, blood
//! pressure and weight can never reach a sentence. A pattern is shown only when at least three
//! days had the thing and three did not, the difference is at least 30 minutes of sleep, 2,000
//! steps or one workout, and it held on at least three of the days. Every day counted is a
//! receipt, on both sides. Nothing is stored: a recap works its sentences out each time it is
//! opened, so a reading removed or deleted is gone from them at once.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashSet};

use jiff::civil::Date;
use jiff::tz::TimeZone;
use nocturne_contract::{
    FeelingPart, ItemKind, Noticed, NoticedMeasure, NoticedTopic, ReadingKind, ReadingValue,
    Receipt, Source,
};
use rusqlite::Connection;

use super::read::{Stored, per_day, readings_of};
use super::{NOTICING, flag};
use crate::error::Result;

/// Days with the thing, and days without, needed before a pattern is shown; also the days it
/// must hold on.
pub(crate) const MIN_DAYS: usize = 3;
/// The least difference in minutes asleep.
pub(crate) const SLEEP_MIN: i64 = 30;
/// The least difference in steps.
pub(crate) const STEPS_MIN: i64 = 2000;
/// Sentences a recap carries.
pub(crate) const MOST: usize = 3;

/// One day's night or steps, with the reading it came from.
#[derive(Debug, Clone)]
pub(crate) struct Counted {
    pub value: i64,
    pub receipt: Receipt,
}

/// What the body did on each day: only what noticing may use.
#[derive(Debug, Default)]
pub(crate) struct Body {
    /// Minutes asleep on the night that ended on the day.
    pub sleep: BTreeMap<Date, Counted>,
    pub steps: BTreeMap<Date, Counted>,
    /// Each workout of the day, as a receipt.
    pub workouts: BTreeMap<Date, Vec<Receipt>>,
}

impl Body {
    pub(crate) fn is_empty(&self) -> bool {
        self.sleep.is_empty() && self.steps.is_empty() && self.workouts.is_empty()
    }
}

/// Something written in the journal, with the days it happened on and what said so.
#[derive(Debug)]
pub(crate) struct Topic {
    pub kind: NoticedTopic,
    pub days: BTreeMap<Date, Vec<Receipt>>,
}

/// A reading's receipt: it opens the Health page of its day, so its day stands in for when it
/// was kept: noon of the day in this computer's zone, written with that zone's offset, so the
/// first ten characters are the day wherever the receipt is read.
pub(crate) fn reading_receipt(s: &Stored, label: &str, tz: &TimeZone) -> Option<Receipt> {
    let day: Date = s.day.parse().ok()?;
    let noon = day.at(12, 0, 0, 0).to_zoned_datetime(tz.clone()).ok()?;
    Some(Receipt {
        item_id: s.item_id.clone(),
        kind: ItemKind::Reading,
        source: Source::Phone,
        label: label.to_string(),
        kept_at: noon.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string(),
        duration_ms: None,
        span: None,
        unconfirmed: false,
        from_device: (!s.phone.is_empty()).then(|| s.phone.clone()),
        calendar: None,
    })
}

/// A workout's receipt: labelled with its kind.
pub(crate) fn workout_receipt(s: &Stored, tz: &TimeZone) -> Option<Receipt> {
    match &s.value {
        ReadingValue::Workout { kind, .. } => reading_receipt(s, kind, tz),
        _ => None,
    }
}

fn day_values(
    conn: &Connection,
    tz: &TimeZone,
    kind: ReadingKind,
    label: &str,
    from: Date,
    to: Date,
    excluded: &HashSet<String>,
) -> Result<BTreeMap<Date, Counted>> {
    let stored: Vec<Stored> = readings_of(conn, kind, &from.to_string(), &to.to_string(), false)?
        .into_iter()
        .filter(|s| !excluded.contains(&s.item_id))
        .collect();
    let mut out = BTreeMap::new();
    for p in per_day(kind, &stored) {
        let (Ok(day), Some(s)) = (
            p.day.parse::<Date>(),
            stored.iter().find(|s| s.item_id == p.item_id),
        ) else {
            continue;
        };
        if let Some(receipt) = reading_receipt(s, label, tz) {
            out.insert(
                day,
                Counted {
                    value: p.value.round() as i64,
                    receipt,
                },
            );
        }
    }
    Ok(out)
}

/// The nights, steps and workouts kept for the days `from` to `to`, leaving out what was
/// removed in Health and the readings in `excluded`.
pub(crate) fn body_of(
    conn: &Connection,
    tz: &TimeZone,
    from: Date,
    to: Date,
    excluded: &HashSet<String>,
) -> Result<Body> {
    let mut workouts: BTreeMap<Date, Vec<Receipt>> = BTreeMap::new();
    for s in readings_of(
        conn,
        ReadingKind::Workout,
        &from.to_string(),
        &to.to_string(),
        false,
    )? {
        if excluded.contains(&s.item_id) {
            continue;
        }
        if let (Ok(day), Some(r)) = (s.day.parse::<Date>(), workout_receipt(&s, tz)) {
            workouts.entry(day).or_default().push(r);
        }
    }
    Ok(Body {
        sleep: day_values(conn, tz, ReadingKind::Sleep, "Sleep", from, to, excluded)?,
        steps: day_values(conn, tz, ReadingKind::Steps, "Steps", from, to, excluded)?,
        workouts,
    })
}

/// A pattern found, with how much it stood out, for putting the plainest first.
struct Found {
    line: Noticed,
    /// How many times over its threshold the difference was, in thousandths.
    strength: i64,
}

fn mean_gap(with: &[i64], without: &[i64]) -> (i64, i64, i64) {
    // The gap between the two averages, kept as a fraction so a threshold edge is exact:
    // (sum_with * n_without - sum_without * n_with) / (n_with * n_without).
    let (nw, no) = (with.len() as i64, without.len() as i64);
    let top = with.iter().sum::<i64>() * no - without.iter().sum::<i64>() * nw;
    (top, nw * no, no)
}

/// One topic against one measure: the days with the topic against the others.
fn against(
    topic: &Topic,
    measure: NoticedMeasure,
    values: &BTreeMap<Date, Counted>,
    least: i64,
) -> Option<Found> {
    let with: Vec<(&Date, &Counted)> = values
        .iter()
        .filter(|(d, _)| topic.days.contains_key(*d))
        .collect();
    let without: Vec<i64> = values
        .iter()
        .filter(|(d, _)| !topic.days.contains_key(*d))
        .map(|(_, c)| c.value)
        .collect();
    if with.len() < MIN_DAYS || without.len() < MIN_DAYS {
        return None;
    }
    let with_values: Vec<i64> = with.iter().map(|(_, c)| c.value).collect();
    let (top, scale, n_without) = mean_gap(&with_values, &without);
    if top == 0 || top.abs() < least * scale {
        return None;
    }
    let more = top > 0;
    // A day holds the pattern when it falls on the same side of the other days' average.
    let sum_without: i64 = without.iter().sum();
    let held = with_values
        .iter()
        .filter(|v| {
            let across = **v * n_without - sum_without;
            if more { across > 0 } else { across < 0 }
        })
        .count();
    if held < MIN_DAYS {
        return None;
    }
    let difference = ((top.abs() as f64) / (scale as f64)).round() as u32;
    let journal: Vec<Receipt> = with
        .iter()
        .flat_map(|(d, _)| topic.days.get(*d).cloned().unwrap_or_default())
        .collect();
    Some(Found {
        strength: top.abs() * 1000 / (least * scale),
        line: Noticed {
            measure,
            topic: topic.kind.clone(),
            days: topic.days.len() as u32,
            held: held as u32,
            other_days: without.len() as u32,
            difference: difference.max(1),
            more,
            body: with.iter().map(|(_, c)| c.receipt.clone()).collect(),
            journal,
        },
    })
}

/// Workout days against the others: the days checked in as a part of the field. It holds when
/// the part was felt on at least three workout days, and on a share of them at least 25
/// percentage points larger than of the days known to have had no workout.
fn on_workout_days(topic: &Topic, part: FeelingPart, body: &Body) -> Option<Found> {
    let known: BTreeSet<Date> = body
        .sleep
        .keys()
        .chain(body.steps.keys())
        .chain(body.workouts.keys())
        .copied()
        .collect();
    let active: BTreeSet<Date> = body.workouts.keys().copied().collect();
    let quiet: Vec<Date> = known.difference(&active).copied().collect();
    if active.len() < MIN_DAYS || quiet.len() < MIN_DAYS {
        return None;
    }
    let hits: Vec<Date> = active
        .iter()
        .filter(|d| topic.days.contains_key(*d))
        .copied()
        .collect();
    let quiet_hits = quiet.iter().filter(|d| topic.days.contains_key(*d)).count();
    if hits.len() < MIN_DAYS {
        return None;
    }
    // Compared as fractions of each group: hits / active - quiet_hits / quiet >= 1 / 4.
    let (a, q) = (active.len() as i64, quiet.len() as i64);
    let gap = hits.len() as i64 * q - quiet_hits as i64 * a;
    if gap * 4 < a * q {
        return None;
    }
    let body_receipts: Vec<Receipt> = active
        .iter()
        .flat_map(|d| body.workouts.get(d).cloned().unwrap_or_default())
        .collect();
    let journal: Vec<Receipt> = hits
        .iter()
        .flat_map(|d| topic.days.get(d).cloned().unwrap_or_default())
        .collect();
    Some(Found {
        strength: gap * 1000 / (a * q),
        line: Noticed {
            measure: NoticedMeasure::Workout,
            topic: NoticedTopic::Felt { part },
            days: active.len() as u32,
            held: hits.len() as u32,
            other_days: quiet.len() as u32,
            difference: 0,
            more: true,
            body: body_receipts,
            journal,
        },
    })
}

fn label_of(kind: &NoticedTopic) -> String {
    match kind {
        NoticedTopic::Name { with } => with.name.clone(),
        NoticedTopic::Felt { part } => match part {
            FeelingPart::WoundUp => "WoundUp",
            FeelingPart::LitUp => "LitUp",
            FeelingPart::Low => "Low",
            FeelingPart::Settled => "Settled",
        }
        .to_string(),
        NoticedTopic::Good => String::new(),
    }
}

/// The sentences the body and the journal support, plainest first and at most [`MOST`].
/// Each topic says one thing about sleep or steps, whichever stood out more.
pub(crate) fn notice(topics: &[Topic], body: &Body) -> Vec<Noticed> {
    let mut found: Vec<Found> = Vec::new();
    for topic in topics {
        let best = [
            against(topic, NoticedMeasure::Sleep, &body.sleep, SLEEP_MIN),
            against(topic, NoticedMeasure::Steps, &body.steps, STEPS_MIN),
        ]
        .into_iter()
        .flatten()
        .max_by_key(|f| f.strength);
        found.extend(best);
        if let NoticedTopic::Felt { part } = &topic.kind {
            found.extend(on_workout_days(topic, *part, body));
        }
    }
    found.sort_by(|a, b| {
        (
            Reverse(a.line.days),
            Reverse(a.strength),
            label_of(&a.line.topic),
        )
            .cmp(&(
                Reverse(b.line.days),
                Reverse(b.strength),
                label_of(&b.line.topic),
            ))
    });
    found.truncate(MOST);
    found.into_iter().map(|f| f.line).collect()
}

/// Whether "Notice what goes together" is on.
pub(crate) fn is_on(conn: &Connection) -> Result<bool> {
    flag(conn, NOTICING, true)
}

#[cfg(test)]
mod tests;
