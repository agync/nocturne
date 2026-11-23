//! Export and delete everything: the person's own data, out of the engine or gone.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::private::Privacy;
use nocturne_contract::{ExportReport, PageLine, PageView};
use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::Engine;
use crate::error::{Error, Result};
use crate::fsutil;
use crate::page::load_page;
use crate::rows::{MEM_COLS, MemRow, mem_row};
use crate::store;
use crate::util::fmt_ts;

/// The exact phrase `delete_everything` requires.
pub const DELETE_PHRASE: &str = "delete everything";

const README: &str = "This is your Nocturne export: everything Nocturne holds, in plain files.\n\
\n\
memories.md        one line per memory, with where it came from\n\
pages/             one file per morning page, named by date\n\
originals/         copies of the photos, recordings and files you kept\n\
nocturne.json      all of it as data, including earlier versions of memories\n\
calendar/          one calendar file for each phone that brought its calendar\n\
health/            one table for each kind of health reading your phones brought\n\
\n\
Memories you marked private are included here, because this is your own copy. They are marked.\n\
Anything you asked Nocturne to forget completely is not here.\n";

/// An item as the export sees it: the view, and the columns the view leaves out.
struct ItemExport {
    view: nocturne_contract::ItemView,
    sha: String,
    ext: Option<String>,
    origin: Option<String>,
    metadata: String,
    left_out: bool,
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut f = fsutil::create_private_file_new(path)?;
    f.write_all(bytes)?;
    Ok(())
}

/// Copy a file into a new one and return the SHA-256 of what was copied.
fn copy_new_hashed(src: &Path, dest: &Path) -> std::io::Result<String> {
    use std::io::{Read, Write};
    let mut input = std::fs::File::open(src)?;
    let mut output = fsutil::create_private_file_new(dest)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        output.write_all(&buf[..n])?;
    }
    Ok(crate::util::hex(&hasher.finalize()))
}

/// Shreds the export folder when dropped, unless the export was committed. Every exit path
/// (an error part way, a panic, a stale generation) leaves nothing behind.
struct ExportGuard {
    root: PathBuf,
    committed: bool,
}

impl ExportGuard {
    fn commit(mut self) -> PathBuf {
        self.committed = true;
        std::mem::take(&mut self.root)
    }
}

impl Drop for ExportGuard {
    fn drop(&mut self) {
        if !self.committed {
            fsutil::shred_tree(&self.root);
        }
    }
}

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
        "tags": m.tags, "entityId": m.entity_id, "movedFrom": m.moved_from, "sources": sources,
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
        let text = match (&q.journal, q.kind) {
            (Some(ask), nocturne_contract::QuestionKind::Journal) => {
                crate::journal::question::question_words(ask, &q.text)
            }
            _ => q.text.clone(),
        };
        let _ = writeln!(out, "## Question\n\n- {text}\n");
    }
    Ok(out)
}

fn nocturne_contract_kind(kind: nocturne_contract::PeriodKind) -> String {
    crate::util::enum_str(&kind)
}

fn ref_label(r: &nocturne_contract::Receipt) -> String {
    format!("{} ({})", r.label, &r.kept_at[..10.min(r.kept_at.len())])
}

/// The name of an item's copy in `originals/`, as seen from the journal folder.
fn beside(copies: &std::collections::HashMap<String, String>, id: &str) -> String {
    copies
        .get(id)
        .map(|c| format!(" (../{c})"))
        .unwrap_or_default()
}

fn feeling_text(f: &nocturne_contract::Feeling) -> String {
    let part = match f.part {
        nocturne_contract::FeelingPart::WoundUp => "wound up",
        nocturne_contract::FeelingPart::LitUp => "lit up",
        nocturne_contract::FeelingPart::Low => "low",
        nocturne_contract::FeelingPart::Settled => "settled",
    };
    let mut out = format!(
        "{part} (pleasant {:.2}, energy {:.2})",
        f.point.pleasant, f.point.energy
    );
    if !f.words.is_empty() {
        let _ = write!(out, ", words: {}", f.words.join(", "));
    }
    out
}

