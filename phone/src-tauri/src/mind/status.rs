//! What the screens show of a phone alone's mind beyond its choice: the monthly limit and this
//! month's count, the reading under way since it was turned on, how it is doing, and the Sent
//! online list, each with its command, in the shapes the screens read.

use jiff::Timestamp;
use jiff::ToSpan;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

use super::read_now::{ReadKind, Stop};
use super::{PhoneMind, Sent, SentFor, lock};
use crate::home::Home;
use crate::sync::ledger::write_private;

const MONTH: &str = "mind-month.json";
const LIMIT: &str = "mind-limit.json";

/// How much may go online each month.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MindLimit {
    #[default]
    None,
    /// Things read and answered.
    Things { count: u32 },
    /// What the service says it cost, in US dollars.
    Amount { usd: f64 },
}

/// This month so far.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MindMonth {
    pub count: u32,
    pub limit: MindLimit,
}

/// What was kept, being read a few at a time, and why reading waits when it does: the phone's
/// power, the person's Read now, and why it last stopped. The screens choose their words from
/// these facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MindReading {
    pub done: u32,
    pub count: u32,
    /// Known to charge.
    pub charging: bool,
    /// The battery level, when the phone says.
    pub percent: Option<u8>,
    /// Battery saving is on.
    pub saver: bool,
    /// Read now or Read anyway, while it lasts.
    pub now: Option<ReadKind>,
    /// Why the last Read now stopped, this run.
    pub stopped: Option<Stop>,
    /// iPhone paused it while the person was away, after this many.
    pub away: Option<u32>,
    /// The last Read now looked once more and the service was still not answering.
    pub still_not: bool,
    /// Android shows no notifications for Nocturne.
    pub quiet: bool,
}

/// How the mind is doing, for Today, Ask and This phone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MindStatus {
    Off,
    Fine {
        host: String,
    },
    /// No answer came from the service since `since`.
    Unreachable {
        host: String,
        since: String,
    },
    /// This phone has no way to the internet: nothing left it.
    Offline {
        host: String,
    },
    KeyRefused {
        host: String,
    },
    NoCredit {
        host: String,
    },
    /// The month's limit is reached: nothing goes until `until` (a date).
    Paused {
        host: String,
        until: String,
    },
}

/// One request, as the Sent online list shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MindSent {
    pub id: String,
    pub at: String,
    pub kind: &'static str,
    pub host: String,
    /// Bytes that left this phone.
    pub bytes: u64,
    /// What it carried, as it left: empty once something it carried was forgotten.
    pub said: Vec<nocturne_provider::Said>,
    /// Its words were removed when something it carried was forgotten.
    pub forgotten: bool,
    /// For a question sent in parts: which part, what the provider asked to look up in the
    /// part before, and where the words new to this part begin in `said`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<nocturne_provider::lookup::Part>,
    /// The question ended with nothing found.
    pub nothing: bool,
}

impl From<Sent> for MindSent {
    fn from(s: Sent) -> MindSent {
        MindSent {
            id: s.id.to_string(),
            at: s.at,
            kind: match s.sent_for {
                SentFor::Check => "check",
                SentFor::Reading | SentFor::Photo | SentFor::Recording => "read",
                SentFor::Answer => "answer",
                SentFor::Page | SentFor::Reflection => "page",
            },
            host: s.host,
            bytes: s.bytes_up,
            said: s.said,
            forgotten: s.forgotten,
            part: s.part,
            nothing: s.nothing,
        }
    }
}

/// What a request's answer says about the service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Trouble {
    Unreachable(Timestamp),
    /// No way to the internet since then.
    Offline(Timestamp),
    KeyRefused,
    NoCredit,
}

impl Trouble {
    /// A request found no way to the internet: offline since it was first found so.
    pub(super) fn offline(now: Timestamp, was: Option<Trouble>) -> Trouble {
        match was {
            Some(Trouble::Offline(since)) => Trouble::Offline(since),
            _ => Trouble::Offline(now),
        }
    }

