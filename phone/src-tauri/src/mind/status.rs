//! What the screens show of a phone alone's mind beyond its choice: the monthly limit and this
//! month's count, the reading under way since it was turned on, how it is doing, and the Sent
//! online list, each with its command, in the shapes the screens read.

use jiff::Timestamp;
use jiff::ToSpan;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

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

/// What was kept before turning on, being read a few at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MindReading {
    pub done: u32,
    pub count: u32,
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
    t.to_zoned(tz.clone()).strftime("%Y-%m").to_string()
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
        let today = self.clock.now().to_zoned(self.clock.tz()).date();
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
        MindReading {
            done,
            count: done + waiting,
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
        .unwrap_or(MindReading { done: 0, count: 0 })
}

#[tauri::command]
pub async fn mind_status<R: Runtime>(app: AppHandle<R>) -> MindStatus {
    blocking(move || app.state::<PhoneMind>().status())
        .await
        .unwrap_or(MindStatus::Off)
}
