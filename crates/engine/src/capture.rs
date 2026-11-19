//! Capture: keep text and files, decide trust and kind, dedupe by hash, queue for reading.
//! Capture never calls the mind and never rejects because the mind is busy or absent.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use jiff::SignedDuration;
use nocturne_contract::{CouldntReadReason, ItemFilter, ItemKind, ItemView, Kept, Source, Trust};
use rusqlite::{OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};

use crate::Engine;
use crate::error::{Error, Result};
use crate::mind::{Transcript, Understanding};
use crate::rows::{ITEM_COLS, get_item_view, item_view};
use crate::types::{FailureOutcome, PendingItem, RecordOutcome};
use crate::util::{enum_str, fmt_ts, hex, new_id, random_hex, sha256_hex};

/// Recordings longer than this are kept but not read.
pub const MAX_AUDIO_MS: u32 = 2 * 60 * 60 * 1000;
/// Documents longer than this are kept but not read.
pub const MAX_PAGES: u32 = 100;
/// An item the mind fails on this many times is given up on.
pub const MAX_FAILURES: u32 = 3;

/// Trust follows the source (data model, "Trust by source").
pub fn trust_for(source: Source) -> Trust {
    match source {
        Source::PastedWeb | Source::Agent => Trust::Untrusted,
        _ => Trust::Trusted,
    }
}

/// Kind and MIME type from a file extension (lowercase, without the dot).
pub fn kind_for_ext(ext: &str) -> Option<(ItemKind, &'static str)> {
    Some(match ext {
        "jpg" | "jpeg" => (ItemKind::Image, "image/jpeg"),
        "png" => (ItemKind::Image, "image/png"),
        "heic" => (ItemKind::Image, "image/heic"),
        "webp" => (ItemKind::Image, "image/webp"),
        "gif" => (ItemKind::Image, "image/gif"),
        "m4a" => (ItemKind::Audio, "audio/mp4"),
        "mp3" => (ItemKind::Audio, "audio/mpeg"),
        "wav" => (ItemKind::Audio, "audio/wav"),
        "ogg" => (ItemKind::Audio, "audio/ogg"),
        "opus" => (ItemKind::Audio, "audio/opus"),
        "webm" => (ItemKind::Audio, "audio/webm"),
        "flac" => (ItemKind::Audio, "audio/flac"),
        "pdf" => (ItemKind::Document, "application/pdf"),
        "txt" => (ItemKind::Document, "text/plain"),
        "md" => (ItemKind::Document, "text/markdown"),
        "docx" => (
            ItemKind::Document,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
        _ => return None,
    })
}

/// The title a kept note gets: its first non-empty line, cut at 80 characters.
pub fn text_title(text: &str) -> String {
    let first = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let mut title: String = first.chars().take(80).collect();
    if first.chars().count() > 80 {
        title.push('\u{2026}');
    }
    title
}

struct NewItem<'a> {
    kind: ItemKind,
    source: Source,
    sha: &'a str,
    ext: Option<&'a str>,
    mime: Option<&'a str>,
    file_name: Option<&'a str>,
    title: &'a str,
    text: Option<&'a str>,
    origin: Option<&'a str>,
}

/// A copy of the bytes in a temporary file, hashed, with what the row needs.
struct Stored {
    tmp: PathBuf,
    sha: String,
    ext: Option<String>,
    file_name: String,
    origin: Option<String>,
}

fn find_by_sha(tx: &Transaction<'_>, originals: &Path, sha: &str) -> Result<Option<ItemView>> {
    let id: Option<String> = tx
        .query_row("SELECT id FROM items WHERE sha256 = ?1", [sha], |r| {
            r.get(0)
        })
        .optional()?;
    match id {
        Some(id) => get_item_view(tx, originals, &id),
        None => Ok(None),
    }
}

/// Insert the row and return its id; the view is read after any file has been put in place.
fn insert_item(tx: &Transaction<'_>, now: &str, n: &NewItem<'_>) -> Result<String> {
    let id = new_id("itm");
    tx.execute(
        "INSERT INTO items(id, kind, source, trust, sha256, ext, mime, file_name, title, kept_at, \
         status, text, origin) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'kept', ?11, ?12)",
        params![
            id,
            enum_str(&n.kind),
            enum_str(&n.source),
            enum_str(&trust_for(n.source)),
            n.sha,
            n.ext,
            n.mime,
            n.file_name,
            n.title,
            now,
            n.text,
            n.origin,
        ],
    )?;
    Ok(id)
}

