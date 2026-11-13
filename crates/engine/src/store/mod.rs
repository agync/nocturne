//! The SQLite store: connections, pragmas and ordered migrations.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::error::{Error, Result};

const SCHEMA: &str = include_str!("schema.sql");

/// Ordered migrations parsed from `schema.sql`, split on `-- migration N` lines.
pub fn migrations() -> Vec<(u32, &'static str)> {
    let mut out: Vec<(u32, &'static str)> = Vec::new();
    let mut current: Option<(u32, usize)> = None;
    let mut offset = 0usize;
    for line in SCHEMA.split_inclusive('\n') {
        if let Some(n) = line.trim().strip_prefix("-- migration ") {
            if let Some((version, start)) = current.take() {
                out.push((version, &SCHEMA[start..offset]));
            }
            let version: u32 = n.trim().parse().expect("migration marker has a number");
            current = Some((version, offset + line.len()));
        }
        offset += line.len();
    }
    if let Some((version, start)) = current {
        out.push((version, &SCHEMA[start..]));
    }
    out
}

/// The newest schema version this build knows.
pub fn latest_version() -> u32 {
    migrations().last().map(|m| m.0).unwrap_or(0)
}

pub fn schema_version(conn: &Connection) -> Result<u32> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

fn configure(conn: &Connection) -> Result<()> {
    conn.busy_timeout(Duration::from_millis(5000))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

/// Open the single writer connection, creating and migrating the store.
pub fn open(path: &Path) -> Result<Connection> {
    // Create the file first so it is private from the start; SQLite gives -wal and -shm the
    // same mode.
    if !path.exists() {
        crate::fsutil::create_private_file(path)?;
    }
    let mut conn = Connection::open(path)?;
    configure(&conn)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // Do not let the -wal file linger at its high-water size.
    conn.pragma_update(None, "journal_size_limit", 0)?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    // Freed pages are overwritten, so forgotten text does not linger in the file.
    conn.pragma_update(None, "secure_delete", "ON")?;
    migrate(&mut conn)?;
    restrict_store_files(path);
    Ok(conn)
}

/// Open a read-only connection (WAL readers never block the writer).
pub fn open_reader(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    configure(&conn)?;
    Ok(conn)
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let current = schema_version(conn)?;
    let known = latest_version();
    if current > known {
        return Err(Error::invalid(format!(
            "the store is version {current}, newer than this app understands ({known})"
        )));
    }
    for (version, sql) in migrations() {
        if version <= current {
            continue;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

/// The store, its write-ahead log and its shared-memory file are for the owner only.
pub(crate) fn restrict_store_files(db: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut name = db.as_os_str().to_owned();
        name.push(suffix);
        crate::fsutil::restrict_file(Path::new(&name));
    }
}
