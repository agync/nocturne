//! Candidates become memories: reinforcement, trust and corroboration, contradictions, history,
//! corrections, and the entries that wait on the person.

use std::collections::HashSet;

use nocturne_contract::{
    EntityKind, EntityView, MemoryFilter, MemoryStatus, MemoryView, NeedsYouEntry, WhichOption,
};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::Engine;
use crate::audio::pages_citing;
use crate::capture::check_lease;
use crate::error::{Error, Result};
use crate::rows::{MEM_COLS, MemRow, load_mem, mem_row, mem_view, query_mems, receipts_for};
use crate::types::{ApplyReport, Candidate, CorrectPatch, Predicate};
use crate::util::{enum_str, fmt_ts, fts_and, key_of, new_id, overlaps};

/// A real-world change (moved house, new dose) closes the old row at the new row's start date.
fn closes_old_value(predicate: &str) -> bool {
    matches!(predicate, "lives_in" | "takes" | "prefers")
}

/// The first word of an object, casefolded: "Metformin 500mg" and "metformin 850mg" are one drug.
fn head(s: &str) -> String {
    key_of(s)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
}

/// Whether two memories with the same subject and predicate compete for one slot.
/// Single-valued: `lives_in`, `scheduled`, `due`, `takes` (per drug head), `prefers` (when tags
/// share a category). Everything else never conflicts.
fn same_slot(predicate: &str, new_object: &str, new_tags: &[String], e: &MemRow) -> bool {
    match predicate {
        "lives_in" | "scheduled" | "due" => true,
        "takes" => head(new_object) == head(&e.object),
        "prefers" => {
            let mine: HashSet<String> = new_tags.iter().map(|t| t.to_lowercase()).collect();
            e.tags.iter().any(|t| mine.contains(&t.to_lowercase()))
        }
        _ => false,
    }
}

/// Independent sources: different origin. Two items with no recorded origin carry no evidence of
/// a shared one, so they count as independent. (Different bytes are guaranteed by dedupe.)
fn independent(a: &Option<String>, b: &Option<String>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x != y,
        _ => true,
    }
}

struct ItemInfo {
    trusted: bool,
    origin: Option<String>,
}

fn upsert_entity(
    tx: &Transaction<'_>,
    subject: &str,
    subject_key: &str,
    kind: Option<EntityKind>,
) -> Result<Option<String>> {
    let find = |k: EntityKind| -> Result<Option<String>> {
        Ok(tx
            .query_row(
                "SELECT id FROM entities WHERE key = ?1",
                [format!("{}:{subject_key}", enum_str(&k))],
                |r| r.get(0),
            )
            .optional()?)
    };
    match kind {
        Some(k) => {
            if let Some(id) = find(k)? {
                return Ok(Some(id));
            }
            let id = new_id("ent");
            tx.execute(
                "INSERT INTO entities(id, kind, name, key) VALUES (?1, ?2, ?3, ?4)",
                params![
                    id,
                    enum_str(&k),
                    subject,
                    format!("{}:{subject_key}", enum_str(&k))
                ],
            )?;
            Ok(Some(id))
        }
        None => {
            for k in [EntityKind::Person, EntityKind::Place, EntityKind::Thing] {
                if let Some(id) = find(k)? {
                    return Ok(Some(id));
                }
            }
            Ok(None)
        }
    }
}

fn delete_memory(tx: &Transaction<'_>, id: &str) -> Result<()> {
    // Rows this one closed or superseded come back: the change that ended them is gone.
    tx.execute(
        "UPDATE memories SET valid_to = NULL, superseded_by = NULL \
         WHERE superseded_by = ?1 AND superseded_at IS NULL \
           AND valid_to = (SELECT valid_from FROM memories WHERE id = ?1)",
        [id],
    )?;
    tx.execute(
        "UPDATE memories SET superseded_at = NULL, superseded_by = NULL WHERE superseded_by = ?1",
        [id],
    )?;
    tx.execute("DELETE FROM conflicts WHERE a = ?1 OR b = ?1", [id])?;
    tx.execute("DELETE FROM questions WHERE memory_id = ?1", [id])?;
    tx.execute("DELETE FROM page_line_memories WHERE memory_id = ?1", [id])?;
    tx.execute("DELETE FROM memories WHERE id = ?1", [id])?;
    Ok(())
}

