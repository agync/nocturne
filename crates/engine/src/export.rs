//! Export and delete everything: the person's own data, out of the engine or gone.

use std::fmt::Write as _;
use std::path::Path;

use nocturne_contract::{PageLine, PageView};
use rusqlite::Connection;
use serde_json::{Value, json};

use crate::Engine;
use crate::error::{Error, Result};
use crate::fsutil;
use crate::page::load_page;
use crate::rows::{MEM_COLS, MemRow, mem_row};
use crate::store;
use crate::types::ExportReport;
use crate::util::fmt_ts;

/// The exact phrase `delete_everything` requires.
pub const DELETE_PHRASE: &str = "delete everything";

const README: &str = "This is your Nocturne export: everything Nocturne holds, in plain files.\n\
\n\
memories.md        one line per memory, with where it came from\n\
pages/             one file per morning page, named by date\n\
originals/         copies of the photos, recordings and files you kept\n\
nocturne.json      all of it as data, including earlier versions of memories\n\
\n\
Memories you marked private are included here, because this is your own copy. They are marked.\n\
Anything you asked Nocturne to forget completely is not here.\n";

/// `id, sha256, ext, file_name, origin, metadata` of an item.
type ItemRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

fn marks(m: &MemRow) -> String {
    let mut out = String::new();
    if m.private {
        out.push_str(" [private]");
    }
    match m.status {
        nocturne_contract::MemoryStatus::Unconfirmed => out.push_str(" [unconfirmed]"),
        nocturne_contract::MemoryStatus::LetGo => out.push_str(" [let go]"),
        nocturne_contract::MemoryStatus::Fading => out.push_str(" [fading]"),
        nocturne_contract::MemoryStatus::Active => {}
    }
    out
}

