//! A memory that moved here from a phone. The phone's backup is unpacked and opened where it
//! waits; before it takes the place of the store in use it is given what belongs to this
//! computer rather than to the memory, and the phone's own row becomes the phone as this computer
//! has it paired.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};

use crate::Engine;
use crate::error::{Error, Result};

/// What this computer keeps of its own across a move: Away from home and Online answers, the
/// phones paired with it (never the phone's own row) and what they did. Each is copied whole into
/// a temporary table first, so the store in use is only ever read and never locked for writing.
const TAKEN: [(&str, &str, &str); 4] = [
    ("remote_state", "arriving_remote_state", ""),
    ("online_state", "arriving_online_state", ""),
    ("remote_events", "arriving_events", ""),
    (
        "remote_devices",
        "arriving_devices",
        " WHERE connection != 'local'",
    ),
];

/// Nothing in the arriving store may let a phone in: every pairing it carries loses its key and
/// counts as removed. The rows stay, so receipts still name them.
const DISARM: &str = "UPDATE main.remote_devices SET public_key = x'', address = NULL, \
    revoked_at = COALESCE(revoked_at, ?1) WHERE connection != 'local'";

const DROP: &str = "
    DROP TABLE IF EXISTS temp.arriving_remote_state;
    DROP TABLE IF EXISTS temp.arriving_online_state;
    DROP TABLE IF EXISTS temp.arriving_events;
    DROP TABLE IF EXISTS temp.arriving_devices;";

/// The columns of `table` in `schema`, in order.
fn columns(conn: &Connection, schema: &str, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA {schema}.table_info({table})"))?;
    let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
    Ok(names.collect::<rusqlite::Result<_>>()?)
}

/// The columns both stores have, without `skip`, so a store a version apart (the one in use is
/// migrated only once it opens) still settles: a column one side lacks keeps its default.
fn shared(conn: &Connection, table: &str, copy: &str, skip: &str) -> Result<Vec<String>> {
    let there = columns(conn, "temp", copy)?;
    let both: Vec<String> = columns(conn, "main", table)?
        .into_iter()
        .filter(|c| c != skip && there.contains(c))
        .collect();
    if both.is_empty() {
        return Err(Error::invalid(format!("{table} has no columns in common")));
    }
    Ok(both)
}

/// This computer's own settings, and its paired phones, which take the place of a disarmed row
/// by id or are added. The phone's memory keeps none of its own.
fn settle(tx: &Connection) -> Result<()> {
    for (table, copy, skip) in [
        ("remote_state", "arriving_remote_state", ""),
        ("online_state", "arriving_online_state", ""),
        ("remote_events", "arriving_events", "id"),
    ] {
        let cols = shared(tx, table, copy, skip)?.join(", ");
        tx.execute_batch(&format!(
            "DELETE FROM main.{table}; \
             INSERT INTO main.{table}({cols}) SELECT {cols} FROM temp.{copy};"
        ))?;
    }
    let devices = shared(tx, "remote_devices", "arriving_devices", "")?;
    let update = devices
        .iter()
        .filter(|c| *c != "id")
        .map(|c| format!("{c} = excluded.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    let devices = devices.join(", ");
    tx.execute_batch(&format!(
        "INSERT INTO main.remote_devices({devices}) \
         SELECT {devices} FROM temp.arriving_devices WHERE true \
         ON CONFLICT(id) DO UPDATE SET {update};"
    ))?;
    Ok(())
}

impl Engine {
    /// Make this store, a phone's memory unpacked from its backup, ready to be this computer's.
    /// It takes the computer's own settings and paired phones from the store in `here`, and the
    /// phone's own row becomes `paired`, as [`Engine::rehome_device`] does, in the same write
    /// transaction: either all of it holds or nothing changed, so the own row never shows in
    /// the phones list. Refused when `paired` is not a live pairing in `here` or already has a
    /// history here.
    pub fn settle_arrival(&self, here: &Path, paired: &str) -> Result<()> {
        self.take_from(here, Some(paired))
    }

    /// Take the computer's own settings and paired phones from `here` again, just before a
    /// settled arrival takes its place: what changed on the computer since it was settled (a
    /// phone removed or added, a new address) holds.
    pub fn retake_computer(&self, here: &Path) -> Result<()> {
        self.take_from(here, None)
    }

    fn take_from(&self, here: &Path, paired: Option<&str>) -> Result<()> {
        let db = here.join(crate::DB_FILE);
        if !db.is_file() {
            return Err(Error::not_found("the store in use"));
        }
        let text = db
            .to_str()
            .ok_or_else(|| Error::invalid("the store path is not valid text"))?;
        self.write_raw(|conn| {
            conn.execute_batch(DROP)?;
            conn.execute("ATTACH DATABASE ?1 AS here", [text])?;
            let took = TAKEN.iter().try_for_each(|(table, copy, only)| {
                conn.execute_batch(&format!(
                    "CREATE TEMP TABLE {copy} AS SELECT * FROM here.{table}{only};"
                ))
            });
            let detached = conn.execute_batch("DETACH DATABASE here");
            took?;
            detached?;
            Ok(())
        })?;
        let now = crate::util::fmt_ts(self.clock_now());
        let settled = self.write(|tx| {
            tx.execute(DISARM, [&now])?;
            settle(tx)?;
            let Some(paired) = paired else {
                return Ok(());
            };
            let local: Option<String> = tx
                .query_row(
                    "SELECT id FROM main.remote_devices WHERE connection = 'local'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            match local {
                Some(local) => crate::remote::rehome(tx, &local, paired),
                None => Err(Error::not_found("the phone's own row")),
            }
        });
        let dropped = self.write_raw(|conn| Ok(conn.execute_batch(DROP)?));
        settled?;
        dropped
    }
}
