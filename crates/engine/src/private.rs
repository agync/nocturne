//! Folders for the person alone. On Unix the mode is 0700. Windows has no modes: the folder gets
//! an access list that lets in the current user, SYSTEM and Administrators (the way root can read
//! a 0700 folder) and no one broader, which everything made inside it inherits. It is set with
//! the system's own `icacls`, since the workspace allows no `unsafe` for the security calls
//! themselves.

use std::io;
use std::path::Path;

/// Make `path` and any missing parents, each readable by its owner alone. On Windows the access
/// list is set only on the folders this call made, so a folder that already exists keeps what it
/// has. A folder that cannot be made private is removed again rather than left open.
pub fn create_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
        restrict_dir(path)
    }
    #[cfg(not(unix))]
    {
        let mut made = Vec::new();
        let mut at = Some(path);
        while let Some(p) = at.filter(|p| !p.as_os_str().is_empty() && !p.exists()) {
            made.push(p);
            at = p.parent();
        }
        std::fs::create_dir_all(path)?;
        for dir in made.iter().rev() {
            if let Err(e) = restrict_dir(dir) {
                for dir in &made {
                    let _ = std::fs::remove_dir(dir);
                }
                return Err(e);
            }
        }
        Ok(())
    }
}

/// Whether a folder ended up closed to others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Privacy {
    Private,
    /// The volume cannot hold an access list, so the folder is as open as the volume is.
    NotPrivate,
}

/// Make exactly `path`, owner-only, and remove it again if its access list cannot be set. Fails
/// if the name exists in any form, a symlink included, so nothing already there is followed or
/// reused.
pub fn create_dir_new(path: &Path) -> io::Result<()> {
    create_new_with(path, &restrict_acl, &|_| false).map(|_| ())
}

/// [`create_dir_new`] for a place the person chose, such as a drive for an export. A volume with
/// no access lists (FAT, FAT32, exFAT) keeps the folder, logs why and answers
/// [`Privacy::NotPrivate`], because the person chose that place. Any other failure removes the
/// folder and is returned.
pub fn create_chosen_dir_new(path: &Path) -> io::Result<Privacy> {
    create_new_with(path, &restrict_acl, &has_no_access_lists)
}

fn create_new_with(
    path: &Path,
    restrict: &dyn Fn(&Path) -> io::Result<()>,
    has_no_acls: &dyn Fn(&Path) -> bool,
) -> io::Result<Privacy> {
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut b = std::fs::DirBuilder::new();
        b.mode(0o700);
        b
    };
    #[cfg(not(unix))]
    let builder = std::fs::DirBuilder::new();
    builder.create(path)?;
    match restrict(path) {
        Ok(()) => Ok(Privacy::Private),
        Err(e) if has_no_acls(path) => {
            tracing::warn!("a folder is not private, its volume keeps no access lists: {e}");
            Ok(Privacy::NotPrivate)
        }
        Err(e) => {
            let _ = std::fs::remove_dir(path);
            Err(e)
        }
    }
}

