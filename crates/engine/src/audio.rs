//! Cached spoken editions. They are derived data: forgetting a memory, or making it private,
//! removes the audio of every page that cited it.

use std::path::PathBuf;

use rusqlite::{Connection, params};

use crate::Engine;
use crate::error::Result;
use crate::fsutil;

fn safe(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The pages that cite any of these memories anywhere: in a line, in a choice, or as the daily
/// question (the saved snapshot is searched for the id, so nothing the snapshot names is missed).
pub(crate) fn pages_citing(conn: &Connection, ids: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        for sql in [
            "SELECT DISTINCT l.page_date FROM page_line_memories pm \
             JOIN page_lines l ON l.id = pm.line_id WHERE pm.memory_id = ?1",
            "SELECT page_date FROM questions WHERE memory_id = ?1",
            "SELECT date FROM pages WHERE instr(body, ?1) > 0",
        ] {
            let mut stmt = conn.prepare_cached(sql)?;
            let rows = stmt.query_map([id], |r| r.get::<_, String>(0))?;
            for d in rows {
                let d = d?;
                if !out.contains(&d) {
                    out.push(d);
                }
            }
        }
    }
    Ok(out)
}

/// Remove the cache rows for these pages and return the files to delete once the transaction
/// has committed.
pub(crate) fn drop_audio_rows(conn: &Connection, dates: &[String]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for d in dates {
        let mut stmt = conn.prepare_cached("SELECT path FROM audio_cache WHERE page_date = ?1")?;
        let rows = stmt.query_map([d], |r| r.get::<_, String>(0))?;
        for p in rows {
            files.push(PathBuf::from(p?));
        }
        conn.execute("DELETE FROM audio_cache WHERE page_date = ?1", [d])?;
    }
    Ok(files)
}

pub(crate) fn shred_all(files: Vec<PathBuf>) {
    for f in files {
        fsutil::shred(&f);
    }
}

impl Engine {
    /// A stamp for the spoken edition of `page_date`. Take it before reading the page to speak,
    /// hand it back with the audio. It changes whenever the page's content may have changed or
    /// something it cites was forgotten, made private, corrected or re-read, so audio made from
    /// stale content is refused rather than cached. (One counter covers every page: refusing
    /// too much is safe, caching stale speech is not.)
    pub fn audio_stamp(&self, _page_date: &str) -> u64 {
        self.inner
            .audio_gen
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Keep the spoken edition of a page for a voice, if `stamp` is still current. Returns
    /// `None` (and keeps nothing) when the page changed since the stamp was taken.
    pub fn cache_audio(
        &self,
        page_date: &str,
        voice: &str,
        bytes: &[u8],
        stamp: u64,
    ) -> Result<Option<PathBuf>> {
        let dir = self.inner.dir.join("audio");
        fsutil::create_private_dir(&dir)?;
        // `.wav`: the asset protocol and the web view pick how to play a file from its extension.
        let path = dir.join(format!("{}-{}.wav", safe(page_date), safe(voice)));
        let stored = path.to_string_lossy().into_owned();
        // Checked under the write lock, which every change to what audio may say also holds, so
        // nothing can slip in between the check and the file.
        let ok = self.write(|tx| {
            if self.audio_stamp(page_date) != stamp {
                return Ok(false);
            }
            {
                use std::io::Write;
                let mut f = fsutil::create_private_file(&path)?;
                f.write_all(bytes)?;
                f.sync_all()?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO audio_cache(page_date, voice, path) VALUES (?1, ?2, ?3)",
                params![page_date, voice, stored],
            )?;
            Ok(true)
        })?;
        Ok(ok.then_some(path))
    }

    /// Remove the spoken editions of pages older than `keep_days` days: they are made again
    /// when asked for, and a morning's worth is megabytes. Returns how many files went.
    pub fn prune_audio(&self, keep_days: i64) -> Result<usize> {
        let today: jiff::civil::Date = self.today().parse()?;
        let cutoff = today
            .checked_sub(jiff::Span::new().days(keep_days))
            .map_err(|e| crate::error::Error::invalid(e.to_string()))?
            .to_string();
        let old: Vec<String> = self.read(|conn| {
            let mut stmt =
                conn.prepare("SELECT DISTINCT page_date FROM audio_cache WHERE page_date < ?1")?;
            let rows = stmt.query_map([&cutoff], |r| r.get::<_, String>(0))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })?;
        if old.is_empty() {
            return Ok(0);
        }
        let files = self.write(|tx| drop_audio_rows(tx, &old))?;
        let count = files.len();
        shred_all(files);
        Ok(count)
    }

    /// The cached spoken edition, when there is one.
    pub fn cached_audio(&self, page_date: &str, voice: &str) -> Result<Option<PathBuf>> {
        self.read(|conn| {
            let p: Option<String> = conn
                .query_row(
                    "SELECT path FROM audio_cache WHERE page_date = ?1 AND voice = ?2",
                    params![page_date, voice],
                    |r| r.get(0),
                )
                .ok();
            Ok(p.map(PathBuf::from).filter(|p| p.exists()))
        })
    }

    /// Shred files in `originals/` and `audio/` that no row accounts for: leftovers of a crash
    /// between a commit and a removal, or of a copy that never finished (`.tmp-*`).
    pub(crate) fn sweep_orphan_files(&self) -> Result<()> {
        let (shas, audio): (Vec<String>, Vec<String>) = self.read(|conn| {
            let mut stmt = conn.prepare("SELECT sha256 FROM items WHERE file_name IS NOT NULL")?;
            let shas = stmt
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let mut stmt = conn.prepare("SELECT path FROM audio_cache")?;
            let paths: Vec<String> = stmt
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let names = paths
                .iter()
                .filter_map(|p| std::path::Path::new(p).file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .collect();
            Ok((shas, names))
        })?;
        if let Ok(entries) = std::fs::read_dir(self.originals_dir()) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let stem = name.split('.').next().unwrap_or("");
                if name.starts_with(".tmp-") || !shas.iter().any(|s| s == stem) {
                    self.bump_forget();
                    fsutil::shred(&e.path());
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(self.inner.dir.join("audio")) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if !audio.contains(&name) {
                    fsutil::shred(&e.path());
                }
            }
        }
        Ok(())
    }
}
