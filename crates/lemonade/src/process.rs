//! Every program Nocturne starts is built here, so none of them opens a console window.
//!
//! The app is a windowed program on Windows. A console program it starts (the engine, a
//! connection helper, a converter) gets a console window of its own unless it is started with
//! `CREATE_NO_WINDOW`; with it, it gets a console without a window, which the programs it starts
//! in turn share. A test below keeps every other `Command::new` out of the shipped sources.

use process_wrap::tokio::{CommandWrap, KillOnDrop};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = windows::Win32::System::Threading::CREATE_NO_WINDOW.0;

/// A program to run and wait for, or to leave running unsupervised. A program that must end with
/// the app goes through [`group`].
pub use nocturne_engine::process::command;

/// [`command`] for async callers.
pub fn tokio_command(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut cmd = tokio::process::Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// A long-running program and everything it starts, as one group that ends with it: a process
/// group on Unix, a job object on Windows. The job object sets the creation flags itself, so the
/// flag is handed to it, and any other flags set on `cmd` are dropped.
pub fn group(cmd: tokio::process::Command) -> CommandWrap {
    let mut wrap = CommandWrap::from(cmd);
    #[cfg(unix)]
    wrap.wrap(process_wrap::tokio::ProcessGroup::leader());
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::PROCESS_CREATION_FLAGS;
        wrap.wrap(process_wrap::tokio::CreationFlags(PROCESS_CREATION_FLAGS(
            CREATE_NO_WINDOW,
        )));
        wrap.wrap(process_wrap::tokio::JobObject);
    }
    wrap.wrap(KillOnDrop);
    wrap
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// Sources that may build a command themselves: this module, the engine's, which it
    /// re-exports, and code built for macOS only.
    const OWN_BUILDERS: &[&str] = &[
        "crates/lemonade/src/process.rs",
        "crates/engine/src/process.rs",
        "crates/intelligence/src/host/macos.rs",
    ];

    fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, found);
            } else if path.extension().is_some_and(|e| e == "rs") {
                found.push(path);
            }
        }
    }

    #[test]
    fn every_program_is_started_without_a_window() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut files = Vec::new();
        for crate_dir in std::fs::read_dir(root.join("crates")).unwrap().flatten() {
            rust_files(&crate_dir.path().join("src"), &mut files);
        }
        rust_files(&root.join("src-tauri/src"), &mut files);
        let offenders: Vec<String> = files
            .iter()
            .filter_map(|f| {
                let name = f
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = std::fs::read_to_string(f).unwrap();
                ((text.contains("Command::new(") || text.contains("Command as "))
                    && !OWN_BUILDERS.contains(&name.as_str()))
                .then_some(name)
            })
            .collect();
        assert!(
            files.len() > 100,
            "the sources were not found under {root:?}"
        );
        assert_eq!(
            offenders,
            Vec::<String>::new(),
            "start these with nocturne_lemonade::process"
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_group_keeps_the_flag_through_its_job_object() {
        use super::*;
        use process_wrap::tokio::{CreationFlags, JobObject};
        let wrap = group(tokio_command("lemond"));
        assert!(wrap.has_wrap::<JobObject>());
        let flags = wrap.get_wrap::<CreationFlags>().map(|f| f.0.0);
        assert_eq!(flags, Some(CREATE_NO_WINDOW));
    }
}
