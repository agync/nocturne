//! Bringing a phone's things into this store, when the computer already holds a memory
//! (`docs/design/moving-memory.md`, "When the computer already holds a memory"). The phone's
//! memory arrives as a backup, unpacked where it waits; this reads it and adds, in one write
//! transaction, every kept thing with its original, how and where it was kept and its private
//! mark, the journal and the check-ins. Nothing a mind worked out on the phone comes: no memories,
//! pages, reflections or mentions. Calendar entries and health readings stay behind too; the phone
//! sends them again. What arrives waits to be read as new; finding it by its words works at once.
//! What the person forgot on the phone stays forgotten: a thing a forget removes does not come,
//! and the claims forgotten from a thing that stays come with it, so no read here says them again.
//!
//! Each thing keeps its id, so bringing the same memory in twice adds nothing the second time.
//! A thing already here (the same thing kept on both, one the phone sent as a companion of this
//! computer, or one brought in before) is kept once: the one here stays, takes the phone's
//! private mark, and notes the phone among the places it was kept (`$.alsoKeptOn`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::types::Value as Sql;
use rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter};
use serde_json::{Value, json};

use crate::Engine;
use crate::capture::{extension_ok, original_path};
use crate::carried::{self, Marks};
use crate::error::{Error, Result};
use crate::util::{new_id, random_hex};

mod derived;
mod entries;

/// What bringing a phone's things in added, and what it found already here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BroughtIn {
    /// Things kept, apart from the journal and check-ins.
    pub things: u32,
    /// Journal entries and good things, as they stand now.
    pub entries: u32,
    /// Check-ins, as they stand now.
    pub checkins: u32,
    /// Things kept on both: the one here stays.
    pub same: u32,
    /// Things already here: sent earlier as a companion of this computer, or brought in before.
    pub already: u32,
}

/// How the things arrive: a phone's memory moving here, which this store reads again as new, or
/// a backup of a phone's own memory, which comes as it was: with what its mind worked out, read
/// and sent online once already ([`derived`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bring {
    Move,
    Backup,
}

/// How the things arrive, and what a backup holds beside them: the records of its calendar
/// entries and health readings. A move brings none.
struct Notes {
    bring: Bring,
    records: Vec<entries::Record>,
}

impl Notes {
    fn of(bring: Bring) -> Notes {
        Notes {
            bring,
            records: Vec::new(),
        }
    }
}

/// Metadata that names another item by its id.
const REFS: [&str; 3] = ["replaces", "about", "photo"];

/// Kinds a phone sends again rather than bringing.
const SENT_AGAIN: &str = "('calendar', 'reading')";

/// Columns reset on arrival from a move: the computer reads each thing as new.
const FRESH: [&str; 13] = [
    "reason",
    "lease_until",
    "lease_token",
    "attempts",
    "failures",
    "status_before",
    "reread",
    "understood_in_ms",
    "priority",
    "forget_epoch",
    "lease_epoch",
    "hold_until",
    "summary",
];

/// One thing on the phone, as its row reads, with what the merge needs to decide its fate.
struct Arriving {
    row: Vec<Sql>,
    id: String,
    kind: String,
    sha: String,
    ext: Option<String>,
    file: bool,
    readable: bool,
    private: bool,
    kept_at: String,
    meta: Value,
    /// The client id it was kept under, when a keep on the phone itself made it.
    client: Option<String>,
    /// The name of the device it was kept on, as the phone knew it.
    place: Option<String>,
    /// Claims the person forgot from it on the phone, as hashes.
    forgotten: Vec<String>,
    /// The person's marks on what the phone's mind read from it, by claim.
    marks: Vec<(String, Marks)>,
}

/// A calendar entry the phone sends again, with what was forgotten or marked of it there.
struct Resent {
    source: String,
    phone_id: String,
    forgotten: Vec<String>,
    marks: Vec<(String, Marks)>,
}

impl Arriving {
    fn role(&self) -> Option<&str> {
        self.meta.get("journal").and_then(Value::as_str)
    }

    fn checkin(&self) -> bool {
        self.kind == "checkin" || self.role() == Some("checkin")
    }

    fn entry_day(&self) -> Option<&str> {
        (self.role() == Some("entry"))
            .then(|| self.meta.get("journal_day").and_then(Value::as_str))
            .flatten()
    }

    fn block(&self) -> u64 {
        self.meta.get("block").and_then(Value::as_u64).unwrap_or(0)
    }
}

enum Fate {
    /// Added under this id.
    Add(String),
    /// Kept on both: the one here, by id.
    Same(String),
    /// Already here, by id.
    Already(String),
}

impl Fate {
    fn here(&self) -> &str {
        match self {
            Fate::Add(id) | Fate::Same(id) | Fate::Already(id) => id,
        }
    }
}