impl Engine {
    /// Keep a piece of text. One write transaction, no mind call.
    pub fn keep_text(&self, text: &str, source: Source) -> Result<Kept> {
        self.keep_text_from(text, source, None)
    }

    /// Keep text with the place it came from (a URL host or a sender), used to judge whether two
    /// sources are independent.
    pub fn keep_text_from(&self, text: &str, source: Source, origin: Option<&str>) -> Result<Kept> {
        if text.trim().is_empty() {
            return Err(Error::invalid("there is nothing to keep"));
        }
        let sha = sha256_hex(text.as_bytes());
        let now = fmt_ts(self.clock().now());
        let kind = if source == Source::PastedWeb {
            ItemKind::Web
        } else {
            ItemKind::Text
        };
        let originals = self.originals_dir();
        self.write(|tx| {
            if let Some(item) = find_by_sha(tx, &originals, &sha)? {
                return Ok(Kept::Duplicate { item });
            }
            let title = text_title(text);
            let id = insert_item(
                tx,
                &now,
                &NewItem {
                    kind,
                    source,
                    sha: &sha,
                    ext: None,
                    mime: None,
                    file_name: None,
                    title: &title,
                    text: Some(text),
                    origin,
                },
            )?;
            let item = get_item_view(tx, &originals, &id)?.ok_or_else(|| Error::not_found(&id))?;
            Ok(Kept::New { item })
        })
    }

