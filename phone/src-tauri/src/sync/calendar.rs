//! The calendar: it has no changes token, so the window is read whole each time and compared with
//! what went across. An entry missing from the window counts as removed only while its day is
//! well inside it, a day clear of each edge so a time zone apart never counts; one that aged out
//! of the window is let go without a word.

use std::collections::{BTreeMap, BTreeSet};

use nocturne_channel::protocol::MAX_CALENDAR_PER_SEND;
use nocturne_contract::{CalendarBatch, CalendarChange, CalendarEntry, PhoneCalendar, RemoteCall};

use super::ledger::{CalendarLedger, SentEntry};
use super::{
    Access, DAYS_AHEAD, DAYS_BACK, Device, LedgerFile, Link, Sent, SourceError, batches, day_plus,
    hash, new_sync_id, send_all,
};
use nocturne_contract::SyncReceipt;

/// The day an entry is on, as the phone read it.
fn day_of(e: &CalendarEntry) -> String {
    e.start.get(..10).unwrap_or_default().to_string()
}

fn phone_id(c: &CalendarChange) -> &str {
    match c {
        CalendarChange::Upsert { entry } => &entry.phone_id,
        CalendarChange::Removed { phone_id } => phone_id,
    }
}

/// What a read says to send: entries new or changed, entries gone from inside the window, and
/// ids to let go (aged out, or in a calendar no longer brought).
#[derive(Debug, Default, PartialEq)]
pub struct Diff {
    pub changes: BTreeMap<String, CalendarChange>,
    pub forget: Vec<String>,
}

/// Compare a read with what went across. Notes are taken off when they are off, whatever the
/// phone gave.
pub fn diff(
    sent: &BTreeMap<String, SentEntry>,
    entries: Vec<CalendarEntry>,
    window: (&str, &str),
    brought: &BTreeSet<String>,
    notes: bool,
) -> Diff {
    let (from, to) = window;
    let inner_from = day_plus(from, 1).unwrap_or_else(|| from.to_string());
    let inner_to = day_plus(to, -1).unwrap_or_else(|| to.to_string());
    let mut out = Diff::default();
    let mut seen = BTreeSet::new();
    for mut entry in entries {
        if !brought.contains(&entry.calendar_id) || !seen.insert(entry.phone_id.clone()) {
            continue;
        }
        if !notes {
            entry.notes = None;
        }
        let changed = sent
            .get(&entry.phone_id)
            .is_none_or(|s| s.hash != hash(&entry));
        if changed {
            out.changes
                .insert(entry.phone_id.clone(), CalendarChange::Upsert { entry });
        }
    }
    for (id, s) in sent {
        if seen.contains(id) {
            continue;
        }
        let day = s.day.as_str();
        if !brought.contains(&s.calendar) || day <= from {
            out.forget.push(id.clone());
        } else if inner_from.as_str() < day && day < inner_to.as_str() {
            out.changes.insert(
                id.clone(),
                CalendarChange::Removed {
                    phone_id: id.clone(),
                },
            );
        }
        // Near an edge: kept, and looked at again once the window has moved.
    }
    out
}

/// Note a batch the computer took.
fn taken(l: &mut CalendarLedger, changes: &[CalendarChange]) {
    for c in changes {
        match c {
            CalendarChange::Upsert { entry } => {
                l.sent.insert(
                    entry.phone_id.clone(),
                    SentEntry {
                        hash: hash(entry),
                        day: day_of(entry),
                        calendar: entry.calendar_id.clone(),
                    },
                );
            }
            CalendarChange::Removed { phone_id } => {
                l.sent.remove(phone_id);
            }
        }
        l.pending.remove(phone_id(c));
    }
}

/// Let go of waiting changes for calendars no longer brought.
pub fn prune(l: &mut CalendarLedger, brought: impl Fn(&str) -> bool) {
    let sent = l.sent.clone();
    l.pending.retain(|id, c| match c {
        CalendarChange::Upsert { entry } => brought(&entry.calendar_id),
        CalendarChange::Removed { .. } => sent.get(id).is_some_and(|s| brought(&s.calendar)),
    });
}

/// Removals first, then by start: what is cut at the cap is what lies furthest ahead.
fn in_order(mut changes: Vec<CalendarChange>) -> Vec<CalendarChange> {
    changes.sort_by(|a, b| match (a, b) {
        (CalendarChange::Removed { phone_id: x }, CalendarChange::Removed { phone_id: y }) => {
            x.cmp(y)
        }
        (CalendarChange::Removed { .. }, _) => std::cmp::Ordering::Less,
        (_, CalendarChange::Removed { .. }) => std::cmp::Ordering::Greater,
        (CalendarChange::Upsert { entry: x }, CalendarChange::Upsert { entry: y }) => {
            (&x.start, &x.phone_id).cmp(&(&y.start, &y.phone_id))
        }
    });
    changes
}