fn memory_json(conn: &Connection, m: &MemRow) -> Result<Value> {
    let mut stmt = conn.prepare_cached(
        "SELECT item_id, span FROM memory_sources WHERE memory_id = ?1 ORDER BY item_id",
    )?;
    let sources: Vec<Value> = stmt
        .query_map([&m.id], |r| {
            let span: Option<String> = r.get(1)?;
            Ok(json!({
                "itemId": r.get::<_, String>(0)?,
                "span": span.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
            }))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(json!({
        "id": m.id, "subject": m.subject, "predicate": m.predicate, "object": m.object,
        "sentence": m.sentence, "validFrom": m.valid_from, "validTo": m.valid_to, "due": m.due,
        "recordedAt": m.recorded_at, "supersededAt": m.superseded_at,
        "supersededBy": m.superseded_by, "confidence": m.confidence, "status": m.status,
        "confirmed": m.confirmed, "lastUsed": m.last_used, "letGoAt": m.let_go_at,
        "done": m.done, "keepForever": m.keep_forever, "private": m.private,
        "tags": m.tags, "entityId": m.entity_id, "sources": sources,
    }))
}

fn line_md(out: &mut String, conn: &Connection, line: &PageLine) -> Result<()> {
    if line.forgotten {
        return Ok(());
    }
    let mut private = false;
    for m in &line.memory_ids {
        private |= conn
            .query_row("SELECT private FROM memories WHERE id = ?1", [m], |r| {
                r.get::<_, bool>(0)
            })
            .unwrap_or(false);
    }
    let from: Vec<String> = line
        .receipts
        .iter()
        .map(|r| format!("{} ({})", r.label, &r.kept_at[..10.min(r.kept_at.len())]))
        .collect();
    let _ = write!(out, "- {}", line.text);
    if private {
        out.push_str(" [private]");
    }
    if !from.is_empty() {
        let _ = write!(out, " | from: {}", from.join("; "));
    }
    out.push('\n');
    Ok(())
}

fn page_md(conn: &Connection, p: &PageView) -> Result<String> {
    let mut out = format!("# {}\n\n{}\n\n", p.headline, p.dek);
    let sections: [(&str, &Vec<PageLine>); 5] = [
        ("Needs you", &p.needs_you),
        ("What changed", &p.changed),
        ("Learned", &p.learned),
        ("Coming up", &p.upcoming),
        ("Let go", &p.let_go),
    ];
    for (title, lines) in sections {
        if lines.iter().all(|l| l.forgotten) {
            continue;
        }
        let _ = writeln!(out, "## {title}\n");
        for l in lines {
            line_md(&mut out, conn, l)?;
        }
        out.push('\n');
    }
    if let Some(q) = &p.question {
        let _ = writeln!(out, "## Question\n\n- {}\n", q.text);
    }
    Ok(out)
}

impl Engine {
    /// Write everything to `dir`: `README.txt`, `memories.md`, `pages/`, `originals/` and
    /// `nocturne.json`. Private memories are included and marked: it is the person's own copy.
    pub fn export_to(&self, dir: &Path) -> Result<ExportReport> {
        let now = fmt_ts(self.clock().now());
        let originals_src = self.originals_dir();
        std::fs::create_dir_all(dir.join("pages"))?;
        std::fs::create_dir_all(dir.join("originals"))?;
        let report = self.read(|conn| {
            // Memories: current ones in the markdown, every version in the data file.
            let all: Vec<MemRow> = {
                let sql = format!(
                    "SELECT {MEM_COLS} FROM memories m WHERE m.forgetting = 0 \
                     ORDER BY m.recorded_at, m.id"
                );
                let mut stmt = conn.prepare(&sql)?;
                stmt.query_map([], mem_row)?.collect::<rusqlite::Result<_>>()?
            };
            let mut md = String::from("# What Nocturne remembers\n\n");
            let mut current = 0u32;
            for m in all.iter().filter(|m| m.superseded_at.is_none()) {
                let receipts = crate::rows::receipts_for(conn, &m.id)?;
                let from: Vec<String> = receipts
                    .iter()
                    .map(|r| format!("{} ({})", r.label, &r.kept_at[..10.min(r.kept_at.len())]))
                    .collect();
                let _ = writeln!(md, "- {}{} | sources: {}", m.sentence, marks(m), from.join("; "));
                current += 1;
            }
            std::fs::write(dir.join("memories.md"), md)?;
            std::fs::write(dir.join("README.txt"), README)?;

            // Pages.
            let dates: Vec<String> = {
                let mut stmt = conn.prepare("SELECT date FROM pages ORDER BY date")?;
                stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
            };
            let mut pages_json = Vec::new();
            for d in &dates {
                let day = d.parse()?;
                if let Some(p) = load_page(conn, Some(day))? {
                    std::fs::write(dir.join("pages").join(format!("{d}.md")), page_md(conn, &p)?)?;
                    pages_json.push(serde_json::to_value(&p)?);
                }
            }

            // Originals: a copy of every kept file.
            let mut copied = 0u32;
            let mut items_json = Vec::new();
            let mut stmt = conn.prepare(
                "SELECT id, sha256, ext, file_name, origin, metadata FROM items ORDER BY kept_at, id",
            )?;
            let rows: Vec<ItemRow> = stmt
                .query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
                })?
                .collect::<rusqlite::Result<_>>()?;
            let views = {
                // `items` goes through the reader pool on its own; reuse the connection here.
                let mut stmt = conn.prepare(&format!(
                    "SELECT {} FROM items i ORDER BY i.kept_at, i.id",
                    crate::rows::ITEM_COLS
                ))?;
                stmt.query_map([], crate::rows::item_view)?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            };
            for ((id, sha, ext, file_name, origin, metadata), view) in rows.into_iter().zip(views) {
                let mut copy_name = None;
                if let Some(name) = &file_name {
                    let src = crate::capture::original_path(&originals_src, &sha, ext.as_deref());
                    let safe: String = name
                        .chars()
                        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
                        .collect();
                    let target = format!("{id}-{safe}");
                    if src.exists() {
                        std::fs::copy(&src, dir.join("originals").join(&target))?;
                        copied += 1;
                        copy_name = Some(format!("originals/{target}"));
                    }
                }
                let mut v = serde_json::to_value(&view)?;
                v["sha256"] = json!(sha);
                v["origin"] = json!(origin);
                v["export"] = json!(copy_name);
                v["metadata"] = serde_json::from_str(&metadata).unwrap_or(Value::Null);
                items_json.push(v);
            }

            let memories_json: Vec<Value> =
                all.iter().map(|m| memory_json(conn, m)).collect::<Result<_>>()?;
            let entities: Vec<Value> = {
                let mut stmt = conn.prepare("SELECT id, kind, name, role FROM entities ORDER BY id")?;
                stmt.query_map([], |r| {
                    Ok(json!({"id": r.get::<_, String>(0)?, "kind": r.get::<_, String>(1)?,
                              "name": r.get::<_, String>(2)?, "role": r.get::<_, Option<String>>(3)?}))
                })?
                .collect::<rusqlite::Result<_>>()?
            };
            let conflicts: Vec<Value> = {
                let mut stmt = conn.prepare(
                    "SELECT id, a, b, created_at, resolved_at, kept FROM conflicts ORDER BY id",
                )?;
                stmt.query_map([], |r| {
                    Ok(json!({"id": r.get::<_, String>(0)?, "a": r.get::<_, String>(1)?,
                              "b": r.get::<_, String>(2)?, "createdAt": r.get::<_, String>(3)?,
                              "resolvedAt": r.get::<_, Option<String>>(4)?,
                              "kept": r.get::<_, Option<String>>(5)?}))
                })?
                .collect::<rusqlite::Result<_>>()?
            };
            let settings = crate::settings::read_settings(conn)?;
            let data = json!({
                "schemaVersion": store::schema_version(conn)?,
                "exportedAt": now,
                "items": items_json,
                "memories": memories_json,
                "entities": entities,
                "conflicts": conflicts,
                "pages": pages_json,
                "settings": settings,
            });
            std::fs::write(dir.join("nocturne.json"), serde_json::to_string_pretty(&data)?)?;
            Ok(ExportReport {
                memories: current,
                pages: dates.len() as u32,
                originals: copied,
            })
        })?;
        Ok(report)
    }

    /// Remove everything: every row, every original, every cached audio file. Requires the exact
    /// phrase `delete everything`. The store stays usable and empty.
    pub fn delete_everything(&self, phrase: &str) -> Result<()> {
        if phrase != DELETE_PHRASE {
            return Err(Error::invalid(
                "type the phrase exactly to delete everything",
            ));
        }
        self.write(|tx| {
            for table in [
                "page_line_memories",
                "page_lines",
                "pages",
                "questions",
                "audio_cache",
                "conflicts",
                "embeddings",
                "memory_sources",
                "memories",
                "entities",
                "items",
                "ledger",
                "settings",
                "forget_tokens",
                "forget_jobs",
            ] {
                tx.execute(&format!("DELETE FROM {table}"), [])?;
            }
            tx.execute(
                "INSERT INTO memories_fts(memories_fts) VALUES ('optimize')",
                [],
            )?;
            tx.execute("INSERT INTO items_fts(items_fts) VALUES ('optimize')", [])?;
            Ok(())
        })?;
        for sub in ["originals", "audio"] {
            let path = self.inner.dir.join(sub);
            if let Ok(entries) = std::fs::read_dir(&path) {
                for e in entries.flatten() {
                    fsutil::shred(&e.path());
                }
            }
        }
        self.invalidate_vectors();
        self.write_raw(|conn| {
            conn.execute_batch("VACUUM")?;
            Ok(())
        })?;
        self.checkpoint_truncate()
    }
}