/// A journal day as plain text: the entry in order, check-ins as part, point and words, and
/// the good things.
fn day_text(
    view: &nocturne_contract::JournalDay,
    ctx: &crate::journal::Ctx,
    copies: &std::collections::HashMap<String, String>,
) -> String {
    use nocturne_contract::ItemKind;
    let heading = view
        .day
        .parse::<jiff::civil::Date>()
        .map(|d| d.strftime("%A %-d %B %Y").to_string())
        .unwrap_or_else(|_| view.day.clone());
    let mut out = format!("{heading}\n\n");
    if !view.entry.is_empty() {
        out.push_str("Entry\n\n");
        for b in &view.entry {
            let text = b.item.text.as_deref().unwrap_or("").trim();
            match b.item.kind {
                ItemKind::Audio => {
                    let secs = b.item.duration_ms.unwrap_or(0) / 1000;
                    let _ = writeln!(
                        out,
                        "[Voice note, {}:{:02}]{} {text}",
                        secs / 60,
                        secs % 60,
                        beside(copies, &b.item.id)
                    );
                }
                ItemKind::Image => {
                    let alt = b
                        .alt
                        .as_deref()
                        .or(b.item.summary.as_deref())
                        .unwrap_or("")
                        .trim();
                    let _ = writeln!(out, "[Photo]{} {alt}", beside(copies, &b.item.id));
                }
                _ => {
                    let _ = writeln!(out, "{text}");
                }
            }
            out.push('\n');
        }
    }
    if !view.checkins.is_empty() {
        out.push_str("Check-ins\n\n");
        for c in &view.checkins {
            let at = crate::util::parse_ts(&c.kept_at)
                .map(|t| crate::journal::lookback::clock_label(&ctx.tz, t))
                .unwrap_or_else(|_| c.kept_at.clone());
            let _ = writeln!(out, "{at}  {}", feeling_text(&c.feeling));
            if let Some(l) = &c.line {
                let _ = writeln!(out, "  What was going on: {l}");
            }
            if let Some(u) = &c.urge {
                let _ = writeln!(out, "  Felt like doing: {u}");
            }
        }
        out.push('\n');
    }
    if !view.good.is_empty() {
        out.push_str("Good things\n\n");
        for g in &view.good {
            let photo = g
                .photo
                .as_ref()
                .map(|p| beside(copies, &p.id))
                .unwrap_or_default();
            let _ = writeln!(out, "- {}{photo}", g.text);
        }
        out.push('\n');
    }
    out
}

/// Every stored reflection and recap as plain text, with the period it is for.
fn reflection_rows(conn: &Connection) -> Result<Vec<(nocturne_contract::Period, String)>> {
    use nocturne_contract::{DayReflection, Period, PeriodKind, Recap};
    let mut stmt = conn.prepare(
        "SELECT kind, start, end, written_at, body FROM reflections ORDER BY start, kind",
    )?;
    let rows: Vec<(String, String, String, String, String)> = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut out = Vec::new();
    for (kind, start, end, written_at, body) in rows {
        let kind: PeriodKind = crate::util::parse_enum(&kind)?;
        let period = Period { kind, start, end };
        let mut text = format!(
            "Looked back on {} to {}\nWritten on this computer at {written_at}\n\n",
            period.start, period.end
        );
        if kind == PeriodKind::Day {
            let r: DayReflection = serde_json::from_str(&body)?;
            for l in &r.the_day {
                let from: Vec<String> = l.receipts.iter().map(ref_label).collect();
                let _ = writeln!(
                    text,
                    "- {} | from: {}",
                    l.text.replace("**", ""),
                    from.join("; ")
                );
            }
        } else {
            let r: Recap = serde_json::from_str(&body)?;
            for h in &r.highlights {
                let from: Vec<String> = h.receipts.iter().map(ref_label).collect();
                let _ = writeln!(
                    text,
                    "- {}: {} | from: {}",
                    h.day,
                    h.text.replace("**", ""),
                    from.join("; ")
                );
            }
            if !r.words.is_empty() {
                let words: Vec<String> = r
                    .words
                    .iter()
                    .map(|w| format!("{} ({})", w.word, w.checkin_ids.len()))
                    .collect();
                let _ = writeln!(text, "\nWords you used most: {}", words.join(", "));
            }
            for p in &r.people {
                let _ = writeln!(text, "{}: {} days", p.with.name, p.days.len());
            }
        }
        out.push((period, text));
    }
    Ok(out)
}

