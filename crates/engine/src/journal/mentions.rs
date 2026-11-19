//! Mentions: which person, place or thing an item names, and where.

use std::collections::{HashMap, HashSet};

use nocturne_contract::{EntityKind, Mention, MentionState, Span};
use rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter};

use crate::error::Result;
use crate::types::Candidate;
use crate::util::{new_id, parse_enum};

/// A name an item says, and the person, place or thing it was filed under.
pub(crate) struct Found {
    pub entity_id: String,
    /// The name as the item says it.
    pub said_as: String,
    /// The words the mind quoted for the claim, when it did.
    pub quote: Option<String>,
    /// This reading made the thing.
    pub created: bool,
}

impl Found {
    pub(crate) fn of(c: &Candidate, entity_id: String, created: bool) -> Found {
        Found {
            entity_id,
            said_as: c.subject.trim().to_string(),
            quote: match &c.span {
                Some(Span::Quote { text }) => Some(text.clone()),
                _ => None,
            },
            created,
        }
    }
}

fn fold(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(one), None) => one,
        _ => c,
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Where `needle` first stands in `text`, ignoring case, as a start and end in UTF-16 code units
/// as JavaScript counts them. With `whole_word`, the match may not be part of a longer word.
pub(crate) fn find_span(text: &str, needle: &str, whole_word: bool) -> Option<(u32, u32)> {
    let hay: Vec<char> = text.chars().collect();
    let pat: Vec<char> = needle.trim().chars().map(fold).collect();
    if pat.is_empty() || pat.len() > hay.len() {
        return None;
    }
    let folded: Vec<char> = hay.iter().copied().map(fold).collect();
    let mut units = Vec::with_capacity(hay.len() + 1);
    let mut at = 0u32;
    units.push(0);
    for c in &hay {
        at += c.len_utf16() as u32;
        units.push(at);
    }
    (0..=folded.len() - pat.len()).find_map(|i| {
        let end = i + pat.len();
        if folded[i..end] != pat[..] {
            return None;
        }
        if whole_word
            && ((i > 0 && is_word(folded[i - 1])) || (end < folded.len() && is_word(folded[end])))
        {
            return None;
        }
        Some((units[i], units[end]))
    })
}

/// Where the name stands in the item: the name as said, else the thing's own name, else the
/// words quoted for the claim.
fn locate(text: &str, found: &Found, entity_name: &str) -> Option<(u32, u32)> {
    find_span(text, &found.said_as, true)
        .or_else(|| find_span(text, entity_name, true))
        .or_else(|| {
            found
                .quote
                .as_deref()
                .and_then(|q| find_span(text, q, false))
        })
}

/// Number the mentions of an item in the order they stand in it; those with no place come last.
fn renumber(tx: &Transaction<'_>, item_id: &str) -> Result<()> {
    let ids: Vec<String> = {
        let mut stmt = tx.prepare(
            "SELECT id FROM mentions WHERE item_id = ?1 \
             ORDER BY start IS NULL, start, ord, created_at, id",
        )?;
        stmt.query_map([item_id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    for (n, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE mentions SET ord = ?2 WHERE id = ?1",
            params![id, n as i64],
        )?;
    }
    Ok(())
}

/// Write what an item names. A thing the item already named keeps what the person did with it
/// (confirmed, removed) and is never written twice. With `prune`, names a reading no longer
/// finds, and the person has not touched, are dropped.
pub(crate) fn write_mentions(
    tx: &Transaction<'_>,
    item_id: &str,
    now: &str,
    found: &[Found],
    prune: bool,
) -> Result<()> {
    let text: Option<String> = tx
        .query_row("SELECT text FROM items WHERE id = ?1", [item_id], |r| {
            r.get(0)
        })
        .optional()?
        .flatten();
    let mut seen: HashSet<&str> = HashSet::new();
    for f in found {
        if !seen.insert(f.entity_id.as_str()) {
            continue;
        }
        let name: String = tx.query_row(
            "SELECT name FROM entities WHERE id = ?1",
            [&f.entity_id],
            |r| r.get(0),
        )?;
        let at = text.as_deref().and_then(|t| locate(t, f, &name));
        tx.execute(
            "INSERT INTO mentions(id, item_id, entity_id, name, start, end, ord, new, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8) \
             ON CONFLICT(item_id, entity_id) DO UPDATE SET name = excluded.name, \
             start = excluded.start, end = excluded.end",
            params![
                new_id("men"),
                item_id,
                f.entity_id,
                f.said_as,
                at.map(|a| a.0),
                at.map(|a| a.1),
                f.created,
                now
            ],
        )?;
    }
    if prune {
        let mut stmt = tx
            .prepare("SELECT id, entity_id FROM mentions WHERE item_id = ?1 AND state = 'found'")?;
        let rows: Vec<(String, String)> = stmt
            .query_map([item_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (id, entity) in rows {
            if !seen.contains(entity.as_str()) {
                tx.execute("DELETE FROM mentions WHERE id = ?1", [id])?;
            }
        }
    }
    renumber(tx, item_id)
}

/// The mentions of each of `item_ids`, in the order they appear in the item. A mention the person
/// removed is returned with its state, so the interface can offer Undo; callers that show only
/// live names filter on `state`.
pub(crate) fn mentions_for_items(
    conn: &Connection,
    item_ids: &[String],
) -> Result<HashMap<String, Vec<Mention>>> {
    let mut out: HashMap<String, Vec<Mention>> = HashMap::new();
    if item_ids.is_empty() {
        return Ok(out);
    }
    let marks = vec!["?"; item_ids.len()].join(", ");
    let sql = format!(
        "SELECT n.id, n.item_id, n.entity_id, e.name, e.kind, e.role, n.start, n.end, n.state, \
         n.new, (SELECT COUNT(*) FROM memory_sources s JOIN memories m ON m.id = s.memory_id \
                 WHERE s.item_id = n.item_id AND m.entity_id = n.entity_id \
                 AND m.superseded_at IS NULL AND m.forgetting = 0) \
         FROM mentions n JOIN entities e ON e.id = n.entity_id \
         WHERE n.item_id IN ({marks}) ORDER BY n.item_id, n.ord"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(item_ids.iter()), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<u32>>(6)?,
            r.get::<_, Option<u32>>(7)?,
            r.get::<_, String>(8)?,
            r.get::<_, bool>(9)?,
            r.get::<_, u32>(10)?,
        ))
    })?;
    for row in rows {
        let (id, item_id, entity_id, name, kind, role, start, end, state, new, memory_count) = row?;
        out.entry(item_id.clone()).or_default().push(Mention {
            id,
            item_id,
            entity_id,
            name,
            kind: parse_enum::<EntityKind>(&kind)?,
            role,
            start,
            end,
            state: parse_enum::<MentionState>(&state)?,
            new,
            memory_count,
        });
    }
    Ok(out)
}
