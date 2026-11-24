//! Online answers in the store: the feature's settings and counters (`online_state`, one row,
//! never the key) and the sent log (`online_sends`), kept as templates with memory ids.
//!
//! A memory line is stored with its sentence replaced by [`NewSend::SLOT`] (and the value it
//! replaced, when the line says one, by [`NewSend::WAS_SLOT`]), beside the memory's id and the
//! SHA-256 of the exact line sent. The log shows the line again only while the memory, read
//! now, rebuilds a line with the same hash: a forgotten or changed memory's words are never
//! kept here.

use std::path::Path;

use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};
use nocturne_contract::{
    KeyStorage, LeftOut, LimitKind, LocalReason, OnlineLimit, OnlinePause, OnlineProblem,
    OnlineSend, SendKind, SendResult, SentLine, SentMemory,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::Engine;
use crate::error::Result;
use crate::util::{enum_str, fmt_ts, parse_enum, parse_ts, sha256_hex};

/// Set, in `settings`, once the person has seen that Online answers holds back only what they
/// mark private: on the consent step's words, or in the one-time notice for those who agreed
/// before.
const ONLY_PRIVATE_SEEN: &str = "onlineOnlyPrivateSeen";

/// Online answers' settings and counters, one row.
#[derive(Debug, Clone, PartialEq)]
pub struct OnlineRow {
    pub wanted_on: bool,
    pub set_up: bool,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub phones: bool,
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
/// replaced by [`NewSend::SLOT`]; `memory_lines` pairs each `[mN]` (in order) with the hash of
/// the exact line sent, so the log never keeps a memory's words. `instructions` is the system
/// message, which holds no memory.
#[derive(Debug, Clone, PartialEq)]
pub struct NewSend {
    pub at: Timestamp,
    pub kind: SendKind,
    pub host: String,
    pub model: String,
    pub question: String,
    pub instructions: String,
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
    /// True when the provider accepted the connection and a status came back.
    pub reached: bool,
}

impl NewSend {
    /// Stands for a memory's sentence in a stored line.
    pub const SLOT: &'static str = "\u{FFFC}";
    /// Stands for the value a memory replaced, in the "before, it was" part of a stored line.
    pub const WAS_SLOT: &'static str = "\u{FFFD}\u{FFFC}";

    /// The hash a sent line is kept as.
    pub fn hash_line(line: &str) -> String {
        sha256_hex(line.as_bytes())
    }
}

/// The last attempt, for the advanced details.
#[derive(Debug, Clone, PartialEq)]
pub struct LastSend {
    pub at: Timestamp,
    pub status: Option<u16>,
    pub seconds: Option<f32>,
    pub bytes_up: f64,
    pub bytes_down: f64,
}

type LastRaw = (String, Option<u16>, Option<f32>, f64, f64);

/// `send_sensitive`, the choice an earlier version offered for health, money and ID details, is
/// left in the table unread: only what the person marks private is held back now.
const STATE_COLS: &str = "wanted_on, set_up, base_url, model, phones, \
    limit_kind, limit_questions, limit_usd, pause, problem, problem_since, sent_bytes, \
    first_on_at, on_since, last_on_at, last_off_at, key_storage";

fn ts_opt(s: Option<String>) -> Result<Option<Timestamp>> {
    s.map(|s| parse_ts(&s)).transpose()
}

type StateRaw = (
    (bool, bool, Option<String>, Option<String>, bool),
    (String, u32, Option<f64>, Option<String>, Option<String>),
    (Option<String>, f64, [Option<String>; 4], Option<String>),
);

fn load_state(conn: &Connection) -> Result<OnlineRow> {
    let (a, b, c): StateRaw = conn.query_row(
        &format!("SELECT {STATE_COLS} FROM online_state WHERE id = 1"),
        [],
        |r| {
            Ok((
                (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?),
                (r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?),
                (
                    r.get(10)?,
                    r.get(11)?,
                    [r.get(12)?, r.get(13)?, r.get(14)?, r.get(15)?],
                    r.get(16)?,
                ),
            ))
        },
    )?;
    let [first, since, last_on, last_off] = c.2;
    Ok(OnlineRow {
        wanted_on: a.0,
        set_up: a.1,
        base_url: a.2,
        model: a.3,
        phones: a.4,
        limit: OnlineLimit {
            kind: parse_enum(&b.0).unwrap_or(LimitKind::Questions),
            questions: b.1,
            usd: b.2,
        },
        // A value this version cannot read is no pause and no problem.
        pause: b.3.and_then(|p| serde_json::from_str(&p).ok()),
        problem: b.4.and_then(|p| parse_enum(&p).ok()),
        problem_since: ts_opt(c.0)?,
        sent_bytes: c.1,
        first_on_at: ts_opt(first)?,
        on_since: ts_opt(since)?,
        last_on_at: ts_opt(last_on)?,
        last_off_at: ts_opt(last_off)?,
        key_storage: c.3.and_then(|k| parse_enum(&k).ok()),
    })
}

fn store_state(conn: &Connection, s: &OnlineRow) -> Result<()> {
    let ts = |t: Option<Timestamp>| t.map(fmt_ts);
    conn.execute(
        "UPDATE online_state SET wanted_on = ?1, set_up = ?2, base_url = ?3, model = ?4, \
         phones = ?5, limit_kind = ?6, limit_questions = ?7, limit_usd = ?8, pause = ?9, \
         problem = ?10, problem_since = ?11, sent_bytes = ?12, first_on_at = ?13, \
         on_since = ?14, last_on_at = ?15, last_off_at = ?16, key_storage = ?17 WHERE id = 1",
        params![
            s.wanted_on,
            s.set_up,
            s.base_url,
            s.model,
            s.phones,
            enum_str(&s.limit.kind),
            s.limit.questions,
            s.limit.usd,
            s.pause.as_ref().and_then(|p| serde_json::to_string(p).ok()),
            s.problem.as_ref().map(enum_str),
            ts(s.problem_since),
            s.sent_bytes,
            ts(s.first_on_at),
            ts(s.on_since),
            ts(s.last_on_at),
            ts(s.last_off_at),
            s.key_storage.as_ref().map(enum_str),
        ],
    )?;
    Ok(())
}

/// Why a memory may not be sent, or `None` when it may. Only the person's own choices keep a
/// memory here: marked private in any version, being forgotten or let go. Nothing about its
/// words or labels does. Fails closed: a memory the store does not know counts as private.
fn verdict(conn: &Connection, id: &str) -> Result<Option<LeftOut>> {
    // A check-in, an entry or a reading kept as it is goes like a memory, unless it is private.
    if let Some(item) = crate::lookup::kept_line_item(id) {
        let private: Option<bool> = conn
            .query_row("SELECT private FROM items WHERE id = ?1", [item], |r| {
                r.get(0)
            })
            .optional()?;
        return Ok(match private {
            Some(false) => None,
            _ => Some(LeftOut::Private),
        });
    }
    let row: Option<(bool, Option<String>)> = conn
        .query_row(
            "SELECT forgetting, let_go_at FROM memories WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((forgetting, let_go)) = row else {
        return Ok(Some(LeftOut::Private));
    };
    if forgetting || let_go.is_some() || crate::memory::is_private_chain(conn, id)? {
        return Ok(Some(LeftOut::Private));
    }
    Ok(None)
}

/// The first instant of `now`'s calendar month in `tz`, and of the next one.
fn month_bounds(now: Timestamp, tz: &TimeZone) -> Result<(Timestamp, Timestamp)> {
    let first = now.to_zoned_datetime(tz.clone()).date().first_of_month();
    let start = first.to_zoned_datetime(tz.clone())?;
    let next = first
        .checked_add(1.month())?
        .to_zoned_datetime(tz.clone())?;
    Ok((start.timestamp(), next.timestamp()))
}

/// The stored template: instructions, lines, and whether the provider was reached.
fn template_json(s: &NewSend) -> String {
    json!({"instructions": s.instructions, "lines": s.template, "reached": s.reached}).to_string()
}

fn result_reason(r: &SendResult) -> Option<String> {
    match r {
        SendResult::Failed { reason } | SendResult::NotSent { reason } => Some(enum_str(reason)),
        _ => None,
    }
}

/// A memory as it reads now, for rebuilding a sent line: `None` when it is gone or going.
struct Current {
    sentence: String,
    was: Option<String>,
    superseded: bool,
}

/// Rebuilds a kept line (a check-in, an entry or a reading) as it reads now.
pub(crate) type KeptNow<'a> = &'a dyn Fn(&Connection, &str) -> Result<Option<String>>;

fn current(conn: &Connection, id: &str, kept: KeptNow<'_>) -> Result<Option<Current>> {
    if crate::lookup::is_kept_line(id) {
        return Ok(kept(conn, id)?.map(|sentence| Current {
            sentence,
            was: None,
            superseded: false,
        }));
    }
    let row: Option<(String, bool, Option<String>)> = conn
        .query_row(
            "SELECT sentence, forgetting, superseded_at FROM memories WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((sentence, false, superseded)) = row else {
        return Ok(None);
    };
    // The value it replaced, as the answer prompt shows it ("before, it was ...").
    let was: Option<String> = conn
        .query_row(
            "SELECT CASE WHEN m.sentence = m.object THEN o.sentence ELSE o.object END \
             FROM memories m JOIN memories o ON o.superseded_by = m.id \
             WHERE m.id = ?1 AND NOT (o.subject_key = m.subject_key \
             AND o.predicate = m.predicate AND o.object_key = m.object_key) \
             AND o.forgetting = 0 ORDER BY o.recorded_at DESC, o.id LIMIT 1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(Some(Current {
        sentence,
        was,
        superseded: superseded.is_some(),
    }))
}

