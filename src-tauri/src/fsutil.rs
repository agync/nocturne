//! Private files: the data folder is for the person alone (folders 0700, files 0600 on Unix; an
//! owner-only access list on Windows, see `nocturne_engine::private`).

use std::path::Path;

/// Create a folder (and parents) readable only by the owner.
pub fn create_private_dir(path: &Path) -> std::io::Result<()> {
    nocturne_engine::private::create_dir(path)
}

/// Make a file readable only by the owner. Missing files are ignored.
pub fn restrict_file(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Write a file atomically with owner-only permissions.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    restrict_file(&tmp);
    std::fs::rename(&tmp, path)
}

/// Owner-only permissions on every file in a folder (log files are created with the process's
/// default mode, which this tightens).
pub fn restrict_folder_files(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            if e.file_type().is_ok_and(|t| t.is_file()) {
                restrict_file(&e.path());
            }
        }
    }
}