/// Conflicts for a memory that has just become active (corroborated or confirmed): it competes
/// with other active memories in the same slot whose validity overlaps. Returns new conflict ids.
fn create_conflicts(tx: &Transaction<'_>, id: &str, now: &str, today: &str) -> Result<Vec<String>> {
    let Some(row) = load_mem(tx, id)? else {
        return Ok(Vec::new());
    };
    if row.status != MemoryStatus::Active {
        return Ok(Vec::new());
    }
    let others = query_mems(
        tx,
        "m.subject_key = ?1 AND m.predicate = ?2 AND m.superseded_at IS NULL AND m.id != ?3 \
         AND m.status IN ('active', 'fading') ORDER BY m.recorded_at, m.id",
        params![row.subject_key, row.predicate, row.id],
    )?;
    let mut made = Vec::new();
    for e in &others {
        if e.object_key == row.object_key || !same_slot(&row.predicate, &row.object, &row.tags, e) {
            continue;
        }
        if !overlaps(
            e.valid_from.as_deref(),
            e.valid_to.as_deref(),
            Some(row.valid_from.as_deref().unwrap_or(today)),
            row.valid_to.as_deref(),
        ) {
            continue;
        }
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM conflicts WHERE resolved_at IS NULL \
             AND ((a = ?1 AND b = ?2) OR (a = ?2 AND b = ?1)))",
            params![e.id, row.id],
            |r| r.get(0),
        )?;
        if !exists {
            let cid = new_id("cfl");
            tx.execute(
                "INSERT INTO conflicts(id, a, b, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![cid, e.id, row.id, now],
            )?;
            made.push(cid);
        }
    }
    Ok(made)
}

