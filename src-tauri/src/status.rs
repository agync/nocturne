//! The status hub: one place that knows what Nocturne is doing, and says so only when it changes.
//!
//! The engine has no status of its own. The hub combines what the engine can count (things
//! waiting on the person, things waiting to be read), what the shell knows (the mind's state,
//! pause, notices, tidy progress) into an `EngineStatus`, and reports whether it differs from the
//! last one it handed out so the caller emits `status-changed` exactly once per change.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use jiff::Timestamp;
use nocturne_contract::{EngineState, EngineStatus, Notice, NoticeCode, TidyProgress};
use serde::{Deserialize, Serialize};

/// The most "couldn't read" notices shown at once; the rest are in the item list.
const MAX_COULDNT_READ: usize = 5;
const PAUSE_FILE: &str = "app-state.json";

/// Whether the AI side is ready to work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MindState {
    /// Nothing is installed or running yet.
    Absent,
    /// Starting up.
    Loading,
    Ready,
}

/// What the engine can count, read fresh for every evaluation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counts {
    pub needs_you: u32,
    pub waiting_to_read: u32,
    /// Items that could not be read (newest first).
    pub couldnt_read: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Paused {
    /// RFC 3339 instant, or `None` until the person resumes.
    until: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PauseFile {
    paused: Option<Paused>,
}

struct Inner {
    last: Option<EngineStatus>,
    last_needs_you: u32,
    notices: Vec<Notice>,
    pause: Option<Paused>,
    tidy: Option<TidyProgress>,
    mind: MindState,
    engine_up: bool,
    sent_to_internet_bytes: f64,
    sent_measured: bool,
    mind_bytes: f64,
    next_tidy: Option<String>,
}

/// The result of one evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub status: EngineStatus,
    pub status_changed: bool,
    pub needs_you_changed: bool,
}

pub struct StatusHub {
    dir: Option<PathBuf>,
    inner: Mutex<Inner>,
}

