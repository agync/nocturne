//! What paired phone apps bring: calendar entries and health readings
//! (`docs/design/phone-apps.md`, `docs/design/phone-health.md`).
//!
//! Each entry or reading is an item on the tape, kind `calendar` or `reading`, trusted, with the
//! phone as its source. What it holds lives in `items.metadata`: `phone` (`device`, `source`,
//! `phoneId`, `hash`), `calendar` or `reading`, `day` and, for a changed version, `replaces`.
//! `phone_records` names the item that holds each entry's current version, keyed by the phone's
//! own id, so a resend is kept once and a change is a new version. `phone_sources` holds, per
//! phone and source, whether the person still takes it and when it last came in.

pub(crate) mod calendar;
pub(crate) mod delete;
pub(crate) mod export;
pub(crate) mod noticing;
pub(crate) mod read;
pub(crate) mod readings;
pub mod usual;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use nocturne_contract::{
    AppPlatform, CameInRow, CameInTag, PhoneApp, PhonePageOptions, PhoneSource, PhoneSources,
    PhoneSourcesView, PhoneUpdate, ReadingKind, SourceState, SyncReceipt,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::Value;

use crate::Engine;
use crate::error::{Error, Result};
use crate::util::{enum_str, fmt_ts, new_id, sha256_hex};

/// The phone builds this computer speaks, oldest and newest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhoneBuilds {
    pub oldest: u32,
    pub newest: u32,
}

/// The most calendar changes one send may bring; the rest of that send is refused.
pub const CALENDAR_PER_SEND: u32 = 500;
/// The most health readings one send may bring.
pub const READINGS_PER_SEND: u32 = 5000;
/// How far ahead "{count} appointments in the next two weeks" looks.
const UPCOMING_DAYS: i64 = 14;

pub(crate) const PAGE_CALENDAR: &str = "phones.page.calendar_today";
pub(crate) const PAGE_SLEEP: &str = "phones.page.sleep";
pub(crate) const NOTICING: &str = "phones.noticing";

/// A setting stored as a JSON boolean, `default` when unset.
pub(crate) fn flag(conn: &Connection, key: &str, default: bool) -> Result<bool> {
    let v: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(v.and_then(|v| serde_json::from_str::<bool>(&v).ok())
        .unwrap_or(default))
}

fn set_flag(tx: &Transaction<'_>, key: &str, on: bool) -> Result<()> {
    tx.execute(
        "INSERT OR REPLACE INTO settings(key, value) VALUES (?1, ?2)",
        params![key, serde_json::to_string(&on)?],
    )?;
    Ok(())
}

pub(crate) fn page_options(conn: &Connection) -> Result<PhonePageOptions> {
    Ok(PhonePageOptions {
        calendar_today: flag(conn, PAGE_CALENDAR, true)?,
        sleep_last_night: flag(conn, PAGE_SLEEP, true)?,
    })
}

pub(crate) fn source_str(s: PhoneSource) -> String {
    enum_str(&s)
}

/// The canonical hash of what a phone sent for one entry: the same content from the same phone
/// hashes the same, whatever order its fields arrived in.
pub(crate) fn content_hash(
    device: &str,
    source: PhoneSource,
    phone_id: &str,
    content: &Value,
) -> String {
    let canonical = serde_json::to_string(&json_sorted(content)).unwrap_or_default();
    sha256_hex(format!("{device}\n{}\n{phone_id}\n{canonical}", source_str(source)).as_bytes())
}

fn json_sorted(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let sorted: std::collections::BTreeMap<_, _> =
                m.iter().map(|(k, v)| (k.clone(), json_sorted(v))).collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(a) => Value::Array(a.iter().map(json_sorted).collect()),
        other => other.clone(),
    }
}

/// One source's row, made when a phone first sends it.
pub(crate) struct SourceRow {
    pub taking: bool,
    pub sync_id: Option<String>,
    pub sync_taken: u32,
    pub calendars: String,
}

