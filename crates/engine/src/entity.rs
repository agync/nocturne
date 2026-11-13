//! How things are filed: renaming a person, place or thing, merging two into one, and moving one
//! memory to another. Each change is logged so it can be taken back, and a name or a filing the
//! person chose is what a later read of the same item must respect (see `resolve_alias` and the
//! `moved_from_key` column).

use std::collections::HashSet;

use nocturne_contract::{
    ChangeKind, EntityChange, EntityKind, EntityView, MemoryFilter, MergeOutcome, RefileTarget,
    Refiled, RenameOutcome,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::Engine;
use crate::error::{Error, Result};
use crate::rows::{load_mem, mem_view};
use crate::util::{enum_str, fmt_ts, key_of, new_id};

/// The longest name a person can give.
const MAX_NAME: usize = 120;

struct EntRow {
    id: String,
    kind: EntityKind,
    name: String,
    key: String,
    merged_into: Option<String>,
}

impl EntRow {
    fn subject_key(&self) -> &str {
        key_tail(&self.key)
    }
}

/// The part of an entity key after its kind: the key its memories carry as `subject_key`.
fn key_tail(key: &str) -> &str {
    key.split_once(':').map_or(key, |(_, tail)| tail)
}

fn load_entity(conn: &Connection, id: &str) -> Result<Option<EntRow>> {
    Ok(conn
        .query_row(
            "SELECT id, kind, name, key, merged_into FROM entities WHERE id = ?1",
            [id],
            |r| {
                Ok(EntRow {
                    id: r.get(0)?,
                    kind: crate::rows::de(r.get(1)?)?,
                    name: r.get(2)?,
                    key: r.get(3)?,
                    merged_into: r.get(4)?,
                })
            },
        )
        .optional()?)
}

fn live_entity(conn: &Connection, id: &str) -> Result<EntRow> {
    load_entity(conn, id)?
        .filter(|e| e.merged_into.is_none())
        .ok_or_else(|| Error::not_found(format!("entity {id}")))
}

/// A live entity, other than `except`, whose name has this key whatever its kind: memories
/// match their subject by that key alone, so two things cannot share it.
fn entity_with_key(conn: &Connection, key: &str, except: Option<&str>) -> Result<Option<EntRow>> {
    let id: Option<String> = conn
        .query_row(
            "SELECT id FROM entities WHERE merged_into IS NULL AND id <> ?2 \
             AND substr(key, instr(key, ':') + 1) = ?1 ORDER BY id LIMIT 1",
            params![key, except.unwrap_or("")],
            |r| r.get(0),
        )
        .optional()?;
    id.map(|id| live_entity(conn, &id)).transpose()
}

/// Names must be something a person could say: trimmed, one space between words, not empty
/// once keyed, not absurdly long.
fn clean_name(name: &str) -> Result<(String, String)> {
    let name = name.nfc().collect::<String>();
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.chars().count() > MAX_NAME {
        return Err(Error::invalid("that name is too long"));
    }
    let key = key_of(&name);
    if key.is_empty() {
        return Err(Error::invalid("a name needs some letters"));
    }
    Ok((name, key))
}

/// Replace each whole-word, exact-case occurrence of `from` in `text` with `to`. Both are put
/// in one Unicode form first, so a name typed with a combining accent still matches. A text
/// with no occurrence is returned as it was.
pub(crate) fn replace_name(text: &str, from: &str, to: &str) -> String {
    let from: String = from.nfc().collect();
    if from.is_empty() || from == to {
        return text.to_string();
    }
    let normal: String = text.nfc().collect();
    let to: String = to.nfc().collect();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::with_capacity(normal.len());
    let mut last = 0;
    let mut replaced = false;
    for (start, found) in normal.match_indices(&from) {
        let end = start + found.len();
        let before = normal[..start].chars().next_back();
        let after = normal[end..].chars().next();
        if start < last || before.is_some_and(word) || after.is_some_and(word) {
            continue;
        }
        out.push_str(&normal[last..start]);
        out.push_str(&to);
        last = end;
        replaced = true;
    }
    if !replaced {
        return text.to_string();
    }
    out.push_str(&normal[last..]);
    out
}

/// What a claim read about a changed name is filed under now, and the change that set it.
pub(crate) struct Alias {
    pub name: String,
    pub change_id: Option<String>,
}

/// A claim read about a name the person has since changed is about the thing it now names.
/// Returns that thing, unless something live already carries the old name.
pub(crate) fn resolve_alias(tx: &Transaction<'_>, subject_key: &str) -> Result<Option<Alias>> {
    if entity_with_key(tx, subject_key, None)?.is_some() {
        return Ok(None);
    }
    Ok(tx
        .query_row(
            "SELECT e.name, n.change_id FROM entity_names n JOIN entities e ON e.id = n.entity_id \
             WHERE n.key = ?1 AND e.merged_into IS NULL ORDER BY n.id DESC LIMIT 1",
            [subject_key],
            |r| {
                Ok(Alias {
                    name: r.get(0)?,
                    change_id: r.get(1)?,
                })
            },
        )
        .optional()?)
}

/// A read wrote this memory (made it, or put the new name in its words) because of a change the
/// person made: the change takes it into account when it is taken back.
pub(crate) fn note_rewrite(tx: &Transaction<'_>, change: Option<&str>, memory: &str) -> Result<()> {
    let Some(change) = change else {
        return Ok(());
    };
    let row: Option<(String, Option<String>, String)> = tx
        .query_row(
            "SELECT kind, undone_at, detail FROM entity_changes WHERE id = ?1",
            [change],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((kind, None, detail)) = row else {
        return Ok(());
    };
    let mut detail: serde_json::Value = serde_json::from_str(&detail)?;
    // Under a merge it also came from the merged-in thing, so it goes back with the rest.
    let lists: &[&str] = if kind == "merge" {
        &["reworded", "moved"]
    } else {
        &["reworded"]
    };
    for list in lists {
        if let Some(ids) = detail.get_mut(*list).and_then(|v| v.as_array_mut())
            && !ids.iter().any(|v| v.as_str() == Some(memory))
        {
            ids.push(serde_json::Value::String(memory.to_string()));
        }
    }
    tx.execute(
        "UPDATE entity_changes SET detail = ?2 WHERE id = ?1",
        params![change, serde_json::to_string(&detail)?],
    )?;
    Ok(())
}

/// The live entity carrying this name key, whatever its kind (memories match their subject by
/// key alone). With several, the one of the requested kind, then person, place, thing, then id.
pub(crate) fn find_by_key(
    conn: &Connection,
    key: &str,
    prefer: Option<EntityKind>,
) -> Result<Option<String>> {
    let prefer = prefer.map(|k| enum_str(&k)).unwrap_or_default();
    Ok(conn
        .query_row(
            "SELECT id FROM entities WHERE merged_into IS NULL \
             AND substr(key, instr(key, ':') + 1) = ?1 \
             ORDER BY kind = ?2 DESC, CASE kind WHEN 'person' THEN 0 WHEN 'place' THEN 1 ELSE 2 END, id \
             LIMIT 1",
            params![key, prefer],
            |r| r.get(0),
        )
        .optional()?)
}

#[derive(Serialize, Deserialize)]
struct RenameDetail {
    entity: String,
    to_key: String,
    /// Memories whose wording carried the old name.
    reworded: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct MergeDetail {
    /// The one that remains.
    entity: String,
    from: String,
    from_key: String,
    moved: Vec<String>,
    reworded: Vec<String>,
    /// Duplicates in the merged-in thing that were folded into what the other already said:
    /// the folded memory and the one that kept the claim.
    folded: Vec<(String, String)>,
    /// Receipts added to the memory that kept a folded claim.
    receipts_added: Vec<(String, String)>,
    names_moved: Vec<i64>,
}

#[derive(Serialize, Deserialize)]
struct RefileDetail {
    /// Where the memories came from.
    entity: String,
    to: String,
    /// The target was made for this move.
    created: bool,
    /// Each moved memory with what it said it was moved from before.
    memories: Vec<(String, Option<String>, Option<String>)>,
}

fn log_change(
    tx: &Transaction<'_>,
    kind: ChangeKind,
    now: &str,
    detail: &impl Serialize,
) -> Result<String> {
    let id = new_id("chg");
    tx.execute(
        "INSERT INTO entity_changes(id, kind, at, detail) VALUES (?1, ?2, ?3, ?4)",
        params![id, enum_str(&kind), now, serde_json::to_string(detail)?],
    )?;
    Ok(id)
}

fn change_detail<T: DeserializeOwned>(conn: &Connection, id: &str, kind: ChangeKind) -> Result<T> {
    let row: Option<(String, Option<String>, String)> = conn
        .query_row(
            "SELECT kind, undone_at, detail FROM entity_changes WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let (k, undone, detail) = row.ok_or_else(|| Error::not_found(format!("change {id}")))?;
    if k != enum_str(&kind) {
        return Err(Error::invalid("that is not the kind of change expected"));
    }
    if undone.is_some() {
        return Err(Error::invalid("that change has already been taken back"));
    }
    Ok(serde_json::from_str(&detail)?)
}

/// Give an entity and everything filed under it a new name. Memories whose wording used the
/// old name follow it: all of them, or only `only` when taking a rename back. Returns the ids
/// reworded.
fn rename_core(
    tx: &Transaction<'_>,
    ent: &EntRow,
    new_name: &str,
    new_key: &str,
    only: Option<&[String]>,
) -> Result<Vec<String>> {
    let renamed = tx.execute(
        "UPDATE entities SET name = ?2, key = ?3 WHERE id = ?1",
        params![
            ent.id,
            new_name,
            format!("{}:{new_key}", enum_str(&ent.kind))
        ],
    )?;
    if renamed != 1 {
        return Err(Error::invalid("it could not be renamed"));
    }
    tx.execute(
        "UPDATE memories SET subject = ?2, subject_key = ?3 WHERE entity_id = ?1",
        params![ent.id, new_name, new_key],
    )?;
    rewrite_wording(tx, &ent.id, &ent.name, new_name, only)
}

fn rewrite_wording(
    tx: &Transaction<'_>,
    entity: &str,
    from: &str,
    to: &str,
    only: Option<&[String]>,
) -> Result<Vec<String>> {
    let rows: Vec<(String, String, String)> = {
        let mut stmt = tx.prepare(
            "SELECT id, sentence, object FROM memories \
             WHERE entity_id = ?1 AND superseded_at IS NULL",
        )?;
        stmt.query_map([entity], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut changed = Vec::new();
    for (id, sentence, object) in rows {
        if only.is_some_and(|ids| !ids.contains(&id)) {
            continue;
        }
        let new_sentence = replace_name(&sentence, from, to);
        if new_sentence == sentence {
            continue;
        }
        // A memory the person corrected in their own words has that text as its object too.
        let new_object = if object == sentence {
            new_sentence.clone()
        } else {
            object.clone()
        };
        tx.execute(
            "UPDATE memories SET sentence = ?2, object = ?3, object_key = ?4 WHERE id = ?1",
            params![id, new_sentence, new_object, key_of(&new_object)],
        )?;
        tx.execute("DELETE FROM embeddings WHERE memory_id = ?1", [&id])?;
        changed.push(id);
    }
    Ok(changed)
}

/// Whether a change made after this one, and not taken back, touches either entity. The newest
/// change goes first: a rename is held only by later renames and merges (a move leaves names
/// alone), a merge by any later change.
fn blocked_by_later(
    conn: &Connection,
    change: &str,
    entities: [&str; 2],
    any_kind: bool,
) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM entity_changes l \
         WHERE l.rowid > (SELECT rowid FROM entity_changes WHERE id = ?1) \
           AND l.undone_at IS NULL AND (?4 OR l.kind IN ('rename', 'merge')) \
           AND (json_extract(l.detail, '$.entity') IN (?2, ?3) \
             OR json_extract(l.detail, '$.from') IN (?2, ?3) \
             OR json_extract(l.detail, '$.to') IN (?2, ?3)))",
        params![change, entities[0], entities[1], any_kind],
        |r| r.get(0),
    )?)
}

fn memory_exists(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM memories WHERE id = ?1)",
        [id],
        |r| r.get(0),
    )?)
}

