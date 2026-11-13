//! Start with this computer.
//!
//! On Linux and macOS Nocturne writes its own entry: the autostart plugin would start the
//! running executable, which under an AppImage is a temporary mount, and writes neither a quoted
//! `Exec` line nor an escaped property list. On Linux the entry is a `.desktop` file; on macOS it
//! is a launchd agent (`~/Library/LaunchAgents/<identifier>.plist`) whose program path is
//! canonicalised and XML-escaped. Both pass `--hidden`, so a start at login goes to the tray
//! (and shows the window when there is no tray). On Windows the autostart plugin does the work.
//!
//! Every function that writes takes the folder as an argument, so tests use a temporary one.

use std::path::{Path, PathBuf};

/// The launchd label and the macOS bundle identifier.
pub const LABEL: &str = "app.nocturne.desktop";

/// The entry's file name: the app identifier and the `.plist` suffix (macOS) or the `.desktop`
/// suffix (elsewhere).
#[cfg(target_os = "macos")]
pub const ENTRY: &str = "app.nocturne.desktop.plist";
#[cfg(not(target_os = "macos"))]
pub const ENTRY: &str = "app.nocturne.desktop.desktop";

/// The reason `apply` gives when the program sits somewhere that will not be there at the next
/// login (a mounted disk image, or the temporary place macOS runs a downloaded app from). The
/// settings screen turns it into a plain sentence.
pub const MOVE_FIRST: &str = "move-to-applications";

/// Whether a program path is somewhere that goes away: a mounted volume (a disk image the person
/// opened the app from) or an App Translocation folder.
pub fn needs_move(exec: &Path) -> bool {
    let s = exec.to_string_lossy();
    s.starts_with("/Volumes/") || s.contains("/AppTranslocation/")
}

/// The entry's text for a program, in the form this platform reads.
pub fn entry_text(exec: &Path) -> String {
    if cfg!(target_os = "macos") {
        launch_agent(exec)
    } else {
        desktop_entry(exec)
    }
}

/// `text` as XML character data: the five characters XML reserves are replaced.
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// The launchd agent for a program: run it at login with `--hidden`, labelled with the app
/// identifier. Every string is escaped; the program is one array element, never a command line.
pub fn launch_agent(exec: &Path) -> String {
    let program = xml_escape(&exec.to_string_lossy());
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
\t<key>Label</key>\n\
\t<string>{label}</string>\n\
\t<key>ProgramArguments</key>\n\
\t<array>\n\
\t\t<string>{program}</string>\n\
\t\t<string>--hidden</string>\n\
\t</array>\n\
\t<key>RunAtLoad</key>\n\
\t<true/>\n\
</dict>\n\
</plist>\n",
        label = xml_escape(LABEL),
    )
}

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
    /// From the environment. Linux: `$XDG_CONFIG_HOME/autostart` (else `~/.config/autostart`)
    /// and the AppImage or the running executable. macOS: `~/Library/LaunchAgents` and the
    /// canonical path of the running executable (empty when it cannot be resolved, so nothing is
    /// written but an existing entry can still be removed).
    pub fn from_env() -> Option<Target> {
        let home = || {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        if cfg!(target_os = "macos") {
            return Some(Target {
                dir: home()?.join("Library").join("LaunchAgents"),
                exec: std::env::current_exe()
                    .and_then(|exe| exe.canonicalize())
                    .unwrap_or_default(),
            });
        }
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".config")))?;
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

/// Whether a program path can go in an entry at all: it must be text with no control character,
/// since a line break would end an `Exec` line and let the rest be read as another key, and
/// control codes are not valid in a property list.
pub fn exec_is_safe(exec: &Path) -> bool {
    exec.to_str()
        .is_some_and(|s| !s.is_empty() && !s.chars().any(char::is_control))
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
        (true, Some(current)) if current == entry_text(exec) => Action::Nothing,
        (true, _) => Action::Write,
        (false, Some(_)) => Action::Remove,
        (false, None) => Action::Nothing,
    }
}

