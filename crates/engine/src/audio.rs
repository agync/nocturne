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

/// The pages whose lines cite any of these memories.
pub(crate) fn pages_citing(conn: &Connection, ids: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        let mut stmt = conn.prepare_cached(
            "SELECT DISTINCT l.page_date FROM page_line_memories pm \
             JOIN page_lines l ON l.id = pm.line_id WHERE pm.memory_id = ?1",
        )?;
        let rows = stmt.query_map([id], |r| r.get::<_, String>(0))?;
        for d in rows {
            let d = d?;
            if !out.contains(&d) {
                out.push(d);
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
    /// Keep the spoken edition of a page for a voice.
    pub fn cache_audio(&self, page_date: &str, voice: &str, bytes: &[u8]) -> Result<PathBuf> {
        let dir = self.inner.dir.join("audio");
        fsutil::create_private_dir(&dir)?;
        let path = dir.join(format!("{}-{}.audio", safe(page_date), safe(voice)));
        {
            use std::io::Write;
            let mut f = fsutil::create_private_file(&path)?;
            f.write_all(bytes)?;
            f.sync_all()?;
        }
        let stored = path.to_string_lossy().into_owned();
        self.write(|tx| {
            tx.execute(
                "INSERT OR REPLACE INTO audio_cache(page_date, voice, path) VALUES (?1, ?2, ?3)",
                params![page_date, voice, stored],
            )?;
            Ok(())
        })?;
        Ok(path)
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
}
