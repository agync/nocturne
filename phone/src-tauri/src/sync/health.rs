//! Health: a full read the first time (and whenever the kinds, the older readings or the changes
//! token change), then only what changed since the token. Each change waits in the ledger, the
//! latest version of each, until the computer takes it, because a token once read moves on.

use std::collections::{BTreeMap, BTreeSet};

use nocturne_channel::protocol::MAX_READINGS_PER_SEND;
use nocturne_contract::{
    Reading, ReadingBatch, ReadingChange, ReadingKind, ReadingValue, RemoteCall, SyncReceipt,
};

use super::ledger::{HealthLedger, SentReading};
use super::{
    Availability, Device, HEALTH_DAYS, HEALTH_DAYS_OLDER, LedgerFile, Link, REFRESH_DAYS, Sent,
    batches, day_plus, hash, new_sync_id, send_all,
};

/// The kinds in the order the design lists them.
const ORDER: [ReadingKind; 7] = [
    ReadingKind::Sleep,
    ReadingKind::HeartRate,
    ReadingKind::RestingHeartRate,
    ReadingKind::BloodPressure,
    ReadingKind::Steps,
    ReadingKind::Workout,
    ReadingKind::Weight,
];

pub fn kind_of(v: &ReadingValue) -> ReadingKind {
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

fn wire(k: ReadingKind) -> String {
    serde_json::to_value(k)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// The ledger's key for a reading: its kind and the phone's id, unique together.
pub fn key(kind: ReadingKind, phone_id: &str) -> String {
    format!("{}/{phone_id}", wire(kind))
}

fn split(key: &str) -> Option<(ReadingKind, &str)> {
    let (k, id) = key.split_once('/')?;
    let kind = ORDER.into_iter().find(|o| wire(*o) == k)?;
    Some((kind, id))
}

fn change_key(c: &ReadingChange) -> String {
    match c {
        ReadingChange::Upsert { reading } => key(kind_of(&reading.value), &reading.phone_id),
        ReadingChange::Removed { phone_id, kind } => key(*kind, phone_id),
    }
}

/// The kinds to read: chosen and allowed, in order.
pub fn kinds_to_read(chosen: &[ReadingKind], allowed: &[ReadingKind]) -> Vec<ReadingKind> {
    ORDER
        .into_iter()
        .filter(|k| chosen.contains(k) && allowed.contains(k))
        .collect()
}

/// Readings that differ from what went across, as pending upserts (the latest wins).
fn upserts(
    h: &mut HealthLedger,
    readings: Vec<Reading>,
    kinds: &[ReadingKind],
) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    for reading in readings {
        let kind = kind_of(&reading.value);
        if !kinds.contains(&kind) {
            continue;
        }
        let k = key(kind, &reading.phone_id);
        seen.insert(k.clone());
        if h.sent.get(&k).is_some_and(|s| s.hash == hash(&reading)) {
            h.pending.remove(&k);
            continue;
        }
        h.pending.insert(k, ReadingChange::Upsert { reading });
    }
    seen
}

/// Kinds kept as one reading a day, worked out from many.
fn daily(kind: ReadingKind) -> bool {
    matches!(
        kind,
        ReadingKind::HeartRate | ReadingKind::RestingHeartRate | ReadingKind::Steps
    )
}

/// What went across from `from_day` on, in these kinds, that a read no longer has: removed.
fn gone_since(
    h: &mut HealthLedger,
    seen: &BTreeSet<String>,
    from_day: &str,
    kinds: &[ReadingKind],
) {
    let gone: Vec<(String, ReadingKind, String)> = h
        .sent
        .iter()
        .filter(|(k, s)| !seen.contains(*k) && s.day.as_str() >= from_day)
        .filter_map(|(k, _)| {
            let (kind, id) = split(k)?;
            kinds
                .contains(&kind)
                .then(|| (k.clone(), kind, id.to_string()))
        })
        .collect();
    for (k, kind, phone_id) in gone {
        h.pending
            .insert(k, ReadingChange::Removed { phone_id, kind });
    }
}

/// The computer keeps readings until deleted there; the phone remembers a year of them.
fn forget_older_than_a_year(h: &mut HealthLedger, today: &str) {
    if let Some(cut) = day_plus(today, -i64::from(HEALTH_DAYS_OLDER)) {
        h.sent.retain(|_, s| s.day >= cut);
    }
}

/// A full read: what changed, and what went across from the days read but is gone now. Where
/// Health hides refusals, a missing reading may only be refused, so nothing is removed.
pub fn take_full(
    h: &mut HealthLedger,
    read: super::FullRead,
    kinds: &[ReadingKind],
    granted: &super::Granted,
    older: bool,
) {
    let seen = upserts(h, read.readings, kinds);
    if !granted.refusals_hidden {
        gone_since(h, &seen, &read.from_day, kinds);
    }
    h.token = Some(read.token);
    h.token_kinds = kinds.to_vec();
    h.token_older = older;
    h.token_distance = granted.distance;
    h.from_day = Some(read.from_day);
    forget_older_than_a_year(h, &jiff::Zoned::now().date().to_string());
}

/// Daily kinds read again over some days, beside the token: the token stays as it is.
pub fn take_days(h: &mut HealthLedger, read: super::FullRead, kinds: &[ReadingKind]) {
    let seen = upserts(h, read.readings, kinds);
    gone_since(h, &seen, &read.from_day, kinds);
}

/// What changed since the token. A deleted reading comes as an id alone; the kind is the one it
/// went across as. A reading of a kind read again over some days, sent before for one of those
/// days and missing now, went in Health. Answers whether a deletion matched nothing known, which
/// for a daily kind means some day changed.
pub fn take_changes(h: &mut HealthLedger, changes: super::Changes, kinds: &[ReadingKind]) -> bool {
    let seen = upserts(h, changes.readings, kinds);
    for (kind, days) in &changes.refreshed {
        if !kinds.contains(kind) {
            continue;
        }
        let days: BTreeSet<&str> = days.iter().map(String::as_str).collect();
        let gone: Vec<(String, String)> = h
            .sent
            .iter()
            .filter(|(k, s)| !seen.contains(*k) && days.contains(s.day.as_str()))
            .filter_map(|(k, _)| {
                let (of, id) = split(k)?;
                (of == *kind).then(|| (k.clone(), id.to_string()))
            })
            .collect();
        for (k, phone_id) in gone {
            h.pending.insert(
                k,
                ReadingChange::Removed {
                    phone_id,
                    kind: *kind,
                },
            );
        }
    }
    let known: BTreeMap<&str, Vec<String>> = h
        .sent
        .keys()
        .chain(h.pending.keys())
        .filter_map(|k| split(k).map(|(_, id)| (id, k.clone())))
        .fold(BTreeMap::new(), |mut m, (id, k)| {
            m.entry(id).or_insert_with(Vec::new).push(k);
            m
        });
    let mut removed = Vec::new();
    let mut unmatched = false;
    for id in &changes.removed {
        let found = known.get(id.as_str());
        unmatched |= found.is_none();
        for k in found.into_iter().flatten() {
            if let Some((kind, _)) = split(k)
                && kinds.contains(&kind)
            {
                removed.push((k.clone(), kind, id.clone()));
            }
        }
    }
    for (k, kind, phone_id) in removed {
        if h.sent.contains_key(&k) {
            h.pending
                .insert(k, ReadingChange::Removed { phone_id, kind });
        } else {
            // Never went across: nothing to tell the computer.
            h.pending.remove(&k);
        }
    }
    if let Some(t) = changes.token {
        h.token = Some(t);
    }
    unmatched
}

fn taken(h: &mut HealthLedger, changes: &[ReadingChange]) {
    for c in changes {
        let k = change_key(c);
        match c {
            ReadingChange::Upsert { reading } => {
                h.sent.insert(
                    k.clone(),
                    SentReading {
                        hash: hash(reading),
                        day: reading.day.clone(),
                    },
                );
            }
            ReadingChange::Removed { .. } => {
                h.sent.remove(&k);
            }
        }
        h.pending.remove(&k);
    }
}

/// Let go of waiting changes in kinds no longer brought.
pub fn prune(h: &mut HealthLedger, kinds: &[ReadingKind]) {
    h.pending
        .retain(|k, _| split(k).is_some_and(|(kind, _)| kinds.contains(&kind)));
}

/// Removals first, then by day: what is cut at the cap is the newest.
fn in_order(mut changes: Vec<ReadingChange>) -> Vec<ReadingChange> {
    changes.sort_by_key(|c| match c {
        ReadingChange::Removed { .. } => (0, String::new(), change_key(c)),
        ReadingChange::Upsert { reading } => (1, reading.day.clone(), change_key(c)),
    });
    changes
}

/// Read what changed, keep it as pending, and send it. While the computer has stopped taking
/// Health, only an empty batch goes, nothing is read and the token stays where it is.
pub async fn sync(
    ledger: &LedgerFile,
    device: &impl Device,
    link: &impl Link,
) -> Option<SyncReceipt> {
    let settings = ledger.get().health.settings;
    if !settings.bring || device.health_availability() != Availability::Available {
        return None;
    }
    let granted = device.health_granted().ok()?;
    let kinds = kinds_to_read(&settings.kinds, &granted.kinds);
    ledger.update(|l| prune(&mut l.health, &kinds));
    if kinds.is_empty() {
        return None;
    }
    if ledger.get().health.stopped {
        let probe = send(ledger, link, Vec::new(), &kinds).await;
        if ledger.get().health.stopped {
            return probe;
        }
    }
    let older = settings.older_readings && granted.history;
    let h = ledger.get().health;
    let full = h.token.is_none()
        || h.token_kinds != kinds
        || h.token_older != older
        || h.token_distance != granted.distance;
    let days = if older {
        HEALTH_DAYS_OLDER
    } else {
        HEALTH_DAYS
    };
    let whole = if full {
        true
    } else {
        let token = h.token.clone().unwrap_or_default();
        match device.health_changes(&kinds, &token, REFRESH_DAYS) {
            Ok(c) if c.expired => true,
            Ok(c) => {
                let unmatched = ledger.update(|l| take_changes(&mut l.health, c, &kinds));
                // A deletion in a daily kind says only an id: read those kinds again. Where
                // Health hides refusals, it reads the days again itself and says which.
                let per_day: Vec<ReadingKind> =
                    kinds.iter().copied().filter(|k| daily(*k)).collect();
                if unmatched
                    && !granted.refusals_hidden
                    && !per_day.is_empty()
                    && let Ok(read) = device.health_read(&per_day, HEALTH_DAYS)
                {
                    ledger.update(|l| take_days(&mut l.health, read, &per_day));
                }
                false
            }
            // Not read this time: what waits from the last read still goes.
            Err(_) => false,
        }
    };
    if whole && let Ok(read) = device.health_read(&kinds, days) {
        ledger.update(|l| take_full(&mut l.health, read, &kinds, &granted, older));
    }
    let changes = in_order(ledger.get().health.pending.into_values().collect());
    // As the calendar: an empty send once after the app starts.
    let went = ledger.went_this_run(true);
    if changes.is_empty() && went {
        ledger.update(|l| l.health.too_many = false);
        return None;
    }
    send(ledger, link, changes, &kinds).await
}

async fn send(
    ledger: &LedgerFile,
    link: &impl Link,
    mut changes: Vec<ReadingChange>,
    kinds: &[ReadingKind],
) -> Option<SyncReceipt> {
    let too_many = changes.len() > MAX_READINGS_PER_SEND;
    changes.truncate(MAX_READINGS_PER_SEND);
    let sync_id = new_sync_id();
    let batch = |seq: u32, last: bool, changes: Vec<ReadingChange>| ReadingBatch {
        sync_id: sync_id.clone(),
        seq,
        last,
        changes,
    };
    let frame = serde_json::to_vec(&batch(0, false, Vec::new())).map_or(0, |b| b.len());
    let (mut parts, too_big) = batches(changes, frame);
    if !too_big.is_empty() {
        // It can never go: noted as sent, so it is not tried again on every send.
        tracing::warn!("{} health readings too large to send", too_big.len());
        ledger.update(|l| taken(&mut l.health, &too_big));
    }
    if parts.is_empty() {
        parts.push(Vec::new());
    }
    let out = send_all(
        link,
        parts,
        |seq, last, changes| RemoteCall::SendReadings {
            batch: batch(seq, last, changes),
        },
        || {
            let s = ledger.get().health.settings;
            s.bring && kinds.iter().all(|k| s.kinds.contains(k))
        },
        |_, sent| {
            ledger.update(|l| {
                taken(&mut l.health, sent);
                l.health.stopped = false;
            })
        },
    )
    .await;
    ledger.update(|l| match out.ended {
        Sent::Stopped => l.health.stopped = true,
        Sent::TooMany => l.health.too_many = true,
        Sent::Done => l.health.too_many = too_many,
        Sent::Away | Sent::Refused => {}
        Sent::Again => {
            let settings = l.health.settings.clone();
            l.health = Default::default();
            l.health.settings = settings;
            l.health.again = true;
        }
        Sent::Off => {
            let chosen = l.health.settings.kinds.clone();
            if l.health.settings.bring {
                prune(&mut l.health, &chosen);
            } else {
                l.health.pending.clear();
            }
        }
    });
    out.total
}