    /// After a request: `None` clears what was wrong, the rest is kept until a request works.
    pub(super) fn after(
        status: Option<u16>,
        now: Timestamp,
        was: Option<Trouble>,
    ) -> Option<Trouble> {
        match status {
            None => Some(match was {
                Some(Trouble::Unreachable(since)) => Trouble::Unreachable(since),
                _ => Trouble::Unreachable(now),
            }),
            Some(401 | 403) => Some(Trouble::KeyRefused),
            Some(402) => Some(Trouble::NoCredit),
            Some(s) if s >= 500 => Some(match was {
                Some(Trouble::Unreachable(since)) => Trouble::Unreachable(since),
                _ => Trouble::Unreachable(now),
            }),
            Some(_) => None,
        }
    }
}

/// The things read and answered this month, kept beside the choice.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(super) struct Month {
    month: String,
    count: u32,
}

fn month_of(t: Timestamp, tz: &jiff::tz::TimeZone) -> String {
    t.to_zoned_datetime(tz.clone())
        .strftime("%Y-%m")
        .to_string()
}

impl PhoneMind {
    fn month_path(&self) -> std::path::PathBuf {
        self.dir.join(MONTH)
    }

    /// Things read and answered this month.
    pub fn this_month(&self) -> u32 {
        let now = month_of(self.clock.now(), &self.clock.tz());
        std::fs::read(self.month_path())
            .ok()
            .and_then(|b| serde_json::from_slice::<Month>(&b).ok())
            .filter(|m| m.month == now)
            .map_or(0, |m| m.count)
    }

    /// Counts `n` more things read or answered.
    pub(super) fn count(&self, n: u32) {
        if n == 0 {
            return;
        }
        let _one = lock(&self.counting);
        let month = Month {
            month: month_of(self.clock.now(), &self.clock.tz()),
            count: self.this_month() + n,
        };
        let bytes = serde_json::to_vec(&month).expect("the month serialises");
        if let Err(e) = write_private(&self.month_path(), &bytes) {
            tracing::warn!("this month's count was not kept: {e}");
        }
    }

    /// What the service says this month cost, from the Sent online list.
    fn spent_this_month(&self) -> f64 {
        let now = month_of(self.clock.now(), &self.clock.tz());
        let tz = self.clock.tz();
        self.sent
            .list(usize::MAX, None)
            .iter()
            .filter(|s| {
                s.at.parse::<Timestamp>()
                    .is_ok_and(|at| month_of(at, &tz) == now)
            })
            .filter_map(|s| s.cost_usd)
            .sum()
    }

