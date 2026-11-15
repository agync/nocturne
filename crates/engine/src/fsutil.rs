//! Private files: the data directory is for the person alone (mode 0700, files 0600 on Unix; an
//! owner-only access list on Windows, see [`crate::private`]).

use std::fs::File;
use std::path::Path;

use crate::error::Result;

/// Create a directory (and parents) readable only by the owner.
pub(crate) fn create_private_dir(path: &Path) -> Result<()> {
    Ok(crate::private::create_dir(path)?)
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

/// Create one new directory, owner-only, in a place the person chose. Fails if the name exists
/// in any form, a symlink included, so nothing already there is followed or reused. Where the
/// file system cannot hold an access list the folder stays, the reason is logged and the answer
/// says it is not private.
pub(crate) fn create_chosen_dir_new(path: &Path) -> std::io::Result<crate::private::Privacy> {
    crate::private::create_chosen_dir_new(path)
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

/// Overwrite and remove every file under a folder, then the folder itself. A missing folder is
/// fine. Best effort: errors are ignored.
pub(crate) fn shred_tree(path: &Path) {
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
