//! Settings, stored as JSON values by key. Defaults come from here; the person's choices
//! overlay them.

use nocturne_contract::{Delivery, LettingGo, SettingsPatch, SettingsView, Theme, TidyReport};
use rusqlite::{Connection, params};
use serde_json::{Map, Value};

use crate::Engine;
use crate::error::{Error, Result};

pub(crate) fn defaults() -> SettingsView {
    SettingsView {
        page_time: "07:00".to_string(),
        delivery: Delivery::Notify,
        auto_print: false,
        voice: "bf_emma".to_string(),
        voice_speed: 1.0,
        shortcut: "CommandOrControl+Shift+Space".to_string(),
        shortcut_available: true,
        inbox_folder: None,
        start_at_login: false,
        letting_go: LettingGo::Normal,
        theme: Theme::System,
        language: "en-GB".to_string(),
        check_updates: false,
        reminders: true,
        lock: false,
        agents: false,
    }
}

pub(crate) fn read_settings(conn: &Connection) -> Result<SettingsView> {
    let Value::Object(mut map) = serde_json::to_value(defaults())? else {
        unreachable!("settings serialise to an object")
    };
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (k, v) = row?;
        if let Ok(v) = serde_json::from_str::<Value>(&v) {
            map.insert(k, v);
        }
    }
    Ok(serde_json::from_value(Value::Object(map)).unwrap_or_else(|_| defaults()))
}

/// Parse `HH:MM`.
pub(crate) fn parse_page_time(s: &str) -> Result<(i8, i8)> {
    let (h, m) = s
        .split_once(':')
        .ok_or_else(|| Error::invalid("page time is HH:MM"))?;
    let h: i8 = h
        .parse()
        .map_err(|_| Error::invalid("page time is HH:MM"))?;
    let m: i8 = m
        .parse()
        .map_err(|_| Error::invalid("page time is HH:MM"))?;
    if !(0..24).contains(&h) || !(0..60).contains(&m) || s.len() != 5 {
        return Err(Error::invalid("page time is HH:MM"));
    }
    Ok((h, m))
}

pub(crate) fn set_page_time(conn: &Connection, h: i8, m: i8) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO settings(key, value) VALUES ('pageTime', ?1)",
        params![serde_json::to_string(&format!("{h:02}:{m:02}"))?],
    )?;
    Ok(())
}

/// Days without use before a memory fades and before it is let go.
pub(crate) fn thresholds(l: LettingGo) -> (i64, i64) {
    match l {
        LettingGo::Gentle => (45, 120),
        LettingGo::Normal => (30, 90),
        LettingGo::Brisk => (14, 45),
    }
}

const LAST_TIDY: &str = "_lastTidy";

impl Engine {
    /// Remember the last overnight tidy, for the advanced details.
    pub fn record_tidy(&self, report: TidyReport) -> Result<()> {
        self.write(|tx| {
            tx.execute(
                "INSERT OR REPLACE INTO settings(key, value) VALUES (?1, ?2)",
                params![LAST_TIDY, serde_json::to_string(&report)?],
            )?;
            Ok(())
        })
    }

    /// The last tidy `record_tidy` stored.
    pub fn last_tidy(&self) -> Result<Option<TidyReport>> {
        self.read(|conn| {
            let v: Option<String> = rusqlite::OptionalExtension::optional(conn.query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [LAST_TIDY],
                |r| r.get(0),
            ))?;
            Ok(v.and_then(|v| serde_json::from_str(&v).ok()))
        })
    }

    pub fn settings(&self) -> Result<SettingsView> {
        self.read(read_settings)
    }

    /// Apply the fields the patch sets; the rest keep their value. Returns the new settings.
    pub fn update_settings(&self, patch: SettingsPatch) -> Result<SettingsView> {
        if let Some(t) = &patch.page_time {
            parse_page_time(t)?;
        }
        if let Some(s) = patch.voice_speed
            && !(0.25..=4.0).contains(&s)
        {
            return Err(Error::invalid("voice speed is out of range"));
        }
        let Value::Object(fields) = serde_json::to_value(&patch)? else {
            unreachable!("a patch serialises to an object")
        };
        self.write(|tx| {
            for (key, value) in Map::from_iter(fields) {
                if value.is_null() {
                    continue;
                }
                tx.execute(
                    "INSERT OR REPLACE INTO settings(key, value) VALUES (?1, ?2)",
                    params![key, value.to_string()],
                )?;
            }
            read_settings(tx)
        })
    }
}