fn columns(conn: &Connection, schema: &str, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA {schema}.table_info({table})"))?;
    let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
    Ok(names.collect::<rusqlite::Result<_>>()?)
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn text(v: &Sql) -> Option<String> {
    match v {
        Sql::Text(s) => Some(s.clone()),
        _ => None,
    }
}

/// Items a started forget still has to remove: they are as good as gone. Only a forget that takes
/// originals removes items, and then only those no memory outside a forget still cites, and no
/// other started forget leaves in place; the others stay on the phone, so they travel, with what
/// was forgotten from them. A thing goes with every earlier version of it.
fn forgetting(phone: &Connection) -> Result<HashSet<String>> {
    let mut stmt = phone.prepare(&format!(
        "SELECT i.id FROM items i WHERE {}",
        crate::forget::going_by_forgets("i.id")
    ))?;
    let items: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut gone = HashSet::new();
    for item in items {
        gone.extend(crate::journal::keep::earlier_versions(phone, &item)?);
        gone.insert(item);
    }
    Ok(gone)
}

/// What the person forgot on the phone, as the claims kept for each item that said them, so the
/// computer never reads them from it again.
fn forgotten(phone: &Connection) -> Result<HashMap<String, Vec<String>>> {
    let mut stmt = phone.prepare("SELECT item_id, claim FROM forgotten_claims")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for row in rows {
        let (item, claim) = row?;
        out.entry(item).or_default().push(claim);
    }
    Ok(out)
}

/// The person's marks on what the phone's mind worked out, by the item it came from and claim.
fn marked(phone: &Connection) -> Result<HashMap<String, Vec<(String, Marks)>>> {
    let mut stmt = phone.prepare(
        "SELECT s.item_id, m.subject_key, m.moved_from_key, m.predicate, m.object_key, m.private, \
         m.done, m.keep_forever, m.tags FROM memories m JOIN memory_sources s ON s.memory_id = m.id \
         WHERE m.forgetting = 0 AND m.superseded_at IS NULL \
           AND (m.private = 1 OR m.done = 1 OR m.keep_forever = 1 OR m.tags != '[]')",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            carried::claims_of(
                &r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?.as_deref(),
                &r.get::<_, String>(3)?,
                &r.get::<_, String>(4)?,
            ),
            Marks {
                private: r.get(5)?,
                done: r.get(6)?,
                keep_forever: r.get(7)?,
                tags: serde_json::from_str(&r.get::<_, String>(8)?).unwrap_or_default(),
            },
        ))
    })?;
    let mut out: HashMap<String, Vec<(String, Marks)>> = HashMap::new();
    for row in rows {
        let (item, claims, marks) = row?;
        let list = out.entry(item).or_default();
        for claim in claims {
            list.push((claim, marks.clone()));
        }
    }
    Ok(out)
}