impl StatusHub {
    /// A hub that keeps the person's pause in `dir` (so it survives a restart); `None` keeps it
    /// in memory only.
    pub fn new(dir: Option<&Path>) -> Self {
        let pause = dir
            .and_then(|d| std::fs::read(d.join(PAUSE_FILE)).ok())
            .and_then(|b| serde_json::from_slice::<PauseFile>(&b).ok())
            .and_then(|f| f.paused);
        StatusHub {
            dir: dir.map(Path::to_path_buf),
            inner: Mutex::new(Inner {
                last: None,
                last_needs_you: 0,
                notices: Vec::new(),
                pause,
                tidy: None,
                mind: MindState::Absent,
                engine_up: true,
                sent_to_internet_bytes: 0.0,
                sent_measured: false,
                mind_bytes: 0.0,
                next_tidy: None,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_engine_up(&self, up: bool) {
        self.lock().engine_up = up;
    }

    pub fn set_mind(&self, mind: MindState) {
        self.lock().mind = mind;
    }

    pub fn set_tidy(&self, tidy: Option<TidyProgress>) {
        self.lock().tidy = tidy;
    }

    pub fn set_next_tidy(&self, at: Option<String>) {
        self.lock().next_tidy = at;
    }

    /// Bytes sent to the internet. Until something measures it, `measured` stays false.
    pub fn set_sent(&self, bytes: f64, measured: bool) {
        let mut g = self.lock();
        g.sent_to_internet_bytes = bytes;
        g.sent_measured = measured;
    }

    pub fn sent(&self) -> (f64, bool) {
        let g = self.lock();
        (g.sent_to_internet_bytes, g.sent_measured)
    }

    pub fn set_mind_bytes(&self, bytes: f64) {
        self.lock().mind_bytes = bytes;
    }

    pub fn mind_bytes(&self) -> f64 {
        self.lock().mind_bytes
    }

    /// Show a notice (replacing one with the same id).
    pub fn add_notice(&self, notice: Notice) {
        let mut g = self.lock();
        g.notices.retain(|n| n.id != notice.id);
        g.notices.push(notice);
    }

    pub fn clear_notice(&self, id: &str) {
        self.lock().notices.retain(|n| n.id != id);
    }

    pub fn clear_notices_of(&self, code: NoticeCode) {
        self.lock().notices.retain(|n| n.code != code);
    }

    /// Pause until an instant, or (`None`) until the person resumes.
    pub fn pause(&self, until: Option<String>) {
        let mut g = self.lock();
        g.pause = Some(Paused { until });
        self.persist(&g);
    }

    pub fn resume(&self) {
        let mut g = self.lock();
        g.pause = None;
        self.persist(&g);
    }

    /// Whether work should be paused at `now`. An expired pause ends here.
    pub fn is_paused(&self, now: Timestamp) -> bool {
        let mut g = self.lock();
        self.expire(&mut g, now);
        g.pause.is_some()
    }

    fn expire(&self, g: &mut Inner, now: Timestamp) {
        let ended = g.pause.as_ref().is_some_and(|p| {
            p.until
                .as_deref()
                .and_then(|u| u.parse::<Timestamp>().ok())
                .is_some_and(|u| u <= now)
        });
        if ended {
            g.pause = None;
            self.persist(g);
        }
    }

    fn persist(&self, g: &Inner) {
        let Some(dir) = &self.dir else { return };
        let path = dir.join(PAUSE_FILE);
        let file = PauseFile {
            paused: g.pause.clone(),
        };
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(dir)?;
            let tmp = dir.join(format!("{PAUSE_FILE}.tmp"));
            std::fs::write(&tmp, serde_json::to_vec(&file)?)?;
            std::fs::rename(&tmp, &path)
        };
        if let Err(e) = write() {
            tracing::warn!("could not keep the pause: {e}");
        }
    }

    /// Drop everything the hub keeps on disk (after "delete everything").
    pub fn forget_persisted(&self) {
        let mut g = self.lock();
        g.pause = None;
        if let Some(dir) = &self.dir {
            let _ = std::fs::remove_file(dir.join(PAUSE_FILE));
        }
    }

    /// The status for these counts, and whether it (or the needs-you count) changed since the
    /// last evaluation.
    pub fn evaluate(&self, counts: &Counts, now: Timestamp) -> Evaluation {
        let mut g = self.lock();
        self.expire(&mut g, now);
        let status = compute(&g, counts);
        let status_changed = g.last.as_ref() != Some(&status);
        let needs_you_changed = g.last_needs_you != status.needs_you;
        if status_changed {
            g.last = Some(status.clone());
        }
        g.last_needs_you = status.needs_you;
        Evaluation {
            status,
            status_changed,
            needs_you_changed,
        }
    }
}

fn compute(g: &Inner, counts: &Counts) -> EngineStatus {
    let paused = g.pause.is_some();
    let state = if !g.engine_up {
        EngineState::Stopped
    } else if paused {
        EngineState::Paused
    } else if g.mind == MindState::Loading {
        EngineState::Starting
    } else if g.mind == MindState::Absent {
        EngineState::GettingReady
    } else if g.tidy.is_some() {
        EngineState::Tidying
    } else if counts.waiting_to_read > 0 {
        EngineState::Reading
    } else if counts.needs_you > 0 {
        EngineState::NeedsYou
    } else {
        EngineState::Resting
    };

    let mut notices = g.notices.clone();
    if !g.engine_up {
        notices.push(Notice {
            id: "repair".to_string(),
            code: NoticeCode::RepairNeeded,
            item_id: None,
            eta_seconds: None,
            bytes: None,
        });
    }
    for item in counts.couldnt_read.iter().take(MAX_COULDNT_READ) {
        notices.push(Notice {
            id: format!("couldntRead:{item}"),
            code: NoticeCode::CouldntRead,
            item_id: Some(item.clone()),
            eta_seconds: None,
            bytes: None,
        });
    }

    EngineStatus {
        state,
        needs_you: counts.needs_you,
        waiting_to_read: counts.waiting_to_read,
        sent_to_internet_bytes: g.sent_to_internet_bytes,
        sent_measured: g.sent_measured,
        paused_until: g.pause.as_ref().and_then(|p| p.until.clone()),
        tidy: g.tidy.clone(),
        next_tidy: g.next_tidy.clone(),
        notices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        "2026-11-13T07:00:00Z".parse().unwrap()
    }

    fn ready() -> StatusHub {
        let h = StatusHub::new(None);
        h.set_mind(MindState::Ready);
        h
    }

    #[test]
    fn emits_once_per_distinct_status() {
        let h = ready();
        let c = Counts::default();
        let first = h.evaluate(&c, now());
        assert!(first.status_changed);
        assert_eq!(first.status.state, EngineState::Resting);
        let same = h.evaluate(&c, now());
        assert!(!same.status_changed, "an identical status is not news");
        let busy = h.evaluate(
            &Counts {
                waiting_to_read: 2,
                ..Counts::default()
            },
            now(),
        );
        assert!(busy.status_changed);
        assert_eq!(busy.status.state, EngineState::Reading);
        assert_eq!(busy.status.waiting_to_read, 2);
        let back = h.evaluate(&c, now());
        assert!(back.status_changed, "going back is a change too");
    }

    #[test]
    fn maps_counts_to_states_in_priority_order() {
        let h = ready();
        let needs = Counts {
            needs_you: 3,
            ..Counts::default()
        };
        assert_eq!(
            h.evaluate(&needs, now()).status.state,
            EngineState::NeedsYou
        );
        let both = Counts {
            needs_you: 3,
            waiting_to_read: 1,
            ..Counts::default()
        };
        assert_eq!(h.evaluate(&both, now()).status.state, EngineState::Reading);
        h.set_tidy(Some(TidyProgress {
            done: 1,
            total: 4,
            eta_seconds: None,
        }));
        assert_eq!(h.evaluate(&both, now()).status.state, EngineState::Tidying);
        h.pause(None);
        let paused = h.evaluate(&both, now());
        assert_eq!(paused.status.state, EngineState::Paused);
        assert_eq!(paused.status.needs_you, 3, "counts still show while paused");
    }

    #[test]
    fn needs_you_change_is_reported_separately() {
        let h = ready();
        let mut c = Counts::default();
        assert!(!h.evaluate(&c, now()).needs_you_changed, "zero to zero");
        c.needs_you = 2;
        assert!(h.evaluate(&c, now()).needs_you_changed);
        assert!(!h.evaluate(&c, now()).needs_you_changed);
    }

    #[test]
    fn the_mind_decides_getting_ready_and_starting() {
        let h = StatusHub::new(None);
        let c = Counts::default();
        assert_eq!(
            h.evaluate(&c, now()).status.state,
            EngineState::GettingReady
        );
        h.set_mind(MindState::Loading);
        assert_eq!(h.evaluate(&c, now()).status.state, EngineState::Starting);
        h.set_mind(MindState::Ready);
        assert_eq!(h.evaluate(&c, now()).status.state, EngineState::Resting);
    }

    #[test]
    fn notices_come_from_the_shell_and_from_unreadable_items() {
        let h = ready();
        h.add_notice(Notice {
            id: "disk".into(),
            code: NoticeCode::DiskFull,
            item_id: None,
            eta_seconds: None,
            bytes: Some(10.0),
        });
        let c = Counts {
            couldnt_read: vec!["itm_a".into(), "itm_b".into()],
            ..Counts::default()
        };
        let s = h.evaluate(&c, now()).status;
        let codes: Vec<_> = s.notices.iter().map(|n| n.code).collect();
        assert_eq!(
            codes,
            vec![
                NoticeCode::DiskFull,
                NoticeCode::CouldntRead,
                NoticeCode::CouldntRead
            ]
        );
        h.clear_notice("disk");
        assert_eq!(h.evaluate(&c, now()).status.notices.len(), 2);
    }

    #[test]
    fn a_missing_engine_is_stopped_with_the_repair_notice() {
        let h = ready();
        h.set_engine_up(false);
        let s = h.evaluate(&Counts::default(), now()).status;
        assert_eq!(s.state, EngineState::Stopped);
        assert!(s.notices.iter().any(|n| n.code == NoticeCode::RepairNeeded));
    }

    #[test]
    fn a_timed_pause_ends_by_itself_and_an_open_one_does_not() {
        let h = ready();
        h.pause(Some("2026-11-13T09:00:00Z".to_string()));
        let c = Counts::default();
        let s = h.evaluate(&c, now()).status;
        assert_eq!(s.state, EngineState::Paused);
        assert_eq!(s.paused_until.as_deref(), Some("2026-11-13T09:00:00Z"));
        let later: Timestamp = "2026-11-13T09:00:01Z".parse().unwrap();
        assert_eq!(h.evaluate(&c, later).status.state, EngineState::Resting);
        h.pause(None);
        let much_later: Timestamp = "2027-01-01T00:00:00Z".parse().unwrap();
        assert!(h.is_paused(much_later));
        h.resume();
        assert!(!h.is_paused(much_later));
    }

    #[test]
    fn the_pause_survives_a_restart_and_delete_everything_clears_it() {
        let dir = tempfile::tempdir().unwrap();
        let h = StatusHub::new(Some(dir.path()));
        h.pause(Some("2026-11-13T09:00:00Z".to_string()));
        let again = StatusHub::new(Some(dir.path()));
        assert!(again.is_paused(now()));
        again.forget_persisted();
        assert!(!StatusHub::new(Some(dir.path())).is_paused(now()));
    }
}