fn current_count(conn: &Connection, entity: &str) -> Result<u32> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE entity_id = ?1 AND superseded_at IS NULL \
         AND forgetting = 0",
        [entity],
        |r| r.get(0),
    )?)
}

/// People, places and things, with their counts, names they had and the change that can still
/// be taken back. Empty ones (everything moved out of them) are left out of listings.
pub(crate) fn entity_views(
    conn: &Connection,
    kind: Option<EntityKind>,
    id: Option<&str>,
) -> Result<Vec<EntityView>> {
    let mut sql = String::from(
        "SELECT e.id, e.kind, e.name, e.role, \
         (SELECT COUNT(*) FROM memories m WHERE m.entity_id = e.id AND m.superseded_at IS NULL \
          AND m.forgetting = 0), \
         (SELECT COUNT(DISTINCT s.item_id) FROM memory_sources s JOIN memories m ON m.id = s.memory_id \
          WHERE m.entity_id = e.id AND m.superseded_at IS NULL AND m.forgetting = 0) \
         FROM entities e WHERE e.merged_into IS NULL",
    );
    let mut args: Vec<String> = Vec::new();
    if let Some(k) = kind {
        args.push(enum_str(&k));
        sql.push_str(&format!(" AND e.kind = ?{}", args.len()));
    }
    if let Some(id) = id {
        args.push(id.to_string());
        sql.push_str(&format!(" AND e.id = ?{}", args.len()));
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
            former_names: Vec::new(),
            last_change: None,
        })
    })?;
    let mut out: Vec<EntityView> = rows.collect::<rusqlite::Result<_>>()?;
    if id.is_none() {
        out.retain(|e| e.memory_count > 0);
    }
    for e in &mut out {
        e.former_names = former_names(conn, &e.id, &e.name)?;
        e.last_change = last_change(conn, &e.id)?;
    }
    Ok(out)
}

