//! Calendar entries and health readings a backup brings (`docs/design/phone-backup.md`, "Onto a
//! phone that has a memory"): each is kept once, by the phone's own id for it in its calendar or
//! Health, and the newer version stays. An entry only in the backup comes with its versions. One
//! on both whose backup version came in later comes too, after the one here, which stays as its
//! earlier version; otherwise the one here stays and the backup's does not come.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::{Map, Value, json};

use super::Arriving;
use crate::error::Result;

/// Whether a thing of `kind` is a calendar entry or a health reading.
pub(super) fn is_entry(kind: &str) -> bool {
    matches!(kind, "calendar" | "reading")
}

/// The backup phone's record of one entry: the item holding its current version.
pub(super) struct Record {
    source: String,
    phone_id: String,
    item: String,
    version: i64,
    removed_at: Option<String>,
    came_in_at: String,
}

/// The records of the phone that made the backup, from its own calendar and Health.
pub(super) fn records(phone: &Connection) -> Result<Vec<Record>> {
    let mut stmt = phone.prepare(
        "SELECT r.source, r.phone_id, r.item_id, r.version, r.removed_at, r.came_in_at \
         FROM phone_records r JOIN remote_devices d ON d.id = r.device_id \
         WHERE d.connection = 'local' ORDER BY r.rowid",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Record {
            source: r.get(0)?,
            phone_id: r.get(1)?,
            item: r.get(2)?,
            version: r.get(3)?,
            removed_at: r.get(4)?,
            came_in_at: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// What comes of the backup's entries.
#[derive(Default)]
pub(super) struct Plan {
    /// Versions that may be added, by their id in the backup.
    pub keep: HashSet<String>,
    /// The earliest version that comes of an entry here too, and the current version here it
    /// follows.
    pub after: HashMap<String, String>,
    /// Records to write once the versions are in: the backup's record and its version here.
    write: Vec<(usize, i64)>,
}

/// Decide, for each entry the backup holds, whether its versions come.
pub(super) fn plan(
    tx: &Transaction<'_>,
    device: &str,
    records: &[Record],
    arriving: &[Arriving],
) -> Result<Plan> {
    let replaces: HashMap<&str, &str> = arriving
        .iter()
        .filter(|p| is_entry(&p.kind))
        .filter_map(|p| Some((p.id.as_str(), p.meta.get("replaces")?.as_str()?)))
        .collect();
    let mut plan = Plan::default();
    for (n, r) in records.iter().enumerate() {
        let mut versions = vec![r.item.as_str()];
        while let Some(prev) = replaces.get(versions[versions.len() - 1])
            && !versions.contains(prev)
        {
            versions.push(prev);
        }
        let here: Option<(String, i64, String)> = tx
            .query_row(
                "SELECT item_id, version, came_in_at FROM phone_records \
                 WHERE device_id = ?1 AND source = ?2 AND phone_id = ?3",
                params![device, r.source, r.phone_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let version = match here {
            None => r.version,
            Some((_, _, came)) if came >= r.came_in_at => continue,
            Some((item, version, _)) => {
                let earliest = versions[versions.len() - 1];
                plan.after.insert(earliest.to_string(), item);
                version + 1
            }
        };
        plan.keep.extend(versions.into_iter().map(str::to_string));
        plan.write.push((n, version));
    }
    Ok(plan)
}

/// An entry's version as it comes: from this phone, with the hash it was made with kept so the
/// next time the calendar or Health sends it, it is known.
pub(super) fn rehome(meta: &mut Map<String, Value>, device: &str) {
    let Some(phone) = meta.get_mut("phone").and_then(Value::as_object_mut) else {
        return;
    };
    if !phone.contains_key("hashDevice")
        && let Some(made) = phone.get("device").cloned()
    {
        phone.insert("hashDevice".into(), made);
    }
    phone.insert("device".into(), json!(device));
}

/// Point each entry that came at its current version here.
pub(super) fn write(
    tx: &Transaction<'_>,
    device: &str,
    records: &[Record],
    plan: &Plan,
    here: &HashMap<&str, &str>,
) -> Result<()> {
    for &(n, version) in &plan.write {
        let r = &records[n];
        let Some(item) = here.get(r.item.as_str()) else {
            continue;
        };
        tx.execute(
            "INSERT INTO phone_records(device_id, source, phone_id, item_id, version, removed_at, \
             came_in_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT(device_id, source, phone_id) DO UPDATE SET item_id = ?4, version = ?5, \
             removed_at = ?6, came_in_at = ?7",
            params![
                device,
                r.source,
                r.phone_id,
                item,
                version,
                r.removed_at,
                r.came_in_at
            ],
        )?;
    }
    Ok(())
}
