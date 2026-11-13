//! Logs: daily files under the data folder (folder 0700, files tightened to 0600 by the status
//! tick), seven kept. At info level they hold error kinds, ids and counts, never what the person
//! kept or where it was.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};

/// Start logging to `dir`. Keep the returned guard alive for the life of the app. `None` when
/// the folder cannot be written; the app runs without logs rather than not at all.
pub fn init(dir: &Path) -> Option<WorkerGuard> {
    crate::fsutil::create_private_dir(dir).ok()?;
    crate::fsutil::restrict_folder_files(dir);
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("nocturne")
        .filename_suffix("log")
        .max_log_files(7)
        .build(dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    // A release build logs at info whatever the environment says; debug detail is for
    // development, where the person is the developer.
    #[cfg(debug_assertions)]
    let filter = tracing_subscriber::EnvFilter::try_from_env("NOCTURNE_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    #[cfg(not(debug_assertions))]
    let filter = tracing_subscriber::EnvFilter::new("info");
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(filter)
        .try_init()
        .ok()?;
    Some(guard)
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn the_log_folder_and_files_are_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");
        let guard = init(&logs);
        assert!(guard.is_some());
        tracing::info!("a line");
        drop(guard);
        crate::fsutil::restrict_folder_files(&logs);
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&logs), 0o700);
        let files: Vec<_> = std::fs::read_dir(&logs).unwrap().flatten().collect();
        assert!(!files.is_empty());
        for f in files {
            assert_eq!(mode(&f.path()), 0o600);
        }
    }
}
