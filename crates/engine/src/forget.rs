//! Forget completely: a confirmed, undoable-never removal of a memory, its history and
//! everything derived from it.
//!
//! `preview_forget` says exactly what will go and hands back a token. `forget` takes the token,
//! checks nothing it covers has changed, and removes the memories and every earlier version,
//! their sources, vectors, search rows, conflicts and questions; redacts the lines of past pages
//! that cited them; deletes cached audio for those pages; and, when asked, removes the original
//! items nothing else depends on. Afterwards the search indexes are compacted and the
//! write-ahead log is truncated so no trace stays in the store files.

use std::collections::BTreeSet;
use std::path::PathBuf;

use nocturne_contract::{ForgetOriginal, ForgetPreview, ForgetReport, ForgetRow, PageView};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::Engine;
use crate::audio::{drop_audio_rows, pages_citing, shred_all};
use crate::capture::original_path;
use crate::error::{Error, Result};
use crate::fsutil;
use crate::page::{template_dek, template_headline};
use crate::types::ForgetOutcome;
use crate::util::{fmt_ts, new_id_hex, parse_enum, parse_ts};

/// A preview is good for ten minutes.
const TOKEN_MINUTES: i64 = 10;

/// The requested memories plus every earlier version they replaced or ended.
fn closure(conn: &Connection, ids: &[String]) -> Result<Vec<String>> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    let mut frontier: Vec<String> = Vec::new();
    for id in ids {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM memories WHERE id = ?1)",
            [id],
            |r| r.get(0),
        )?;
        if exists && set.insert(id.clone()) {
            frontier.push(id.clone());
        }
    }
    while let Some(id) = frontier.pop() {
        let mut stmt = conn.prepare_cached("SELECT id FROM memories WHERE superseded_by = ?1")?;
        let older: Vec<String> = stmt
            .query_map([&id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for o in older {
            if set.insert(o.clone()) {
                frontier.push(o);
            }
        }
    }
    Ok(set.into_iter().collect())
}

/// Changes if anything the preview covers is used, corrected, resolved or re-sourced.
fn stamp(conn: &Connection, ids: &[String]) -> Result<String> {
    let mut latest = String::new();
    let mut sources = 0i64;
    for id in ids {
        let (a, b, c, s): (String, Option<String>, Option<String>, i64) = conn.query_row(
            "SELECT recorded_at, last_used, superseded_at, \
             (SELECT COUNT(*) FROM memory_sources WHERE memory_id = memories.id) \
             FROM memories WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        for v in [Some(a), b, c].into_iter().flatten() {
            if v > latest {
                latest = v;
            }
        }
        sources += s;
    }
    Ok(format!("{latest}|{}|{sources}", ids.len()))
}

fn source_items(conn: &Connection, ids: &[String]) -> Result<Vec<String>> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for id in ids {
        let mut stmt =
            conn.prepare_cached("SELECT item_id FROM memory_sources WHERE memory_id = ?1")?;
        for r in stmt.query_map([id], |r| r.get::<_, String>(0))? {
            out.insert(r?);
        }
    }
    Ok(out.into_iter().collect())
}

