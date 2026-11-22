//! Mentions: which person, place or thing an item names, and where.

use std::collections::{HashMap, HashSet};

use nocturne_contract::{
    EntityKind, Mention, MentionState, Receipt, RefileTarget, SameNameAnswer, SameNameAnswered,
    SameNameOption, SameNameQuestion, Span,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter};

use serde::{Deserialize, Serialize};

use crate::Engine;
use crate::entity::{clean_name, find_by_key, resolve_alias};
use crate::error::{Error, Result};
use crate::types::{Candidate, Predicate};
use crate::util::{fmt_ts, key_of, new_id, parse_enum};

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
    pub(crate) fn of(c: &Routed, entity_id: String, created: bool) -> Found {
        Found {
            entity_id,
            said_as: c.said_as.clone(),
            quote: match &c.cand.span {
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

/// The item and the versions it replaced, newest first.
fn versions_of(tx: &Transaction<'_>, item_id: &str) -> Result<Vec<String>> {
    let mut out = vec![item_id.to_string()];
    out.extend(super::keep::earlier_versions(tx, item_id)?);
    Ok(out)
}

/// What the person did with a name on the closest earlier version that has them do something.
fn earlier_state(tx: &Transaction<'_>, item_id: &str, entity_id: &str) -> Result<Option<String>> {
    for earlier in super::keep::earlier_versions(tx, item_id)? {
        let state: Option<String> = tx
            .query_row(
                "SELECT state FROM mentions WHERE item_id = ?1 AND entity_id = ?2 \
                 AND state != 'found'",
                params![earlier, entity_id],
                |r| r.get(0),
            )
            .optional()?;
        if state.is_some() {
            return Ok(state);
        }
    }
    Ok(None)
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
        let existed: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM mentions WHERE item_id = ?1 AND entity_id = ?2)",
            params![item_id, f.entity_id],
            |r| r.get(0),
        )?;
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
        if !existed && let Some(state) = earlier_state(tx, item_id, &f.entity_id)? {
            tx.execute(
                "UPDATE mentions SET state = ?3 WHERE item_id = ?1 AND entity_id = ?2",
                params![item_id, f.entity_id, state],
            )?;
        }
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

/// A claim on its way to being filed, with the name as the item says it (which a held claim,
/// once answered, no longer carries as its subject).
pub(crate) struct Routed {
    pub cand: Candidate,
    pub said_as: String,
}

/// A claim held back until the person says which of two people it is about.
#[derive(Serialize, Deserialize)]
struct Held {
    subject: String,
    kind: Option<EntityKind>,
    predicate: String,
    object: String,
    sentence: String,
    valid_from: Option<String>,
    valid_to: Option<String>,
    due: Option<String>,
    confidence: f32,
    span: Option<Span>,
    tags: Vec<String>,
    moved: bool,
}

impl Held {
    fn of(c: &Candidate) -> Held {
        let date = |d: Option<jiff::civil::Date>| d.map(|d| d.to_string());
        Held {
            subject: c.subject.clone(),
            kind: c.subject_kind,
            predicate: c.predicate.as_str().to_string(),
            object: c.object.clone(),
            sentence: c.sentence.clone(),
            valid_from: date(c.valid_from),
            valid_to: date(c.valid_to),
            due: date(c.due),
            confidence: c.confidence,
            span: c.span.clone(),
            tags: c.tags.clone(),
            moved: c.moved,
        }
    }

    fn into_candidate(self) -> Candidate {
        let date = |d: Option<String>| d.and_then(|d| d.parse().ok());
        Candidate {
            subject: self.subject,
            subject_kind: self.kind,
            predicate: Predicate::parse(&self.predicate),
            object: self.object,
            sentence: self.sentence,
            valid_from: date(self.valid_from),
            valid_to: date(self.valid_to),
            due: date(self.due),
            confidence: self.confidence,
            span: self.span,
            tags: self.tags,
            moved: self.moved,
        }
    }
}

fn first_word(name: &str) -> String {
    key_of(name)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string()
}

/// The people whose name starts with this word, when the word is no name of anyone. Only people
/// Nocturne knows something about count.
fn people_called(tx: &Transaction<'_>, word: &str) -> Result<Vec<SameNameOption>> {
    let mut stmt = tx.prepare(
        "SELECT e.id, e.name, e.role FROM entities e WHERE e.kind = 'person' \
         AND e.merged_into IS NULL AND (EXISTS (SELECT 1 FROM memories m WHERE m.entity_id = e.id \
         AND m.forgetting = 0) OR EXISTS (SELECT 1 FROM mentions n WHERE n.entity_id = e.id)) \
         ORDER BY e.name, e.id",
    )?;
    let rows: Vec<(String, String, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows
        .into_iter()
        .filter(|(_, name, _)| first_word(name) == word)
        .map(|(entity_id, name, role)| SameNameOption {
            entity_id,
            name,
            role,
        })
        .collect())
}

/// The sentence of the item that holds the name, as written.
fn quote_of(text: Option<&str>, name: &str, fallback: &str) -> String {
    text.and_then(|t| {
        t.split_inclusive(['.', '!', '?', '\n'])
            .map(str::trim)
            .find(|s| find_span(s, name, true).is_some())
    })
    .unwrap_or(fallback)
    .trim()
    .to_string()
}

/// Sort an item's claims before they are filed. A claim about a bare first name that fits two or
/// more people Nocturne knows is held, with a question for the person; one already answered is
/// filed the way it was answered. Everything else goes on as it was.
pub(crate) fn route(
    tx: &Transaction<'_>,
    now: &str,
    item_id: &str,
    cands: Vec<Candidate>,
) -> Result<Vec<Routed>> {
    let text: Option<String> = tx
        .query_row("SELECT text FROM items WHERE id = ?1", [item_id], |r| {
            r.get(0)
        })
        .optional()?
        .flatten();
    let versions = versions_of(tx, item_id)?;
    let answered: Vec<(String, SameNameAnswer)> = {
        let mut stmt = tx.prepare(
            "SELECT name, answer FROM name_questions WHERE item_id = ?1 AND answered_at IS NOT NULL \
             ORDER BY answered_at DESC, id DESC",
        )?;
        let mut all = Vec::new();
        for version in &versions {
            let rows: Vec<(String, Option<String>)> = stmt
                .query_map([version], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            all.extend(
                rows.into_iter()
                    .filter_map(|(name, a)| Some((key_of(&name), serde_json::from_str(&a?).ok()?))),
            );
        }
        all
    };
    for earlier in &versions[1..] {
        tx.execute(
            "DELETE FROM name_questions WHERE item_id = ?1 AND answered_at IS NULL",
            [earlier],
        )?;
    }
    let mut out: Vec<Routed> = Vec::new();
    let mut held: Vec<(String, String, Vec<SameNameOption>, Vec<Held>)> = Vec::new();
    for c in cands {
        let said_as = c.subject.trim().to_string();
        let key = key_of(&c.subject);
        let person = matches!(c.subject_kind, None | Some(EntityKind::Person));
        if !person || key.is_empty() || key.split_whitespace().count() != 1 {
            out.push(Routed { cand: c, said_as });
            continue;
        }
        if let Some((_, answer)) = answered.iter().find(|(k, _)| *k == key) {
            match answer {
                SameNameAnswer::Leave => {}
                SameNameAnswer::New => {
                    let mut c = c;
                    c.subject_kind = Some(EntityKind::Person);
                    out.push(Routed { cand: c, said_as });
                }
                SameNameAnswer::Entity { entity_id } => {
                    let named: Option<(String, String)> = tx
                        .query_row(
                            "SELECT name, kind FROM entities WHERE id = ?1 AND merged_into IS NULL",
                            [entity_id],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()?;
                    let Some((name, kind)) = named else {
                        continue;
                    };
                    let mut c = c;
                    c.subject = name;
                    c.subject_kind = Some(parse_enum::<EntityKind>(&kind)?);
                    out.push(Routed { cand: c, said_as });
                }
            }
            continue;
        }
        if find_by_key(tx, &key, None)?.is_some() || resolve_alias(tx, &key)?.is_some() {
            out.push(Routed { cand: c, said_as });
            continue;
        }
        let options = people_called(tx, &key)?;
        if options.len() < 2 {
            out.push(Routed { cand: c, said_as });
            continue;
        }
        match held.iter_mut().find(|h| h.0 == key) {
            Some(h) => h.3.push(Held::of(&c)),
            None => held.push((key, said_as, options, vec![Held::of(&c)])),
        }
    }
    let mut keep_open: Vec<String> = Vec::new();
    for (key, name, options, claims) in held {
        let quote = quote_of(
            text.as_deref(),
            &name,
            claims.first().map_or("", |h| h.sentence.as_str()),
        );
        let open: Option<String> = {
            let mut stmt = tx.prepare(
                "SELECT id, name FROM name_questions WHERE item_id = ?1 AND answered_at IS NULL",
            )?;
            let rows: Vec<(String, String)> = stmt
                .query_map([item_id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            rows.into_iter()
                .find(|(_, n)| key_of(n) == key)
                .map(|r| r.0)
        };
        let id = match open {
            Some(id) => {
                tx.execute(
                    "UPDATE name_questions SET quote = ?2, options = ?3, candidates = ?4 \
                     WHERE id = ?1",
                    params![
                        id,
                        quote,
                        serde_json::to_string(&options)?,
                        serde_json::to_string(&claims)?
                    ],
                )?;
                id
            }
            None => {
                let id = new_id("snq");
                tx.execute(
                    "INSERT INTO name_questions(id, item_id, name, quote, options, candidates, \
                     created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        id,
                        item_id,
                        name,
                        quote,
                        serde_json::to_string(&options)?,
                        serde_json::to_string(&claims)?,
                        now
                    ],
                )?;
                id
            }
        };
        keep_open.push(id);
    }
    let stale: Vec<String> = {
        let mut stmt =
            tx.prepare("SELECT id FROM name_questions WHERE item_id = ?1 AND answered_at IS NULL")?;
        stmt.query_map([item_id], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?
            .into_iter()
            .filter(|id| !keep_open.contains(id))
            .collect()
    };
    for id in stale {
        tx.execute("DELETE FROM name_questions WHERE id = ?1", [id])?;
    }
    Ok(out)
}

/// The open "Which {name} is this?" questions, oldest first; only those of `item_ids` when given.
pub(crate) fn open_questions(
    conn: &Connection,
    item_ids: Option<&[String]>,
) -> Result<Vec<SameNameQuestion>> {
    let mut stmt = conn.prepare(
        "SELECT q.id, q.item_id, q.name, q.quote, q.options, \
                i.kind, i.source, i.title, i.kept_at, i.duration_ms, i.trust, \
                COALESCE((SELECT rd.name FROM remote_keeps rk JOIN remote_devices rd \
                 ON rd.id = rk.device_id AND rd.connection != 'local' WHERE rk.item_id = i.id), json_extract(i.metadata, '$.keptOn')) \
         FROM name_questions q JOIN items i ON i.id = q.item_id \
         WHERE q.answered_at IS NULL ORDER BY q.created_at, q.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, String>(8)?,
            r.get::<_, Option<u32>>(9)?,
            r.get::<_, String>(10)?,
            r.get::<_, Option<String>>(11)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, item_id, name, quote, options, kind, source, title, kept_at, duration, trust, dev) =
            row?;
        if item_ids.is_some_and(|ids| !ids.contains(&item_id)) {
            continue;
        }
        let stored: Vec<SameNameOption> = serde_json::from_str(&options)?;
        let mut live = Vec::new();
        for o in stored {
            let now: Option<(String, Option<String>)> = conn
                .query_row(
                    "SELECT name, role FROM entities WHERE id = ?1 AND merged_into IS NULL",
                    [&o.entity_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if let Some((name, role)) = now {
                live.push(SameNameOption {
                    entity_id: o.entity_id,
                    name,
                    role,
                });
            }
        }
        if live.is_empty() {
            continue;
        }
        out.push(SameNameQuestion {
            id,
            name,
            quote: quote.clone(),
            receipt: Receipt {
                item_id,
                kind: parse_enum(&kind)?,
                source: parse_enum(&source)?,
                label: title,
                kept_at,
                duration_ms: duration,
                span: Some(Span::Quote { text: quote }),
                unconfirmed: trust == "untrusted",
                from_device: dev,
                calendar: None,
            },
            options: live,
        });
    }
    Ok(out)
}

impl Engine {
    /// The open "Which {name} is this?" questions, those of `item_ids` only when given.
    pub fn same_name_questions(
        &self,
        item_ids: Option<&[String]>,
    ) -> Result<Vec<SameNameQuestion>> {
        self.read(|conn| open_questions(conn, item_ids))
    }

    /// Answer a "Which {name} is this?" question. Choosing a person files what was held back
    /// under them; "Someone new" files it under a new person with the name; "Leave it" drops it.
    /// Either way the question is closed and the item keeps one mention of whom it was about.
    pub fn answer_same_name(
        &self,
        question_id: &str,
        answer: SameNameAnswer,
    ) -> Result<SameNameAnswered> {
        let now = fmt_ts(self.clock().now());
        let tz = self.clock().tz();
        let today = self.clock().now().to_zoned(tz.clone()).date().to_string();
        let (answered, files) = self.write(|tx| {
            let row: Option<(String, String, String, Option<String>)> = tx
                .query_row(
                    "SELECT item_id, name, candidates, answered_at FROM name_questions \
                     WHERE id = ?1",
                    [question_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?;
            let (item_id, name, candidates, done) =
                row.ok_or_else(|| Error::not_found(format!("question {question_id}")))?;
            if done.is_some() {
                return Err(Error::invalid("that question has been answered"));
            }
            let held: Vec<Held> = serde_json::from_str(&candidates)?;
            let (filed_as, role, files): (Option<(String, EntityKind)>, Option<String>, _) =
                match &answer {
                    SameNameAnswer::Leave => (None, None, Vec::new()),
                    SameNameAnswer::New => {
                        (Some((name.clone(), EntityKind::Person)), None, Vec::new())
                    }
                    SameNameAnswer::Entity { entity_id } => {
                        let found: Option<(String, String, Option<String>)> = tx
                            .query_row(
                                "SELECT name, kind, role FROM entities \
                                 WHERE id = ?1 AND merged_into IS NULL",
                                [entity_id],
                                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                            )
                            .optional()?;
                        let (ename, kind, role) =
                            found.ok_or_else(|| Error::not_found(format!("entity {entity_id}")))?;
                        (
                            Some((ename, parse_enum::<EntityKind>(&kind)?)),
                            role,
                            Vec::new(),
                        )
                    }
                };
            let mut files = files;
            let result_name = filed_as.as_ref().map_or(name.clone(), |f| f.0.clone());
            if let Some((target, kind)) = &filed_as {
                let routed: Vec<Routed> = held
                    .into_iter()
                    .map(|h| {
                        let mut c = h.into_candidate();
                        let said_as = c.subject.trim().to_string();
                        c.subject = target.clone();
                        c.subject_kind = Some(*kind);
                        Routed { cand: c, said_as }
                    })
                    .collect();
                files.extend(crate::memory::file_held(
                    tx, &now, &today, &tz, &item_id, routed,
                )?);
            }
            tx.execute(
                "UPDATE name_questions SET answered_at = ?2, answer = ?3 WHERE id = ?1",
                params![question_id, now, serde_json::to_string(&answer)?],
            )?;
            Ok((
                SameNameAnswered {
                    name: result_name,
                    role,
                },
                files,
            ))
        })?;
        crate::audio::shred_all(files);
        self.invalidate_vectors();
        self.bump_audio();
        Ok(answered)
    }
}

struct MentionRow {
    item_id: String,
    entity_id: String,
    state: MentionState,
}

fn mention_row(conn: &Connection, id: &str) -> Result<MentionRow> {
    let row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT item_id, entity_id, state FROM mentions WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let (item_id, entity_id, state) =
        row.ok_or_else(|| Error::not_found(format!("mention {id}")))?;
    Ok(MentionRow {
        item_id,
        entity_id,
        state: parse_enum(&state)?,
    })
}

fn mention_view(conn: &Connection, id: &str) -> Result<Mention> {
    let row = mention_row(conn, id)?;
    mentions_for_items(conn, std::slice::from_ref(&row.item_id))?
        .remove(&row.item_id)
        .unwrap_or_default()
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| Error::not_found(format!("mention {id}")))
}

/// The current memories this item says about a thing.
fn memories_of(conn: &Connection, item_id: &str, entity_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT m.id FROM memory_sources s JOIN memories m ON m.id = s.memory_id \
         WHERE s.item_id = ?1 AND m.entity_id = ?2 AND m.superseded_at IS NULL \
         AND m.forgetting = 0 ORDER BY m.recorded_at, m.id",
    )?;
    Ok(stmt
        .query_map(params![item_id, entity_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

/// A memory this item shares with others is split first: this item's link moves to a copy, so
/// moving the copy leaves what the other items said where it was. Returns the memory to move.
fn split_off(tx: &Transaction<'_>, memory: &str, item_id: &str) -> Result<String> {
    let others: u32 = tx.query_row(
        "SELECT COUNT(*) FROM memory_sources WHERE memory_id = ?1 AND item_id != ?2",
        params![memory, item_id],
        |r| r.get(0),
    )?;
    if others == 0 {
        return Ok(memory.to_string());
    }
    let cols: Vec<String> = {
        let mut stmt =
            tx.prepare("SELECT name FROM pragma_table_info('memories') WHERE name != 'id'")?;
        stmt.query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    let cols = cols.join(", ");
    let copy = new_id("mem");
    tx.execute(
        &format!("INSERT INTO memories(id, {cols}) SELECT ?1, {cols} FROM memories WHERE id = ?2"),
        params![copy, memory],
    )?;
    tx.execute(
        "INSERT INTO embeddings(memory_id, dim, vec) SELECT ?1, dim, vec FROM embeddings \
         WHERE memory_id = ?2",
        params![copy, memory],
    )?;
    tx.execute(
        "UPDATE memory_sources SET memory_id = ?1 WHERE memory_id = ?2 AND item_id = ?3",
        params![copy, memory, item_id],
    )?;
    Ok(copy)
}

/// What removing a mention let go, and what the mention was, so that Undo brings back exactly that.
#[derive(Serialize, Deserialize)]
struct LetGo {
    mention: String,
    entity: String,
    prior: MentionState,
    memories: Vec<(String, String, Option<String>)>,
}

impl Engine {
    /// "That's right": the link is confirmed, a role is kept when the thing has none, and the
    /// memories this item says about them count as confirmed, as confirming does anywhere.
    pub fn confirm_mention(&self, id: &str, role: Option<&str>) -> Result<Mention> {
        let now = fmt_ts(self.clock().now());
        let today = self.today();
        let role = role.map(str::trim).filter(|r| !r.is_empty());
        self.write(|tx| {
            let row = mention_row(tx, id)?;
            if row.state == MentionState::Removed {
                return Err(Error::invalid("bring the name back first"));
            }
            tx.execute(
                "UPDATE mentions SET state = 'confirmed' WHERE id = ?1",
                [id],
            )?;
            if let Some(role) = role {
                tx.execute(
                    "UPDATE entities SET role = ?2 WHERE id = ?1 AND role IS NULL",
                    params![row.entity_id, role],
                )?;
            }
            for memory in memories_of(tx, &row.item_id, &row.entity_id)? {
                crate::memory::confirm_tx(tx, &memory, true, &now, &today)?;
            }
            Ok(())
        })?;
        self.bump_audio();
        self.read(|conn| mention_view(conn, id))
    }

    /// "Not about {name}": the link goes, and what this item alone said about them is let go,
    /// not deleted. Nothing else is touched.
    pub fn remove_mention(&self, id: &str) -> Result<Mention> {
        let now = fmt_ts(self.clock().now());
        self.write(|tx| {
            let row = mention_row(tx, id)?;
            if row.state == MentionState::Removed {
                return Ok(());
            }
            let mut let_go = Vec::new();
            for memory in memories_of(tx, &row.item_id, &row.entity_id)? {
                let (status, at, alone): (String, Option<String>, bool) = tx.query_row(
                    "SELECT status, let_go_at, NOT EXISTS (SELECT 1 FROM memory_sources o \
                     WHERE o.memory_id = memories.id AND o.item_id != ?2) \
                     FROM memories WHERE id = ?1",
                    params![memory, row.item_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?;
                if !alone || status == "letGo" {
                    continue;
                }
                tx.execute(
                    "UPDATE memories SET status = 'letGo', let_go_at = ?2 WHERE id = ?1",
                    params![memory, now],
                )?;
                let_go.push((memory, status, at));
            }
            tx.execute(
                "INSERT INTO entity_changes(id, kind, at, detail) VALUES (?1, 'mention', ?2, ?3)",
                params![
                    new_id("chg"),
                    now,
                    serde_json::to_string(&LetGo {
                        mention: id.to_string(),
                        entity: row.entity_id.clone(),
                        prior: row.state,
                        memories: let_go,
                    })?
                ],
            )?;
            tx.execute("UPDATE mentions SET state = 'removed' WHERE id = ?1", [id])?;
            Ok(())
        })?;
        self.invalidate_vectors();
        self.bump_audio();
        self.read(|conn| mention_view(conn, id))
    }

    /// Undo "Not about {name}": the link and exactly what it let go come back.
    pub fn restore_mention(&self, id: &str) -> Result<Mention> {
        let now = fmt_ts(self.clock().now());
        self.write(|tx| {
            let row = mention_row(tx, id)?;
            if row.state != MentionState::Removed {
                return Ok(());
            }
            let record: Option<(String, String)> = tx
                .query_row(
                    "SELECT id, detail FROM entity_changes WHERE kind = 'mention' \
                     AND undone_at IS NULL AND json_extract(detail, '$.mention') = ?1 \
                     ORDER BY rowid DESC LIMIT 1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let mut state = MentionState::Found;
            if let Some((change, detail)) = record {
                let detail: LetGo = serde_json::from_str(&detail)?;
                state = detail.prior;
                for (memory, status, at) in detail.memories {
                    tx.execute(
                        "UPDATE memories SET status = ?2, let_go_at = ?3 \
                         WHERE id = ?1 AND status = 'letGo'",
                        params![memory, status, at],
                    )?;
                }
                tx.execute(
                    "UPDATE entity_changes SET undone_at = ?2 WHERE id = ?1",
                    params![change, now],
                )?;
            }
            tx.execute(
                "UPDATE mentions SET state = ?2 WHERE id = ?1",
                params![id, crate::util::enum_str(&state)],
            )?;
            Ok(())
        })?;
        self.invalidate_vectors();
        self.bump_audio();
        self.read(|conn| mention_view(conn, id))
    }

    /// "Someone else...": move only what this item says about the name to another person, place
    /// or thing, or a new one. Their words stay as they are. The link follows them.
    pub fn refile_mention(&self, id: &str, target: RefileTarget) -> Result<Mention> {
        let (row, memories, created) = self.read(|conn| {
            let row = mention_row(conn, id)?;
            let memories = memories_of(conn, &row.item_id, &row.entity_id)?;
            let created = match &target {
                RefileTarget::Existing { .. } => false,
                RefileTarget::New { name, .. } => {
                    let (_, key) = clean_name(name)?;
                    find_by_key(conn, &key, None)?.is_none()
                }
            };
            if let RefileTarget::Existing { entity_id } = &target
                && *entity_id == row.entity_id
            {
                return Err(Error::invalid("it is already filed there"));
            }
            if let RefileTarget::New { name, .. } = &target {
                let (_, key) = clean_name(name)?;
                if find_by_key(conn, &key, None)?.as_deref() == Some(row.entity_id.as_str()) {
                    return Err(Error::invalid("it is already filed there"));
                }
            }
            Ok((row, memories, created))
        })?;
        let mut dest: Option<String> = match &target {
            RefileTarget::Existing { entity_id } => Some(entity_id.clone()),
            RefileTarget::New { .. } => None,
        };
        let memories: Vec<String> = self.write(|tx| {
            memories
                .iter()
                .map(|m| split_off(tx, m, &row.item_id))
                .collect()
        })?;
        for memory in &memories {
            let refiled = self.refile_memory(memory, target.clone())?;
            dest = Some(refiled.entity.id);
        }
        self.write(|tx| {
            let dest = match dest {
                Some(d) => d,
                None => {
                    let RefileTarget::New { name, kind } = &target else {
                        unreachable!("an existing target names its thing")
                    };
                    let (name, key) = clean_name(name)?;
                    match crate::memory::upsert_entity(tx, &name, &key, Some(*kind))? {
                        Some((d, _)) => d,
                        None => unreachable!("a kind was given"),
                    }
                }
            };
            let twin: Option<String> = tx
                .query_row(
                    "SELECT id FROM mentions WHERE item_id = ?1 AND entity_id = ?2",
                    params![row.item_id, dest],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(twin) = twin {
                tx.execute("DELETE FROM mentions WHERE id = ?1", [id])?;
                renumber(tx, &row.item_id)?;
                return Ok(twin);
            }
            tx.execute(
                "UPDATE mentions SET entity_id = ?2, new = ?3 WHERE id = ?1",
                params![id, dest, created],
            )?;
            Ok(id.to_string())
        })
        .and_then(|kept| {
            self.invalidate_vectors();
            self.bump_audio();
            self.read(|conn| mention_view(conn, &kept))
        })
    }
}
