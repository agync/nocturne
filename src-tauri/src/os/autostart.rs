//! Start with this computer.
//!
//! On Linux Nocturne writes its own autostart entry: the plugin would start the running
//! executable, which under an AppImage is a temporary mount, and it does not quote the path. The
//! entry passes `--hidden`, so a start at login goes to the tray (and shows the window when
//! there is no tray). On Windows the autostart plugin does the work.
//!
//! Every function that writes takes the folder as an argument, so tests use a temporary one.

use std::path::{Path, PathBuf};

/// The entry's file name: the app identifier and the `.desktop` suffix.
pub const ENTRY: &str = "app.nocturne.desktop.desktop";

/// What reconciling did, or would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Write,
    Remove,
    Nothing,
}

/// Where the entry goes and what it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub dir: PathBuf,
    pub exec: PathBuf,
}

impl Target {
    /// From the environment: `$XDG_CONFIG_HOME/autostart` (else `~/.config/autostart`) and the
    /// AppImage or the running executable.
    pub fn from_env() -> Option<Target> {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .filter(|v| !v.is_empty())
                    .map(|h| PathBuf::from(h).join(".config"))
            })?;
        let exec = std::env::var_os("APPIMAGE")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::current_exe().ok())?;
        Some(Target {
            dir: config.join("autostart"),
            exec,
        })
    }
}

/// Whether a program path can go in an `Exec` line at all: it must be text, and a line break
/// would end the line and let the rest be read as another key.
pub fn exec_is_safe(exec: &Path) -> bool {
    exec.to_str()
        .is_some_and(|s| !s.is_empty() && !s.contains(['\n', '\r', '\0']))
}

/// An `Exec` value for the desktop entry spec: quoted when it has anything the spec reserves,
/// with `%` doubled so it is not read as a field code.
fn exec_field(exec: &Path) -> String {
    let raw = exec.to_string_lossy();
    let reserved = " \t\n\"'\\><~|&;$*?#()`";
    let quoted = raw.chars().any(|c| reserved.contains(c));
    let mut out = String::new();
    for c in raw.chars() {
        match c {
            '%' => out.push_str("%%"),
            // Escaped for the quoting rules, then again for the string value.
            '"' | '`' | '$' if quoted => {
                out.push_str("\\\\");
                out.push(c);
            }
            '\\' if quoted => out.push_str("\\\\\\\\"),
            _ => out.push(c),
        }
    }
    if quoted { format!("\"{out}\"") } else { out }
}

/// The entry for a program.
pub fn desktop_entry(exec: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Nocturne\nExec={} --hidden\nTerminal=false\nStartupNotify=false\nX-GNOME-Autostart-enabled=true\n",
        exec_field(exec)
    )
}

/// What has to happen so the entry in `dir` matches `want`.
pub fn reconcile(want: bool, dir: &Path, exec: &Path) -> Action {
    let existing = std::fs::read_to_string(dir.join(ENTRY)).ok();
    match (want, existing) {
        (true, Some(current)) if current == desktop_entry(exec) => Action::Nothing,
        (true, _) => Action::Write,
        (false, Some(_)) => Action::Remove,
        (false, None) => Action::Nothing,
    }
}

/// Make the entry in `target.dir` match `want`. Touches that folder and nothing else.
pub fn apply(want: bool, target: &Target) -> std::io::Result<Action> {
    if want && !exec_is_safe(&target.exec) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the program path cannot be written to an autostart entry",
        ));
    }
    let action = reconcile(want, &target.dir, &target.exec);
    let path = target.dir.join(ENTRY);
    match action {
        Action::Write => {
            std::fs::create_dir_all(&target.dir)?;
            std::fs::write(&path, desktop_entry(&target.exec))?;
        }
        Action::Remove => std::fs::remove_file(&path)?,
        Action::Nothing => {}
    }
    Ok(action)
}