/// The calendar entries the phone sends again that carry something forgotten or marked, by the
/// phone's own id for each. Every version of an entry names it.
fn resent(
    phone: &Connection,
    claims: &mut HashMap<String, Vec<String>>,
    marks: &mut HashMap<String, Vec<(String, Marks)>>,
) -> Result<Vec<Resent>> {
    let mut stmt = phone.prepare(&format!(
        "SELECT id, json_extract(metadata, '$.phone.source'), json_extract(metadata, '$.phone.phoneId') \
         FROM items WHERE kind IN {SENT_AGAIN}"
    ))?;
    let rows: Vec<(String, Option<String>, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut by_entry: HashMap<(String, String), Resent> = HashMap::new();
    for (id, source, phone_id) in rows {
        let forgotten = claims.remove(&id).unwrap_or_default();
        let marked = marks.remove(&id).unwrap_or_default();
        let (Some(source), Some(phone_id)) = (source, phone_id) else {
            continue;
        };
        if forgotten.is_empty() && marked.is_empty() {
            continue;
        }
        let entry = by_entry
            .entry((source.clone(), phone_id.clone()))
            .or_insert_with(|| Resent {
                source,
                phone_id,
                forgotten: Vec::new(),
                marks: Vec::new(),
            });
        entry.forgotten.extend(forgotten);
        entry.marks.extend(marked);
    }
    Ok(by_entry.into_values().collect())
}

/// Every thing on the phone that may travel, oldest first, read with the columns both stores
/// share.
fn read_phone(
    phone: &Connection,
    shared: &[String],
    name: &str,
    bring: Bring,
) -> Result<(Vec<Arriving>, Vec<Resent>)> {
    let at = |c: &str| shared.iter().position(|s| s == c);
    let need = |c: &str| at(c).ok_or_else(|| Error::invalid(format!("the phone's items lack {c}")));
    let (id, kind, sha, ext, file, txt, private, kept, meta) = (
        need("id")?,
        need("kind")?,
        need("sha256")?,
        need("ext")?,
        need("file_name")?,
        need("text")?,
        need("private")?,
        need("kept_at")?,
        need("metadata")?,
    );
    let gone = forgetting(phone)?;
    let mut claims = forgotten(phone)?;
    let mut marks = marked(phone)?;
    // A backup brings calendar entries and health readings itself, with what was forgotten or
    // marked of them; after a move the phone sends them again.
    let again = match bring {
        Bring::Move => resent(phone, &mut claims, &mut marks)?,
        Bring::Backup => Vec::new(),
    };
    // The device each phone keep came through, and the name of any that is not the phone itself.
    let mut keeps: HashMap<String, (String, Option<String>)> = HashMap::new();
    {
        let mut stmt = phone.prepare(
            "SELECT rk.item_id, rk.client_id, CASE WHEN rd.connection = 'local' THEN NULL \
             ELSE rd.name END FROM remote_keeps rk LEFT JOIN remote_devices rd \
             ON rd.id = rk.device_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        for row in rows {
            let (item, client, through) = row?;
            keeps.insert(item, (client, through));
        }
    }
    let cols = shared.join(", ");
    let only = match bring {
        Bring::Move => format!("WHERE kind NOT IN {SENT_AGAIN}"),
        Bring::Backup => String::new(),
    };
    let mut stmt = phone.prepare(&format!(
        "SELECT {cols} FROM items {only} ORDER BY kept_at, rowid"
    ))?;
    let width = shared.len();
    let rows = stmt.query_map([], |r| {
        (0..width)
            .map(|i| r.get::<_, Sql>(i))
            .collect::<rusqlite::Result<Vec<Sql>>>()
    })?;
    let mut out = Vec::new();
    for row in rows {
        let row = row?;
        let item_id = text(&row[id]).ok_or_else(|| Error::invalid("an item has no id"))?;
        if gone.contains(&item_id) {
            continue;
        }
        let sha256 = text(&row[sha]).unwrap_or_default();
        let ext = text(&row[ext]);
        let has_file = !matches!(row[file], Sql::Null);
        if has_file && (!is_sha256(&sha256) || ext.as_deref().is_some_and(|e| !extension_ok(e))) {
            return Err(Error::invalid("an original's name is not a stored name"));
        }
        let meta: Value = text(&row[meta])
            .and_then(|m| serde_json::from_str(&m).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        // Only a keep through the phone itself names a client this computer may see again.
        let (client, through) = match keeps.remove(&item_id) {
            Some((client, None)) => (Some(client), None),
            Some((_, through)) => (None, through),
            None => (None, None),
        };
        let place = meta
            .get("keptOn")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or(through)
            .or_else(|| Some(name.to_string()));
        let forgotten = claims.remove(&item_id).unwrap_or_default();
        let marks = marks.remove(&item_id).unwrap_or_default();
        out.push(Arriving {
            forgotten,
            marks,
            id: item_id,
            kind: text(&row[kind]).unwrap_or_default(),
            sha: sha256,
            ext,
            file: has_file,
            readable: has_file || !matches!(row[txt], Sql::Null),
            private: matches!(row[private], Sql::Integer(n) if n != 0),
            kept_at: text(&row[kept]).unwrap_or_default(),
            meta,
            client,
            place,
            row,
        });
    }
    Ok((out, again))
}

/// The client id of the keep that made a journal item on the phone, from `$.keep`
/// (`{device}/{client}`).
fn journal_client(meta: &Value) -> Option<&str> {
    meta.get("keep")?.as_str()?.split_once('/').map(|(_, c)| c)
}

/// What becomes of one thing: already here by its id, kept here already (the same original or
/// words, or the same phone keep), or added.
fn fate(tx: &Transaction<'_>, device: &str, p: &Arriving, taken: &HashSet<String>) -> Result<Fate> {
    let same_id: Option<String> = tx
        .query_row("SELECT sha256 FROM items WHERE id = ?1", [&p.id], |r| {
            r.get(0)
        })
        .optional()?;
    if same_id.as_deref() == Some(p.sha.as_str()) {
        return Ok(Fate::Already(p.id.clone()));
    }
    let same_sha: Option<String> = tx
        .query_row("SELECT id FROM items WHERE sha256 = ?1", [&p.sha], |r| {
            r.get(0)
        })
        .optional()?;
    if let Some(here) = same_sha {
        return Ok(Fate::Same(here));
    }
    if let Some(client) = &p.client {
        let sent: Option<String> = tx
            .query_row(
                "SELECT item_id FROM remote_keeps WHERE device_id = ?1 AND client_id = ?2",
                [device, client],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(here) = sent {
            return Ok(Fate::Already(here));
        }
    }
    if let (Some(role), Some(client)) = (p.role(), journal_client(&p.meta)) {
        let sent: Option<String> = tx
            .query_row(
                "SELECT id FROM items WHERE json_extract(metadata, '$.journal') = ?1 \
                 AND json_extract(metadata, '$.keep') = ?2",
                params![role, format!("{device}/{client}")],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(here) = sent {
            return Ok(Fate::Already(here));
        }
    }
    // An id that is taken here by something else, or by another thing arriving, gets a new one.
    let id = if same_id.is_some() || taken.contains(&p.id) {
        new_id("itm")
    } else {
        p.id.clone()
    };
    Ok(Fate::Add(id))
}

/// Entry blocks of `day` on both sides, numbered again in the order each was first kept, so
/// nothing is joined into one entry and the day reads in time order. Answers the new number of
/// each arriving block; blocks here are renumbered in place, by id.
fn number_blocks(
    tx: &Transaction<'_>,
    day: &str,
    arriving: &[(u64, &str)],
) -> Result<HashMap<u64, u64>> {
    let mut stmt = tx.prepare(
        "SELECT id, COALESCE(json_extract(metadata, '$.block'), 0), kept_at FROM items \
         WHERE json_extract(metadata, '$.journal') = 'entry' \
         AND json_extract(metadata, '$.journal_day') = ?1",
    )?;
    let here: Vec<(String, u64, String)> = stmt
        .query_map([day], |r| {
            Ok((r.get(0)?, r.get::<_, i64>(1)? as u64, r.get(2)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut first: HashMap<(bool, u64), String> = HashMap::new();
    for (_, block, at) in &here {
        let e = first.entry((false, *block)).or_insert_with(|| at.clone());
        if at < e {
            *e = at.clone();
        }
    }
    for (block, at) in arriving {
        let e = first
            .entry((true, *block))
            .or_insert_with(|| at.to_string());
        if *at < e.as_str() {
            *e = at.to_string();
        }
    }
    let mut order: Vec<((bool, u64), String)> = first.into_iter().collect();
    order.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    let number: HashMap<(bool, u64), u64> = order
        .iter()
        .enumerate()
        .map(|(n, (key, _))| (*key, n as u64))
        .collect();
    for (id, block, _) in &here {
        let new = number[&(false, *block)];
        if new != *block {
            tx.execute(
                "UPDATE items SET metadata = json_set(metadata, '$.block', ?2) WHERE id = ?1",
                params![id, new as i64],
            )?;
        }
    }
    Ok(number
        .into_iter()
        .filter(|((phone, _), _)| *phone)
        .map(|((_, block), n)| (block, n))
        .collect())
}

/// The name of the place a thing here was kept, or none when it was kept on this computer.
fn place_here(tx: &Transaction<'_>, id: &str) -> Result<Option<String>> {
    Ok(tx.query_row(
        "SELECT COALESCE((SELECT rd.name FROM remote_keeps rk JOIN remote_devices rd \
           ON rd.id = rk.device_id AND rd.connection != 'local' WHERE rk.item_id = i.id), \
           json_extract(i.metadata, '$.keptOn')) FROM items i WHERE i.id = ?1",
        [id],
        |r| r.get(0),
    )?)
}

/// A thing kept on both stays as it is here; the phone's private mark holds, and the phone joins
/// the places it was kept. Answers cached audio to shred once committed.
fn join_same(tx: &Transaction<'_>, here: &str, p: &Arriving) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let private: bool = tx.query_row("SELECT private FROM items WHERE id = ?1", [here], |r| {
        r.get(0)
    })?;
    if p.private && !private {
        tx.execute("UPDATE items SET private = 1 WHERE id = ?1", [here])?;
        for m in crate::lifecycle::memories_of(tx, here, false)? {
            files.extend(crate::lifecycle::set_private_tx(tx, &m, true)?);
        }
    }
    if let Some(place) = &p.place
        && place_here(tx, here)?.as_deref() != Some(place.as_str())
    {
        let had: Option<String> = tx.query_row(
            "SELECT json_extract(metadata, '$.alsoKeptOn') FROM items WHERE id = ?1",
            [here],
            |r| r.get(0),
        )?;
        let mut places: Vec<Value> = had
            .and_then(|h| serde_json::from_str(&h).ok())
            .unwrap_or_default();
        let named = |v: &Value| v.as_str().or_else(|| v.get("name")?.as_str()) == Some(place);
        if !places.iter().any(named) {
            let at = (!p.kept_at.is_empty()).then_some(p.kept_at.as_str());
            places.push(json!({"name": place, "at": at}));
            tx.execute(
                "UPDATE items SET metadata = json_set(metadata, '$.alsoKeptOn', json(?2)) \
                 WHERE id = ?1",
                params![
                    here,
                    serde_json::to_string(&places).expect("names serialise")
                ],
            )?;
        }
    }
    Ok(files)
}

/// What the person forgot from a thing on the phone holds for it here: kept with the claims
/// forgotten here, so no read says them again. A thing already here moves to a new forget epoch,
/// as a forget here does, so a read that began before cannot write them either.
fn carry_forgotten(tx: &Transaction<'_>, here: &str, p: &Arriving, added: bool) -> Result<()> {
    let mut new = 0;
    for claim in &p.forgotten {
        new += tx.execute(
            "INSERT OR IGNORE INTO forgotten_claims(item_id, claim) VALUES (?1, ?2)",
            params![here, claim],
        )?;
    }
    if new > 0 && !added {
        tx.execute(
            "UPDATE items SET forget_epoch = forget_epoch + 1, reread = 0 WHERE id = ?1",
            [here],
        )?;
    }
    Ok(())
}

/// What was forgotten or marked of calendar entries the phone sends again is kept for each, so
/// every version that comes takes it; an entry here already (sent while the phone was a
/// companion) takes it now. Answers cached audio to shred once committed.
fn carry_resent(tx: &Transaction<'_>, device: &str, again: &[Resent]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for r in again {
        for claim in &r.forgotten {
            carried::keep_for_phone(
                tx,
                device,
                &r.source,
                &r.phone_id,
                claim,
                true,
                &Marks::default(),
            )?;
        }
        for (claim, marks) in &r.marks {
            carried::keep_for_phone(tx, device, &r.source, &r.phone_id, claim, false, marks)?;
        }
        let here: Option<String> = tx
            .query_row(
                "SELECT item_id FROM phone_records WHERE device_id = ?1 AND source = ?2 \
                 AND phone_id = ?3",
                params![device, r.source, r.phone_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(item) = here {
            carried::take_for_phone(tx, device, &r.source, &r.phone_id, &item)?;
            if !r.forgotten.is_empty() {
                tx.execute(
                    "UPDATE items SET forget_epoch = forget_epoch + 1, reread = 0 WHERE id = ?1",
                    [&item],
                )?;
            }
            files.extend(carried::apply(tx, &item)?);
        }
    }
    Ok(files)
}

/// The originals the arriving things need that are not here yet, copied beside the others
/// under passing names. They are put in place under the store's lock, as a keep does, so a keep
/// or a forget running meanwhile never finds a file that has no row or loses one that has.
#[derive(Default)]
struct Staged {
    /// Each copy waiting, by its digest.
    waiting: HashMap<String, PathBuf>,
    /// Digests the phone's backup holds no original for.
    missing: HashSet<String>,
    /// Originals put in place by this merge, taken away again should it not commit.
    placed: Vec<(String, PathBuf)>,
}

impl Staged {
    fn copy(from: &Path, to: &Path, items: &[&Arriving]) -> Result<Staged> {
        let mut staged = Staged::default();
        for p in items.iter().filter(|p| p.file) {
            if staged.waiting.contains_key(&p.sha) {
                continue;
            }
            let src = original_path(from, &p.sha, p.ext.as_deref());
            if !src.is_file() {
                staged.missing.insert(p.sha.clone());
                continue;
            }
            let tmp = to.join(format!(".tmp-{}", random_hex(8)));
            let written = (|| {
                let mut input = std::fs::File::open(&src)?;
                let mut output = crate::fsutil::create_private_file(&tmp)?;
                std::io::copy(&mut input, &mut output)?;
                output.sync_all()?;
                Ok::<_, Error>(())
            })();
            if let Err(e) = written {
                let _ = std::fs::remove_file(&tmp);
                staged.drop_waiting();
                return Err(e);
            }
            staged.waiting.insert(p.sha.clone(), tmp);
        }
        Ok(staged)
    }

    /// Put the original of a thing being added in place, under the store's lock. Refused as
    /// busy when it is not here and no copy waits for it (something took it away since the
    /// copies were made): the phone carries on, and it is copied again then.
    fn place(&mut self, to: &Path, p: &Arriving) -> Result<()> {
        if !p.file {
            return Ok(());
        }
        let dest = original_path(to, &p.sha, p.ext.as_deref());
        if dest.exists() {
            return Ok(());
        }
        match self.waiting.remove(&p.sha) {
            Some(tmp) => {
                std::fs::rename(&tmp, &dest)?;
                self.placed.push((p.sha.clone(), dest));
                Ok(())
            }
            None if self.missing.contains(&p.sha) => Ok(()),
            None => Err(Error::Busy),
        }
    }

    fn drop_waiting(&mut self) {
        for (_, tmp) in self.waiting.drain() {
            let _ = std::fs::remove_file(tmp);
        }
    }
}

impl Engine {
    /// Bring the phone's memory in the store folder `staged` (a backup unpacked where it waits)
    /// into this one, as things kept through paired phone `device`. All of it is added in one
    /// write transaction, or none of it. Refused when `device` is not a live pairing here.
    pub fn bring_in(&self, staged: &Path, device: &str) -> Result<BroughtIn> {
        let db = staged.join(crate::DB_FILE);
        if !db.is_file() {
            return Err(Error::not_found("the phone's memory"));
        }
        let name: Option<String> = self.read(|conn| {
            Ok(conn
                .query_row(
                    "SELECT name FROM remote_devices WHERE id = ?1 AND connection != 'local' \
                     AND revoked_at IS NULL",
                    [device],
                    |r| r.get(0),
                )
                .optional()?)
        })?;
        let name = name.ok_or_else(|| Error::not_found(format!("device {device}")))?;
        self.bring_in_as(staged, device, &name, Bring::Move)
    }

    /// Bring a backup of a phone's own memory, unpacked in the store folder `staged`, into the
    /// memory here (`docs/design/phone-backup.md`, "Onto a phone that has a memory"): nothing here
    /// is put aside or removed, and a thing on both is kept once. Unlike [`Engine::bring_in`],
    /// what comes in comes as it was: each thing with its reading, the memories, pages and
    /// reflections worked out from it and the names taught, so nothing is read or sent online
    /// again. What comes in is kept through this phone's own row.
    pub fn bring_in_backup(&self, staged: &Path) -> Result<BroughtIn> {
        let db = staged.join(crate::DB_FILE);
        if !db.is_file() {
            return Err(Error::not_found("the backup's memory"));
        }
        let own: Option<(String, String)> = self.read(|conn| {
            Ok(conn
                .query_row(
                    "SELECT id, name FROM remote_devices WHERE connection = 'local' \
                     ORDER BY paired_at LIMIT 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?)
        })?;
        let (device, name) = own.ok_or_else(|| Error::not_found("this phone's own row"))?;
        self.bring_in_as(staged, &device, &name, Bring::Backup)
    }

    fn bring_in_as(
        &self,
        staged: &Path,
        device: &str,
        name: &str,
        bring: Bring,
    ) -> Result<BroughtIn> {
        let db = staged.join(crate::DB_FILE);
        let phone = crate::store::open_untrusted(&db)?;
        let here_cols = self.read(|conn| columns(conn, "main", "items"))?;
        let shared: Vec<String> = columns(&phone, "main", "items")?
            .into_iter()
            .filter(|c| here_cols.contains(c))
            .collect();
        let (arriving, again) = read_phone(&phone, &shared, name, bring)?;
        // A move needs nothing more of the phone's store; a backup brings what was worked out,
        // and its calendar entries and health readings by the phone's own ids for them.
        let (phone, notes) = match bring {
            Bring::Move => {
                drop(phone);
                (None, Notes::of(bring))
            }
            Bring::Backup => {
                let notes = Notes {
                    records: entries::records(&phone)?,
                    ..Notes::of(bring)
                };
                (Some(phone), notes)
            }
        };

        // The originals are copied first, outside the transaction, so the store is never held
        // while they are; they are put in place inside it. What the transaction then finds here
        // already needs none of them.
        let needed: Vec<&Arriving> = {
            let have: HashSet<String> = self.read(|conn| {
                let mut stmt = conn.prepare("SELECT sha256 FROM items")?;
                let shas = stmt.query_map([], |r| r.get(0))?;
                Ok(shas.collect::<rusqlite::Result<_>>()?)
            })?;
            arriving.iter().filter(|p| !have.contains(&p.sha)).collect()
        };
        let originals = self.originals_dir();
        let mut copies = Staged::copy(&staged.join("originals"), &originals, &needed)?;

        let written = self.write(|tx| {
            let (brought, placed, mut files) =
                self.add_arriving(tx, device, &arriving, &notes, &shared, &mut copies)?;
            files.extend(carry_resent(tx, device, &again)?);
            if let Some(phone) = &phone {
                files.extend(derived::bring(tx, phone, &placed)?);
            }
            Ok((brought, files))
        });
        copies.drop_waiting();
        match written {
            Ok((brought, files)) => {
                if !files.is_empty() {
                    self.bump_audio();
                }
                // Memories came with their vectors: finding by meaning sees them now.
                if bring == Bring::Backup {
                    self.invalidate_vectors();
                }
                crate::audio::shred_all(files);
                Ok(brought)
            }
            Err(e) => {
                // Only an original no row came to need since: a keep puts its own in place
                // under the same lock.
                let placed = std::mem::take(&mut copies.placed);
                let _ = self.write(|tx| {
                    for (sha, file) in &placed {
                        let used: bool = tx.query_row(
                            "SELECT EXISTS(SELECT 1 FROM items WHERE sha256 = ?1)",
                            [sha],
                            |r| r.get(0),
                        )?;
                        if !used {
                            crate::fsutil::shred(file);
                        }
                    }
                    Ok(())
                });
                Err(e)
            }
        }
    }

    fn add_arriving(
        &self,
        tx: &Transaction<'_>,
        device: &str,
        arriving: &[Arriving],
        notes: &Notes,
        shared: &[String],
        copies: &mut Staged,
    ) -> Result<(BroughtIn, derived::Placed, Vec<PathBuf>)> {
        // A forget here that has not finished yet still removes its things and their originals:
        // a thing it names could swallow the phone's own copy of it, or lose the original an
        // arriving thing shares. Nothing is added until it is done; the phone carries on.
        let forgetting: bool =
            tx.query_row("SELECT EXISTS(SELECT 1 FROM forget_jobs)", [], |r| r.get(0))?;
        if forgetting {
            return Err(Error::Busy);
        }
        let originals = self.originals_dir();
        let mut brought = BroughtIn::default();
        let mut files = Vec::new();
        // An entry version the plan leaves out comes only when it is here already.
        let bring = notes.bring;
        let plan = entries::plan(tx, device, &notes.records, arriving)?;
        let mut kept: Vec<&Arriving> = Vec::with_capacity(arriving.len());
        for p in arriving {
            let comes = !entries::is_entry(&p.kind)
                || plan.keep.contains(&p.id)
                || tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM items WHERE sha256 = ?1)",
                    [&p.sha],
                    |r| r.get(0),
                )?;
            if comes {
                kept.push(p);
            }
        }
        let arriving = kept;
        let mut fates: Vec<Fate> = Vec::with_capacity(arriving.len());
        let mut taken: HashSet<String> = HashSet::new();
        for p in &arriving {
            let f = fate(tx, device, p, &taken)?;
            if let Fate::Add(id) = &f {
                taken.insert(id.clone());
            }
            fates.push(f);
        }
        let map: HashMap<&str, &str> = arriving
            .iter()
            .zip(&fates)
            .map(|(p, f)| (p.id.as_str(), f.here()))
            .collect();

        // A photo or voice note in an entry on the phone that is a plain thing here takes its
        // place in the entry: the one here stays, and the entry keeps its block.
        let mut placing: HashSet<usize> = HashSet::new();
        for (n, (p, f)) in arriving.iter().zip(&fates).enumerate() {
            // As in placing one by hand: only a photo or a voice note, never words.
            let media = matches!(p.kind.as_str(), "image" | "audio");
            if matches!(f, Fate::Add(_)) || p.entry_day().is_none() || !media {
                continue;
            }
            let (role, private): (Option<String>, bool) = tx.query_row(
                "SELECT json_extract(metadata, '$.journal'), private FROM items WHERE id = ?1",
                [f.here()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if role.is_none() && !private && !p.private {
                placing.insert(n);
            }
        }

        // Entry blocks of days both sides wrote on, in time order.
        let mut days: HashMap<&str, Vec<(u64, &str)>> = HashMap::new();
        for (n, (p, f)) in arriving.iter().zip(&fates).enumerate() {
            let adds = matches!(f, Fate::Add(_)) || placing.contains(&n);
            if let (true, Some(day)) = (adds, p.entry_day()) {
                days.entry(day)
                    .or_default()
                    .push((p.block(), p.kept_at.as_str()));
            }
        }
        let mut blocks: HashMap<&str, HashMap<u64, u64>> = HashMap::new();
        for (day, list) in &days {
            blocks.insert(day, number_blocks(tx, day, list)?);
        }

        let at = |c: &str| shared.iter().position(|s| s == c);
        let cols = shared.join(", ");
        let marks = (1..=shared.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let insert = format!("INSERT INTO items({cols}) VALUES ({marks})");
        for (n, (p, f)) in arriving.iter().zip(&fates).enumerate() {
            let id = match f {
                Fate::Add(id) => id,
                Fate::Same(here) | Fate::Already(here) => {
                    // Calendar entries and health readings are not things the person counts.
                    if entries::is_entry(&p.kind) {
                    } else if matches!(f, Fate::Same(_)) {
                        brought.same += 1;
                    } else {
                        brought.already += 1;
                    }
                    files.extend(join_same(tx, here, p)?);
                    carry_forgotten(tx, here, p, false)?;
                    for (claim, marks) in &p.marks {
                        carried::keep(tx, here, claim, marks)?;
                    }
                    files.extend(carried::apply(tx, here)?);
                    if placing.contains(&n)
                        && let Some(day) = p.entry_day()
                    {
                        let block = blocks
                            .get(day)
                            .and_then(|b| b.get(&p.block()))
                            .copied()
                            .unwrap_or_else(|| p.block());
                        let mut place =
                            json!({"journal": "entry", "journal_day": day, "block": block});
                        if let Some(saved) = p.meta.get("saved_at") {
                            place["saved_at"] = saved.clone();
                        }
                        tx.execute(
                            "UPDATE items SET metadata = json_patch(metadata, ?2) WHERE id = ?1",
                            params![here, place.to_string()],
                        )?;
                        files.extend(crate::memory::mark_journal_tx(tx, here)?);
                    }
                    continue;
                }
            };
            let mut meta = p.meta.clone();
            let obj = meta.as_object_mut().expect("metadata is an object");
            for key in REFS {
                if let Some(to) = obj
                    .get(key)
                    .and_then(Value::as_str)
                    .and_then(|r| map.get(r))
                {
                    obj.insert(key.into(), json!(to));
                }
            }
            if let Some(client) = journal_client(&p.meta) {
                obj.insert("keep".into(), json!(format!("{device}/{client}")));
            }
            if let Some(place) = &p.place {
                obj.insert("keptOn".into(), json!(place));
            }
            if let Some(n) = p
                .entry_day()
                .and_then(|d| blocks.get(d))
                .and_then(|b| b.get(&p.block()))
            {
                obj.insert("block".into(), json!(n));
            }
            match bring {
                Bring::Move => {
                    obj.remove("phone");
                }
                Bring::Backup => entries::rehome(obj, device),
            }
            if let Some(before) = plan.after.get(&p.id) {
                obj.insert("replaces".into(), json!(before));
            }
            // A thing the person forgot something from is never read again by itself, as here
            // (`Engine::queue_reread`): it arrives as read, so no mind gets a second chance to say
            // what was forgotten. Only one not read yet waits to be read. From a backup, every
            // thing comes as it was.
            let fresh = bring == Bring::Move && p.readable && p.forgotten.is_empty();
            if fresh {
                obj.remove("read_at");
            }
            let mut row = p.row.clone();
            let mut set = |c: &str, v: Sql| {
                if let Some(i) = at(c) {
                    row[i] = v;
                }
            };
            set("id", Sql::Text(id.clone()));
            set("metadata", Sql::Text(meta.to_string()));
            copies.place(&originals, p)?;
            if fresh {
                set("status", Sql::Text("kept".into()));
            }
            match bring {
                Bring::Move => {
                    for c in FRESH {
                        set(
                            c,
                            match c {
                                "attempts" | "failures" | "reread" | "priority"
                                | "forget_epoch" | "lease_epoch" => Sql::Integer(0),
                                _ => Sql::Null,
                            },
                        );
                    }
                }
                Bring::Backup => {
                    // Only a read that was under way when the night was made starts again, as
                    // after a restart (`capture::reset_all_leases`).
                    let reading =
                        at("status").is_some_and(|i| text(&p.row[i]).as_deref() == Some("reading"));
                    if reading {
                        let before = at("status_before")
                            .and_then(|i| text(&p.row[i]))
                            .unwrap_or_else(|| "kept".into());
                        set("status", Sql::Text(before));
                        set("status_before", Sql::Null);
                    }
                    for c in ["lease_until", "lease_token"] {
                        set(c, Sql::Null);
                    }
                    for c in ["forget_epoch", "lease_epoch"] {
                        set(c, Sql::Integer(0));
                    }
                }
            }
            tx.execute(&insert, params_from_iter(row.iter()))?;
            carry_forgotten(tx, id, p, true)?;
            for (claim, marks) in &p.marks {
                carried::keep(tx, id, claim, marks)?;
            }
            if let Some(client) = &p.client {
                tx.execute(
                    "INSERT OR IGNORE INTO remote_keeps(item_id, device_id, client_id, kept_at) \
                     VALUES (?1, ?2, ?3, ?4)",
                    params![id, device, client, p.kept_at],
                )?;
            }
            // Counted as they stand now: an earlier version replaced by another arriving one
            // is not counted again.
            let replaced = arriving.iter().zip(&fates).any(|(q, g)| {
                matches!(g, Fate::Add(_))
                    && q.meta.get("replaces").and_then(Value::as_str) == Some(p.id.as_str())
            });
            if replaced || entries::is_entry(&p.kind) {
                continue;
            }
            match p.role() {
                _ if p.checkin() => brought.checkins += 1,
                Some("entry") | Some("good") => brought.entries += 1,
                Some(_) => {}
                None => brought.things += 1,
            }
        }
        entries::write(tx, device, &notes.records, &plan, &map)?;
        let placed = derived::Placed {
            items: arriving
                .iter()
                .zip(&fates)
                .map(|(p, f)| (p.id.clone(), f.here().to_string()))
                .collect(),
            added: arriving
                .iter()
                .zip(&fates)
                .filter(|(_, f)| matches!(f, Fate::Add(_)))
                .map(|(p, _)| p.id.clone())
                .collect(),
        };
        Ok((brought, placed, files))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(sha: &str) -> Arriving {
        Arriving {
            row: Vec::new(),
            id: "itm_a".into(),
            kind: "image".into(),
            sha: sha.into(),
            ext: Some("png".into()),
            file: true,
            readable: true,
            private: false,
            kept_at: String::new(),
            meta: json!({}),
            client: None,
            place: None,
            forgotten: Vec::new(),
            marks: Vec::new(),
        }
    }

    /// Something took an original away after the copies were made (a forget that finished):
    /// nothing is added without it, and the phone carries on.
    #[test]
    fn an_original_gone_since_the_copies_were_made_is_not_added_without_it() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("from"), dir.path().join("to"));
        std::fs::create_dir_all(&from).unwrap();
        std::fs::create_dir_all(&to).unwrap();
        let (a, b) = ("a".repeat(64), "b".repeat(64));
        std::fs::write(original_path(&from, &a, Some("png")), b"a").unwrap();
        let mut staged = Staged::copy(&from, &to, &[&photo(&a)]).unwrap();
        assert!(
            !original_path(&to, &a, Some("png")).exists(),
            "not in place yet"
        );
        staged.place(&to, &photo(&a)).unwrap();
        assert!(original_path(&to, &a, Some("png")).is_file());
        assert_eq!(staged.placed.len(), 1);
        assert!(matches!(staged.place(&to, &photo(&b)), Err(Error::Busy)));
        staged.drop_waiting();
        let left: Vec<_> = std::fs::read_dir(&to).unwrap().flatten().collect();
        assert_eq!(left.len(), 1, "no copy is left behind");
    }
}
