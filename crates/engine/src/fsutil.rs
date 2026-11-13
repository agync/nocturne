//! Private files: the data directory is for the person alone (mode 0700, files 0600 on Unix).

use std::fs::File;
use std::path::Path;

use crate::error::Result;

/// Create a directory (and parents) readable only by the owner.
pub(crate) fn create_private_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(path)?;
    Ok(())
}

/// Create (or truncate) a file readable only by the owner.
pub(crate) fn create_private_file(path: &Path) -> Result<File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    Ok(opts.open(path)?)
}

/// Make an existing file readable only by the owner. Missing files are ignored.
pub(crate) fn restrict_file(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Overwrite a file with zeros, then remove it. Best effort: a missing file is fine.
pub(crate) fn shred(path: &Path) {
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

/// Create one new directory, owner-only. Fails if the name exists in any form, a symlink
/// included, so nothing already there is followed or reused.
pub(crate) fn create_private_dir_new(path: &Path) -> std::io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(path)
}

/// Create one new file, owner-only, failing if the name exists (`O_CREAT | O_EXCL`, which does
/// not follow a symlink at that name).
pub(crate) fn create_private_file_new(path: &Path) -> std::io::Result<File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}
