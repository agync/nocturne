//! The journal (`docs/design/journaling.md`): entries, check-ins, good things, the people and
//! places found in them, and the reflections and recaps that look back.
//!
//! Journal items are ordinary items on the tape. What makes one a journal item lives in its
//! `metadata`: `journal` (`entry`, `good` or `checkin`), `journal_day`, `block`, `replaces`,
//! `about`, `feeling`, `line`, `urge`, `tried`, `photo`, `alt`, `saved_at` and `read_at`.

pub(crate) mod day;
pub(crate) mod keep;
pub(crate) mod lookback;
pub(crate) mod mentions;
pub(crate) mod store;

use std::collections::HashSet;
use std::path::Path;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use nocturne_contract::{ItemStatus, ItemView, JournalRole, Receipt};
use rusqlite::{Connection, params_from_iter};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::rows::{ITEM_COLS, item_view};
use crate::settings::{parse_page_time, read_settings};
use crate::util::{fmt_ts, parse_date, parse_ts};

/// How long a saved block waits for more writing before it is read.
pub(crate) const HOLD_MINUTES: i64 = 10;
/// Good things per day.
pub(crate) const MAX_GOOD_THINGS: u32 = 3;
/// The setting that says the person has dismissed "Your journal".
pub(crate) const INTRO_KEY: &str = "_journalIntroDismissed";

/// The time as the journal reads it: the person's time zone and page time, and now.
#[derive(Clone)]
pub(crate) struct Ctx {
    pub tz: TimeZone,
    pub page_hm: (i8, i8),
    pub now: Timestamp,
}

impl Ctx {
    pub fn load(conn: &Connection, clock: &dyn crate::Clock) -> Result<Ctx> {
        let settings = read_settings(conn)?;
        Ok(Ctx {
            tz: clock.tz(),
            page_hm: parse_page_time(&settings.page_time)?,
            now: clock.now(),
        })
    }

    pub fn day_of(&self, t: Timestamp) -> Result<Date> {
        crate::day::journal_day_of(t, self.page_hm, &self.tz)
    }

    pub fn today(&self) -> Result<Date> {
        self.day_of(self.now)
    }

    pub fn bounds(&self, day: Date) -> Result<(Timestamp, Timestamp)> {
        crate::day::journal_day_bounds(day, self.page_hm, &self.tz)
    }

    /// Whether the day has ended: nothing more can be kept on it, by the clock.
    pub fn ended(&self, day: Date) -> Result<bool> {
        Ok(self.bounds(day)?.1 <= self.now)
    }
}

/// An item with the columns the journal needs beyond its view.
#[derive(Debug, Clone)]
pub(crate) struct JItem {
    pub view: ItemView,
    pub meta: Value,
    pub left_out: bool,
    pub trusted: bool,
}

impl JItem {
    pub fn id(&self) -> &str {
        &self.view.id
    }

    pub fn role(&self) -> Option<JournalRole> {
        self.view.journal
    }

    pub fn meta_str(&self, key: &str) -> Option<&str> {
        self.meta.get(key).and_then(Value::as_str)
    }

    pub fn day_text(&self) -> Option<&str> {
        self.meta_str("journal_day")
    }

    pub fn block(&self) -> u32 {
        self.meta.get("block").and_then(Value::as_u64).unwrap_or(0) as u32
    }

    pub fn replaces(&self) -> Option<&str> {
        self.meta_str("replaces")
    }

    pub fn about(&self) -> Option<&str> {
        self.meta_str("about")
    }

    /// Not yet read: waiting or being read.
    pub fn pending(&self) -> bool {
        matches!(self.view.status, ItemStatus::Kept | ItemStatus::Reading)
    }

    /// The instant it was last saved: the latest save of an entry block, else when it was kept.
    pub fn saved_at(&self) -> &str {
        self.meta_str("saved_at").unwrap_or(&self.view.kept_at)
    }

    /// The journal day it belongs to.
    pub fn day(&self, ctx: &Ctx) -> Result<Date> {
        match self.day_text() {
            Some(d) => parse_date(d),
            None => ctx.day_of(parse_ts(&self.view.kept_at)?),
        }
    }

    pub fn receipt(&self) -> Receipt {
        Receipt {
            item_id: self.view.id.clone(),
            kind: self.view.kind,
            source: self.view.source,
            label: self.view.title.clone(),
            kept_at: self.view.kept_at.clone(),
            duration_ms: self.view.duration_ms,
            span: None,
            unconfirmed: !self.trusted,
            from_device: self.view.from_device.clone(),
        }
    }
}

const JITEM_COLS: &str = "i.metadata, i.left_out, (i.trust = 'trusted')";

fn jitem_sql(where_sql: &str) -> String {
    format!(
        "SELECT {ITEM_COLS}, {JITEM_COLS} FROM items i WHERE {where_sql} ORDER BY i.kept_at, i.rowid"
    )
}

/// Items matching a WHERE clause over `items i`, oldest first.
pub(crate) fn load_items(
    conn: &Connection,
    originals: &Path,
    where_sql: &str,
    args: &[String],
) -> Result<Vec<JItem>> {
    let mut stmt = conn.prepare(&jitem_sql(where_sql))?;
    let to_view = item_view(originals);
    let rows = stmt.query_map(params_from_iter(args.iter()), |r| {
        let view = to_view(r)?;
        let meta: String = r.get(20 + 1)?;
        let left_out: bool = r.get(20 + 2)?;
        let trusted: bool = r.get(20 + 3)?;
        Ok(JItem {
            view,
            meta: serde_json::from_str(&meta).unwrap_or(Value::Null),
            left_out,
            trusted,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub(crate) fn load_item(conn: &Connection, originals: &Path, id: &str) -> Result<JItem> {
    load_items(conn, originals, "i.id = ?1", &[id.to_string()])?
        .pop()
        .ok_or_else(|| Error::not_found(format!("item {id}")))
}

/// Everything that belongs to the journal days `start` to `end`, both included: items with a
/// journal day in range, and items with none that were kept inside the range.
pub(crate) fn items_in_days(
    conn: &Connection,
    ctx: &Ctx,
    originals: &Path,
    start: Date,
    end: Date,
) -> Result<Vec<JItem>> {
    let from = fmt_ts(ctx.bounds(start)?.0);
    let to = fmt_ts(ctx.bounds(end)?.1);
    load_items(
        conn,
        originals,
        "(json_extract(i.metadata, '$.journal_day') BETWEEN ?1 AND ?2) \
         OR (json_extract(i.metadata, '$.journal_day') IS NULL AND i.kept_at >= ?3 AND i.kept_at < ?4)",
        &[start.to_string(), end.to_string(), from, to],
    )
}

/// The items in `all` that are not the earlier version of another.
pub(crate) fn current(all: &[JItem]) -> Vec<&JItem> {
    let replaced: HashSet<&str> = all.iter().filter_map(JItem::replaces).collect();
    all.iter().filter(|i| !replaced.contains(i.id())).collect()
}

/// The first line of an entry block or note as the glance and highlights quote it.
pub(crate) fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}
