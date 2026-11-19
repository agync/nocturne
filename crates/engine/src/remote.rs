//! Away from home in the store: paired phones, what they kept and what they did.

use jiff::{SignedDuration, Timestamp};
use nocturne_contract::{AutoOff, RemoteAction, RemoteConnection, RemoteEvent, RemoteResult};
use rusqlite::{OptionalExtension, Row, params};

use crate::Engine;
use crate::error::{Error, Result};
use crate::util::{enum_str, fmt_ts, parse_enum, parse_ts};

const COLLAPSE_SECS: i64 = 60;
const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// A paired phone. A removed phone keeps its row with `revoked_at` set.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceRow {
    pub id: String,
    pub name: String,
    pub public_key: Vec<u8>,
    pub home_origin: String,
    pub connection: RemoteConnection,
    pub address: Option<String>,
    pub paired_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
    pub revoked_at: Option<Timestamp>,
}

/// Away from home's settings and counters, one row.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteRow {
    pub connection: RemoteConnection,
    pub wanted_on: bool,
    pub auto_off: AutoOff,
    pub keep_awake: bool,
    pub sent_bytes: f64,
    pub first_on_at: Option<Timestamp>,
    pub on_since: Option<Timestamp>,
    pub last_on_at: Option<Timestamp>,
    pub last_off_at: Option<Timestamp>,
    pub address: Option<String>,
    pub address_since: Option<Timestamp>,
    pub machine_name: Option<String>,
}

const DEVICE_COLS: &str = "id, name, public_key, home_origin, connection, address, paired_at, \
    last_used_at, revoked_at";

fn ts_opt(s: Option<String>) -> Result<Option<Timestamp>> {
    s.as_deref().map(parse_ts).transpose()
}

fn device_row(r: &Row<'_>) -> rusqlite::Result<Result<DeviceRow>> {
    let connection: String = r.get(4)?;
    let (paired, used, revoked): (String, Option<String>, Option<String>) =
        (r.get(6)?, r.get(7)?, r.get(8)?);
    let (id, name, public_key, home_origin, address) =
        (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(5)?);
    Ok((|| {
        Ok(DeviceRow {
            id,
            name,
            public_key,
            home_origin,
            connection: parse_enum(&connection)?,
            address,
            paired_at: parse_ts(&paired)?,
            last_used_at: ts_opt(used)?,
            revoked_at: ts_opt(revoked)?,
        })
    })())
}