/// Whether the volume of `path` is FAT, FAT32 or exFAT. The file system's own name is read, which
/// Windows does not translate, where `icacls` messages and exit codes are the same for many
/// causes. Only a lettered drive is asked: a share or an unreadable answer counts as one with
/// access lists, so the failure stays fatal.
fn has_no_access_lists(path: &Path) -> bool {
    #[cfg(windows)]
    {
        windows::volume_format(path)
            .is_some_and(|f| ["FAT", "FAT32", "EXFAT"].contains(&f.to_ascii_uppercase().as_str()))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

/// The access list alone: Unix has the mode set when the folder is made.
fn restrict_acl(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        windows::owner_only(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(())
    }
}

/// What `icacls` said, from its two streams, as one trimmed line. Bytes outside UTF-8 become
/// replacement characters; `icacls` writes in the console's own code page.
#[cfg(any(windows, test))]
fn icacls_said(stdout: &[u8], stderr: &[u8]) -> String {
    [stdout, stderr]
        .into_iter()
        .map(|bytes| String::from_utf8_lossy(bytes).trim().to_string())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod said_tests {
    use super::icacls_said;

    #[test]
    fn what_icacls_said_reads_both_streams_and_skips_the_empty_ones() {
        assert_eq!(icacls_said(b"", b""), "");
        assert_eq!(icacls_said(b" \r\n", b"\t\n"), "");
        assert_eq!(
            icacls_said(b"processed file: x\r\n", b""),
            "processed file: x"
        );
        assert_eq!(
            icacls_said(b"", b"Access is denied.\n"),
            "Access is denied."
        );
        assert_eq!(
            icacls_said(b"processed file: x\n", b"Access is denied.\n"),
            "processed file: x Access is denied."
        );
    }

    #[test]
    fn text_outside_utf8_is_reported_with_replacement_characters() {
        let said = icacls_said(b"", &[0xff, b'x']);
        assert!(said.ends_with('x') && said.contains('\u{fffd}'), "{said}");
    }
}

/// Leave an existing folder readable by its owner alone.
pub fn restrict_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
    }
    #[cfg(windows)]
    {
        windows::owner_only(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn folders_are_owner_only_and_a_new_one_is_never_reused() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("a").join("b");
        create_dir(&nested).unwrap();
        assert_eq!((mode(&tmp.path().join("a")), mode(&nested)), (0o700, 0o700));
        let fresh = tmp.path().join("fresh");
        create_dir_new(&fresh).unwrap();
        assert_eq!(mode(&fresh), 0o700);
        assert!(create_dir_new(&fresh).is_err());
    }

    fn refuses(_: &Path) -> io::Result<()> {
        Err(io::Error::other("no access lists here"))
    }

    #[test]
    fn a_volume_without_access_lists_keeps_the_folder_and_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        let chosen = tmp.path().join("chosen");
        let got = create_new_with(&chosen, &refuses, &|_| true).unwrap();
        assert_eq!(got, Privacy::NotPrivate);
        assert!(chosen.is_dir());
        assert!(create_new_with(&chosen, &refuses, &|_| true).is_err());
    }

    #[test]
    fn any_other_failure_removes_the_folder_and_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let ours = tmp.path().join("ours");
        assert!(create_new_with(&ours, &refuses, &|_| false).is_err());
        assert!(!ours.exists());
        let fine = tmp.path().join("fine");
        assert_eq!(
            create_new_with(&fine, &|_| Ok(()), &|_| true).unwrap(),
            Privacy::Private
        );
    }
}

#[cfg(windows)]
mod windows {
    use std::io;
    use std::path::Path;
    use std::sync::OnceLock;

    use super::icacls_said;
    use crate::process::command;