/// One stored line as it may be shown now: a memory's words only while the memory still
/// rebuilds the exact line sent.
fn rebuild(
    conn: &Connection,
    line: SentLine,
    memory_lines: &[(String, String)],
    kept: KeptNow<'_>,
) -> Result<SentLine> {
    let SentLine::Memory { n, text, .. } = line else {
        return Ok(line);
    };
    let shown = |text: Option<String>, state: SentMemory| SentLine::Memory { n, text, state };
    let Some((id, hash)) = memory_lines.get((n as usize).saturating_sub(1)) else {
        return Ok(shown(None, SentMemory::Deleted));
    };
    let Some(now) = current(conn, id, kept)? else {
        return Ok(shown(None, SentMemory::Deleted));
    };
    let mut line = text
        .unwrap_or_default()
        .replacen(NewSend::SLOT, &now.sentence, 1);
    if let Some(was) = &now.was {
        line = line.replacen(NewSend::WAS_SLOT, was, 1);
    }
    if now.superseded || &NewSend::hash_line(&line) != hash {
        return Ok(shown(None, SentMemory::Changed));
    }
    Ok(shown(Some(line), SentMemory::Same))
}

type SendRaw = (
    i64,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    f64,
    f64,
    Option<f64>,
    String,
);

const SEND_COLS: &str = "id, at, kind, host, model, question, template, memory_lines, bytes_up, \
    bytes_down, cost_usd, result";

