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
use std::path::{Path, PathBuf};

use jiff::civil::Date;
use nocturne_contract::{
    ForgetOriginal, ForgetPreview, ForgetReport, ForgetRow, PageView, Period, PeriodKind,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::Engine;
use crate::audio::{drop_audio_rows, pages_citing, shred_all};
use crate::capture::original_path;
use crate::error::{Error, Result};
use crate::fsutil;
use crate::journal::Ctx;
use crate::memory::restore_replaced;
use crate::page::{template_dek, template_headline};
use crate::types::ForgetOutcome;
use crate::util::{claim_hash, fmt_ts, new_id_hex, parse_date, parse_enum, parse_ts};

/// A preview is good for ten minutes.
const TOKEN_MINUTES: i64 = 10;
/// A keep can be undone for ten minutes.
const UNKEEP_MINUTES: i64 = 10;

/// SQL condition over `items i` that keeps only rows whose original no pending forget removes:
/// an unkeep or a forget of the originals that names the item.
pub(crate) const ORIGINALS_NOT_IN_FORGET: &str = "NOT EXISTS (SELECT 1 FROM forget_jobs j \
     WHERE (j.kind = 'unkeep' OR j.include_originals = 1) \
     AND instr(j.items, '\"' || i.id || '\"') > 0)";

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

/// Changes if anything the preview covers is used, corrected, resolved or re-sourced, or an item
/// it deletes is gone or changed.
fn stamp(conn: &Connection, ids: &[String], items: &[String]) -> Result<String> {
    let mut seen = Vec::new();
    for item in items {
        let row: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT status, (SELECT id FROM items r WHERE json_extract(r.metadata, '$.replaces') = items.id) \
                 FROM items WHERE id = ?1",
                [item],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        seen.push(format!("{row:?}"));
    }
    Ok(format!("{}|{}", memory_stamp(conn, ids)?, seen.join(",")))
}

fn memory_stamp(conn: &Connection, ids: &[String]) -> Result<String> {
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

/// Remember, per item that said them, the claims being forgotten, and move those items to a
/// new forget epoch: a read that began before the forget can no longer write, and no later
/// read writes the forgotten claims again. Nothing is queued for a re-read.
fn stamp_sources(tx: &Transaction<'_>, memories: &[String], items: &[String]) -> Result<()> {
    for id in memories {
        let claim: Option<(String, String, String)> = tx
            .query_row(
                "SELECT subject_key, predicate, object_key FROM memories WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((subject, predicate, object)) = claim else {
            continue;
        };
        // The claim may be read again under any name it was filed under before the person
        // renamed it or moved it.
        let mut subjects = vec![subject];
        let mut stmt = tx.prepare(
            "SELECT moved_from_key FROM memories WHERE id = ?1 AND moved_from_key IS NOT NULL \
             UNION SELECT n.key FROM entity_names n JOIN memories m ON m.entity_id = n.entity_id \
             WHERE m.id = ?1",
        )?;
        for key in stmt.query_map([id], |r| r.get::<_, String>(0))? {
            subjects.push(key?);
        }
        subjects.sort();
        subjects.dedup();
        for subject in subjects {
            tx.execute(
                "INSERT OR IGNORE INTO forgotten_claims(item_id, claim) \
                 SELECT item_id, ?2 FROM memory_sources WHERE memory_id = ?1",
                params![id, claim_hash(&subject, &predicate, &object)],
            )?;
        }
    }
    for item in items {
        tx.execute(
            "UPDATE items SET forget_epoch = forget_epoch + 1, reread = 0 WHERE id = ?1",
            [item],
        )?;
    }
    Ok(())
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
        self.preview_delete(ids, include_originals, &[], false)
    }

    /// [`Engine::preview_forget`] for deleting items. `also` are listed and deleted with the
    /// originals even when no memory came from them (a check-in, something not read yet). With
    /// `only_also`, the other originals are listed and never deleted: only what the person ticked
    /// goes. Only what the preview says is removable is deleted, however the store changes before
    /// `forget`.
    pub fn preview_delete(
        &self,
        ids: &[String],
        include_originals: bool,
        also: &[String],
        only_also: bool,
    ) -> Result<ForgetPreview> {
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
            if all.is_empty() && also.is_empty() {
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
            let mut listed = source_items(tx, &all)?;
            for item in also {
                if !listed.contains(item) {
                    listed.push(item.clone());
                }
            }
            for item in listed {
                let (title, kind, kept_at, duration_ms): (String, String, String, Option<u32>) = tx
                    .query_row(
                        "SELECT title, kind, kept_at, duration_ms FROM items WHERE id = ?1",
                        [&item],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                    )?;
                let others = other_memories(tx, &item, &all)?;
                originals.push(ForgetOriginal {
                    removable: others == 0,
                    item_id: item,
                    label: title,
                    kind: parse_enum(&kind)?,
                    kept_at,
                    duration_ms,
                    other_memories: others,
                });
            }
            let pages_affected = pages_citing(tx, &all)?.len() as u32;
            // What will be deleted, and the lines in reflections and recaps that cite it and
            // come out with it.
            let gone: Vec<String> = originals
                .iter()
                .filter(|o| {
                    include_originals && o.removable && (!only_also || also.contains(&o.item_id))
                })
                .map(|o| o.item_id.clone())
                .collect();
            let mut doomed: Vec<String> = Vec::new();
            for item in &gone {
                doomed.push(item.clone());
                doomed.extend(crate::journal::keep::earlier_versions(tx, item)?);
            }
            let (reflection_lines, recap_lines) =
                crate::journal::store::count_lines(tx, &doomed)?;
            let token = new_id_hex();
            tx.execute(
                "INSERT INTO forget_tokens(token, ids, include_originals, stamp, expires_at, items) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    token,
                    serde_json::to_string(ids)?,
                    include_originals,
                    stamp(tx, &all, &gone)?,
                    fmt_ts(expires),
                    serde_json::to_string(&gone)?,
                ],
            )?;
            Ok(ForgetPreview {
                reflection_lines: (reflection_lines > 0).then_some(reflection_lines),
                recap_lines: (recap_lines > 0).then_some(recap_lines),
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

    /// Finish every pending job, as the next open does. For tests that stop part way.
    #[cfg(any(test, feature = "fake"))]
    pub fn resume_forgets_for_test(&self) -> Result<()> {
        self.resume_forgets()
    }

    fn forget_begin(&self, token: &str) -> Result<(i64, u32)> {
        let now = self.clock().now();
        let stamp_now = fmt_ts(now);
        let (job, pages_redacted, files) = self.write(|tx| {
            type TokenRow = (String, bool, String, String, Option<String>);
            let row: Option<TokenRow> = tx
                .query_row(
                    "SELECT ids, include_originals, stamp, expires_at, items FROM forget_tokens \
                     WHERE token = ?1",
                    [token],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;
            let Some((ids, include_originals, saved_stamp, expires, gone)) = row else {
                return Err(Error::StaleToken);
            };
            if parse_ts(&expires)? <= now {
                // Refused; an expired token is purged by the next preview.
                return Err(Error::StaleToken);
            }
            let ids: Vec<String> = serde_json::from_str(&ids)?;
            let gone: Vec<String> = match gone {
                Some(g) => serde_json::from_str(&g)?,
                None => Vec::new(),
            };
            let all = closure(tx, &ids)?;
            if (all.is_empty() && gone.is_empty()) || stamp(tx, &all, &gone)? != saved_stamp {
                return Err(Error::StaleToken);
            }
            tx.execute("DELETE FROM forget_tokens WHERE token = ?1", [token])?;
            let sources = source_items(tx, &all)?;
            // With originals, exactly those the preview listed as going; otherwise the sources,
            // as before.
            let items = if include_originals {
                gone.clone()
            } else {
                sources.clone()
            };
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
            self.bump_forget();
            let mut touched = sources;
            for g in &gone {
                if !touched.contains(g) {
                    touched.push(g.clone());
                }
            }
            stamp_sources(tx, &all, &touched)?;
            for id in &all {
                tx.execute(
                    "UPDATE memories SET forgetting = 1, superseded_at = COALESCE(superseded_at, ?2) \
                     WHERE id = ?1",
                    params![id, stamp_now],
                )?;
            }
            redact_pages(tx, &all, &pages, None)?;
            let files = drop_audio_rows(tx, &pages)?;
            // Recorded before the commit, so a crash before the files are shredded still
            // finds them.
            append_job_files(tx, job, &files)?;
            Ok((job, pages.len() as u32, files))
        })?;
        self.bump_forget();
        self.invalidate_vectors();
        shred_all(files);
        Ok((job, pages_redacted))
    }

    /// Undo a keep: the person's Undo right after keeping something. Allowed within ten minutes
    /// of `kept_at`, otherwise `Error::TooLate`.
    ///
    /// The item, its original, transcript, summary and search rows go, and so does every
    /// memory whose only source is this item, with its earlier versions that came from it
    /// alone, the pages that cited it and their cached audio. A memory with other sources
    /// stays and loses only the link to this item. A memory this item had replaced or ended
    /// comes back as it was. It runs as a forget job, so a crash part way is finished on the
    /// next open. The first phase already removes the item row and lists its original for
    /// shredding, so nothing readable is left once it commits.
    pub fn unkeep(&self, item_id: &str) -> Result<ForgetOutcome> {
        // A feeling tapped with a note was kept with it, so taking the note back takes it too.
        let feelings: Vec<String> = self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM items WHERE json_extract(metadata, '$.journal') = 'checkin' \
                 AND json_extract(metadata, '$.about') = ?1",
            )?;
            Ok(stmt
                .query_map([item_id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })?;
        let outcome = self.unkeep_one(item_id)?;
        for feeling in feelings {
            self.unkeep_one(&feeling)?;
        }
        Ok(outcome)
    }

    fn unkeep_one(&self, item_id: &str) -> Result<ForgetOutcome> {
        let (job, pages_redacted, had_original) = self.unkeep_begin(item_id)?;
        let mut outcome = self.finish_forget(job)?;
        outcome.report.pages_redacted = pages_redacted;
        outcome.report.originals_removed = u32::from(had_original);
        Ok(outcome)
    }

    /// Phase 1 of `unkeep` only, for tests that simulate stopping part way.
    #[cfg(any(test, feature = "fake"))]
    pub fn unkeep_begin_only(&self, item_id: &str) -> Result<()> {
        self.unkeep_begin(item_id).map(|_| ())
    }

    fn unkeep_begin(&self, item_id: &str) -> Result<(i64, u32, bool)> {
        let now = self.clock().now();
        let stamp_now = fmt_ts(now);
        let dir = self.originals_dir();
        let (job, pages_redacted, had_original, files) = self.write(|tx| {
            type Row = (String, String, Option<String>, Option<String>, String);
            let row: Option<Row> = tx
                .query_row(
                    "SELECT kept_at, sha256, ext, file_name, status FROM items WHERE id = ?1",
                    [item_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;
            let (kept_at, sha, ext, file_name, status) =
                row.ok_or_else(|| Error::not_found(format!("item {item_id}")))?;
            if crate::capture::in_forget_job(tx, item_id)? {
                return Err(Error::not_found(format!("item {item_id}")));
            }
            let window = jiff::SignedDuration::from_mins(UNKEEP_MINUTES);
            if now.duration_since(parse_ts(&kept_at)?) > window {
                return Err(Error::TooLate);
            }

            // Memories this item alone supports go; the rest only lose the link.
            let sole: Vec<String> = {
                let mut stmt = tx.prepare(
                    "SELECT memory_id FROM memory_sources GROUP BY memory_id \
                     HAVING COUNT(*) = 1 AND MIN(item_id) = ?1 ORDER BY memory_id",
                )?;
                stmt.query_map([item_id], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            };
            let shared: Vec<String> = {
                let mut stmt = tx.prepare(
                    "SELECT memory_id FROM memory_sources WHERE item_id = ?1 ORDER BY memory_id",
                )?;
                let all: Vec<String> = stmt
                    .query_map([item_id], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
                all.into_iter().filter(|m| !sole.contains(m)).collect()
            };
            for id in &sole {
                restore_replaced(tx, id)?;
            }
            tx.execute("DELETE FROM memory_sources WHERE item_id = ?1", [item_id])?;
            for id in &shared {
                // Without a trusted source left, only the person's word keeps it active, and
                // a "which is right?" it entered by being promoted goes with it.
                let demoted = tx.execute(
                    "UPDATE memories SET status = 'unconfirmed', let_go_at = NULL \
                     WHERE id = ?1 AND confirmed = 0 AND superseded_at IS NULL \
                       AND status IN ('active', 'fading') \
                       AND NOT EXISTS (SELECT 1 FROM memory_sources s JOIN items i ON i.id = s.item_id \
                                       WHERE s.memory_id = ?1 AND i.trust = 'trusted')",
                    [id],
                )?;
                if demoted > 0 {
                    tx.execute(
                        "DELETE FROM conflicts WHERE resolved_at IS NULL AND (a = ?1 OR b = ?1)",
                        [id],
                    )?;
                }
            }

            let mut pages = pages_citing(tx, &sole)?;
            {
                let mut stmt = tx.prepare("SELECT date FROM pages WHERE instr(body, ?1) > 0")?;
                for d in stmt.query_map([item_id], |r| r.get::<_, String>(0))? {
                    let d = d?;
                    if !pages.contains(&d) {
                        pages.push(d);
                    }
                }
            }
            tx.execute(
                "INSERT INTO forget_jobs(ids, items, include_originals, kind) \
                 VALUES (?1, ?2, 1, 'unkeep')",
                params![
                    serde_json::to_string(&sole)?,
                    serde_json::to_string(&[item_id])?,
                ],
            )?;
            let job = tx.last_insert_rowid();
            self.bump_audio();
            self.bump_forget();
            for id in &sole {
                tx.execute(
                    "UPDATE memories SET forgetting = 1, superseded_at = COALESCE(superseded_at, ?2) \
                     WHERE id = ?1",
                    params![id, stamp_now],
                )?;
            }
            redact_pages(tx, &sole, &pages, Some(item_id))?;
            // The item's own row (text, summary and search rows with it) goes now, with its
            // original recorded for shredding in the same transaction.
            crate::journal::store::scrub_items(tx, &[item_id.to_string()])?;
            let ctx = Ctx::load(tx, self.clock())?;
            let days = BTreeSet::from_iter(journal_day_of(tx, &ctx, item_id)?);
            delete_earlier_versions(tx, &dir, job, item_id)?;
            tx.execute("DELETE FROM items WHERE id = ?1", [item_id])?;
            drop_emptied_days(tx, &ctx, &days)?;
            let pending = matches!(status.as_str(), "kept" | "reading");
            forget_in_pages(tx, &kept_at, pending)?;
            let mut files = drop_audio_rows(tx, &pages)?;
            let had_original = file_name.is_some();
            if had_original {
                files.push(original_path(&dir, &sha, ext.as_deref()));
            }
            append_job_files(tx, job, &files)?;
            Ok((job, pages.len() as u32, had_original, files))
        })?;
        self.bump_forget();
        self.invalidate_vectors();
        shred_all(files);
        Ok((job, pages_redacted, had_original))
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

        // Items and originals are about to go; an export that is running is void.
        self.bump_forget();

        // Phase 2: remove the memories in chunks, so capture never waits long. Each chunk
        // first redacts any page that has come to cite it since phase 1.
        for chunk in closure_ids.chunks(200) {
            let files = self.write(|tx| {
                let pages = pages_citing(tx, chunk)?;
                let files = if pages.is_empty() {
                    Vec::new()
                } else {
                    redact_pages(tx, &closure_ids, &pages, None)?;
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
                    let ctx = Ctx::load(tx, self.clock())?;
                    let mut days = BTreeSet::new();
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
                            crate::journal::store::scrub_items(tx, std::slice::from_ref(item))?;
                            days.extend(journal_day_of(tx, &ctx, item)?);
                            delete_earlier_versions(tx, &dir, job, item)?;
                            tx.execute("DELETE FROM items WHERE id = ?1", [item])?;
                            if file_name.is_some() {
                                doomed.push(original_path(&dir, &sha, ext.as_deref()));
                            }
                            originals_removed += 1;
                        }
                    }
                    drop_emptied_days(tx, &ctx, &days)?;
                    append_job_files(tx, job, &doomed)?;
                    Ok(doomed)
                })?;
                shred_in(&dir, &doomed);
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
        shred_in(&self.originals_dir(), &recorded);
        self.write(|tx| {
            // Nothing filed under a thing, nothing left of it. One merged into another stays
            // for as long as the one it was merged into does, so the merge can be taken back.
            tx.execute(
                "WITH RECURSIVE keep(id) AS ( \
                   SELECT id FROM entities WHERE merged_into IS NULL \
                     AND (id IN (SELECT entity_id FROM memories WHERE entity_id IS NOT NULL) \
                       OR id IN (SELECT n.entity_id FROM mentions n JOIN items i ON i.id = n.item_id \
                                 WHERE i.kind = 'checkin')) \
                   UNION SELECT e.id FROM entities e JOIN keep k ON e.merged_into = k.id) \
                 DELETE FROM entities WHERE id NOT IN (SELECT id FROM keep)",
                [],
            )?;
            // A change that named a thing no longer there cannot be taken back, and keeps no
            // trace of it.
            tx.execute(
                "DELETE FROM entity_changes WHERE NOT EXISTS \
                 (SELECT 1 FROM entities e WHERE e.id = json_extract(entity_changes.detail, '$.entity')) \
                 OR (kind = 'merge' AND NOT EXISTS \
                 (SELECT 1 FROM entities e WHERE e.id = json_extract(entity_changes.detail, '$.from')))",
                [],
            )?;
            tx.execute("DELETE FROM forget_jobs WHERE id = ?1", [job])?;
            crate::store::optimize_fts(tx)?;
            Ok(())
        })?;
        self.invalidate_vectors();
        self.bump_audio();
        self.bump_forget();
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

/// The journal day an item belongs to, as the journal reads it.
fn journal_day_of(tx: &Transaction<'_>, ctx: &Ctx, item: &str) -> Result<Option<Date>> {
    let row: Option<(Option<String>, String)> = tx
        .query_row(
            "SELECT json_extract(metadata, '$.journal_day'), kept_at FROM items WHERE id = ?1",
            [item],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(match row {
        Some((Some(day), _)) => Some(parse_date(&day)?),
        Some((None, kept_at)) => Some(ctx.day_of(parse_ts(&kept_at)?)?),
        None => None,
    })
}

/// A day left with no item at all keeps no reflection: it was written from what is gone. The
/// reflection is dropped, not declined, so a day that is kept on again can be reflected on.
fn drop_emptied_days(tx: &Transaction<'_>, ctx: &Ctx, days: &BTreeSet<Date>) -> Result<()> {
    for day in days {
        let (from, to) = ctx.bounds(*day)?;
        let left: u32 = tx.query_row(
            "SELECT COUNT(*) FROM items WHERE json_extract(metadata, '$.journal_day') = ?1 \
             OR (json_extract(metadata, '$.journal_day') IS NULL AND kept_at >= ?2 AND kept_at < ?3)",
            params![day.to_string(), fmt_ts(from), fmt_ts(to)],
            |r| r.get(0),
        )?;
        if left == 0 {
            let period = Period {
                kind: PeriodKind::Day,
                start: day.to_string(),
                end: day.to_string(),
            };
            crate::journal::store::delete(tx, &period)?;
        }
    }
    Ok(())
}

/// An item that is gone no longer counts in the pages whose window held it: their counts and
/// the template dek say so (a written dek would still quote the old count).
fn forget_in_pages(tx: &Transaction<'_>, kept_at: &str, was_pending: bool) -> Result<()> {
    let dates: Vec<String> = {
        let mut stmt = tx.prepare(
            "SELECT date FROM pages WHERE window_start <= ?1 AND ?1 < window_end ORDER BY date",
        )?;
        stmt.query_map([kept_at], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    for date in dates {
        let body: String =
            tx.query_row("SELECT body FROM pages WHERE date = ?1", [&date], |r| {
                r.get(0)
            })?;
        let mut view: PageView = serde_json::from_str(&body)?;
        view.kept_count = view.kept_count.saturating_sub(1);
        if was_pending {
            view.pending = view.pending.saturating_sub(1);
        }
        let needs =
            view.needs_you.iter().filter(|l| !l.forgotten).count() as u32 + view.more_needs_you;
        view.dek = template_dek(view.kept_count, needs, view.let_go_count);
        tx.execute(
            "UPDATE pages SET body = ?2, dek = ?3 WHERE date = ?1",
            params![date, crate::page::stored_body(&view)?, view.dek],
        )?;
    }
    Ok(())
}

/// Shred only files inside `dir`; a path from a job row can never reach outside it.
fn shred_in(dir: &Path, files: &[PathBuf]) {
    let Ok(root) = dunce::canonicalize(dir) else {
        return;
    };
    for file in files {
        let target = match dunce::canonicalize(file) {
            Ok(path) => path,
            Err(_) => match (file.parent(), file.file_name()) {
                (Some(parent), Some(name)) => match dunce::canonicalize(parent) {
                    Ok(parent) => parent.join(name),
                    Err(_) => continue,
                },
                _ => continue,
            },
        };
        if target.starts_with(&root) && target != root {
            fsutil::shred(file);
        }
    }
}

/// Deleting an entry block deletes its earlier versions with it: their rows, the memories only
/// they support, their reflection lines, and their files (recorded on the job for shredding).
fn delete_earlier_versions(tx: &Transaction<'_>, dir: &Path, job: i64, item: &str) -> Result<()> {
    let earlier = crate::journal::keep::earlier_versions(tx, item)?;
    if earlier.is_empty() {
        return Ok(());
    }
    let mut sole: BTreeSet<String> = BTreeSet::new();
    for e in &earlier {
        let mut stmt =
            tx.prepare_cached("SELECT memory_id FROM memory_sources WHERE item_id = ?1")?;
        let mems: Vec<String> = stmt
            .query_map([e], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for m in mems {
            let others: u32 = tx.query_row(
                "SELECT COUNT(*) FROM memory_sources WHERE memory_id = ?1 AND item_id NOT IN \
                 (SELECT value FROM json_each(?2))",
                params![m, serde_json::to_string(&earlier)?],
                |r| r.get(0),
            )?;
            if others == 0 {
                sole.insert(m);
            }
        }
    }
    let sole: Vec<String> = sole.into_iter().collect();
    let mut files = Vec::new();
    if !sole.is_empty() {
        let pages = pages_citing(tx, &sole)?;
        redact_pages(tx, &sole, &pages, None)?;
        files.extend(drop_audio_rows(tx, &pages)?);
        for m in &sole {
            purge_memory(tx, m)?;
        }
    }
    crate::journal::store::scrub_items(tx, &earlier)?;
    for e in &earlier {
        let row: Option<(String, Option<String>, Option<String>)> = tx
            .query_row(
                "SELECT sha256, ext, file_name FROM items WHERE id = ?1",
                [e],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((sha, ext, file_name)) = row {
            tx.execute("DELETE FROM items WHERE id = ?1", [e])?;
            if file_name.is_some() {
                files.push(original_path(dir, &sha, ext.as_deref()));
            }
        }
    }
    append_job_files(tx, job, &files)
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
    crate::journal::question::drop_memory_questions(tx, id)?;
    tx.execute("DELETE FROM page_line_memories WHERE memory_id = ?1", [id])?;
    crate::online::scrub_memory(tx, id)?;
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

/// `item` also redacts every line whose receipts name that item (the lines of memories that
/// outlive an undone keep still must not name it).
fn redact_pages(
    tx: &Transaction<'_>,
    ids: &[String],
    pages: &[String],
    item: Option<&str>,
) -> Result<()> {
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
                    || line.choices.iter().any(|c| ids.contains(&c.memory_id))
                    || item.is_some_and(|i| {
                        line.receipts.iter().any(|r| r.item_id == i)
                            || line.choices.iter().any(|c| c.receipt.item_id == i)
                    });
                if hit {
                    tx.execute(
                        "UPDATE page_lines SET forgotten = 1, text = '' WHERE id = ?1",
                        [&line.id],
                    )?;
                    redact_line(line);
                }
            }
        }
        if view.question.as_ref().is_some_and(|q| {
            q.memory_id.as_ref().is_some_and(|m| ids.contains(m))
                || item.is_some_and(|i| q.receipt.as_ref().is_some_and(|r| r.item_id == i))
        }) {
            view.question = None;
        }
        let needs =
            view.needs_you.iter().filter(|l| !l.forgotten).count() as u32 + view.more_needs_you;
        let date_parsed: jiff::civil::Date = date.parse()?;
        view.headline = template_headline(date_parsed);
        view.dek = template_dek(view.kept_count, needs, view.let_go_count);
        tx.execute(
            "UPDATE pages SET body = ?2, headline = ?3, dek = ?4 WHERE date = ?1",
            params![
                date,
                crate::page::stored_body(&view)?,
                view.headline,
                view.dek
            ],
        )?;
    }
    Ok(())
}