    /// The current user's security identifier, from `whoami /user`. Only a success is kept, so a
    /// failure that passes is tried again.
    fn user_sid() -> io::Result<&'static str> {
        static SID: OnceLock<String> = OnceLock::new();
        if let Some(sid) = SID.get() {
            return Ok(sid);
        }
        let out = command("whoami")
            .args(["/user", "/fo", "csv", "/nh"])
            .output()
            .map_err(|e| io::Error::other(format!("the current user could not be read: {e}")))?;
        let sid = sid_from(&String::from_utf8_lossy(&out.stdout)).ok_or_else(|| {
            io::Error::other(format!(
                "the current user could not be read: whoami {}",
                out.status
            ))
        })?;
        Ok(SID.get_or_init(|| sid))
    }

    /// The file system name of the lettered drive `path` is on (`NTFS`, `FAT32`, `exFAT`), from
    /// .NET's `DriveInfo`, which needs no administrator. `None` for a share or a failed query.
    pub(super) fn volume_format(path: &Path) -> Option<String> {
        use std::path::{Component, Prefix};
        let letter = match path.components().next()? {
            Component::Prefix(p) => match p.kind() {
                Prefix::Disk(l) | Prefix::VerbatimDisk(l) => l as char,
                _ => return None,
            },
            _ => return None,
        };
        if !letter.is_ascii_alphabetic() {
            return None;
        }
        let script = format!("(New-Object System.IO.DriveInfo '{letter}').DriveFormat");
        let out = command("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// The last quoted field of `whoami /user /fo csv /nh`: `"name","S-1-5-..."`.
    pub(super) fn sid_from(csv: &str) -> Option<String> {
        let sid = csv.trim().rsplit(',').next()?.trim_matches('"');
        sid.starts_with("S-1-").then(|| sid.to_string())
    }

    /// Drops what the folder inherits, then grants the user, SYSTEM and Administrators full
    /// control, inherited by the folders and files inside, and removes the broad groups.
    pub(super) fn owner_only(path: &Path) -> io::Result<()> {
        let user = format!("*{}:(OI)(CI)F", user_sid()?);
        icacls(path, &["/inheritance:r"])?;
        for grant in [user.as_str(), SYSTEM, ADMINISTRATORS] {
            icacls(path, &["/grant:r", grant])?;
        }
        for sid in BROAD {
            icacls(path, &["/remove:g", sid])?;
        }
        Ok(())
    }

    /// Like root on Unix, which a mode 0700 folder keeps out of no one, SYSTEM and Administrators
    /// stay: Defender, the search indexer, shadow copies and backup agents run as SYSTEM.
    const SYSTEM: &str = "*S-1-5-18:(OI)(CI)F";
    const ADMINISTRATORS: &str = "*S-1-5-32-544:(OI)(CI)F";

    /// Everyone, Authenticated Users and Users: broad groups a folder may hold for itself.
    const BROAD: [&str; 3] = ["*S-1-1-0", "*S-1-5-11", "*S-1-5-32-545"];

    /// Output is captured rather than inherited: every call otherwise writes its summary to the
    /// terminal, which lands in the output of every test that makes a folder private.
    fn icacls(path: &Path, args: &[&str]) -> io::Result<()> {
        let out = command("icacls").arg(path).args(args).arg("/q").output()?;
        if out.status.success() {
            return Ok(());
        }
        let said = icacls_said(&out.stdout, &out.stderr);
        let detail = if said.is_empty() {
            String::new()
        } else {
            format!("; it said: {said}")
        };
        Err(io::Error::other(format!(
            "the folder could not be made private: icacls {}{detail}",
            out.status
        )))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_file_system_of_a_drive_is_named() {
            let tmp = tempfile::tempdir().unwrap();
            let name = volume_format(tmp.path()).expect("a name for the temp drive");
            assert!(!has_no_acls_name(&name), "{name}");
            assert_eq!(volume_format(Path::new(r"\\server\share\x")), None);
        }

        fn has_no_acls_name(name: &str) -> bool {
            ["FAT", "FAT32", "EXFAT"].contains(&name.to_ascii_uppercase().as_str())
        }

        #[test]
        fn the_sid_is_read_from_whoami() {
            let csv = "\"pc\\\\someone\",\"S-1-5-21-1-2-3-1001\"\r\n";
            assert_eq!(sid_from(csv).as_deref(), Some("S-1-5-21-1-2-3-1001"));
            assert_eq!(sid_from("nothing"), None);
        }

        /// The folder lists the user, SYSTEM and Administrators, inherited by what is made in it,
        /// and nothing broader or inherited from its parent. A parent that `create_dir` makes is private too.
        #[test]
        fn a_private_folder_lets_in_the_user_and_the_system_only() {
            let tmp = tempfile::tempdir().unwrap();
            let dir = tmp.path().join("private");
            super::super::create_dir_new(&dir).unwrap();
            std::fs::write(dir.join("inside"), "x").unwrap();
            let nested = tmp.path().join("outer").join("inner");
            super::super::create_dir(&nested).unwrap();
            for target in [
                dir.clone(),
                dir.join("inside"),
                nested.clone(),
                nested.parent().unwrap().to_path_buf(),
            ] {
                let out = command("icacls").arg(&target).output().unwrap();
                let up = command("icacls").arg(tmp.path()).output().unwrap();
                let listing = String::from_utf8_lossy(&out.stdout).to_string();
                let parent = String::from_utf8_lossy(&up.stdout).to_string();
                // The first line starts with the path, which may hold names such as `Users`.
                let shown = target.display().to_string();
                let entries: Vec<&str> = listing
                    .lines()
                    .map(|l| l.trim().strip_prefix(shown.as_str()).unwrap_or(l).trim())
                    .filter(|l| l.contains(":("))
                    .collect();
                let note = format!("{listing}\nparent:\n{parent}");
                assert_eq!(entries.len(), 3, "{note}");
                for name in ["SYSTEM", "BUILTIN\\Administrators"] {
                    assert_eq!(
                        entries.iter().filter(|e| e.contains(name)).count(),
                        1,
                        "{note}"
                    );
                }
                for entry in &entries {
                    assert_eq!(entry.contains("(OI)(CI)"), target.is_dir(), "{note}");
                    for broad in ["Everyone", "BUILTIN\\Users", "Authenticated"] {
                        assert!(!entry.contains(broad), "{note}");
                    }
                }
            }
        }
    }
}