pub(crate) fn former_names(conn: &Connection, entity: &str, current: &str) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare_cached("SELECT name FROM entity_names WHERE entity_id = ?1 ORDER BY id DESC")?;
    let names: Vec<String> = stmt
        .query_map([entity], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut seen = HashSet::new();
    Ok(names
        .into_iter()
        .filter(|n| n != current && seen.insert(n.clone()))
        .collect())
}

/// The newest rename or merge into this entity that has not been taken back, if it still can be.
fn last_change(conn: &Connection, entity: &str) -> Result<Option<EntityChange>> {
    let row: Option<(String, String, String, String)> = conn
        .query_row(
            "SELECT id, kind, at, detail FROM entity_changes \
             WHERE undone_at IS NULL AND kind IN ('rename', 'merge') \
             AND json_extract(detail, '$.entity') = ?1 ORDER BY rowid DESC LIMIT 1",
            [entity],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((id, kind, at, detail)) = row else {
        return Ok(None);
    };
    let Some(ent) = load_entity(conn, entity)? else {
        return Ok(None);
    };
    let (kind, from) = if kind == "rename" {
        let d: RenameDetail = serde_json::from_str(&detail)?;
        if d.to_key != ent.subject_key() {
            return Ok(None);
        }
        let from: Option<String> = conn
            .query_row(
                "SELECT name FROM entity_names WHERE change_id = ?1 ORDER BY id LIMIT 1",
                [&id],
                |r| r.get(0),
            )
            .optional()?;
        if blocked_by_later(conn, &id, [entity, entity], false)? {
            return Ok(None);
        }
        (ChangeKind::Rename, from)
    } else {
        let d: MergeDetail = serde_json::from_str(&detail)?;
        if blocked_by_later(conn, &id, [&d.entity, &d.from], true)? {
            return Ok(None);
        }
        let gone = load_entity(conn, &d.from)?.filter(|g| g.merged_into.as_deref() == Some(entity));
        (ChangeKind::Merge, gone.map(|g| g.name))
    };
    Ok(from.map(|from| EntityChange { id, kind, from, at }))
}

impl Engine {
    /// Rename a person, place or thing. The new name shows everywhere the old one did, the old
    /// name is kept as a former name, and a later read of the same words files under the new
    /// name. A name another thing already has changes nothing and says which, so the person
    /// can merge them.
    pub fn rename_entity(&self, id: &str, name: &str) -> Result<RenameOutcome> {
        let (name, key) = clean_name(name)?;
        let now = fmt_ts(self.clock().now());
        let out = self.write(|tx| {
            let ent = live_entity(tx, id)?;
            if let Some(other) = entity_with_key(tx, &key, Some(id))? {
                let entity = entity_views(tx, None, Some(&other.id))?.remove(0);
                return Ok(RenameOutcome::Exists { entity });
            }
            if ent.name == name {
                let entity = entity_views(tx, None, Some(id))?.remove(0);
                return Ok(RenameOutcome::Renamed {
                    entity,
                    change_id: None,
                });
            }
            let change = new_id("chg");
            // The old name stays as a former name, and a name this one had before and now
            // takes up again is no longer former.
            tx.execute(
                "DELETE FROM entity_names WHERE entity_id = ?1 AND key = ?2",
                params![id, key],
            )?;
            tx.execute(
                "INSERT INTO entity_names(entity_id, name, key, change_id, at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, ent.name, ent.subject_key(), change, now],
            )?;
            let reworded = rename_core(tx, &ent, &name, &key, None)?;
            tx.execute(
                "INSERT INTO entity_changes(id, kind, at, detail) VALUES (?1, 'rename', ?2, ?3)",
                params![
                    change,
                    now,
                    serde_json::to_string(&RenameDetail {
                        entity: id.to_string(),
                        to_key: key,
                        reworded,
                    })?
                ],
            )?;
            let entity = entity_views(tx, None, Some(id))?.remove(0);
            Ok(RenameOutcome::Renamed {
                entity,
                change_id: Some(change),
            })
        })?;
        self.invalidate_vectors();
        self.bump_audio();
        Ok(out)
    }

    /// Make `from` and `into` one thing, named `into`'s. What was filed under `from` moves to
    /// `into`, its names become names `into` is also known by, and a claim both already held
    /// is kept once with both receipts. `from` stays, hidden, so the merge can be taken back.
    pub fn merge_entities(&self, from: &str, into: &str) -> Result<MergeOutcome> {
        if from == into {
            return Err(Error::invalid("a thing cannot be merged with itself"));
        }
        let now = fmt_ts(self.clock().now());
        let out = self.write(|tx| {
            let gone = live_entity(tx, from)?;
            let kept = live_entity(tx, into)?;
            let moved: Vec<String> = {
                let mut stmt = tx.prepare("SELECT id FROM memories WHERE entity_id = ?1")?;
                stmt.query_map([from], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            };
            tx.execute(
                "UPDATE memories SET entity_id = ?2, subject = ?3, subject_key = ?4 \
                 WHERE entity_id = ?1",
                params![from, into, kept.name, kept.subject_key()],
            )?;
            let reworded = rewrite_wording(tx, into, &gone.name, &kept.name, Some(&moved))?;

            let mut folded = Vec::new();
            let mut receipts_added = Vec::new();
            for m in &moved {
                let Some(row) = load_mem(tx, m)? else {
                    continue;
                };
                if row.superseded_at.is_some() {
                    continue;
                }
                let twins: Vec<(String, bool, String)> = {
                    let mut stmt = tx.prepare(
                        "SELECT t.id, t.confirmed, t.status FROM memories t \
                         WHERE t.entity_id = ?1 AND t.id <> ?2 AND t.predicate = ?3 \
                         AND t.object_key = ?4 AND t.superseded_at IS NULL AND t.forgetting = 0 \
                         AND t.private = ?5 AND t.keep_forever = ?6 \
                         AND NOT EXISTS (SELECT 1 FROM conflicts c WHERE c.resolved_at IS NULL \
                           AND (c.a IN (?2, t.id) OR c.b IN (?2, t.id))) \
                         ORDER BY t.recorded_at, t.id",
                    )?;
                    stmt.query_map(
                        params![
                            into,
                            m,
                            row.predicate,
                            row.object_key,
                            row.private,
                            row.keep_forever
                        ],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )?
                    .collect::<rusqlite::Result<_>>()?
                };
                // Only a claim the other thing already held, in the same way (as sure, as
                // private, as lasting), is folded into it; otherwise both stay.
                let Some((twin, _, _)) = twins.into_iter().find(|(id, confirmed, status)| {
                    !moved.contains(id)
                        && *confirmed == row.confirmed
                        && *status == enum_str(&row.status)
                }) else {
                    continue;
                };
                let sources: Vec<String> = {
                    let mut stmt =
                        tx.prepare("SELECT item_id FROM memory_sources WHERE memory_id = ?1")?;
                    stmt.query_map([m], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?
                };
                for item in sources {
                    let added = tx.execute(
                        "INSERT OR IGNORE INTO memory_sources(memory_id, item_id, span) \
                         SELECT ?1, item_id, span FROM memory_sources WHERE memory_id = ?2 \
                         AND item_id = ?3",
                        params![twin, m, item],
                    )?;
                    if added > 0 {
                        receipts_added.push((twin.clone(), item));
                    }
                }
                tx.execute(
                    "UPDATE memories SET superseded_at = ?2, superseded_by = ?3 WHERE id = ?1",
                    params![m, now, twin],
                )?;
                folded.push((m.clone(), twin));
            }

            // What `from` was called, and had been called, are now also names of `into`.
            let names_moved: Vec<i64> = {
                let mut stmt = tx.prepare("SELECT id FROM entity_names WHERE entity_id = ?1")?;
                stmt.query_map([from], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            };
            tx.execute(
                "UPDATE entity_names SET entity_id = ?2 WHERE entity_id = ?1",
                params![from, into],
            )?;
            let change = new_id("chg");
            tx.execute(
                "INSERT INTO entity_names(entity_id, name, key, change_id, at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![into, gone.name, gone.subject_key(), change, now],
            )?;
            tx.execute(
                "UPDATE entities SET merged_into = ?2, key = ?3 WHERE id = ?1",
                params![from, into, format!("merged:{from}")],
            )?;
            let detail = MergeDetail {
                entity: into.to_string(),
                from: from.to_string(),
                from_key: gone.key.clone(),
                moved: moved.clone(),
                reworded,
                folded,
                receipts_added,
                names_moved,
            };
            tx.execute(
                "INSERT INTO entity_changes(id, kind, at, detail) VALUES (?1, 'merge', ?2, ?3)",
                params![change, now, serde_json::to_string(&detail)?],
            )?;
            let entity = entity_views(tx, None, Some(into))?.remove(0);
            Ok(MergeOutcome {
                entity,
                change_id: change,
                moved: u32::try_from(moved.len()).unwrap_or(u32::MAX),
            })
        })?;
        self.invalidate_vectors();
        self.bump_audio();
        Ok(out)
    }

    /// Move one memory (and the earlier versions of it) to another person, place or thing, or
    /// to a new one made from `target`. Its words are not touched. A later read of the same
    /// words finds it where the person put it.
    pub fn refile_memory(&self, id: &str, target: RefileTarget) -> Result<Refiled> {
        let now = fmt_ts(self.clock().now());
        let out = self.write(|tx| {
            let mem = load_mem(tx, id)?.ok_or_else(|| Error::not_found(format!("memory {id}")))?;
            if mem.superseded_at.is_some() {
                return Err(Error::invalid("only a current memory can be moved"));
            }
            let from = mem
                .entity_id
                .clone()
                .ok_or_else(|| Error::invalid("that memory is not filed under anything"))?;
            let (dest, created) = match target {
                RefileTarget::Existing { entity_id } => (live_entity(tx, &entity_id)?, false),
                RefileTarget::New { name, kind } => {
                    let (name, key) = clean_name(&name)?;
                    match entity_with_key(tx, &key, None)? {
                        Some(found) => (found, false),
                        None => {
                            let new = new_id("ent");
                            tx.execute(
                                "INSERT INTO entities(id, kind, name, key) VALUES (?1, ?2, ?3, ?4)",
                                params![
                                    new,
                                    enum_str(&kind),
                                    name,
                                    format!("{}:{key}", enum_str(&kind))
                                ],
                            )?;
                            (live_entity(tx, &new)?, true)
                        }
                    }
                }
            };
            if dest.id == from {
                return Err(Error::invalid("it is already filed there"));
            }
            let versions: Vec<(String, Option<String>, Option<String>)> = {
                let ids = crate::memory::chain(tx, id)?;
                let mut out = Vec::new();
                for v in ids {
                    let row: Option<(Option<String>, Option<String>, Option<String>)> = tx
                        .query_row(
                            "SELECT entity_id, moved_from, moved_from_key FROM memories \
                             WHERE id = ?1",
                            [&v],
                            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                        )
                        .optional()?;
                    if let Some((entity, mf, mk)) = row
                        && entity.as_deref() == Some(from.as_str())
                    {
                        out.push((v, mf, mk));
                    }
                }
                out
            };
            for (v, _, _) in &versions {
                tx.execute(
                    "UPDATE memories SET entity_id = ?2, subject = ?3, subject_key = ?4, \
                     moved_from = COALESCE(moved_from, subject), \
                     moved_from_key = COALESCE(moved_from_key, subject_key) WHERE id = ?1",
                    params![v, dest.id, dest.name, dest.subject_key()],
                )?;
            }
            let change = log_change(
                tx,
                ChangeKind::Refile,
                &now,
                &RefileDetail {
                    entity: from,
                    to: dest.id.clone(),
                    created,
                    memories: versions,
                },
            )?;
            let memory = mem_view(
                tx,
                &load_mem(tx, id)?.ok_or_else(|| Error::not_found(format!("memory {id}")))?,
            )?;
            let entity = entity_views(tx, None, Some(&dest.id))?.remove(0);
            Ok(Refiled {
                memory,
                entity,
                change_id: change,
            })
        })?;
        self.bump_audio();
        Ok(out)
    }

    /// Take back a rename, a merge or a move, if nothing since has made that impossible.
    pub fn undo_change(&self, change_id: &str) -> Result<()> {
        let now = fmt_ts(self.clock().now());
        let kind: String = self.read(|conn| {
            conn.query_row(
                "SELECT kind FROM entity_changes WHERE id = ?1",
                [change_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::not_found(format!("change {change_id}")))
        })?;
        self.write(|tx| {
            match kind.as_str() {
                "rename" => undo_rename(tx, change_id)?,
                "merge" => undo_merge(tx, change_id)?,
                _ => undo_refile(tx, change_id)?,
            }
            tx.execute(
                "UPDATE entity_changes SET undone_at = ?2 WHERE id = ?1",
                params![change_id, now],
            )?;
            Ok(())
        })?;
        self.invalidate_vectors();
        self.bump_audio();
        Ok(())
    }

    /// Every memory filed under this person, place or thing, and the thing itself.
    pub fn entity(&self, id: &str) -> Result<(EntityView, Vec<nocturne_contract::MemoryView>)> {
        let view = self
            .read(|conn| entity_views(conn, None, Some(id)))?
            .into_iter()
            .next()
            .ok_or_else(|| Error::not_found(format!("entity {id}")))?;
        let memories = self.memories(MemoryFilter {
            entity_id: Some(id.to_string()),
            ..Default::default()
        })?;
        Ok((view, memories))
    }
}

fn undo_rename(tx: &Transaction<'_>, change: &str) -> Result<()> {
    let d: RenameDetail = change_detail(tx, change, ChangeKind::Rename)?;
    let ent = live_entity(tx, &d.entity)?;
    if d.to_key != ent.subject_key() || blocked_by_later(tx, change, [&ent.id, &ent.id], false)? {
        return Err(Error::invalid("it has been renamed since"));
    }
    let (old_name, old_key): (String, String) = tx
        .query_row(
            "SELECT name, key FROM entity_names WHERE change_id = ?1 ORDER BY id LIMIT 1",
            [change],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| Error::invalid("the earlier name is gone"))?;
    if entity_with_key(tx, &old_key, Some(&ent.id))?.is_some() {
        return Err(Error::invalid("something else has that name now"));
    }
    rename_core(tx, &ent, &old_name, &old_key, Some(&d.reworded))?;
    // Taken back, the name it had since is no longer one it is known by.
    tx.execute("DELETE FROM entity_names WHERE change_id = ?1", [change])?;
    tx.execute(
        "DELETE FROM entity_names WHERE entity_id = ?1 AND key = ?2",
        params![ent.id, old_key],
    )?;
    Ok(())
}

fn undo_merge(tx: &Transaction<'_>, change: &str) -> Result<()> {
    let d: MergeDetail = change_detail(tx, change, ChangeKind::Merge)?;
    let kept = live_entity(tx, &d.entity)?;
    let gone = load_entity(tx, &d.from)?
        .filter(|g| g.merged_into.as_deref() == Some(d.entity.as_str()))
        .ok_or_else(|| Error::invalid("what was merged in is gone"))?;
    if blocked_by_later(tx, change, [&d.entity, &d.from], true)? {
        return Err(Error::invalid(
            "something has changed since, so undo that first",
        ));
    }
    let gone_tail = key_tail(&d.from_key).to_string();
    if entity_with_key(tx, &gone_tail, Some(&kept.id))?.is_some() {
        return Err(Error::invalid("something else has that name now"));
    }
    let original = EntRow {
        key: d.from_key.clone(),
        ..gone
    };
    for (folded, _) in &d.folded {
        if !memory_exists(tx, folded)? {
            continue;
        }
        let n = tx.execute(
            "UPDATE memories SET superseded_at = NULL, superseded_by = NULL WHERE id = ?1",
            [folded],
        )?;
        if n != 1 {
            return Err(Error::invalid("a memory could not be separated again"));
        }
    }
    for (memory, item) in &d.receipts_added {
        tx.execute(
            "DELETE FROM memory_sources WHERE memory_id = ?1 AND item_id = ?2",
            params![memory, item],
        )?;
    }
    // Wording first, while the memories are still `kept`'s.
    rewrite_wording(tx, &kept.id, &kept.name, &original.name, Some(&d.reworded))?;
    for m in &d.moved {
        // A memory forgotten since is not there to move back; one that is there must go.
        if !memory_exists(tx, m)? {
            continue;
        }
        let n = tx.execute(
            "UPDATE memories SET entity_id = ?2, subject = ?3, subject_key = ?4 \
             WHERE id = ?1 AND entity_id = ?5",
            params![m, original.id, original.name, gone_tail, kept.id],
        )?;
        if n != 1 {
            return Err(Error::invalid("a memory has been moved since"));
        }
    }
    tx.execute(
        "UPDATE entities SET merged_into = NULL, key = ?2 WHERE id = ?1",
        params![original.id, d.from_key],
    )?;
    tx.execute("DELETE FROM entity_names WHERE change_id = ?1", [change])?;
    for row in &d.names_moved {
        let there: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM entity_names WHERE id = ?1)",
            [row],
            |r| r.get(0),
        )?;
        if !there {
            continue;
        }
        let n = tx.execute(
            "UPDATE entity_names SET entity_id = ?2 WHERE id = ?1 AND entity_id = ?3",
            params![row, original.id, kept.id],
        )?;
        if n != 1 {
            return Err(Error::invalid("a name has changed since"));
        }
    }
    Ok(())
}

fn undo_refile(tx: &Transaction<'_>, change: &str) -> Result<()> {
    let d: RefileDetail = change_detail(tx, change, ChangeKind::Refile)?;
    let back = live_entity(tx, &d.entity)
        .map_err(|_| Error::invalid("where it came from is gone, so it cannot be moved back"))?;
    let dest = load_entity(tx, &d.to)?;
    for (id, moved_from, moved_from_key) in &d.memories {
        if !memory_exists(tx, id)? {
            continue;
        }
        let n = tx.execute(
            "UPDATE memories SET entity_id = ?2, subject = ?3, subject_key = ?4, \
             moved_from = ?5, moved_from_key = ?6 WHERE id = ?1 AND entity_id = ?7",
            params![
                id,
                back.id,
                back.name,
                back.subject_key(),
                moved_from,
                moved_from_key,
                d.to
            ],
        )?;
        if n != 1 {
            return Err(Error::invalid("it has been moved again since"));
        }
    }
    if d.created && dest.is_some() && current_count(tx, &d.to)? == 0 {
        let left: u32 = tx.query_row(
            "SELECT COUNT(*) FROM memories WHERE entity_id = ?1",
            [&d.to],
            |r| r.get(0),
        )?;
        if left == 0 {
            tx.execute("DELETE FROM entities WHERE id = ?1", [&d.to])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::replace_name;

    #[test]
    fn a_name_is_replaced_as_a_whole_word_in_the_same_case() {
        assert_eq!(
            replace_name("Leo likes tea.", "Leo", "Max"),
            "Max likes tea."
        );
        assert_eq!(replace_name("Ask Leo", "Leo", "Max"), "Ask Max");
        assert_eq!(
            replace_name("Leo, Leo and Leo.", "Leo", "Max"),
            "Max, Max and Max."
        );
        assert_eq!(replace_name("leo is a lion", "Leo", "Max"), "leo is a lion");
        assert_eq!(
            replace_name("Two words here", "Two words", "One"),
            "One here"
        );
    }

    #[test]
    fn possessives_follow_the_name() {
        assert_eq!(
            replace_name("Leo’s swimming", "Leo", "Max"),
            "Max’s swimming"
        );
        assert_eq!(
            replace_name("Leo's swimming", "Leo", "Max"),
            "Max's swimming"
        );
        assert_eq!(
            replace_name("The dentist of Leo", "Leo", "Max"),
            "The dentist of Max"
        );
    }

    #[test]
    fn part_of_a_longer_word_is_left_alone() {
        for text in [
            "Leopard",
            "Cleo",
            "Leon",
            "Leo_two",
            "2Leo",
            "Cleopatra Leopold",
        ] {
            assert_eq!(replace_name(text, "Leo", "Max"), text, "{text}");
        }
        assert_eq!(
            replace_name("Leo and Leopold", "Leo", "Max"),
            "Max and Leopold"
        );
    }

    #[test]
    fn nothing_changes_when_there_is_nothing_to_change() {
        assert_eq!(replace_name("Leo", "Leo", "Leo"), "Leo");
        assert_eq!(replace_name("Leo", "", "Max"), "Leo");
        assert_eq!(replace_name("", "Leo", "Max"), "");
        // Text in another Unicode form is returned exactly as it was.
        assert_eq!(
            replace_name("Cafe\u{301} is open", "Leo", "Max"),
            "Cafe\u{301} is open"
        );
    }

    #[test]
    fn both_sides_are_put_in_one_unicode_form() {
        // Decomposed in the text, composed in the name, and the other way round.
        assert_eq!(
            replace_name("Cafe\u{301} is open", "Café", "Bar"),
            "Bar is open"
        );
        assert_eq!(
            replace_name("Café is open", "Cafe\u{301}", "Bar"),
            "Bar is open"
        );
        // The replacement is composed too, so what is written is in the form the store keeps.
        assert_eq!(
            replace_name("Bar is open", "Bar", "Cafe\u{301}"),
            "Café is open"
        );
        // An accent after the name makes it a different word.
        assert_eq!(
            replace_name("Cafés are open", "Café", "Bar"),
            "Cafés are open"
        );
        assert_eq!(
            replace_name("Cafe\u{301}s are open", "Café", "Bar"),
            "Cafe\u{301}s are open"
        );
    }
}