/// The device must be paired now, or be this phone's own row.
pub(crate) fn check_device(tx: &Transaction<'_>, device_id: &str) -> Result<()> {
    let live: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM remote_devices WHERE id = ?1 AND revoked_at IS NULL)",
        [device_id],
        |r| r.get(0),
    )?;
    if !live {
        return Err(Error::not_found(format!("device {device_id}")));
    }
    Ok(())
}

pub(crate) fn source_row(
    tx: &Transaction<'_>,
    device_id: &str,
    source: PhoneSource,
) -> Result<SourceRow> {
    tx.execute(
        "INSERT OR IGNORE INTO phone_sources(device_id, source) VALUES (?1, ?2)",
        params![device_id, source_str(source)],
    )?;
    Ok(tx.query_row(
        "SELECT taking, sync_id, sync_taken, calendars FROM phone_sources \
         WHERE device_id = ?1 AND source = ?2",
        params![device_id, source_str(source)],
        |r| {
            Ok(SourceRow {
                taking: r.get(0)?,
                sync_id: r.get(1)?,
                sync_taken: r.get(2)?,
                calendars: r.get(3)?,
            })
        },
    )?)
}

/// How many changes of a send are taken: counted across its batches, refused past `cap`.
pub(crate) struct SendCount {
    pub taken_before: u32,
    pub cap: u32,
}

impl SendCount {
    pub fn new(row: &SourceRow, sync_id: &str, cap: u32) -> SendCount {
        let taken_before = if row.sync_id.as_deref() == Some(sync_id) {
            row.sync_taken
        } else {
            0
        };
        SendCount { taken_before, cap }
    }

    /// Whether the next change fits.
    pub fn fits(&mut self) -> bool {
        if self.taken_before >= self.cap {
            return false;
        }
        self.taken_before += 1;
        true
    }
}

pub(crate) fn finish_send(
    tx: &Transaction<'_>,
    device_id: &str,
    source: PhoneSource,
    sync_id: &str,
    count: &SendCount,
    now: &str,
) -> Result<()> {
    tx.execute(
        "UPDATE phone_sources SET last_in_at = ?3, last_count = ?4, sync_id = ?5, sync_taken = ?4 \
         WHERE device_id = ?1 AND source = ?2",
        params![
            device_id,
            source_str(source),
            now,
            count.taken_before,
            sync_id
        ],
    )?;
    Ok(())
}

pub(crate) fn stopped() -> SyncReceipt {
    SyncReceipt {
        taken: 0,
        unchanged: 0,
        refused: 0,
        stopped: true,
        too_many: false,
    }
}

/// The current version of one entry or reading.
pub(crate) struct Record {
    pub item_id: String,
    pub version: u32,
    pub removed: bool,
    pub hash: Option<String>,
    /// The phone id the hash was made with, when a move gave the entry to another phone id.
    pub hash_device: Option<String>,
}

impl Record {
    /// Whether the entry still stands as `content` says: taken, not removed and hashing the
    /// same. The hash holds the device it was made with, so an entry that moved to another
    /// device id is checked with the one it was made with.
    pub(crate) fn is_current(
        &self,
        device: &str,
        source: PhoneSource,
        phone_id: &str,
        content: &Value,
    ) -> bool {
        let made_with = self.hash_device.as_deref().unwrap_or(device);
        !self.removed
            && self.hash.as_deref()
                == Some(content_hash(made_with, source, phone_id, content).as_str())
    }
}

pub(crate) fn record(
    tx: &Transaction<'_>,
    device_id: &str,
    source: PhoneSource,
    phone_id: &str,
) -> Result<Option<Record>> {
    Ok(tx
        .query_row(
            "SELECT r.item_id, r.version, r.removed_at IS NOT NULL, \
             json_extract(i.metadata, '$.phone.hash'), \
             json_extract(i.metadata, '$.phone.hashDevice') \
             FROM phone_records r JOIN items i ON i.id = r.item_id \
             WHERE r.device_id = ?1 AND r.source = ?2 AND r.phone_id = ?3",
            params![device_id, source_str(source), phone_id],
            |r| {
                Ok(Record {
                    item_id: r.get(0)?,
                    version: r.get(1)?,
                    removed: r.get(2)?,
                    hash: r.get(3)?,
                    hash_device: r.get(4)?,
                })
            },
        )
        .optional()?)
}

