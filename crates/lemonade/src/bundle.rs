//! Where the engine lives: the shipped bundle and the per-person working directories.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::RuntimeError;

const TEMP_PREFIX: &str = "tmp-";
const TEMP_KEPT: Duration = Duration::from_secs(24 * 3600);

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

    /// A new folder under `run` for the temporary files of one start of the engine, which Windows
    /// never cleans up by itself. Folders earlier starts left are removed once they have not
    /// changed for a day, so one that an engine still stopping uses is left alone.
    pub fn launch_temp(&self) -> Result<PathBuf, RuntimeError> {
        self.launch_temp_keeping(TEMP_KEPT)
    }

    fn launch_temp_keeping(&self, kept: Duration) -> Result<PathBuf, RuntimeError> {
        if let Ok(entries) = std::fs::read_dir(self.run()) {
            for entry in entries.flatten() {
                let old = entry.file_name().to_string_lossy().starts_with(TEMP_PREFIX)
                    && entry
                        .metadata()
                        .and_then(|m| m.modified())
                        .is_ok_and(|at| at.elapsed().unwrap_or_default() >= kept);
                if old {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
        let since = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let dir = self
            .run()
            .join(format!("{TEMP_PREFIX}{}", since.as_nanos()));
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

fn restrict(dir: &Path) -> Result<(), RuntimeError> {
    nocturne_engine::private::restrict_dir(dir)?;
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
    fn each_start_gets_its_own_temp_folder_and_old_ones_go() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = Dirs::new(tmp.path().join("bundle"), tmp.path().join("root"));
        dirs.ensure().unwrap();
        std::fs::write(dirs.run().join("lemond.pid"), "1").unwrap();
        let first = dirs.launch_temp().unwrap();
        std::fs::write(first.join("part"), "x").unwrap();
        let second = dirs.launch_temp().unwrap();
        assert_ne!(first, second);
        assert!(first.join("part").exists(), "a recent folder is left alone");
        let third = dirs.launch_temp_keeping(Duration::ZERO).unwrap();
        assert!(!first.exists() && !second.exists());
        assert!(third.is_dir());
        assert!(
            dirs.run().join("lemond.pid").exists(),
            "only temp folders go"
        );
    }

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
