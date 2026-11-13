//! Logs: daily files under the data folder, seven kept. At info level they hold ids and
//! counts, never what the person kept.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};

/// Start logging to `dir`. Keep the returned guard alive for the life of the app. `None` when
/// the folder cannot be written; the app runs without logs rather than not at all.
pub fn init(dir: &Path) -> Option<WorkerGuard> {
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("nocturne")
        .filename_suffix("log")
        .max_log_files(7)
        .build(dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = tracing_subscriber::EnvFilter::try_from_env("NOCTURNE_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(filter)
        .try_init()
        .ok()?;
    Some(guard)
}
