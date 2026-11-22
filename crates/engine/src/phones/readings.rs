//! Health readings in: each one an item, trusted, read by no mind and making no memory.

use jiff::Timestamp;
use jiff::civil::Date;
use nocturne_contract::{
    PhoneSource, Reading, ReadingBatch, ReadingChange, ReadingKind, ReadingValue, SyncReceipt,
};
use serde_json::{Value, json};

use super::{
    NewVersion, READINGS_PER_SEND, SendCount, check_device, content_hash, finish_send,
    insert_version, mark_removed, record, set_replaces, source_row, stopped,
};
use crate::Engine;
use crate::error::Result;
use crate::util::{enum_str, fmt_ts};

const MAX_LABEL: usize = 100;
const MAX_SPANS: usize = 100;
const MAX_HOURS: usize = 48;

pub(crate) fn kind_of(v: &ReadingValue) -> ReadingKind {
    match v {
        ReadingValue::Sleep { .. } => ReadingKind::Sleep,
        ReadingValue::HeartRate { .. } => ReadingKind::HeartRate,
        ReadingValue::RestingHeartRate { .. } => ReadingKind::RestingHeartRate,
        ReadingValue::BloodPressure { .. } => ReadingKind::BloodPressure,
        ReadingValue::Steps { .. } => ReadingKind::Steps,
        ReadingValue::Workout { .. } => ReadingKind::Workout,
        ReadingValue::Weight { .. } => ReadingKind::Weight,
    }
}

/// A kind as a reading's item is titled.
pub(crate) fn kind_title(k: ReadingKind) -> &'static str {
    match k {
        ReadingKind::Sleep => "Sleep",
        ReadingKind::HeartRate => "Heart rate",
        ReadingKind::RestingHeartRate => "Resting heart rate",
        ReadingKind::BloodPressure => "Blood pressure",
        ReadingKind::Steps => "Steps",
        ReadingKind::Workout => "Workout",
        ReadingKind::Weight => "Weight",
    }
}

/// The phone's own ids are unique per kind only.
pub(crate) fn record_id(kind: ReadingKind, phone_id: &str) -> String {
    format!("{}/{phone_id}", enum_str(&kind))
}

fn ts_ok(s: &str) -> bool {
    s.parse::<Timestamp>().is_ok()
}

fn num_ok(n: f64, max: f64) -> bool {
    n.is_finite() && (0.0..=max).contains(&n)
}

fn valid(r: &Reading) -> bool {
    let label_ok = |s: &str| !s.trim().is_empty() && s.chars().count() <= MAX_LABEL;
    let shape = match &r.value {
        ReadingValue::Sleep { asleep, .. } => {
            !asleep.is_empty()
                && asleep.len() <= MAX_SPANS
                && asleep.iter().all(|s| {
                    matches!((s.from.parse::<Timestamp>(), s.to.parse::<Timestamp>()),
                        (Ok(a), Ok(b)) if b >= a)
                })
        }
        ReadingValue::HeartRate { hours, .. } => {
            hours.len() <= MAX_HOURS
                && hours.iter().all(|h| {
                    ts_ok(&h.start)
                        && num_ok(h.low, 400.0)
                        && num_ok(h.high, 400.0)
                        && num_ok(h.middle, 400.0)
                        && h.low <= h.high
                })
        }
        ReadingValue::RestingHeartRate { bpm } => num_ok(*bpm, 400.0),
        ReadingValue::BloodPressure { top, bottom } => {
            num_ok(*top, 400.0) && num_ok(*bottom, 400.0)
        }
        ReadingValue::Steps { .. } => true,
        ReadingValue::Workout {
            kind, distance_m, ..
        } => label_ok(kind) && distance_m.is_none_or(|d| num_ok(d, 1_000_000.0)),
        ReadingValue::Weight { value, unit } => num_ok(*value, 2000.0) && label_ok(unit),
    };
    shape
        && !r.phone_id.is_empty()
        && r.phone_id.len() <= 200
        && r.day.len() == 10
        && r.day.parse::<Date>().is_ok()
        && ts_ok(&r.start)
        && r.end.as_deref().is_none_or(ts_ok)
        && r.source_device.as_deref().is_none_or(label_ok)
}

pub(crate) fn take(engine: &Engine, device_id: &str, batch: ReadingBatch) -> Result<SyncReceipt> {
    let now = fmt_ts(engine.clock_now());
    engine.write(|tx| {
        check_device(tx, device_id)?;
        let row = source_row(tx, device_id, PhoneSource::Health)?;
        if !row.taking {
            return Ok(stopped());
        }
        let mut count = SendCount::new(&row, &batch.sync_id, READINGS_PER_SEND);
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
                ReadingChange::Upsert { reading } => {
                    if !valid(&reading) {
                        receipt.refused += 1;
                        continue;
                    }
                    let kind = kind_of(&reading.value);
                    let key = record_id(kind, &reading.phone_id);
                    let content = json!({
                        "reading": reading.value,
                        "start": reading.start,
                        "end": reading.end,
                        "sourceDevice": reading.source_device,
                        "day": reading.day,
                    });
                    let hash = content_hash(device_id, PhoneSource::Health, &key, &content);
                    let before = record(tx, device_id, PhoneSource::Health, &key)?;
                    if let Some(b) = &before
                        && b.is_current(device_id, PhoneSource::Health, &key, &content)
                    {
                        receipt.unchanged += 1;
                        continue;
                    }
                    let id = insert_version(
                        tx,
                        &now,
                        NewVersion {
                            device_id,
                            source: PhoneSource::Health,
                            phone_id: &key,
                            kind: "reading",
                            title: kind_title(kind),
                            text: None,
                            // Nothing to read: a reading yields no memories.
                            status: "understood",
                            hash: &hash,
                            metadata: Value::clone(&content),
                        },
                        before.as_ref(),
                    )?;
                    set_replaces(tx, &id, before.as_ref())?;
                    receipt.taken += 1;
                }
                ReadingChange::Removed { phone_id, kind } => {
                    let key = record_id(kind, &phone_id);
                    match record(tx, device_id, PhoneSource::Health, &key)? {
                        Some(b) if !b.removed => {
                            mark_removed(tx, device_id, PhoneSource::Health, &key, &now)?;
                            receipt.taken += 1;
                        }
                        _ => receipt.unchanged += 1,
                    }
                }
            }
        }
        finish_send(
            tx,
            device_id,
            PhoneSource::Health,
            &batch.sync_id,
            &count,
            &now,
        )?;
        Ok(receipt)
    })
}
