//! The app's own log: what Nocturne on this phone did, at INFO and above, in a file beside its
//! memory, so a move or a send that went wrong can be read back afterwards. The phone shows no
//! output of its own, so without this every line went nowhere. Only Nocturne's own lines are
//! written, and they name what happened, never what the person kept. The file starts again once
//! it passes `MAX_BYTES`, keeping the one before it.

use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};

const FILE: &str = "nocturne.log";
const MAX_BYTES: u64 = 1024 * 1024;

struct Open {
    file: File,
    written: u64,
}

pub struct FileLog {
    path: PathBuf,
    open: Mutex<Option<Open>>,
    spans: AtomicU64,
}

impl FileLog {
    /// The log in `dir`, made when the first line comes.
    pub fn new(dir: &Path) -> FileLog {
        FileLog {
            path: dir.join(FILE),
            open: Mutex::new(None),
            spans: AtomicU64::new(1),
        }
    }

    fn write(&self, line: &str) {
        let mut slot = self.open.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(open) = slot.take_if(|o| o.written >= MAX_BYTES)
            && std::fs::rename(&self.path, self.path.with_extension("log.1")).is_err()
        {
            // Not put aside: start it again rather than let it grow.
            let _ = open.file.set_len(0);
        }
        if slot.is_none() {
            let Some(file) = self.path.parent().and_then(|dir| {
                std::fs::create_dir_all(dir).ok()?;
                let mut options = OpenOptions::new();
                options.create(true).append(true);
                #[cfg(unix)]
                std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
                options.open(&self.path).ok()
            }) else {
                return;
            };
            let written = file.metadata().map(|m| m.len()).unwrap_or(0);
            *slot = Some(Open { file, written });
        }
        if let Some(open) = slot.as_mut()
            && open.file.write_all(line.as_bytes()).is_ok()
        {
            open.written += line.len() as u64;
        }
    }
}

/// One event: its message, then its fields.
#[derive(Default)]
struct Line {
    message: String,
    fields: String,
}

impl Visit for Line {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "message" => {
                let _ = write!(self.message, "{value:?}");
            }
            name => {
                let _ = write!(self.fields, " {name}={value:?}");
            }
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "message" => self.message.push_str(value),
            name => {
                let _ = write!(self.fields, " {name}={value}");
            }
        }
    }
}

impl Subscriber for FileLog {
    fn enabled(&self, meta: &Metadata<'_>) -> bool {
        *meta.level() <= Level::INFO && meta.target().starts_with("nocturne")
    }

    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(self.spans.fetch_add(1, Ordering::Relaxed))
    }

    fn record(&self, _: &Id, _: &Record<'_>) {}

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let meta = event.metadata();
        let mut line = Line::default();
        event.record(&mut line);
        let text = format!("{}{}", line.message, line.fields).replace('\n', "\\n");
        self.write(&format!(
            "{} {} {}: {text}\n",
            jiff::Timestamp::now(),
            meta.level(),
            meta.target()
        ));
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

/// Every line from here on goes to the log in `dir`.
pub fn start(dir: &Path) {
    if tracing::subscriber::set_global_default(FileLog::new(dir)).is_err() {
        tracing::warn!("the log was already set up");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nocturnes_own_lines_go_to_the_file_and_it_starts_again_when_full() {
        let dir = tempfile::tempdir().unwrap();
        let log = FileLog::new(&dir.path().join("logs"));
        tracing::subscriber::with_default(log, || {
            tracing::info!(device = "d1", "sent {} of {} bytes", 10, 20);
            tracing::debug!("not written");
            tracing::info!(target: "hyper", "not ours");
            tracing::info!("two\nlines");
        });
        let text = std::fs::read_to_string(dir.path().join("logs").join(FILE)).unwrap();
        assert_eq!(text.lines().count(), 2, "{text}");
        assert!(text.contains(": two\\nlines\n"), "{text}");
        assert!(
            text.contains(" INFO nocturne_phone_lib::log::tests: sent 10 of 20 bytes device=d1")
        );

        let log = FileLog::new(&dir.path().join("logs"));
        let long = "x".repeat(MAX_BYTES as usize);
        log.write(&long);
        log.write("after\n");
        let now = std::fs::read_to_string(dir.path().join("logs").join(FILE)).unwrap();
        assert_eq!(now, "after\n");
        assert!(dir.path().join("logs").join("nocturne.log.1").exists());
    }
}