/// Every version of one memory: what it replaced or ended and what replaced or ended it,
/// followed in both directions. Empty if the id does not exist.
pub(crate) fn chain(conn: &rusqlite::Connection, id: &str) -> Result<Vec<String>> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM memories WHERE id = ?1)",
        [id],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(Vec::new());
    }
    let mut seen: Vec<String> = vec![id.to_string()];
    let mut i = 0;
    while i < seen.len() {
        let cur = seen[i].clone();
        i += 1;
        let mut older = conn.prepare_cached("SELECT id FROM memories WHERE superseded_by = ?1")?;
        let mut neighbours: Vec<String> = older
            .query_map([&cur], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let newer: Option<String> = conn
            .query_row(
                "SELECT superseded_by FROM memories WHERE id = ?1",
                [&cur],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        neighbours.extend(newer);
        for n in neighbours {
            if !seen.contains(&n) {
                seen.push(n);
            }
        }
    }
    Ok(seen)
}

/// Whether a memory, or any version of it, is private. Unknown ids fail closed: they count as
/// private, so a line citing something that no longer exists never reaches audio or print.
pub(crate) fn is_private_chain(conn: &rusqlite::Connection, id: &str) -> Result<bool> {
    let ids = chain(conn, id)?;
    if ids.is_empty() {
        return Ok(true);
    }
    for v in &ids {
        let private: bool =
            conn.query_row("SELECT private FROM memories WHERE id = ?1", [v], |r| {
                r.get(0)
            })?;
        if private {
            return Ok(true);
        }
    }
    Ok(false)
}

fn span_json(c: &Candidate) -> Option<String> {
    c.span.as_ref().and_then(|s| serde_json::to_string(s).ok())
}

fn date_str(d: Option<jiff::civil::Date>) -> Option<String> {
    d.map(|d| d.to_string())
}

fn apply(
    tx: &Transaction<'_>,
    now: &str,
    today: &str,
    item_id: &str,
    lease: &str,
    cands: Vec<Candidate>,
) -> Result<ApplyReport> {
    check_lease(tx, item_id, lease)?;
    let (trust, origin): (String, Option<String>) = tx
        .query_row(
            "SELECT trust, origin FROM items WHERE id = ?1",
            [item_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| Error::not_found(format!("item {item_id}")))?;
    let item = ItemInfo {
        trusted: trust == "trusted",
        origin,
    };
    let mut report = ApplyReport::default();

    // Applying is idempotent per item: detach what this item said before, apply again, then
    // drop what nothing supports any more.
    let prev: Vec<(String, Option<String>)> = {
        let mut stmt =
            tx.prepare("SELECT memory_id, span FROM memory_sources WHERE item_id = ?1")?;
        let rows = stmt.query_map([item_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    tx.execute("DELETE FROM memory_sources WHERE item_id = ?1", [item_id])?;
    let relinked: HashSet<&str> = prev.iter().map(|p| p.0.as_str()).collect();
    let mut touched: HashSet<String> = HashSet::new();

    for c in &cands {
        apply_one(
            tx,
            now,
            today,
            item_id,
            &item,
            &relinked,
            &mut touched,
            &mut report,
            c,
        )?;
    }

    for (mid, span) in &prev {
        if touched.contains(mid) {
            continue;
        }
        let remaining: u32 = tx.query_row(
            "SELECT COUNT(*) FROM memory_sources WHERE memory_id = ?1",
            [mid],
            |r| r.get(0),
        )?;
        if remaining > 0 {
            continue;
        }
        let Some(row) = load_mem(tx, mid)? else {
            continue;
        };
        // Nothing now supports this memory. It is deleted only if nothing else depends on it:
        // a memory the person kept forever, used, confirmed or marked private, or that a saved
        // page cites, is never silently dropped by a re-read.
        let protected = row.keep_forever
            || row.last_used.is_some()
            || row.confirmed
            || row.private
            || !pages_citing(tx, std::slice::from_ref(mid))?.is_empty();
        if row.superseded_at.is_none() && !protected {
            delete_memory(tx, mid)?;
            report.removed.push(mid.clone());
        } else {
            // It keeps a tombstone link to this item (every memory has a receipt) and, unless
            // the person confirmed it, goes back to unconfirmed: nothing supports it now.
            tx.execute(
                "INSERT OR IGNORE INTO memory_sources(memory_id, item_id, span) VALUES (?1, ?2, ?3)",
                params![mid, item_id, span],
            )?;
            if row.superseded_at.is_none() && !row.confirmed {
                tx.execute(
                    "UPDATE memories SET status = 'unconfirmed', let_go_at = NULL WHERE id = ?1",
                    [mid],
                )?;
            }
        }
    }

    let status = if !item.trusted {
        "unconfirmed"
    } else if !report.conflicts.is_empty() {
        "needsYou"
    } else {
        "understood"
    };
    tx.execute(
        "UPDATE items SET status = ?2, lease_until = NULL, lease_token = NULL WHERE id = ?1",
        params![item_id, status],
    )?;
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn apply_one(
    tx: &Transaction<'_>,
    now: &str,
    today: &str,
    item_id: &str,
    item: &ItemInfo,
    relinked: &HashSet<&str>,
    touched: &mut HashSet<String>,
    report: &mut ApplyReport,
    c: &Candidate,
) -> Result<()> {
    if c.subject.trim().is_empty() || c.object.trim().is_empty() || c.sentence.trim().is_empty() {
        return Err(Error::invalid(
            "a candidate needs a subject, an object and a sentence",
        ));
    }
    let subject_key = key_of(&c.subject);
    let object_key = key_of(&c.object);
    let predicate = c.predicate.as_str().to_string();
    let confidence = f64::from(c.confidence.clamp(0.0, 1.0));
    let (vf, vt, due) = (
        date_str(c.valid_from),
        date_str(c.valid_to),
        date_str(c.due),
    );
    let span = span_json(c);

    // The person already corrected this claim away; do not bring it back.
    let known_wrong: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM memories w JOIN memories s ON s.id = w.superseded_by \
         WHERE w.subject_key = ?1 AND w.predicate = ?2 AND w.object_key = ?3 \
           AND w.superseded_at IS NOT NULL AND s.confirmed = 1 AND s.superseded_at IS NULL)",
        params![subject_key, predicate, object_key],
        |r| r.get(0),
    )?;
    if known_wrong {
        return Ok(());
    }

    let current = query_mems(
        tx,
        "m.subject_key = ?1 AND m.predicate = ?2 AND m.superseded_at IS NULL \
         ORDER BY m.recorded_at, m.id",
        params![subject_key, predicate],
    )?;
    let same_date = |e: &MemRow| {
        matches!(predicate.as_str(), "due" | "scheduled") && due.is_some() && e.due == due
    };
    // A date-only match (same due date, different words) is a convenience for trusted items
    // agreeing with settled memories. It never matches an unconfirmed memory, so it can never
    // promote one: promotion needs the same object from a trusted independent item.
    let matched = current.iter().find(|e| {
        e.object_key == object_key
            || (item.trusted && e.status != MemoryStatus::Unconfirmed && same_date(e))
    });

    if let Some(m) = matched {
        tx.execute(
            "INSERT OR IGNORE INTO memory_sources(memory_id, item_id, span) VALUES (?1, ?2, ?3)",
            params![m.id, item_id, span],
        )?;
        touched.insert(m.id.clone());
        if relinked.contains(m.id.as_str()) {
            // The same item read again: refresh, do not count it as a second voice. An untrusted
            // re-read never rewrites a memory that trusted sources support; it may only refine
            // an unconfirmed memory it alone supports. (`m.sources` was counted after this
            // item's link was removed, so 0 means this item is the only source.)
            let may_refresh =
                item.trusted || (m.status == MemoryStatus::Unconfirmed && m.sources == 0);
            if !m.confirmed && may_refresh {
                tx.execute(
                    "UPDATE memories SET sentence = ?2, confidence = ?3, valid_from = ?4, \
                     valid_to = ?5, due = ?6 WHERE id = ?1",
                    params![m.id, c.sentence.trim(), confidence, vf, vt, due],
                )?;
                if m.sentence != c.sentence.trim() {
                    // The old vector described the old words.
                    tx.execute("DELETE FROM embeddings WHERE memory_id = ?1", [&m.id])?;
                    report.reworded.push(m.id.clone());
                }
            }
            return Ok(());
        }
        if !item.trusted {
            return Ok(());
        }
        let others: Vec<Option<String>> = {
            let mut stmt = tx.prepare(
                "SELECT i.origin FROM memory_sources s JOIN items i ON i.id = s.item_id \
                 WHERE s.memory_id = ?1 AND s.item_id != ?2",
            )?;
            let rows = stmt.query_map(params![m.id, item_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let all_independent = others.iter().all(|o| independent(o, &item.origin));
        let promote =
            m.status == MemoryStatus::Unconfirmed && all_independent && !c.predicate.is_other();
        let status = match m.status {
            MemoryStatus::Fading | MemoryStatus::LetGo => MemoryStatus::Active,
            MemoryStatus::Unconfirmed if promote => MemoryStatus::Active,
            other => other,
        };
        tx.execute(
            "UPDATE memories SET last_used = ?2, confidence = MAX(confidence, ?3), status = ?4, \
             let_go_at = CASE WHEN ?4 = 'letGo' THEN let_go_at ELSE NULL END WHERE id = ?1",
            params![m.id, now, confidence, enum_str(&status)],
        )?;
        if promote {
            report.promoted.push(m.id.clone());
            // Becoming active makes it compete with what is already active.
            let made = create_conflicts(tx, &m.id, now, today)?;
            report.conflicts.extend(made);
        }
        report.reinforced.push(m.id.clone());
        return Ok(());
    }

    // A new memory.
    let status = if !item.trusted || c.predicate.is_other() {
        MemoryStatus::Unconfirmed
    } else {
        MemoryStatus::Active
    };
    let id = new_id("mem");
    let mut to_close: Vec<String> = Vec::new();
    let mut in_conflict: Vec<String> = Vec::new();
    if status == MemoryStatus::Active {
        for e in &current {
            if !matches!(e.status, MemoryStatus::Active | MemoryStatus::Fading)
                || e.object_key == object_key
                || !same_slot(&predicate, &c.object, &c.tags, e)
            {
                continue;
            }
            let later_start = match (&e.valid_from, &vf) {
                (None, Some(_)) => true,
                (Some(a), Some(b)) => a < b,
                _ => false,
            };
            let still_open = match (&e.valid_to, &vf) {
                (None, _) => true,
                (Some(t), Some(f)) => t > f,
                _ => false,
            };
            if closes_old_value(&predicate) && later_start && still_open {
                to_close.push(e.id.clone());
            } else if overlaps(
                e.valid_from.as_deref(),
                e.valid_to.as_deref(),
                // A claim with no start is about now, so it does not collide with what ended.
                Some(vf.as_deref().unwrap_or(today)),
                vt.as_deref(),
            ) {
                in_conflict.push(e.id.clone());
            }
        }
    }
    let safety = c.predicate == Predicate::Takes
        || c.tags.iter().any(|t| {
            matches!(
                t.to_lowercase().as_str(),
                "health" | "allergy" | "medication"
            )
        });
    let entity_id = upsert_entity(tx, &c.subject, &subject_key, c.subject_kind)?;
    tx.execute(
        "INSERT INTO memories(id, subject, subject_key, entity_id, predicate, object, object_key, \
         sentence, valid_from, valid_to, due, recorded_at, confidence, status, keep_forever, \
         suggest_private, tags) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, ?16)",
        params![
            id,
            c.subject.trim(),
            subject_key,
            entity_id,
            predicate,
            c.object.trim(),
            object_key,
            c.sentence.trim(),
            vf,
            vt,
            due,
            now,
            confidence,
            enum_str(&status),
            safety,
            serde_json::to_string(&c.tags)?,
        ],
    )?;
    tx.execute(
        "INSERT INTO memory_sources(memory_id, item_id, span) VALUES (?1, ?2, ?3)",
        params![id, item_id, span],
    )?;
    for old in &to_close {
        tx.execute(
            "UPDATE memories SET valid_to = ?2, superseded_by = ?3 WHERE id = ?1",
            params![old, vf, id],
        )?;
    }
    for old in &in_conflict {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM conflicts WHERE resolved_at IS NULL \
             AND ((a = ?1 AND b = ?2) OR (a = ?2 AND b = ?1)))",
            params![old, id],
            |r| r.get(0),
        )?;
        if !exists {
            let cid = new_id("cfl");
            tx.execute(
                "INSERT INTO conflicts(id, a, b, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![cid, old, id, now],
            )?;
            report.conflicts.push(cid);
        }
    }
    touched.insert(id.clone());
    if status == MemoryStatus::Unconfirmed {
        report.unconfirmed.push(id.clone());
    }
    report.created.push(id);
    Ok(())
}

/// The entries waiting on the person, as of `today` (local).
pub(crate) fn needs_you_on(
    conn: &rusqlite::Connection,
    today: jiff::civil::Date,
) -> Result<Vec<NeedsYouEntry>> {
    let horizon = today
        .checked_add(jiff::Span::new().days(14))
        .map_err(Error::from)?
        .to_string();
    let mut out: Vec<NeedsYouEntry> = Vec::new();

    // Due items, soonest first; an undated one waits at the end like an undated promise.
    let dated = query_mems(
        conn,
        "m.predicate = 'due' AND m.superseded_at IS NULL AND m.done = 0 \
         AND m.status IN ('active', 'fading') \
         AND (COALESCE(m.due, m.valid_from) IS NULL OR COALESCE(m.due, m.valid_from) <= ?1) \
         ORDER BY COALESCE(m.due, m.valid_from) IS NULL, COALESCE(m.due, m.valid_from), \
         m.recorded_at, m.id",
        [&horizon],
    )?;
    for m in dated {
        let due = m
            .due
            .clone()
            .or_else(|| m.valid_from.clone())
            .unwrap_or_default();
        out.push(NeedsYouEntry::Due {
            memory: mem_view(conn, &m)?,
            due,
            done: false,
        });
    }

    let promises = query_mems(
        conn,
        "m.predicate = 'promised' AND m.superseded_at IS NULL AND m.done = 0 \
         AND m.status IN ('active', 'fading') \
         AND (COALESCE(m.due, m.valid_from) IS NULL OR COALESCE(m.due, m.valid_from) <= ?1) \
         ORDER BY COALESCE(m.due, m.valid_from) IS NULL, COALESCE(m.due, m.valid_from), \
         m.recorded_at, m.id",
        [&horizon],
    )?;
    for m in promises {
        out.push(NeedsYouEntry::Promise {
            memory: mem_view(conn, &m)?,
            done: false,
        });
    }

    let mut stmt = conn.prepare(
        "SELECT id, a, b FROM conflicts WHERE resolved_at IS NULL ORDER BY created_at, id",
    )?;
    let conflicts: Vec<(String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (cid, a, b) in conflicts {
        let (Some(ma), Some(mb)) = (load_mem(conn, &a)?, load_mem(conn, &b)?) else {
            continue;
        };
        if ma.superseded_at.is_some() || mb.superseded_at.is_some() {
            continue;
        }
        let mut options = Vec::new();
        for m in [&ma, &mb] {
            let Some(receipt) = receipts_for(conn, &m.id)?.into_iter().next() else {
                continue;
            };
            options.push(WhichOption {
                memory_id: m.id.clone(),
                text: m.sentence.clone(),
                receipt,
            });
        }
        if options.len() == 2 {
            out.push(NeedsYouEntry::Which {
                conflict_id: cid,
                title: ma.subject.clone(),
                options,
            });
        }
    }

    let unconfirmed = query_mems(
        conn,
        "m.status = 'unconfirmed' AND m.superseded_at IS NULL \
         ORDER BY m.recorded_at DESC, m.id",
        [],
    )?;
    for m in unconfirmed {
        out.push(NeedsYouEntry::Unconfirmed {
            agreeing_sources: m.sources,
            used: m.last_used.is_some(),
            memory: mem_view(conn, &m)?,
        });
    }
    Ok(out)
}

pub(crate) fn confirm_tx(
    tx: &Transaction<'_>,
    id: &str,
    is_true: bool,
    now: &str,
    today: &str,
) -> Result<()> {
    let m = load_mem(tx, id)?.ok_or_else(|| Error::not_found(format!("memory {id}")))?;
    if is_true {
        tx.execute(
            "UPDATE memories SET confirmed = 1, status = 'active', last_used = ?2, \
             let_go_at = NULL WHERE id = ?1",
            params![m.id, now],
        )?;
        if m.status == MemoryStatus::Unconfirmed {
            // Becoming active, it competes with what is already active.
            create_conflicts(tx, &m.id, now, today)?;
        }
    } else {
        tx.execute(
            "UPDATE memories SET confirmed = 0, status = 'unconfirmed', let_go_at = NULL \
             WHERE id = ?1",
            [&m.id],
        )?;
    }
    Ok(())
}

pub(crate) fn mem_views(conn: &rusqlite::Connection, rows: &[MemRow]) -> Result<Vec<MemoryView>> {
    rows.iter().map(|r| mem_view(conn, r)).collect()
}

impl Engine {
    /// Turn what the mind found in an item into memories. The caller must hold the item's lease.
    /// Idempotent per item: it replaces what the same item said the last time it was applied.
    /// What a re-read no longer supports is deleted only if nothing depends on it (see
    /// `apply`); otherwise it goes back to unconfirmed with a link to the item.
    pub fn apply_candidates(
        &self,
        item: &str,
        lease: &str,
        candidates: Vec<Candidate>,
    ) -> Result<ApplyReport> {
        let now = fmt_ts(self.clock().now());
        let today = self
            .clock()
            .now()
            .to_zoned(self.clock().tz())
            .date()
            .to_string();
        let report = self.write(|tx| apply(tx, &now, &today, item, lease, candidates))?;
        if !report.removed.is_empty() || !report.reworded.is_empty() {
            self.invalidate_vectors();
        }
        self.bump_audio();
        Ok(report)
    }

    /// Current memories, newest first. Browsing does not count as use.
    pub fn memories(&self, filter: MemoryFilter) -> Result<Vec<MemoryView>> {
        self.read(|conn| {
            let mut sql =
                format!("SELECT {MEM_COLS} FROM memories m WHERE m.superseded_at IS NULL");
            let mut args: Vec<String> = Vec::new();
            if let Some(e) = &filter.entity_id {
                args.push(e.clone());
                sql.push_str(&format!(" AND m.entity_id = ?{}", args.len()));
            }
            if let Some(i) = &filter.item_id {
                args.push(i.clone());
                sql.push_str(&format!(
                    " AND m.id IN (SELECT memory_id FROM memory_sources WHERE item_id = ?{})",
                    args.len()
                ));
            }
            if let Some(s) = &filter.status {
                args.push(enum_str(s));
                sql.push_str(&format!(" AND m.status = ?{}", args.len()));
            }
            if let Some(q) = filter.query.as_deref().and_then(fts_and) {
                args.push(q);
                sql.push_str(&format!(
                    " AND m.rowid IN (SELECT rowid FROM memories_fts WHERE memories_fts MATCH ?{})",
                    args.len()
                ));
            }
            sql.push_str(" ORDER BY m.recorded_at DESC, m.id");
            if let Some(limit) = filter.limit {
                sql.push_str(&format!(" LIMIT {limit}"));
            }
            let mut stmt = conn.prepare(&sql)?;
            let rows: Vec<MemRow> = stmt
                .query_map(rusqlite::params_from_iter(args.iter()), mem_row)?
                .collect::<rusqlite::Result<_>>()?;
            mem_views(conn, &rows)
        })
    }

    pub fn get_memory(&self, id: &str) -> Result<MemoryView> {
        self.read(|conn| {
            let row =
                load_mem(conn, id)?.ok_or_else(|| Error::not_found(format!("memory {id}")))?;
            mem_view(conn, &row)
        })
    }

    /// Store vectors for memories, in chunks so capture never waits long. Unknown ids are ignored.
    pub fn store_embeddings(&self, rows: Vec<(String, Vec<f32>)>) -> Result<()> {
        for chunk in rows.chunks(200) {
            self.write(|tx| {
                for (id, vec) in chunk {
                    let blob: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
                    tx.execute(
                        "INSERT OR REPLACE INTO embeddings(memory_id, dim, vec) \
                         SELECT id, ?2, ?3 FROM memories WHERE id = ?1",
                        params![id, vec.len() as i64, blob],
                    )?;
                }
                Ok(())
            })?;
        }
        self.invalidate_vectors();
        Ok(())
    }

    /// Memories that need a vector, newest first.
    pub fn memories_without_embedding(&self, limit: usize) -> Result<Vec<(String, String)>> {
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT m.id, m.sentence FROM memories m \
                 WHERE m.superseded_at IS NULL \
                   AND NOT EXISTS (SELECT 1 FROM embeddings e WHERE e.memory_id = m.id) \
                 ORDER BY m.recorded_at DESC, m.id LIMIT ?1",
            )?;
            let rows = stmt.query_map([limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    /// Everything waiting on the person, ranked: due soonest, then promises, then "which is
    /// right?", then unconfirmed. Entries are not-done by construction.
    pub fn needs_you(&self) -> Result<Vec<NeedsYouEntry>> {
        let tz = self.clock().tz();
        let today = self.clock().now().to_zoned(tz).date();
        self.read(|conn| needs_you_on(conn, today))
    }

    /// The person says it is true (or not). True confirms and counts as use; false makes it
    /// unconfirmed again and asks again.
    pub fn confirm(&self, id: &str, is_true: bool) -> Result<()> {
        let now = fmt_ts(self.clock().now());
        let today = self.today();
        self.write(|tx| confirm_tx(tx, id, is_true, &now, &today))?;
        self.bump_audio();
        Ok(())
    }

    /// Choose which of two conflicting memories is right. The other stays as history.
    pub fn resolve_which(&self, conflict: &str, keep: &str) -> Result<()> {
        let now = fmt_ts(self.clock().now());
        self.write(|tx| {
            let row: Option<(String, String, Option<String>)> = tx
                .query_row(
                    "SELECT a, b, resolved_at FROM conflicts WHERE id = ?1",
                    [conflict],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let (a, b, resolved) =
                row.ok_or_else(|| Error::not_found(format!("conflict {conflict}")))?;
            if resolved.is_some() {
                return Err(Error::invalid("that choice has already been made"));
            }
            let other = if keep == a {
                b
            } else if keep == b {
                a
            } else {
                return Err(Error::invalid("the memory is not part of that choice"));
            };
            tx.execute(
                "UPDATE memories SET superseded_at = ?2, superseded_by = ?3 \
                 WHERE id = ?1",
                params![other, now, keep],
            )?;
            tx.execute(
                "UPDATE memories SET confirmed = 1, status = 'active', last_used = ?2, \
                 let_go_at = NULL WHERE id = ?1",
                params![keep, now],
            )?;
            tx.execute(
                "UPDATE conflicts SET resolved_at = ?2, kept = ?3 \
                 WHERE resolved_at IS NULL AND (a = ?1 OR b = ?1)",
                params![other, now, keep],
            )?;
            Ok(())
        })?;
        self.invalidate_vectors();
        self.bump_audio();
        Ok(())
    }

    /// The person corrects a memory. The old row closes into history (its dates untouched: it
    /// was wrong, not ended) and the new row cites the same sources. `new_object` replaces the
    /// old object inside the sentence when it appears there, otherwise it becomes the sentence.
    pub fn correct(&self, id: &str, patch: CorrectPatch) -> Result<String> {
        let new_object = patch.object.trim().to_string();
        if new_object.is_empty() {
            return Err(Error::invalid("a correction needs some text"));
        }
        let now = fmt_ts(self.clock().now());
        let created = self.write(|tx| {
            let old = load_mem(tx, id)?.ok_or_else(|| Error::not_found(format!("memory {id}")))?;
            if old.superseded_at.is_some() {
                return Err(Error::invalid("only a current memory can be corrected"));
            }
            let sentence = match patch.sentence.as_deref().map(str::trim) {
                Some(s) if !s.is_empty() => s.to_string(),
                _ if !old.object.is_empty() && old.sentence.contains(&old.object) => {
                    old.sentence.replacen(&old.object, &new_object, 1)
                }
                _ => new_object.clone(),
            };
            let new_id = new_memory_id();
            tx.execute(
                "INSERT INTO memories(id, subject, subject_key, entity_id, predicate, object, \
                 object_key, sentence, valid_from, valid_to, due, recorded_at, confidence, status, \
                 confirmed, last_used, done, keep_forever, private, suggest_private, tags) \
                 SELECT ?2, subject, subject_key, entity_id, predicate, ?3, ?4, ?5, valid_from, \
                 valid_to, due, ?6, 1.0, 'active', 1, ?6, done, keep_forever, private, \
                 suggest_private, tags FROM memories WHERE id = ?1",
                params![id, new_id, new_object, key_of(&new_object), sentence, now],
            )?;
            for (column, value) in [
                ("due", patch.due),
                ("valid_from", patch.valid_from),
                ("valid_to", patch.valid_to),
            ] {
                if let Some(v) = value {
                    tx.execute(
                        &format!("UPDATE memories SET {column} = ?2 WHERE id = ?1"),
                        params![new_id, date_str(v)],
                    )?;
                }
            }
            tx.execute(
                "INSERT INTO memory_sources(memory_id, item_id, span) \
                 SELECT ?2, item_id, span FROM memory_sources WHERE memory_id = ?1",
                params![id, new_id],
            )?;
            tx.execute(
                "UPDATE memories SET superseded_at = ?2, superseded_by = ?3 WHERE id = ?1",
                params![id, now, new_id],
            )?;
            tx.execute(
                "UPDATE conflicts SET a = ?2 WHERE a = ?1 AND resolved_at IS NULL",
                params![id, new_id],
            )?;
            tx.execute(
                "UPDATE conflicts SET b = ?2 WHERE b = ?1 AND resolved_at IS NULL",
                params![id, new_id],
            )?;
            Ok(new_id)
        })?;
        self.invalidate_vectors();
        self.bump_audio();
        Ok(created)
    }

    /// Every version of a memory, newest first: what it is now, and what it replaced or ended.
    pub fn history(&self, memory_id: &str) -> Result<Vec<MemoryView>> {
        self.read(|conn| {
            let ids = chain(conn, memory_id)?;
            if ids.is_empty() {
                return Err(Error::not_found(format!("memory {memory_id}")));
            }
            let mut rows = Vec::new();
            for id in &ids {
                if let Some(r) = load_mem(conn, id)? {
                    rows.push(r);
                }
            }
            rows.sort_by(|a, b| {
                b.recorded_at
                    .cmp(&a.recorded_at)
                    .then_with(|| a.id.cmp(&b.id))
            });
            mem_views(conn, &rows)
        })
    }

    /// The same entries as `needs_you`, without anything private, for places that must never
    /// show private content: notifications, the tray, a locked window.
    pub fn needs_you_public(&self) -> Result<Vec<NeedsYouEntry>> {
        let tz = self.clock().tz();
        let today = self.clock().now().to_zoned(tz).date();
        self.read(|conn| {
            let mut out = Vec::new();
            for entry in needs_you_on(conn, today)? {
                let private = match &entry {
                    NeedsYouEntry::Due { memory, .. }
                    | NeedsYouEntry::Promise { memory, .. }
                    | NeedsYouEntry::Unconfirmed { memory, .. } => {
                        is_private_chain(conn, &memory.id)?
                    }
                    NeedsYouEntry::Which { options, .. } => {
                        let mut any = false;
                        for o in options {
                            any |= is_private_chain(conn, &o.memory_id)?;
                        }
                        any
                    }
                };
                if !private {
                    out.push(entry);
                }
            }
            Ok(out)
        })
    }

    /// Tick off a due or promised memory. Counts as use.
    pub fn mark_done(&self, memory_id: &str) -> Result<()> {
        let now = fmt_ts(self.clock().now());
        self.write(|tx| {
            let n = tx.execute(
                "UPDATE memories SET done = 1, last_used = ?2, \
                 status = CASE WHEN status IN ('fading', 'letGo') THEN 'active' ELSE status END, \
                 let_go_at = CASE WHEN status = 'letGo' THEN NULL ELSE let_go_at END \
                 WHERE id = ?1",
                params![memory_id, now],
            )?;
            if n == 0 {
                return Err(Error::not_found(format!("memory {memory_id}")));
            }
            tx.execute(
                "UPDATE page_lines SET done = 1 WHERE id IN \
                 (SELECT line_id FROM page_line_memories WHERE memory_id = ?1)",
                [memory_id],
            )?;
            Ok(())
        })
    }

    /// People, places and things the person has kept things about.
    pub fn entities(&self, kind: Option<EntityKind>) -> Result<Vec<EntityView>> {
        self.entities_matching(kind, None)
    }

    /// Entities whose name or role contains `query` (case-insensitive; blank means all),
    /// optionally of one kind. Most memories first, then by name.
    pub fn entities_matching(
        &self,
        kind: Option<EntityKind>,
        query: Option<&str>,
    ) -> Result<Vec<EntityView>> {
        let needle = query
            .map(|q| q.trim().to_lowercase())
            .filter(|q| !q.is_empty());
        let mut out: Vec<EntityView> = self.read(|conn| {
            let mut sql = String::from(
                "SELECT e.id, e.kind, e.name, e.role, \
                 (SELECT COUNT(*) FROM memories m WHERE m.entity_id = e.id AND m.superseded_at IS NULL \
                  AND m.forgetting = 0), \
                 (SELECT COUNT(DISTINCT s.item_id) FROM memory_sources s JOIN memories m ON m.id = s.memory_id \
                  WHERE m.entity_id = e.id AND m.superseded_at IS NULL AND m.forgetting = 0) \
                 FROM entities e",
            );
            let mut args = Vec::new();
            if let Some(k) = kind {
                args.push(enum_str(&k));
                sql.push_str(" WHERE e.kind = ?1");
            }
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| {
                Ok(EntityView {
                    id: r.get(0)?,
                    kind: crate::rows::de(r.get(1)?)?,
                    name: r.get(2)?,
                    role: r.get(3)?,
                    memory_count: r.get(4)?,
                    item_count: r.get(5)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })?;
        if let Some(n) = &needle {
            out.retain(|e| {
                e.name.to_lowercase().contains(n.as_str())
                    || e.role
                        .as_ref()
                        .is_some_and(|r| r.to_lowercase().contains(n.as_str()))
            });
        }
        out.sort_by(|a, b| {
            b.memory_count
                .cmp(&a.memory_count)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(out)
    }

    pub fn entity(&self, id: &str) -> Result<(EntityView, Vec<MemoryView>)> {
        let view = self
            .entities(None)?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| Error::not_found(format!("entity {id}")))?;
        let memories = self.memories(MemoryFilter {
            entity_id: Some(id.to_string()),
            ..Default::default()
        })?;
        Ok((view, memories))
    }
}

fn new_memory_id() -> String {
    new_id("mem")
}