fn new_device_id() -> String {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("the operating system provides random bytes");
    let mut out = String::with_capacity(22);
    for chunk in buf.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        let chars = chunk.len() + 1;
        for i in 0..chars {
            out.push(BASE64URL[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

fn load_device(conn: &rusqlite::Connection, id: &str) -> Result<Option<DeviceRow>> {
    conn.query_row(
        &format!("SELECT {DEVICE_COLS} FROM remote_devices WHERE id = ?1"),
        [id],
        device_row,
    )
    .optional()?
    .transpose()
}

fn load_state(conn: &rusqlite::Connection) -> Result<RemoteRow> {
    type Raw = (
        String,
        bool,
        String,
        bool,
        f64,
        [Option<String>; 5],
        Option<String>,
        Option<String>,
    );
    let raw: Raw = conn.query_row(
        "SELECT connection, wanted_on, auto_off, keep_awake, sent_bytes, first_on_at, on_since, \
         last_on_at, last_off_at, address_since, address, machine_name FROM remote_state \
         WHERE id = 1",
        [],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                [r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?],
                r.get(10)?,
                r.get(11)?,
            ))
        },
    )?;
    let [first, since, last_on, last_off, addr_since] = raw.5;
    Ok(RemoteRow {
        connection: parse_enum(&raw.0)?,
        wanted_on: raw.1,
        auto_off: parse_enum(&raw.2)?,
        keep_awake: raw.3,
        sent_bytes: raw.4,
        first_on_at: ts_opt(first)?,
        on_since: ts_opt(since)?,
        last_on_at: ts_opt(last_on)?,
        last_off_at: ts_opt(last_off)?,
        address_since: ts_opt(addr_since)?,
        address: raw.6,
        machine_name: raw.7,
    })
}

fn store_state(conn: &rusqlite::Connection, s: &RemoteRow) -> Result<()> {
    let ts = |t: Option<Timestamp>| t.map(fmt_ts);
    conn.execute(
        "UPDATE remote_state SET connection = ?1, wanted_on = ?2, auto_off = ?3, keep_awake = ?4, \
         sent_bytes = ?5, first_on_at = ?6, on_since = ?7, last_on_at = ?8, last_off_at = ?9, \
         address_since = ?10, address = ?11, machine_name = ?12 WHERE id = 1",
        params![
            enum_str(&s.connection),
            s.wanted_on,
            enum_str(&s.auto_off),
            s.keep_awake,
            s.sent_bytes,
            ts(s.first_on_at),
            ts(s.on_since),
            ts(s.last_on_at),
            ts(s.last_off_at),
            ts(s.address_since),
            s.address,
            s.machine_name,
        ],
    )?;
    Ok(())
}

/// An item a phone must not see: private itself, or a source of any memory that is private in
/// any version, so its text and original cannot carry the private part along with a public
/// one. An item the store does not know counts as withheld.
pub(crate) fn withheld_from_phones(conn: &rusqlite::Connection, item_id: &str) -> Result<bool> {
    let private: Option<bool> = conn
        .query_row("SELECT private FROM items WHERE id = ?1", [item_id], |r| {
            r.get(0)
        })
        .optional()?;
    match private {
        None | Some(true) => return Ok(true),
        Some(false) => {}
    }
    let memories: Vec<String> = conn
        .prepare_cached("SELECT memory_id FROM memory_sources WHERE item_id = ?1")?
        .query_map([item_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for m in &memories {
        if crate::memory::is_private_chain(conn, m)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn day_bytes(conn: &rusqlite::Connection, device_id: &str, date: &str) -> Result<u64> {
    let used: Option<i64> = conn
        .query_row(
            "SELECT CASE WHEN day = ?2 THEN day_bytes ELSE 0 END FROM remote_devices \
             WHERE id = ?1",
            params![device_id, date],
            |r| r.get(0),
        )
        .optional()?;
    Ok(used.unwrap_or(0).max(0) as u64)
}

impl Engine {
    pub fn remote_state(&self) -> Result<RemoteRow> {
        self.read(load_state)
    }

    /// Reads the row, applies `f` and writes it back in one write transaction.
    pub fn update_remote_state(&self, f: impl FnOnce(&mut RemoteRow)) -> Result<RemoteRow> {
        self.write(|tx| {
            let mut state = load_state(tx)?;
            f(&mut state);
            store_state(tx, &state)?;
            Ok(state)
        })
    }

    pub fn add_device(
        &self,
        name: &str,
        public_key: &[u8],
        home_origin: &str,
        connection: RemoteConnection,
        address: &str,
    ) -> Result<DeviceRow> {
        let id = new_device_id();
        let now = self.clock_now();
        self.write(|tx| {
            tx.execute(
                "INSERT INTO remote_devices(id, name, public_key, home_origin, connection, \
                 address, paired_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    name,
                    public_key,
                    home_origin,
                    enum_str(&connection),
                    address,
                    fmt_ts(now)
                ],
            )?;
            load_device(tx, &id)?.ok_or_else(|| Error::not_found(format!("device {id}")))
        })
    }

    pub fn devices(&self, include_revoked: bool) -> Result<Vec<DeviceRow>> {
        self.read(|conn| {
            let filter = if include_revoked {
                ""
            } else {
                "WHERE revoked_at IS NULL "
            };
            let mut stmt = conn.prepare(&format!(
                "SELECT {DEVICE_COLS} FROM remote_devices {filter}ORDER BY paired_at, id"
            ))?;
            let rows = stmt.query_map([], device_row)?;
            rows.map(|r| r?).collect()
        })
    }

    pub fn device(&self, id: &str) -> Result<Option<DeviceRow>> {
        self.read(|conn| load_device(conn, id))
    }

    pub fn home_origins(&self) -> Result<Vec<String>> {
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT home_origin FROM remote_devices WHERE revoked_at IS NULL \
                 ORDER BY home_origin",
            )?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    pub fn rename_device(&self, id: &str, name: &str) -> Result<DeviceRow> {
        self.write(|tx| {
            let changed = tx.execute(
                "UPDATE remote_devices SET name = ?2 WHERE id = ?1 AND revoked_at IS NULL",
                params![id, name],
            )?;
            if changed == 0 {
                return Err(Error::not_found(format!("device {id}")));
            }
            load_device(tx, id)?.ok_or_else(|| Error::not_found(format!("device {id}")))
        })
    }

    pub fn revoke_device(&self, id: &str) -> Result<bool> {
        let now = fmt_ts(self.clock_now());
        self.write(|tx| {
            let changed = tx.execute(
                "UPDATE remote_devices SET revoked_at = ?2 WHERE id = ?1 AND revoked_at IS NULL",
                params![id, now],
            )?;
            Ok(changed > 0)
        })
    }

    pub fn revoke_all_devices(&self) -> Result<u32> {
        let now = fmt_ts(self.clock_now());
        self.write(|tx| {
            let changed = tx.execute(
                "UPDATE remote_devices SET revoked_at = ?1 WHERE revoked_at IS NULL",
                [now],
            )?;
            Ok(changed as u32)
        })
    }

    pub fn touch_device(&self, id: &str, address: &str) -> Result<()> {
        let now = fmt_ts(self.clock_now());
        self.write(|tx| {
            tx.execute(
                "UPDATE remote_devices SET last_used_at = ?2, address = ?3 \
                 WHERE id = ?1 AND revoked_at IS NULL",
                params![id, now, address],
            )?;
            Ok(())
        })
    }

    /// Bytes a phone has kept on `date`, a local `YYYY-MM-DD`.
    pub fn remote_day(&self, device_id: &str, date: &str) -> Result<u64> {
        self.read(|conn| day_bytes(conn, device_id, date))
    }

    /// Count `bytes` against a phone's `date`. False, with nothing counted, when that would go
    /// past `max` or the phone is not paired.
    pub fn charge_remote_day(
        &self,
        device_id: &str,
        date: &str,
        bytes: u64,
        max: u64,
    ) -> Result<bool> {
        self.write(|tx| {
            let used = day_bytes(tx, device_id, date)?;
            let Some(total) = used.checked_add(bytes).filter(|t| *t <= max) else {
                return Ok(false);
            };
            let changed = tx.execute(
                "UPDATE remote_devices SET day = ?2, day_bytes = ?3 \
                 WHERE id = ?1 AND revoked_at IS NULL",
                params![device_id, date, total as i64],
            )?;
            Ok(changed > 0)
        })
    }

    /// Give back what a keep that did not happen was charged on `date`.
    pub fn refund_remote_day(&self, device_id: &str, date: &str, bytes: u64) -> Result<()> {
        self.write(|tx| {
            tx.execute(
                "UPDATE remote_devices SET day_bytes = MAX(0, day_bytes - ?3) \
                 WHERE id = ?1 AND day = ?2",
                params![device_id, date, bytes as i64],
            )?;
            Ok(())
        })
    }

    pub fn link_remote_keep(&self, item_id: &str, device_id: &str, client_id: &str) -> Result<()> {
        let now = fmt_ts(self.clock_now());
        self.write(|tx| {
            tx.execute(
                "INSERT OR IGNORE INTO remote_keeps(item_id, device_id, client_id, kept_at) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![item_id, device_id, client_id, now],
            )?;
            Ok(())
        })
    }

    pub fn remote_keep(&self, device_id: &str, client_id: &str) -> Result<Option<String>> {
        self.read(|conn| {
            Ok(conn
                .query_row(
                    "SELECT item_id FROM remote_keeps WHERE device_id = ?1 AND client_id = ?2",
                    [device_id, client_id],
                    |r| r.get(0),
                )
                .optional()?)
        })
    }

    /// The client id a phone gave the keep that made `item_id`.
    pub fn remote_keep_client(&self, item_id: &str) -> Result<Option<String>> {
        self.read(|conn| {
            Ok(conn
                .query_row(
                    "SELECT client_id FROM remote_keeps WHERE item_id = ?1",
                    [item_id],
                    |r| r.get(0),
                )
                .optional()?)
        })
    }

    pub fn remote_keep_owner(&self, item_id: &str) -> Result<Option<(String, Timestamp)>> {
        self.read(|conn| {
            let row: Option<(String, String)> = conn
                .query_row(
                    "SELECT device_id, kept_at FROM remote_keeps WHERE item_id = ?1",
                    [item_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            row.map(|(device, at)| Ok((device, parse_ts(&at)?)))
                .transpose()
        })
    }

    /// Records what happened, never what it held. A repeat of the newest row within a minute
    /// raises its count instead of adding a row.
    pub fn log_remote_event(
        &self,
        device_id: Option<&str>,
        action: RemoteAction,
        result: RemoteResult,
        bytes: f64,
    ) -> Result<()> {
        let now = self.clock_now();
        let (action, result) = (enum_str(&action), enum_str(&result));
        self.write(|tx| {
            let newest: Option<(i64, Option<String>, String, String, String)> = tx
                .query_row(
                    "SELECT id, device_id, action, result, at FROM remote_events \
                     ORDER BY at DESC, id DESC LIMIT 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;
            if let Some((id, dev, act, res, at)) = newest
                && dev.as_deref() == device_id
                && act == action
                && res == result
                && now.duration_since(parse_ts(&at)?) <= SignedDuration::from_secs(COLLAPSE_SECS)
            {
                tx.execute(
                    "UPDATE remote_events SET count = count + 1, bytes = bytes + ?2, at = ?3 \
                     WHERE id = ?1",
                    params![id, bytes, fmt_ts(now)],
                )?;
                return Ok(());
            }
            tx.execute(
                "INSERT INTO remote_events(device_id, action, result, bytes, count, at) \
                 VALUES (?1, ?2, ?3, ?4, 1, ?5)",
                params![device_id, action, result, bytes, fmt_ts(now)],
            )?;
            Ok(())
        })
    }

    pub fn remote_events(&self, limit: u32) -> Result<Vec<RemoteEvent>> {
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT d.name, e.action, e.at, e.result, e.count FROM remote_events e \
                 LEFT JOIN remote_devices d ON d.id = e.device_id \
                 ORDER BY e.at DESC, e.id DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map([limit], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, u32>(4)?,
                ))
            })?;
            rows.map(|row| {
                let (device_name, action, at, result, count) = row?;
                Ok(RemoteEvent {
                    device_name,
                    action: parse_enum(&action)?,
                    at: fmt_ts(parse_ts(&at)?),
                    result: parse_enum(&result)?,
                    count,
                })
            })
            .collect()
        })
    }

    pub fn prune_remote_events(&self, before: Timestamp) -> Result<u32> {
        self.write(|tx| {
            let n = tx.execute("DELETE FROM remote_events WHERE at < ?1", [fmt_ts(before)])?;
            Ok(n as u32)
        })
    }

    /// Which of these item and memory ids must not reach a phone: memories that are private in
    /// any version, items that are [`withheld_from_phones`], and ids the store does not know
    /// (fail closed).
    pub fn private_ids(&self, ids: &[String]) -> Result<Vec<String>> {
        self.read(|conn| {
            let mut out = Vec::new();
            for id in ids {
                let private = if id.starts_with("mem_") {
                    crate::memory::is_private_chain(conn, id)?
                } else {
                    withheld_from_phones(conn, id)?
                };
                if private {
                    out.push(id.clone());
                }
            }
            Ok(out)
        })
    }

    pub fn add_sent_to_phones(&self, bytes: u64) -> Result<()> {
        self.write(|tx| {
            tx.execute(
                "UPDATE remote_state SET sent_bytes = sent_bytes + ?1 WHERE id = 1",
                [bytes as f64],
            )?;
            Ok(())
        })
    }
}
