//! Online answers in the store: the feature's settings and counters (`online_state`, one row,
//! never the key) and the sent log (`online_sends`), kept as templates with memory ids.

use std::path::Path;

use jiff::Timestamp;
use nocturne_contract::{
    KeyStorage, OnlineLimit, OnlinePause, OnlineProblem, SendKind, SendResult, SentLine,
};
use rusqlite::Connection;

use crate::error::Result;

/// Online answers' settings and counters, one row.
#[derive(Debug, Clone, PartialEq)]
pub struct OnlineRow {
    pub wanted_on: bool,
    pub set_up: bool,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub phones: bool,
    pub send_sensitive: bool,
    pub limit: OnlineLimit,
    pub pause: Option<OnlinePause>,
    pub problem: Option<OnlineProblem>,
    pub problem_since: Option<Timestamp>,
    pub sent_bytes: f64,
    pub first_on_at: Option<Timestamp>,
    pub on_since: Option<Timestamp>,
    pub last_on_at: Option<Timestamp>,
    pub last_off_at: Option<Timestamp>,
    pub key_storage: Option<KeyStorage>,
}

/// One attempt for the sent log. `template` is the body as sent with each memory sentence
/// replaced by its number; `memory_lines` pairs each `[mN]` (in order) with the hash of the exact
/// line sent, so the log never keeps a memory's words.
#[derive(Debug, Clone, PartialEq)]
pub struct NewSend {
    pub at: Timestamp,
    pub kind: SendKind,
    pub host: String,
    pub model: String,
    pub question: String,
    pub template: Vec<SentLine>,
    pub memory_lines: Vec<(String, String)>,
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub cost_usd: Option<f64>,
    pub status: Option<u16>,
    pub seconds: Option<f32>,
    pub result: SendResult,
    /// True when the question counts toward the month (it reached the engine route).
    pub counted: bool,
}

/// The export's part for Online answers. Writes nothing yet.
pub(crate) fn export_section(_conn: &Connection, _dir: &Path) -> Result<()> {
    Ok(())
}