/// One query as JSON rows, column by column.
fn rows_json(conn: &Connection, sql: &str) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(sql)?;
    let names: Vec<String> = stmt.column_names().map(|n| n.to_string()).collect();
    let rows = stmt
        .query_map([], |r| {
            let mut o = serde_json::Map::new();
            for (i, name) in names.iter().enumerate() {
                let v = match r.get_ref(i)? {
                    rusqlite::types::ValueRef::Integer(n) => json!(n),
                    rusqlite::types::ValueRef::Real(f) => json!(f),
                    rusqlite::types::ValueRef::Text(t) => {
                        json!(String::from_utf8_lossy(t))
                    }
                    _ => Value::Null,
                };
                o.insert(name.clone(), v);
            }
            Ok(Value::Object(o))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// The journal's own tables as data. Reflection and recap bodies are parsed.
fn journal_json(conn: &Connection) -> Result<Value> {
    let mut reflections = rows_json(conn, "SELECT * FROM reflections ORDER BY start, kind")?;
    for r in &mut reflections {
        if let Some(body) = r["body"]
            .as_str()
            .and_then(|b| serde_json::from_str::<Value>(b).ok())
        {
            r["body"] = body;
        }
    }
    Ok(json!({
        "mentions": rows_json(conn, "SELECT * FROM mentions ORDER BY item_id, ord")?,
        "reflections": reflections,
        "reflectionCites": rows_json(conn, "SELECT * FROM reflection_cites ORDER BY reflection_id, item_id")?,
        "nameQuestions": rows_json(conn, "SELECT * FROM name_questions ORDER BY id")?,
    }))
}

impl Engine {
    /// Write everything into a new folder `Nocturne export YYYY-MM-DD` (with ` (n)` if that name
    /// is taken) inside `dir`, owner-only, and return its path. It contains `README.txt`, `memories.md`, `pages/`, `originals/` and
    /// `nocturne.json`. Private memories are included and marked: it is the person's own copy.
    pub fn export_to(&self, dir: &Path) -> Result<ExportReport> {
        self.export_to_with(dir, &|_, _| {})
    }

    /// Like `export_to`, and calls `progress(done, total)` as files are written: first with
    /// `(0, total)`, then after each file, last with `(total, total)`. `total` is every file the
    /// export will write (the three top-level files, one per page, one per original).
    pub fn export_to_with(&self, dir: &Path, progress: &dyn Fn(u32, u32)) -> Result<ExportReport> {
        let generation = self
            .inner
            .forget_gen
            .load(std::sync::atomic::Ordering::SeqCst);
        let now = fmt_ts(self.clock().now());
        let originals_src = self.originals_dir();
        // A fresh, owner-only folder named for the day; never an existing one.
        let day = self
            .clock()
            .now()
            .to_zoned(self.clock().tz())
            .date()
            .to_string();
        std::fs::create_dir_all(dir)?;
        let mut n = 0u32;
        let mut private = true;
        let root: PathBuf = loop {
            let name = if n == 0 {
                format!("Nocturne export {day}")
            } else {
                format!("Nocturne export {day} ({n})")
            };
            let candidate = dir.join(name);
            match fsutil::create_chosen_dir_new(&candidate) {
                Ok(privacy) => {
                    private &= privacy == Privacy::Private;
                    break candidate;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
                Err(e) => return Err(e.into()),
            }
        };
        let guard = ExportGuard {
            root,
            committed: false,
        };
        let dir = guard.root.as_path();
        for sub in ["pages", "originals", "journal", "journal/reflections"] {
            private &= fsutil::create_chosen_dir_new(&dir.join(sub))? == Privacy::Private;
        }
        let mut report = self.read(|conn| {
            // One snapshot for the whole export: what is written is one consistent moment,
            // however much changes while the files are being copied.
            let snapshot = conn.unchecked_transaction()?;
            let conn: &Connection = &snapshot;
            let dates: Vec<String> = {
                let mut stmt = conn.prepare("SELECT date FROM pages ORDER BY date")?;
                stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
            };
            // Each item once, by id: its view and its stored columns come from one row.
            let rows: Vec<ItemExport> = {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {}, i.origin, i.metadata, i.left_out FROM items i \
                     WHERE {} \
                     ORDER BY i.kept_at, i.id",
                    crate::rows::ITEM_COLS,
                    crate::forget::ORIGINALS_NOT_IN_FORGET
                ))?;
                let to_view = crate::rows::item_view(&originals_src);
                stmt.query_map([], |r| {
                    Ok(ItemExport {
                        view: to_view(r)?,
                        sha: r.get(17)?,
                        ext: r.get(18)?,
                        origin: r.get(crate::rows::ITEM_COL_COUNT)?,
                        metadata: r.get(crate::rows::ITEM_COL_COUNT + 1)?,
                        left_out: r.get(crate::rows::ITEM_COL_COUNT + 2)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?
            };
            let with_files = rows.iter().filter(|r| r.view.file_name.is_some()).count();
            // The journal: one file per day that has an entry, a check-in or a good thing, and
            // one per reflection or recap.
            let jctx = crate::journal::Ctx::load(conn, self.clock())?;
            let mut journal_days = Vec::new();
            for day in crate::journal::day::days_with_items(conn, &jctx)?.into_iter().rev() {
                let view = crate::journal::day::build_day(conn, &jctx, &originals_src, day, false)?;
                if !(view.entry.is_empty() && view.checkins.is_empty() && view.good.is_empty()) {
                    journal_days.push(view);
                }
            }
            let reflection_rows = reflection_rows(conn)?;
            let total = (3 + dates.len() + with_files + journal_days.len() + reflection_rows.len())
                as u32;
            let mut done = 0u32;
            let mut step = || {
                done += 1;
                progress(done, total);
            };
            progress(0, total);

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
            write_new(&dir.join("memories.md"), md.as_bytes())?;
            step();
            write_new(&dir.join("README.txt"), README.as_bytes())?;
            step();

            // Pages.
            let mut pages_json = Vec::new();
            for d in &dates {
                let day = d.parse()?;
                if let Some(p) = load_page(conn, Some(day))? {
                    write_new(&dir.join("pages").join(format!("{d}.md")), page_md(conn, &p)?.as_bytes())?;
                    pages_json.push(serde_json::to_value(&p)?);
                }
                step();
            }

            // Originals: a copy of every kept file.
            let mut copied = 0u32;
            let mut items_json = Vec::new();
            let mut copies: std::collections::HashMap<String, String> = std::collections::HashMap::new();
            // Whether an item is still in the store now, for telling a file that is missing
            // from one that went because the item was undone while the export ran.
            let mut live: Option<Connection> = None;
            let mut still_kept = |id: &str| -> bool {
                if live.is_none() {
                    live = store::open_reader(&self.db_path()).ok();
                }
                // Fails closed: if it cannot be checked, the item is treated as not kept.
                live.as_ref().is_some_and(|c| {
                    c.query_row("SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)", [id], |r| {
                        r.get::<_, bool>(0)
                    })
                    .unwrap_or(false)
                })
            };
            for item in rows {
                let ItemExport { view, sha, ext, origin, metadata, left_out } = item;
                let id = view.id.clone();
                let mut copy_name = None;
                let mut included = true;
                if let Some(name) = &view.file_name {
                    let src = crate::capture::original_path(&originals_src, &sha, ext.as_deref());
                    let safe: String = name
                        .chars()
                        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
                        .collect();
                    let target = format!("{id}-{safe}");
                    let dest = dir.join("originals").join(&target);
                    // A copy counts only if it is the bytes the item was kept with: an original
                    // being removed underneath is overwritten before it is deleted.
                    let intact = match copy_new_hashed(&src, &dest) {
                        Ok(h) if h == sha => true,
                        Ok(_) => {
                            fsutil::shred(&dest);
                            false
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                        Err(e) => return Err(e.into()),
                    };
                    if intact {
                        copied += 1;
                        copy_name = Some(format!("originals/{target}"));
                    } else if !still_kept(&id) {
                        included = false;
                    }
                    step();
                }
                if !included {
                    continue;
                }
                // The export names its own copy; where the store keeps the original is not part
                // of the person's data.
                let mut view = view;
                view.original_path = None;
                let mut v = serde_json::to_value(&view)?;
                v["sha256"] = json!(sha);
                v["origin"] = json!(origin);
                v["export"] = json!(copy_name);
                v["metadata"] = serde_json::from_str(&metadata).unwrap_or(Value::Null);
                v["leftOut"] = json!(left_out);
                if let Some(c) = &copy_name {
                    copies.insert(id.clone(), c.clone());
                }
                items_json.push(v);
            }

            for view in &journal_days {
                write_new(
                    &dir.join("journal").join(format!("{}.txt", view.day)),
                    day_text(view, &jctx, &copies).as_bytes(),
                )?;
                step();
            }
            for (period, text) in &reflection_rows {
                write_new(
                    &dir.join("journal").join("reflections").join(format!(
                        "{}-{}.txt",
                        nocturne_contract_kind(period.kind),
                        period.start
                    )),
                    text.as_bytes(),
                )?;
                step();
            }
            let journal = journal_json(conn)?;
            let memories_json: Vec<Value> =
                all.iter().map(|m| memory_json(conn, m)).collect::<Result<_>>()?;
            // Things merged into another are not listed: what was filed under them is under
            // the one that remains, which is also known by their names.
            let entities: Vec<Value> = {
                let mut stmt = conn.prepare(
                    "SELECT id, kind, name, role FROM entities WHERE merged_into IS NULL ORDER BY id",
                )?;
                let rows: Vec<(String, String, String, Option<String>)> = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                    .collect::<rusqlite::Result<_>>()?;
                let mut out = Vec::new();
                for (id, kind, name, role) in rows {
                    let former = crate::entity::former_names(conn, &id, &name)?;
                    out.push(json!({"id": id, "kind": kind, "name": name, "role": role,
                                    "formerNames": former}));
                }
                out
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
                "journal": journal,
                "settings": settings,
            });
            write_new(&dir.join("nocturne.json"), serde_json::to_string_pretty(&data)?.as_bytes())?;
            step();
            crate::online::export_section(conn, dir)?;
            crate::phones::export::export_section(conn, dir, &self.clock().tz())?;
            Ok(ExportReport {
                memories: current,
                pages: dates.len() as u32,
                originals: copied,
                path: String::new(),
                not_private: false,
            })
        })?;
        // A forget or an undone keep began while files were being copied: what was written may
        // hold what the person just removed, so none of it is kept.
        if self
            .inner
            .forget_gen
            .load(std::sync::atomic::Ordering::SeqCst)
            != generation
        {
            return Err(Error::Busy);
        }
        report.not_private = !private;
        report.path = guard.commit().to_string_lossy().into_owned();
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
        // Anything exporting now is void, and so is anything that starts before the rows go.
        self.bump_forget();
        let wrote = self.write(|tx| {
            for table in [
                "phone_records",
                "phone_sources",
                "online_sends",
                "online_state",
                "embeddings_next",
                "reindex_state",
                "remote_keeps",
                "remote_events",
                "remote_devices",
                "remote_state",
                "reflection_cites",
                "reflections",
                "reflections_declined",
                "decision_items",
                "decisions",
                "mentions",
                "name_questions",
                "page_line_memories",
                "page_lines",
                "pages",
                "questions",
                "audio_cache",
                "conflicts",
                "embeddings",
                "memory_sources",
                "memories",
                "entity_names",
                "entity_changes",
                "entities",
                "items",
                "ledger",
                "settings",
                "forget_tokens",
                "forget_jobs",
            ] {
                tx.execute(&format!("DELETE FROM {table}"), [])?;
            }
            tx.execute("INSERT INTO remote_state(id) VALUES (1)", [])?;
            tx.execute("INSERT INTO online_state(id) VALUES (1)", [])?;
            tx.execute("INSERT INTO reindex_state(id) VALUES (1)", [])?;
            crate::store::optimize_fts(tx)?;
            Ok(())
        });
        self.bump_forget();
        wrote?;
        for sub in ["originals", "audio"] {
            let path = self.inner.dir.join(sub);
            if let Ok(entries) = std::fs::read_dir(&path) {
                for e in entries.flatten() {
                    fsutil::shred(&e.path());
                }
            }
        }
        self.invalidate_vectors();
        self.bump_audio();
        self.write_raw(|conn| {
            conn.execute_batch("VACUUM")?;
            Ok(())
        })?;
        self.checkpoint_truncate()?;
        Ok(())
    }
}