    pub fn limit(&self) -> MindLimit {
        std::fs::read(self.dir.join(LIMIT))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Sets the monthly limit, before or after turning on. False when it could not be kept.
    pub fn set_limit(&self, limit: MindLimit) -> bool {
        let bytes = serde_json::to_vec(&limit).expect("the limit serialises");
        write_private(&self.dir.join(LIMIT), &bytes)
            .inspect_err(|e| tracing::warn!("the limit was not kept: {e}"))
            .is_ok()
    }

    /// Removes the limit and this month's count.
    pub(super) fn forget_month(&self) {
        for name in [LIMIT, MONTH] {
            let _ = std::fs::remove_file(self.dir.join(name));
        }
    }

    pub fn month(&self) -> MindMonth {
        MindMonth {
            count: self.this_month(),
            limit: self.limit(),
        }
    }

    /// The first day of next month, when the limit's count starts again; `None` until the limit
    /// is reached.
    pub fn paused_until(&self) -> Option<String> {
        let reached = match self.limit() {
            MindLimit::None => false,
            MindLimit::Things { count } => self.this_month() >= count,
            MindLimit::Amount { usd } => self.spent_this_month() >= usd,
        };
        if !reached {
            return None;
        }
        let today = self.clock.now().to_zoned_datetime(self.clock.tz()).date();
        today
            .first_of_month()
            .checked_add(1.month())
            .ok()
            .map(|d| d.to_string())
    }

    /// The reading since the mind was turned on: what has been read, of what was waiting.
    pub fn reading(&self, home: &Home) -> MindReading {
        let done = lock(&self.on).as_ref().map_or(0, |on| on.read);
        let waiting = home
            .using()
            .ok()
            .and_then(|(_use, open)| open.engine.pending_items(10_000).ok())
            .map_or(0, |p| p.len() as u32);
        let read_now = lock(&self.read_now);
        MindReading {
            done,
            count: done + waiting,
            charging: self.host.charging(),
            percent: self.host.percent(),
            saver: self.host.saver(),
            now: read_now.on,
            stopped: read_now.stopped,
            away: read_now.away,
            still_not: read_now.still_not,
            quiet: read_now.quiet,
        }
    }

    pub fn status(&self) -> MindStatus {
        let Some(host) = lock(&self.on).as_ref().map(|on| on.host.clone()) else {
            return MindStatus::Off;
        };
        if let Some(until) = self.paused_until() {
            return MindStatus::Paused { host, until };
        }
        match *lock(&self.trouble) {
            None => MindStatus::Fine { host },
            Some(Trouble::Unreachable(since)) => MindStatus::Unreachable {
                host,
                since: since.to_string(),
            },
            Some(Trouble::Offline(_)) => MindStatus::Offline { host },
            Some(Trouble::KeyRefused) => MindStatus::KeyRefused { host },
            Some(Trouble::NoCredit) => MindStatus::NoCredit { host },
        }
    }
}

/// The Sent online list, newest first: up to `limit` (100 when not given) before the row
/// `before`.
#[tauri::command]
pub async fn mind_sent<R: Runtime>(
    app: AppHandle<R>,
    limit: Option<u32>,
    before: Option<String>,
) -> Vec<MindSent> {
    let before = before.and_then(|b| b.parse().ok());
    let limit = limit.unwrap_or(100).min(1000) as usize;
    blocking(move || {
        let mind = app.state::<PhoneMind>();
        mind.sent(limit, before)
            .into_iter()
            .map(MindSent::from)
            .collect()
    })
    .await
    .unwrap_or_default()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tauri::async_runtime::spawn_blocking(f).await.ok()
}

/// Sets the monthly limit; `failed` when it could not be kept.
#[tauri::command]
pub async fn mind_set_limit<R: Runtime>(app: AppHandle<R>, limit: MindLimit) -> Result<(), String> {
    match blocking(move || app.state::<PhoneMind>().set_limit(limit)).await {
        Some(true) => Ok(()),
        _ => Err("failed".into()),
    }
}

#[tauri::command]
pub async fn mind_month<R: Runtime>(app: AppHandle<R>) -> Option<MindMonth> {
    blocking(move || app.state::<PhoneMind>().month()).await
}

#[tauri::command]
pub async fn mind_reading<R: Runtime>(app: AppHandle<R>) -> MindReading {
    blocking(move || app.state::<PhoneMind>().reading(&app.state::<Home>()))
        .await
        .unwrap_or(MindReading {
            done: 0,
            count: 0,
            charging: false,
            percent: None,
            saver: false,
            now: None,
            stopped: None,
            away: None,
            still_not: false,
            quiet: false,
        })
}

#[tauri::command]
pub async fn mind_status<R: Runtime>(app: AppHandle<R>) -> MindStatus {
    blocking(move || app.state::<PhoneMind>().status())
        .await
        .unwrap_or(MindStatus::Off)
}

/// The phone's line for the reading under way: its words filled with how far it has got.
pub(super) fn reading_note(
    words: &super::read_now::ReadWords,
    reading: &MindReading,
) -> tauri_plugin_nocturne_system::ReadingNote {
    use super::read_now::fill;
    let (done, count) = (reading.done, reading.count);
    tauri_plugin_nocturne_system::ReadingNote::Reading {
        channel: words.channel.clone(),
        channel_about: words.channel_about.clone(),
        title: words.title.clone(),
        text: fill(&words.progress, done, count),
        stop: words.stop.clone(),
        done,
        count,
        paused_title: words.paused_title.clone(),
        paused_text: fill(&words.paused_text, done, count),
    }
}

/// The phone's line once the reading on battery ended for `stop`.
pub(super) fn ended_note(
    words: &super::read_now::ReadWords,
    stop: Stop,
) -> tauri_plugin_nocturne_system::ReadingNote {
    let line = super::read_now::ended_line(words, stop.why, stop.done);
    let why = serde_json::to_value(stop.why)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    tauri_plugin_nocturne_system::ReadingNote::Ended {
        why,
        title: line.as_ref().map(|l| l.0.clone()),
        text: line.map(|l| l.1),
    }
}

/// Read now or Read anyway, from the person's tap while Nocturne is in front, as Android
/// requires to start showing it. `words` are what the phone shows it with. The refusal is
/// named as `read_now::Refused` says.
#[tauri::command]
pub async fn mind_read_now<R: Runtime>(
    app: AppHandle<R>,
    kind: super::read_now::ReadKind,
    words: super::read_now::ReadWords,
) -> Result<MindReading, super::read_now::Refused> {
    use tauri_plugin_nocturne_system::SystemExt;
    blocking(move || {
        let mind = app.state::<PhoneMind>();
        lock(&mind.read_now).words = words.clone();
        mind.read_now(kind, &tokio::runtime::Handle::current())?;
        let reading = mind.reading(&app.state::<Home>());
        match app.system().reading_now(&reading_note(&words, &reading)) {
            Ok(answer) => mind.set_quiet(!answer.notes),
            Err(e) => tracing::warn!("the reading could not be shown by the phone: {e:?}"),
        }
        Ok(mind.reading(&app.state::<Home>()))
    })
    .await
    .unwrap_or(Err(super::read_now::Refused::Off))
}

/// Stop, in Nocturne: the reading on battery ends, and the phone stops showing it.
#[tauri::command]
pub async fn mind_read_stop<R: Runtime>(app: AppHandle<R>) -> Option<MindReading> {
    use tauri_plugin_nocturne_system::SystemExt;
    blocking(move || {
        let mind = app.state::<PhoneMind>();
        let was = mind.read_now_on().is_some();
        mind.stop_read_now();
        if was && let Some(stop) = mind.read_now_stopped() {
            let words = lock(&mind.read_now).words.clone();
            if let Err(e) = app.system().reading_now(&ended_note(&words, stop)) {
                tracing::warn!("the phone could not stop showing the reading: {e:?}");
            }
        }
        mind.reading(&app.state::<Home>())
    })
    .await
}

/// Between passes: the reading on battery as the phone shows it, kept up to date, and what the
/// person did there. `shown` is whether the phone shows it now; the answer is whether it does
/// after.
pub(super) fn tell_the_phone<R: Runtime>(
    app: &AppHandle<R>,
    mind: &PhoneMind,
    shown: bool,
) -> bool {
    use tauri_plugin_nocturne_system::SystemExt;
    let words = lock(&mind.read_now).words.clone();
    if mind.read_now_on().is_some() {
        let reading = mind.reading(&app.state::<Home>());
        match app.system().reading_now(&reading_note(&words, &reading)) {
            Ok(answer) => {
                mind.set_quiet(!answer.notes);
                if answer.paused {
                    mind.paused_away();
                }
                if answer.stop {
                    mind.stop_read_now();
                    return tell_the_phone(app, mind, true);
                }
            }
            Err(e) => tracing::warn!("the phone could not show the reading: {e:?}"),
        }
        return true;
    }
    if shown
        && let Some(stop) = mind.read_now_stopped()
        && let Err(e) = app.system().reading_now(&ended_note(&words, stop))
    {
        tracing::warn!("the phone could not stop showing the reading: {e:?}");
    }
    false
}