fn send_raw(r: &rusqlite::Row<'_>) -> rusqlite::Result<SendRaw> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
        r.get(8)?,
        r.get(9)?,
        r.get(10)?,
        r.get(11)?,
    ))
}

fn memory_lines_of(text: &str) -> Vec<(String, String)> {
    serde_json::from_str::<Vec<Value>>(text)
        .unwrap_or_default()
        .iter()
        .map(|v| {
            (
                v["id"].as_str().unwrap_or_default().to_string(),
                v["hash"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

fn send_view(conn: &Connection, raw: SendRaw, kept: KeptNow<'_>) -> Result<OnlineSend> {
    let (id, at, kind, host, model, question, template, memory_lines, up, down, cost, result) = raw;
    let template: Value = serde_json::from_str(&template).unwrap_or(Value::Null);
    let stored: Vec<SentLine> =
        serde_json::from_value(template["lines"].clone()).unwrap_or_default();
    let memory_lines = memory_lines_of(&memory_lines);
    let lines = stored
        .into_iter()
        .map(|l| rebuild(conn, l, &memory_lines, kept))
        .collect::<Result<Vec<_>>>()?;
    Ok(OnlineSend {
        id: id as f64,
        at,
        kind: parse_enum(&kind)?,
        host,
        model,
        question,
        lines,
        instructions: template["instructions"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        bytes_up: up,
        bytes_down: down,
        cost_usd: cost,
        result: serde_json::from_str(&result).unwrap_or(SendResult::NotSent {
            reason: LocalReason::Problem,
        }),
        reached: template["reached"].as_bool().unwrap_or(false),
        journal: Some(journal_sent(conn, &memory_lines)?).filter(|j| !j.is_empty()),
    })
}

/// The sent memory lines read from a journal entry or a good thing, with the day it was kept on.
fn journal_sent(
    conn: &Connection,
    memory_lines: &[(String, String)],
) -> Result<Vec<nocturne_contract::JournalSent>> {
    let mut out = Vec::new();
    for (i, (id, _)) in memory_lines.iter().enumerate() {
        let day: Option<String> = conn
            .query_row(
                "SELECT json_extract(i.metadata, '$.journal_day') FROM memory_sources s \
                 JOIN items i ON i.id = s.item_id WHERE s.memory_id = ?1 \
                 AND json_extract(i.metadata, '$.journal') IN ('entry', 'good', 'answer') \
                 ORDER BY i.kept_at LIMIT 1",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if let Some(day) = day {
            out.push(nocturne_contract::JournalSent {
                n: i as u32 + 1,
                day,
            });
        }
    }
    Ok(out)
}

impl Engine {
    pub fn online_state(&self) -> Result<OnlineRow> {
        self.read(load_state)
    }

    /// Reads the row, applies `f` and writes it back in one write transaction.
    pub fn update_online_state(&self, f: impl FnOnce(&mut OnlineRow)) -> Result<OnlineRow> {
        self.write(|tx| {
            let mut state = load_state(tx)?;
            f(&mut state);
            store_state(tx, &state)?;
            Ok(state)
        })
    }

    /// Whether the person has seen that only what they mark private is held back. Until they
    /// have, nothing goes online.
    pub fn online_change_seen(&self) -> Result<bool> {
        self.read(|conn| {
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM settings WHERE key = ?1)",
                [ONLY_PRIVATE_SEEN],
                |r| r.get(0),
            )?)
        })
    }

    /// Records that the person has seen that only what they mark private is held back.
    pub fn see_online_change(&self) -> Result<()> {
        self.write(|tx| {
            tx.execute(
                "INSERT OR REPLACE INTO settings(key, value) VALUES (?1, 'true')",
                [ONLY_PRIVATE_SEEN],
            )?;
            Ok(())
        })
    }

    /// Whether each memory may be sent online now. Unknown ids are `false`.
    pub fn sendable(&self, ids: &[String]) -> Result<Vec<bool>> {
        Ok(self
            .online_verdicts(ids)?
            .into_iter()
            .map(|v| v.is_none())
            .collect())
    }

    /// For each memory, why it may not be sent online, or `None` when it may: private in any
    /// version, let go, being forgotten or unknown (`Private`).
    pub fn online_verdicts(&self, ids: &[String]) -> Result<Vec<Option<LeftOut>>> {
        self.read(|conn| ids.iter().map(|id| verdict(conn, id)).collect())
    }

    /// For each memory, whether the person's own online mind may be given it (ADR 0020): known,
    /// not being forgotten and not let go. A private one may: private decides only what comes
    /// up on its own.
    pub fn mind_may_read(&self, ids: &[String]) -> Result<Vec<bool>> {
        self.read(|conn| {
            ids.iter()
                .map(|id| {
                    if let Some(item) = crate::lookup::kept_line_item(id) {
                        let known: bool = conn.query_row(
                            "SELECT EXISTS (SELECT 1 FROM items WHERE id = ?1 AND forget_epoch = 0)",
                            [item],
                            |r| r.get(0),
                        )?;
                        return Ok(known);
                    }
                    let row: Option<(bool, Option<String>, String)> = conn
                        .query_row(
                            "SELECT forgetting, let_go_at, status FROM memories WHERE id = ?1",
                            [id],
                            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                        )
                        .optional()?;
                    Ok(row.is_some_and(|(forgetting, let_go, status)| {
                        !forgetting && let_go.is_none() && status != "letGo"
                    }))
                })
                .collect()
        })
    }

    /// Writes one attempt to the sent log and returns its id.
    pub fn log_online_send(&self, s: NewSend) -> Result<i64> {
        let lines: Vec<Value> = s
            .memory_lines
            .iter()
            .map(|(id, hash)| json!({"id": id, "hash": hash}))
            .collect();
        self.write(|tx| {
            tx.execute(
                "INSERT INTO online_sends(at, kind, host, model, question, template, \
                 memory_lines, bytes_up, bytes_down, cost_usd, status, seconds, result, reason, \
                 counted) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    fmt_ts(s.at),
                    enum_str(&s.kind),
                    s.host,
                    s.model,
                    s.question,
                    template_json(&s),
                    Value::Array(lines).to_string(),
                    s.bytes_up as f64,
                    s.bytes_down as f64,
                    s.cost_usd,
                    s.status,
                    s.seconds,
                    serde_json::to_string(&s.result)?,
                    result_reason(&s.result),
                    s.counted,
                ],
            )?;
            Ok(tx.last_insert_rowid())
        })
    }

    /// The sent log, newest first, `limit` at a time, before the send with id `before`. Each
    /// memory line shows its words only while the memory still reads as it was sent.
    pub fn online_sends(&self, limit: u32, before: Option<i64>) -> Result<Vec<OnlineSend>> {
        self.read(|conn| {
            let raws: Vec<SendRaw> = conn
                .prepare(&format!(
                    "SELECT {SEND_COLS} FROM online_sends WHERE ?1 IS NULL OR id < ?1 \
                     ORDER BY id DESC LIMIT ?2"
                ))?
                .query_map(params![before, limit], send_raw)?
                .collect::<rusqlite::Result<_>>()?;
            let originals = self.originals_dir();
            let kept = |conn: &Connection, id: &str| {
                crate::lookup::kept_line_now(conn, self.clock(), &originals, id)
            };
            raws.into_iter()
                .map(|raw| send_view(conn, raw, &kept))
                .collect()
        })
    }

    /// Questions this calendar month (in `tz`) that reached the engine route.
    pub fn questions_this_month(&self, tz: &TimeZone) -> Result<u32> {
        let (start, next) = month_bounds(self.clock_now(), tz)?;
        self.read(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM online_sends WHERE kind = 'question' AND counted = 1 \
                 AND at >= ?1 AND at < ?2",
                params![fmt_ts(start), fmt_ts(next)],
                |r| r.get(0),
            )?)
        })
    }

    /// What the provider said this month's answers cost, or `None` when it said nothing.
    pub fn spent_this_month(&self, tz: &TimeZone) -> Result<Option<f64>> {
        let (start, next) = month_bounds(self.clock_now(), tz)?;
        self.read(|conn| {
            Ok(conn.query_row(
                "SELECT SUM(cost_usd) FROM online_sends WHERE cost_usd IS NOT NULL \
                 AND at >= ?1 AND at < ?2",
                params![fmt_ts(start), fmt_ts(next)],
                |r| r.get(0),
            )?)
        })
    }

    /// Whether the provider reported cost on each of the last three answered questions.
    pub fn online_cost_reported(&self) -> Result<bool> {
        self.read(|conn| {
            let costs: Vec<Option<f64>> = conn
                .prepare(
                    "SELECT cost_usd FROM online_sends WHERE kind = 'question' \
                     AND (result LIKE '%\"answered\"%' OR result LIKE '%\"notSure\"%') \
                     ORDER BY id DESC LIMIT 3",
                )?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(costs.len() == 3 && costs.iter().all(Option::is_some))
        })
    }

    /// The newest attempt in the sent log, and how many are kept.
    pub fn last_online_send(&self) -> Result<(Option<LastSend>, u32)> {
        self.read(|conn| {
            let kept: u32 =
                conn.query_row("SELECT COUNT(*) FROM online_sends", [], |r| r.get(0))?;
            let last: Option<LastRaw> = conn
                .query_row(
                    "SELECT at, status, seconds, bytes_up, bytes_down FROM online_sends \
                     ORDER BY id DESC LIMIT 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;
            let last = match last {
                Some((at, status, seconds, up, down)) => Some(LastSend {
                    at: parse_ts(&at)?,
                    status,
                    seconds,
                    bytes_up: up,
                    bytes_down: down,
                }),
                None => None,
            };
            Ok((last, kept))
        })
    }

    /// Removes the sent log's rows from before `before`; returns how many went.
    pub fn prune_online_sends(&self, before: Timestamp) -> Result<u32> {
        self.write(|tx| {
            Ok(tx.execute("DELETE FROM online_sends WHERE at < ?1", [fmt_ts(before)])? as u32)
        })
    }

    /// Adds bytes handed to the engine route to the running total.
    pub fn add_sent_online(&self, bytes: u64) -> Result<()> {
        self.write(|tx| {
            tx.execute(
                "UPDATE online_state SET sent_bytes = sent_bytes + ?1 WHERE id = 1",
                [bytes as f64],
            )?;
            Ok(())
        })
    }

    /// The first day of next month in `tz` (`YYYY-MM-DD`), when a monthly count starts again.
    pub fn online_resets_on(&self, tz: &TimeZone) -> String {
        let first = self
            .clock_now()
            .to_zoned_datetime(tz.clone())
            .date()
            .first_of_month();
        first
            .checked_add(1.month())
            .map(|d| d.to_string())
            .unwrap_or_default()
    }
}

/// Takes a forgotten memory out of the sent log: its lines keep only "a memory, since
/// deleted", with no words, details, id or hash left behind.
pub(crate) fn scrub_memory(conn: &Connection, id: &str) -> Result<()> {
    let pattern = format!("%{}%", serde_json::to_string(id)?);
    let rows: Vec<(i64, String, String)> = conn
        .prepare("SELECT id, template, memory_lines FROM online_sends WHERE memory_lines LIKE ?1")?
        .query_map([pattern], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (row, template, lines) in rows {
        let mut lines: Vec<Value> = serde_json::from_str(&lines).unwrap_or_default();
        let gone: Vec<u32> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l["id"] == id)
            .map(|(i, _)| i as u32 + 1)
            .collect();
        if gone.is_empty() {
            continue;
        }
        for n in &gone {
            lines[*n as usize - 1] = json!({"id": "", "hash": ""});
        }
        let mut template: Value = serde_json::from_str(&template).unwrap_or(Value::Null);
        let stored: Vec<SentLine> =
            serde_json::from_value(template["lines"].clone()).unwrap_or_default();
        let scrubbed: Vec<SentLine> = stored
            .into_iter()
            .map(|l| match l {
                SentLine::Memory { n, .. } if gone.contains(&n) => SentLine::Memory {
                    n,
                    text: None,
                    state: SentMemory::Deleted,
                },
                other => other,
            })
            .collect();
        template["lines"] = serde_json::to_value(scrubbed)?;
        conn.execute(
            "UPDATE online_sends SET template = ?2, memory_lines = ?3 WHERE id = ?1",
            params![row, template.to_string(), Value::Array(lines).to_string()],
        )?;
    }
    Ok(())
}

/// The export's part for Online answers: `online/sent.md`, what was sent and when, readable,
/// with lines rebuilt as the sent log shows them. Never the key, which the store never holds.
pub(crate) fn export_section(conn: &Connection, dir: &Path, kept: KeptNow<'_>) -> Result<()> {
    let raws: Vec<SendRaw> = conn
        .prepare(&format!("SELECT {SEND_COLS} FROM online_sends ORDER BY id"))?
        .query_map([], send_raw)?
        .collect::<rusqlite::Result<_>>()?;
    if raws.is_empty() {
        return Ok(());
    }
    let mut out = String::from("# What Online answers sent\n");
    for raw in raws {
        let send = send_view(conn, raw, kept)?;
        let what = match send.kind {
            SendKind::Question => "Question",
            SendKind::Check => "Connection check",
        };
        out.push_str(&format!(
            "\n## {}: {what} to {}\n\nModel: {}\nQuestion: {}\nSent: {} bytes. Received: {} bytes.\n",
            send.at, send.host, send.model, send.question, send.bytes_up, send.bytes_down
        ));
        out.push_str(if send.reached {
            "Reached the provider: yes\n"
        } else {
            "Reached the provider: no\n"
        });
        if let Some(c) = send.cost_usd {
            out.push_str(&format!("Cost: {c} USD\n"));
        }
        out.push_str(&format!(
            "Result: {}\n\nInstructions:\n\n",
            serde_json::to_string(&send.result)?
        ));
        for line in send.instructions.lines() {
            out.push_str(&format!("    {line}\n"));
        }
        out.push_str("\nWhat was sent:\n\n");
        for line in send.lines {
            let text = match line {
                SentLine::Text { text } => text,
                SentLine::Memory {
                    text: Some(t),
                    state: SentMemory::Same,
                    ..
                } => t,
                SentLine::Memory {
                    n,
                    state: SentMemory::Changed,
                    ..
                } => format!("[m{n}] A memory you have since changed"),
                SentLine::Memory { n, .. } => format!("[m{n}] A memory you have since deleted"),
            };
            for part in text.lines() {
                out.push_str(&format!("    {part}\n"));
            }
        }
    }
    let folder = dir.join("online");
    crate::private::create_dir(&folder)?;
    std::fs::write(folder.join("sent.md"), out)?;
    Ok(())
}
