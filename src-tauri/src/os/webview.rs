//! What the web view keeps outside Nocturne's data folder: its own storage and cache, named after
//! the app identifier. "Delete everything" removes these too, so nothing the interface held is
//! left behind. The list is made once at start from the real home folder and held in
//! [`super::state::OsState`], so tests never reach a real one.

use std::path::{Path, PathBuf};

/// Under macOS: `~/Library/WebKit/<id>`, `~/Library/Caches/<id>` and the saved window state.
pub fn macos_dirs(home: &Path, identifier: &str) -> Vec<PathBuf> {
    if !plain_name(identifier) {
        return Vec::new();
    }
    let library = home.join("Library");
    vec![
        library.join("WebKit").join(identifier),
        library.join("Caches").join(identifier),
        library
            .join("Saved Application State")
            .join(format!("{identifier}.savedState")),
    ]
}

/// Under Linux (WebKitGTK): the storage it keeps inside `<data home>/<id>/` beside Nocturne's own
/// folders, which are left to the engine's deletion, and `<cache home>/<id>`.
pub fn linux_dirs(data_home: &Path, cache_home: &Path, identifier: &str) -> Vec<PathBuf> {
    if !plain_name(identifier) {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = [
        "CacheStorage",
        "hsts-storage.sqlite",
        "mediakeys",
        "storage",
        "WebKitCache",
    ]
    .iter()
    .map(|name| data_home.join(identifier).join(name))
    .collect();
    out.push(cache_home.join(identifier));
    out
}

/// One folder name: not empty, no separator, not a way up.
fn plain_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".."
}

/// The folders to remove for this system, from the environment. Windows keeps its web view data
/// with the app's own data and has nothing extra.
pub fn default_dirs(identifier: &str) -> Vec<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(target_os = "macos") {
        return var("HOME").map_or_else(Vec::new, |home| macos_dirs(&home, identifier));
    }
    if cfg!(target_os = "linux") {
        let home = var("HOME");
        let data = var("XDG_DATA_HOME").or_else(|| home.as_ref().map(|h| h.join(".local/share")));
        let cache = var("XDG_CACHE_HOME").or_else(|| home.as_ref().map(|h| h.join(".cache")));
        if let (Some(data), Some(cache)) = (data, cache) {
            return linux_dirs(&data, &cache, identifier);
        }
    }
    Vec::new()
}

/// Remove each folder or file in `dirs`. A missing one is fine; a failure is logged and the rest
/// are still tried.
pub fn remove(dirs: &[PathBuf]) {
    for path in dirs {
        let result = match std::fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
            _ => std::fs::remove_file(path),
        };
        if let Err(e) = result {
            tracing::warn!("web view data could not be removed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_macos_folders_are_named_after_the_identifier_under_library() {
        let dirs = macos_dirs(Path::new("/home-x"), "app.nocturne.desktop");
        assert_eq!(
            dirs,
            [
                PathBuf::from("/home-x/Library/WebKit/app.nocturne.desktop"),
                PathBuf::from("/home-x/Library/Caches/app.nocturne.desktop"),
                PathBuf::from(
                    "/home-x/Library/Saved Application State/app.nocturne.desktop.savedState"
                ),
            ]
        );
    }

    #[test]
    fn the_linux_folders_leave_the_engines_own_folders_alone() {
        let dirs = linux_dirs(Path::new("/d"), Path::new("/c"), "app.nocturne.desktop");
        let names: Vec<_> = dirs
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"/c/app.nocturne.desktop".to_string()));
        assert!(names.contains(&"/d/app.nocturne.desktop/WebKitCache".to_string()));
        for own in ["store", "engine", "logs"] {
            assert!(
                !names.iter().any(|n| n.ends_with(&format!("/{own}"))),
                "{own}"
            );
        }
    }

    #[test]
    fn an_identifier_that_is_not_one_plain_name_gives_nothing() {
        for bad in ["", ".", "..", "a/b", "../x", "a\\b"] {
            assert!(macos_dirs(Path::new("/h"), bad).is_empty(), "{bad:?}");
            assert!(
                linux_dirs(Path::new("/d"), Path::new("/c"), bad).is_empty(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn removing_clears_what_is_there_and_skips_what_is_not() {
        let root = tempfile::tempdir().unwrap();
        let dirs = macos_dirs(root.path(), "app.nocturne.desktop");
        std::fs::create_dir_all(dirs[0].join("WebsiteData")).unwrap();
        std::fs::write(dirs[0].join("WebsiteData/x"), "x").unwrap();
        std::fs::create_dir_all(&dirs[2]).unwrap();
        let keep = root.path().join("Library/Caches/other.app");
        std::fs::create_dir_all(&keep).unwrap();
        remove(&dirs);
        assert!(dirs.iter().all(|d| !d.exists()));
        assert!(keep.exists(), "another app's folder is not touched");
        remove(&dirs);
    }
}
