//! Stored reflections and recaps: one row per period, each citing the items it came from.

use std::collections::BTreeSet;

use nocturne_contract::{DayReflection, Period, PeriodKind, Recap, Receipt, WrittenWhen};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::Value;

use crate::error::Result;
use crate::util::{enum_str, parse_enum};

pub(crate) struct Row {
    pub id: String,
    pub written_at: String,
    pub written: WrittenWhen,
    pub so_far: bool,
    pub from_count: u32,
    pub lines_removed: u32,
    pub body: String,
}

pub(crate) fn load(conn: &Connection, period: &Period) -> Result<Option<Row>> {
    let row = conn
        .query_row(
            "SELECT id, written_at, written, so_far, from_count, lines_removed, body \
             FROM reflections WHERE kind = ?1 AND start = ?2",
            params![enum_str(&period.kind), period.start],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, u32>(4)?,
                    r.get::<_, u32>(5)?,
                    r.get::<_, String>(6)?,
                ))
            },
        )
        .optional()?;
    row.map(
        |(id, written_at, written, so_far, from_count, lines_removed, body)| {
            Ok(Row {
                id,
                written_at,
                written: parse_enum(&written)?,
                so_far,
                from_count,
                lines_removed,
                body,
            })
        },
    )
    .transpose()
}

pub(crate) fn exists(conn: &Connection, period: &Period) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM reflections WHERE kind = ?1 AND start = ?2)",
        params![enum_str(&period.kind), period.start],
        |r| r.get(0),
    )?)
}

/// A finished writing is stored for the period; a so-far one does not count.
pub(crate) fn is_final(conn: &Connection, period: &Period) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM reflections WHERE kind = ?1 AND start = ?2 AND so_far = 0)",
        params![enum_str(&period.kind), period.start],
        |r| r.get(0),
    )?)
}

/// The person deleted this period's writing: it is not written again until they ask.
pub(crate) fn decline(tx: &Transaction<'_>, period: &Period, at: &str) -> Result<()> {
    tx.execute(
        "INSERT OR REPLACE INTO reflections_declined(kind, start, declined_at) VALUES (?1, ?2, ?3)",
        params![enum_str(&period.kind), period.start, at],
    )?;
    Ok(())
}

/// The person asked for this period again.
pub(crate) fn allow(tx: &Transaction<'_>, period: &Period) -> Result<()> {
    tx.execute(
        "DELETE FROM reflections_declined WHERE kind = ?1 AND start = ?2",
        params![enum_str(&period.kind), period.start],
    )?;
    Ok(())
}

pub(crate) fn is_declined(conn: &Connection, period: &Period) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM reflections_declined WHERE kind = ?1 AND start = ?2)",
        params![enum_str(&period.kind), period.start],
        |r| r.get(0),
    )?)
}

/// Every item id a stored body names, wherever it appears.
pub(crate) fn item_ids_in(value: &Value) -> BTreeSet<String> {
    fn walk(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::String(s) if is_item_id(s) => {
                out.insert(s.clone());
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    walk(value, &mut out);
    out
}

fn is_item_id(s: &str) -> bool {
    s.len() == 20
        && s.strip_prefix("itm_")
            .is_some_and(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))
}

pub(crate) struct NewRow<'a> {
    pub period: &'a Period,
    pub id: &'a str,
    pub written_at: &'a str,
    pub written: WrittenWhen,
    pub so_far: bool,
    pub from_count: u32,
    pub lines_removed: u32,
    pub body: &'a Value,
}

/// Store one period's writing, replacing what was there: a finished one replaces a so-far one.
pub(crate) fn upsert(tx: &Transaction<'_>, n: &NewRow<'_>) -> Result<()> {
    delete(tx, n.period)?;
    tx.execute(
        "INSERT INTO reflections(id, kind, start, end, written_at, written, so_far, from_count, \
         lines_removed, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            n.id,
            enum_str(&n.period.kind),
            n.period.start,
            n.period.end,
            n.written_at,
            enum_str(&n.written),
            n.so_far,
            n.from_count,
            n.lines_removed,
            n.body.to_string(),
        ],
    )?;
    for item in item_ids_in(n.body) {
        tx.execute(
            "INSERT OR IGNORE INTO reflection_cites(reflection_id, item_id) VALUES (?1, ?2)",
            params![n.id, item],
        )?;
    }
    Ok(())
}

pub(crate) fn delete(tx: &Transaction<'_>, period: &Period) -> Result<()> {
    tx.execute(
        "DELETE FROM reflections WHERE kind = ?1 AND start = ?2",
        params![enum_str(&period.kind), period.start],
    )?;
    Ok(())
}

fn allowed(receipts: &[Receipt], keep: &dyn Fn(&str) -> bool) -> bool {
    receipts.iter().all(|r| keep(&r.item_id))
}