    /// Keep a file: hash and copy outside the lock, then rename and insert under it.
    pub fn keep_file(&self, path: &Path, source: Source) -> Result<Kept> {
        let originals = self.originals_dir();
        std::fs::create_dir_all(&originals)?;
        let tmp = originals.join(format!(".tmp-{}", random_hex(8)));
        let sha = match copy_and_hash(path, &tmp) {
            Ok(sha) => sha,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        };
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .filter(|e| extension_ok(e));
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_string());
        let origin = path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .filter(|p| !p.is_empty());
        self.keep_stored(
            Stored {
                tmp,
                sha,
                ext,
                file_name,
                origin,
            },
            source,
        )
    }

    /// Keep bytes that never had a file: a pasted image or a recording. The extension must be
    /// one of the supported kinds (see [`kind_for_ext`]); the rest follows `keep_file`.
    pub fn keep_bytes(&self, bytes: &[u8], ext: &str, source: Source) -> Result<Kept> {
        let ext = ext.trim().trim_start_matches('.').to_lowercase();
        let Some((kind, _)) = kind_for_ext(&ext) else {
            return Err(Error::invalid(format!(
                "{ext:?} is not a kind of file Nocturne keeps"
            )));
        };
        if bytes.is_empty() {
            return Err(Error::invalid("there is nothing to keep"));
        }
        let originals = self.originals_dir();
        std::fs::create_dir_all(&originals)?;
        let tmp = originals.join(format!(".tmp-{}", random_hex(8)));
        let written = (|| -> Result<()> {
            let mut out = crate::fsutil::create_private_file(&tmp)?;
            std::io::Write::write_all(&mut out, bytes)?;
            out.sync_all()?;
            Ok(())
        })();
        if let Err(e) = written {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        let base = match (source, kind) {
            (Source::Screenshot, _) => "Screenshot",
            (Source::Recording, _) | (_, ItemKind::Audio) => "Recording",
            (_, ItemKind::Image) => "Pasted image",
            _ => "Pasted file",
        };
        self.keep_stored(
            Stored {
                tmp,
                sha: sha256_hex(bytes),
                file_name: format!("{base}.{ext}"),
                ext: Some(ext),
                origin: None,
            },
            source,
        )
    }

    /// Put a hashed temporary file in its place and insert its row, or recognise a duplicate.
    /// The temporary file is gone afterwards either way.
    fn keep_stored(&self, stored: Stored, source: Source) -> Result<Kept> {
        let Stored {
            tmp,
            sha,
            ext,
            file_name,
            origin,
        } = stored;
        let originals = self.originals_dir();
        let known = ext.as_deref().and_then(kind_for_ext);
        let kind = known.map(|k| k.0).unwrap_or(ItemKind::Document);
        let mime = known.map(|k| k.1);
        let dest = original_path(&originals, &sha, ext.as_deref());
        let now = fmt_ts(self.clock().now());

        let result = self.write(|tx| {
            if let Some(item) = find_by_sha(tx, &originals, &sha)? {
                return Ok(Kept::Duplicate { item });
            }
            let id = insert_item(
                tx,
                &now,
                &NewItem {
                    kind,
                    source,
                    sha: &sha,
                    ext: ext.as_deref(),
                    mime,
                    file_name: Some(&file_name),
                    title: &file_name,
                    text: None,
                    origin: origin.as_deref(),
                },
            )?;
            std::fs::rename(&tmp, &dest)?;
            let item = get_item_view(tx, &originals, &id)?.ok_or_else(|| Error::not_found(&id))?;
            Ok(Kept::New { item })
        });
        match &result {
            Ok(Kept::New { .. }) => {}
            _ => {
                let _ = std::fs::remove_file(&tmp);
            }
        }
        result
    }

    /// Items, newest first.
    pub fn items(&self, filter: ItemFilter) -> Result<Vec<ItemView>> {
        let originals = self.originals_dir();
        self.read(|conn| {
            let mut sql = format!("SELECT {ITEM_COLS} FROM items i WHERE 1=1");
            let mut args: Vec<String> = Vec::new();
            if let Some(since) = &filter.since {
                let since = since
                    .parse::<jiff::Timestamp>()
                    .map_err(|_| Error::invalid("since is not an RFC 3339 instant"))?;
                args.push(fmt_ts(since));
                sql.push_str(&format!(" AND i.kept_at >= ?{}", args.len()));
            }
            if let Some(status) = &filter.status {
                args.push(enum_str(status));
                sql.push_str(&format!(" AND i.status = ?{}", args.len()));
            }
            sql.push_str(" ORDER BY i.kept_at DESC, i.id");
            if let Some(limit) = filter.limit {
                sql.push_str(&format!(" LIMIT {limit}"));
            }
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(args.iter()),
                item_view(&originals),
            )?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    pub fn get_item(&self, id: &str) -> Result<ItemView> {
        let originals = self.originals_dir();
        self.read(|conn| get_item_view(conn, &originals, id))?
            .ok_or_else(|| Error::not_found(format!("item {id}")))
    }

    /// Put an item that could not be read back in the queue, at the front. An item that is
    /// already waiting is only moved to the front; anything else is left alone.
    pub fn retry_item(&self, id: &str) -> Result<()> {
        self.write(|tx| {
            let n = tx.execute(
                "UPDATE items SET status = 'kept', reason = NULL, lease_until = NULL, attempts = 0, \
                 failures = 0 \
                 WHERE id = ?1 AND status IN ('couldntRead', 'partlyUnderstood')",
                [id],
            )?;
            if n == 0 {
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)",
                    [id],
                    |r| r.get(0),
                )?;
                if !exists {
                    return Err(Error::not_found(format!("item {id}")));
                }
            }
            bump_priority(tx, id)
        })
    }

    /// The person pressed Read now: this waiting item is leased before the others. Items that
    /// are not waiting (being read, read, or unreadable) are unaffected.
    pub fn prioritize(&self, id: &str) -> Result<()> {
        self.write(|tx| {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)",
                [id],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(Error::not_found(format!("item {id}")));
            }
            bump_priority(tx, id)
        })
    }

    /// Items waiting to be read, oldest first, without leasing them (their `lease` is empty).
    pub fn pending_items(&self, limit: usize) -> Result<Vec<PendingItem>> {
        let dir = self.originals_dir();
        let now = fmt_ts(self.clock().now());
        self.read(|conn| pending(conn, &dir, Some(limit), None, &now))
    }

    /// Atomically take up to `limit` waiting items: they become `reading` under a lease token,
    /// so two workers never read the same item. Every later call about the item (`record_*`,
    /// `apply_candidates`, `fail_item`) must present the token. An expired lease puts the item
    /// back in the queue and voids its token.
    pub fn lease_pending(&self, limit: usize, lease: Duration) -> Result<Vec<PendingItem>> {
        let now = self.clock().now();
        let until = lease_until(now, lease)?;
        let dir = self.originals_dir();
        self.write(|tx| {
            reset_expired_leases(tx, &fmt_ts(now))?;
            let mut items = pending(tx, &dir, Some(limit), None, &fmt_ts(now))?;
            for item in &mut items {
                take_lease(tx, item, &fmt_ts(until))?;
            }
            Ok(items)
        })
    }

    /// Lease one particular item to read it again (a retry, or a re-read after the mind improved).
    /// Refused for an item that cannot be read, that someone else is reading, or that is part of
    /// a forget.
    pub fn lease_item(&self, id: &str, lease: Duration) -> Result<PendingItem> {
        self.lease_one(id, lease, false)
    }

    /// Like `lease_item`, for a read the person asked for ("Read it again"). An item something
    /// was forgotten from can be read again this way (the forgotten claims are still filtered
    /// out when the result is applied); automatic re-reads never are.
    pub fn lease_item_by_request(&self, id: &str, lease: Duration) -> Result<PendingItem> {
        self.lease_one(id, lease, true)
    }

    fn lease_one(&self, id: &str, lease: Duration, requested: bool) -> Result<PendingItem> {
        let now = self.clock().now();
        let until = lease_until(now, lease)?;
        let dir = self.originals_dir();
        self.write(|tx| {
            reset_expired_leases(tx, &fmt_ts(now))?;
            let status: Option<String> = tx
                .query_row("SELECT status FROM items WHERE id = ?1", [id], |r| r.get(0))
                .optional()?;
            match status.as_deref() {
                None => return Err(Error::not_found(format!("item {id}"))),
                Some("reading") | Some("couldntRead") => return Err(Error::LeaseLost),
                Some("kept") => {}
                // Reading an item again: refused after a forget unless the person asked.
                Some(_) if !requested => {
                    let forgotten: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM forgotten_claims WHERE item_id = ?1)",
                        [id],
                        |r| r.get(0),
                    )?;
                    if forgotten {
                        return Err(Error::LeaseLost);
                    }
                }
                Some(_) => {}
            }
            if in_forget_job(tx, id)? {
                return Err(Error::LeaseLost);
            }
            let mut items = pending(tx, &dir, None, Some(id), &fmt_ts(now))?;
            let mut item = items
                .pop()
                .ok_or_else(|| Error::not_found(format!("item {id}")))?;
            take_lease(tx, &mut item, &fmt_ts(until))?;
            Ok(item)
        })
    }

    /// Give a read back: the item returns to where it was (waiting, or read before for a
    /// re-read) and its token is void. `counted = false` gives the attempt back too (the mind
    /// went away, or an ask pre-empted it: not the item's fault).
    pub fn release_lease(&self, item: &str, lease: &str, counted: bool) -> Result<()> {
        self.write(|tx| {
            // Whoever holds the token may give it back, even when a forget has since taken the
            // item: it must not stay "being read" until the lease runs out.
            let held: Option<(String, Option<String>)> = tx
                .query_row(
                    "SELECT status, lease_token FROM items WHERE id = ?1",
                    [item],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            match held {
                None => return Err(Error::not_found(format!("item {item}"))),
                Some((status, token))
                    if status != "reading"
                        || lease.is_empty()
                        || token.as_deref() != Some(lease) =>
                {
                    return Err(Error::LeaseLost);
                }
                Some(_) => {}
            }
            tx.execute(
                "UPDATE items SET status = COALESCE(status_before, 'kept'), status_before = NULL, \
                 lease_until = NULL, lease_token = NULL, \
                 attempts = CASE WHEN ?2 THEN attempts ELSE MAX(attempts - 1, 0) END \
                 WHERE id = ?1",
                params![item, counted],
            )?;
            Ok(())
        })
    }

    /// Keep a read going: the lease now ends `lease` from now.
    pub fn renew_lease(&self, item: &str, lease: &str, extend: Duration) -> Result<()> {
        let until = fmt_ts(lease_until(self.clock().now(), extend)?);
        self.write(|tx| {
            check_lease(tx, item, lease)?;
            tx.execute(
                "UPDATE items SET lease_until = ?2 WHERE id = ?1",
                params![item, until],
            )?;
            Ok(())
        })
    }

    /// The mind failed on this item (`MindError::Failed`). Counts toward the item, apart from
    /// leases taken: the third failure makes it `couldntRead` (reason `corrupt`); before that
    /// it goes back to where it was.
    pub fn record_failure(&self, item: &str, lease: &str) -> Result<FailureOutcome> {
        self.write(|tx| {
            check_lease(tx, item, lease)?;
            tx.execute(
                "UPDATE items SET failures = failures + 1 WHERE id = ?1",
                [item],
            )?;
            let failures: u32 =
                tx.query_row("SELECT failures FROM items WHERE id = ?1", [item], |r| {
                    r.get(0)
                })?;
            if failures >= MAX_FAILURES {
                fail(tx, item, CouldntReadReason::Corrupt, None, None)?;
                return Ok(FailureOutcome::GaveUp);
            }
            tx.execute(
                "UPDATE items SET status = COALESCE(status_before, 'kept'), status_before = NULL, \
                 lease_until = NULL, lease_token = NULL WHERE id = ?1",
                [item],
            )?;
            Ok(FailureOutcome::Retry)
        })
    }

    /// A scanned document the mind could not read, but which has a title worth keeping: the
    /// item is `partlyUnderstood` under that title.
    pub fn mark_partly(&self, item: &str, lease: &str, title: &str) -> Result<()> {
        self.write(|tx| {
            check_lease(tx, item, lease)?;
            tx.execute(
                "UPDATE items SET status = 'partlyUnderstood', \
                 title = CASE WHEN ?2 = '' THEN title ELSE ?2 END, lease_until = NULL, \
                 lease_token = NULL, status_before = NULL, reread = 0 WHERE id = ?1",
                params![item, title.trim()],
            )?;
            Ok(())
        })
    }

    /// How long the reading of an item took, from picking it up to its memories being kept.
    /// Only an item that was understood, wholly or in part, keeps a time.
    pub fn record_reading_time(&self, item: &str, took: Duration) -> Result<()> {
        let ms = u32::try_from(took.as_millis()).unwrap_or(u32::MAX);
        let now = fmt_ts(self.clock().now());
        self.write(|tx| {
            tx.execute(
                "UPDATE items SET understood_in_ms = ?2, \
                 metadata = CASE WHEN json_extract(metadata, '$.journal') IS NOT NULL \
                   THEN json_set(metadata, '$.read_at', ?3) ELSE metadata END WHERE id = ?1 \
                 AND status IN ('understood', 'partlyUnderstood', 'needsYou', 'unconfirmed')",
                params![item, ms, now],
            )?;
            Ok(())
        })
    }

    /// Mark read items to be read again at low priority (after an upgrade or a tier change).
    /// The mark survives restarts and is cleared when the item is applied again. Items that
    /// are waiting, being read or unreadable are skipped, and so are items something was
    /// forgotten from: a better mind must not get a second chance to say it. Returns how many
    /// were marked.
    pub fn queue_reread(&self, ids: &[String]) -> Result<u32> {
        self.write(|tx| {
            let mut marked = 0u32;
            for id in ids {
                marked += tx.execute(
                    "UPDATE items SET reread = 1 WHERE id = ?1 AND reread = 0 \
                     AND status IN ('understood', 'partlyUnderstood', 'needsYou', 'unconfirmed') \
                     AND NOT EXISTS (SELECT 1 FROM forgotten_claims WHERE item_id = items.id)",
                    [id],
                )? as u32;
            }
            Ok(marked)
        })
    }

    /// Take the re-read mark off items (a re-read that keeps failing is given up on).
    pub fn clear_reread(&self, ids: &[String]) -> Result<()> {
        self.write(|tx| {
            for id in ids {
                tx.execute("UPDATE items SET reread = 0 WHERE id = ?1", [id])?;
            }
            Ok(())
        })
    }

    /// Items marked for a re-read, most recently kept first, that nobody is reading now. An
    /// item the person has forgotten something from is never re-read automatically.
    pub fn rereads(&self, limit: usize) -> Result<Vec<String>> {
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM items WHERE reread = 1 \
                 AND status IN ('understood', 'partlyUnderstood', 'needsYou', 'unconfirmed') \
                 AND NOT EXISTS (SELECT 1 FROM forgotten_claims WHERE item_id = items.id) \
                 AND NOT EXISTS (SELECT 1 FROM forget_jobs WHERE instr(items, '\"' || items.id || '\"') > 0) \
                 ORDER BY kept_at DESC, id LIMIT ?1",
            )?;
            let rows = stmt.query_map([limit.min(i64::MAX as usize) as i64], |r| r.get(0))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    /// Store a transcript for an audio item. A recording over two hours is kept but not read
    /// (`RecordOutcome::TooLong`, status `couldntRead`).
    pub fn record_transcript(
        &self,
        item: &str,
        lease: &str,
        t: Transcript,
    ) -> Result<RecordOutcome> {
        let meta = serde_json::json!({ "segments": t.segments });
        self.write(|tx| {
            check_lease(tx, item, lease)?;
            if t.duration_ms > MAX_AUDIO_MS {
                if is_reread(tx, item)? {
                    return Ok(RecordOutcome::TooLong);
                }
                fail(
                    tx,
                    item,
                    CouldntReadReason::TooLong,
                    Some(t.duration_ms),
                    None,
                )?;
                return Ok(RecordOutcome::TooLong);
            }
            tx.execute(
                "UPDATE items SET text = ?2, language = ?3, duration_ms = ?4, \
                 metadata = json_patch(metadata, ?5) WHERE id = ?1",
                params![item, t.text, t.language, t.duration_ms, meta.to_string()],
            )?;
            Ok(RecordOutcome::Recorded)
        })
    }

    /// Store what the mind read from an image or document. A document over 100 pages is kept but
    /// not read.
    pub fn record_understanding(
        &self,
        item: &str,
        lease: &str,
        u: Understanding,
    ) -> Result<RecordOutcome> {
        self.write(|tx| {
            check_lease(tx, item, lease)?;
            if u.pages.is_some_and(|p| p > MAX_PAGES) {
                if is_reread(tx, item)? {
                    return Ok(RecordOutcome::TooLong);
                }
                fail(tx, item, CouldntReadReason::TooLong, None, u.pages)?;
                return Ok(RecordOutcome::TooLong);
            }
            tx.execute(
                "UPDATE items SET title = CASE WHEN ?2 = '' THEN title ELSE ?2 END, text = ?3, \
                 summary = ?4, language = ?5, pages = ?6 WHERE id = ?1",
                params![item, u.title.trim(), u.text, u.summary, u.language, u.pages],
            )?;
            Ok(RecordOutcome::Recorded)
        })
    }

    /// Mark an item as one the mind could not read, with the reason.
    pub fn fail_item(&self, item: &str, lease: &str, reason: CouldntReadReason) -> Result<()> {
        self.write(|tx| {
            check_lease(tx, item, lease)?;
            fail(tx, item, reason, None, None)
        })
    }
}