/// Whether an entry is there.
pub fn enabled(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join(ENTRY)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_quotes_spaces_and_percent() {
        let entry = desktop_entry(Path::new("/opt/My Apps/Noc%turne"));
        assert!(
            entry.contains("Exec=\"/opt/My Apps/Noc%%turne\" --hidden\n"),
            "{entry}"
        );
        let plain = desktop_entry(Path::new("/usr/bin/nocturne"));
        assert!(
            plain.contains("Exec=/usr/bin/nocturne --hidden\n"),
            "{plain}"
        );
        let tricky = desktop_entry(Path::new("/opt/a \"b\"/n"));
        assert!(
            tricky.contains("Exec=\"/opt/a \\\\\"b\\\\\"/n\" --hidden\n"),
            "{tricky}"
        );
        assert!(entry.starts_with("[Desktop Entry]\n"));
    }

    fn target(dir: &Path) -> Target {
        Target {
            dir: dir.join("autostart"),
            exec: PathBuf::from("/usr/bin/nocturne"),
        }
    }

    #[test]
    fn reconcile_table() {
        let dir = tempfile::tempdir().unwrap();
        let t = target(dir.path());
        assert_eq!(
            reconcile(true, &t.dir, &t.exec),
            Action::Write,
            "on and missing"
        );
        assert_eq!(
            reconcile(false, &t.dir, &t.exec),
            Action::Nothing,
            "off and missing"
        );
        std::fs::create_dir_all(&t.dir).unwrap();
        std::fs::write(
            t.dir.join(ENTRY),
            desktop_entry(Path::new("/gone/old-nocturne")),
        )
        .unwrap();
        assert_eq!(
            reconcile(true, &t.dir, &t.exec),
            Action::Write,
            "on and stale"
        );
        assert_eq!(
            reconcile(false, &t.dir, &t.exec),
            Action::Remove,
            "off and present"
        );
        std::fs::write(t.dir.join(ENTRY), desktop_entry(&t.exec)).unwrap();
        assert_eq!(
            reconcile(true, &t.dir, &t.exec),
            Action::Nothing,
            "on and current"
        );
    }

    #[test]
    fn a_program_path_that_could_break_the_entry_is_refused() {
        assert!(exec_is_safe(Path::new("/opt/My Apps/nocturne")));
        assert!(!exec_is_safe(Path::new("/opt/a\nExec=/bin/evil")));
        assert!(!exec_is_safe(Path::new("/opt/a\rb")));
        assert!(!exec_is_safe(Path::new("")));
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let bytes = std::ffi::OsStr::from_bytes(b"/opt/caf\xe9");
            assert!(!exec_is_safe(Path::new(bytes)), "not valid text");
            let root = tempfile::tempdir().unwrap();
            let target = Target {
                dir: root.path().join("autostart"),
                exec: PathBuf::from(bytes),
            };
            assert!(apply(true, &target).is_err());
            assert!(!target.dir.exists(), "nothing was written");
            let newline = Target {
                dir: root.path().join("autostart"),
                exec: PathBuf::from("/opt/a\nExec=evil"),
            };
            assert!(apply(true, &newline).is_err());
            assert!(apply(false, &newline).is_ok(), "removing is always allowed");
        }
    }

    #[test]
    fn writes_only_inside_given_dir() {
        let root = tempfile::tempdir().unwrap();
        let t = target(root.path());
        assert_eq!(apply(true, &t).unwrap(), Action::Write);
        assert!(enabled(&t.dir));
        let listing = |p: &Path| {
            let mut v: Vec<_> = std::fs::read_dir(p)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            v.sort();
            v
        };
        assert_eq!(listing(root.path()), ["autostart"]);
        assert_eq!(listing(&t.dir), [ENTRY]);
        assert_eq!(apply(true, &t).unwrap(), Action::Nothing);
        assert_eq!(apply(false, &t).unwrap(), Action::Remove);
        assert!(!enabled(&t.dir));
        assert_eq!(listing(&t.dir), Vec::<String>::new());
    }
}
