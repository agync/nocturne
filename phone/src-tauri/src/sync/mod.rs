//! Sending the calendar and Health to the computer, while the app is open: when it opens, when it
//! comes back to the front and when the computer becomes reachable. Each source is read, compared
//! with what last went across, and only what changed is sent, in batches under one send id.
//! While the computer cannot be reached the changes wait, the latest version of each.
//!
//! The phone's sources and the way to the computer sit behind [`Device`] and [`Link`], so the
//! whole of it runs in tests with fakes. Self-contained: the computer's tests use it as is.

pub mod calendar;
pub mod health;
pub mod ledger;

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use nocturne_channel::client::CallError;
use nocturne_channel::protocol::{MAX_SYNC_BATCH, MAX_SYNC_JSON};
use nocturne_contract::{
    CalendarEntry, PhoneCalendar, PhoneError, PhoneFeature, Reading, ReadingKind, RemoteAnswer,
    RemoteCall, SyncReceipt,
};
use serde::{Deserialize, Serialize};

pub use ledger::{CalendarSettings, HealthSettings, LastSent, Ledger, LedgerFile, SourceSent};

/// Two days back to two weeks ahead.
pub const DAYS_BACK: u32 = 2;
pub const DAYS_AHEAD: u32 = 14;
/// Health on first connecting: the last 30 days, or up to a year with older readings on.
pub const HEALTH_DAYS: u32 = 30;
pub const HEALTH_DAYS_OLDER: u32 = 365;
/// Days of heart rate, resting heart rate and steps read again on each send.
pub const REFRESH_DAYS: u32 = 2;
/// The computer takes a few sends a minute from a phone: a send it turns away is tried again
/// after a pause, this many times.
const BUSY_TRIES: u32 = 30;
const BUSY_PAUSE: Duration = Duration::from_secs(6);
/// Room left in each batch for the frame around the changes.
const BATCH_MARGIN: usize = 512;

/// Whether the app may read the calendar. `Refused`: the phone can still ask. `Blocked`: it will
/// no longer show its prompt, or the permission was taken back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Access {
    Granted,
    Ask,
    Refused,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    Available,
    NeedsUpdate,
    Missing,
}

/// The days read, both included, and the appointments on them.
#[derive(Debug, Clone, PartialEq)]
pub struct Instances {
    pub window_from: String,
    pub window_to: String,
    pub entries: Vec<CalendarEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Granted {
    pub kinds: Vec<ReadingKind>,
    pub history: bool,
    /// Workouts carry their distance only when it is allowed too.
    #[serde(default)]
    pub distance: bool,
    /// Health never says which kinds it refused (iPhone): only its own deletions remove.
    #[serde(default)]
    pub refusals_hidden: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FullRead {
    pub readings: Vec<Reading>,
    pub token: String,
    pub from_day: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Changes {
    pub readings: Vec<Reading>,
    pub removed: Vec<String>,
    pub token: Option<String>,
    pub expired: bool,
    /// The days read again whole, by kind.
    pub refreshed: std::collections::HashMap<ReadingKind, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    NotAllowed,
    Unavailable,
    Failed(String),
}

/// The phone's own sources: the plugins on a phone, a fake in tests.
pub trait Device: Send + Sync {
    fn calendar_access(&self) -> Access;
    fn calendars(&self) -> Result<Vec<PhoneCalendar>, SourceError>;
    fn instances(
        &self,
        back: u32,
        ahead: u32,
        calendar_ids: &[String],
        with_notes: bool,
    ) -> Result<Instances, SourceError>;
    fn health_availability(&self) -> Availability;
    fn health_granted(&self) -> Result<Granted, SourceError>;
    fn health_read(&self, kinds: &[ReadingKind], days: u32) -> Result<FullRead, SourceError>;
    fn health_changes(
        &self,
        kinds: &[ReadingKind],
        token: &str,
        refresh_days: u32,
    ) -> Result<Changes, SourceError>;
}

/// The way to the computer: the paired phone's client, a fake in tests.
pub trait Link: Send + Sync {
    fn send(
        &self,
        call: RemoteCall,
    ) -> impl Future<Output = Result<RemoteAnswer, CallError>> + Send;
    fn pause(&self, how_long: Duration) -> impl Future<Output = ()> + Send;
}

/// What one send did, per source; none for a source that sent nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub at: String,
    pub calendar: Option<SyncReceipt>,
    pub health: Option<SyncReceipt>,
}

/// A source on This phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceStatus {
    On,
    NotBrought,
    OffInSettings,
    StoppedOnComputer,
    NeedsNewerComputer,
}

/// What This phone shows under "What this phone brings".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThisPhoneSources {
    pub calendar: SourceStatus,
    pub calendars: u32,
    pub health: SourceStatus,
    pub kinds: u32,
    pub last_sent: Option<LastSent>,
    pub calendar_last_sent: Option<SourceSent>,
    pub health_last_sent: Option<SourceSent>,
    /// "Readings from {date} onwards": the first day the last full read of Health covered.
    pub health_from: Option<String>,
    pub computer_takes: Vec<PhoneFeature>,
    /// "Too many to bring at once. Choose fewer calendars."
    pub calendar_too_many: bool,
}

