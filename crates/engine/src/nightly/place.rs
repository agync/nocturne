//! Where the locked pieces go: the person's own account, reached through [`BackupPlace`]. The
//! iCloud and Google Drive places live with the phone; [`MemoryPlace`] and [`FolderPlace`] stand
//! in for them in tests and checks.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use jiff::Timestamp;

/// Why the place did not do what was asked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlaceError {
    /// The account has no room left.
    #[error("there is no room in the account")]
    NoRoom,
    /// The account is signed out, turned off, or access was taken back.
    #[error("the account cannot be reached")]
    NoAccess,
    /// The phone is offline, or the account did not answer.
    #[error("the account did not answer")]
    Offline,
    #[error("the account refused: {0}")]
    Failed(String),
}

/// One file in the place, as its listing shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceEntry {
    pub name: String,
    pub size: u64,
    /// When the account says the file last changed, when it says.
    pub modified: Option<Timestamp>,
}

/// A flat folder of named files in the person's account. Names are short ASCII
/// (`[a-z0-9-]`). Every call blocks: call them off the async workers.
pub trait BackupPlace: Send + Sync {
    /// Write `bytes` under `name`, in place of any file there. A file shows under its name only
    /// once it is whole, and no earlier version of it stays in the account.
    ///
    /// This matters most for [`KEY_PHRASE`](super::KEY_PHRASE) and
    /// [`KEY_RECOVERY`](super::KEY_RECOVERY): "Change the phrase" and a new recovery key lock the
    /// same key again, so an earlier version left readable would keep opening every night with
    /// the old phrase or recovery key. A place answers `Ok` only once no earlier version of the
    /// file can be read: on Google Drive, after deleting every revision but the new one; in
    /// iCloud, after removing the file's other versions. If that fails, it answers an error, and
    /// the caller writes the file again.
    fn put(&self, name: &str, bytes: &[u8]) -> Result<(), PlaceError>;
    /// The file's bytes, or `None` when there is no such file.
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, PlaceError>;
    /// Every file whose name starts with `prefix`.
    fn list(&self, prefix: &str) -> Result<Vec<PlaceEntry>, PlaceError>;
    /// Remove the file, with any earlier versions the account kept. A file that is not there is
    /// fine.
    fn delete(&self, name: &str) -> Result<(), PlaceError>;
}

/// Whether `name` is one a place takes.
pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn check(name: &str) -> Result<(), PlaceError> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(PlaceError::Failed(format!(
            "{name:?} is not a name a place takes"
        )))
    }
}

/// A place in this process's memory, for tests. `fail` makes the next calls answer an error.
#[derive(Default)]
pub struct MemoryPlace {
    files: Mutex<BTreeMap<String, (Vec<u8>, Timestamp)>>,
    fail: Mutex<Option<PlaceError>>,
    puts: Mutex<u64>,
}

impl MemoryPlace {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every call answers `error` until this is called again with `None`.
    pub fn fail_with(&self, error: Option<PlaceError>) {
        *lock(&self.fail) = error;
    }

    /// How many files were written so far.
    pub fn puts(&self) -> u64 {
        *lock(&self.puts)
    }

    /// The names held now, in order.
    pub fn names(&self) -> Vec<String> {
        lock(&self.files).keys().cloned().collect()
    }

    /// Change a file's bytes in place, as a damaged or hostile account might.
    pub fn tamper(&self, name: &str, f: impl FnOnce(&mut Vec<u8>)) {
        if let Some((bytes, _)) = lock(&self.files).get_mut(name) {
            f(bytes);
        }
    }

    fn failing(&self) -> Result<(), PlaceError> {
        match lock(&self.fail).clone() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl BackupPlace for MemoryPlace {
    fn put(&self, name: &str, bytes: &[u8]) -> Result<(), PlaceError> {
        check(name)?;
        self.failing()?;
        lock(&self.files).insert(name.to_string(), (bytes.to_vec(), Timestamp::now()));
        *lock(&self.puts) += 1;
        Ok(())
    }

    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, PlaceError> {
        check(name)?;
        self.failing()?;
        Ok(lock(&self.files).get(name).map(|(b, _)| b.clone()))
    }

    fn list(&self, prefix: &str) -> Result<Vec<PlaceEntry>, PlaceError> {
        self.failing()?;
        Ok(lock(&self.files)
            .iter()
            .filter(|(n, _)| n.starts_with(prefix))
            .map(|(n, (b, at))| PlaceEntry {
                name: n.clone(),
                size: b.len() as u64,
                modified: Some(*at),
            })
            .collect())
    }

    fn delete(&self, name: &str) -> Result<(), PlaceError> {
        check(name)?;
        self.failing()?;
        lock(&self.files).remove(name);
        Ok(())
    }
}

/// A place in a folder on this machine, for checks by hand and tests across restarts.
pub struct FolderPlace {
    dir: PathBuf,
}

impl FolderPlace {
    /// The folder must exist.
    pub fn new(dir: &Path) -> Self {
        FolderPlace {
            dir: dir.to_path_buf(),
        }
    }
}

fn io(e: std::io::Error) -> PlaceError {
    if e.kind() == std::io::ErrorKind::StorageFull {
        PlaceError::NoRoom
    } else {
        PlaceError::Failed(e.to_string())
    }
}

impl BackupPlace for FolderPlace {
    fn put(&self, name: &str, bytes: &[u8]) -> Result<(), PlaceError> {
        check(name)?;
        let partial = self.dir.join(format!(".{name}.partial"));
        std::fs::write(&partial, bytes).map_err(io)?;
        std::fs::rename(&partial, self.dir.join(name)).map_err(io)
    }

    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, PlaceError> {
        check(name)?;
        match std::fs::read(self.dir.join(name)) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }

    fn list(&self, prefix: &str) -> Result<Vec<PlaceEntry>, PlaceError> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(&self.dir).map_err(io)? {
            let e = e.map_err(io)?;
            let Some(name) = e.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if !valid_name(&name) || !name.starts_with(prefix) {
                continue;
            }
            let meta = e.metadata().map_err(io)?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| Timestamp::try_from(t).ok());
            out.push(PlaceEntry {
                name,
                size: meta.len(),
                modified,
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn delete(&self, name: &str) -> Result<(), PlaceError> {
        check(name)?;
        match std::fs::remove_file(self.dir.join(name)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io(e)),
        }
    }
}