/// The lease is a re-read of an item that was read before: a reading that fails must not
/// take away what the earlier one understood.
fn is_reread(conn: &rusqlite::Connection, item: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT status_before IS NOT NULL FROM items WHERE id = ?1",
        [item],
        |r| r.get(0),
    )?)
}

/// Move a waiting item ahead of everything waiting, the latest request first.
pub(crate) fn bump_priority(tx: &Transaction<'_>, id: &str) -> Result<()> {
    tx.execute(
        "UPDATE items SET priority = (SELECT COALESCE(MAX(priority), 0) + 1 FROM items) \
         WHERE id = ?1 AND status = 'kept'",
        [id],
    )?;
    Ok(())
}

fn lease_until(now: jiff::Timestamp, lease: Duration) -> Result<jiff::Timestamp> {
    now.checked_add(SignedDuration::try_from(lease).map_err(Error::invalid)?)
        .map_err(Error::invalid)
}

fn take_lease(tx: &Transaction<'_>, item: &mut PendingItem, until: &str) -> Result<()> {
    item.attempts += 1;
    item.lease = random_hex(16);
    tx.execute(
        "UPDATE items SET lease_epoch = forget_epoch, \
         status_before = CASE WHEN status IN ('kept', 'reading') THEN NULL ELSE status END, \
         status = 'reading', lease_until = ?2, lease_token = ?3, \
         attempts = attempts + 1, priority = 0 WHERE id = ?1",
        params![item.id, until, item.lease],
    )?;
    Ok(())
}