/// What a new version needs to become an item.
pub(crate) struct NewVersion<'a> {
    pub device_id: &'a str,
    pub source: PhoneSource,
    pub phone_id: &'a str,
    pub kind: &'a str,
    pub title: &'a str,
    pub text: Option<&'a str>,
    pub status: &'a str,
    pub hash: &'a str,
    /// The rest of `metadata`; `phone` is added here, `replaces` by [`set_replaces`].
    pub metadata: Value,
}

/// A health reading's item id, made from what Health says of it: the kind and the health app's
/// own id for it, and its contents. A reading read again from Health after a restore, whose
/// backup leaves readings out, gets the id it had, so what cites it finds it again. When that id
/// is taken here already (the same contents came back after a change), a new one is made.
fn reading_item_id(tx: &Transaction<'_>, key: &str, content: &Value) -> Result<String> {
    let canonical = serde_json::to_string(&json_sorted(content)).unwrap_or_default();
    let hash = sha256_hex(format!("reading\n{key}\n{canonical}").as_bytes());
    let id = format!("itm_{}", &hash[..16]);
    let taken: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)",
        [&id],
        |r| r.get(0),
    )?;
    Ok(if taken { new_id("itm") } else { id })
}

/// Insert a version as a new item and point the entry's record at it. Returns the item id.
pub(crate) fn insert_version(
    tx: &Transaction<'_>,
    now: &str,
    v: NewVersion<'_>,
    before: Option<&Record>,
) -> Result<String> {
    let id = if v.kind == "reading" {
        reading_item_id(tx, v.phone_id, &v.metadata)?
    } else {
        new_id("itm")
    };
    let version = before.map_or(1, |b| b.version + 1);
    let mut meta = v.metadata;
    meta["phone"] = serde_json::json!({
        "device": v.device_id,
        "source": source_str(v.source),
        "phoneId": v.phone_id,
        "hash": v.hash,
    });
    // Unique per version: the same content may come back after a change.
    let sha = sha256_hex(format!("{}\n{version}", v.hash).as_bytes());
    // A version the person marked private stays private when the phone changes it.
    let private: bool = match before {
        Some(b) => tx
            .query_row(
                "SELECT private FROM items WHERE id = ?1",
                [&b.item_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(false),
        None => false,
    };
    // A calendar entry waits to be read below anything the person kept.
    let priority = if v.kind == "calendar" { -1 } else { 0 };
    tx.execute(
        "INSERT INTO items(id, kind, source, trust, sha256, title, kept_at, status, text, metadata, \
         private, priority) \
         VALUES (?1, ?2, 'phone', 'trusted', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            v.kind,
            sha,
            v.title,
            now,
            v.status,
            v.text,
            serde_json::to_string(&meta)?,
            private,
            priority
        ],
    )?;
    if let Some(b) = before {
        // An earlier version nobody read yet will not be: the new one is read instead.
        tx.execute(
            "UPDATE items SET status = 'understood', lease_until = NULL, lease_token = NULL \
             WHERE id = ?1 AND status IN ('kept', 'reading')",
            [&b.item_id],
        )?;
    }
    // The phone is named on every receipt, as for what it keeps.
    tx.execute(
        "INSERT INTO remote_keeps(item_id, device_id, client_id, kept_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, v.device_id, format!("phone-{id}"), now],
    )?;
    tx.execute(
        "INSERT INTO phone_records(device_id, source, phone_id, item_id, version, removed_at, came_in_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6) \
         ON CONFLICT(device_id, source, phone_id) DO UPDATE SET item_id = ?4, version = ?5, \
         removed_at = NULL, came_in_at = ?6",
        params![v.device_id, source_str(v.source), v.phone_id, id, version, now],
    )?;
    crate::carried::take_for_phone(tx, v.device_id, &source_str(v.source), v.phone_id, &id)?;
    Ok(id)
}

/// Name the version a new one replaces. Set once the new version's claims are applied, so the
/// earlier version keeps its own as their history.
pub(crate) fn set_replaces(
    tx: &Transaction<'_>,
    item_id: &str,
    before: Option<&Record>,
) -> Result<()> {
    if let Some(b) = before {
        tx.execute(
            "UPDATE items SET metadata = json_set(metadata, '$.replaces', ?2) WHERE id = ?1",
            params![item_id, b.item_id],
        )?;
    }
    Ok(())
}

pub(crate) fn mark_removed(
    tx: &Transaction<'_>,
    device_id: &str,
    source: PhoneSource,
    phone_id: &str,
    now: &str,
) -> Result<()> {
    tx.execute(
        "UPDATE phone_records SET removed_at = ?4 \
         WHERE device_id = ?1 AND source = ?2 AND phone_id = ?3",
        params![device_id, source_str(source), phone_id, now],
    )?;
    Ok(())
}

fn source_state(
    conn: &Connection,
    device_id: &str,
    source: PhoneSource,
    today: jiff::civil::Date,
) -> Result<Option<SourceState>> {
    let row: Option<(bool, Option<String>, String)> = conn
        .query_row(
            "SELECT taking, last_in_at, calendars FROM phone_sources \
             WHERE device_id = ?1 AND source = ?2",
            params![device_id, source_str(source)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((taking, last_in_at, calendars)) = row else {
        return Ok(None);
    };
    let mut state = SourceState {
        taking,
        last_in_at,
        calendars: 0,
        upcoming: 0,
        kinds: Vec::new(),
    };
    match source {
        PhoneSource::Calendar => {
            state.calendars = serde_json::from_str::<Vec<Value>>(&calendars)
                .map(|c| c.len() as u32)
                .unwrap_or(0);
            let until = today
                .checked_add(jiff::Span::new().days(UPCOMING_DAYS))
                .map_err(Error::from)?;
            state.upcoming = conn.query_row(
                "SELECT COUNT(*) FROM phone_records r JOIN items i ON i.id = r.item_id \
                 WHERE r.device_id = ?1 AND r.source = 'calendar' AND r.removed_at IS NULL \
                 AND json_extract(i.metadata, '$.day') BETWEEN ?2 AND ?3",
                params![device_id, today.to_string(), until.to_string()],
                |r| r.get(0),
            )?;
        }
        PhoneSource::Health => {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT json_extract(i.metadata, '$.reading.type') \
                 FROM phone_records r JOIN items i ON i.id = r.item_id \
                 WHERE r.device_id = ?1 AND r.source = 'health'",
            )?;
            let kinds: BTreeSet<String> = stmt
                .query_map([device_id], |r| r.get::<_, Option<String>>(0))?
                .filter_map(|r| r.transpose())
                .collect::<rusqlite::Result<_>>()?;
            for k in KIND_ORDER {
                if kinds.contains(&enum_str(&k)) {
                    state.kinds.push(k);
                }
            }
        }
    }
    Ok(Some(state))
}

/// The kinds in the order the design lists them.
pub(crate) const KIND_ORDER: [ReadingKind; 7] = [
    ReadingKind::Sleep,
    ReadingKind::HeartRate,
    ReadingKind::RestingHeartRate,
    ReadingKind::BloodPressure,
    ReadingKind::Steps,
    ReadingKind::Workout,
    ReadingKind::Weight,
];

impl Engine {
    /// Take one batch of calendar changes from a paired phone.
    pub fn take_calendar(
        &self,
        device_id: &str,
        batch: nocturne_contract::CalendarBatch,
    ) -> Result<SyncReceipt> {
        let receipt = calendar::take(self, device_id, batch)?;
        if receipt.taken > 0 {
            self.invalidate_vectors();
            self.bump_audio();
        }
        Ok(receipt)
    }

    /// Take one batch of health readings from a paired phone.
    pub fn take_readings(
        &self,
        device_id: &str,
        batch: nocturne_contract::ReadingBatch,
    ) -> Result<SyncReceipt> {
        readings::take(self, device_id, batch)
    }

    /// From your phones: every phone, paired or removed, that is paired now or brought
    /// something, with what it brings. No update lines: see [`Engine::phone_sources_for`].
    pub fn phone_sources(&self) -> Result<PhoneSourcesView> {
        self.phone_sources_for(PhoneBuilds {
            oldest: 0,
            newest: 0,
        })
    }

    /// From your phones, with whether each app should update for the phone builds this
    /// computer speaks.
    pub fn phone_sources_for(&self, builds: PhoneBuilds) -> Result<PhoneSourcesView> {
        let today: jiff::civil::Date = self.today().parse()?;
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, app_platform, app_version, agreed_build FROM remote_devices d \
                 WHERE connection != 'local' AND (revoked_at IS NULL \
                 OR EXISTS (SELECT 1 FROM phone_sources s WHERE s.device_id = d.id)) \
                 ORDER BY paired_at, id",
            )?;
            type Row = (String, String, Option<String>, Option<String>, Option<u32>);
            let devices: Vec<Row> = stmt
                .query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?
                .collect::<rusqlite::Result<_>>()?;
            let mut phones = Vec::new();
            for (device_id, name, platform, version, agreed) in devices {
                let calendar = source_state(conn, &device_id, PhoneSource::Calendar, today)?;
                let health = source_state(conn, &device_id, PhoneSource::Health, today)?;
                let app = match (platform.as_deref().and_then(AppPlatform::parse), version) {
                    (Some(platform), Some(version)) => Some(PhoneApp {
                        platform,
                        version,
                        update: match agreed {
                            _ if platform == AppPlatform::Browser => None,
                            Some(b) if b < builds.oldest => Some(PhoneUpdate::Needed),
                            Some(b) if b < builds.newest => Some(PhoneUpdate::Ready),
                            _ => None,
                        },
                    }),
                    _ => None,
                };
                phones.push(PhoneSources {
                    // A phone that has not said which app it runs, and has never offered
                    // anything, is taken to be on the browser page.
                    in_browser: match &app {
                        Some(a) => a.platform == AppPlatform::Browser,
                        None => calendar.is_none() && health.is_none(),
                    },
                    device_id,
                    name,
                    calendar,
                    health,
                    app,
                });
            }
            Ok(PhoneSourcesView {
                phones,
                page: page_options(conn)?,
                noticing: flag(conn, NOTICING, true)?,
            })
        })
    }

    /// Note which app and version a phone runs, and the newest phone build it and this computer
    /// both speak, as it says when it connects. True when any of them changed.
    pub fn set_phone_app(
        &self,
        device_id: &str,
        platform: AppPlatform,
        version: &str,
        agreed_build: u32,
    ) -> Result<bool> {
        self.write(|tx| {
            let changed = tx.execute(
                "UPDATE remote_devices SET app_platform = ?2, app_version = ?3, agreed_build = ?4 \
                 WHERE id = ?1 AND (app_platform IS NOT ?2 OR app_version IS NOT ?3 \
                 OR agreed_build IS NOT ?4)",
                params![device_id, platform.as_str(), version, agreed_build],
            )?;
            Ok(changed > 0)
        })
    }

    /// Stop or start taking one source from one phone. Stopping deletes nothing.
    pub fn set_source_taking(
        &self,
        device_id: &str,
        source: PhoneSource,
        taking: bool,
    ) -> Result<()> {
        let now = fmt_ts(self.clock_now());
        self.write(|tx| {
            let known: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM remote_devices WHERE id = ?1)",
                [device_id],
                |r| r.get(0),
            )?;
            if !known {
                return Err(Error::not_found(format!("device {device_id}")));
            }
            tx.execute(
                "INSERT OR IGNORE INTO phone_sources(device_id, source) VALUES (?1, ?2)",
                params![device_id, source_str(source)],
            )?;
            tx.execute(
                "UPDATE phone_sources SET taking = ?3, \
                 stopped_at = CASE WHEN ?3 THEN NULL ELSE ?4 END \
                 WHERE device_id = ?1 AND source = ?2",
                params![device_id, source_str(source), taking, now],
            )?;
            Ok(())
        })
    }

    /// Whether the person takes this source from this phone. A source never sent is taken.
    pub fn source_taking(&self, device_id: &str, source: PhoneSource) -> Result<bool> {
        self.read(|conn| {
            Ok(conn
                .query_row(
                    "SELECT taking FROM phone_sources WHERE device_id = ?1 AND source = ?2",
                    params![device_id, source_str(source)],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or(true))
        })
    }

    pub fn set_phone_page_options(&self, options: PhonePageOptions) -> Result<()> {
        self.write(|tx| {
            set_flag(tx, PAGE_CALENDAR, options.calendar_today)?;
            set_flag(tx, PAGE_SLEEP, options.sleep_last_night)
        })?;
        self.bump_audio();
        Ok(())
    }

    /// "Notice what goes together".
    pub fn set_noticing(&self, on: bool) -> Result<()> {
        self.write(|tx| set_flag(tx, NOTICING, on))
    }

    /// "What came in" from one source of one phone, newest first: each version as new or
    /// changed when it came in, and each removal when it came in. A page after the last row
    /// shown starts before its `came_in_at` and, with `before_id`, after its item id, since
    /// every row of one send came in at the same time.
    pub fn came_in(
        &self,
        device_id: &str,
        source: PhoneSource,
        before: Option<&str>,
        before_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<CameInRow>> {
        let before = before
            .map(|b| crate::util::parse_ts(b).map(fmt_ts))
            .transpose()?;
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, at, title, day, changed, removed, metadata FROM ( \
                 SELECT i.id, i.kept_at AS at, i.title, json_extract(i.metadata, '$.day') AS day, \
                 json_extract(i.metadata, '$.replaces') IS NOT NULL AS changed, 0 AS removed, \
                 i.metadata FROM items i \
                 WHERE json_extract(i.metadata, '$.phone.device') = ?1 \
                 AND json_extract(i.metadata, '$.phone.source') = ?2 \
                 UNION ALL \
                 SELECT i.id, r.removed_at, i.title, json_extract(i.metadata, '$.day'), 0, 1, \
                 i.metadata FROM phone_records r JOIN items i ON i.id = r.item_id \
                 WHERE r.device_id = ?1 AND r.source = ?2 AND r.removed_at IS NOT NULL) \
                 WHERE ?3 IS NULL OR at < ?3 OR (at = ?3 AND ?5 IS NOT NULL AND id > ?5) \
                 ORDER BY at DESC, id LIMIT ?4",
            )?;
            type Raw = (String, String, String, Option<String>, bool, bool, String);
            let rows: Vec<Raw> = stmt
                .query_map(
                    params![device_id, source_str(source), before, limit, before_id],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                        ))
                    },
                )?
                .collect::<rusqlite::Result<_>>()?;
            let calendars = if source == PhoneSource::Calendar {
                read::phone_calendars(conn, device_id)?
            } else {
                Vec::new()
            };
            Ok(rows
                .into_iter()
                .map(|(item_id, at, title, day, changed, removed, meta)| {
                    let entry = serde_json::from_str::<Value>(&meta)
                        .ok()
                        .and_then(|m| m.get("entry").cloned())
                        .and_then(|e| {
                            serde_json::from_value::<nocturne_contract::CalendarEntry>(e).ok()
                        });
                    CameInRow {
                        item_id,
                        came_in_at: at,
                        title,
                        day: day.unwrap_or_default(),
                        tag: if removed {
                            CameInTag::Removed
                        } else if changed {
                            CameInTag::Changed
                        } else {
                            CameInTag::New
                        },
                        start: entry.as_ref().map(|e| e.start.clone()),
                        all_day: entry.as_ref().map(|e| e.all_day),
                        calendar: entry.as_ref().and_then(|e| {
                            calendars
                                .iter()
                                .find(|c| c.id == e.calendar_id)
                                .map(|c| c.name.clone())
                        }),
                    }
                })
                .collect())
        })
    }
}
