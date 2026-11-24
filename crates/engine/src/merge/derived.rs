//! What a backup brings beside its things: what the phone's mind worked out from them, as it was
//! (`docs/design/phone-backup.md`, "Onto a phone that has a memory"). Each memory comes with links
//! to the things it was read from, the entity it is filed under, the names the person taught and
//! its vector; what the person confirmed or settled comes with it. Pages and their questions come
//! for days this store has no page for, and reflections and recaps for periods it has none of and
//! did not decline. Nothing here is read or sent online again for any of it.
//!
//! A memory on both (the same id, or the same claim read from a thing on both) is kept once, with
//! the person's marks from either side. A memory whose claim the person forgot from a thing here,
//! or whose things all stayed behind, does not come, and a line of a page or a reflection that
//! cites what did not come comes blank, as a forget leaves it.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use rusqlite::types::Value as Sql;
use rusqlite::{Connection, OptionalExtension, Params, Transaction, params, params_from_iter};

use crate::carried::{self, Marks};
use crate::error::Result;
use crate::util::new_id;

/// The things that came, by their id in the backup: the id each has here, and those added.
pub(crate) struct Placed {
    pub items: HashMap<String, String>,
    pub added: HashSet<String>,
}

/// A table read from the backup, with the columns both stores share.
struct Table {
    name: &'static str,
    cols: Vec<String>,
}

impl Table {
    fn new(tx: &Transaction<'_>, phone: &Connection, name: &'static str) -> Result<Table> {
        let here = super::columns(tx, "main", name)?;
        let cols = super::columns(phone, "main", name)?
            .into_iter()
            .filter(|c| here.contains(c))
            .collect();
        Ok(Table { name, cols })
    }

    fn without(mut self, col: &str) -> Table {
        self.cols.retain(|c| c != col);
        self
    }

    fn at(&self, col: &str) -> Option<usize> {
        self.cols.iter().position(|c| c == col)
    }

    fn text(&self, row: &[Sql], col: &str) -> Option<String> {
        self.at(col).and_then(|i| super::text(&row[i]))
    }

    fn flag(&self, row: &[Sql], col: &str) -> bool {
        self.at(col)
            .is_some_and(|i| matches!(row[i], Sql::Integer(n) if n != 0))
    }

    fn set(&self, row: &mut [Sql], col: &str, value: Sql) {
        if let Some(i) = self.at(col) {
            row[i] = value;
        }
    }

