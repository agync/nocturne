//! Where the engine lives: the shipped bundle and the per-person working directories.

use std::path::{Path, PathBuf};

use crate::error::RuntimeError;

/// The bundle holds the read-only `lemond`; the root holds everything the person owns:
/// `cache`, `config`, `models`, `logs` and `run`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    pub bundle: PathBuf,
    pub root: PathBuf,
}

impl Dirs {
    pub fn new(bundle: impl Into<PathBuf>, root: impl Into<PathBuf>) -> Self {
        Dirs {
            bundle: bundle.into(),
            root: root.into(),
        }
    }

    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn config(&self) -> PathBuf {
        self.root.join("config")
    }

    pub fn models(&self) -> PathBuf {
        self.root.join("models")
    }

    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Used as `XDG_RUNTIME_DIR` for the engine; owner-only.
    pub fn run(&self) -> PathBuf {
        self.root.join("run")
    }

    /// Creates every working directory, with `run` readable by its owner only.
    pub fn ensure(&self) -> Result<(), RuntimeError> {
        for dir in [
            self.cache(),
            self.config(),
            self.models(),
            self.logs(),
            self.run(),
        ] {
            std::fs::create_dir_all(&dir)?;
        }
        restrict(&self.run())?;
        Ok(())
    }
}

#[cfg(unix)]
fn restrict(dir: &Path) -> Result<(), RuntimeError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict(_dir: &Path) -> Result<(), RuntimeError> {
    Ok(())
}

/// Finds `lemond` (or `lemond.exe`) in the bundle, at its top or under `bin/`.
pub fn locate(bundle: &Path) -> Result<PathBuf, RuntimeError> {
    let names = ["lemond", "lemond.exe"];
    for dir in [bundle.to_path_buf(), bundle.join("bin")] {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(RuntimeError::BundleMissing(format!(
        "no lemond in {}",
        bundle.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locates_lemond_at_the_top_and_under_bin() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(
            locate(tmp.path()),
            Err(RuntimeError::BundleMissing(_))
        ));
        std::fs::create_dir(tmp.path().join("bin")).unwrap();
        std::fs::write(tmp.path().join("bin/lemond.exe"), b"x").unwrap();
        assert_eq!(
            locate(tmp.path()).unwrap(),
            tmp.path().join("bin/lemond.exe")
        );
        std::fs::write(tmp.path().join("lemond"), b"x").unwrap();
        assert_eq!(locate(tmp.path()).unwrap(), tmp.path().join("lemond"));
    }

    #[test]
    fn the_error_names_the_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let err = locate(tmp.path()).unwrap_err().to_string();
        assert!(err.contains("no lemond in"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn ensure_creates_dirs_and_locks_run() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let dirs = Dirs::new(tmp.path().join("b"), tmp.path().join("root"));
        std::fs::create_dir_all(dirs.run()).unwrap();
        std::fs::set_permissions(dirs.run(), std::fs::Permissions::from_mode(0o755)).unwrap();
        dirs.ensure().unwrap();
        for d in [dirs.cache(), dirs.config(), dirs.models(), dirs.logs()] {
            assert!(d.is_dir());
        }
        let mode = std::fs::metadata(dirs.run()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
}
