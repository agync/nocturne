//! What a phone that brought its things in had forgotten or marked of what its mind worked out,
//! kept by claim so this computer's own reading keeps it (`docs/design/moving-memory.md`). The
//! things arrive and are read again here; a memory read from a thing that carries a mark for its
//! claim takes the mark: private, done, kept forever, its tags. A calendar entry the phone sends
//! again carries its marks and forgotten claims by the phone's own id for it, so every version
//! of it takes them.

use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::{Transaction, params};

use crate::error::Result;
use crate::util::claim_hash;

/// The person's marks on one claim.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Marks {
    pub private: bool,
    pub done: bool,
    pub keep_forever: bool,
    pub tags: Vec<String>,
}

impl Marks {
    pub fn any(&self) -> bool {
        self.private || self.done || self.keep_forever || !self.tags.is_empty()
    }

    /// Both sets of marks: a mark on either holds.
    pub fn join(&mut self, other: &Marks) {
        self.private |= other.private;
        self.done |= other.done;
        self.keep_forever |= other.keep_forever;
        for t in &other.tags {
            if !self.tags.contains(t) {
                self.tags.push(t.clone());
            }
        }
    }

    fn tags_json(&self) -> String {
        serde_json::to_string(&self.tags).expect("tags serialise")
    }
}

fn tags_of(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

/// Every claim a memory may be read again under: its subject as filed now, and as first filed
/// before the person moved it.
pub(crate) fn claims_of(
    subject: &str,
    moved_from: Option<&str>,
    predicate: &str,
    object: &str,
) -> Vec<String> {
    let mut out = vec![claim_hash(subject, predicate, object)];
    if let Some(m) = moved_from.filter(|m| *m != subject) {
        out.push(claim_hash(m, predicate, object));
    }
    out
}

/// Keep `marks` for `claim` on item `item_id`, joined with any kept already.
pub(crate) fn keep(tx: &Transaction<'_>, item_id: &str, claim: &str, marks: &Marks) -> Result<()> {
    let had: Option<(bool, bool, bool, String)> = tx
        .query_row(
            "SELECT private, done, keep_forever, tags FROM carried_marks \
             WHERE item_id = ?1 AND claim = ?2",
            params![item_id, claim],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok();
    let mut all = marks.clone();
    if let Some((private, done, keep_forever, tags)) = had {
        all.join(&Marks {
            private,
            done,
            keep_forever,
            tags: tags_of(&tags),
        });
    }
    tx.execute(
        "INSERT OR REPLACE INTO carried_marks(item_id, claim, private, done, keep_forever, tags) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            item_id,
            claim,
            all.private,
            all.done,
            all.keep_forever,
            all.tags_json()
        ],
    )?;
    Ok(())
}

/// Keep a forgotten claim or marks for a calendar entry the phone sends again.
pub(crate) fn keep_for_phone(
    tx: &Transaction<'_>,
    device: &str,
    source: &str,
    phone_id: &str,
    claim: &str,
    forgotten: bool,
    marks: &Marks,
) -> Result<()> {
    tx.execute(
        "INSERT INTO carried_phone(device_id, source, phone_id, claim, forgotten, private, done, \
           keep_forever, tags) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
         ON CONFLICT(device_id, source, phone_id, claim) DO UPDATE SET \
           forgotten = MAX(forgotten, excluded.forgotten), private = MAX(private, excluded.private), \
           done = MAX(done, excluded.done), keep_forever = MAX(keep_forever, excluded.keep_forever), \
           tags = excluded.tags",
        params![
            device,
            source,
            phone_id,
            claim,
            forgotten,
            marks.private,
            marks.done,
            marks.keep_forever,
            marks.tags_json()
        ],
    )?;
    Ok(())
}

/// A new version of a calendar entry takes what was carried for it: its forgotten claims, so no
/// reading says them, and its marks.
pub(crate) fn take_for_phone(
    tx: &Transaction<'_>,
    device: &str,
    source: &str,
    phone_id: &str,
    item_id: &str,
) -> Result<()> {
    tx.execute(
        "INSERT OR IGNORE INTO forgotten_claims(item_id, claim) SELECT ?4, claim FROM carried_phone \
         WHERE device_id = ?1 AND source = ?2 AND phone_id = ?3 AND forgotten = 1",
        params![device, source, phone_id, item_id],
    )?;
    let mut stmt = tx.prepare(
        "SELECT claim, private, done, keep_forever, tags FROM carried_phone \
         WHERE device_id = ?1 AND source = ?2 AND phone_id = ?3 AND forgotten = 0",
    )?;
    let rows: Vec<(String, Marks)> = stmt
        .query_map(params![device, source, phone_id], |r| {
            Ok((
                r.get(0)?,
                Marks {
                    private: r.get(1)?,
                    done: r.get(2)?,
                    keep_forever: r.get(3)?,
                    tags: tags_of(&r.get::<_, String>(4)?),
                },
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    for (claim, marks) in rows {
        keep(tx, item_id, &claim, &marks)?;
    }
    Ok(())
}

/// A memory read from an item: its id, its claim's parts, whether it is private, its tags.
type Read = (String, String, Option<String>, String, String, bool, String);

/// Every memory read from item `item_id` whose claim carries marks takes them. Marks are only
/// ever added here, never taken away. Answers cached audio to shred once committed.
pub(crate) fn apply(tx: &Transaction<'_>, item_id: &str) -> Result<Vec<PathBuf>> {
    let marks: HashMap<String, Marks> = {
        let mut stmt = tx.prepare(
            "SELECT claim, private, done, keep_forever, tags FROM carried_marks WHERE item_id = ?1",
        )?;
        stmt.query_map([item_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Marks {
                    private: r.get(1)?,
                    done: r.get(2)?,
                    keep_forever: r.get(3)?,
                    tags: tags_of(&r.get::<_, String>(4)?),
                },
            ))
        })?
        .collect::<rusqlite::Result<_>>()?
    };
    if marks.is_empty() {
        return Ok(Vec::new());
    }
    let memories: Vec<Read> = {
        let mut stmt = tx.prepare(
            "SELECT m.id, m.subject_key, m.moved_from_key, m.predicate, m.object_key, m.private, \
             m.tags FROM memories m JOIN memory_sources s ON s.memory_id = m.id \
             WHERE s.item_id = ?1 AND m.forgetting = 0",
        )?;
        stmt.query_map([item_id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?
    };
    let mut files = Vec::new();
    for (id, subject, moved_from, predicate, object, private, tags) in memories {
        let mut found = Marks::default();
        for claim in claims_of(&subject, moved_from.as_deref(), &predicate, &object) {
            if let Some(m) = marks.get(&claim) {
                found.join(m);
            }
        }
        if !found.any() {
            continue;
        }
        if found.private && !private {
            files.extend(crate::lifecycle::set_private_tx(tx, &id, true)?);
        }
        let mut all = Marks {
            tags: tags_of(&tags),
            ..Marks::default()
        };
        all.join(&Marks {
            tags: found.tags.clone(),
            ..Marks::default()
        });
        tx.execute(
            "UPDATE memories SET done = MAX(done, ?2), keep_forever = MAX(keep_forever, ?3), \
             tags = ?4 WHERE id = ?1",
            params![id, found.done, found.keep_forever, all.tags_json()],
        )?;
    }
    Ok(files)
}