fn other_memories(conn: &Connection, item: &str, ids: &[String]) -> Result<u32> {
    let mut stmt =
        conn.prepare_cached("SELECT DISTINCT memory_id FROM memory_sources WHERE item_id = ?1")?;
    let all: Vec<String> = stmt
        .query_map([item], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(all.iter().filter(|m| !ids.contains(m)).count() as u32)
}

impl Engine {
    /// Say exactly what forgetting these memories removes. Nothing changes until `forget`.
    ///
    /// Originals are listed with `removable`. An original is removed only when the preview was
    /// asked to include originals and no other memory cites it. When it stays (not included, or
    /// still cited), its transcript, summary and search row stay with it: they are the person's
    /// own item, not something derived from the forgotten memory.
    pub fn preview_forget(&self, ids: &[String], include_originals: bool) -> Result<ForgetPreview> {
        let now = self.clock().now();
        let expires = now
            .checked_add(jiff::SignedDuration::from_mins(TOKEN_MINUTES))
            .map_err(Error::from)?;
        self.write(|tx| {
            tx.execute(
                "DELETE FROM forget_tokens WHERE expires_at <= ?1",
                [fmt_ts(now)],
            )?;
            let all = closure(tx, ids)?;
            if all.is_empty() {
                return Err(Error::not_found("those memories"));
            }
            let mut memories = Vec::new();
            for id in &all {
                let (sentence, recorded): (String, String) = tx.query_row(
                    "SELECT sentence, recorded_at FROM memories WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                memories.push((recorded, id.clone(), sentence));
            }
            memories.sort();
            let mut originals = Vec::new();
            for item in source_items(tx, &all)? {
                let (title, kind, kept_at, duration_ms): (String, String, String, Option<u32>) = tx
                    .query_row(
                        "SELECT title, kind, kept_at, duration_ms FROM items WHERE id = ?1",
                        [&item],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                    )?;
                let others = other_memories(tx, &item, &all)?;
                originals.push(ForgetOriginal {
                    item_id: item,
                    label: title,
                    kind: parse_enum(&kind)?,
                    kept_at,
                    duration_ms,
                    removable: others == 0,
                    other_memories: others,
                });
            }
            let pages_affected = pages_citing(tx, &all)?.len() as u32;
            let token = new_id_hex();
            tx.execute(
                "INSERT INTO forget_tokens(token, ids, include_originals, stamp, expires_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    token,
                    serde_json::to_string(ids)?,
                    include_originals,
                    stamp(tx, &all)?,
                    fmt_ts(expires),
                ],
            )?;
            Ok(ForgetPreview {
                token,
                memories: memories
                    .into_iter()
                    .map(|(_, memory_id, sentence)| ForgetRow {
                        memory_id,
                        sentence,
                    })
                    .collect(),
                originals,
                include_originals,
                pages_affected,
            })
        })
    }

    /// Carry out a previewed forget. The token is single use and expires; it is refused if
    /// anything it covers has changed since the preview.
    ///
    /// Phase 1 is one transaction: it spends the token, hides the memories everywhere (so no
    /// page, search, answer or list can show them from this moment), redacts the pages that cite
    /// them and drops their cached audio, and records a job. The job is then finished in chunks;
    /// if the app stops in between, the next `open` finishes it.
    pub fn forget(&self, token: &str) -> Result<ForgetOutcome> {
        let (job, pages_redacted) = self.forget_begin(token)?;
        let mut outcome = self.finish_forget(job)?;
        outcome.report.pages_redacted = pages_redacted;
        Ok(outcome)
    }

    /// Phase 1 only, for tests that simulate stopping part way. Not for production use.
    #[cfg(any(test, feature = "fake"))]
    pub fn forget_begin_only(&self, token: &str) -> Result<()> {
        self.forget_begin(token).map(|_| ())
    }

    fn forget_begin(&self, token: &str) -> Result<(i64, u32)> {
        let now = self.clock().now();
        let stamp_now = fmt_ts(now);
        let (job, pages_redacted, files) = self.write(|tx| {
            let row: Option<(String, bool, String, String)> = tx
                .query_row(
                    "SELECT ids, include_originals, stamp, expires_at FROM forget_tokens WHERE token = ?1",
                    [token],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?;
            let Some((ids, include_originals, saved_stamp, expires)) = row else {
                return Err(Error::StaleToken);
            };
            if parse_ts(&expires)? <= now {
                // Refused; an expired token is purged by the next preview.
                return Err(Error::StaleToken);
            }
            let ids: Vec<String> = serde_json::from_str(&ids)?;
            let all = closure(tx, &ids)?;
            if all.is_empty() || stamp(tx, &all)? != saved_stamp {
                return Err(Error::StaleToken);
            }
            tx.execute("DELETE FROM forget_tokens WHERE token = ?1", [token])?;
            let items = source_items(tx, &all)?;
            let pages = pages_citing(tx, &all)?;
            tx.execute(
                "INSERT INTO forget_jobs(ids, items, include_originals) VALUES (?1, ?2, ?3)",
                params![
                    serde_json::to_string(&all)?,
                    serde_json::to_string(&items)?,
                    include_originals
                ],
            )?;
            let job = tx.last_insert_rowid();
            self.bump_audio();
            for id in &all {
                tx.execute(
                    "UPDATE memories SET forgetting = 1, superseded_at = COALESCE(superseded_at, ?2) \
                     WHERE id = ?1",
                    params![id, stamp_now],
                )?;
            }
            redact_pages(tx, &all, &pages)?;
            let files = drop_audio_rows(tx, &pages)?;
            // Recorded before the commit, so a crash before the files are shredded still
            // finds them.
            append_job_files(tx, job, &files)?;
            Ok((job, pages.len() as u32, files))
        })?;
        self.invalidate_vectors();
        shred_all(files);
        Ok((job, pages_redacted))
    }

    /// Finish forgets that were started and not completed (the app stopped part way).
    pub(crate) fn resume_forgets(&self) -> Result<()> {
        let jobs: Vec<i64> = self.read(|conn| {
            let mut stmt = conn.prepare("SELECT id FROM forget_jobs ORDER BY id")?;
            Ok(stmt
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })?;
        for job in jobs {
            self.finish_forget(job)?;
        }
        Ok(())
    }

    fn finish_forget(&self, job: i64) -> Result<ForgetOutcome> {
        let (closure_ids, items, include_originals): (Vec<String>, Vec<String>, bool) =
            self.read(|conn| {
                let (ids, items, inc): (String, String, bool) = conn.query_row(
                    "SELECT ids, items, include_originals FROM forget_jobs WHERE id = ?1",
                    [job],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?;
                Ok((
                    serde_json::from_str(&ids)?,
                    serde_json::from_str(&items)?,
                    inc,
                ))
            })?;

        // Phase 2: remove the memories in chunks, so capture never waits long. Each chunk
        // first redacts any page that has come to cite it since phase 1.
        for chunk in closure_ids.chunks(200) {
            let files = self.write(|tx| {
                let pages = pages_citing(tx, chunk)?;
                let files = if pages.is_empty() {
                    Vec::new()
                } else {
                    redact_pages(tx, &closure_ids, &pages)?;
                    let files = drop_audio_rows(tx, &pages)?;
                    append_job_files(tx, job, &files)?;
                    files
                };
                for id in chunk {
                    purge_memory(tx, id)?;
                }
                Ok(files)
            })?;
            shred_all(files);
        }

        // Phase 3: originals nothing else depends on, when the person asked.
        let mut originals_removed = 0u32;
        if include_originals {
            let dir = self.originals_dir();
            for chunk in items.chunks(200) {
                let doomed = self.write(|tx| {
                    let mut doomed = Vec::new();
                    for item in chunk {
                        let left: u32 = tx.query_row(
                            "SELECT COUNT(*) FROM memory_sources WHERE item_id = ?1",
                            [item],
                            |r| r.get(0),
                        )?;
                        if left > 0 {
                            continue;
                        }
                        let row: Option<(String, Option<String>, Option<String>)> = tx
                            .query_row(
                                "SELECT sha256, ext, file_name FROM items WHERE id = ?1",
                                [item],
                                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                            )
                            .optional()?;
                        if let Some((sha, ext, file_name)) = row {
                            tx.execute("DELETE FROM items WHERE id = ?1", [item])?;
                            if file_name.is_some() {
                                doomed.push(original_path(&dir, &sha, ext.as_deref()));
                            }
                            originals_removed += 1;
                        }
                    }
                    append_job_files(tx, job, &doomed)?;
                    Ok(doomed)
                })?;
                for f in doomed {
                    fsutil::shred(&f);
                }
            }
        }

        // Phase 4: remove every file the job recorded (a resumed job may find some already
        // gone), orphaned entity names, the finished job, compacted indexes, a truncated log.
        let recorded: Vec<PathBuf> = self.read(|conn| {
            let files: String =
                conn.query_row("SELECT files FROM forget_jobs WHERE id = ?1", [job], |r| {
                    r.get(0)
                })?;
            let paths: Vec<String> = serde_json::from_str(&files)?;
            Ok(paths.into_iter().map(PathBuf::from).collect())
        })?;
        shred_all(recorded);
        self.write(|tx| {
            tx.execute(
                "DELETE FROM entities WHERE id NOT IN \
                 (SELECT entity_id FROM memories WHERE entity_id IS NOT NULL)",
                [],
            )?;
            tx.execute("DELETE FROM forget_jobs WHERE id = ?1", [job])?;
            tx.execute(
                "INSERT INTO memories_fts(memories_fts) VALUES ('optimize')",
                [],
            )?;
            tx.execute("INSERT INTO items_fts(items_fts) VALUES ('optimize')", [])?;
            Ok(())
        })?;
        self.invalidate_vectors();
        self.bump_audio();
        let fully_scrubbed = self.checkpoint_truncate()?;

        Ok(ForgetOutcome {
            report: ForgetReport {
                memories_removed: closure_ids.len() as u32,
                originals_removed,
                pages_redacted: 0,
            },
            fully_scrubbed,
        })
    }
}

/// Remember files a forget must shred, in the same transaction that makes them unreachable.
fn append_job_files(tx: &Transaction<'_>, job: i64, files: &[PathBuf]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    let current: String =
        tx.query_row("SELECT files FROM forget_jobs WHERE id = ?1", [job], |r| {
            r.get(0)
        })?;
    let mut paths: Vec<String> = serde_json::from_str(&current)?;
    paths.extend(files.iter().map(|p| p.to_string_lossy().into_owned()));
    tx.execute(
        "UPDATE forget_jobs SET files = ?2 WHERE id = ?1",
        params![job, serde_json::to_string(&paths)?],
    )?;
    Ok(())
}

/// Remove one memory and everything that hangs off it. Sources and vectors cascade; search rows
/// go through the delete trigger.
fn purge_memory(tx: &Transaction<'_>, id: &str) -> Result<()> {
    tx.execute("DELETE FROM conflicts WHERE a = ?1 OR b = ?1", [id])?;
    tx.execute("DELETE FROM questions WHERE memory_id = ?1", [id])?;
    tx.execute("DELETE FROM page_line_memories WHERE memory_id = ?1", [id])?;
    tx.execute("DELETE FROM memories WHERE id = ?1", [id])?;
    Ok(())
}

/// Lines citing a forgotten memory are redacted in the line table and in the saved snapshot; the
/// page keeps its shape, and its headline and dek fall back to the template (no mind call).
/// Blank a line the way a forgotten line reads: present, empty and flagged.
pub(crate) fn redact_line(line: &mut nocturne_contract::PageLine) {
    line.forgotten = true;
    line.text.clear();
    line.memory_ids.clear();
    line.receipts.clear();
    line.choices.clear();
    line.subject = None;
    line.conflict_id = None;
    line.on = None;
    line.last_used = None;
    line.done = false;
}

fn redact_pages(tx: &Transaction<'_>, ids: &[String], pages: &[String]) -> Result<()> {
    let mut lines: BTreeSet<String> = BTreeSet::new();
    for id in ids {
        let mut stmt =
            tx.prepare_cached("SELECT line_id FROM page_line_memories WHERE memory_id = ?1")?;
        for r in stmt.query_map([id], |r| r.get::<_, String>(0))? {
            lines.insert(r?);
        }
    }
    for line in &lines {
        tx.execute(
            "UPDATE page_lines SET forgotten = 1, text = '' WHERE id = ?1",
            [line],
        )?;
    }
    for date in pages {
        let body: String = tx.query_row("SELECT body FROM pages WHERE date = ?1", [date], |r| {
            r.get(0)
        })?;
        let mut view: PageView = serde_json::from_str(&body)?;
        for list in [
            &mut view.needs_you,
            &mut view.changed,
            &mut view.let_go,
            &mut view.upcoming,
            &mut view.learned,
        ] {
            for line in list.iter_mut() {
                let hit = lines.contains(&line.id)
                    || line.memory_ids.iter().any(|m| ids.contains(m))
                    || line.choices.iter().any(|c| ids.contains(&c.memory_id));
                if hit {
                    redact_line(line);
                }
            }
        }
        if view
            .question
            .as_ref()
            .and_then(|q| q.memory_id.as_ref())
            .is_some_and(|m| ids.contains(m))
        {
            view.question = None;
        }
        let needs = view.needs_you.iter().filter(|l| !l.forgotten).count() as u32;
        let date_parsed: jiff::civil::Date = date.parse()?;
        view.headline = template_headline(date_parsed);
        view.dek = template_dek(view.kept_count, needs, view.let_go_count);
        tx.execute(
            "UPDATE pages SET body = ?2, headline = ?3, dek = ?4 WHERE date = ?1",
            params![date, serde_json::to_string(&view)?, view.headline, view.dek],
        )?;
    }
    Ok(())
}