pub(crate) fn in_forget_job(conn: &rusqlite::Connection, item: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM forget_jobs WHERE instr(items, '\"' || ?1 || '\"') > 0)",
        [item],
        |r| r.get(0),
    )?)
}

/// The caller must hold the current lease on an item that is being read and is not part of a
/// forget.
pub(crate) fn check_lease(conn: &rusqlite::Connection, item: &str, lease: &str) -> Result<()> {
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT status, lease_token FROM items WHERE id = ?1",
            [item],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((status, token)) = row else {
        return Err(Error::not_found(format!("item {item}")));
    };
    if status != "reading" || lease.is_empty() || token.as_deref() != Some(lease) {
        return Err(Error::LeaseLost);
    }
    if in_forget_job(conn, item)? {
        return Err(Error::LeaseLost);
    }
    Ok(())
}

fn fail(
    tx: &Transaction<'_>,
    item: &str,
    reason: CouldntReadReason,
    duration_ms: Option<u32>,
    pages: Option<u32>,
) -> Result<()> {
    let n = tx.execute(
        "UPDATE items SET status = 'couldntRead', reason = ?2, lease_until = NULL, \
         lease_token = NULL, status_before = NULL, duration_ms = COALESCE(?3, duration_ms), \
         pages = COALESCE(?4, pages) WHERE id = ?1",
        params![item, enum_str(&reason), duration_ms, pages],
    )?;
    if n == 0 {
        return Err(Error::not_found(format!("item {item}")));
    }
    Ok(())
}

