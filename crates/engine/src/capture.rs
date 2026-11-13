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
use crate::types::{PendingItem, RecordOutcome};
use crate::util::{enum_str, fmt_ts, hex, new_id, random_hex, sha256_hex};

/// Recordings longer than this are kept but not read.
pub const MAX_AUDIO_MS: u32 = 2 * 60 * 60 * 1000;
/// Documents longer than this are kept but not read.
pub const MAX_PAGES: u32 = 100;

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

fn title_of(text: &str) -> String {
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
            let title = title_of(text);
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
            .filter(|e| !e.is_empty());
        let known = ext.as_deref().and_then(kind_for_ext);
        let kind = known.map(|k| k.0).unwrap_or(ItemKind::Document);
        let mime = known.map(|k| k.1);
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_string());
        let origin = path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .filter(|p| !p.is_empty());
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

    /// Put an item that could not be read back in the queue.
    pub fn retry_item(&self, id: &str) -> Result<()> {
        self.write(|tx| {
            let n = tx.execute(
                "UPDATE items SET status = 'kept', reason = NULL, lease_until = NULL, attempts = 0 \
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
            Ok(())
        })
    }

    /// Items waiting to be read, oldest first, without leasing them (their `lease` is empty).
    pub fn pending_items(&self, limit: usize) -> Result<Vec<PendingItem>> {
        let dir = self.originals_dir();
        self.read(|conn| pending(conn, &dir, Some(limit), None))
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
            let mut items = pending(tx, &dir, Some(limit), None)?;
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
                _ => {}
            }
            if in_forget_job(tx, id)? {
                return Err(Error::LeaseLost);
            }
            let mut items = pending(tx, &dir, None, Some(id))?;
            let mut item = items
                .pop()
                .ok_or_else(|| Error::not_found(format!("item {id}")))?;
            take_lease(tx, &mut item, &fmt_ts(until))?;
            Ok(item)
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

fn lease_until(now: jiff::Timestamp, lease: Duration) -> Result<jiff::Timestamp> {
    now.checked_add(SignedDuration::try_from(lease).map_err(Error::invalid)?)
        .map_err(Error::invalid)
}

fn take_lease(tx: &Transaction<'_>, item: &mut PendingItem, until: &str) -> Result<()> {
    item.attempts += 1;
    item.lease = random_hex(16);
    tx.execute(
        "UPDATE items SET status = 'reading', lease_until = ?2, lease_token = ?3, \
         attempts = attempts + 1 WHERE id = ?1",
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
         lease_token = NULL, duration_ms = COALESCE(?3, duration_ms), \
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
        "UPDATE items SET status = 'kept', lease_until = NULL, lease_token = NULL \
         WHERE status = 'reading' AND (lease_until IS NULL OR lease_until <= ?1)",
        [now],
    )?)
}

/// Queue rows: waiting items (`status = 'kept'`), or exactly the one named.
fn pending(
    conn: &rusqlite::Connection,
    dir: &Path,
    limit: Option<usize>,
    only: Option<&str>,
) -> Result<Vec<PendingItem>> {
    let mut sql = String::from(
        "SELECT id, kind, source, trust, kept_at, sha256, ext, file_name, mime, text, attempts \
         FROM items WHERE ",
    );
    sql.push_str(if only.is_some() {
        "id = ?1"
    } else {
        "status = 'kept' ORDER BY kept_at, id LIMIT ?1"
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
                [limit.unwrap_or(usize::MAX).min(i64::MAX as usize) as i64],
                map,
            )?
            .collect::<rusqlite::Result<_>>()?,
    };
    Ok(rows)
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