    fn list(&self) -> String {
        self.cols
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn rows(&self, phone: &Connection, tail: &str, args: impl Params) -> Result<Vec<Vec<Sql>>> {
        let mut stmt =
            phone.prepare_cached(&format!("SELECT {} FROM {} {tail}", self.list(), self.name))?;
        let width = self.cols.len();
        let rows = stmt.query_map(args, |r| {
            (0..width)
                .map(|i| r.get::<_, Sql>(i))
                .collect::<rusqlite::Result<Vec<Sql>>>()
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Insert `row`, unless a row with the same unique key is here already.
    fn insert(&self, tx: &Transaction<'_>, row: &[Sql]) -> Result<usize> {
        let marks = (1..=self.cols.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(tx.execute(
            &format!(
                "INSERT OR IGNORE INTO {}({}) VALUES ({marks})",
                self.name,
                self.list()
            ),
            params_from_iter(row.iter()),
        )?)
    }
}

fn exists(tx: &Transaction<'_>, sql: &str, args: impl Params) -> Result<bool> {
    Ok(tx.query_row(&format!("SELECT EXISTS({sql})"), args, |r| r.get(0))?)
}

/// An id the backup gave something, kept unless it is taken here.
fn free_id(tx: &Transaction<'_>, table: &str, id: &str, prefix: &str) -> Result<String> {
    let taken = exists(tx, &format!("SELECT 1 FROM {table} WHERE id = ?1"), [id])?;
    Ok(if taken {
        new_id(prefix)
    } else {
        id.to_string()
    })
}

/// `text` with every id the backup gave something that has another id here replaced by that id.
/// Ids are a short prefix, `_` and 16 hex digits, so they are found by their shape.
fn remap(text: &str, ids: &HashMap<String, String>) -> String {
    if ids.is_empty() {
        return text.to_string();
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut copied = 0;
    while i < bytes.len() {
        let at_word = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        if at_word
            && i + 20 <= bytes.len()
            && bytes[i..i + 3].iter().all(u8::is_ascii_lowercase)
            && bytes[i + 3] == b'_'
            && bytes[i + 4..i + 20].iter().all(u8::is_ascii_hexdigit)
            && bytes.get(i + 20).is_none_or(|b| !b.is_ascii_alphanumeric())
            && let Some(to) = ids.get(&text[i..i + 20])
        {
            out.push_str(&text[copied..i]);
            out.push_str(to);
            i += 20;
            copied = i;
            continue;
        }
        i += 1;
    }
    out.push_str(&text[copied..]);
    out
}

/// The entities memories and mentions are filed under, brought as they are needed: one with the
/// same key here is the same entity, and the names the person taught it come with it.
struct Entities {
    table: Table,
    names: Table,
    rows: HashMap<String, Vec<Sql>>,
    map: HashMap<String, String>,
}

impl Entities {
    fn load(tx: &Transaction<'_>, phone: &Connection) -> Result<Entities> {
        let table = Table::new(tx, phone, "entities")?;
        let names = Table::new(tx, phone, "entity_names")?.without("id");
        let id = table.at("id").expect("entities have ids");
        let rows = table
            .rows(phone, "", [])?
            .into_iter()
            .filter_map(|r| super::text(&r[id]).map(|i| (i, r)))
            .collect();
        Ok(Entities {
            table,
            names,
            rows,
            map: HashMap::new(),
        })
    }

    /// The id here of the backup's entity `id`, bringing it in when it is not here yet.
    fn here(
        &mut self,
        tx: &Transaction<'_>,
        phone: &Connection,
        id: &str,
    ) -> Result<Option<String>> {
        if let Some(here) = self.map.get(id) {
            return Ok(Some(here.clone()));
        }
        let Some(mut row) = self.rows.get(id).cloned() else {
            return Ok(None);
        };
        let key = self.table.text(&row, "key").unwrap_or_default();
        let same: Option<String> = tx
            .query_row("SELECT id FROM entities WHERE key = ?1", [&key], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(here) = same {
            self.map.insert(id.to_string(), here.clone());
            self.bring_names(tx, phone, id, &here)?;
            return Ok(Some(here));
        }
        let here = free_id(tx, "entities", id, "ent")?;
        self.map.insert(id.to_string(), here.clone());
        let merged = self.table.text(&row, "merged_into");
        self.table.set(&mut row, "id", Sql::Text(here.clone()));
        self.table.set(&mut row, "merged_into", Sql::Null);
        self.table.insert(tx, &row)?;
        if let Some(into) = merged
            && let Some(into) = self.here(tx, phone, &into)?
            && into != here
        {
            tx.execute(
                "UPDATE entities SET merged_into = ?2 WHERE id = ?1",
                params![here, into],
            )?;
        }
        self.bring_names(tx, phone, id, &here)?;
        Ok(Some(here))
    }

    fn bring_names(
        &self,
        tx: &Transaction<'_>,
        phone: &Connection,
        from: &str,
        to: &str,
    ) -> Result<()> {
        for mut row in self
            .names
            .rows(phone, "WHERE entity_id = ?1 ORDER BY id", [from])?
        {
            let key = self.names.text(&row, "key").unwrap_or_default();
            if exists(
                tx,
                "SELECT 1 FROM entity_names WHERE entity_id = ?1 AND key = ?2",
                params![to, key],
            )? {
                continue;
            }
            self.names
                .set(&mut row, "entity_id", Sql::Text(to.to_string()));
            // The change that set it is this store's own log, which does not come.
            self.names.set(&mut row, "change_id", Sql::Null);
            self.names.insert(tx, &row)?;
        }
        Ok(())
    }
}

/// The memories that came, by their id in the backup, and those that did not.
#[derive(Default)]
struct Memories {
    map: HashMap<String, String>,
    dropped: HashSet<String>,
}

/// Whether the person forgot any of `claims` from a thing here.
fn forgotten_here(
    tx: &Transaction<'_>,
    items: &[(String, Sql)],
    claims: &[String],
) -> Result<bool> {
    for (item, _) in items {
        for claim in claims {
            if exists(
                tx,
                "SELECT 1 FROM forgotten_claims WHERE item_id = ?1 AND claim = ?2",
                params![item, claim],
            )? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// The memory here that says the same as one arriving: the current one with the same claim,
/// read from one of the same things when the arriving one names any.
fn same_claim(
    tx: &Transaction<'_>,
    claim: (&str, &str, &str),
    items: &[(String, Sql)],
) -> Result<Option<String>> {
    let (subject, predicate, object) = claim;
    let current = "m.subject_key = ?1 AND m.predicate = ?2 AND m.object_key = ?3 \
                   AND m.superseded_at IS NULL AND m.forgetting = 0";
    if items.is_empty() {
        return Ok(tx
            .query_row(
                &format!("SELECT m.id FROM memories m WHERE {current} LIMIT 1"),
                params![subject, predicate, object],
                |r| r.get(0),
            )
            .optional()?);
    }
    for (item, _) in items {
        let found: Option<String> = tx
            .query_row(
                &format!(
                    "SELECT m.id FROM memories m JOIN memory_sources s ON s.memory_id = m.id \
                     WHERE {current} AND s.item_id = ?4 LIMIT 1"
                ),
                params![subject, predicate, object, item],
                |r| r.get(0),
            )
            .optional()?;
        if found.is_some() {
            return Ok(found);
        }
    }
    Ok(None)
}

/// A memory on both keeps the person's marks from either side. Answers cached audio to shred
/// once committed.
fn join_marks(
    tx: &Transaction<'_>,
    here: &str,
    marks: &Marks,
    confirmed: bool,
) -> Result<Vec<PathBuf>> {
    let (private, tags): (bool, String) = tx.query_row(
        "SELECT private, tags FROM memories WHERE id = ?1",
        [here],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut files = Vec::new();
    if marks.private && !private {
        files.extend(crate::lifecycle::set_private_tx(tx, here, true)?);
    }
    let mut all: Vec<String> = serde_json::from_str(&tags).unwrap_or_default();
    for t in &marks.tags {
        if !all.contains(t) {
            all.push(t.clone());
        }
    }
    tx.execute(
        "UPDATE memories SET done = MAX(done, ?2), keep_forever = MAX(keep_forever, ?3), \
         confirmed = MAX(confirmed, ?4), tags = ?5 WHERE id = ?1",
        params![
            here,
            marks.done,
            marks.keep_forever,
            confirmed,
            serde_json::to_string(&all)?
        ],
    )?;
    Ok(files)
}

fn link(tx: &Transaction<'_>, memory: &str, items: &[(String, Sql)]) -> Result<()> {
    for (item, span) in items {
        tx.execute(
            "INSERT OR IGNORE INTO memory_sources(memory_id, item_id, span) VALUES (?1, ?2, ?3)",
            params![memory, item, span],
        )?;
    }
    Ok(())
}

fn bring_memories(
    tx: &Transaction<'_>,
    phone: &Connection,
    placed: &Placed,
    entities: &mut Entities,
    files: &mut Vec<PathBuf>,
) -> Result<Memories> {
    let table = Table::new(tx, phone, "memories")?;
    let vectors = Table::new(tx, phone, "embeddings")?;
    let mut sources: HashMap<String, Vec<(String, Sql)>> = HashMap::new();
    {
        let mut stmt = phone.prepare("SELECT memory_id, item_id, span FROM memory_sources")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Sql>(2)?,
            ))
        })?;
        for row in rows {
            let (memory, item, span) = row?;
            sources.entry(memory).or_default().push((item, span));
        }
    }
    let mut out = Memories::default();
    let mut added = Vec::new();
    for mut row in table.rows(phone, "WHERE forgetting = 0 ORDER BY rowid", [])? {
        let Some(id) = table.text(&row, "id") else {
            continue;
        };
        let had = sources.remove(&id).unwrap_or_default();
        let items: Vec<(String, Sql)> = had
            .iter()
            .filter_map(|(item, span)| placed.items.get(item).map(|h| (h.clone(), span.clone())))
            .collect();
        if !had.is_empty() && items.is_empty() {
            out.dropped.insert(id);
            continue;
        }
        let subject = table.text(&row, "subject_key").unwrap_or_default();
        let predicate = table.text(&row, "predicate").unwrap_or_default();
        let object = table.text(&row, "object_key").unwrap_or_default();
        let moved = table.text(&row, "moved_from_key");
        let claims = carried::claims_of(&subject, moved.as_deref(), &predicate, &object);
        if forgotten_here(tx, &items, &claims)? {
            out.dropped.insert(id);
            continue;
        }
        let marks = Marks {
            private: table.flag(&row, "private"),
            done: table.flag(&row, "done"),
            keep_forever: table.flag(&row, "keep_forever"),
            tags: table
                .text(&row, "tags")
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default(),
        };
        let current = table
            .at("superseded_at")
            .is_some_and(|i| matches!(row[i], Sql::Null));
        let here = if exists(tx, "SELECT 1 FROM memories WHERE id = ?1", [&id])? {
            Some(id.clone())
        } else if current {
            same_claim(tx, (&subject, &predicate, &object), &items)?
        } else {
            None
        };
        if let Some(here) = here {
            files.extend(join_marks(
                tx,
                &here,
                &marks,
                table.flag(&row, "confirmed"),
            )?);
            link(tx, &here, &items)?;
            out.map.insert(id, here);
            continue;
        }
        // Read only from things that were here already: what this store's own reading no longer
        // says (a later read let it go) does not come back from an earlier night.
        if !had.is_empty() && !had.iter().any(|(item, _)| placed.added.contains(item)) {
            out.dropped.insert(id);
            continue;
        }
        if let Some(entity) = table.text(&row, "entity_id") {
            let here = entities.here(tx, phone, &entity)?;
            table.set(&mut row, "entity_id", here.map_or(Sql::Null, Sql::Text));
        }
        // What was read from a private thing here is private too.
        for (item, _) in &items {
            if exists(
                tx,
                "SELECT 1 FROM items WHERE id = ?1 AND private = 1",
                [item],
            )? {
                table.set(&mut row, "private", Sql::Integer(1));
            }
        }
        table.insert(tx, &row)?;
        link(tx, &id, &items)?;
        for vector in vectors.rows(phone, "WHERE memory_id = ?1", [&id])? {
            vectors.insert(tx, &vector)?;
        }
        out.map.insert(id.clone(), id.clone());
        added.push(id);
    }
    // A memory that replaced another names it by its id here.
    for id in added {
        let by: Option<String> = tx.query_row(
            "SELECT superseded_by FROM memories WHERE id = ?1",
            [&id],
            |r| r.get(0),
        )?;
        if let Some(to) = by.and_then(|b| out.map.get(&b)).filter(|to| **to != id) {
            tx.execute(
                "UPDATE memories SET superseded_by = ?2 WHERE id = ?1",
                params![id, to],
            )?;
        }
    }
    Ok(out)
}

/// "Which is right?" between two memories that both came, as asked or as settled.
fn bring_conflicts(tx: &Transaction<'_>, phone: &Connection, memories: &Memories) -> Result<()> {
    let table = Table::new(tx, phone, "conflicts")?;
    for mut row in table.rows(phone, "ORDER BY rowid", [])? {
        let (Some(id), Some(a), Some(b)) = (
            table.text(&row, "id"),
            table
                .text(&row, "a")
                .and_then(|a| memories.map.get(&a).cloned()),
            table
                .text(&row, "b")
                .and_then(|b| memories.map.get(&b).cloned()),
        ) else {
            continue;
        };
        if a == b
            || exists(tx, "SELECT 1 FROM conflicts WHERE id = ?1", [&id])?
            || exists(
                tx,
                "SELECT 1 FROM conflicts WHERE (a = ?1 AND b = ?2) OR (a = ?2 AND b = ?1)",
                params![a, b],
            )?
        {
            continue;
        }
        let kept = table
            .text(&row, "kept")
            .and_then(|k| memories.map.get(&k).cloned());
        table.set(&mut row, "a", Sql::Text(a));
        table.set(&mut row, "b", Sql::Text(b));
        table.set(&mut row, "kept", kept.map_or(Sql::Null, Sql::Text));
        table.insert(tx, &row)?;
    }
    Ok(())
}

/// The names found in each thing that was added, and its "Which {name} is this?" questions.
fn bring_mentions(
    tx: &Transaction<'_>,
    phone: &Connection,
    placed: &Placed,
    entities: &mut Entities,
) -> Result<()> {
    let mentions = Table::new(tx, phone, "mentions")?;
    for old in &placed.added {
        let item = &placed.items[old];
        for mut row in mentions.rows(phone, "WHERE item_id = ?1 ORDER BY ord", [old])? {
            let Some(entity) = mentions
                .text(&row, "entity_id")
                .map(|e| entities.here(tx, phone, &e))
                .transpose()?
                .flatten()
            else {
                continue;
            };
            let id = free_id(
                tx,
                "mentions",
                &mentions.text(&row, "id").unwrap_or_default(),
                "men",
            )?;
            mentions.set(&mut row, "id", Sql::Text(id));
            mentions.set(&mut row, "item_id", Sql::Text(item.clone()));
            mentions.set(&mut row, "entity_id", Sql::Text(entity));
            mentions.insert(tx, &row)?;
        }
    }
    Ok(())
}

fn bring_name_questions(
    tx: &Transaction<'_>,
    phone: &Connection,
    placed: &Placed,
    ids: &HashMap<String, String>,
) -> Result<()> {
    let table = Table::new(tx, phone, "name_questions")?;
    for old in &placed.added {
        for mut row in table.rows(phone, "WHERE item_id = ?1 ORDER BY rowid", [old])? {
            let id = free_id(
                tx,
                "name_questions",
                &table.text(&row, "id").unwrap_or_default(),
                "snq",
            )?;
            table.set(&mut row, "id", Sql::Text(id));
            table.set(&mut row, "item_id", Sql::Text(placed.items[old].clone()));
            for col in ["options", "candidates"] {
                if let Some(t) = table.text(&row, col) {
                    table.set(&mut row, col, Sql::Text(remap(&t, ids)));
                }
            }
            table.insert(tx, &row)?;
        }
    }
    Ok(())
}

/// Pages for the days this store has none for, with their lines and questions. A line that
/// cites a memory or a thing that did not come is blank, as a forget leaves it.
fn bring_pages(
    tx: &Transaction<'_>,
    phone: &Connection,
    memories: &Memories,
    ids: &HashMap<String, String>,
    stayed: &[String],
) -> Result<()> {
    let pages = Table::new(tx, phone, "pages")?;
    let lines = Table::new(tx, phone, "page_lines")?;
    let questions = Table::new(tx, phone, "questions")?;
    for mut page in pages.rows(phone, "ORDER BY date", [])? {
        let Some(date) = pages.text(&page, "date") else {
            continue;
        };
        if exists(tx, "SELECT 1 FROM pages WHERE date = ?1", [&date])? {
            continue;
        }
        let mut renamed = HashMap::new();
        let mut page_lines = Vec::new();
        for mut line in lines.rows(phone, "WHERE page_date = ?1 ORDER BY ord", [&date])? {
            let old = lines.text(&line, "id").unwrap_or_default();
            let id = free_id(tx, "page_lines", &old, "pln")?;
            if id != old {
                renamed.insert(old.clone(), id.clone());
            }
            lines.set(&mut line, "id", Sql::Text(id.clone()));
            page_lines.push((old, id, line));
        }
        if let Some(body) = pages.text(&page, "body") {
            let body = remap(&remap(&body, ids), &renamed);
            pages.set(&mut page, "body", Sql::Text(body));
        }
        pages.insert(tx, &page)?;
        let mut cites_gone: Vec<String> = Vec::new();
        for (old, id, line) in page_lines {
            lines.insert(tx, &line)?;
            let mut stmt = phone
                .prepare_cached("SELECT memory_id FROM page_line_memories WHERE line_id = ?1")?;
            let cited: Vec<String> = stmt
                .query_map([&old], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for memory in cited {
                match memories.map.get(&memory) {
                    Some(here) => {
                        tx.execute(
                            "INSERT OR IGNORE INTO page_line_memories(line_id, memory_id) \
                             VALUES (?1, ?2)",
                            params![id, here],
                        )?;
                    }
                    None => cites_gone.push(memory),
                }
            }
        }
        for mut q in questions.rows(phone, "WHERE page_date = ?1", [&date])? {
            if let Some(m) = questions.text(&q, "memory_id") {
                match memories.map.get(&m) {
                    Some(here) => questions.set(&mut q, "memory_id", Sql::Text(here.clone())),
                    None => {
                        cites_gone.push(m);
                        continue;
                    }
                }
            }
            if let Some(j) = questions.text(&q, "journal") {
                questions.set(&mut q, "journal", Sql::Text(remap(&j, ids)));
            }
            questions.insert(tx, &q)?;
        }
        let body: String =
            tx.query_row("SELECT body FROM pages WHERE date = ?1", [&date], |r| {
                r.get(0)
            })?;
        let gone: Vec<String> = memories
            .dropped
            .iter()
            .filter(|m| body.contains(m.as_str()))
            .cloned()
            .chain(cites_gone)
            .collect();
        let dates = [date];
        if !gone.is_empty() {
            crate::forget::redact_pages(tx, &gone, &dates, None)?;
        }
        for item in stayed.iter().filter(|i| body.contains(i.as_str())) {
            crate::forget::redact_pages(tx, &[], &dates, Some(item))?;
        }
    }
    Ok(())
}

/// Reflections and recaps for periods this store has none of and the person did not delete
/// here. A line that cites a thing that did not come is taken out, as a forget does.
fn bring_reflections(
    tx: &Transaction<'_>,
    phone: &Connection,
    placed: &Placed,
    ids: &HashMap<String, String>,
    stayed: &[String],
) -> Result<()> {
    let table = Table::new(tx, phone, "reflections")?;
    for mut row in table.rows(phone, "ORDER BY rowid", [])? {
        let (Some(old), Some(kind), Some(start)) = (
            table.text(&row, "id"),
            table.text(&row, "kind"),
            table.text(&row, "start"),
        ) else {
            continue;
        };
        if exists(
            tx,
            "SELECT 1 FROM reflections WHERE kind = ?1 AND start = ?2 \
             UNION SELECT 1 FROM reflections_declined WHERE kind = ?1 AND start = ?2",
            params![kind, start],
        )? {
            continue;
        }
        let id = free_id(tx, "reflections", &old, "rfl")?;
        table.set(&mut row, "id", Sql::Text(id.clone()));
        if let Some(body) = table.text(&row, "body") {
            table.set(&mut row, "body", Sql::Text(remap(&body, ids)));
        }
        table.insert(tx, &row)?;
        let mut stmt = phone
            .prepare_cached("SELECT item_id FROM reflection_cites WHERE reflection_id = ?1")?;
        let cites: Vec<String> = stmt
            .query_map([&old], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for item in cites {
            let here = placed.items.get(&item).cloned().unwrap_or(item);
            tx.execute(
                "INSERT OR IGNORE INTO reflection_cites(reflection_id, item_id) VALUES (?1, ?2)",
                params![id, here],
            )?;
        }
    }
    // A period declined there stays declined here.
    let declined = Table::new(tx, phone, "reflections_declined")?;
    for row in declined.rows(phone, "", [])? {
        declined.insert(tx, &row)?;
    }
    crate::journal::store::scrub_cites(tx, stayed)?;
    Ok(())
}

/// Bring what the backup's mind worked out from the things that came. Answers cached audio to
/// shred once committed.
pub(super) fn bring(
    tx: &Transaction<'_>,
    phone: &Connection,
    placed: &Placed,
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    // Things in the backup that did not come, so nothing brought may cite them.
    let stayed: Vec<String> = {
        let mut stmt = phone.prepare("SELECT id FROM items")?;
        let all: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        all.into_iter()
            .filter(|i| !placed.items.contains_key(i))
            .collect()
    };
    let mut entities = Entities::load(tx, phone)?;
    let memories = bring_memories(tx, phone, placed, &mut entities, &mut files)?;
    bring_conflicts(tx, phone, &memories)?;
    bring_mentions(tx, phone, placed, &mut entities)?;
    let ids: HashMap<String, String> = placed
        .items
        .iter()
        .chain(&memories.map)
        .chain(&entities.map)
        .filter(|(from, to)| from != to)
        .map(|(from, to)| (from.clone(), to.clone()))
        .collect();
    bring_name_questions(tx, phone, placed, &ids)?;
    bring_pages(tx, phone, &memories, &ids, &stayed)?;
    bring_reflections(tx, phone, placed, &ids, &stayed)?;
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_replaced_whole_and_nothing_else_is() {
        let ids = HashMap::from([
            (
                "itm_0123456789abcdef".to_string(),
                "itm_fedcba9876543210".to_string(),
            ),
            (
                "mem_0000000000000001".to_string(),
                "mem_0000000000000002".to_string(),
            ),
        ]);
        assert_eq!(
            remap(
                r#"{"itemId":"itm_0123456789abcdef","m":["mem_0000000000000001"],"x":"xitm_0123456789abcdef","y":"itm_0123456789abcdef0"}"#,
                &ids
            ),
            r#"{"itemId":"itm_fedcba9876543210","m":["mem_0000000000000002"],"x":"xitm_0123456789abcdef","y":"itm_0123456789abcdef0"}"#
        );
        assert_eq!(
            remap("café itm_0123456789abcdef", &ids),
            "café itm_fedcba9876543210"
        );
    }
}