pub(crate) fn reset_expired_leases(conn: &rusqlite::Connection, now: &str) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE items SET status = COALESCE(status_before, 'kept'), status_before = NULL, \
         lease_until = NULL, lease_token = NULL \
         WHERE status = 'reading' AND (lease_until IS NULL OR lease_until <= ?1)",
        [now],
    )?)
}

/// Every item being read goes back to where it was. Used when the store is opened: one
/// instance owns it, so nobody can still be reading.
pub(crate) fn reset_all_leases(conn: &rusqlite::Connection) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE items SET status = COALESCE(status_before, 'kept'), status_before = NULL, \
         lease_until = NULL, lease_token = NULL WHERE status = 'reading'",
        [],
    )?)
}

/// Queue rows: waiting items (`status = 'kept'`) that are not held back for more writing, or
/// exactly the one named.
fn pending(
    conn: &rusqlite::Connection,
    dir: &Path,
    limit: Option<usize>,
    only: Option<&str>,
    now: &str,
) -> Result<Vec<PendingItem>> {
    let mut sql = String::from(
        "SELECT id, kind, source, trust, kept_at, sha256, ext, file_name, mime, text, attempts \
         FROM items WHERE ",
    );
    sql.push_str(if only.is_some() {
        "id = ?1"
    } else {
        "status = 'kept' AND (hold_until IS NULL OR hold_until <= ?2) \
         AND NOT EXISTS (SELECT 1 FROM forget_jobs WHERE instr(items, '\"' || items.id || '\"') > 0) \
         ORDER BY priority DESC, kept_at, id LIMIT ?1"
    });
    let mut stmt = conn.prepare(&sql)?;
    let map = |r: &rusqlite::Row<'_>| {
        let sha: String = r.get(5)?;
        let ext: Option<String> = r.get(6)?;
        let file_name: Option<String> = r.get(7)?;
        Ok(PendingItem {
            id: r.get(0)?,
            kind: crate::rows::de(r.get(1)?)?,
            source: crate::rows::de(r.get(2)?)?,
            trust: crate::rows::de(r.get(3)?)?,
            kept_at: r.get(4)?,
            original: file_name.map(|_| original_path(dir, &sha, ext.as_deref())),
            mime: r.get(8)?,
            text: r.get(9)?,
            attempts: r.get(10)?,
            lease: String::new(),
        })
    };
    let rows = match only {
        Some(id) => stmt
            .query_map([id], map)?
            .collect::<rusqlite::Result<_>>()?,
        None => stmt
            .query_map(
                params![
                    limit.unwrap_or(usize::MAX).min(i64::MAX as usize) as i64,
                    now
                ],
                map,
            )?
            .collect::<rusqlite::Result<_>>()?,
    };
    Ok(rows)
}

/// Whether an extension can be appended to an original's name without becoming a path or a
/// name another system refuses.
pub(crate) fn extension_ok(ext: &str) -> bool {
    !ext.is_empty()
        && ext.len() <= 190
        && ext != "."
        && ext != ".."
        && !ext.ends_with([' ', '.'])
        && !ext.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
}

/// Where an original lives: `originals/<sha256>.<ext>`.
pub(crate) fn original_path(dir: &Path, sha: &str, ext: Option<&str>) -> PathBuf {
    match ext {
        Some(ext) => dir.join(format!("{sha}.{ext}")),
        None => dir.join(sha),
    }
}

fn copy_and_hash(src: &Path, dest: &Path) -> Result<String> {
    let mut input = std::fs::File::open(src)?;
    let mut output = crate::fsutil::create_private_file(dest)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        std::io::Write::write_all(&mut output, &buf[..n])?;
    }
    output.sync_all()?;
    Ok(hex(&hasher.finalize()))
}