/// Write `text` to `path` through a temporary file in the same folder, readable by everyone and
/// writable only by the owner (launchd refuses an agent others can write), then rename it into
/// place, so a reader never sees half a file.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!(
        ".{}.{}.tmp",
        name.unwrap_or_default(),
        std::process::id()
    ));
    let written = std::fs::write(&tmp, text).and_then(|()| {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644))?;
        }
        std::fs::rename(&tmp, path)
    });
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// Make the entry in `target.dir` match `want`. Touches that folder and nothing else.
pub fn apply(want: bool, target: &Target) -> std::io::Result<Action> {
    if want && !exec_is_safe(&target.exec) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the program path cannot be written to an autostart entry",
        ));
    }
    if want && cfg!(target_os = "macos") && needs_move(&target.exec) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            MOVE_FIRST,
        ));
    }
    let action = reconcile(want, &target.dir, &target.exec);
    let path = target.dir.join(ENTRY);
    match action {
        Action::Write => {
            std::fs::create_dir_all(&target.dir)?;
            write_atomic(&path, &entry_text(&target.exec))?;
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
    fn a_launch_agent_escapes_the_path_and_keeps_it_one_argument() {
        let plist = launch_agent(Path::new("/Applications/R&D <\"a\"> 'b'/Nocturne"));
        assert!(
            plist.contains(
                "<string>/Applications/R&amp;D &lt;&quot;a&quot;&gt; &apos;b&apos;/Nocturne</string>"
            ),
            "{plist}"
        );
        assert!(plist.contains("<key>Label</key>\n\t<string>app.nocturne.desktop</string>"));
        assert!(plist.contains("\t\t<string>--hidden</string>\n\t</array>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n\t<true/>"));
        assert_eq!(plist.matches("<string>").count(), 3, "{plist}");
        let plain = launch_agent(Path::new(
            "/Applications/Nocturne.app/Contents/MacOS/Nocturne",
        ));
        assert!(
            plain.contains("<string>/Applications/Nocturne.app/Contents/MacOS/Nocturne</string>")
        );
    }

    #[test]
    fn a_path_with_a_control_character_is_refused_on_every_platform() {
        for bad in ["/a\tb", "/a\nb", "/a\u{7f}b", "/a\u{1b}[0m", ""] {
            assert!(!exec_is_safe(Path::new(bad)), "{bad:?}");
        }
        assert!(exec_is_safe(Path::new(
            "/Applications/R&D/My Apps/Nocturne"
        )));
    }

    #[test]
    fn a_launch_agent_round_trips_in_a_temporary_folder() {
        let root = tempfile::tempdir().unwrap();
        let t = Target {
            dir: root.path().join("LaunchAgents"),
            exec: PathBuf::from("/Applications/R&D/Nocturne"),
        };
        assert_eq!(apply(true, &t).unwrap(), Action::Write);
        let text = std::fs::read_to_string(t.dir.join(ENTRY)).unwrap();
        assert_eq!(text, entry_text(&t.exec));
        assert_eq!(apply(true, &t).unwrap(), Action::Nothing);
        assert_eq!(apply(false, &t).unwrap(), Action::Remove);
        assert!(!enabled(&t.dir));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_reads_the_escaped_agent_as_a_valid_property_list() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("agent.plist");
        std::fs::write(
            &file,
            launch_agent(Path::new("/Applications/R&D <\"a\"> 'b'/Nocturne")),
        )
        .unwrap();
        let lint = std::process::Command::new("/usr/bin/plutil")
            .arg("-lint")
            .arg(&file)
            .output()
            .unwrap();
        assert!(
            lint.status.success(),
            "{}",
            String::from_utf8_lossy(&lint.stdout)
        );
        let read = std::process::Command::new("/usr/bin/plutil")
            .args(["-extract", "ProgramArguments.0", "raw", "-o", "-"])
            .arg(&file)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&read.stdout).trim_end_matches('\n'),
            "/Applications/R&D <\"a\"> 'b'/Nocturne"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn on_a_mac_the_target_is_the_launch_agents_folder_and_a_real_path() {
        let t = Target::from_env().expect("HOME and the executable resolve");
        assert!(t.dir.ends_with("Library/LaunchAgents"), "{:?}", t.dir);
        assert_eq!(t.exec, t.exec.canonicalize().unwrap());
        assert!(exec_is_safe(&t.exec));
    }

    #[test]
    fn a_program_on_a_mounted_disk_or_in_translocation_needs_moving() {
        assert!(needs_move(Path::new(
            "/Volumes/Nocturne/Nocturne.app/Contents/MacOS/Nocturne"
        )));
        assert!(needs_move(Path::new(
            "/private/var/folders/ab/cd/T/AppTranslocation/1234/d/Nocturne.app/Contents/MacOS/Nocturne"
        )));
        assert!(!needs_move(Path::new(
            "/Applications/Nocturne.app/Contents/MacOS/Nocturne"
        )));
        assert!(!needs_move(Path::new("/Applications/Spare/Nocturne")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn on_a_mac_a_program_that_needs_moving_is_refused_with_the_reason() {
        let root = tempfile::tempdir().unwrap();
        let t = Target {
            dir: root.path().join("LaunchAgents"),
            exec: PathBuf::from("/Volumes/Nocturne/Nocturne.app/Contents/MacOS/Nocturne"),
        };
        let err = apply(true, &t).unwrap_err();
        assert_eq!(err.to_string(), MOVE_FIRST);
        assert!(!t.dir.exists(), "nothing was written");
        assert!(apply(false, &t).is_ok(), "removing is always allowed");
    }

    #[test]
    fn an_empty_path_removes_but_never_writes() {
        let root = tempfile::tempdir().unwrap();
        let t = Target {
            dir: root.path().join("LaunchAgents"),
            exec: PathBuf::new(),
        };
        assert!(apply(true, &t).is_err());
        assert!(!t.dir.exists());
        std::fs::create_dir_all(&t.dir).unwrap();
        std::fs::write(t.dir.join(ENTRY), "old").unwrap();
        assert_eq!(apply(false, &t).unwrap(), Action::Remove);
        assert!(!enabled(&t.dir));
    }

    #[cfg(unix)]
    #[test]
    fn the_entry_is_written_whole_and_readable_by_all_whatever_the_umask() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let t = target(root.path());
        apply(true, &t).unwrap();
        let path = t.dir.join(ENTRY);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), entry_text(&t.exec));
        let names: Vec<_> = std::fs::read_dir(&t.dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [ENTRY], "no temporary file is left");
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
            entry_text(Path::new("/gone/old-nocturne")),
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
        std::fs::write(t.dir.join(ENTRY), entry_text(&t.exec)).unwrap();
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