/// Drop the lines of a day's reflection that cite something `keep` refuses. Returns how many.
pub(crate) fn retain_day(r: &mut DayReflection, keep: &dyn Fn(&str) -> bool) -> u32 {
    let before = r.the_day.len();
    r.the_day.retain(|l| allowed(&l.receipts, keep));
    r.felt.retain(|c| keep(&c.id));
    r.good.retain(|g| keep(&g.id));
    for n in &mut r.people {
        n.item_ids.retain(|i| keep(i));
    }
    r.people.retain(|n| !n.item_ids.is_empty());
    r.still_open.retain(|m| allowed(&m.receipts, keep));
    (before - r.the_day.len()) as u32
}

/// Drop what a recap says that cites something `keep` refuses. Returns how many lines went:
/// highlights, "together" lines and lines of "What changed".
pub(crate) fn retain_recap(r: &mut Recap, keep: &dyn Fn(&str) -> bool) -> u32 {
    let before = r.highlights.len() + r.together.len() + r.changed.len();
    r.highlights
        .retain(|h| allowed(&h.receipts, keep) && h.photo.as_ref().is_none_or(|p| keep(&p.id)));
    r.together.retain(|t| allowed(&t.receipts, keep));
    for t in &mut r.together {
        t.with.item_ids.retain(|i| keep(i));
    }
    r.changed.retain(|l| allowed(&l.receipts, keep));
    r.checkins.retain(|c| keep(&c.id));
    for w in &mut r.words {
        w.checkin_ids.retain(|i| keep(i));
    }
    r.words.retain(|w| !w.checkin_ids.is_empty());
    r.good.retain(|g| keep(&g.id));
    for g in &mut r.good {
        if g.photo.as_ref().is_some_and(|p| !keep(&p.id)) {
            g.photo = None;
        }
    }
    for p in &mut r.people {
        p.with.item_ids.retain(|i| keep(i));
    }
    r.people.retain(|p| !p.with.item_ids.is_empty());
    for n in &mut r.new_names {
        n.item_ids.retain(|i| keep(i));
    }
    r.new_names.retain(|n| !n.item_ids.is_empty());
    r.tried.retain(|t| keep(&t.checkin_id));
    r.still_open.retain(|m| allowed(&m.receipts, keep));
    if let Some(c) = &mut r.compare {
        c.count = r.checkins.len() as u32;
    }
    (before - (r.highlights.len() + r.together.len() + r.changed.len())) as u32
}

/// The stored reflections and recaps that cite any of these items, as (id, kind, body).
fn citing(conn: &Connection, items: &[String]) -> Result<Vec<(String, PeriodKind, String, u32)>> {
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for item in items {
        let mut stmt =
            conn.prepare_cached("SELECT reflection_id FROM reflection_cites WHERE item_id = ?1")?;
        for r in stmt.query_map([item], |r| r.get::<_, String>(0))? {
            ids.insert(r?);
        }
    }
    let mut out = Vec::new();
    for id in ids {
        let (kind, body, removed): (String, String, u32) = conn.query_row(
            "SELECT kind, body, lines_removed FROM reflections WHERE id = ?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        out.push((id, parse_enum(&kind)?, body, removed));
    }
    Ok(out)
}

fn scrubbed(kind: PeriodKind, body: &str, gone: &BTreeSet<&str>) -> Result<(String, u32)> {
    let keep = |id: &str| !gone.contains(id);
    if kind == PeriodKind::Day {
        let mut r: DayReflection = serde_json::from_str(body)?;
        let n = retain_day(&mut r, &keep);
        r.lines_removed += n;
        Ok((serde_json::to_string(&r)?, n))
    } else {
        let mut r: Recap = serde_json::from_str(body)?;
        let n = retain_recap(&mut r, &keep);
        r.lines_removed += n;
        Ok((serde_json::to_string(&r)?, n))
    }
}

/// How many lines in stored reflections and recaps cite these items and would be taken out,
/// and how many of those are in week, month and year recaps.
pub(crate) fn count_lines(conn: &Connection, items: &[String]) -> Result<(u32, u32)> {
    let gone: BTreeSet<&str> = items.iter().map(String::as_str).collect();
    let (mut total, mut recaps) = (0, 0);
    for (_, kind, body, _) in citing(conn, items)? {
        let n = scrubbed(kind, &body, &gone)?.1;
        total += n;
        if kind != PeriodKind::Day {
            recaps += n;
        }
    }
    Ok((total, recaps))
}

/// The items are being deleted: take every line that cites them out of every stored reflection
/// and recap, count what went, and drop the citations. Returns the lines taken out.
pub(crate) fn scrub_items(tx: &Transaction<'_>, items: &[String]) -> Result<u32> {
    if items.is_empty() {
        return Ok(0);
    }
    crate::nightly::note_forgot(tx)?;
    let gone: BTreeSet<&str> = items.iter().map(String::as_str).collect();
    let mut total = 0;
    for (id, kind, body, removed) in citing(tx, items)? {
        let (new_body, n) = scrubbed(kind, &body, &gone)?;
        tx.execute(
            "UPDATE reflections SET body = ?2, lines_removed = ?3 WHERE id = ?1",
            params![id, new_body, removed + n],
        )?;
        total += n;
    }
    for item in items {
        tx.execute("DELETE FROM reflection_cites WHERE item_id = ?1", [item])?;
    }
    super::question::forget_sentences_about(tx, items)?;
    Ok(total)
}