/// How one source's send ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sent {
    /// Every batch went across.
    Done,
    /// The computer is not taking this source; nothing was taken.
    Stopped,
    /// The computer took what fits and no more.
    TooMany,
    /// The computer could not be reached; the changes wait.
    Away,
    /// The computer refused the send.
    Refused,
    /// The person turned the source off, or took its permission back, during the send.
    Off,
}

/// A send id: random, in the channel's id alphabet.
pub fn new_sync_id() -> String {
    nocturne_channel::crypto::b64(&nocturne_channel::crypto::random::<16>())
}

pub(crate) fn hash(value: &impl Serialize) -> String {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(value).expect("a change serialises");
    Sha256::digest(&bytes)[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Changes split into batches: at most `MAX_SYNC_BATCH` each, and each batch with its frame
/// under `MAX_SYNC_JSON`. A change too large to go even alone comes back on its own.
pub(crate) fn batches<T: Serialize>(changes: Vec<T>, frame: usize) -> (Vec<Vec<T>>, Vec<T>) {
    let room = MAX_SYNC_JSON.saturating_sub(frame + BATCH_MARGIN);
    let mut out: Vec<Vec<T>> = Vec::new();
    let mut size = 0;
    let mut too_big = Vec::new();
    for change in changes {
        let len = serde_json::to_vec(&change).map_or(usize::MAX, |b| b.len()) + 1;
        if len > room {
            too_big.push(change);
            continue;
        }
        match out.last_mut() {
            Some(batch) if batch.len() < MAX_SYNC_BATCH && size + len <= room => {
                size += len;
                batch.push(change);
            }
            _ => {
                size = len;
                out.push(vec![change]);
            }
        }
    }
    (out, too_big)
}

/// A day as `YYYY-MM-DD`, moved by a number of days.
pub(crate) fn day_plus(day: &str, days: i64) -> Option<String> {
    let d: jiff::civil::Date = day.parse().ok()?;
    d.checked_add(jiff::Span::new().days(days))
        .ok()
        .map(|d| d.to_string())
}

/// How a send went: what the computer took, and how it ended.
pub(crate) struct Outcome {
    pub total: Option<SyncReceipt>,
    pub ended: Sent,
}

/// Send the batches in order. Before each, `still_on` says whether the person still brings the
/// source; `took` notes each batch the computer took in full.
pub(crate) async fn send_all<T: Clone>(
    link: &impl Link,
    parts: Vec<Vec<T>>,
    call: impl Fn(u32, bool, Vec<T>) -> RemoteCall,
    still_on: impl Fn() -> bool,
    mut took: impl FnMut(u32, &[T]),
) -> Outcome {
    let count = parts.len();
    let mut out = Outcome {
        total: None,
        ended: Sent::Done,
    };
    for (seq, part) in parts.into_iter().enumerate() {
        if !still_on() {
            out.ended = Sent::Off;
            break;
        }
        let seq = seq as u32;
        match send_batch(link, call(seq, seq as usize + 1 == count, part.clone())).await {
            Answer::Took(receipt) if receipt.too_many => {
                add(&mut out.total, receipt);
                out.ended = Sent::TooMany;
                break;
            }
            Answer::Took(receipt) => {
                add(&mut out.total, receipt);
                took(seq, &part);
            }
            Answer::Ended(e) => {
                out.ended = e;
                break;
            }
        }
    }
    if out.ended == Sent::Stopped {
        out.total = Some(stopped());
    }
    out
}

/// The receipt of a send the computer is not taking.
pub(crate) fn stopped() -> SyncReceipt {
    SyncReceipt {
        taken: 0,
        unchanged: 0,
        refused: 0,
        stopped: true,
        too_many: false,
    }
}

pub(crate) fn add(total: &mut Option<SyncReceipt>, r: SyncReceipt) {
    let t = total.get_or_insert(SyncReceipt {
        taken: 0,
        unchanged: 0,
        refused: 0,
        stopped: false,
        too_many: false,
    });
    t.taken += r.taken;
    t.unchanged += r.unchanged;
    t.refused += r.refused;
    t.stopped |= r.stopped;
    t.too_many |= r.too_many;
}

/// What one batch came back with.
pub(crate) enum Answer {
    Took(SyncReceipt),
    Ended(Sent),
}

/// One batch, tried again after a pause while the computer says it has had enough for now.
pub(crate) async fn send_batch(link: &impl Link, call: RemoteCall) -> Answer {
    let busy = |code| code == PhoneError::TooMany || code == PhoneError::Busy;
    for _ in 0..BUSY_TRIES {
        match link.send(call.clone()).await {
            Ok(RemoteAnswer::Synced { receipt }) if receipt.stopped => {
                return Answer::Ended(Sent::Stopped);
            }
            Ok(RemoteAnswer::Synced { receipt }) => return Answer::Took(receipt),
            Ok(RemoteAnswer::Error { code }) | Err(CallError::Phone { code }) if busy(code) => {
                link.pause(BUSY_PAUSE).await;
            }
            Ok(RemoteAnswer::Revoked) | Err(CallError::Connection { .. }) => {
                return Answer::Ended(Sent::Away);
            }
            Ok(RemoteAnswer::Error { code }) | Err(CallError::Phone { code })
                if code == PhoneError::SessionExpired =>
            {
                return Answer::Ended(Sent::Away);
            }
            Ok(_) | Err(_) => return Answer::Ended(Sent::Refused),
        }
    }
    Answer::Ended(Sent::Away)
}

/// The sends, one at a time.
pub struct Syncer {
    pub ledger: LedgerFile,
    busy: AtomicBool,
}

struct Busy<'a>(&'a AtomicBool);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Syncer {
    pub fn new(ledger: LedgerFile) -> Syncer {
        Syncer {
            ledger,
            busy: AtomicBool::new(false),
        }
    }

    /// Read both sources and send what changed. None while another send is under way.
    pub async fn sync(
        &self,
        device: &impl Device,
        link: &impl Link,
        takes: &[PhoneFeature],
        now: String,
    ) -> Option<SyncReport> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return None;
        }
        let _busy = Busy(&self.busy);
        let mut report = SyncReport {
            at: now.clone(),
            calendar: None,
            health: None,
        };
        if takes.contains(&PhoneFeature::Calendar) {
            report.calendar = calendar::sync(&self.ledger, device, link).await;
        }
        if takes.contains(&PhoneFeature::Health) {
            report.health = health::sync(&self.ledger, device, link).await;
        }
        let taken = |r: &Option<SyncReceipt>| r.map_or(0, |r| r.taken);
        let (cal, readings) = (taken(&report.calendar), taken(&report.health));
        if cal + readings > 0 {
            self.ledger.update(|l| {
                let at = |count| {
                    Some(SourceSent {
                        at: now.clone(),
                        count,
                    })
                };
                if cal > 0 {
                    l.calendar.last_sent = at(cal);
                }
                if readings > 0 {
                    l.health.last_sent = at(readings);
                }
                l.last_sent = Some(LastSent {
                    at: now.clone(),
                    calendar_changes: cal,
                    readings,
                })
            });
        }
        Some(report)
    }

    /// What This phone shows, from the ledger and what the phone says now.
    pub fn sources(&self, device: &impl Device, takes: &[PhoneFeature]) -> ThisPhoneSources {
        let l = self.ledger.get();
        let calendar = if !takes.contains(&PhoneFeature::Calendar) {
            SourceStatus::NeedsNewerComputer
        } else {
            match (l.calendar.settings.bring, device.calendar_access()) {
                (_, Access::Blocked) => SourceStatus::OffInSettings,
                (false, _) => SourceStatus::NotBrought,
                (true, Access::Granted) if l.calendar.stopped => SourceStatus::StoppedOnComputer,
                (true, Access::Granted) => SourceStatus::On,
                (true, _) => SourceStatus::OffInSettings,
            }
        };
        let calendars = match calendar {
            SourceStatus::On | SourceStatus::StoppedOnComputer => {
                device.calendars().map_or(0, |all| {
                    all.iter().filter(|c| l.calendar.settings.brings(c)).count() as u32
                })
            }
            _ => 0,
        };
        let granted = || device.health_granted().unwrap_or_default();
        let (health, kinds) = if !takes.contains(&PhoneFeature::Health) {
            (SourceStatus::NeedsNewerComputer, 0)
        } else if !l.health.settings.bring
            || device.health_availability() != Availability::Available
        {
            (SourceStatus::NotBrought, 0)
        } else {
            let granted = granted();
            let n = l
                .health
                .settings
                .kinds
                .iter()
                .filter(|k| granted.kinds.contains(k))
                .count() as u32;
            match n {
                0 => (SourceStatus::OffInSettings, 0),
                n if l.health.stopped => (SourceStatus::StoppedOnComputer, n),
                n => (SourceStatus::On, n),
            }
        };
        ThisPhoneSources {
            calendar,
            calendars,
            health,
            kinds,
            last_sent: l.last_sent,
            calendar_last_sent: l.calendar.last_sent,
            health_last_sent: l.health.last_sent,
            health_from: l.health.from_day,
            computer_takes: takes.to_vec(),
            calendar_too_many: l.calendar.too_many,
        }
    }
}
