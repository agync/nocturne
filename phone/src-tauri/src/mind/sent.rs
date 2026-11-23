//! The Sent online list on the phone: one row for each request the mind made to its provider,
//! whatever came of it, the connection check's included, so the list matches what the provider
//! received, request for request. Rows are kept 90 days in a file of the app's beside the choice,
//! never in the memory or a backup, and "Delete everything" removes them with the key.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use jiff::{SignedDuration, Timestamp};
use nocturne_engine::Clock;
use nocturne_provider::{Exchange, Job};
use serde::{Deserialize, Serialize};

use super::lock;
use crate::sync::ledger::write_private;

const FILE: &str = "sent.jsonl";
/// How long a row is kept.
const KEEP_DAYS: i64 = 90;

/// What a request was for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SentFor {
    Check,
    Reading,
    Photo,
    Recording,
    Answer,
    Page,
    Reflection,
}

impl From<Job> for SentFor {
    fn from(job: Job) -> SentFor {
        match job {
            Job::Check => SentFor::Check,
            Job::Reading => SentFor::Reading,
            Job::Photo => SentFor::Photo,
            Job::Recording => SentFor::Recording,
            Job::Answer => SentFor::Answer,
            Job::Page => SentFor::Page,
            Job::Reflection => SentFor::Reflection,
        }
    }
}

/// One request, as the Sent online list shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sent {
    /// Counts up from 1; the list pages by it.
    pub id: u64,
    pub at: String,
    #[serde(rename = "for")]
    pub sent_for: SentFor,
    pub host: String,
    pub model: String,
    /// The provider's answer; `None` when none came.
    pub status: Option<u16>,
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub seconds: f32,
    pub cost_usd: Option<f64>,
}

/// The list, in the app's folder.
pub struct SentLog {
    path: PathBuf,
    clock: Arc<dyn Clock>,
    /// The last id given, once the file has been read. Held while a row is added.
    last: Mutex<Option<u64>>,
}

impl SentLog {
    pub fn new(dir: PathBuf, clock: Arc<dyn Clock>) -> SentLog {
        SentLog {
            path: dir.join(FILE),
            clock,
            last: Mutex::new(None),
        }
    }

    fn rows(&self) -> Vec<Sent> {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        text.lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    fn cutoff(&self) -> Timestamp {
        self.clock.now() - SignedDuration::from_hours(24 * KEEP_DAYS)
    }

    fn old(&self, row: &Sent, cutoff: Timestamp) -> bool {
        row.at.parse::<Timestamp>().is_ok_and(|at| at < cutoff)
    }

    /// Adds one request to the list, and lets go of rows older than 90 days.
    pub fn note(&self, job: Job, host: &str, model: &str, exchange: &Exchange) {
        let mut last = lock(&self.last);
        let rows = self.rows();
        if last.is_none() {
            *last = Some(rows.iter().map(|r| r.id).max().unwrap_or(0));
        }
        let id = last.unwrap_or(0) + 1;
        let row = Sent {
            id,
            at: self.clock.now().to_string(),
            sent_for: job.into(),
            host: host.to_string(),
            model: model.to_string(),
            status: exchange.status,
            bytes_up: exchange.bytes_up,
            bytes_down: exchange.bytes_down,
            seconds: exchange.seconds,
            cost_usd: exchange.cost_usd,
        };
        let cutoff = self.cutoff();
        let written = if rows.iter().any(|r| self.old(r, cutoff)) {
            let mut text = String::new();
            for r in rows.iter().filter(|r| !self.old(r, cutoff)).chain([&row]) {
                text.push_str(&serde_json::to_string(r).expect("a row serialises"));
                text.push('\n');
            }
            write_private(&self.path, text.as_bytes())
        } else {
            self.append(&row)
        };
        match written {
            Ok(()) => *last = Some(id),
            Err(e) => tracing::warn!("a request was not added to the sent list: {e}"),
        }
    }

    fn append(&self, row: &Sent) -> std::io::Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut line = serde_json::to_string(row).expect("a row serialises");
        line.push('\n');
        options.open(&self.path)?.write_all(line.as_bytes())
    }

    /// Up to `limit` rows, newest first, before the row `before` when given. Rows older than
    /// 90 days are not shown.
    pub fn list(&self, limit: usize, before: Option<u64>) -> Vec<Sent> {
        let cutoff = self.cutoff();
        let mut rows: Vec<Sent> = self
            .rows()
            .into_iter()
            .filter(|r| !self.old(r, cutoff) && before.is_none_or(|b| r.id < b))
            .collect();
        rows.reverse();
        rows.truncate(limit);
        rows
    }

    /// Removes every row.
    pub fn clear(&self) {
        let mut last = lock(&self.last);
        if let Err(e) = std::fs::remove_file(&self.path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!("the sent list could not be removed: {e}");
        }
        *last = Some(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocturne_engine::ManualClock;

    fn exchange(status: u16) -> Exchange {
        Exchange {
            status: Some(status),
            bytes_up: 120,
            bytes_down: 40,
            seconds: 0.5,
            ..Exchange::default()
        }
    }

    #[test]
    fn rows_come_back_newest_first_and_page_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let clock = Arc::new(ManualClock::at("2026-11-22T09:00:00Z", "Europe/London"));
        let log = SentLog::new(dir.path().to_path_buf(), clock);
        log.note(Job::Check, "h", "m", &exchange(200));
        log.note(Job::Reading, "h", "m", &exchange(200));
        log.note(Job::Answer, "h", "m", &exchange(429));
        let all = log.list(10, None);
        assert_eq!(all.iter().map(|r| r.id).collect::<Vec<_>>(), [3, 2, 1]);
        assert_eq!(all[0].sent_for, SentFor::Answer);
        assert_eq!(all[0].status, Some(429));
        assert_eq!(log.list(1, Some(3))[0].id, 2);
        let json = serde_json::to_value(&all[2]).unwrap();
        assert_eq!(json["for"], "check");
        assert_eq!(json["bytesUp"], 120);
    }

    #[test]
    fn rows_older_than_ninety_days_go() {
        let dir = tempfile::tempdir().unwrap();
        let clock = Arc::new(ManualClock::at("2026-08-01T09:00:00Z", "Europe/London"));
        let log = SentLog::new(dir.path().to_path_buf(), clock.clone());
        log.note(Job::Reading, "h", "m", &exchange(200));
        clock.advance_days(91);
        assert!(log.list(10, None).is_empty());
        let again = SentLog::new(dir.path().to_path_buf(), clock);
        again.note(Job::Page, "h", "m", &exchange(200));
        let left = again.list(10, None);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, 2);
        assert_eq!(again.rows().len(), 1);
    }

    #[test]
    fn clearing_removes_every_row() {
        let dir = tempfile::tempdir().unwrap();
        let clock = Arc::new(ManualClock::at("2026-11-22T09:00:00Z", "Europe/London"));
        let log = SentLog::new(dir.path().to_path_buf(), clock);
        log.note(Job::Reading, "h", "m", &exchange(200));
        log.clear();
        assert!(log.list(10, None).is_empty());
        assert!(!dir.path().join(FILE).exists());
    }
}