/// Whether the person still brings the calendar and the phone still allows it.
fn on(ledger: &LedgerFile, device: &impl Device) -> bool {
    ledger.get().calendar.settings.bring && device.calendar_access() == Access::Granted
}

/// Read the window, keep what changed as pending, and send it. While the computer has stopped
/// taking the calendar, only an empty batch goes, to learn when it takes it again.
pub async fn sync(
    ledger: &LedgerFile,
    device: &impl Device,
    link: &impl Link,
) -> Option<SyncReceipt> {
    let settings = ledger.get().calendar.settings;
    if !on(ledger, device) {
        ledger.update(|l| l.calendar.pending.clear());
        return None;
    }
    let all = device.calendars().ok()?;
    let mine: Vec<PhoneCalendar> = all.into_iter().filter(|c| settings.brings(c)).collect();
    if ledger.get().calendar.stopped {
        let window = ledger.get().calendar.window.clone()?;
        let probe = send(ledger, device, link, mine.clone(), window, Vec::new()).await;
        if ledger.get().calendar.stopped {
            return probe;
        }
    }
    let ids: Vec<String> = mine.iter().map(|c| c.id.clone()).collect();
    let brought: BTreeSet<String> = ids.iter().cloned().collect();
    match device.instances(DAYS_BACK, DAYS_AHEAD, &ids, settings.notes) {
        Ok(read) => ledger.update(|l| {
            let d = diff(
                &l.calendar.sent,
                read.entries,
                (&read.window_from, &read.window_to),
                &brought,
                settings.notes,
            );
            for id in d.forget {
                l.calendar.sent.remove(&id);
            }
            l.calendar.pending = d.changes;
            l.calendar.window = Some((read.window_from, read.window_to));
        }),
        Err(SourceError::NotAllowed) => {
            ledger.update(|l| l.calendar.pending.clear());
            return None;
        }
        // Not read this time: what waits from the last read still goes, for what is still brought.
        Err(_) => ledger.update(|l| prune(&mut l.calendar, |c| brought.contains(c))),
    }
    let l = ledger.get().calendar;
    let window = l.window.clone()?;
    let changes = in_order(l.pending.into_values().collect());
    let list_changed = l.calendars_sent.as_deref() != Some(hash(&mine).as_str());
    // Nothing changed: nothing goes, except once after the app starts, empty, so a computer that
    // lost what this phone sent can ask for all of it again.
    let went = ledger.went_this_run(false);
    if changes.is_empty() && !list_changed && went {
        ledger.update(|l| l.calendar.too_many = false);
        return None;
    }
    send(ledger, device, link, mine, window, changes).await
}

async fn send(
    ledger: &LedgerFile,
    device: &impl Device,
    link: &impl Link,
    calendars: Vec<PhoneCalendar>,
    (window_from, window_to): (String, String),
    mut changes: Vec<CalendarChange>,
) -> Option<SyncReceipt> {
    let list_hash = hash(&calendars);
    let too_many = changes.len() > MAX_CALENDAR_PER_SEND;
    changes.truncate(MAX_CALENDAR_PER_SEND);
    let sync_id = new_sync_id();
    let batch = |seq: u32, last: bool, changes: Vec<CalendarChange>| CalendarBatch {
        sync_id: sync_id.clone(),
        seq,
        last,
        window_from: window_from.clone(),
        window_to: window_to.clone(),
        // The list goes with the first batch, where it replaces the one the computer has.
        calendars: if seq == 0 {
            calendars.clone()
        } else {
            Vec::new()
        },
        changes,
    };
    let frame = serde_json::to_vec(&batch(0, false, Vec::new())).map_or(0, |b| b.len());
    let (mut parts, too_big) = batches(changes, frame);
    if !too_big.is_empty() {
        // It can never go: noted as sent, so it is not tried again on every send.
        tracing::warn!("{} calendar entries too large to send", too_big.len());
        ledger.update(|l| taken(&mut l.calendar, &too_big));
    }
    if parts.is_empty() {
        parts.push(Vec::new());
    }
    let out = send_all(
        link,
        parts,
        |seq, last, changes| RemoteCall::SendCalendar {
            batch: batch(seq, last, changes),
        },
        || on(ledger, device),
        |seq, sent| {
            ledger.update(|l| {
                taken(&mut l.calendar, sent);
                l.calendar.stopped = false;
                if seq == 0 {
                    l.calendar.calendars_sent = Some(list_hash.clone());
                }
            })
        },
    )
    .await;
    ledger.update(|l| match out.ended {
        Sent::Stopped => l.calendar.stopped = true,
        Sent::TooMany => l.calendar.too_many = true,
        Sent::Done => l.calendar.too_many = too_many,
        Sent::Away | Sent::Refused => {}
        Sent::Off => l.calendar.pending.clear(),
        Sent::Again => {
            let settings = l.calendar.settings.clone();
            l.calendar = Default::default();
            l.calendar.settings = settings;
            l.calendar.again = true;
        }
    });
    out.total
}
