//! What a question on the look-up path needs from the device that answers it, and what it
//! leaves: the look-ups run here ([`LookUps`]), the parts it was sent in ([`Part`]), and what is
//! known of whether the provider takes look-ups at all ([`Tools`]).

use std::sync::Arc;
use std::time::Duration;

use jiff::civil::Date;
use nocturne_contract::ReadingKind;
use nocturne_engine::{AnswerMeta, Fact};
use nocturne_mind::lookup::{LookUp, Place};
use serde::{Deserialize, Serialize};

/// The most requests one question makes, rounds of look-ups and look-ups in all, in one round,
/// and lines handed in all.
pub const MAX_REQUESTS: u32 = 4;
pub const MAX_ROUNDS: u32 = 3;
pub const MAX_LOOK_UPS: usize = 8;
pub const MAX_PER_ROUND: usize = 4;
pub const MAX_LINES: usize = 60;
/// The most lines in the first request.
pub const FIRST_LINES: usize = 24;
/// How long one look-up on the device may take.
pub const LOOK_UP_SECONDS: Duration = Duration::from_secs(2);

/// Runs one look-up on the device: the lines found, and how many more there were; `None` when
/// it could not be looked up.
pub type Run = Arc<dyn Fn(&LookUp) -> Option<(Vec<Fact>, usize)> + Send + Sync>;

/// Told where the next round of look-ups looks, as it begins.
pub type Looking = Arc<dyn Fn(&[Place]) + Send + Sync>;

/// The device's side of the look-up path.
#[derive(Clone)]
pub struct LookUps {
    /// The kinds of health reading kept here, so only those are offered.
    pub kinds: Vec<ReadingKind>,
    pub run: Run,
    /// Told where the next round looks, as it begins.
    pub looking: Looking,
    /// The whole question's time.
    pub limit: Duration,
}

/// Before each part: whether it may go (the month's limit), and after each part answered, that
/// it counts once.
#[derive(Clone, Copy)]
pub struct Parts<'a> {
    pub may_send: &'a (dyn Fn() -> bool + Sync),
    pub counted: &'a (dyn Fn() + Sync),
}

impl Parts<'static> {
    /// No limit, and nothing counted.
    pub fn free() -> Parts<'static> {
        Parts {
            may_send: &|| true,
            counted: &|| (),
        }
    }
}

/// One look-up as Sent online names it: where, which days or words, and how many lines came
/// back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asked {
    /// "calendar", "journal", "search" or "health".
    pub look: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ReadingKind>,
    pub lines: u32,
}

impl Asked {
    pub fn of(look: &LookUp, lines: usize) -> Asked {
        let day = |d: &Date| Some(d.to_string());
        let (name, from, to, words, kind) = match look {
            LookUp::Calendar { from, to } => ("calendar", day(from), day(to), None, None),
            LookUp::Journal { from, to } => ("journal", day(from), day(to), None, None),
            LookUp::Search { words } => ("search", None, None, Some(words.clone()), None),
            LookUp::Health { kind, from, to } => ("health", day(from), day(to), None, Some(*kind)),
        };
        Asked {
            look: name.into(),
            from,
            to,
            words,
            kind,
            lines: lines as u32,
        }
    }
}

/// Which part of a question a request was: the question's own id, its number from 1, what
/// the provider asked for in the part before (nothing for the first), and where in what it
/// carried the words new to it begin.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub ask: String,
    pub n: u32,
    #[serde(default)]
    pub asked: Vec<Asked>,
    #[serde(default)]
    pub from: u32,
}

/// What is known of a provider and the name to use: it takes look-ups, or does not, as of a
/// day; or nothing yet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "known", rename_all = "camelCase")]
pub enum Tools {
    #[default]
    Unknown,
    /// `on` is the day it was learned, as YYYY-MM-DD.
    Takes {
        on: String,
    },
    Refuses {
        on: String,
    },
}

impl Tools {
    /// Whether look-ups are offered: when it takes them, and once to find out.
    pub fn offered(&self) -> bool {
        !matches!(self, Tools::Refuses { .. })
    }
}

/// How a question on the look-up path ended.
#[derive(Debug, Clone, Default)]
pub struct Answered {
    pub meta: AnswerMeta,
    /// Every line handed in the question, in the order numbered.
    pub facts: Vec<Fact>,
    /// The requests that were answered, each counted once.
    pub parts: u32,
    /// It ended with nothing found after a look-up, with no request after it.
    pub nothing: bool,
}

/// The question of the look-up check: no personal data, the `calendar` look-up offered.
pub const CHECK_QUESTION: &str = "What is on 1 January 2030?";

/// The look-up check, made after the connection check: whether the provider and the name to
/// use ask for `calendar` with both days as 2030-01-01 when offered it. `None` when no answer
/// came, so nothing is known yet. The request is never counted.
pub async fn check(provider: &crate::Provider) -> (Option<bool>, crate::Exchange) {
    let body = serde_json::json!({
        "messages": [{"role": "user", "content": CHECK_QUESTION}],
        "tools": [nocturne_mind::lookup::tools(&[])[0].clone()],
        "tool_choice": "auto",
        "temperature": nocturne_mind::temperature::CHECK,
        "max_tokens": 200,
    });
    let reply = provider
        .chat(body, Duration::from_secs(crate::client::CHECK_SECONDS))
        .await;
    match reply {
        Ok(r) => {
            let passed = r.tool_calls.first().is_some_and(|c| {
                let f = &c["function"];
                let args: serde_json::Value =
                    serde_json::from_str(f["arguments"].as_str().unwrap_or_default())
                        .unwrap_or_default();
                f["name"] == "calendar"
                    && args["from"] == "2030-01-01"
                    && args["to"] == "2030-01-01"
            });
            (Some(passed), r.exchange)
        }
        Err(f) if f.refused => (None, f.exchange),
        Err(f) if matches!(f.exchange.status, Some(400 | 422)) => (Some(false), f.exchange),
        Err(f) => (None, f.exchange),
    }
}
