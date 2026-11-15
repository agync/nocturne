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

/// Overwrite a file with zeros, then remove it. Best effort: a missing file is fine.
pub fn shred(path: &Path) {
    use std::io::Write;
    if let Ok(meta) = std::fs::metadata(path)
        && meta.is_file()
        && let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(path)
    {
        let zeros = vec![0u8; 64 * 1024];
        let mut left = meta.len();
        while left > 0 {
            let n = left.min(zeros.len() as u64) as usize;
            if f.write_all(&zeros[..n]).is_err() {
                break;
            }
            left -= n as u64;
        }
        let _ = f.sync_all();
    }
    let _ = std::fs::remove_file(path);
}

/// Overwrite and remove every file under a folder, then the folder itself. A missing folder is
/// fine. Best effort: errors are ignored.
pub fn shred_tree(path: &Path) {
    if let Ok(entries) = std::fs::read_dir(path) {
        for e in entries.flatten() {
            let p = e.path();
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                shred_tree(&p);
            } else {
                shred(&p);
            }
        }
    }
    let _ = std::fs::remove_dir(path);
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
